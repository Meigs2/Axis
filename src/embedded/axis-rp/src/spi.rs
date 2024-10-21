use embassy_rp::spi::{Async, Instance, Spi};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;

pub type SpiBus<'a, T: Instance> = Mutex<CriticalSectionRawMutex, Spi<'a, T, Async>>;

