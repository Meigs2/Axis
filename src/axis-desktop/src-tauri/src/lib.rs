use std::{thread::sleep, time::Duration};

use tauri::AppHandle;

mod usb;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn test() -> String {
    "test!".to_string()
}

fn setup_app<'a>(app: &'a mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // This one
    let handle = app.handle();

    tauri::async_runtime::spawn(async {
        loop {
            usb::client::run().await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(setup_app)
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub async fn read_usb(handle: &AppHandle) {

}
