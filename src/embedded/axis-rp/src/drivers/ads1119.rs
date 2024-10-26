use bitfield::bitfield;
use defmt::info;
use embedded_hal_async::i2c::I2c;

#[derive(Clone, Copy, Debug)]
pub enum Error<E> {
    I2cError(E),
}

pub struct Ads1119<I2C: I2c> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Ads1119<I2C> {
    pub fn new(i2c: I2C, address: u8) -> Self {
        Self {
            i2c,
            address,
        }
    }

    pub async fn reset(&mut self) -> Result<(), Error<I2C::Error>> {
        const RESET: u8 = 0b0000_0110;
        self.i2c.write(self.address, &RESET.to_be_bytes()[..]).await.map_err(Error::I2cError)
    }
    
    pub async fn read_data(&mut self) -> Result<i16, I2C::Error> {
        const RDATA: u8 = 0b0001_0000;
        let buf = &mut [0u8; 2];
        self.i2c.write(self.address, &[RDATA]).await?;
        self.i2c.read(self.address, buf).await?;
        info!("{:?}", buf);
        Ok(i16::from_be_bytes(*buf))
    }

    pub async fn configure(&mut self, config: ConfigRegister) -> Result<(), I2C::Error> {
        self.i2c.write(self.address.clone(), &[0b0100_0000, config.0]).await
    }
    
    pub async fn start_conversion(&mut self) -> Result<(), I2C::Error> {
        self.i2c.write(self.address.clone(), &[0b0000_1000]).await
    }

}

bitfield! {
    pub struct ConfigRegister(u8);
    impl Debug;
    u8;
    mux, _set_mux : 7, 5;
    gain, _set_gain: 4, 4;
    data_rate, _set_data_rate: 3, 2;
    conversion_mode, _set_conversion_mode: 1, 1;
    vref, _set_vref: 0, 0;
}

impl ConfigRegister {
    pub fn new() -> Self {
        ConfigRegister(0) // Default value is 0x00
    }

    pub fn set_mux(&mut self, mux_config: MuxConfig) {
        self._set_mux(mux_config as u8);
    }

    pub fn set_gain(&mut self, gain: GainSetting) {
        self._set_gain(gain as u8)
    }
    
    pub fn set_data_rate(&mut self, data_rate: DataRate) {
        self._set_data_rate(data_rate as u8)
    }
    
    pub fn set_conversion_mode(&mut self, conversion_mode: ConversionMode) {
        self._set_conversion_mode(conversion_mode as u8)
    }
    
    pub fn set_vref(&mut self, voltage_reference: VoltageReference) {
        self._set_vref(voltage_reference as u8)
    }
}


#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum MuxConfig {
    AIN0_AIN1  = 0b000,         // 000: AINP = AIN0, AINN = AIN1 (default)
    AIN2_AIN3  = 0b001,         // 001: AINP = AIN2, AINN = AIN3
    AIN1_AIN2  = 0b010,         // 010: AINP = AIN1, AINN = AIN2
    AIN0_AGND  = 0b011,         // 011: AINP = AIN0, AINN = AGND
    AIN1_AGND  = 0b100,         // 100: AINP = AIN1, AINN = AGND
    AIN2_AGND  = 0b101,         // 101: AINP = AIN2, AINN = AGND
    AIN3_AGND  = 0b110,         // 110: AINP = AIN3, AINN = AGND
    AVDD_DIV_2 = 0b111,        // 111: AINP and AINN shorted to AVDD / 2
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum GainSetting {
    _0 = 0,
    _4 = 1,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum DataRate {
    _20SPS   = 0b00,
    _90SPS   = 0b01,
    _330SPS  = 0b10,
    _1000SPS = 0b11,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum ConversionMode {
    SingleShot = 0b0,
    Continuous = 0b1,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum VoltageReference {
    Internal2_048 = 0b0,
    External      = 0b1,
}

bitfield! {
    pub struct StatusRegister(u8);
    impl Debug;
    u8;
    pub drdy, _: 7, 7;
}

