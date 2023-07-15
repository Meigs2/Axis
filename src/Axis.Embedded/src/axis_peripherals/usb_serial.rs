use alloc::borrow::ToOwned;
use byte_slice_cast::AsMutByteSlice;
use core::fmt::{Debug, Display, Formatter};
use core::future::Future;
use core::ops::Deref;
use defmt::info;
use embassy_futures::join::{join, join3, join4};
use embassy_rp::bind_interrupts;
use embassy_rp::pac::xip_ctrl::regs::Stat;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, Instance, InterruptHandler};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Receiver, Sender, TryRecvError};
use embassy_sync::pipe::Pipe;
use embassy_sync::pubsub::WaitResult::Message;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use embassy_usb::{Builder, Config, UsbDevice};
use futures::StreamExt;
use lorawan::parser::AsPhyPayloadBytes;
use static_cell::{make_static, StaticCell};
use thiserror_no_std::Error;

use crate::{MessageDTO};

type MyDriver<'a> = Driver<'a, USB>;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

#[derive(Error, Debug)]
pub enum UsbError {
    #[error("the USB was disconnected.")]
    Disconnected,
    #[error("the usb endpoint is disabled")]
    Disabled,
    #[error("data store disconnected")]
    EmptyChannel,
    #[error("error decoding message")]
    DecodeError,
}

impl From<EndpointError> for UsbError {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => UsbError::Disabled,
        }
    }
}

impl From<TryRecvError> for UsbError {
    fn from(value: TryRecvError) -> Self {
        match value {
            TryRecvError::Empty => UsbError::EmptyChannel,
        }
    }
}
