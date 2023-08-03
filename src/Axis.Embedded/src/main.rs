#![no_std]
#![no_main]
#![feature(async_fn_in_trait)]
#![feature(type_alias_impl_trait)]
#![feature(async_closure)]
#![feature(never_type)]

mod client_communicator;
mod controllers;
mod peripherals;
mod pid;
mod runtime;
mod sensors;

use crate::client_communicator::{ClientCommunicator, MAX_PACKET_SIZE};
use crate::runtime::Runtime;
use crate::sensors::ads1115::{Ads1115, AdsConfig};
use crate::sensors::max31855::MAX31855;
use bit_field::BitField;
use core::future::Future;
use defmt::unwrap;
use defmt::Format;
use embassy_executor::{Executor, InterruptExecutor};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::interrupt::{InterruptExt, Priority};
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::peripherals::I2C1;
use embassy_rp::peripherals::USB;
use embassy_rp::spi::Spi;
use embassy_rp::usb::Driver;
use embassy_rp::watchdog::Watchdog;
use embassy_rp::{bind_interrupts, interrupt};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_sync::signal::Signal;
use embassy_time::Duration;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::Builder;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json_core::heapless::String;
use static_cell::{make_static, StaticCell};
use Message::*;
use {defmt_rtt as _, panic_probe as _};

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR0: StaticCell<Executor> = StaticCell::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

bind_interrupts!(struct I2cIrqs {
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<I2C1>;
});

pub const MAX_STRING_SIZE: usize = 62;
pub const THERMOCOUPLE_SPI_FREQUENCY: u32 = 500_000;
pub const MAX_OUTBOUND_MESSAGES: usize = 1;

#[derive(Format, Debug, Serialize, Deserialize)]
pub enum Message {
    Ping,
    Pong { value: String<MAX_STRING_SIZE> },
    ThermocoupleReading { temperature: f32 },
    AdsReading { value: f32 },
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

pub fn bits_to_i16(bits: u16, len: usize, divisor: i16, shift: usize) -> i16 {
    let negative = bits.get_bit(len - 1);
    if negative {
        (bits << shift) as i16 / divisor
    } else {
        bits as i16
    }
}

pub trait PeripheralError {}

pub trait AxisPeripheral {
    fn initialize() -> Result<(), String<MAX_STRING_SIZE>>;
}

mod tasks {
    use byte_slice_cast::AsByteSlice;
    use defmt::{debug, error};

    use embassy_executor::Spawner;
    use embassy_futures::select::select;
    use embassy_rp::gpio::Output;
    use embassy_rp::peripherals::{I2C1, PIN_11, PIN_7, SPI1, USB};
    use embassy_rp::usb::Driver;
    use embassy_rp::watchdog::Watchdog;
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::channel::{Receiver, Sender};
    use embassy_sync::signal::Signal;
    use embassy_time::{Duration, Timer};

    use static_cell::make_static;

    use embassy_usb::UsbDevice;
    use embedded_io::asynch::Write;
    use heapless::{String, Vec};
    use serde_json_core::de::Error;
    use serde_json_core::{from_str, to_slice, to_string};

    use crate::client_communicator::ClientCommunicator;
    use crate::runtime::Runtime;
    use crate::sensors::ads1115::Ads1115;
    use crate::sensors::max31855::{Unit, MAX31855};
    use crate::Message::*;
    use crate::{Message, MAX_STRING_SIZE};

    #[embassy_executor::task]
    pub async fn run_usb(
        usb: &'static mut ClientCommunicator<'static, 5>,
        stop_signal: &'static mut Signal<CriticalSectionRawMutex, ()>,
    ) {
        usb.run(stop_signal).await
    }

    #[embassy_executor::task]
    pub async fn handle_message(message: Message, runtime: &'static Runtime<'static>) {
        runtime.receive(message).await;
    }

    #[embassy_executor::task]
    pub async fn process_internal_messages(
        inbound_receiver: Receiver<'static, CriticalSectionRawMutex, Message, 1>,
        _spawner: Spawner,
        runtime: &'static Runtime<'static>,
    ) {
        loop {
            let message = inbound_receiver.recv().await;
            let _ = _spawner.spawn(handle_message(message, runtime));
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
        thermocouple: &'static mut MAX31855<'static, SPI1, PIN_11>,
    ) {
        loop {
            Timer::after(Duration::from_millis(500)).await;
            let reading = thermocouple.read_thermocouple(Unit::Fahrenheit).await;
            debug!("{:?}", reading);
            inbound_sender
                .send(ThermocoupleReading {
                    temperature: reading.unwrap(),
                })
                .await;
        }
    }

    #[embassy_executor::task]
    pub async fn read_ads(mut ads: Ads1115<'static, I2C1>, runtime: Runtime<'static>) -> ! {
        loop {
            let initialize = ads.initialize();
            if let Err(e) = initialize {
                error!("Ads1115 initialization error: {:?}", e);
                Timer::after(Duration::from_secs(1)).await;
                continue;
            }
            loop {
                Timer::after(Duration::from_millis(100)).await;
                let res: Result<f32, embassy_rp::i2c::Error> = ads.read();
                match res {
                    Ok(v) => {
                        runtime.receive(AdsReading { value: v }).await;
                    }
                    Err(e) => {
                        error!("ADS115 read error: {:?}", e);
                        break;
                    }
                }
            }
            Timer::after(Duration::from_millis(100)).await;
        }
    }
}

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = embassy_rp::init(Default::default());

    let internal_channel: &'static mut Channel<CriticalSectionRawMutex, Message, MAX_OUTBOUND_MESSAGES>  = make_static!(Channel::new());
    let external_channel: &'static mut Channel<CriticalSectionRawMutex, Message, MAX_OUTBOUND_MESSAGES> = make_static!(Channel::new());

    let inbound_sender = internal_channel.sender();
    let outbound_receiver = external_channel.receiver();
    let outbound_sender = external_channel.sender();
    let inbound_receiver = internal_channel.receiver();

    let pin = make_static!(Output::new(p.PIN_7, Level::Low));

    let mut config = embassy_usb::Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("Embassy");
    config.product = Some("USB-serial logger");
    config.serial_number = Some("12345678");
    config.max_power = 100;
    config.max_packet_size_0 = MAX_PACKET_SIZE as u8;

    // Required for windows compatiblity.
    // https://developer.nordicsemi.com/nRF_Connect_SDK/doc/1.9.1/kconfig/CONFIG_CDC_ACM_IAD.html#help
    config.device_class = 0xEF;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;
    config.composite_with_iads = true;

    let client: &mut Channel<CriticalSectionRawMutex, Message, MAX_OUTBOUND_MESSAGES> = make_static!(Channel::new());
    // Create the driver, from the HAL.
    let driver = Driver::new(p.USB, client_communicator::Irqs);

    let mut device_descriptor = make_static!([0; 256]);
    let mut config_descriptor = make_static!([0; 256]);
    let mut bos_descriptor = make_static!([0; 256]);
    let mut control_buf = make_static!([0; 64]);
    let mut state = make_static!(State::new());

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
    let usb = builder.build();

    let (ref mut usb_sender, ref mut usb_receiver) = make_static!(class.split());

    let client_communicator = ClientCommunicator::new(usb, usb_sender, usb_receiver, client.sender(), client.receiver());

    let watchdog = make_static!(Watchdog::new(p.WATCHDOG));
    watchdog.start(Duration::from_secs(5));

    let runtime = make_static!(Runtime::new(client_communicator.get_sender()));

    let th_clk = p.PIN_10;
    let th_miso = p.PIN_12;
    let rx_dma = p.DMA_CH3;

    let mut config = embassy_rp::spi::Config::default();
    config.frequency = THERMOCOUPLE_SPI_FREQUENCY;
    let thermocouple_spi = make_static!(Spi::new_rxonly(p.SPI1, th_clk, th_miso, rx_dma, config));

    let thermocouple_pin = Output::new(p.PIN_11, Level::High);
    let thermocouple = make_static!(MAX31855::new(thermocouple_spi, thermocouple_pin));

    let sda = p.PIN_14;
    let scl = p.PIN_15;

    let i2c = embassy_rp::i2c::I2c::new_async(
        p.I2C1,
        scl,
        sda,
        I2cIrqs,
        embassy_rp::i2c::Config::default(),
    );

    let ads_config = AdsConfig {
        sensor_min_voltage: 0.5,
        sensor_max_voltage: 4.5,
        sensor_min_value: 0.0,
        sensor_max_value: 200.0,
    };

    let mut ads = Ads1115::new(i2c, ads_config);
    ads.initialize().unwrap();

    interrupt::SWI_IRQ_1.set_priority(Priority::P2);
    let spawner = EXECUTOR_HIGH.start(interrupt::SWI_IRQ_1);
    let _ = spawner.spawn(tasks::read_thermocouple(
        internal_channel.sender(),
        thermocouple,
    ));
    //let _ = spawner.spawn(tasks::read_ads(ads, runtime));

    let signal: &'static mut Signal<CriticalSectionRawMutex, ()> = make_static!(Signal::new());

    spawn_core1(p.CORE1, unsafe { &mut CORE1_STACK }, move || {
        let executor1 = EXECUTOR1.init(Executor::new());
        executor1.run(|spawner| {
            unwrap!(spawner.spawn(client_communicator::run(client_communicator, &mut signal)));
            unwrap!(spawner.spawn(tasks::write_usb(outbound_receiver, usb_sender)));
            unwrap!(spawner.spawn(tasks::process_internal_messages(
                inbound_receiver,
                spawner,
                runtime
            )));
            unwrap!(spawner.spawn(tasks::blink(pin, watchdog)));
        });
    });

    let executor0 = EXECUTOR0.init(Executor::new());
    executor0.run(|spawner| {
        //unwrap!(spawner.spawn(tasks::run_usb(usb)));
    })

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
