use bitfield::{bitfield, BitRange};
use defmt::info;
use embassy_rp::spi::Instance;
use embedded_hal_async::spi::SpiDevice;

#[derive(Clone, Copy, Debug)]
pub enum Error<E> {
    SpiError(E),
    FaultDetected(FaultInfo),
}

#[derive(Clone, Copy, Debug)]
pub struct FaultInfo {
    pub short_to_vcc: bool,
    pub short_to_gnd: bool,
    pub open_circuit: bool,
}

pub struct Max31855<SPI: SpiDevice> {
    spi: SPI
}

pub struct TestStruct {

}

impl<SPI: SpiDevice> Max31855<SPI> {
    pub fn new(spi: SPI) -> Self {
        Self {
            spi
        }
    }

    pub async fn read_raw(&mut self) -> Result<MemoryMap<[u8; 4]>, Error<SPI::Error>> {
        let buf = &mut [0u8; 4];
        self.spi.read(buf).await.map_err(Error::SpiError)?;
        #[cfg(target_endian = "little")]
        buf.reverse();
        Ok(MemoryMap(*buf))
    }

    pub async fn read_thcpl_temp(&mut self) -> Result<f32, Error<SPI::Error>> {
        let memory_map = self.read_raw().await?;

        if memory_map.fault() == 0 {
            Ok(memory_map.get_temp())
        } else {
            Err(Error::FaultDetected(FaultInfo {
                short_to_vcc: memory_map.short_to_vcc() > 0,
                short_to_gnd: memory_map.short_to_gnd() > 0,
                open_circuit: memory_map.open_circuit() > 0,
            }))
        }
    }
}

bitfield! {
    #[derive(Clone, Copy, Debug)]
    pub struct MemoryMap([u8]);
    u8;
    thcpl_sign, _: 31, 31;
    u16, thcpl_dat, _: 30, 18;
    pub fault, _: 16, 16;
    in_sign, _: 15, 15;
    in_data, _: 14, 4;
    short_to_vcc, _: 2, 2;
    short_to_gnd, _: 1, 1;
    open_circuit, _: 0, 0;
}

impl<T: AsRef<[u8]>> MemoryMap<T> {
    pub fn get_temp(&self) -> f32 {
        // Convert the 13-bit unsigned data to a signed 16-bit integer
        let data = self.thcpl_dat();
        let is_positive = self.thcpl_sign() == 0;

        // 0b1_1111_1111_1111 is equivalent to 0x1FFF, but more explicit
        let mut temp = (data & 0b1_1111_1111_1111) as i16;

        // Apply the scaling factor (0.25°C per bit)
        let mut float_temp = (temp as f32 * 0.25);

        // Apply the sign
        if !is_positive {
            float_temp = -float_temp;
        }

       float_temp
    }
}

