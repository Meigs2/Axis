use core::future::IntoFuture;
use embassy_rp::bind_interrupts;
use embassy_rp::i2c::{Async, I2c, Instance, Mode};
use embassy_rp::peripherals::{I2C0, I2C1};
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, RawMutex};
use embassy_sync::mutex::{Mutex, MutexGuard};


pub type I2cBus<'a, T> = Mutex<CriticalSectionRawMutex, I2c<'a, T, Async>>;

bind_interrupts!(pub struct I2c0Irqs {
    I2C0_IRQ => embassy_rp::i2c::InterruptHandler<I2C0>;
});

bind_interrupts!(pub struct I2c1Irqs {
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<I2C1>;
});