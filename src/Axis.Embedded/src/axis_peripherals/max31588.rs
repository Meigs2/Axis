use embassy_rp::gpio::Output;
use embassy_rp::peripherals::{PIN_11, PIN_24, PIN_27, SPI1};
use embassy_rp::spi::{Async, Spi};
use crate::axis_peripherals::max31588::ThermocoupleError::FailedRead;

const CLOCK_FRQ: u32 = 500_000;

pub struct ThermocoupleState {
    pub temperature: f32,
}

pub enum ThermocoupleError {
    FailedRead,
    ParseError,
}

pub fn parse(buff: &[u8]) -> Result<ThermocoupleState, ThermocoupleError> {
    Ok(ThermocoupleState { temperature: 50.0 })
}

pub struct MAX31855 {
    spi: &'static mut Spi<'static, SPI1, Async>,
    dc: Output<'static, PIN_11>,
}

impl MAX31855 {
    pub fn new(spi: &'static mut Spi<'static, SPI1, Async>, dc: Output<'static, PIN_11>) -> Self {
        Self { spi, dc }
    }

    pub async fn read(&mut self) -> Result<ThermocoupleState, ThermocoupleError> {
        let mut buf = [0u8; 4];
        self.dc.set_high();

        if (self.spi.read(buf.as_mut()).await).is_err() {
            defmt::debug!("MAX31855 read error.");
            return Err(FailedRead);
        }

        if let Ok(s) = parse(&buf) {
            self.dc.set_low();
            return Ok(s);
        };

        return Err(FailedRead);
    }
}
