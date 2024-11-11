use std::borrow::Borrow;
use futures_core::Stream;
use futures_util::StreamExt;  // Brings in useful Stream extension traits
use tokio;  // We'll use tokio for the runtime



pub async fn run() {
    let mut watch: nusb::hotplug::HotplugWatch = nusb::watch_devices().unwrap();

    let item = watch.next().await.unwrap();

    println!("Event test: {:?}", item);
}