use core::future::Future;

use bitfield::{bitfield, BitMut, BitRange, BitRangeMut};
use bitfield::bitfield_fields;
use embassy_rp::gpio::{Input, Output};
use embassy_rp::i2c::{Async, Error, I2c, Instance};

bitfield! {
    #[derive(Clone, Copy)]
    struct AddressByte(u8);
    fixed_address, _ : 7, 4;
    a2, _, set_a2 : 3;
    a1, _, set_a1 : 2;
    a0, _, set_a0 : 1;
    read_byte, _ : 0;
}

impl AddressByte {
    pub fn with_read(self, value: bool) -> AddressByte { 
        let mut res = self.clone();
        
        res.set_bit(0, value);
        
        return res;
    }
}

bitfield! {
    #[derive(Clone, Copy)]
    struct InputRegister(u8);
    pub p0, _ : 0;
    pub p1, _ : 1;
    pub p2, _ : 2;
    pub p3, _ : 3;
    pub p4, _ : 4;
    pub p5, _ : 5;
    pub p6, _ : 6;
    pub p7, _ : 7;
}

bitfield! {
    #[derive(Clone, Copy)]
    struct OutputRegister(u8);
    pub p0, _ : 0;
    pub p1, _ : 1;
    pub p2, _ : 2;
    pub p3, _ : 3;
    pub p4, _ : 4;
    pub p5, _ : 5;
    pub p6, _ : 6;
    pub p7, _ : 7;
}

bitfield! {
    #[derive(Clone, Copy)]
    struct PolarityInversionRegister(u8);
    pub p0, _ : 0;
    pub p1, _ : 1;
    pub p2, _ : 2;
    pub p3, _ : 3;
    pub p4, _ : 4;
    pub p5, _ : 5;
    pub p6, _ : 6;
    pub p7, _ : 7;
}

bitfield! {
    #[derive(Clone, Copy)]
    struct ConfigurationRegister(u8);
    pub p0, _ : 0;
    pub p1, _ : 1;
    pub p2, _ : 2;
    pub p3, _ : 3;
    pub p4, _ : 4;
    pub p5, _ : 5;
    pub p6, _ : 6;
    pub p7, _ : 7;
}

#[derive(Clone, Copy)]
struct TCA9534Registers {
    pub input_register: InputRegister,
    pub output_register: OutputRegister,
    pub polarity_inversion_register: PolarityInversionRegister,
    pub config_register: ConfigurationRegister 
}

impl TCA9534Registers {
    pub fn to_bytes(self) -> [u8; 4] {
        let mut res = [0u8; 4];
        res[0] = self.input_register.0;
        res[1] = self.output_register.0;
        res[2] = self.polarity_inversion_register.0;
        res[3] = self.config_register.0;
        
        return res;
    }
}

pub struct TCA9534<'a, T> where T: Instance {
    i2c: I2c<'a, T, Async>,
    signal_pin: &'a mut Input<'a>,
    address_byte: AddressByte,
    configuration: TCA9534Registers
}

impl<'a, T> TCA9534<'a, T>
where
    T: Instance,
{
    pub fn new(i2c: I2c<'static, T, Async>, signal_pin: &'a mut Input<'a>) -> Self {
        let mut address_byte = AddressByte(0);
        address_byte.set_bit_range(7, 4, 0b0100);
        
        let configuration = TCA9534Registers { 
            input_register: InputRegister(0),
            output_register: OutputRegister(0xFF),
            polarity_inversion_register: PolarityInversionRegister(0),
            config_register: ConfigurationRegister(0xFF),
        };

        return TCA9534 {
            i2c,
            signal_pin,
            address_byte,
            configuration
        }
    }

    pub async fn wait_signal(&mut self) {
        self.signal_pin.wait_for_falling_edge().await;
    }

    pub async fn read_async(&mut self) -> Result<TCA9534Registers, Error> {
        let mut buf = [0u8; 4];
        let address = self.address_byte.with_read(true);

        self.i2c.read_async(address.0, &mut buf).await?;

        Ok(TCA9534Registers {
            input_register: InputRegister(buf[0]),
            output_register: OutputRegister(buf[1]),
            polarity_inversion_register: PolarityInversionRegister(buf[2]),
            config_register: ConfigurationRegister(buf[3]),
        })
    }
    
    pub async fn write_async() {
        
    }
}