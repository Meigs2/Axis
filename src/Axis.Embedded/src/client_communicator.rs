use crate::{Message, MAX_STRING_SIZE};
use byte_slice_cast::AsByteSlice;
use core::cell::{RefCell, RefMut};

use defmt::{debug, error};
use embassy_futures::select::select;
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
use static_cell::{make_static, StaticCell};

pub const MAX_PACKET_SIZE: usize = 64;

bind_interrupts!(pub struct Irqs {
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
});

pub struct ClientCommunicator<'a, const N: usize> {
    usb: UsbDevice<'a, Driver<'a, USB>>,
    usb_sender: embassy_usb::class::cdc_acm::Sender<'a, Driver<'a, USB>>,
    usb_receiver: embassy_usb::class::cdc_acm::Receiver<'a, Driver<'a, USB>>,
    channel: Channel<CriticalSectionRawMutex, Message, N>
}

impl<'a, const N: usize> ClientCommunicator<'a, N> {
    pub fn new(usb: USB,
    ) -> Self {
        // Create the driver, from the HAL.
        let driver = Driver::new(usb, Irqs);

        let mut config = embassy_usb::Config::new(0xc0de, 0xcafe);
        config.manufacturer = Some("Embassy");
        config.product = Some("USB-serial logger");
        config.serial_number = Some("12345678");
        config.max_power = 100;
        config.max_packet_size_0 = MAX_PACKET_SIZE as u8;

        // Required for windows compatiblity.
        // https://developer.nordicsemi.com/nRF_Connect_SDK/doc/1.9.1/kconfig/CONFIG_CDC_ACM_IAD.html#help
        config.device_class = 0xEF;
        config.device_sub_class = 0x02;
        config.device_protocol = 0x01;
        config.composite_with_iads = true;

        let mut device_descriptor = [0; 256];
        let mut config_descriptor = [0; 256];
        let mut bos_descriptor = [0; 256];
        let mut control_buf = [0; 64];
        let mut state = State::new();

        let mut builder = Builder::new(
            driver,
            config,
            &mut device_descriptor,
            &mut config_descriptor,
            &mut bos_descriptor,
            &mut control_buf,
        );

        // Create classes on the builder.
        let class = CdcAcmClass::new(&mut builder, &mut state, MAX_PACKET_SIZE as u16);

        let usb = builder.build();

        let (sender, receiver) = class.split();

        let channel: Channel<CriticalSectionRawMutex, Message, N> = Channel::new();
        Self {
            usb,
            usb_sender: sender,
            usb_receiver: receiver,
            channel,
        }
    }

    pub async fn run(&'a mut self) {
        loop {
            embassy_futures::select::select3(
                self.usb.run(),
                Self::receive_incoming_packets(self.usb_receiver, self.channel.sender().clone()),
                Self::write_outgoing_packets(self.usb_sender, self.channel.receiver().clone()),
            )
            .await;
            self.usb.disable().await;
        }
    }

    pub fn get_receiver(&self) -> Receiver<'a, CriticalSectionRawMutex, Message, N> {
        self.channel.receiver().clone()
    }

    pub fn get_sender(&self) -> Sender<'a, CriticalSectionRawMutex, Message, N> {
        self.channel.sender().clone()
    }

    async fn write_outgoing_packets(
        mut usb_sender:
            embassy_usb::class::cdc_acm::Sender<'a, Driver<'a, USB>>,
        receiver: Receiver<'a, CriticalSectionRawMutex, Message, N>,
    ) {
        loop {
            let m = receiver.recv().await;

            let mut a: String<MAX_STRING_SIZE> = to_string(&m).unwrap();
            a.push_str("\r\n").unwrap();

            debug!("Outbound message: {:?}", a);
            let timeout = Timer::after(Duration::from_millis(1));
            select(usb_sender.write_packet(a.as_byte_slice()), timeout).await;
        }
    }

    async fn receive_incoming_packets(
        mut usb_receiver:
            embassy_usb::class::cdc_acm::Receiver<'a, Driver<'a, USB>,>,
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
