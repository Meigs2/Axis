#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(async_closure)]

use core::fmt::{Debug, Display, Formatter};

use defmt::unwrap;
use embassy_executor::{Executor, InterruptExecutor};

use embassy_rp::interrupt;

use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::interrupt::{InterruptExt, Priority};

use embassy_rp::peripherals::{PIN_24, PIN_26, PIN_28, SPI1};
use embassy_rp::spi::{Async, Config, Spi};
use embassy_rp::watchdog::Watchdog;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Duration;
use embassy_time::TimeoutError;
use embassy_time::Timer;

use static_cell::StaticCell;

use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::task]
async fn max_reading_loop(
    mut spi: Spi<'static, SPI1, Async>,
    mut pin: Output<'static, PIN_24>,
    rt: &'static AxisRuntime,
) {
}

pub struct AxisRuntime {}

mod peripherals {
    use embassy_rp::gpio::Output;
    use embassy_rp::peripherals::{PIN_24, SPI0};
    use embassy_rp::spi::{Async, Spi};

    const CLOCK_FRQ: u32 = 500_000;

    pub struct ThermocoupleState {
        temperature: f32,
    }

    pub enum ThermocoupleError {}

    pub fn parse(buff: &[u8]) -> Result<ThermocoupleState, ThermocoupleError> {
        Ok(ThermocoupleState { temperature: 50.0 })
    }

    pub struct MAX31855 {
        spi: Spi<'static, SPI0, Async>,
        dc: Output<'static, PIN_24>,
    }

    impl MAX31855 {
        pub fn new(spi: Spi<'static, SPI0, Async>, dc: Output<'static, PIN_24>) -> Self {
            Self { spi, dc }
        }

        pub async fn read(&mut self, buf: &mut [u8]) -> ThermocoupleState {
            loop {
                self.dc.set_high();

                if (self.spi.read(buf).await).is_err() {
                    defmt::debug!("MAX31855 read error.");
                    continue;
                }

                if let Ok(s) = parse(&buf) {
                    self.dc.set_low();
                    return s;
                };
            }
        }
    }
}

unsafe fn any_as_u8_slice<T>(p: &T) -> &[u8] {
    core::slice::from_raw_parts((p as *const T) as *const u8, core::mem::size_of::<T>())
}

pub struct Message {
    message_type: MessageType,
    byte_length: u32,
    string_bytes: [u8],
}

impl Message {
    pub unsafe fn to_bytes(&self) -> &[u8] {
        core::slice::from_raw_parts(
            (self as *const Self) as *const u8,
            core::mem::size_of::<MessageType>()
                + core::mem::size_of::<u32>()
                + self.string_bytes.len(),
        )
    }
}

#[repr(u8)]
pub enum MessageType {
    Request = 0,
    Response = 1,
}

pub enum KillSignal {}

static EXECUTOR_HIGH: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_MED: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_LOW: StaticCell<Executor> = StaticCell::new();
static RUNTIME: AxisRuntime = AxisRuntime {};
static WATCHDOG: StaticCell<Watchdog> = StaticCell::new();
static KILL_SIGNAL: Signal<CriticalSectionRawMutex, KillSignal> = Signal::new();

#[interrupt]
unsafe fn SWI_IRQ_1() {
    EXECUTOR_HIGH.on_interrupt()
}

#[interrupt]
unsafe fn SWI_IRQ_0() {
    EXECUTOR_MED.on_interrupt()
}

#[embassy_executor::task]
pub async fn watchdog_monitor(
    watchdog: &'static mut Watchdog,
    mut request_pin: Output<'static, PIN_26>,
    mut reply_pin: Input<'static, PIN_28>,
) {
    watchdog.start(Duration::from_secs(5));

    loop {
        if (wait_with_timeout(3_000, reply_pin.wait_for_high()).await).is_err() {
            watchdog.trigger_reset();
            return;
        }
        request_pin.set_high();

        if (wait_with_timeout(1_000, reply_pin.wait_for_low()).await).is_err() {
            watchdog.trigger_reset();
            return;
        }

        request_pin.set_low();
        watchdog.feed();
    }
}

pub async fn wait_with_timeout<F: core::future::Future>(
    millis: u64,
    fut: F,
) -> Result<F::Output, TimeoutError> {
    embassy_time::with_timeout(Duration::from_millis(millis), fut).await
}

const SPI_CLOCK_FREQ: u32 = 500_000;

#[embassy_executor::main]
async fn main(_s: embassy_executor::Spawner) {
    let p = embassy_rp::init(Default::default());

    let mut startup_pin: Input<'static, PIN_28> = Input::new(p.PIN_28, Pull::None);
    let mut notify_pin: Output<'static, PIN_26> = Output::new(p.PIN_26, Level::Low);

    let watchdog = WATCHDOG.init(Watchdog::new(p.WATCHDOG));

    // Raise
    notify_pin.set_high();
    startup_pin.wait_for_high().await;
    notify_pin.set_low();
    startup_pin.wait_for_low().await;

    let mut spi0_config = Config::default();
    spi0_config.frequency = SPI_CLOCK_FREQ;

    let miso = p.PIN_12;
    let mosi = p.PIN_11;
    let clk = p.PIN_10;
    let cs = p.PIN_16;
    let rx_dma = p.DMA_CH0;

    let spi: Spi<'static, SPI1, Async> = Spi::new_rxonly(p.SPI1, clk, miso, rx_dma, spi0_config);
    let pin = Output::new(p.PIN_24, Level::Low);

    // High-priority executor: SWI_IRQ_1, priority level 2
    interrupt::SWI_IRQ_1.set_priority(Priority::P1);
    let spawner = EXECUTOR_HIGH.start(interrupt::SWI_IRQ_1);
    unwrap!(spawner.spawn(max_reading_loop(spi, pin, &RUNTIME)));
    unwrap!(spawner.spawn(watchdog_monitor(watchdog, notify_pin, startup_pin)));

    KILL_SIGNAL.wait().await;
}
