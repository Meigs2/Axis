use bitfield::bitfield;
use embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice;
use embassy_rp::i2c::Instance;
use embassy_rp::pac::i2c;
use embedded_hal_async::i2c::I2c;
use crate::I2cBus;

const ADDR: u8 = 0b111_0000;

pub enum Error<E> {
    I2cError(E)
}

#[derive(Clone, Copy, Debug)]
pub enum Channel {
    None,
    Channel0,
    Channel1,
    Channel2,
    Channel3
}

bitfield! {
    #[derive(Clone, Copy, Debug)]
    struct ControlRegister(u8);
    u8;
    int3, _:    7, 7;
    int2, _:    6, 6;
    int1, _:    5, 5;
    int0, _:    4, 4;
    b2, set_b2: 2, 2;
    b1, set_b1: 1, 1;
    b0, set_b0: 0, 0;
}

pub struct Pca9544<'a, I2C: I2c> {
    i2c: &'a mut I2C,
    address: u8,
}

impl<'a, I2C: I2c> Pca9544<'a, I2C> {
    pub fn new(i2c: &'a mut I2C, address: u8) -> Self {
        Self {
            i2c,
            address
        }
    }
    
    pub async fn set_channel(&mut self, channel: Channel) -> Result<(), I2C::Error> {
        let mut register = ControlRegister(0);

        match channel {
            Channel::Channel0 => {
                register.set_b2(1)
            }
            Channel::Channel1 => {
                register.set_b2(1);
                register.set_b0(1);
            }
            Channel::Channel2 => {
                register.set_b2(1);
                register.set_b1(1);
            }
            Channel::Channel3 => {
                register.set_b2(1);
                register.set_b1(1);
                register.set_b0(1);
            }
            Channel::None => {
                register.set_b2(0);
            }
        }
        
        self.i2c.write(self.address, &[register.0]).await?;

        Ok(())
    }
}
