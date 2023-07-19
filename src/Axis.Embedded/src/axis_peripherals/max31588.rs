//! # max31855
//!
//! Driver for [MAX31855 thermocouple converter](https://www.maximintegrated.com/en/products/sensors/MAX31855.html) using traits from `embedded-hal`.
//!
//! ## Features
//! * Read thermocouple temperature
//! * Read internal reference junction temperature
//! * Read fault data (missing thermocouple, short to ground or short to vcc)
//! * Supports 16-bit (thermocouple + fault only) or 32-bit (thermocouple, internal and full fault details)
//! * Supports Celsius, Fahrenheit or Kelvin units
//! * Supports returning raw (ADC count) readings
//!
//! ## Example:
//! ```
//!     let freq: Hertz = 4.mhz().into();
//!     let mode = Mode {
//!         polarity: Polarity::IdleLow,
//!         phase: Phase::CaptureOnFirstTransition
//!     };
//!
//!     let mut spi = Spi::spi2(
//!         device.SPI2,
//!         (sck_pin, miso_pin, mosi_pin)
//!         mode,
//!         freq,
//!         clocks,
//!         &mut rcc.apb1
//!     );
//!
//!     // Full 32-bit read, result contains both thermocouple and internal temperatures
//!     match spi.read_all(&mut cs_pin, Unit::Celsius) {
//!         Ok(v) => info!("Ok: {:?}", v),
//!         Err(e) => info!("Err: {:?}", e),
//!     }
//!
//!     // Just thermocouple 16-bit read
//!     match spi.read_thermocouple(&mut cs_pin, Unit::Celsius) {
//!         Ok(v) => info!("Ok: {:?}", v),
//!         Err(e) => info!("Err: {:?}", e),
//!     }
//! ```

use embassy_rp::gpio::Output;
use embassy_rp::peripherals::{PIN_11, SPI1};
use embassy_rp::spi::{Async, Spi};

use bit_field::BitField;
use bitfield::bitfield;
use core::ops::RangeInclusive;
use {defmt_rtt as _, panic_probe as _};

use crate::axis_peripherals::max31588::ThermocoupleError::*;
use defmt::Format;
use embedded_hal_1::spi::SpiBus;

bitfield! {
    #[derive(Clone, Copy, Format)]
    pub struct Readout(u32);
    impl Debug;
    impl Copy;

    i16, thermocouple_temp, _ : 31, 18;
    /// Bit reads 1 when there's any fault on the SCV, SCG or OC faults.
    u8, fault, _ : 16;
    i16, internal_temp, _ : 15, 4;
    /// Bit reads 1 when the thermocouple is shorted to Vcc.
    u8, scv_fault, _ : 2;
    /// Bit reads 1 when the thermocouple is shorted to GND.
    u8, scg_fault, _ : 1;
    /// Bit reads 1 when the thermocouple is not connected.
    u8, oc_fault, _ : 0;
}

/// Possible errors returned by this crate
#[derive(Format, Clone, Copy, Debug)]
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
#[derive(Format, Debug, Clone, Copy)]
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
#[derive(Format, Debug, Clone, Copy)]
pub struct FullResult {
    /// The temperature of the thermocouple
    pub thermocouple: f32,
    /// The temperature of the MAX31855 reference junction
    pub internal: f32,
    /// The unit that the temperatures are in
    pub unit: Unit,
}

pub struct MAX31855 {
    spi: &'static mut Spi<'static, SPI1, Async>,
    dc: Output<'static, PIN_11>,
}

impl MAX31855 {
    pub fn new(spi: &'static mut Spi<'static, SPI1, Async>, dc: Output<'static, PIN_11>) -> Self {
        Self { spi, dc }
    }

    /// Reads the thermocouple temperature and leave it as a raw ADC count. Checks if there is a fault but doesn't detect what kind of fault it is
    async fn read_thermocouple_raw(&mut self) -> Result<i16, ThermocoupleError> {
        let mut buffer = [0; 4];

        let readout = self.read_spi(&mut buffer);

        if let Err(_e) = readout {
            return Err(ThermocoupleError::SpiError);
        }

        let readout = Readout(u32::from_le_bytes(buffer));

        if readout.fault() {
            Err(ThermocoupleError::Fault)?
        }

        let thermocouple = readout.thermocouple_temp();

        Ok(thermocouple)
    }

    async fn read_spi<const N: usize>(
        &mut self,
        buffer: &mut [u8; N],
    ) -> Result<(), embassy_rp::spi::Error> {
        self.dc.set_low();
        let readout = self.spi.read(buffer).await;
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

        let readout = Readout(u32::from_le_bytes(buffer));

        if readout.fault() {
            if readout.oc_fault() {
                Err(MissingThermocoupleFault)?
            } else if readout.scg_fault() {
                Err(GroundShortFault)?
            } else if readout.scv_fault() {
                Err(VccShortFault)?
            } else {
                // This should impossible, one of the other fields should be set as well
                // but handled here just-in-case
                Err(Fault)?
            }
        }

        let thermocouple = readout.thermocouple_temp();
        let internal = readout.internal_temp();

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
