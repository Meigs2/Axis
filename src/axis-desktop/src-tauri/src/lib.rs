use tauri::AppHandle;
use tokio;

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
        usb_task().await
    });

    Ok(())
}

pub async fn usb_task() {

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
