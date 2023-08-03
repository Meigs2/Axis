use byte_slice_cast::AsByteSlice;
use crate::{Message, MAX_STRING_SIZE};
use defmt::{debug, error};
use embassy_futures::select::{Either, Select, select};
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::Driver;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::{Builder, UsbDevice};
use heapless::{String, Vec};
use serde_json_core::de::Error;
use serde_json_core::{from_str, to_string};
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

#[embassy_executor::task]
pub async fn run(mut usb: UsbDevice<'static, Driver<'static, USB>>, usb_sender: &'static mut embassy_usb::class::cdc_acm::Sender<'static, Driver<'static, USB>>, usb_receiver: &'static mut embassy_usb::class::cdc_acm::Receiver<'static, Driver<'static, USB>>, sender: Sender<'static, CriticalSectionRawMutex, Message, 1>, receiver: &'static Receiver<'static, CriticalSectionRawMutex, Message, 1>) {
    let receive = async move {
        loop {
            let m = receiver.recv().await;

            let mut a: String<MAX_STRING_SIZE> = to_string(&m).unwrap();
            a.push_str("\r\n").unwrap();

            debug!("Outbound message: {:?}", a);
            let timeout = Timer::after(Duration::from_millis(1));
            select(usb_sender.write_packet(a.as_byte_slice()), timeout).await;
        }
    };

    loop {
        embassy_futures::select::select3(
            usb.run(),
            receive,
            write_usb(receiver, usb_sender),
        )
            .await;
        usb.disable();
    }
}

async fn write_usb(
    outbound_receiver: &'static Receiver<'static, CriticalSectionRawMutex, Message, 1>,
    usb_sender: &'static mut embassy_usb::class::cdc_acm::Sender<'static, Driver<'static, USB>>,
) {
}

async fn receive<'a>(
    usb_receiver: &'a mut embassy_usb::class::cdc_acm::Receiver<'a, Driver<'a, USB>>,
    sender: Sender<'a, CriticalSectionRawMutex, Message, 1>,
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
}
