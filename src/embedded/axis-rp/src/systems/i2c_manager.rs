use embassy_rp::i2c::{Async, I2c, Instance, Mode};
use embassy_rp::peripherals::{I2C0, I2C1};
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, RawMutex};
use embassy_sync::mutex::{Mutex, MutexGuard};


pub type I2cMutex<'a, T> = Mutex<CriticalSectionRawMutex, I2c<'a, T, Async>>;

pub struct I2cManager<'a> {
    i2c0: I2cMutex<'a, I2C0>
}

pub type Guard<'a, T> = MutexGuard<'a, CriticalSectionRawMutex, I2c<'a, T, Async>>;

impl<'a> I2cManager<'a> {
    pub fn new(i2c0: I2c<'a, I2C0, Async>, i2c1: I2c<'a, I2C1, Async>) -> Self {
        Self {
            i2c0: Mutex::new(i2c0),
        }
    }

    pub async fn get_i2c0<'b, F, Fut, R>(&mut self) -> Guard<I2C0>
    where
        F: FnOnce(&mut I2c<'b, I2C0, Async>) -> Fut,
    {
        return self.i2c0.lock().await;
    }
}

pub struct Test<'a> {
    manager: I2cManager<'a>
}