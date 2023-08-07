use core::cell::RefCell;
use defmt::{debug, error, Format};
use embassy_futures::select::{select, Either};
use embassy_rp::gpio::{Input, Output, Pin};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver};

use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use thiserror_no_std::Error;

#[derive(Error, Debug)]
pub enum DimmerError {
    #[error("The pin did not receive a zero-cross signal.")]
    NoZeroCross,
}

#[derive(Debug, Clone, Copy, Format)]
pub enum DimmerCommand {
    Off,
    PercentOn(f32),
}

pub struct ZeroCrossDimmer<'a, Tzc, To>
where
    Tzc: Pin,
    To: Pin
{
    zero_cross: Input<'a, Tzc>,
    output: Output<'a, To>,
    setting: RefCell<DimmerCommand>,
    acc: RefCell<u16>,
    pub signal: &'a Channel<CriticalSectionRawMutex, DimmerCommand, 1>,
}

impl<'a, Tzc, To> ZeroCrossDimmer<'a, Tzc, To>
    where
        Tzc: Pin,
        To: Pin
{
    pub fn new(
        zero_cross_pin: Input<'a, Tzc>,
        output_pin: Output<'a, To>,
        signal: &'a Channel<CriticalSectionRawMutex, DimmerCommand, 1>,
    ) -> ZeroCrossDimmer<'a, Tzc, To> {
        Self {
            zero_cross: zero_cross_pin,
            output: output_pin,
            setting: RefCell::new(DimmerCommand::Off),
            acc: RefCell::new(0),
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
                    error!("No Zero Cross Detected.");
                    return Err(DimmerError::NoZeroCross);
                }

                match *self.setting.borrow() {
                    DimmerCommand::Off => {
                        debug!("Setting: Off");
                        self.output.set_low();
                    }
                    DimmerCommand::PercentOn(p) => {
                        debug!("Setting Percent On: {:?}", p);
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

        match select(Self::read_command(&self.setting, self.signal.receiver().clone()), run_future).await {
            Either::First(res) => res,
            Either::Second(res) => res,
        }
    }

    async fn read_command<'b>(
        state: &RefCell<DimmerCommand>,
        signal: Receiver<'b, CriticalSectionRawMutex, DimmerCommand, 1>,
    ) -> Result<(), DimmerError> {
        loop {
            let a = signal.recv().await;
            debug!("Setting new setting value: {:?}", a);
            state.replace(a);
        }
    }
}
