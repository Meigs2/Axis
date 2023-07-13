use defmt::info;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::USB;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Receiver, Sender, TryRecvError};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::driver::EndpointError;
use embassy_usb::{Builder, Config, UsbDevice};
use embassy_rp::usb::{Driver, Instance, InterruptHandler};
use static_cell::make_static;

use crate::MessageDTO;

type MyDriver<'a> = Driver<'a, USB>;

bind_interrupts!(struct Irqs {
        USBCTRL_IRQ => InterruptHandler<USB>;
    });

struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

impl From<TryRecvError> for Disconnected {
    fn from(value: TryRecvError) -> Self {
        match value {
            a =>{a.}},
        }
    }
}

struct UsbInterface<'a> {
    cdc_adm: CdcAcmClass<'a, Driver<'a, USB>>,
    usb_device: UsbDevice<'a, MyDriver<'a>>,
    external_to_internal_channel: Sender<'a, CriticalSectionRawMutex, MessageDTO<'a>, 1>,
    internal_to_external_channel: Receiver<'a, CriticalSectionRawMutex, MessageDTO<'a>, 1>,
}

impl<'a> UsbInterface<'a> {
    pub fn new(usb: USB, state: &'a mut State<'a>) -> UsbInterface<'a> {
        // Create the driver, from the HAL.
        let driver = Driver::new(usb, Irqs);

        // Create embassy-usb Config
        let mut config = Config::new(0xc0de, 0xcafe);
        config.manufacturer = Some("Axis");
        config.product = Some("Axis USB Serial Interface");
        config.serial_number = Some("axis-usb");
        config.max_power = 100;
        config.max_packet_size_0 = 64;

        // Required for windows compatibility.
        // https://developer.nordicsemi.com/nRF_Connect_SDK/doc/1.9.1/kconfig/CONFIG_CDC_ACM_IAD.html#help
        config.device_class = 0xEF;
        config.device_sub_class = 0x02;
        config.device_protocol = 0x01;
        config.composite_with_iads = true;

        // Create embassy-usb DeviceBuilder using the driver and config.
        let mut builder = Builder::new(
            driver,
            config,
            &mut make_static!([0; 256])[..],
            &mut make_static!([0; 256])[..],
            &mut make_static!([0; 256])[..],
            &mut make_static!([0; 64])[..],
        );

        // Create classes on the builder.
        let cdc_adm = CdcAcmClass::new(&mut builder, state, 64);

        // Build the builder.
        let usb_device = builder.build();

        todo!();
    }

    pub async fn start(&mut self) -> ! {
        loop {
            self.cdc_adm.wait_connection().await;
            defmt::info!("Connected");
            let _ = self.process_packets().await;
            defmt::info!("Disconnected");
        }
    }

    async fn process_packets(&mut self) -> Result<(), Disconnected> {
        let mut buf = [0; 64];
        loop {
            let a = self.internal_to_external_channel.try_recv()?;

            let n = self.cdc_adm.read_packet(&mut buf).await?;
            let data: MessageDTO = minicbor::decode(&buf);
            let data = &buf[..n];
            info!("data: {:x}", data);
            self.cdc_adm.write_packet(data).await?;
        }
    }
}
