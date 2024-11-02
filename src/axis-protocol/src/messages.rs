use serde::{Deserialize, Serialize};
use defmt::Format;

/// Events created by the host (MCU) to send back to the client
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Format)]
#[repr(u8)]
pub enum Messages {
    ThermocoupleReadout { deg_celcius: f32 } = 0,
}
