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

use crate::client_communicator::ClientCommunicator;
use crate::sensors::ads1115::{Ads1115, AdsConfig};
use crate::sensors::max31855::MAX31855;
use bit_field::BitField;
use core::future::Future;
use defmt::unwrap;
use defmt::Format;
use embassy_executor::{Executor, InterruptExecutor};
use embassy_futures::select::select;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::interrupt::{InterruptExt, Priority};
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::peripherals::{I2C1, PIN_16, PIN_17};

use embassy_rp::spi::Spi;

use embassy_rp::watchdog::Watchdog;
use embassy_rp::{bind_interrupts, interrupt};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::State;

use crate::peripherals::pump_dimmer::DimmerCommand;
use serde::{Deserialize, Serialize};
use serde_json_core::heapless::String;
use static_cell::{make_static, StaticCell};
use Message::*;
use {defmt_rtt as _, panic_probe as _};

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
            reading @ ThermocoupleReading { .. } => {
                self.outbound_sender.send(reading).await;
            }
            reading @ AdsReading { .. } => {
                self.outbound_sender.send(reading).await;
            }
        }
    }
}

pub const MAX_STRING_SIZE: usize = 64;
pub const MAX_PACKET_SIZE: usize = 64;
pub const THERMOCOUPLE_SPI_FREQUENCY: u32 = 500_000;

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

static mut CORE1_STACK: Stack<8192> = Stack::new();
static EXECUTOR_CORE1: StaticCell<Executor> = StaticCell::new();

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

#[embassy_executor::task]
pub async fn dimmer_test(
    dimmer: &'static mut peripherals::pump_dimmer::ZeroCrossDimmer<'static, PIN_16, PIN_17>,
) {
    let sender = dimmer.signal.sender().clone();
    let run = async {
        loop {
            Timer::after(Duration::from_secs(1)).await;
            sender.send(DimmerCommand::PercentOn(1.0)).await;
            Timer::after(Duration::from_secs(1)).await;
            sender.send(DimmerCommand::Off).await;
        }
    };
    select(dimmer.run(), run).await;
}

mod tasks {

    use defmt::{debug, error};

    use embassy_executor::Spawner;

    use embassy_rp::gpio::Output;
    use embassy_rp::peripherals::{I2C1, PIN_11, PIN_7, SPI1, USB};
    use embassy_rp::usb::Driver;
    use embassy_rp::watchdog::Watchdog;
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::channel::{Receiver, Sender};
    use embassy_time::{Duration, Timer};
    use embassy_usb::UsbDevice;

    use crate::client_communicator::ClientCommunicator;
    use crate::sensors::ads1115::Ads1115;
    use crate::sensors::max31855::{Unit, MAX31855};
    use crate::Message::*;
    use crate::{Message, Runtime};

    #[embassy_executor::task]
    pub async fn run_usb(usb: &'static mut ClientCommunicator<'static, 1>) {
        debug!("Running USB");
        usb.run().await;
    }

    #[embassy_executor::task]
    pub async fn run_usb_literal(usb: &'static mut UsbDevice<'static, Driver<'static, USB>>) {
        debug!("Running USB");
        usb.run().await;
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
    pub async fn read_task(
        mut ads: Ads1115<'static, I2C1>,
        inbound_sender: Sender<'static, CriticalSectionRawMutex, Message, 1>,
    ) -> ! {
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
                        inbound_sender.send(Message::AdsReading { value: v }).await;
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

    let device_descriptor = make_static!([0u8; 256]);
    let config_descriptor = make_static!([0u8; 256]);
    let bos_descriptor = make_static!([0u8; 256]);
    let control_buf = make_static!([0u8; 64]);
    let state = make_static!(State::new());

    let data = client_communicator::UsbData {
        device_descriptor,
        config_descriptor,
        bos_descriptor,
        control_buf,
        state,
    };

    let internal_channel = make_static!(Channel::new());
    let external_channel = make_static!(Channel::new());

    let client_tuple = make_static!(ClientCommunicator::new(
        p.USB,
        data,
        internal_channel.sender(),
        external_channel.receiver()
    ));

    let usb = &mut client_tuple.1;
    let client = &mut client_tuple.0;

    let pin = make_static!(Output::new(p.PIN_7, Level::Low));

    let watchdog = make_static!(Watchdog::new(p.WATCHDOG));
    watchdog.start(Duration::from_secs(5));

    let runtime = make_static!(Runtime::new(external_channel.sender()));

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

    let ads_config = AdsConfig {
        sensor_min_voltage: 0.5,
        sensor_max_voltage: 4.5,
        sensor_min_value: 0.0,
        sensor_max_value: 200.0,
    };

    let mut ads = Ads1115::new(i2c, ads_config);

    interrupt::SWI_IRQ_1.set_priority(Priority::P2);
    let spawner = EXECUTOR_HIGH.start(interrupt::SWI_IRQ_1);
    unwrap!(spawner.spawn(tasks::run_usb_literal(usb)));

    spawn_core1(p.CORE1, unsafe { &mut CORE1_STACK }, move || {
        let executor1 = EXECUTOR_CORE1.init(Executor::new());
        executor1.run(|spawner| {
            unwrap!(spawner.spawn(tasks::run_usb(client)));
        });
    });

    let _ = spawner.spawn(tasks::read_thermocouple(
        internal_channel.sender(),
        thermocouple,
    ));

    unwrap!(spawner.spawn(tasks::read_task(ads, internal_channel.sender())));
    let signal_channel: &mut Channel<CriticalSectionRawMutex, DimmerCommand, 1> =
        make_static!(Channel::new());
    let zero_cross_pin = Input::new(p.PIN_16, Pull::None);
    let output_pin = Output::new(p.PIN_17, Level::Low);
    let dimmer = make_static!(peripherals::pump_dimmer::ZeroCrossDimmer::new(
        zero_cross_pin,
        output_pin,
        signal_channel
    ));
    unwrap!(spawner.spawn(dimmer_test(dimmer)));

    // Low priority executor: runs in thread mode, using WFE/SEV
    let executor = EXECUTOR_LOW.init(Executor::new());
    executor.run(|spawner| {
        unwrap!(spawner.spawn(tasks::process_internal_messages(
            internal_channel.receiver(),
            spawner,
            runtime
        )));
        unwrap!(spawner.spawn(tasks::blink(pin, watchdog)));
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
