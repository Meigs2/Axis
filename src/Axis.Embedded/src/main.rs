#![no_std]
#![no_main]
#![feature(async_fn_in_trait)]
#![feature(type_alias_impl_trait)]
#![feature(async_closure)]
#![feature(never_type)]

mod axis_peripherals;

use core::future::Future;
use defmt::{info, unwrap};
use embassy_executor::{Executor, InterruptExecutor, Spawner};
use embassy_futures::join::join;
use embassy_futures::select::Either;
use embassy_net::tcp::TcpSocket;
use embassy_net::{Stack, StackResources};
use embassy_rp::peripherals::{PIN_16, SPI0, SPI1, USB};
use embassy_rp::{bind_interrupts, interrupt};
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::interrupt::{InterruptExt, Priority};
use embassy_rp::spi::{Async, Spi};
use embassy_rp::usb::Driver;
use embassy_rp::watchdog::Watchdog;
use embassy_sync::blocking_mutex::CriticalSectionMutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_ncm::embassy_net::{Device, Runner, State as NetState};
use embassy_usb::class::cdc_ncm::{CdcNcmClass, State};
use embassy_usb::{Builder, Config, UsbDevice};
use embassy_usb::class::cdc_acm::CdcAcmClass;
use embassy_usb::driver::EndpointError;
use embedded_io::asynch::Write;
use futures::SinkExt;
use static_cell::{make_static, StaticCell};
use {defmt_rtt as _, panic_probe as _};
use crate::MessageError::InvalidMessageType;
use crate::MessageType::{Ping, Pong, ThermocoupleReading};

pub struct AxisRuntime<'a> {
    internal_to_external_channel: &'a Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
    external_to_internal_channel: &'a Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
    pub watchdog: &'a mut Watchdog,
    client_spi: &'a mut Spi<'a, SPI0, Async>,
}

impl<'a> AxisRuntime<'a> {
    pub fn new(
        internal_to_external_channel: &'a Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
        external_to_internal_channel: &'a Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
        watchdog: &'a mut Watchdog,
        client_spi: &'a mut Spi<'a, SPI0, Async>,
    ) -> Self {
        Self {
            internal_to_external_channel,
            external_to_internal_channel,
            watchdog,
            client_spi,
        }
    }

    async fn handle(m: MessageDTO<'a>) -> Result<Option<MessageType>, MessageError> {
        match m.message_type {
            Ping => Ok(Some(Pong)),
            _ => Err(InvalidMessageType),
        }
    }

    async fn SendPong(&mut self) -> Result<(), MessageError> {
        self.internal_to_external_channel
            .send(MessageDTO {
                message_type: Pong,
                content_len: 0,
                content: &[0u8; 0],
            })
            .await;
        self.watchdog.feed();
        Ok(())
    }

    pub async fn run(&mut self) {
        loop {
            self.process_internal_to_external().await;
            self.process_external_to_internal().await;
        }
    }

    async fn process_external_to_internal(&mut self) {
        let mut type_buff = [0u8; 1];
        let mut len_buff = [0u8; 2];
        let mut content_buffer = [0u8; 1024];
        let content = make_static!([0u8; 1024]);
    }

    async fn process_internal_to_external(&mut self) {
        let task = self.internal_to_external_channel.recv();

        let _ = embassy_time::with_timeout(Duration::from_micros(1), task)
            .await
            .map(move |m| async move {
                unsafe {
                    embassy_time::with_timeout(
                        Duration::from_millis(1),
                        self.client_spi.write(core::slice::from_raw_parts(
                          (&m as *const MessageDTO) as *const u8,
                          1 + 2 + m.content.len()),
                        ))
                }.await
            });
    }
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

#[embassy_executor::task]
pub async unsafe fn client_mcu_communication_loop(
    spi: &'static mut Spi<'static, SPI0, Async>,
    external_to_internal_channel: &'static Channel<CriticalSectionRawMutex, MessageDTO<'static>, 1>,
    internal_to_external_channel: &'static Channel<CriticalSectionRawMutex, MessageDTO<'static>, 1>,
) {
    let mut type_buff = [0u8; 1];
    let mut len_buff = [0u8; 2];
    let mut content_buffer = [0u8; 1024];
    let content = make_static!([0u8; 1024]);

    loop {
        let work = internal_to_external_channel.recv();

        let _a = embassy_time::with_timeout(Duration::from_micros(1), work)
            .await
            .map(|m| async {
                embassy_time::with_timeout(
                    Duration::from_micros(1),
                    internal_to_external_channel.send(m),
                )
                .await
            });
    }
}

pub struct MessageDTO<'a> {
    message_type: MessageType,
    content_len: u16,
    content: &'a [u8],
}

#[repr(u8)]
pub enum MessageType {
    Startup = 0,
    Acknowledge = 1,
    Ping = 2,
    Pong = 3,
    ThermocoupleReading = 4,
}

impl MessageType {
    pub fn from_u8(val: &u8) -> Result<MessageType, MessageError> {
        match val {
            1 => Ok(Ping),
            2 => Ok(Pong),
            _ => Err(InvalidMessageType),
        }
    }
}

#[derive(Debug)]
pub enum MessageError {
    InvalidMessageType,
    Unknown,
}

impl<'a> MessageDTO<'a> {
    pub fn to_bytes(&self, buffer: &'a mut [u8]) -> &'a [u8] {
        let content_len = usize::from(self.content_len);

        if buffer.len() < content_len + 3 {
            return &[];  // or handle this case differently
        }

        buffer[0] = match self.message_type {
            MessageType::Startup => 0,
            MessageType::Acknowledge => 1,
            MessageType::Ping => 2,
            MessageType::Pong => 3,
            MessageType::ThermocoupleReading => 4,
        };

        let content_len_bytes = self.content_len.to_be_bytes();
        buffer[1..3].copy_from_slice(&content_len_bytes);

        buffer[3..3+content_len].copy_from_slice(self.content);

        &buffer[..3+content_len]
    }

    pub fn parse(data: &[u8]) -> Option<MessageDTO> {
        let t = MessageType::from_u8(data[..1].first().unwrap());

        let n = u16::from_le_bytes(data[2..4].try_into().unwrap());

        let data = &data[5..n as usize];

        Some(MessageDTO {
            message_type: t.unwrap(),
            content_len: n,
            content: data,
        })
    }
}

pub struct Messenger<'a> {
    channel: &'a Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
}

impl<'a> Messenger<'a> {
    pub fn new(channel: &'a Channel<embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex, MessageDTO<'a>, 1>) -> Self {
        Self { channel }
    }

    pub async fn publish(&self, m: MessageDTO<'a>) {
        self.channel.send(m).await;
    }

    pub async fn receive(&self) -> MessageDTO<'a> {
        self.channel.recv().await
    }
}

pub enum SignalFlag {}

static EXECUTOR_HIGH: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_MED: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_LOW: StaticCell<Executor> = StaticCell::new();
static RUNTIME: StaticCell<AxisRuntime> = StaticCell::new();
static WATCHDOG: StaticCell<Watchdog> = StaticCell::new();
static CLIENT_SPI: StaticCell<Spi<SPI0, Async>> = StaticCell::new();
static THERMOCOUPLE_SPI: StaticCell<Spi<SPI1, Async>> = StaticCell::new();

static EXTERNAL_TO_INTERNAL_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, MessageDTO, 1>> =
    StaticCell::new();

static TEST: StaticCell<CriticalSectionMutex<Channel<CriticalSectionRawMutex, MessageDTO, 1>>> = StaticCell::new();

static KILL_SIGNAL: Signal<CriticalSectionRawMutex, SignalFlag> = Signal::new();

#[interrupt]
unsafe fn SWI_IRQ_1() {
    EXECUTOR_HIGH.on_interrupt()
}

#[interrupt]
unsafe fn SWI_IRQ_0() {
    EXECUTOR_MED.on_interrupt()
}

pub async fn wait_with_timeout<F: Future>(millis: u64, fut: F) -> Result<F::Output, embassy_time::TimeoutError> {
    embassy_time::with_timeout(Duration::from_millis(millis), fut).await
}

const SPI_CLOCK_FREQ: u32 = 500_000;

static THERMOCOUPLE_BUFFER: &[u8] = [0u8; 4].as_slice();

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

#[embassy_executor::task]
pub async fn usb_reader(mut class: &'static mut CdcAcmClass<'static, Driver<'static, USB>>) {
    loop {
        class.wait_connection().await;
        defmt::info!("Connected");
        let _ = echo(&mut class).await;
        defmt::info!("Disconnected");
    }
}

#[embassy_executor::main]
async fn main(_s: embassy_executor::Spawner) {
    let p = embassy_rp::init(Default::default());
    let spawner = EXECUTOR_HIGH.start(interrupt::SWI_IRQ_1);

    // Low priority executor: runs in thread mode, using WFE/SEV
    let executor = EXECUTOR_LOW.init(Executor::new());
    let executor = executor.run(|spawner| {
        unwrap!(spawner.spawn(usb_task(usb, class)));
    });

    // Run the USB device.
    // let usb_fut = usb.run();

    // Do stuff with the class!

    let external_to_internal_channel = EXTERNAL_TO_INTERNAL_CHANNEL.init(Channel::new());

    static INTERNAL_TO_EXTERNAL_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, MessageDTO, 1>> = StaticCell::new();
    let c = &*INTERNAL_TO_EXTERNAL_CHANNEL.init(Channel::new());

    let watchdog = WATCHDOG.init(Watchdog::new(p.WATCHDOG));

    let mut spi0_config = embassy_rp::spi::Config::default();
    spi0_config.frequency = 1_000_000;

    let client_mosi = p.PIN_3;
    let client_miso = p.PIN_0;
    let clk = p.PIN_2;
    let cs = p.PIN_1;
    let mosi_dma = p.DMA_CH1;
    let miso_dma = p.DMA_CH2;

    let mut client_spi: &mut Spi<'static, SPI0, Async> = CLIENT_SPI.init(Spi::new(
        p.SPI0,
        clk,
        client_mosi,
        client_miso,
        mosi_dma,
        miso_dma,
        spi0_config,
    ));

    let th_clk = p.PIN_10;
    let th_miso = p.PIN_12;
    let rx_dma = p.DMA_CH3;
    let mut config = embassy_rp::spi::Config::default();
    config.frequency = 500_000;
    let mut thermocouple_spi: &mut Spi<'static, SPI1, Async> =
        THERMOCOUPLE_SPI.init(Spi::new_rxonly(p.SPI1, th_clk, th_miso, rx_dma, config));

    let thermocouple_pinout = Output::new(p.PIN_11, Level::Low);
    let thermocouple = MAX31855::new(thermocouple_spi, thermocouple_pinout);

    let content = [0u8; 1024];

    let mut type_buff = [0u8; 1];
    let mut len_buff = [0u8; 2];
    let mut content_buffer = [0u8; 1024];

    let mut gpio = Output::new(p.PIN_7, Level::Low);
    let mut inbound_flag = Input::new(p.PIN_4, Pull::None);

    loop {
        let blink_task = async {
            gpio.set_high();
            Timer::after(Duration::from_millis(500)).await;
            gpio.set_low();
            Timer::after(Duration::from_millis(500)).await;
        };
    }


    loop {
        gpio.set_high();
        Timer::after(Duration::from_millis(100)).await;
        gpio.set_low();
        Timer::after(Duration::from_millis(100)).await;
    }

    let c2 = c;
    let runtime = RUNTIME.init(AxisRuntime::new(
        c2,
        external_to_internal_channel,
        watchdog,
        client_spi,
    ));


    // High-priority executor: SWI_IRQ_1, priority level 2
    interrupt::SWI_IRQ_1.set_priority(Priority::P1);
    unwrap!(spawner.spawn(main_loop(runtime)));

    let c2 = c;
    unwrap!(spawner.spawn(thermocouple_read(thermocouple, c2)));

    KILL_SIGNAL.wait().await;
}
