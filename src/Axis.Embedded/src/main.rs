#![no_std]
#![no_main]
#![feature(async_fn_in_trait)]
#![feature(type_alias_impl_trait)]
#![feature(async_closure)]

extern crate alloc;

use crate::MessageError::InvalidMessageType;
use crate::MessageType::{Ping, Pong};
use alloc::boxed::Box;
use byte_slice_cast::{AsByteSlice, AsMutByteSlice};
use core::fmt::{Debug, Display, Formatter};
use core::future::Future;
use core::pin::{pin, Pin};
use defmt::unwrap;
use embassy_executor::{Executor, InterruptExecutor};
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::interrupt;
use embassy_rp::interrupt::{InterruptExt, Priority};
use embassy_rp::pac::xosc::regs::Startup;
use embassy_rp::peripherals::{PIN_0, PIN_16, PIN_24, PIN_26, PIN_28, SPI0, SPI1};
use embassy_rp::spi::{Async, Config, Spi};
use embassy_rp::watchdog::Watchdog;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::Duration;
use embassy_time::TimeoutError;
use embassy_time::Timer;
use futures::task::LocalFutureObj;
use static_cell::{make_static, StaticCell};
use {defmt_rtt as _, panic_probe as _};
use crate::peripherals::MAX31855;

mod peripherals;

pub struct AxisRuntime<'a> {
    internal_to_external_channel: &'a Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
    external_to_internal_channel: &'a Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
    pub watchdog: &'a mut Watchdog,
    client_spi: &'a mut Spi<'a, SPI0, Async>,
}

impl<'a> AxisRuntime<'a> {
    pub fn new(
        internal_to_external_channel: &'a Channel<CriticalSectionRawMutex, MessageDTO, 1>,
        external_to_internal_channel: &'a Channel<CriticalSectionRawMutex, MessageDTO, 1>,
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

    async fn handle(&mut self, m: MessageDTO<'a>) -> Result<(), MessageError> {
        match m.message_type {
            Ping => self.SendPong().await,
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

        // First, process inbound messages, if any, then outbound.
        let task = read_message(
            &mut self.client_spi,
            &mut type_buff,
            &mut len_buff,
            &mut content_buffer,
            content,
        );

        let _ = embassy_time::with_timeout(Duration::from_micros(1), task)
            .await
            .map(|m| {
                m.map(|b| async {
                    embassy_time::with_timeout(
                        Duration::from_micros(1),
                        self.external_to_internal_channel.send(b),
                    )
                        .await
                })
            });

    }
    async fn process_internal_to_external(&mut self) {
        let task = self.internal_to_external_channel.recv();

        let _ = embassy_time::with_timeout(Duration::from_micros(1), task)
            .await
            .map(move |m| async move {
                unsafe {
                    embassy_time::with_timeout(
                        Duration::from_millis(1),
                        self.client_spi.write(m.to_bytes()))
                        .await
                }
            });
    }
}

async fn read_message<'a>(
    spi: &mut Spi<'a, SPI0, Async>,
    type_buff: &mut [u8; 1],
    len_buff: &mut [u8; 2],
    content_buffer: &mut [u8; 1024],
    content: &'a [u8],
) -> Option<MessageDTO<'a>> {
    // get type byte.
    if let Err(_) = spi.read(type_buff).await {
        spi.flush().unwrap();
        return None;
    }

    let message_type = match MessageType::from_u8(type_buff[0]) {
        Ok(t) => t,
        Err(_) => return None,
    };

    // read length buffer
    if let Err(_) = spi.read(len_buff).await {
        spi.flush().unwrap();
        return None;
    }

    let content_len = u16::from_le_bytes([len_buff[0], len_buff[1]]);
    if content_len > 0 {
        if let Err(_) = spi.read(&mut content_buffer[..content_len as usize]).await {
            spi.flush().unwrap();
            return None;
        }
    }

    content_buffer[..content_len as usize].clone_from_slice(content);

    let message = MessageDTO {
        message_type,
        content_len,
        content: content[..content_len as usize].as_ref(),
    };
    Some(message)
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

async fn publish_message<'a>(
    spi: &mut Spi<'a, SPI0, Async>,
    external_to_internal_channel: &Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
) {
    let mut type_buff = [0u8; 1];
    let mut len_buff = [0u8; 2];
    let mut content_buffer = [0u8; 1024];
    let content = make_static!([0u8; 1024]);

    loop {
        let message = match read_message(
            spi,
            &mut type_buff,
            &mut len_buff,
            &mut content_buffer,
            content,
        )
        .await
        {
            Some(value) => value,
            None => continue,
        };

        external_to_internal_channel.send(message).await;
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
}

impl MessageType {
    pub fn from_u8(val: u8) -> Result<MessageType, MessageError> {
        match val {
            1 => Ok(Ping),
            2 => Ok(Pong),
            _ => Err(InvalidMessageType),
        }
    }
}

pub enum MessageError {
    InvalidMessageType,
    Unknown,
}

impl<'a> MessageDTO<'a> {
    pub unsafe fn to_bytes(&self) -> &[u8] {
        core::slice::from_raw_parts(
            (self as *const Self) as *const u8,
            1 + 2 + self.content.len(),
        )
    }
}

pub struct Messenger<'a> {
    channel: &'a Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
}

impl<'a> Messenger<'a> {
    pub fn new(channel: &'a Channel<CriticalSectionRawMutex, MessageDTO, 1>) -> Self {
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

static INTERNAL_TO_EXTERNAL_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, MessageDTO, 1>> =
    StaticCell::new();
static EXTERNAL_TO_INTERNAL_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, MessageDTO, 1>> =
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

// #[embassy_executor::task]
// pub async fn watchdog_monitor(
//     watchdog: &'static mut Watchdog,
//     mut request_pin: Output<'static, PIN_26>,
//     mut reply_pin: Input<'static, PIN_28>,
// ) {
//     watchdog.start(Duration::from_secs(5));
//
//     loop {
//         if (wait_with_timeout(3_000, reply_pin.wait_for_high()).await).is_err() {
//             watchdog.trigger_reset();
//             return;
//         }
//         request_pin.set_high();
//
//         if (wait_with_timeout(1_000, reply_pin.wait_for_low()).await).is_err() {
//             watchdog.trigger_reset();
//             return;
//         }
//
//         request_pin.set_low();
//         watchdog.feed();
//     }
// }

pub async fn wait_with_timeout<F: Future>(millis: u64, fut: F) -> Result<F::Output, TimeoutError> {
    embassy_time::with_timeout(Duration::from_millis(millis), fut).await
}

const SPI_CLOCK_FREQ: u32 = 500_000;

#[embassy_executor::task]
pub async fn main_loop(runtime: &'static mut AxisRuntime<'static>) {
    embassy_futures::join::join(runtime.run(), async { KILL_SIGNAL.wait().await; return  } ).await;
    runtime.watchdog.trigger_reset();
}

#[embassy_executor::main]
async fn main(_s: embassy_executor::Spawner) {
    let p = embassy_rp::init(Default::default());

    let spawner = EXECUTOR_HIGH.start(interrupt::SWI_IRQ_1);

    let external_to_internal_channel = EXTERNAL_TO_INTERNAL_CHANNEL.init(Channel::new());
    let internal_to_external_channel = INTERNAL_TO_EXTERNAL_CHANNEL.init(Channel::new());
    let watchdog = WATCHDOG.init(Watchdog::new(p.WATCHDOG));

    let mut spi0_config = Config::default();
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
    let mut config = Config::default();
    config.frequency = 500_000;
    let mut thermocouple_spi: &mut Spi<'static, SPI1, Async> = THERMOCOUPLE_SPI.init(Spi::new_rxonly(
        p.SPI1,
        th_clk,
        th_miso,
        rx_dma,
        config
    ));

    let thermocouple_pinout = Output::new(p.PIN_11, Level::Low);
    let thermocouple = MAX31855::new(thermocouple_spi, thermocouple_pinout);

    let content = make_static!([0u8; 1024]);

    loop {
        let mut type_buff = [0u8; 1];
        let mut len_buff = [0u8; 2];
        let mut content_buffer = [0u8; 1024];

        let message = read_message(
            &mut client_spi,
            &mut type_buff,
            &mut len_buff,
            &mut content_buffer,
            content,
        )
        .await;
        match message {
            None => {}
            Some(m) => match m.message_type {
                MessageType::Startup => {
                    break;
                }
                _ => {}
            },
        }
    }

    let runtime = RUNTIME.init(AxisRuntime::new(
        internal_to_external_channel,
        external_to_internal_channel,
        watchdog,
        client_spi,
    ));


    // High-priority executor: SWI_IRQ_1, priority level 2
    interrupt::SWI_IRQ_1.set_priority(Priority::P1);

    let _ = spawner.spawn(main_loop(runtime));

    KILL_SIGNAL.wait().await;
}
