use core::cell::RefCell;
use embassy_futures::select::{select, Either};
use embassy_rp::gpio::{Input, Output, Pin};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use thiserror_no_std::Error;

#[derive(Error, Debug)]
pub enum DimmerError {
    #[error("The pin did not receive a zero-cross signal.")]
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
    zero_cross: Input<'a, T>,
    output: Output<'a, T>,
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
            zero_cross: zero_cross_pin,
            output: output_pin,
            setting,
            acc,
            signal,
        }
    }

    pub async fn run(&mut self) -> Result<(), DimmerError> {
        self.output.set_low();

        let run_future = async {
            let max = u16::MAX as f32;
            loop {
                if let Either::First(_) = select(
                    Timer::after(Duration::from_millis(500)),
                    self.zero_cross.wait_for_rising_edge(),
                )
                .await
                {
                    self.output.set_low();
                    return Err(DimmerError::NoZeroCross);
                }

                match *self.setting.borrow() {
                    DimmerCommand::Off => {
                        self.output.set_low();
                    }
                    DimmerCommand::PercentOn(p) => {
                        let add = (p * max) as u16;
                        let (val, overflow) = self.acc.borrow().overflowing_add(add);
                        self.acc.replace(val);
                        match overflow {
                            true => {
                                self.output.set_high();
                            }
                            false => {
                                self.output.set_low();
                            }
                        }
                    }
                }
            }
        };

        match select(Self::read_command(self.setting, self.signal), run_future).await {
            Either::First(res) => res,
            Either::Second(res) => res,
        }
    }

    async fn read_command(
        state: &RefCell<DimmerCommand>,
        signal: &Signal<CriticalSectionRawMutex, DimmerCommand>,
    ) -> Result<(), DimmerError> {
        loop {
            let a = signal.wait().await;
            state.replace(a);
        }
    }
}
