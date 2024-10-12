#![no_std]
#![no_main]
#![feature(async_fn_in_trait)]
#![feature(type_alias_impl_trait)]
#![feature(async_closure)]
#![feature(never_type)]

mod client_communicator;
mod systems;
mod peripherals;
mod pid;
mod runtime;

use crate::client_communicator::ClientCommunicator;
use core::future::Future;
use core::ops::Deref;
use defmt::unwrap;
use defmt::Format;
use embassy_executor::{Executor, InterruptExecutor};
use embassy_futures::select::select;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::interrupt::{InterruptExt, Priority};
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::peripherals::{I2C0, I2C1, PIN_16, PIN_17};

use embassy_rp::spi::Spi;

use embassy_rp::watchdog::Watchdog;
use embassy_rp::{bind_interrupts, interrupt};
use embassy_rp::i2c::I2c;
use embassy_rp::interrupt::typelevel::I2C0_IRQ;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use embassy_usb::class::cdc_acm::State;
use log::debug;

use crate::peripherals::pump_dimmer::DimmerCommand;
use serde::{Deserialize, Serialize};
use serde_json_core::heapless::String;
use static_cell::{make_static, StaticCell};
use Message::*;
use {defmt_rtt as _, panic_probe as _};

use crate::systems::i2c_manager;
use crate::systems::i2c_manager::{I2cManager, I2cMutex};

bind_interrupts!(struct I2c0Irqs {
    I2C0_IRQ => embassy_rp::i2c::InterruptHandler<I2C0>;
});

bind_interrupts!(struct I2c1Irqs {
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<I2C1>;
});

pub const MAX_STRING_SIZE: usize = 64;
pub const MAX_PACKET_SIZE: usize = 64;
pub const THERMOCOUPLE_SPI_FREQUENCY: u32 = 500_000;

#[derive(Format, Debug, Serialize, Deserialize)]
pub enum Message {
    Ping,
    Pong { value: String<MAX_STRING_SIZE> },
    ThermocoupleReading { value: f32 },
    AdsReading { value: f32 },
    BrewSwitch { is_on: bool },
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
    let negative = (bits & (1 << (len - 1))) != 0;
    if negative {
        ((bits as i32) << shift) as i16 / divisor
    } else {
        bits as i16
    }
}

pub trait PeripheralError {}

pub trait AxisPeripheral {
    fn initialize() -> Result<(), String<MAX_STRING_SIZE>>;
}

#[embassy_executor::task]
async fn run_low(uart: &'static mut I2cManager) {
    loop {
        uart.get_i2c0().await;

        // Spin-wait to simulate a long CPU computation
        embassy_time::block_for(embassy_time::Duration::from_secs(2)); // ~2 seconds

        let end = Instant::now();

        Timer::after_ticks(82983).await;
    }
}

#[embassy_executor::task]
async fn run_high(uart: &'static mut I2cManager) {
    loop {
        let start = Instant::now();

        // Spin-wait to simulate a long CPU computation
        embassy_time::block_for(embassy_time::Duration::from_secs(2)); // ~2 seconds

        let end = Instant::now();

        Timer::after_ticks(82983).await;
    }
}

static MANAGER: StaticCell<I2cManager<'static>> =  StaticCell::new();

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = embassy_rp::init(Default::default());

    let blink_pin = Output::new(p.PIN_25, Level::Low);

    let uart_tx = p.PIN_0;
    let uart_rx = p.PIN_1;

    let gpio2 = p.PIN_2;
    let gpio3 = p.PIN_3;
    let gpio4 = p.PIN_4;
    let gpio5 = p.PIN_5;
    let gpio6 = p.PIN_6;
    let gpio7 = p.PIN_7;

    let spi1_rx = p.PIN_8;
    let spi1_cs0 = p.PIN_9;
    let spi1_sck = p.PIN_10;
    let spi1_tx = p.PIN_11;

    let i2c0_sda = p.PIN_12;
    let i2c0_scl = p.PIN_13;

    let io_int = p.PIN_14;

    let spi0_cs1 = p.PIN_15;
    let spi0_cs0 = p.PIN_17;
    let spi0_scp = p.PIN_18;

    let i2c1_sda = p.PIN_20;
    let i2c1_scl = p.PIN_21;

    let mux_int = p.PIN_26;

    let hv_io1 = p.PIN_27;
    let zc_sig = p.PIN_28;
    let hv_io2 = p.PIN_29;

    let manager = I2cManager::new(I2c::new_async(p.I2C0, i2c0_scl, i2c0_sda, I2c0Irqs, embassy_rp::i2c::Config::default()),
                                  I2c::new_async(p.I2C1, i2c1_sda, i2c1_scl, I2c1Irqs, embassy_rp::i2c::Config::default()));

    let manager = MANAGER.init(manager);

    let executor = EXECUTOR_LOW.init(Executor::new());

    executor.run(|spawner|
        {
            spawner.must_spawn(run_low(manager));
            spawner.must_spawn(run_high(manager));
        });
}
