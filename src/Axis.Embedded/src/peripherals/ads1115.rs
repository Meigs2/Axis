use bitfield::bitfield;
use cortex_m::prelude::_embedded_hal_blocking_i2c_Write;
use defmt::debug;
use embassy_rp::i2c::{Async, Error, Instance};

bitfield! {
    // Define a new type `ConfigRegister` with base type u16 (as the ADS1115 config register is 16 bits)
    #[derive(Clone, Copy)]
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

#[derive(Copy, Clone, Debug)]
pub struct AdsConfig {
    /// minimum voltage output of the sensor
    pub sensor_min_voltage: f32,
    /// maximum voltage output of the sensor
    pub sensor_max_voltage: f32,
    /// minimum sensor reading (e.g. 0 psi)
    pub sensor_min_value: f32,
    /// maximum sensor reading (e.g. 200 psi)
    pub sensor_max_value: f32,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, Default)]
pub enum GainSetting {
    V6_111 = 0b000,
    #[default]
    V4_096 = 0b001,
    V2_048 = 0b010,
    V1_024 = 0b011,
    V0_512 = 0b100,
    V0_256 = 0b101,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, Default)]
pub enum SpsConfig {
    Sps8 = 0b000,
    Sps16 = 0b001,
    Sps32 = 0b010,
    Sps64 = 0b011,
    Sps128 = 0b100,
    Sps250 = 0b101,
    #[default]
    Sps475 = 0b110,
    Sps860 = 0b111,
}

impl ConfigRegister {
    pub fn initialize() -> ConfigRegister {
        let mut cfg = ConfigRegister(0);
        cfg.set_mode(0);
        cfg.set_dr(SpsConfig::default() as u8);
        cfg.set_pga(GainSetting::default() as u8);

        debug!("{:?}", cfg.0);

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

const ADDR: u8 = 0b1001000;

fn write_config(_cfg: ConfigRegister) -> [u8; 3] {
    let mut result = [0u8; 3];
    // I couldn't get the config above to work, so for now we're manual.
    result[0] = 0b00000001;
    result[1] = 0b01000000;
    result[2] = 0b11000000;
    debug!("{:?}", result);
    result
}

pub struct Ads1115<'a, I>
where
    I: Instance,
{
    i2c: embassy_rp::i2c::I2c<'a, I, Async>,
    pub config: AdsConfig,
}
impl<'a, I> Ads1115<'a, I>
where
    I: Instance,
{
    pub fn new(i2c: embassy_rp::i2c::I2c<'static, I, Async>, config: AdsConfig) -> Self {
        Self { i2c, config }
    }

    pub fn initialize(&mut self) -> Result<(), Error> {
        self.i2c
            .write(ADDR, write_config(ConfigRegister::initialize()).as_slice())?;
        Ok(())
    }

    pub fn read(&mut self) -> Result<f32, Error> {
        let mut s = [0u8; 1];
        let buff = &mut [0u8; 2];

        s[0] = 0b00000000;
        self.i2c.blocking_write(ADDR, s.as_slice())?;

        self.i2c.blocking_read(ADDR, buff)?;

        let raw_value = (buff[0] as i16) << 8 | (buff[1] as i16);

        Ok(Self::scale_value(
            raw_value as f32,
            (-32767.0, 32767.0),
            (-6.114, 6.114),
        ))
    }

    pub fn scale_value(input: f32, input_range: (f32, f32), output_range: (f32, f32)) -> f32 {
        let (input_min, input_max) = input_range;
        let (output_min, output_max) = output_range;

        // Ensure the input is within the expected range
        if input < input_min || input > input_max {
            panic!("Input out of expected range");
        }

        // Compute the scale factor between the input and output ranges
        let scale_factor = (output_max - output_min) / (input_max - input_min);

        // Scale the input value to the output range
        (input - input_min) * scale_factor + output_min
    }
}
