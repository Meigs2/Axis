use bitfield::bitfield;
use bitflags::bitflags;
use embedded_hal_async::i2c::I2c;
use num_traits::ToBytes;

pub enum Error<E> {
    I2cError(E)
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum CommandBytes {
    InputPort = 0x00,
    OutputPort = 0x01,
    Polarity = 0x02,
    Config = 0x03,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct RegisterValues: u8 {
        const I0 = 0b0000_0001;
        const I1 = 0b0000_0010;
        const I2 = 0b0000_0100;
        const I3 = 0b0000_1000;
        const I5 = 0b0001_0000;
        const I6 = 0b0010_0000;
        const I7 = 0b0100_0000;
        const I8 = 0b1000_0000;
    }
}

pub struct Tca9534<I2C: I2c> {
    i2c: I2C,
    address: u8
}

impl<I2C: I2c> Tca9534<I2C> {
    pub fn new(i2c: I2C, address: u8) -> Self {
        Self {
            i2c,
            address
        }
    }

    async fn get_register(&mut self, command: CommandBytes) -> Result<RegisterValues, I2C::Error> {
        let buf = &mut [0u8; 1];
        self.i2c.write_read(self.address, &(command as u8).to_be_bytes(), buf).await?;
        
        Ok(RegisterValues::from_bits(u8::from_be_bytes(*buf)).unwrap())
    }

    pub async fn get_inputs(&mut self) -> Result<RegisterValues, I2C::Error> {
        self.get_register(CommandBytes::InputPort).await
    }

    pub async fn get_outputs(&mut self) -> Result<RegisterValues, I2C::Error> {
        self.get_register(CommandBytes::OutputPort).await
    }

    pub async fn get_polarity(&mut self) -> Result<RegisterValues, I2C::Error> {
        self.get_register(CommandBytes::Polarity).await
    }

    pub async fn get_config(&mut self) -> Result<RegisterValues, I2C::Error> {
        self.get_register(CommandBytes::Config).await
    }

    async fn set_register(&mut self, command: CommandBytes, register: RegisterValues) -> Result<(), I2C::Error> {
        let data_buf = &mut [command as u8, register.bits()];
        self.i2c.write(self.address, data_buf).await?;
        
        Ok(())
    }

    pub async fn set_outputs(&mut self, register: RegisterValues) -> Result<(), I2C::Error> {
        self.set_register(CommandBytes::OutputPort, register).await
    }

    pub async fn set_polarity(&mut self, register: RegisterValues) -> Result<(), I2C::Error> {
        self.set_register(CommandBytes::Polarity, register).await
    }

    pub async fn set_config(&mut self, register: RegisterValues) -> Result<(), I2C::Error> {
        self.set_register(CommandBytes::Config, register).await
    }
}