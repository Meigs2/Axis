#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(async_closure)]
#![feature(never_type)]

mod client_communicator;
mod systems;
mod runtime;
mod drivers;

use core::cmp::max;
use {defmt_rtt as _, panic_probe as _};
use core::future::Future;
use core::ops::Deref;
use assign_resources::assign_resources;
use cortex_m::prelude::_embedded_hal_blocking_i2c_Write;
use defmt::{error, info, unwrap};
use defmt::Format;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::{Executor, InterruptExecutor, Spawner};
use embassy_rp::interrupt::{InterruptExt, Priority};
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::{bind_interrupts, i2c, spi, interrupt};
use embassy_rp::gpio::{AnyPin, Level, Output};
use embassy_rp::i2c::{Config, Error, I2c};
use embassy_time::{Duration, Instant, Timer};
use serde::{Deserialize, Serialize};
use serde_json_core::heapless::String;
use static_cell::{make_static, StaticCell};
use Message::*;
use embassy_rp::peripherals;
use embassy_rp::peripherals::{I2C0, I2C1, SPI0, SPI1};
use embassy_rp::spi::Spi;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use crate::drivers::ads1119;
use crate::drivers::ads1119::{Ads1119, ConfigRegister, ConversionMode, MuxConfig, VoltageReference};
use crate::drivers::pca9544a::Channel;

pub const MAX_STRING_SIZE: usize = 64;
pub const MAX_PACKET_SIZE: usize = 64;
pub const THERMOCOUPLE_SPI_FREQUENCY: u32 = 500_000;

pub type SpiBus<'a, T: spi::Instance> = Mutex<CriticalSectionRawMutex, Spi<'a, T, spi::Async>>;
pub type I2cBus<'a, T: i2c::Instance> = Mutex<CriticalSectionRawMutex, I2c<'a, T, i2c::Async>>;

bind_interrupts!(pub struct I2c0Irqs {
    I2C0_IRQ => embassy_rp::i2c::InterruptHandler<I2C0>;
});

bind_interrupts!(pub struct I2c1Irqs {
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<I2C1>;
});

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

type I2c0Bus = Mutex<CriticalSectionRawMutex, I2c<'static, I2C0, i2c::Async>>;
type I2c1Bus = Mutex<CriticalSectionRawMutex, I2c<'static, I2C1, i2c::Async>>;

type Spi0Bus = Mutex<CriticalSectionRawMutex, Spi<'static, SPI0, spi::Async>>;
type Spi1Bus = Mutex<CriticalSectionRawMutex, Spi<'static, SPI1, spi::Async>>;


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
        spi0_rx:  PIN_16,
        spi0_cs0: PIN_17,
        spi0_sck: PIN_18,
        spi0:     SPI0,
        rx_dma0:   DMA_CH0,
        rx_dma1:   DMA_CH1,
    }
    spi1: Spi1Resources {
        spi1_rx:  PIN_8,
        spi1_cs0: PIN_9,
        spi1_sck: PIN_10,
        spi1_tx:  PIN_11,
        spi1:     SPI1,
        rx_dma0:   DMA_CH2,
    }
    hv_breakout: HighVoltageBreakoutResources {
        hv_io1: PIN_20,
        zc_sig: PIN_21,
        hv_io2: PIN_28
    }
    other: OtherResources {
        led: PIN_25,
    }
}

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = embassy_rp::init(Default::default());
    let r = split_resources!(p);

    let i2c0 = I2c::new_async(r.i2c0.peripheral, r.i2c0.scl, r.i2c0.sda, I2c0Irqs, Config::default());
    let i2c1 = I2c::new_async(r.i2c1.peripheral, r.i2c1.scl, r.i2c1.sda, I2c1Irqs, Config::default());

    let spi0_cs0 = Output::new(r.spi0.spi0_cs0, Level::High);
    let mut spi0_config = spi::Config::default();
    spi0_config.frequency = 1_000_000;

    let spi0 = Spi::new_rxonly(r.spi0.spi0, r.spi0.spi0_sck, r.spi0.spi0_rx, r.spi0.rx_dma0, r.spi0.rx_dma1, spi0_config);

    static I2C0_BUS: StaticCell<I2c0Bus> = StaticCell::new();
    static I2C1_BUS: StaticCell<I2c1Bus> = StaticCell::new();

    static SPI0_BUS: StaticCell<Spi0Bus> = StaticCell::new();
    static SPI1_BUS: StaticCell<Spi1Bus> = StaticCell::new();

    let i2c0_bus = I2C0_BUS.init(I2c0Bus::new(i2c0));
    let i2c1_bus = I2C1_BUS.init(I2c1Bus::new(i2c1));

    let spi0_bus = SPI0_BUS.init(Spi0Bus::new(spi0));

    // High-priority executor: SWI_IRQ_1, priority level 2
    interrupt::SWI_IRQ_1.set_priority(Priority::P2);
    let spawner = EXECUTOR_HIGH.start(interrupt::SWI_IRQ_1);
    unwrap!(spawner.spawn(temp(spi0_cs0, spi0_bus)));

    // Medium-priority executor: SWI_IRQ_0, priority level 3
    interrupt::SWI_IRQ_0.set_priority(Priority::P3);
    let spawner = EXECUTOR_MED.start(interrupt::SWI_IRQ_0);
    unwrap!(spawner.spawn(read_ads(i2c1_bus)));

    // Low priority executor: runs in thread mode, using WFE/SEV
    let executor = EXECUTOR_LOW.init(Executor::new());
    executor.run(|spawner| {
        unwrap!(spawner.spawn(blink(r.other)));
    });
}

#[embassy_executor::task]
async fn temp(cs: Output<'static>, spi_bus: &'static Spi0Bus) {
    let device = SpiDevice::new(spi_bus, cs);

    let mut max = drivers::max31855::Max31855::new(device);

    loop {
        let res = max.read_raw().await;
        match res {
            Ok(value) => {
                //info!("Success");
                //info!("Raw temp: {:?}", value.get_temp());
                //info!("Raw value: {:?}", value.0)
            },
            Err(e) => {
                error!("Error or something :(")
            }
        }
        Timer::after_millis(250).await;
    }
}

#[embassy_executor::task]
async fn blink(other: OtherResources) {
    let mut led = Output::new(other.led, Level::Low);

    loop {
        //info!("led on");
        led.set_high();
        Timer::after_secs(1).await;

        //info!("led off");
        led.set_low();
        Timer::after_secs(1).await;
    }
}

#[embassy_executor::task]
async fn read_ads(bus: &'static I2c1Bus) {
    let mut pca_i2c_device = I2cDevice::new(bus);
    let mut pca = drivers::pca9544a::Pca9544a::new(&mut pca_i2c_device, 0b111_0000);

    let ads_i2c = pca.create_device(Channel::Channel1);

    let mut ads = Ads1119::new(ads_i2c, 0b100_0000);

    let mut setup: bool = false;

    loop {
        Timer::after_secs(1).await;
        if setup == false {
            let mut config = ads1119::ConfigRegister(0);
            config.set_mux(MuxConfig::AIN0_AGND);
            config.set_vref(VoltageReference::External);
            config.set_conversion_mode(ConversionMode::Continuous);
            ads.configure(config).await.unwrap();
            ads.start_conversion().await.unwrap();
            setup = true;
        }
        
        pca.set_channel(Channel::Channel3).await.unwrap();
        
        Timer::after_millis(500).await;

        let value = ads.read_data().await.unwrap();

        info!("Ads Data: {:?}", value);
    }
}
