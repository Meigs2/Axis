#![no_std]
#![no_main]
#![feature(async_fn_in_trait)]
#![feature(type_alias_impl_trait)]
#![feature(async_closure)]
#![feature(never_type)]

use core::future::Future;
use defmt::*;
use embassy_executor::{Executor, InterruptExecutor, Spawner};
use embassy_futures::select::Either;
use embassy_net::tcp::TcpSocket;
use embassy_net::{Stack, StackResources};
use embassy_rp::peripherals::{PIN_16, SPI0, SPI1, USB};
use embassy_rp::usb::{Driver, Instance, InterruptHandler};
use embassy_rp::{bind_interrupts, interrupt, peripherals};
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::spi::{Async, Spi};
use embassy_rp::watchdog::Watchdog;
use embassy_sync::blocking_mutex::CriticalSectionMutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_ncm::embassy_net::{Device, Runner, State as NetState};
use embassy_usb::class::cdc_ncm::{CdcNcmClass, State};
use embassy_usb::{Builder, Config, UsbDevice};
use embassy_usb::class::cdc_acm::CdcAcmClass;
use embassy_usb::driver::EndpointError;
use embedded_io::asynch::Write;
use futures::SinkExt;
use static_cell::{make_static, StaticCell};
use {defmt_rtt as _, panic_probe as _};
use crate::MessageError::InvalidMessageType;
use crate::MessageType::{Ping, Pong, ThermocoupleReading};

pub struct AxisRuntime<'a> {
    internal_to_external_channel: &'a Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
    external_to_internal_channel: &'a Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
    pub watchdog: &'a mut Watchdog,
    client_spi: &'a mut Spi<'a, SPI0, Async>,
}

impl<'a> AxisRuntime<'a> {
    pub fn new(
        internal_to_external_channel: &'a Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
        external_to_internal_channel: &'a Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
        watchdog: &'a mut Watchdog,
        client_spi: &'a mut Spi<'a, SPI0, Async>,
    ) -> Self {
        Self {
            internal_to_external_channel,
            external_to_internal_channel,
            watchdog,
            client_spi,
        }
    }

    async fn handle(&mut self, m: MessageDTO<'a>) -> Result<(), MessageError> {
        match m.message_type {
            Ping => self.SendPong().await,
            _ => Err(InvalidMessageType),
        }
    }

    async fn SendPong(&mut self) -> Result<(), MessageError> {
        self.internal_to_external_channel
            .send(MessageDTO {
                message_type: Pong,
                content_len: 0,
                content: &[0u8; 0],
            })
            .await;
        self.watchdog.feed();
        Ok(())
    }

    pub async fn run(&mut self) {
        loop {
            self.process_internal_to_external().await;
            self.process_external_to_internal().await;
        }
    }

    async fn process_external_to_internal(&mut self) {
        let mut type_buff = [0u8; 1];
        let mut len_buff = [0u8; 2];
        let mut content_buffer = [0u8; 1024];
        let content = make_static!([0u8; 1024]);

        // First, process inbound messages, if any, then outbound.
        let task = read_message(
            &mut self.client_spi,
            &mut type_buff,
            &mut len_buff,
            &mut content_buffer,
            content,
        );

        let _ = embassy_time::with_timeout(Duration::from_micros(1), task)
            .await
            .map(|m| {
                m.map(|b| async {
                    embassy_time::with_timeout(
                        Duration::from_micros(1),
                        self.external_to_internal_channel.send(b),
                    )
                    .await
                })
            });
    }
    async fn process_internal_to_external(&mut self) {
        let task = self.internal_to_external_channel.recv();

        let _ = embassy_time::with_timeout(Duration::from_micros(1), task)
            .await
            .map(move |m| async move {
                unsafe {
                    embassy_time::with_timeout(
                        Duration::from_millis(1),
                        self.client_spi.write(core::slice::from_raw_parts(
                          (&m as *const MessageDTO) as *const u8,
                          1 + 2 + m.content.len()),
                        ))
                }.await
            });
    }
}

async fn read_message<'a>(
    spi: &mut Spi<'a, SPI0, Async>,
    type_buff: &mut [u8; 1],
    len_buff: &mut [u8; 2],
    content_buffer: &mut [u8; 1024],
    content: &'a [u8],
) -> Option<MessageDTO<'a>> {

    // get type byte.
    if let Err(_) = spi.read(type_buff).await {
        spi.flush().unwrap();
        return None;
    }

    let message_type = match MessageType::from_u8(type_buff[0]) {
        Ok(t) => t,
        Err(_) => return None,
    };

    // read length buffer
    if let Err(_) = spi.read(len_buff).await {
        spi.flush().unwrap();
        return None;
    }

    let content_len = u16::from_le_bytes([len_buff[0], len_buff[1]]);
    if content_len > 0 {
        if let Err(_) = spi.read(&mut content_buffer[..content_len as usize]).await {
            spi.flush().unwrap();
            return None;
        }
    }

    content_buffer[..content_len as usize].clone_from_slice(content);

    let message = MessageDTO {
        message_type,
        content_len,
        content: content[..content_len as usize].as_ref(),
    };
    Some(message)
}

#[embassy_executor::task]
async fn read_thermocouple(mut pin: Output<'static, PIN_16>) {
    loop {
        pin.set_high();
        Timer::after(Duration::from_millis(500)).await;
        pin.set_low();
        Timer::after(Duration::from_millis(500)).await;
    }
}

async fn publish_message<'a>(
    spi: &mut Spi<'a, SPI0, Async>,
    external_to_internal_channel: &Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
) {
    let mut type_buff = [0u8; 1];
    let mut len_buff = [0u8; 2];
    let mut content_buffer = [0u8; 1024];
    let content = make_static!([0u8; 1024]);

    loop {
        let message = match read_message(
            spi,
            &mut type_buff,
            &mut len_buff,
            &mut content_buffer,
            content,
        )
        .await
        {
            Some(value) => value,
            None => continue,
        };

        external_to_internal_channel.send(message).await;
    }
}

#[embassy_executor::task]
pub async unsafe fn client_mcu_communication_loop(
    spi: &'static mut Spi<'static, SPI0, Async>,
    external_to_internal_channel: &'static Channel<CriticalSectionRawMutex, MessageDTO<'static>, 1>,
    internal_to_external_channel: &'static Channel<CriticalSectionRawMutex, MessageDTO<'static>, 1>,
) {
    let mut type_buff = [0u8; 1];
    let mut len_buff = [0u8; 2];
    let mut content_buffer = [0u8; 1024];
    let content = make_static!([0u8; 1024]);

    loop {
        let work = internal_to_external_channel.recv();

        let _a = embassy_time::with_timeout(Duration::from_micros(1), work)
            .await
            .map(|m| async {
                embassy_time::with_timeout(
                    Duration::from_micros(1),
                    internal_to_external_channel.send(m),
                )
                .await
            });
    }
}

pub struct MessageDTO<'a> {
    message_type: MessageType,
    content_len: u16,
    content: &'a [u8],
}

#[repr(u8)]
pub enum MessageType {
    Startup = 0,
    Acknowledge = 1,
    Ping = 2,
    Pong = 3,
    ThermocoupleReading = 4,
}

impl MessageType {
    pub fn from_u8(val: u8) -> Result<MessageType, MessageError> {
        match val {
            1 => Ok(Ping),
            2 => Ok(Pong),
            _ => Err(InvalidMessageType),
        }
    }
}

pub enum MessageError {
    InvalidMessageType,
    Unknown,
}

impl<'a> MessageDTO<'a> {
    pub unsafe fn to_bytes(&self) -> &[u8] {
        core::slice::from_raw_parts(
            (self as *const Self) as *const u8,
            1 + 2 + self.content.len(),
        )
    }
}

pub struct Messenger<'a> {
    channel: &'a Channel<CriticalSectionRawMutex, MessageDTO<'a>, 1>,
}

impl<'a> Messenger<'a> {
    pub fn new(channel: &'a Channel<embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex, MessageDTO<'a>, 1>) -> Self {
        Self { channel }
    }

    pub async fn publish(&self, m: MessageDTO<'a>) {
        self.channel.send(m).await;
    }

    pub async fn receive(&self) -> MessageDTO<'a> {
        self.channel.recv().await
    }
}

pub enum SignalFlag {}

static EXECUTOR_HIGH: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_MED: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_LOW: StaticCell<Executor> = StaticCell::new();
static RUNTIME: StaticCell<AxisRuntime> = StaticCell::new();
static WATCHDOG: StaticCell<Watchdog> = StaticCell::new();
static CLIENT_SPI: StaticCell<Spi<SPI0, Async>> = StaticCell::new();
static THERMOCOUPLE_SPI: StaticCell<Spi<SPI1, Async>> = StaticCell::new();

static EXTERNAL_TO_INTERNAL_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, MessageDTO, 1>> =
    StaticCell::new();

static TEST: StaticCell<CriticalSectionMutex<Channel<CriticalSectionRawMutex, MessageDTO, 1>>> = StaticCell::new();

static KILL_SIGNAL: Signal<CriticalSectionRawMutex, SignalFlag> = Signal::new();

#[interrupt]
unsafe fn SWI_IRQ_1() {
    EXECUTOR_HIGH.on_interrupt()
}

#[interrupt]
unsafe fn SWI_IRQ_0() {
    EXECUTOR_MED.on_interrupt()
}

// #[embassy_executor::task]
// pub async fn watchdog_monitor(
//     watchdog: &'static mut Watchdog,
//     mut request_pin: Output<'static, PIN_26>,
//     mut reply_pin: Input<'static, PIN_28>,
// ) {
//     watchdog.start(Duration::from_secs(5));
//
//     loop {
//         if (wait_with_timeout(3_000, reply_pin.wait_for_high()).await).is_err() {
//             watchdog.trigger_reset();
//             return;
//         }
//         request_pin.set_high();
//
//         if (wait_with_timeout(1_000, reply_pin.wait_for_low()).await).is_err() {
//             watchdog.trigger_reset();
//             return;
//         }
//
//         request_pin.set_low();
//         watchdog.feed();
//     }
// }

pub async fn wait_with_timeout<F: Future>(millis: u64, fut: F) -> Result<F::Output, embassy_time::TimeoutError> {
    embassy_time::with_timeout(Duration::from_millis(millis), fut).await
}

const SPI_CLOCK_FREQ: u32 = 500_000;

static THERMOCOUPLE_BUFFER: &[u8] = [0u8; 4].as_slice();

#[embassy_executor::task]
pub async fn thermocouple_read(
    mut thermocouple: MAX31855,
    mut channel: &'static Channel<CriticalSectionRawMutex, MessageDTO<'static>, 1>,
) {
    static BUFFER: StaticCell<[u8; 4]> = StaticCell::new();
    let mut c = BUFFER.init([0u8; 4]);
        let a = embassy_time::with_timeout(Duration::from_millis(100), thermocouple.read()).await;
        if let Ok(r) = a {
            if let Ok(s) = r {
                let u = s.temperature.to_le_bytes();
                c.copy_from_slice(&u);
                let _ = embassy_time::with_timeout(Duration::from_millis(100), async {
                    channel
                        .send(MessageDTO {
                            message_type: ThermocoupleReading,
                            content: c,
                            content_len: 4,
                        })
                        .await
                })
                .await;
            };
        };
}

#[embassy_executor::task]
pub async fn main_loop(runtime: &'static mut AxisRuntime<'static>) {
    embassy_futures::join::join(
        runtime.run(),
        async {
            KILL_SIGNAL.wait().await;
        }
    )
    .await;
    runtime.watchdog.trigger_reset();
}

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

type MyDriver = Driver<'static, embassy_rp::peripherals::USB>;

#[embassy_executor::task]
async fn usb_task(mut device: UsbDevice<'static, MyDriver>) -> ! {
    device.run().await
}

#[embassy_executor::task]
pub async fn usb_reader(mut class: &'static mut CdcAcmClass<'static, Driver<'static, USB>>) {
    loop {
        class.wait_connection().await;
        info!("Connected");
        let _ = echo(&mut class).await;
        info!("Disconnected");
    }
}


// Global static state
static DRIVER: StaticCell<Driver<USB>> = StaticCell::new();
static USB_CONFIG: StaticCell<embassy_usb::Config> = StaticCell::new();
static DEVICE_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
static BOS_DESCRIPTOR:StaticCell<[u8; 256]> = StaticCell::new();
static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
static USB_STATE: StaticCell<State> = StaticCell::new();
static BUILDER: StaticCell<Builder<Driver<USB>>> = StaticCell::new();
static CDC_ADM_CLASS: StaticCell<CdcAcmClass<Driver<USB>>> = StaticCell::new();
static USB: StaticCell<UsbDevice<Driver<USB>>> = StaticCell::new();

#[embassy_executor::main]
async fn main(_s: embassy_executor::Spawner) {
    let p = embassy_rp::init(Default::default());
    let spawner = EXECUTOR_HIGH.start(interrupt::SWI_IRQ_1);

    // Create the driver, from the HAL.
    let driver = Driver::new(p.USB, Irqs);

    // Create embassy-usb Config
    let mut config = embassy_usb::Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("Embassy");
    config.product = Some("USB-serial example");
    config.serial_number = Some("12345678");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    // Required for windows compatibility.
    // https://developer.nordicsemi.com/nRF_Connect_SDK/doc/1.9.1/kconfig/CONFIG_CDC_ACM_IAD.html#help
    config.device_class = 0xEF;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;
    config.composite_with_iads = true;

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    let mut device_descriptor = [0; 256];
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut control_buf = [0; 64];

    // Create embassy-usb DeviceBuilder using the driver and config.
    let mut builder = Builder::new(
        driver,
        config,
        &mut make_static!([0; 256])[..],
        &mut make_static!([0; 256])[..],
        &mut make_static!([0; 256])[..],
        &mut make_static!([0; 128])[..],
    );

    // Create classes on the builder.
    let mut class = CdcAcmClass::new(&mut builder, make_static!(State::new()), 64);

    // Build the builder.
    let usb = builder.build();

    unwrap!(spawner.spawn(usb_task(usb)));

    // Run the USB device.
    // let usb_fut = usb.run();

    // Do stuff with the class!
    let echo_fut = async {
        loop {
            class.wait_connection().await;
            info!("Connected");
            let _ = echo(&mut class).await;
            info!("Disconnected");
        }
    };

    let external_to_internal_channel = EXTERNAL_TO_INTERNAL_CHANNEL.init(Channel::new());

    static INTERNAL_TO_EXTERNAL_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, MessageDTO, 1>> = StaticCell::new();
    let c = &*INTERNAL_TO_EXTERNAL_CHANNEL.init(Channel::new());

    let watchdog = WATCHDOG.init(Watchdog::new(p.WATCHDOG));

    let mut spi0_config = embassy_rp::spi::Config::default();
    spi0_config.frequency = 1_000_000;

    let client_mosi = p.PIN_3;
    let client_miso = p.PIN_0;
    let clk = p.PIN_2;
    let cs = p.PIN_1;
    let mosi_dma = p.DMA_CH1;
    let miso_dma = p.DMA_CH2;

    let mut client_spi: &mut Spi<'static, SPI0, Async> = CLIENT_SPI.init(Spi::new(
        p.SPI0,
        clk,
        client_mosi,
        client_miso,
        mosi_dma,
        miso_dma,
        spi0_config,
    ));

    let th_clk = p.PIN_10;
    let th_miso = p.PIN_12;
    let rx_dma = p.DMA_CH3;
    let mut config = embassy_rp::spi::Config::default();
    config.frequency = 500_000;
    let mut thermocouple_spi: &mut Spi<'static, SPI1, Async> =
        THERMOCOUPLE_SPI.init(Spi::new_rxonly(p.SPI1, th_clk, th_miso, rx_dma, config));

    let thermocouple_pinout = Output::new(p.PIN_11, Level::Low);
    let thermocouple = MAX31855::new(thermocouple_spi, thermocouple_pinout);

    let content = make_static!([0u8; 1024]);

    let mut type_buff = [0u8; 1];
    let mut len_buff = [0u8; 2];
    let mut content_buffer = [0u8; 1024];

    let mut gpio = Output::new(p.PIN_7, Level::Low);
    let mut inbound_flag = Input::new(p.PIN_4, Pull::None);

    loop {
        let blink_task = async {
            gpio.set_high();
            Timer::after(Duration::from_millis(500)).await;
            gpio.set_low();
            Timer::after(Duration::from_millis(500)).await;
        };

        let wait_for_startup = async {
            inbound_flag.wait_for_high().await;
            read_message(
                &mut client_spi,
                &mut type_buff,
                &mut len_buff,
                &mut content_buffer,
                content,
        ).await };

        let res = embassy_futures::select::select(blink_task, wait_for_startup).await;
        match res {
            Either::First(_) => {}
            Either::Second(message) => {
                match message {
                    None => {
                        return;
                    }
                    Some(m) => if let MessageType::Startup = m.message_type {
                        break;
                    },
                }
            }
        }
    }


    loop {
        gpio.set_high();
        Timer::after(Duration::from_millis(100)).await;
        gpio.set_low();
        Timer::after(Duration::from_millis(100)).await;
    }

    let c2 = c;
    let runtime = RUNTIME.init(AxisRuntime::new(
        c2,
        external_to_internal_channel,
        watchdog,
        client_spi,
    ));


    // High-priority executor: SWI_IRQ_1, priority level 2
    interrupt::SWI_IRQ_1.set_priority(Priority::P1);
    unwrap!(spawner.spawn(main_loop(runtime)));

    let c2 = c;
    unwrap!(spawner.spawn(thermocouple_read(thermocouple, c2)));

    KILL_SIGNAL.wait().await;
}

#[embassy_executor::task]
async fn test(p0: &'static mut UsbDevice<'static, Driver<'static, USB>>, p1: CdcAcmClass<'static, Driver<'static, USB>>) {
}

struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

async fn echo<'d, T: Instance + 'd>(class: &mut CdcAcmClass<'d, Driver<'d, T>>) -> Result<(), Disconnected> {
    let mut buf = [0; 64];
    loop {
        let n = class.read_packet(&mut buf).await?;
        let data = &buf[..n];
        info!("data: {:x}", data);
        class.write_packet(data).await?;
    }
}
