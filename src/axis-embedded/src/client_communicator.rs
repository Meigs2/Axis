use core::any::Any;
use crate::{MessageType};
use byte_slice_cast::AsByteSlice;

use defmt::{debug, error};
use embassy_futures::select::{select, Either};
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::Driver;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};

use embassy_usb::{Builder, UsbDevice};
use embassy_usb::driver::EndpointError;
use heapless::String;
use static_cell::make_static;
use axis_protocol::MessageHeader;
use axis_protocol::messages::Messages;

pub const MAX_PACKET_SIZE: usize = 64;

bind_interrupts!(pub struct Irqs {
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
});

type UsbSender<'a> = embassy_usb::class::cdc_acm::Sender<'a, Driver<'a, USB>>;
type UsbReceiver<'a> = embassy_usb::class::cdc_acm::Receiver<'a, Driver<'a, USB>>;

pub struct UsbWrapper<'a, const N: usize> {
    inner: Mutex<CriticalSectionRawMutex, UsbWrapperInner<'a, N>>
}

pub struct UsbData<'a> {
    pub device_descriptor: &'a mut [u8; 256],
    pub config_descriptor: &'a mut [u8; 256],
    pub bos_descriptor: &'a mut [u8; 256],
    pub control_buf: &'a mut [u8; 64],
    pub state: &'a mut State<'a>,
}

pub struct UsbWrapperInner<'a, const N: usize> {
    usb_sender: UsbSender<'a>,
    usb_receiver: UsbReceiver<'a>,
    channel: Channel<CriticalSectionRawMutex, Messages, N>,
}

impl<'a, const N: usize> UsbWrapperInner<'a, N> {
    pub fn new<'b>(
        usb: USB,
        data: UsbData<'a>,
    ) -> (Self, UsbDevice<'a, Driver<'a, USB>>) {
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

        let mut builder = Builder::new(
            driver,
            config,
            data.device_descriptor,
            data.config_descriptor,
            data.bos_descriptor,
            data.control_buf,
        );

        // Create classes on the builder.
        let class = CdcAcmClass::new(&mut builder, data.state, MAX_PACKET_SIZE as u16);

        let usb = builder.build();

        let (usb_sender, usb_receiver) = class.split();

        (
            Self {
                usb_sender,
                usb_receiver,
                channel: Channel::new()
            },
            usb,
        )
    }

    pub async fn run(&'a mut self) {
        loop {
            select(
                Self::read(&mut self.usb_receiver, self.channel.sender().clone()),
                Self::write_outgoing_packets(&mut self.usb_sender, self.channel.receiver().clone()),
            ).await;
        }
    }

    async fn write_outgoing_packets<'b>(
        usb_sender: &'b mut embassy_usb::class::cdc_acm::Sender<'a, Driver<'a, USB>>,
        receiver: Receiver<'a, CriticalSectionRawMutex, Messages, N>,
    ) {
        let mut buf = [0u8; N];
        loop {
            let m = receiver.receive().await;

            debug!("Outbound message: {:?}", m);
            let slice = postcard::to_slice(&m, &mut buf);

            let Ok(ser) = slice else {
                debug!("Failed to serialize message: {:?}", m);
                continue;
            };

            let duration = Duration::from_millis(5);
            let timeout = Timer::after(duration);
            match select(usb_sender.write_packet(ser), timeout).await {
                Either::First(_) => {}
                Either::Second(_) => {
                    debug!("Failed to send message over USB, timeout exceeded. Message: {:?}, Timeout: {:?}", m, duration);
                }
            };
        }
    }

    async fn read<'b>(
        usb_receiver: &'b mut UsbReceiver<'a>,
        sender: Sender<'a, CriticalSectionRawMutex, Messages, N>,
    ) {
        let mut buff = [0u8; N];
        debug!("Waiting for USB connection");
        usb_receiver.wait_connection().await;
        debug!("Connected to host");
        loop {
            let res = usb_receiver.read_packet(&mut buff[..]).await;

            let s = match res {
                Ok(v) => v,
                Err(e) => {
                    error!("Error reading packet: {:?}", e);
                    continue;
                }
            };

            debug!("Read data: {:?}", buff);

            let res: postcard::Result<MessageHeader> = postcard::from_bytes(&buff[..3]);

            let Ok(header) = res else {
                error!("Failed to deserialize header: {:?}", &buff[..3]);
                continue;
            };

            let Ok(message) = postcard::from_bytes(&buff[3..s]) else {
                error!("Failed to deserialize header: {:?}", &buff[3..s]);
                continue;
            };

            sender.send(message).await;
        }
    }
}
