//! Background USB watcher.
//!
//! Polls `adb devices` / `fastboot devices` every couple of seconds and pushes
//! changes to the frontend as events, so the UI reacts the moment a phone is
//! plugged in — "dès que je branche".

use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::{adb, tools};

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last_devices = String::new();
        let mut last_info_serial = String::new();
        let mut last_fastboot = String::new();
        let mut last_ready: Option<bool> = None;

        loop {
            let ready = tools::tools_ready(&app);
            if last_ready != Some(ready) {
                last_ready = Some(ready);
                let _ = app.emit("tools", serde_json::json!({ "ready": ready }));
            }

            if ready {
                let adb_path = tools::adb_path(&app);
                if let Ok(out) = adb::run(&adb_path, ["devices", "-l"]) {
                    let devices = pra_core::device::parse_adb_devices(&out.stdout);
                    let sig = format!("{devices:?}");
                    if sig != last_devices {
                        last_devices = sig;
                        let _ = app.emit("devices", &devices);
                    }

                    match devices
                        .iter()
                        .find(|d| matches!(d.state, pra_core::device::AdbState::Device))
                    {
                        Some(d) => {
                            if d.serial != last_info_serial {
                                last_info_serial = d.serial.clone();
                                if let Ok(props_out) =
                                    adb::run(&adb_path, ["-s", d.serial.as_str(), "shell", "getprop"])
                                {
                                    let props = pra_core::device::parse_getprop(&props_out.stdout);
                                    let info =
                                        pra_core::device::DeviceInfo::from_props(&d.serial, &props);
                                    let _ = app.emit("device-info", &info);
                                }
                            }
                        }
                        None => last_info_serial.clear(),
                    }
                }

                let fastboot_path = tools::fastboot_path(&app);
                if let Ok(fb_out) = adb::run(&fastboot_path, ["devices"]) {
                    let serials = pra_core::fastboot::parse_fastboot_devices(&format!(
                        "{}{}",
                        fb_out.stdout, fb_out.stderr
                    ));
                    let sig = format!("{serials:?}");
                    if sig != last_fastboot {
                        last_fastboot = sig;
                        let _ = app.emit("fastboot", serde_json::json!({ "serials": serials }));
                    }
                }
            }

            std::thread::sleep(Duration::from_secs(2));
        }
    });
}
