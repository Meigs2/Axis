#![no_std]
#![no_main]
#![feature(async_fn_in_trait)]
#![feature(type_alias_impl_trait)]
#![feature(async_closure)]
#![feature(never_type)]

mod axis_peripherals;

use serde::{Serialize, Deserialize};

use crate::MessageError::InvalidMessageType;
use crate::MessageType::{Ping, Pong, ThermocoupleReading};
use core::future::Future;
use core::str::from_utf8;
use byte_slice_cast::{AsByteSlice, AsMutByteSlice};
use cortex_m::prelude::_embedded_hal_blocking_serial_Write;
use defmt::{debug, error};
use embassy_executor::{Executor, InterruptExecutor, Spawner};
use embassy_futures::join::{join, join3, join4, join5};
use embassy_futures::select::Either;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::interrupt::{InterruptExt, Priority};
use embassy_rp::peripherals::{PIN_16, SPI0, SPI1, USB};
use embassy_rp::spi::{Async, Spi};
use embassy_rp::usb::Driver;
use embassy_rp::watchdog::Watchdog;
use embassy_rp::{bind_interrupts, interrupt};
use embassy_rp::pac::io::Gpio;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::CriticalSectionMutex;
use embassy_sync::channel::{Channel, Receiver, RecvFuture, Sender};
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::{Builder, UsbDevice};
use embassy_usb::driver::EndpointError;
use futures::SinkExt;
use serde::ser::SerializeStruct;
use serde_json_core::{from_slice, from_str, heapless, to_slice};
use serde_json_core::de::Error;
use serde_json_core::heapless::{String, Vec};
use static_cell::{make_static, StaticCell};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
});


pub struct AxisRuntime<'a> {
    pub watchdog: &'a mut Watchdog,
    client_spi: &'a mut Spi<'a, SPI0, Async>,
}

#[embassy_executor::task]
async fn read_thermocouple(mut pin: Output<'static, PIN_16>) {
    loop {
        pin.set_high();
        Timer::after(Duration::from_millis(500)).await;
        pin.set_low();
        Timer::after(Duration::from_millis(500)).await;
    }
}

#[derive(Serialize, Deserialize)]
pub struct MessageDTO<const N: usize> {
    message_type: MessageType,
    contents: String<N>
}

#[derive(Copy, Clone, Serialize, Deserialize)]
pub enum MessageType {
    Unknown,
    Startup,
    Acknowledge,
    Ping,
    Pong,
    ThermocoupleReading,
    Log,
    Error,
}

#[derive(Debug)]
pub enum MessageError {
    InvalidMessageType,
    Unknown,
}

pub enum SignalFlag {}

const N: usize = 64;

static EXECUTOR_HIGH: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_MED: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_LOW: StaticCell<Executor> = StaticCell::new();
static RUNTIME: StaticCell<AxisRuntime> = StaticCell::new();
static WATCHDOG: StaticCell<Watchdog> = StaticCell::new();
static CLIENT_SPI: StaticCell<Spi<SPI0, Async>> = StaticCell::new();
static THERMOCOUPLE_SPI: StaticCell<Spi<SPI1, Async>> = StaticCell::new();

static INTERNAL_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, MessageDTO<N>, 1>> =
    StaticCell::new();

static EXTERNAL_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, MessageDTO<N>, 1>> =
    StaticCell::new();

static TEST: StaticCell<CriticalSectionMutex<Channel<CriticalSectionRawMutex, MessageDTO<N>, 1>>> =
    StaticCell::new();

static KILL_SIGNAL: Signal<CriticalSectionRawMutex, SignalFlag> = Signal::new();

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

const SPI_CLOCK_FREQ: u32 = 500_000;

static THERMOCOUPLE_BUFFER: &[u8] = [0u8; 4].as_slice();

#[embassy_executor::main]
async fn main(_s: embassy_executor::Spawner) {
    let p = embassy_rp::init(Default::default());
    let spawner = EXECUTOR_HIGH.start(interrupt::SWI_IRQ_1);
    // Create the driver, from the HAL.
    let driver = Driver::new(p.USB, Irqs);

    const MAX_PACKET_SIZE: u8 = 64;
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

    let mut usb = builder.build();

    let internal_channel = INTERNAL_CHANNEL.init(Channel::new());
    let external_channel = EXTERNAL_CHANNEL.init(Channel::new());

    let inbound_sender = internal_channel.sender();
    let outbound_receiver = external_channel.receiver();
    let outbound_sender = external_channel.sender();
    let inbound_receiver = internal_channel.receiver();

    let (mut usb_sender, mut usb_reader) = class.split();

    let mut pin = Output::new(p.PIN_7, Level::Low);

    let mut watchdog = Watchdog::new(p.WATCHDOG);
    watchdog.start(Duration::from_secs(5));

    let a = async {
        loop {
            pin.set_high();
            Timer::after(Duration::from_millis(500)).await;
            pin.set_low();
            Timer::after(Duration::from_millis(500)).await;
            watchdog.feed();
        }
    };

    let run_fut = usb.run();
    let read_usb_fut = async {
        let mut buff = [0u8; 64];
        usb_reader.wait_connection().await;
        loop {
            match usb_reader.read_packet(&mut buff[..]).await {
                Ok(s) => {
                    let stopwatch = embassy_time::Instant::now();
                    let string = from_utf8(&buff[..s]).unwrap();
                    debug!("Read packet: {:?}", string);
                    let result: Result<(MessageDTO<N>, usize), Error> = from_str(string);
                    match result {
                        Ok(m) => {
                            inbound_sender.send(m.0).await;
                        },
                        Err(e) => {
                            error!("Error deserializing packet.")
                        }
                    }
                    debug!("Read/Write Operation Elapsed: {:?} microseconds", stopwatch.elapsed().as_micros());
                }
                Err(e) => {
                    error!("Error reading packet: {:?}", e)
                }
            }
        }
    };
    let write_usb_fut = async {
        let mut buffer: Vec<u8, N> = Vec::new();
        for _ in 0..N-1 {
            buffer.push(0x00).unwrap();
        }
        loop {
            let m = outbound_receiver.recv().await;

            if let Ok(s) = to_slice(&m, &mut buffer[..]) {
                if let Ok(x) = usb_sender.write_packet(&buffer[..s]).await {
                }
            }
        }
    };
    let fut = async {
        loop {
            let message = inbound_receiver.recv().await;
            outbound_sender.send(message).await;
        }
    };

    let a = join5(run_fut, read_usb_fut,write_usb_fut, fut,a).await;
    //let a = join5(run_fut, read_usb_fut, write_usb_fut, fut, a).await;

    // Low priority executor: runs in thread mode, using WFE/SEV
    let executor = EXECUTOR_LOW.init(Executor::new());
    let executor = executor.run(|spawner| {
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

