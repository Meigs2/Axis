use crate::Message;
use crate::Message::*;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Sender;
use heapless::String;
use thiserror_no_std::Error;

const MAX_OUTBOUND_MESSAGES: usize = 1;

#[derive(Error, Debug)]
pub enum HandleError {
    #[error("Unknown error during runtime state transition: Current State: {0:?}, Command: {1:?}")]
    Unknown(State, Command),
    #[error("An error occurrred while attempting to transition between runtime states, current state: {0:?}, command: {1:?}, error: {2}")]
    Generic(State, Command, String<64>),
}

use HandleError::*;

#[derive(Debug, Copy, Clone)]
pub enum Profile {
    Manual,
    Automatic,
}

#[derive(Debug, Copy, Clone)]
pub enum State {
    Reset,
    Standby,
    Brewing(Profile),
    Steaming,
}

#[derive(Debug, Copy, Clone)]
pub enum Command {
    Initialize,
    StartBrew(Profile),
    Steam,
    Reset,
}

use Command::*;

pub use State::*;

#[derive(Clone)]
pub struct Runtime<'a> {
    outbound_sender: Sender<'a, CriticalSectionRawMutex, Message, MAX_OUTBOUND_MESSAGES>,
    state: State,
}

impl<'a> Runtime<'a> {
    pub fn new(
        outbound_sender: Sender<'a, CriticalSectionRawMutex, Message, MAX_OUTBOUND_MESSAGES>,
    ) -> Self {
        Self {
            outbound_sender,
            state: State::Reset,
        }
    }

    pub fn handle(&mut self, command: Command) -> Result<(), HandleError> {
        match (&self.state, command) {
            (State::Reset, Initialize) => Ok(self.state = Standby),
            (Standby, StartBrew(profile)) => Ok(self.state = Brewing(profile)),
            (Standby, Steam) => Ok(self.state = Steaming),
            current @ (_, _) => Err(Unknown(current.0.clone(), current.1)),
        }
    }

    pub async fn receive(&self, message: Message) {
        match message {
            Ping => {
                self.outbound_sender
                    .send(Pong {
                        value: "Pong!".into(),
                    })
                    .await;
            }
            Pong { .. } => {
                self.outbound_sender.send(Ping).await;
            }
            reading @ ThermocoupleReading { .. } => {
                self.outbound_sender.send(reading).await;
            }
            reading @ AdsReading { .. } => {
                self.outbound_sender.send(reading).await;
            }
            BrewSwitch { .. } => {
                // Nothing for now
            }
        }
    }
}
