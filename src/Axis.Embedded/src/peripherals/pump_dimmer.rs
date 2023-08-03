use core::cell::RefCell;
use embassy_futures::select::{select, Either};
use embassy_rp::gpio::{Input, Output, Pin};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Receiver;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use thiserror_no_std::Error;

#[derive(Error, Debug)]
pub enum DimmerError {
    #[error("The pin did not receive a zero-cross signal within ")]
    NoZeroCross,
}

#[derive(Debug, Clone, Copy)]
pub enum DimmerCommand {
    Off,
    PercentOn(f32),
}

pub struct ZeroCrossDimmer<'a, T>
where
    T: Pin,
{
    zero_cross_pin: Input<'a, T>,
    output_pin: Output<'a, T>,
    setting: &'a RefCell<DimmerCommand>,
    acc: &'a RefCell<u16>,
    pub signal: &'a Signal<CriticalSectionRawMutex, DimmerCommand>,
}

impl<'a, T> ZeroCrossDimmer<'a, T>
where
    T: Pin,
{
    pub fn new(
        zero_cross_pin: Input<'a, T>,
        output_pin: Output<'a, T>,
        setting: &'a RefCell<DimmerCommand>,
        acc: &'a RefCell<u16>,
        signal: &'a Signal<CriticalSectionRawMutex, DimmerCommand>,
    ) -> ZeroCrossDimmer<'a, T> {
        Self {
            zero_cross_pin,
            output_pin,
            setting,
            acc,
            signal,
        }
    }

    pub async fn run(&mut self) -> Result<(), DimmerError> {
        self.output_pin.set_low();
        let max = u16::MAX as f32;
        loop {
            if let Either::First(_) = select(
                Timer::after(Duration::from_millis(500)),
                self.zero_cross_pin.wait_for_rising_edge(),
            )
            .await
            {
                self.output_pin.set_low();
                return Err(DimmerError::NoZeroCross);
            }

            match self.setting.borrow().clone() {
                DimmerCommand::Off => {
                    self.output_pin.set_low();
                }
                DimmerCommand::PercentOn(p) => {
                    let add = (p * max) as u16;
                    let (val, overflow) = self.acc.borrow().overflowing_add(add);
                    self.acc.replace(val);
                    match overflow {
                        true => {
                            self.output_pin.set_high();
                        }
                        false => {
                            self.output_pin.set_low();
                        }
                    }
                }
            }
        }
    }

    async fn read_command(&mut self) {
        loop {
            let a = self.signal.wait().await;
            self.setting.replace(a);
        }
    }
}
