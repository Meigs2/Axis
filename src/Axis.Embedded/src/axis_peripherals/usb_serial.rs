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
use minicbor::decode::Error;
use minicbor::{decode, Decode, Decoder, Encode, Encoder};
use static_cell::{make_static, StaticCell};
use thiserror_no_std::Error;

use crate::{MessageDTO};

type MyDriver<'a> = Driver<'a, USB>;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

impl<'a> Encode<MessageDTO<'a>> for MessageDTO<'a> {
    fn encode<W: minicbor::encode::Write>(
        &self,
        e: &mut minicbor::Encoder<W>,
        ctx: &mut MessageDTO<'a>,
    ) -> Result<(), minicbor::encode::Error<W::Error>> {
        e.u16(self.message_type as u16)?;
        e.u16(self.content_len)?;
        e.bytes(self.content)?;
        e.ok()
    }
}

impl<'a, C> Decode<'a, C> for MessageDTO<'a> {
    fn decode(d: &mut minicbor::Decoder, ctx: &mut C) -> Result<Self, minicbor::decode::Error> {
        let message_type = d.u8()?;
        let content_len = d.u16()?;
        let content = d.bytes()?;

        Ok(MessageDTO {
            message_type: message_type.into(),
            content_len,
            content,
        })
    }
}

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

impl From<decode::Error> for UsbError {
    fn from(value: Error) -> Self {
        match value {
            Error { .. } => UsbError::DecodeError,
        }
    }
}

pub struct UsbInterface {}

impl UsbInterface {
    pub async fn run<'a>(
        class: CdcAcmClass<'a, Driver<'a, USB>>,
        mut usb: UsbDevice<'a, Driver<'a, USB>>,
        inbound_sender: Sender<'a, CriticalSectionRawMutex, MessageDTO<'a>, 1>,
        outbound_receiver: Receiver<'a, CriticalSectionRawMutex, MessageDTO<'a>, 1>,
        outbound_sender: Sender<'a, CriticalSectionRawMutex, MessageDTO<'a>, 1>,
        inbound_receiver: Receiver<'a, CriticalSectionRawMutex, MessageDTO<'a>, 1>,
        message_buffer: &'a mut [u8]
    ) {
    }

    async fn write_usb<'b>(
        usb_sender: &mut embassy_usb::class::cdc_acm::Sender<'b, Driver<'b, USB>>,
        outbound_receiver: Receiver<'b, CriticalSectionRawMutex, MessageDTO<'b>, 1>,
    ) {
    }

    // pub async fn start(&mut self) -> ! {
    //     let (a,b) = self.cdc_adm.split();
    //
    //
    //
    //     let connection_loop = async {
    //         loop {
    //         self.cdc_adm.wait_connection().await;
    //         defmt::info!("Connected");
    //         let _ = self.process_packets().await;
    //         defmt::info!("Disconnected");
    //     }};
    //
    //     let run_loop = self.usb_device.run();
    //
    //     loop {
    //         embassy_futures::join::join(connection_loop, run_loop).await;
    //     }
    // }
    //
    // async fn read(&self, adm: &mut CdcAcmClass<'a, Driver<'a, USB>>) {
    //     loop {
    //         adm.wait_connection().await;
    //         defmt::info!("Connected");
    //         let _ = self.process_packets().await;
    //         defmt::info!("Disconnected");
    //     }
    // }
    //
    // async fn process_packets(&mut self) -> Result<(), UsbError> {
    //     let mut buf = [0; 64];
    //     let decoder = Decoder::new(&buf);
    //     loop {
    //         let n = self.cdc_adm.read_packet(&mut buf).await?;
    //         let data: MessageDTO = minicbor::decode(&buf)?;
    //         let data = &buf[..n];
    //         info!("data: {:x}", data);
    //         self.cdc_adm.write_packet(data).await?;
    //     }
    // }
}
