use bitfield::bitfield;
use cortex_m::prelude::_embedded_hal_blocking_i2c_Write;
use defmt::debug;
use embassy_rp::i2c::{Async, Error};
use embassy_rp::peripherals::I2C1;
use {defmt_rtt as _, panic_probe as _};
use static_cell::make_static;

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

impl ConfigRegister {
    pub fn initialize() -> ConfigRegister {
        let mut cfg = ConfigRegister(0);

        cfg.set_mode(0b0);

        cfg
    }
}


#[repr(u16)]
#[derive(Debug, Clone, Copy)]
enum Registers {
    Conversion = 0x00,
    Config = 0x01,
    LoThresh = 0x02,
    HiThresh = 0x03,
}

enum Addresses {
    Write = 0b01001000,
    Read = 0b10010001,
}

impl Addresses {
    pub fn write_config(cfg: ConfigRegister) -> [u8; 3] {
        let mut result = [0u8; 3];
        result[0] = Registers::Config as u8;

        result[2..3].copy_from_slice(&cfg.0.to_le_bytes()[1..2]);
        debug!("{:?}", result);
        result
    }
}

pub struct Ads1115 {
    i2c: embassy_rp::i2c::I2c<'static, I2C1, Async>
}

impl Ads1115 {
    pub fn new(i2c: embassy_rp::i2c::I2c<'static, I2C1, Async>) -> Self {
        Self {
            i2c
        }
    }

    pub fn initialize(&mut self) -> Result<(), Error> {
        self.i2c.write(Addresses::Write as u8, Addresses::write_config(ConfigRegister::initialize()).as_slice()).unwrap();
        Ok(())
    }

    pub fn read(&mut self) -> Result<u16, Error> {
        let buff = &mut [0u8; 2];
        let mut s = [0u8; 1];
        self.i2c.blocking_write(Addresses::Write as u8, s.as_slice()).unwrap();
        self.i2c.blocking_read(Addresses::Write as u8, buff).unwrap();
        Ok(u16::from_le_bytes(*buff))
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