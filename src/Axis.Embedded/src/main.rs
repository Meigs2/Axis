#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(async_closure)]

use core::fmt::{Debug, Display, Formatter};

use defmt::{info, unwrap};
use embassy_executor::{Executor, InterruptExecutor};
use embassy_rp::{interrupt, Peripherals};

use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::interrupt::{InterruptExt, Priority};
use embassy_rp::peripherals::{PIN_24, SPI1};
use embassy_rp::spi::{Async, Config, Spi};
use embassy_time::{Duration, Instant, Timer, TICK_HZ};
use static_cell::StaticCell;

use {defmt_rtt as _, panic_probe as _};



/// This example shows how async gpio can be used with a RP2040.
///
/// It requires an external signal to be manually triggered on PIN 16. For
/// example, this could be accomplished using an external power source with a
/// button so that it is possible to toggle the signal from low to high.
///
/// This example will begin with turning on the LED on the board and wait for a
/// high signal on PIN 16. Once the high event/signal occurs the program will
/// continue and turn off the LED, and then wait for 2 seconds before completing
/// the loop and starting over again.

#[embassy_executor::task]
async fn client_message_loop(p: Peripherals) {
    let miso = p.PIN_12;
    let mosi = p.PIN_11;
    let clk = p.PIN_10;

    let mut spi= Spi::new(p.SPI1, clk, mosi, miso, p.DMA_CH0, p.DMA_CH1, embassy_rp::spi::Config::default());
    let mut pin = Input::new(p.PIN_4, Pull::None);
}

#[embassy_executor::task]
async fn max_reading_loop(
    mut spi: Spi<'static, SPI1, Async>,
    mut pin: Output<'static, PIN_24>,
    rt: &'static AxisRuntime) {
    let mut buf = [0u8; 4];
    loop {
        buf.fill(0);
        pin.set_high();
        spi.read(buf.as_mut()).await;
        match Max31855State::parse(&buf) {
            Ok(s) => {
                rt.publish(s)
            }
            Err(_) => {}
        };
        pin.set_low();

        Timer::after(Duration::from_millis(1)).await
    }
}

pub struct AxisRuntime {
}

impl AxisRuntime {
    pub fn publish(&self, p0: Max31855State) {
    }
}

pub struct Max31855State {
    temperature: f32
}

impl Max31855State {
    pub fn parse(buff: &[u8]) -> Result<Max31855State, MAX31855Error> {
        Ok(Max31855State{temperature: 50.0})
    }
}

pub enum MAX31855Error {
}

mod peripherals {
    use embassy_rp::gpio::Input;
    use embassy_rp::peripherals::{PIN_24, SPI0};
    use embassy_rp::spi::{Async, Spi};
    use crate::{any_as_u8_slice, Message};

    const CLOCK_FRQ: u32 = 500_000;

    pub struct ThermocoupleState {
        temperature: f32
    }

    pub struct MAX31855 {
        spi: Spi<'static, SPI0, Async>,
        dc: Input<'static, PIN_24>,
    }

    impl MAX31855
    {
        pub fn new(spi: Spi<'static, SPI0, Async>, dc: Input<'static, PIN_24>) -> Self {
            Self { spi, dc }
        }

        pub async fn read(&mut self) {
        }

        pub fn is_pin_high(&mut self) {
            self.dc.is_high();
        }
    }
}

unsafe fn any_as_u8_slice<T>(p: &T) -> &[u8] {
    ::core::slice::from_raw_parts(
        (p as *const T) as *const u8,
        ::core::mem::size_of::<T>(),
    )
}


pub struct Message {
    message_type: MessageType,
    byte_length: u32,
    string_bytes: [u8]
}

impl Message {
    pub unsafe fn to_bytes(&self) -> &[u8] {
        ::core::slice::from_raw_parts(
            (self as *const Self) as *const u8,
            ::core::mem::size_of::<MessageType>() + ::core::mem::size_of::<u32>() + self.string_bytes.len(),
        )
    }
}


#[repr(u8)]
pub enum MessageType {
    Request = 0,
    Response = 1,
}

static EXECUTOR_HIGH: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_MED: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_LOW: StaticCell<Executor> = StaticCell::new();
static RUNTIME: AxisRuntime = AxisRuntime {};

#[interrupt]
unsafe fn SWI_IRQ_1() {
    EXECUTOR_HIGH.on_interrupt()
}

#[interrupt]
unsafe fn SWI_IRQ_0() {
    EXECUTOR_MED.on_interrupt()
}

#[embassy_executor::main]
async fn main(_s: embassy_executor::Spawner) {
    info!("Hello World!");

    let p = embassy_rp::init(Default::default());

    let miso = p.PIN_12;
    let mosi = p.PIN_11;
    let clk = p.PIN_10;
    let cs = p.PIN_16;
    let rx_dma = p.DMA_CH0;

    let spi: Spi<'static, SPI1, Async> = Spi::new_rxonly(p.SPI1, clk, miso, rx_dma, Config::default());
    let mut pin = Output::new(p.PIN_24, Level::Low);

    // High-priority executor: SWI_IRQ_1, priority level 2
    interrupt::SWI_IRQ_1.set_priority(Priority::P2);
    let spawner = EXECUTOR_HIGH.start(interrupt::SWI_IRQ_1);
    unwrap!(spawner.spawn(max_reading_loop(spi, pin, &RUNTIME)));

    // Medium-priority executor: SWI_IRQ_0, priority level 3
    interrupt::SWI_IRQ_0.set_priority(Priority::P3);
    let spawner = EXECUTOR_MED.start(interrupt::SWI_IRQ_0);
    // unwrap!(spawner.spawn(client_message_loop(_p)));

    // Low priority executor: runs in thread mode, using WFE/SEV
    let executor = EXECUTOR_LOW.init(Executor::new());
    loop {
        executor.run(|spawner| {
            // unwrap!(spawner.spawn(run_low()));
        });
    }
}
