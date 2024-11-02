#![no_std]
pub mod events;
pub mod messages;

use bitfield::bitfield;
use serde::{Deserialize, Serialize};

bitfield! {
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    pub struct MessageHeader(u16);
    u8;
    pub get_sequence, set_sequence: 15, 8;
    pub get_message_id, set_message: 7, 0;
}
