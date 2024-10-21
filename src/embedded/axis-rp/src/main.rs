#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(async_closure)]
#![feature(never_type)]

mod client_communicator;
mod systems;
mod axis_peripherals;
mod runtime;
mod i2c;
mod drivers;
mod spi;

use {defmt_rtt as _, panic_probe as _};
use core::future::Future;
use core::ops::Deref;
use core::panic::PanicInfo;
use assign_resources::assign_resources;
use cortex_m::prelude::_embedded_hal_blocking_i2c_Write;
use defmt::{error, info, unwrap};
use defmt::Format;
use embassy_executor::{Executor, InterruptExecutor, Spawner};
use embassy_rp::interrupt::{InterruptExt, Priority};
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::{bind_interrupts, interrupt};
use embassy_rp::i2c::{Async, Config, Error, I2c};
use embassy_time::{Duration, Instant, Timer};
use serde::{Deserialize, Serialize};
use serde_json_core::heapless::String;
use static_cell::{make_static, StaticCell};
use Message::*;
use embassy_rp::peripherals;
use embassy_rp::peripherals::{I2C0, I2C1};
use crate::i2c::{I2c0Bus, I2cAddress, I2cBus};

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

static I2C0_BUS: StaticCell<I2cBus<'static, I2C0>> = StaticCell::new();
static I2C1_BUS: StaticCell<I2cBus<'static, I2C1>> = StaticCell::new();

assign_resources! {
    i2c0: I2c0Resources {
        peripheral: I2C0,
        sda: PIN_12,
        scl: PIN_13,
    }
    i2c1: I2c1Resources {
        peripheral: I2C1,
        sda: PIN_26,
        scl: PIN_27,
    }
    gpio: ExtraGpios {
        gpio1: PIN_2,
        gpio2: PIN_3,
        gpio3: PIN_4,
        gpio4: PIN_5,
        gpio5: PIN_6,
        gpio6: PIN_7,
    }
    spi0: Spi0Resources {
        spi1_rx:  PIN_8,
        spi1_cs0: PIN_9,
        spi1_sck: PIN_10,
        spi1_tx:  PIN_11,
    }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let r = split_resources!(p);


    let mut executor  = EXECUTOR_LOW.init(Executor::new());


    let (i2c0, i2c1 ) = setup_i2c(r.i2c0, r.i2c1);
    let thing = I2C0_BUS.init(I2c0Bus::new(i2c0));


    let data: [u8; 1] = [0b1010_1010];
    let addr1 = I2cAddress::U16(0b0101_0101);
    loop {
        info!("led on!");
        Timer::after_secs(1).await;

        let value: u16 = 0xBABE;

        let mut a = thing.lock().await;
        a.write_async(addr1.clone(), value.to_be_bytes()).await.unwrap();

        info!("led off.");
        a.write_async(addr1.clone(), value.to_be_bytes()).await.unwrap();
        Timer::after_secs(1).await;
    }
}

fn setup_i2c<'a>(i2c0: I2c0Resources, i2c1: I2c1Resources) -> (I2c<'a, I2C0, Async>, I2c<'a, I2C1, Async>) {
    let mut i2c0 = I2c::new_async(i2c0.peripheral, i2c0.scl, i2c0.sda, i2c::I2c0Irqs, Config::default());
    let mut i2c1 = I2c::new_async(i2c1.peripheral, i2c1.scl, i2c1.sda, i2c::I2c1Irqs, Config::default());

    (i2c0, i2c1)
}
