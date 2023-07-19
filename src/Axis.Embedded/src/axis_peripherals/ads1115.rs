use bitfield::bitfield;
use core::ops::RangeInclusive;
use embassy_rp::i2c::{Async};
use embassy_rp::peripherals::I2C1;
use {defmt_rtt as _, panic_probe as _};
use crate::axis_peripherals::ads1115::Addresses::I2CDefault;
use embedded_hal_async::i2c::I2c;

bitfield! {
    // Define a new type `ConfigRegister` with base type u16 (as the ADS1115 config register is 16 bits)
    pub struct ConfigRegister(u16);
    impl Debug;

    u8, get_os, set_os: 15, 15;
    u8, get_mux, set_mux: 14, 12;
    u8, get_pga, set_pga: 11, 9;
    u8, get_mode, set_mode: 8, 8;
    u8, get_dr, set_dr: 7, 5;
    u8, get_comp_mode, set_comp_mode: 4, 4;
    u8, get_comp_pol, set_comp_pol: 3, 3;
    u8, get_comp_lat, set_comp_lat: 2, 2;
    u8, get_comp_que, set_comp_que: 1, 0;
}

enum Addresses {
    I2CWrite = 0b1001000_0,
    I2CRead = 0b1001000_1
}

pub struct Ads1115 {
    i2c: I2c<'static>
}

impl Ads1115 {
    pub fn new(i2c: I2c<>) -> Self {
        Self {
            i2c
        }
    }

    pub fn initialize(&self) {
        self.i2c.write_async(I2CDefault, I2CDefault
    }
}


#[derive(Debug, Clone, Copy, Default)]
pub struct Config {
}


pub struct AddressField {
    value: u8
}

pub struct ConversionRegister {

}