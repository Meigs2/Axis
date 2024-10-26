use core::ops;
use core::ops::{Add, AddAssign};
use bitfield::bitfield;
use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
use embedded_hal_async::i2c::I2c;

const ADDR: u8 = 0b1010;

pub struct Fm24cl16b<I2C: I2c> {
    pub memory_address: MemoryAddress,
    i2c: I2C,
}

pub enum Error<E> {
    I2cError(E),
    InvalidBufferSize,
}

impl<I2C: I2c> Fm24cl16b<I2C> {
    pub fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            memory_address: MemoryAddress(0)
        }
    }
    
    
    /// Reads data from the FM24CL16B using the IC's internal address.
    /// 
    /// # Arguments 
    /// 
    /// * `buf`: Byte buffer to read data into.
    /// 
    /// returns: Result<(), Error<<I2C as ErrorType>::Error>> 
    pub async fn read_current(&mut self, buf: &mut [u8]) -> Result<(), Error<I2C::Error>> {
        let address = Self::create_address(&self.memory_address);
        self.read(address, buf).await
    }

    /// Reads data from the FM24CL16B starting at the specified address.
    /// 
    /// # Arguments 
    /// 
    /// * `new_address`: The new address to begin reading from.
    /// * `buf`: The buffer to load data into. Data is not corrected for endianness.
    /// 
    /// returns: Result<(), Error<<I2C as ErrorType>::Error>> 
    pub async fn read_random(&mut self, new_address: MemoryAddress, buf: &mut [u8]) -> Result<(), Error<I2C::Error>> {
        let address = Self::create_address(&new_address);
        let word = Self::create_word(&new_address);
        
        self.i2c.write(address, &[word]).await.map_err(Error::I2cError)?;
        self.memory_address = new_address;
        // Call self.read to update internal address value
        self.read(address, buf).await
    }

    async fn read(&mut self, address: u8, buf: &mut [u8]) -> Result<(), Error<I2C::Error>> {
        if buf.len() > ((MemoryAddress::MAX + 1) as usize) {
            return Err(Error::InvalidBufferSize);
        }
        self.i2c.read(address, buf).await.map_err(Error::I2cError)?;
        self.memory_address += buf.len() as u16;
        Ok(())
    }

    fn create_address(address: &MemoryAddress) -> u8 {
        let page = (address.0 >> 8 & 0b111) as u8;
        (ADDR << 3) | page
    }

    fn create_word(address: &MemoryAddress) -> u8 {
        (address.0 & 0xFF) as u8
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MemoryAddress(u16);

impl MemoryAddress {
    pub const MAX: u16 = 2047;

    pub fn new(value: u16) -> Result<MemoryAddress, &'static str> {
        match value {
            v if v > MemoryAddress::MAX => Err("value exceeds maximum of 2047"),
            v => Ok(Self(v))
        }
    }
}

impl From<MemoryAddress> for u16 {
    fn from(value: MemoryAddress) -> u16 {
        value.0
    }
}

// Allow creation from u16 with error handling
impl TryFrom<u16> for MemoryAddress {
    type Error = &'static str;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl Add<u16> for MemoryAddress {
    type Output = Self;

    fn add(self, rhs: u16) -> Self::Output {
        // Calculate new value with wrapping
        let new_value = (self.0.wrapping_add(rhs)) % MemoryAddress::MAX;
        // Since we know this will always be valid (less than MAX), we can unwrap
        Self::new(new_value).unwrap()
    }
}

impl AddAssign<u16> for MemoryAddress {
    fn add_assign(&mut self, rhs: u16) {
        let value = self.add(rhs);
        self.0 = value.0;
    }
}

