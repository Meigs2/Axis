#![no_std]
#![no_main]
#![feature(async_fn_in_trait)]
#![feature(type_alias_impl_trait)]
#![feature(async_closure)]
#![feature(never_type)]

mod axis_peripherals;

use crate::axis_peripherals::ads1115;
use crate::axis_peripherals::ads1115::Ads1115;
use crate::axis_peripherals::max31588::MAX31855;
use crate::Message::{Ping, Pong};
use core::future::Future;
use defmt::Format;
use defmt::{info, unwrap};
use embassy_executor::{Executor, InterruptExecutor};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::i2c::Config;
use embassy_rp::interrupt::{InterruptExt, Priority};
use embassy_rp::peripherals::I2C1;
use embassy_rp::peripherals::USB;
use embassy_rp::spi::Spi;
use embassy_rp::usb::Driver;
use embassy_rp::watchdog::Watchdog;
use embassy_rp::{bind_interrupts, i2c, interrupt};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_time::Duration;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::Builder;
use embedded_hal_async::i2c::I2c;
use serde::{Deserialize, Serialize};
use serde_json_core::heapless::String;
use static_cell::{make_static, StaticCell};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
});

bind_interrupts!(struct I2cIrqs {
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<I2C1>;
});

#[derive(Clone)]
pub struct Runtime<'a> {
    outbound_sender: Sender<'a, CriticalSectionRawMutex, Message, 1>,
}

impl<'a> Runtime<'a> {
    pub fn new(outbound_sender: Sender<'a, CriticalSectionRawMutex, Message, 1>) -> Self {
        Self { outbound_sender }
    }

    pub async fn handle(&self, message: Message) {
        match message {
            Ping => {
                self.outbound_sender
                    .send(Pong {
                        value: "Pong!".into(),
                    })
                    .await;
            }
            Pong { .. } => {
                self.outbound_sender.send(Ping).await;
            }
            thermocouple_reading => {
                self.outbound_sender.send(thermocouple_reading).await;
            }
        }
    }
}

pub const MAX_STRING_SIZE: usize = 64;
pub const MAX_PACKET_SIZE: u8 = 64;

#[derive(Format, Debug, Serialize, Deserialize)]
pub enum Message {
    Ping,
    Pong { value: String<MAX_STRING_SIZE> },
    ThermocoupleReading { temperature: f32 },
}

#[derive(Copy, Clone, Serialize, Deserialize)]
pub enum MessageType {}

#[derive(Debug)]
pub enum MessageError {
    InvalidMessageType,
    Unknown,
}

static EXECUTOR_HIGH: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_MED: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_LOW: StaticCell<Executor> = StaticCell::new();

static INTERNAL_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, Message, 1>> =
    StaticCell::new();

static EXTERNAL_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, Message, 1>> =
    StaticCell::new();

#[interrupt]
unsafe fn SWI_IRQ_1() {
    EXECUTOR_HIGH.on_interrupt()
}

#[interrupt]
unsafe fn SWI_IRQ_0() {
    EXECUTOR_MED.on_interrupt()
}

pub async fn wait_with_timeout<F: Future>(
    millis: u64,
    fut: F,
) -> Result<F::Output, embassy_time::TimeoutError> {
    embassy_time::with_timeout(Duration::from_millis(millis), fut).await
}

mod tasks {

    use defmt::{debug, error};
    use embassy_executor::Spawner;

    use embassy_futures::select::select;
    use embassy_rp::gpio::Output;
    use embassy_rp::peripherals::{PIN_7, USB};
    use embassy_rp::usb::Driver;
    use embassy_rp::watchdog::Watchdog;
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::channel::{Receiver, Sender};
    use embassy_time::{Duration, Timer};
    use heapless::pool::Pool;
    use static_cell::make_static;

    use crate::axis_peripherals::max31588::{Unit, MAX31855};
    use crate::Message::*;
    use crate::{Message, Runtime, MAX_STRING_SIZE};
    use embassy_usb::UsbDevice;
    use heapless::Vec;
    use serde_json_core::de::Error;
    use serde_json_core::{from_str, to_slice};

    #[embassy_executor::task]
    pub async fn run_usb(usb: &'static mut UsbDevice<'static, Driver<'static, USB>>) -> ! {
        usb.run().await
    }

    #[embassy_executor::task]
    pub async fn read_usb(
        inbound_sender: Sender<'static, CriticalSectionRawMutex, Message, 1>,
        usb_reader: &'static mut embassy_usb::class::cdc_acm::Receiver<
            'static,
            Driver<'static, USB>,
        >,
    ) {
        let buff = make_static!([0u8; MAX_STRING_SIZE]);
        usb_reader.wait_connection().await;
        loop {
            match usb_reader.read_packet(&mut buff[..]).await {
                Ok(s) => {
                    let stopwatch = embassy_time::Instant::now();
                    let string = core::str::from_utf8(&buff[..s]).unwrap();
                    let result: Result<(Vec<Message, MAX_STRING_SIZE>, _), Error> =
                        from_str(string);
                    let msgs = match result {
                        Ok(m) => m.0,
                        Err(_e) => {
                            error!("Error deserializing packet(s).");
                            Vec::new()
                        }
                    };

                    for msg in msgs {
                        inbound_sender.send(msg).await;
                    }
                    let a = stopwatch.elapsed().as_micros();
                    debug!("Read/Write Operation Elapsed: {:?} microseconds", a);
                }
                Err(e) => {
                    error!("Error reading packet: {:?}", e)
                }
            }
        }
    }

    #[embassy_executor::task]
    pub async fn handle_message(message: Message, runtime: &'static Runtime<'static>) {
        runtime.handle(message).await;
    }

    #[embassy_executor::task]
    pub async fn process_internal_messages(
        inbound_receiver: Receiver<'static, CriticalSectionRawMutex, Message, 1>,
        _spawner: Spawner,
        runtime: &'static Runtime<'static>,
    ) {
        loop {
            let message = inbound_receiver.recv().await;
            runtime.handle(message).await;
        }
    }

    #[embassy_executor::task]
    pub async fn write_usb(
        outbound_receiver: Receiver<'static, CriticalSectionRawMutex, Message, 1>,
        usb_sender: &'static mut embassy_usb::class::cdc_acm::Sender<'static, Driver<'static, USB>>,
    ) {
        let buff = make_static!([0u8; MAX_STRING_SIZE]);
        loop {
            let m = outbound_receiver.recv().await;

            let s = if let Ok(s) = to_slice(&m, buff) {
                s
            } else {
                continue;
            };
            debug!("Outbound message: {:?}", buff[..s]);
            let timeout = Timer::after(Duration::from_millis(1));
            select(usb_sender.write_packet(&buff[..s]), timeout).await;
        }
    }

    #[embassy_executor::task]
    pub async fn blink(pin: &'static mut Output<'static, PIN_7>, watchdog: &'static mut Watchdog) {
        loop {
            pin.set_high();
            Timer::after(Duration::from_millis(500)).await;
            pin.set_low();
            Timer::after(Duration::from_millis(500)).await;
            watchdog.feed();
        }
    }

    #[embassy_executor::task]
    pub async fn read_thermocouple(
        inbound_sender: Sender<'static, CriticalSectionRawMutex, Message, 1>,
        thermocouple: &'static mut MAX31855,
    ) {
        loop {
            Timer::after(Duration::from_millis(500)).await;
            let reading = thermocouple.read_thermocouple(Unit::Fahrenheit).await;
            inbound_sender
                .send(ThermocoupleReading {
                    temperature: reading.unwrap(),
                })
                .await;
        }
    }
}

#[embassy_executor::main]
async fn main(_s: embassy_executor::Spawner) {
    let p = embassy_rp::init(Default::default());

    // Create the driver, from the HAL.
    let driver = Driver::new(p.USB, Irqs);

    let mut config = embassy_usb::Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("Embassy");
    config.product = Some("USB-serial logger");
    config.serial_number = Some("12345678");
    config.max_power = 100;
    config.max_packet_size_0 = MAX_PACKET_SIZE;

    // Required for windows compatiblity.
    // https://developer.nordicsemi.com/nRF_Connect_SDK/doc/1.9.1/kconfig/CONFIG_CDC_ACM_IAD.html#help
    config.device_class = 0xEF;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;
    config.composite_with_iads = true;

    let device_descriptor = make_static!([0; 256]);
    let config_descriptor = make_static!([0; 256]);
    let bos_descriptor = make_static!([0; 256]);
    let control_buf = make_static!([0; 64]);
    let state = make_static!(State::new());

    let mut builder = Builder::new(
        driver,
        config,
        device_descriptor,
        config_descriptor,
        bos_descriptor,
        control_buf,
    );

    // Create classes on the builder.
    let class = CdcAcmClass::new(&mut builder, state, MAX_PACKET_SIZE as u16);

    let usb = make_static!(builder.build());

    let internal_channel = INTERNAL_CHANNEL.init(Channel::new());
    let external_channel = EXTERNAL_CHANNEL.init(Channel::new());

    let inbound_sender = internal_channel.sender();
    let outbound_receiver = external_channel.receiver();
    let outbound_sender = external_channel.sender();
    let inbound_receiver = internal_channel.receiver();

    let (sender, reader) = class.split();

    let usb_sender = make_static!(sender);
    let usb_reader = make_static!(reader);

    let pin = make_static!(Output::new(p.PIN_7, Level::Low));

    let watchdog = make_static!(Watchdog::new(p.WATCHDOG));
    watchdog.start(Duration::from_secs(5));

    let runtime = make_static!(Runtime::new(outbound_sender));

    let th_clk = p.PIN_10;
    let th_miso = p.PIN_12;
    let rx_dma = p.DMA_CH3;

    let mut config = embassy_rp::spi::Config::default();
    config.frequency = 500_000;
    let thermocouple_spi = make_static!(Spi::new_rxonly(p.SPI1, th_clk, th_miso, rx_dma, config));

    let thermocouple_pinout = Output::new(p.PIN_11, Level::High);
    let thermocouple = make_static!(MAX31855::new(thermocouple_spi, thermocouple_pinout));

    let sda = p.PIN_14;
    let scl = p.PIN_15;

    info!("set up i2c ");
    let mut i2c: i2c::I2c<I2C1, i2c::Async> =
        i2c::I2c::new_async(p.I2C1, scl, sda, I2cIrqs, Config::default());
        i2c.write_read(address, write, read)

    let ads = Ads1115::new(i2c);

    interrupt::SWI_IRQ_1.set_priority(Priority::P0);
    let spawner = EXECUTOR_HIGH.start(interrupt::SWI_IRQ_1);
    let _ = spawner.spawn(tasks::read_thermocouple(
        internal_channel.sender(),
        thermocouple,
    ));

    // Low priority executor: runs in thread mode, using WFE/SEV
    let executor = EXECUTOR_LOW.init(Executor::new());
    executor.run(|spawner| {
        unwrap!(spawner.spawn(tasks::run_usb(usb)));
        unwrap!(spawner.spawn(tasks::read_usb(inbound_sender, usb_reader)));
        unwrap!(spawner.spawn(tasks::write_usb(outbound_receiver, usb_sender)));
        unwrap!(spawner.spawn(tasks::process_internal_messages(
            inbound_receiver,
            spawner,
            runtime
        )));
        unwrap!(spawner.spawn(tasks::blink(pin, watchdog)));
    });

    // let c = &*EXTERNAL_CHANNEL.init(Channel::new());
    //
    // let watchdog = WATCHDOG.init(Watchdog::new(p.WATCHDOG));
    //
    // let mut spi0_config = embassy_rp::spi::Config::default();
    // spi0_config.frequency = 1_000_000;
    //
    // let client_mosi = p.PIN_3;
    // let client_miso = p.PIN_0;
    // let clk = p.PIN_2;
    // let cs = p.PIN_1;
    // let mosi_dma = p.DMA_CH1;
    // let miso_dma = p.DMA_CH2;
    //
    // let mut client_spi: &mut Spi<'static, SPI0, Async> = CLIENT_SPI.init(Spi::new(
    //     p.SPI0,
    //     clk,
    //     client_mosi,
    //     client_miso,
    //     mosi_dma,
    //     miso_dma,
    //     spi0_config,
    // ));
    //
    // let th_clk = p.PIN_10;
    // let th_miso = p.PIN_12;
    // let rx_dma = p.DMA_CH3;
    // let mut config = embassy_rp::spi::Config::default();
    // config.frequency = 500_000;
    // let mut thermocouple_spi: &mut Spi<'static, SPI1, Async> =
    //     THERMOCOUPLE_SPI.init(Spi::new_rxonly(p.SPI1, th_clk, th_miso, rx_dma, config));
    //
    // let thermocouple_pinout = Output::new(p.PIN_11, Level::Low);
    // // let thermocouple = MAX31855::new(thermocouple_spi, thermocouple_pinout);
    //
    // let content = [0u8; 1024];
    //
    // let mut type_buff = [0u8; 1];
    // let mut len_buff = [0u8; 2];
    // let mut content_buffer = [0u8; 1024];
    //
    // let mut gpio = Output::new(p.PIN_7, Level::Low);
    // let mut inbound_flag = Input::new(p.PIN_4, Pull::None);
    //
    // loop {
    //     let blink_task = async {
    //         gpio.set_high();
    //         Timer::after(Duration::from_millis(500)).await;
    //         gpio.set_low();
    //         Timer::after(Duration::from_millis(500)).await;
    //     };
    // }
    //
    // loop {
    //     gpio.set_high();
    //     Timer::after(Duration::from_millis(100)).await;
    //     gpio.set_low();
    //     Timer::after(Duration::from_millis(100)).await;
    // }
    //
    // let c2 = c;
    // // let runtime = RUNTIME.init(AxisRuntime::new(
    // //     c2,
    // //     external_to_internal_channel,
    // //     watchdog,
    // //     client_spi,
    // // ));
    //
    // // High-priority executor: SWI_IRQ_1, priority level 2
    // interrupt::SWI_IRQ_1.set_priority(Priority::P1);
    // //unwrap!(spawner.spawn(main_loop(runtime)));
    //
    // let c2 = c;
    // //unwrap!(spawner.spawn(thermocouple_read(thermocouple, c2)));
    //
    // KILL_SIGNAL.wait().await;
}
