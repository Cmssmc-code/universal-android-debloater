// Hide the console window in release builds (GUI app).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adb;
mod commands;
mod tools;
mod watcher;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Start watching for a plugged-in phone right away.
            let handle = app.handle().clone();
            watcher::spawn(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_plan,
            commands::honest_summary,
            commands::tools_status,
            commands::ensure_tools,
            commands::list_devices,
            commands::device_info,
            commands::reboot_bootloader,
            commands::install_magisk,
            commands::pick_and_push_boot,
            commands::pull_patched_boot,
            commands::fastboot_state,
            commands::flash_patched_boot,
            commands::reboot_from_fastboot,
            commands::verify_root,
            commands::open_url,
            commands::open_data_folder,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Poco Root Assistant");
}
