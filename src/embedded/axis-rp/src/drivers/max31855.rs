use crate::spi::SpiBus;
use bitfield::{bitfield, BitRange};
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

impl<SPI: SpiDevice> Max31855<SPI> {
    pub async fn read_raw(&mut self) -> Result<MemoryMap, SPI::Error> {
        let mut buf = [0u8; 4];
        self.spi.read(&mut buf).await.map_err(Error::SpiError)?;
        Ok(MemoryMap(buf))
    }

    pub async fn read_thcpl_temp(&mut self) -> Result<i16, Error<SPI::Error>> {
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
    pub struct MemoryMap([u8; 4]);
    u8;
    thcpl_sign, _: 31, 31;
    u16, thcpl_dat, _: 30, 18;
    fault, _: 16, 16;
    in_sign, _: 15, 15;
    in_data, _: 14, 4;
    short_to_vcc, _: 2, 2;
    short_to_gnd, _: 1, 1;
    open_circuit, _: 0, 0;
}

impl MemoryMap {
    pub fn get_temp(&self) -> i16 {
        // Convert the 13-bit unsigned data to a signed 16-bit integer
        // 0b1_1111_1111_1111 is equivalent to 0x1FFF, but more explicit
        let data = self.thcpl_dat();
        let is_positive = self.thcpl_sign() == 0;

        let mut temp = (data & 0b1_1111_1111_1111) as i16;

        // Apply the scaling factor (0.25°C per bit)
        temp = (temp as f32 * 0.25) as i16;

        // Apply the sign
        if !is_positive {
            temp = -temp;
        }

        temp
    }
}

