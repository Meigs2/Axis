

use bitfield::bitfield;
use embassy_rp::i2c::{Async, I2c, Instance};

bitfield! {
    struct Address([u8]);
    impl Debug;
    u16;
    get_addr, _: 0, 4;
    get_test, set_test: 5, 15;
}

pub struct fm24cl16b {

}

impl fm24cl16b {
    pub fn read<T: Instance>(bus: I2c<T, Async>, address: u8) {
        let mut test = Address([0; 1]);
        
        test.get
    }
}