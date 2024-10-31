use bitfield::bitfield;
use core::marker::PhantomData;
use bitflags::bitflags;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::{Mutex, MutexGuard};
use embedded_hal_1::i2c::{ErrorType, Operation, SevenBitAddress};
use embedded_hal_async::i2c::I2c;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Channel {
    None,
    Channel0,
    Channel1,
    Channel2,
    Channel3
}

bitflags! {
    pub struct InterruptFlags: u8 {
        const INT0 = 0b0001;
        const INT1 = 0b0010;
        const INT2 = 0b0100;
        const INT3 = 0b1000;
    }
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

impl ControlRegister {
    pub fn set_channel(&mut self, channel: Channel) {
        match channel {
            Channel::Channel0 => {
                self.set_b2(1)
            }
            Channel::Channel1 => {
                self.set_b2(1);
                self.set_b0(1);
            }
            Channel::Channel2 => {
                self.set_b2(1);
                self.set_b1(1);
            }
            Channel::Channel3 => {
                self.set_b2(1);
                self.set_b1(1);
                self.set_b0(1);
            }
            Channel::None => {
                self.set_b2(0);
            }
        }
    }

    pub fn get_channel(&self) -> Channel {
        match (self.b2(), self.b1(), self.b0()) {
            (0, _, _) => Channel::None,
            (1, 0, 0) => Channel::Channel0,
            (1, 0, 1) => Channel::Channel1,
            (1, 1, 0) => Channel::Channel2,
            (1, 1, 1) => Channel::Channel3,
            _ => {panic!("A channel bitfield for the pca9544a has a non-binary value somehow.")}
        }
    }
    
    pub fn get_interrupt_flags(&self) -> InterruptFlags {
        InterruptFlags::from_bits_truncate(self.0)
    }
}

pub struct Pca9544a<'a, I2C: I2c> {
    mutex: Mutex<CriticalSectionRawMutex, Pca9544aInner<'a, I2C>>,
}

impl<'a, I2C: I2c> Pca9544a<'a, I2C> {
    pub fn new(i2c: I2C, address: u8) -> Self {
        let inner = Pca9544aInner {
            phantom_data: PhantomData,
            last_selected_channel: Channel::None,
            i2c,
            address
        };

        Pca9544a {
            mutex: Mutex::new(inner)
        }
    }

    pub fn create_device(&'a self, channel: Channel) -> Pca9544aDevice<'a, I2C> {
        Pca9544aDevice {
            mutex: &self.mutex,
            channel,
        }
    }

    pub async fn set_channel(&self, channel: Channel) -> Result<(), I2C::Error> {
        let mut inner = self.mutex.lock().await;
        inner.set_channel(channel).await
    }
    
    async fn read_register(&self) -> Result<ControlRegister, I2C::Error> {
        let mut inner = self.mutex.lock().await;
        let address = inner.address;

        let mut buf = [0u8; 1];
        inner.i2c.read(address, &mut buf).await?;

       Ok(ControlRegister(u8::from_be_bytes(buf)))
    }

    pub async fn get_current_channel(&self) -> Result<Channel, I2C::Error> {
        self.read_register().await.map(|register| register.get_channel())
    }

    pub async fn get_interrupt_flags(&self) -> Result<InterruptFlags, I2C::Error> {
        self.read_register().await.map(|register| register.get_interrupt_flags())
    }
}

pub struct Pca9544aDevice<'a, I2C: I2c> {
    mutex: &'a Mutex<CriticalSectionRawMutex, Pca9544aInner<'a, I2C>>,
    channel: Channel,
}

struct Pca9544aInner<'a, I2C: I2c> {
    i2c: I2C,
    last_selected_channel: Channel,
    address: u8,
    phantom_data: PhantomData<&'a I2C>,
}

impl<'a, I2C: I2c> Pca9544aInner<'a, I2C> {
    async fn set_channel(&mut self, channel: Channel) -> Result<(), I2C::Error> {
        let mut register = ControlRegister(0);

        register.set_channel(channel);

        self.i2c.write(self.address, &[register.0]).await?;
        self.last_selected_channel = channel;
        Ok(())
    }
}

// Implement ErrorType for our device
impl<T: I2c> ErrorType for Pca9544aDevice<'_, T> {
    type Error = T::Error;
}

type InnerGuard<'a, T> = MutexGuard<'a, CriticalSectionRawMutex, T>;

impl<'a, T: I2c> I2c for Pca9544aDevice<'a, T> {
    async fn read(&mut self, address: SevenBitAddress, read: &mut [u8]) -> Result<(), Self::Error> {
        let mut inner = self.mutex.lock().await;
        let mut original = None;

        if inner.last_selected_channel != self.channel {
            original = Some(self.channel);
            inner.set_channel(self.channel).await?;
        }

        inner.i2c.read(address, read).await?;

        if original.is_some() {
            inner.set_channel(original.unwrap()).await?;
        }

        Ok(())
    }

    async fn write(&mut self, address: SevenBitAddress, write: &[u8]) -> Result<(), Self::Error> {
        let mut inner = self.mutex.lock().await;
        let mut original = None;

        if inner.last_selected_channel != self.channel {
            original = Some(self.channel);
            inner.set_channel(self.channel).await?;
        }

        inner.i2c.write(address, write).await?;

        if original.is_some() {
            inner.set_channel(original.unwrap()).await?;
        }

        Ok(())
    }

    async fn write_read(&mut self, address: SevenBitAddress, write: &[u8], read: &mut [u8]) -> Result<(), Self::Error> {
        let mut inner = self.mutex.lock().await;
        let mut original = None;

        if inner.last_selected_channel != self.channel {
            original = Some(self.channel);
            inner.set_channel(self.channel).await?;
        }

        inner.i2c.write_read(address, write, read).await?;

        if original.is_some() {
            inner.set_channel(original.unwrap()).await?;
        }

        Ok(())
    }

    async fn transaction(&mut self, address: SevenBitAddress, operations: &mut [Operation<'_>]) -> Result<(), Self::Error> {
        let mut inner = self.mutex.lock().await;
        let mut original = None;

        if inner.last_selected_channel != self.channel {
            original = Some(self.channel);
            inner.set_channel(self.channel).await?;
        }

        for operation in operations {
            match operation {
                Operation::Read(buf) => {
                    inner.i2c.read(address, buf).await?;
                }
                Operation::Write(buf) => {
                    inner.i2c.write(address, buf).await?
                }
            }
        }

        if original.is_some() {
            inner.set_channel(original.unwrap()).await?;
        }

        Ok(())
    }
}
