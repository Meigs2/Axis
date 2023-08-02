use crate::{Message, MAX_STRING_SIZE};
use defmt::{debug, error};
use embassy_futures::select::{Either, Select};
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::Driver;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_sync::signal::Signal;
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::{Builder, UsbDevice};
use heapless::Vec;
use serde_json_core::de::Error;
use serde_json_core::from_str;
use static_cell::make_static;

pub const MAX_PACKET_SIZE: usize = 64;

bind_interrupts!(pub struct Irqs {
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
});

pub struct ClientCommunicator<'a, const N: usize> {
    sender: Sender<'a, CriticalSectionRawMutex, Message, N>,
    receiver: Receiver<'a, CriticalSectionRawMutex, Message, N>,
    usb: UsbDevice<'a, Driver<'a, USB>>,
    usb_sender: &'a mut embassy_usb::class::cdc_acm::Sender<'a, Driver<'a, USB>>,
    usb_receiver: &'a mut embassy_usb::class::cdc_acm::Receiver<'a, Driver<'a, USB>>,
}

impl<'a, const N: usize> ClientCommunicator<'a, N> {

    pub fn new(usb: UsbDevice<'a, Driver<'a, USB>>, usb_sender: &'a mut embassy_usb::class::cdc_acm::Sender<'a, Driver<'a, USB>>, usb_receiver: &'a mut embassy_usb::class::cdc_acm::Receiver<'a, Driver<'a, USB>>, sender: Sender<'a, CriticalSectionRawMutex, Message, N>, receiver: Receiver<'a, CriticalSectionRawMutex, Message, N>) -> Self {
        Self {
            sender,
            receiver,
            usb,
            usb_sender,
            usb_receiver,
        }
    }

    pub async fn run(&'a mut self, stop_signal: &mut Signal<CriticalSectionRawMutex, ()>) {
        embassy_futures::select::select3(
            stop_signal.wait(),
            self.usb.run(),
            Self::receive(self.usb_receiver, self.sender.clone()),
        )
        .await;
        self.usb.disable();
    }

    pub fn get_receiver(&self) -> Receiver<'a, CriticalSectionRawMutex, Message, N> {
        self.receiver.clone()
    }

    pub fn get_sender(&self) -> Sender<'a, CriticalSectionRawMutex, Message, N> {
        self.sender.clone()
    }

    pub async fn publish(&self, m: Message) {}

    async fn receive(
        usb_receiver: &'a mut embassy_usb::class::cdc_acm::Receiver<'a, Driver<'a, USB>>,
        sender: Sender<'a, CriticalSectionRawMutex, Message, N>,
    ) {
        let buff = make_static!([0u8; crate::MAX_STRING_SIZE]);
        usb_receiver.wait_connection().await;
        loop {
            match usb_receiver.read_packet(&mut buff[..]).await {
                Ok(s) => {
                    let stopwatch = embassy_time::Instant::now();
                    let string = core::str::from_utf8(&buff[..s]).unwrap();
                    let result: Result<(Vec<Message, MAX_STRING_SIZE>, _), Error> =
                        from_str(string);
                    let msgs = match result {
                        Ok(m) => m.0,
                        Err(_e) => {
                            error!("Error deserializing packet(s).");
                            Vec::new()
                        }
                    };

                    for msg in msgs {
                        sender.send(msg).await;
                    }
                    let a = stopwatch.elapsed().as_micros();
                    debug!("Read/Write Operation Elapsed: {:?} microseconds", a);
                }
                Err(e) => {
                    error!("Error reading packet: {:?}", e)
                }
            }
        }
    }
}
