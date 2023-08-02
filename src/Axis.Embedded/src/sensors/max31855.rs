use embassy_rp::gpio::{Output, Pin};
use embassy_rp::peripherals::{PIN_11, SPI1};
use embassy_rp::spi::{Async, Instance, Spi};

use bit_field::BitField;
use core::ops::RangeInclusive;
use {defmt_rtt as _, panic_probe as _};

use crate::sensors::max31855::ThermocoupleError::*;
use defmt::Format;

/// The bits that represent the thermocouple value when reading the first u16 from the sensor
const THERMOCOUPLE_BITS: RangeInclusive<usize> = 2..=15;
/// The bit that indicates some kind of fault when reading the first u16 from the sensor
const FAULT_BIT: usize = 0;
/// The bits that represent the internal value when reading the second u16 from the sensor
const INTERNAL_BITS: RangeInclusive<usize> = 4..=15;
/// The bit that indicates a short-to-vcc fault when reading the second u16 from the sensor
const FAULT_VCC_SHORT_BIT: usize = 2;
/// The bit that indicates a short-to-gnd fault when reading the second u16 from the sensor
const FAULT_GROUND_SHORT_BIT: usize = 1;
/// The bit that indicates a missing thermocouple fault when reading the second u16 from the sensor
const FAULT_NO_THERMOCOUPLE_BIT: usize = 0;

/// Possible errors returned by this crate
#[derive(Format, Debug)]
pub enum ThermocoupleError {
    /// An error returned by a call to Transfer::transfer
    SpiError,
    /// An error returned by a call to OutputPin::{set_high, set_low}
    ChipSelectError,
    /// The fault bit (16) was set in the response from the MAX31855
    Fault,
    /// The SCV fault bit (2) was set in the response from the MAX31855
    VccShortFault,
    /// The SCG fault bit (1) was set in the response from the MAX31855
    GroundShortFault,
    /// The OC fault bit (0) was set in the response from the MAX31855
    MissingThermocoupleFault,
}

/// The temperature unit to use
#[derive(Format, Clone, Copy, Debug)]
pub enum Unit {
    /// Degrees Celsius
    Celsius,
    /// Degrees Fahrenheit
    Fahrenheit,
    /// Degrees Kelvin
    Kelvin,
}

impl Unit {
    /// Converts degrees celsius into this unit
    pub fn convert(&self, celsius: f32) -> f32 {
        match self {
            Unit::Celsius => celsius,
            Unit::Fahrenheit => celsius * 1.8 + 32.,
            Unit::Kelvin => celsius + 273.15,
        }
    }
}

/// Possible MAX31855 readings
pub enum Reading {
    /// The attached thermocouple
    Thermocouple,
    /// The internal reference junction
    Internal,
}

impl Reading {
    /// Convert the raw ADC count into degrees celsius
    pub fn convert(self, count: i16) -> f32 {
        let count = count as f32;
        match self {
            Reading::Thermocouple => count * 0.25,
            Reading::Internal => count * 0.0625,
        }
    }
}

/// Represents the data contained in a full 32-bit read from the MAX31855 as raw ADC counts
#[derive(Debug)]
pub struct FullResultRaw {
    /// The temperature of the thermocouple as raw ADC counts
    pub thermocouple: i16,
    /// The temperature of the MAX31855 reference junction as raw ADC counts
    pub internal: i16,
}

impl FullResultRaw {
    /// Convert the raw ADC counts into degrees in the provided Unit
    pub fn convert(self, unit: Unit) -> FullResult {
        let thermocouple = unit.convert(Reading::Thermocouple.convert(self.thermocouple));
        let internal = unit.convert(Reading::Internal.convert(self.internal));

        FullResult {
            thermocouple,
            internal,
            unit,
        }
    }
}

/// Represents the data contained in a full 32-bit read from the MAX31855 as degrees in the included Unit
#[derive(Debug)]
pub struct FullResult {
    /// The temperature of the thermocouple
    pub thermocouple: f32,
    /// The temperature of the MAX31855 reference junction
    pub internal: f32,
    /// The unit that the temperatures are in
    pub unit: Unit,
}

const CLOCK_FRQ: u32 = 500_000;

pub struct MAX31855<'a, I, P>
where
    I: Instance,
    P: Pin,
{
    spi: &'a mut Spi<'a, I, Async>,
    dc: Output<'a, P>,
}

impl<'a, I, P> MAX31855<'a, I, P>
where
    I: Instance,
    P: Pin,
{
    pub fn new(spi: &'a mut Spi<'a, I, Async>, dc: Output<'a, P>) -> Self {
        Self { spi, dc }
    }

    /// Reads the thermocouple temperature and leave it as a raw ADC count. Checks if there is a fault but doesn't detect what kind of fault it is
    async fn read_thermocouple_raw(&mut self) -> Result<i16, ThermocoupleError> {
        let mut buffer = [0; 4];

        let readout = self.read_spi(&mut buffer);

        if let Err(_e) = readout {
            return Err(ThermocoupleError::SpiError);
        }

        if buffer[1].get_bit(FAULT_BIT) {
            Err(ThermocoupleError::Fault)?
        }

        let raw = (buffer[0] as u16) << 8 | (buffer[1] as u16);

        let thermocouple = crate::bits_to_i16(raw.get_bits(THERMOCOUPLE_BITS), 14, 4, 2);

        Ok(thermocouple)
    }

    fn read_spi<const N: usize>(
        &mut self,
        buffer: &mut [u8; N],
    ) -> Result<(), embassy_rp::spi::Error> {
        self.dc.set_low();
        let readout = self.spi.blocking_read(buffer);
        self.dc.set_high();
        readout
    }

    /// Reads the thermocouple temperature and converts it into degrees in the provided unit. Checks if there is a fault but doesn't detect what kind of fault it is
    pub async fn read_thermocouple(&mut self, unit: Unit) -> Result<f32, ThermocoupleError> {
        self.read_thermocouple_raw()
            .await
            .map(|r| unit.convert(Reading::Thermocouple.convert(r)))
    }

    /// Reads both the thermocouple and the internal temperatures, leaving them as raw ADC counts and resolves faults to one of vcc short, ground short or missing thermocouple
    async fn read_all_raw(&mut self) -> Result<FullResultRaw, ThermocoupleError> {
        let mut buffer = [0; 4];

        let readout = self.read_spi(&mut buffer);

        if let Err(_e) = readout {
            return Err(ThermocoupleError::Fault);
        }

        let fault = buffer[1].get_bit(0);

        if fault {
            let raw = (buffer[2] as u16) << 8 | (buffer[3] as u16);

            if raw.get_bit(FAULT_NO_THERMOCOUPLE_BIT) {
                Err(MissingThermocoupleFault)?
            } else if raw.get_bit(FAULT_GROUND_SHORT_BIT) {
                Err(GroundShortFault)?
            } else if raw.get_bit(FAULT_VCC_SHORT_BIT) {
                Err(VccShortFault)?
            } else {
                // This should impossible, one of the other fields should be set as well
                // but handled here just-in-case
                Err(Fault)?
            }
        }

        let first_u16 = (buffer[0] as u16) << 8 | (buffer[1] as u16);
        let second_u16 = (buffer[2] as u16) << 8 | (buffer[3] as u16);

        let thermocouple = crate::bits_to_i16(first_u16.get_bits(THERMOCOUPLE_BITS), 14, 4, 2);
        let internal = crate::bits_to_i16(second_u16.get_bits(INTERNAL_BITS), 12, 16, 4);

        Ok(FullResultRaw {
            thermocouple,
            internal,
        })
    }

    /// Reads both the thermocouple and the internal temperatures, converts them into degrees in the provided unit and resolves faults to one of vcc short, ground short or missing thermocouple
    pub async fn read_all(&mut self, unit: Unit) -> Result<FullResult, ThermocoupleError> {
        self.read_all_raw().await.map(|r| r.convert(unit))
    }
}
