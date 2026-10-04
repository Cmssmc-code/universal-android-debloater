//! Tauri commands exposed to the frontend.
//!
//! Every command that touches a process or the network is wrapped in
//! `spawn_blocking` so the UI thread never stalls. All parsing is delegated to
//! `pra_core`.

use std::path::PathBuf;

use tauri::{AppHandle, Emitter};

use crate::{adb, tools};

/// Run blocking work off the UI thread and flatten the join result.
async fn spawn<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

fn log(app: &AppHandle, line: impl Into<String>) {
    let _ = app.emit("log", serde_json::json!({ "line": line.into() }));
}

fn require_adb(app: &AppHandle) -> Result<PathBuf, String> {
    let p = tools::adb_path(app);
    if p.exists() {
        Ok(p)
    } else {
        Err("Outils non prêts. Clique d'abord sur « Préparer les outils ».".into())
    }
}

fn require_fastboot(app: &AppHandle) -> Result<PathBuf, String> {
    let p = tools::fastboot_path(app);
    if p.exists() {
        Ok(p)
    } else {
        Err("Outils non prêts. Clique d'abord sur « Préparer les outils ».".into())
    }
}

/// Read up to `n` bytes from the start of a file (for boot-image validation).
fn read_head(path: &std::path::Path, n: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; n];
    let read = f.read(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(read);
    Ok(buf)
}

#[derive(serde::Serialize)]
pub struct FastbootState {
    serials: Vec<String>,
    unlocked: pra_core::fastboot::UnlockState,
}

// ---------------------------------------------------------------------------
// Informational (sync, instant)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_plan() -> Vec<pra_core::flow::Step> {
    pra_core::flow::root_plan()
}

#[tauri::command]
pub fn honest_summary() -> &'static str {
    pra_core::flow::HONEST_SUMMARY
}

#[tauri::command]
pub fn tools_status(app: AppHandle) -> bool {
    tools::tools_ready(&app)
}

// ---------------------------------------------------------------------------
// Tool management
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn ensure_tools(app: AppHandle) -> Result<(), String> {
    spawn(move || tools::ensure_tools(&app)).await
}

// ---------------------------------------------------------------------------
// ADB side
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_devices(app: AppHandle) -> Result<Vec<pra_core::device::AdbDevice>, String> {
    spawn(move || {
        let adb = require_adb(&app)?;
        let out = adb::run(&adb, ["devices", "-l"]).map_err(|e| e.to_string())?;
        Ok(pra_core::device::parse_adb_devices(&out.stdout))
    })
    .await
}

#[tauri::command]
pub async fn device_info(app: AppHandle, serial: String) -> Result<pra_core::device::DeviceInfo, String> {
    spawn(move || {
        let adb = require_adb(&app)?;
        let out = adb::run(&adb, ["-s", serial.as_str(), "shell", "getprop"]).map_err(|e| e.to_string())?;
        let props = pra_core::device::parse_getprop(&out.stdout);
        Ok(pra_core::device::DeviceInfo::from_props(&serial, &props))
    })
    .await
}

#[tauri::command]
pub async fn reboot_bootloader(app: AppHandle, serial: String) -> Result<(), String> {
    spawn(move || {
        let adb = require_adb(&app)?;
        let out = adb::run(&adb, ["-s", serial.as_str(), "reboot", "bootloader"]).map_err(|e| e.to_string())?;
        log(&app, format!("adb reboot bootloader → {}", if out.ok { "ok" } else { "échec" }));
        if out.ok {
            Ok(())
        } else {
            Err(format!("{}{}", out.stdout, out.stderr))
        }
    })
    .await
}

#[tauri::command]
pub async fn install_magisk(app: AppHandle, serial: String) -> Result<String, String> {
    spawn(move || {
        let adb = require_adb(&app)?;
        let apk = tools::magisk_apk_path(&app);
        if !apk.exists() {
            return Err("APK Magisk absent. Clique d'abord sur « Préparer les outils ».".into());
        }
        let apk_s = apk.to_string_lossy().to_string();
        let out = adb::run(&adb, ["-s", serial.as_str(), "install", "-r", apk_s.as_str()])
            .map_err(|e| e.to_string())?;
        let combined = format!("{}{}", out.stdout, out.stderr);
        log(&app, format!("install Magisk: {}", combined.trim()));
        if out.ok || combined.contains("Success") {
            Ok(combined)
        } else {
            Err(combined)
        }
    })
    .await
}

/// Result of pushing a stock image to the phone: which partition it belongs to
/// (so the UI can flash the patched version back to the same one).
#[derive(serde::Serialize)]
pub struct PushedImage {
    message: String,
    partition: pra_core::BootPartition,
}

#[tauri::command]
pub async fn pick_and_push_boot(app: AppHandle, serial: String) -> Result<PushedImage, String> {
    // The native file picker must run on the main thread.
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let file = rfd::FileDialog::new()
            .add_filter("Image de démarrage", &["img"])
            .set_title("Choisir init_boot.img (s'il existe) sinon boot.img, de ta version exacte")
            .pick_file();
        let _ = tx.send(file);
    })
    .map_err(|e| e.to_string())?;

    let picked = rx.recv().map_err(|e| e.to_string())?;
    let path = picked.ok_or("Aucun fichier choisi.")?;

    let app2 = app.clone();
    spawn(move || {
        let adb = require_adb(&app2)?;

        // Guard 1: the file name decides the target partition. Refuse anything
        // that is not clearly boot.img or init_boot.img rather than guess.
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let partition = pra_core::partition_for_image(&file_name).ok_or_else(|| {
            format!(
                "Nom de fichier « {file_name} » non reconnu. Le fichier doit s'appeler boot.img ou init_boot.img (tel qu'extrait de la ROM)."
            )
        })?;

        // Guard 2: it must really be an Android boot image.
        let head = read_head(&path, 8)?;
        if !pra_core::looks_like_boot_image(&head) {
            return Err("Ce fichier ne ressemble pas à une image boot Android (magic « ANDROID! » absent). Reprends le fichier d'origine extrait de ta ROM.".into());
        }

        let remote = format!("/sdcard/Download/{}.img", partition.fastboot_name());
        let ps = path.to_string_lossy().to_string();
        let out = adb::run(
            &adb,
            ["-s", serial.as_str(), "push", ps.as_str(), remote.as_str()],
        )
        .map_err(|e| e.to_string())?;
        let combined = format!("{}{}", out.stdout, out.stderr);
        log(&app2, format!("push {file_name} → {remote}: {}", combined.trim()));
        if out.ok {
            Ok(PushedImage {
                message: format!(
                    "{file_name} copié dans /sdcard/Download/ (partition cible : {}). Patche-le maintenant dans l'app Magisk.",
                    partition.fastboot_name()
                ),
                partition,
            })
        } else {
            Err(combined)
        }
    })
    .await
}

#[tauri::command]
pub async fn pull_patched_boot(app: AppHandle, serial: String) -> Result<String, String> {
    spawn(move || {
        let adb = require_adb(&app)?;
        let ls = adb::run(&adb, ["-s", serial.as_str(), "shell", "ls", "-1t", "/sdcard/Download/"])
            .map_err(|e| e.to_string())?;
        let name = pra_core::magisk::latest_patched_image(&ls.stdout)
            .ok_or("Aucun magisk_patched_*.img trouvé dans Download. As-tu lancé le patch dans Magisk ?")?;
        let remote = format!("/sdcard/Download/{name}");
        let outdir = tools::data_dir(&app).join("out");
        std::fs::create_dir_all(&outdir).map_err(|e| e.to_string())?;
        let local = outdir.join("magisk_patched.img");
        let local_s = local.to_string_lossy().to_string();
        let out = adb::run(&adb, ["-s", serial.as_str(), "pull", remote.as_str(), local_s.as_str()])
            .map_err(|e| e.to_string())?;
        log(&app, format!("pull {name} → {}", format!("{}{}", out.stdout, out.stderr).trim()));
        if out.ok && local.exists() {
            Ok(local_s)
        } else {
            Err(format!("{}{}", out.stdout, out.stderr))
        }
    })
    .await
}

// ---------------------------------------------------------------------------
// Fastboot side
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn fastboot_state(app: AppHandle) -> Result<FastbootState, String> {
    spawn(move || {
        let fb = require_fastboot(&app)?;
        let devices = adb::run(&fb, ["devices"]).map_err(|e| e.to_string())?;
        let serials =
            pra_core::fastboot::parse_fastboot_devices(&format!("{}{}", devices.stdout, devices.stderr));
        let unlocked = if serials.is_empty() {
            pra_core::fastboot::UnlockState::Unknown
        } else {
            let g = adb::run(&fb, ["getvar", "unlocked"]).map_err(|e| e.to_string())?;
            pra_core::fastboot::unlock_state(&format!("{}{}", g.stdout, g.stderr))
        };
        Ok(FastbootState { serials, unlocked })
    })
    .await
}

#[tauri::command]
pub async fn flash_patched_boot(
    app: AppHandle,
    img_path: String,
    partition: String,
    serial: Option<String>,
) -> Result<String, String> {
    spawn(move || {
        let fb = require_fastboot(&app)?;
        // Guard 0: only ever flash to a whitelisted partition. The UI value is
        // never passed to fastboot unless it is exactly "boot" or "init_boot".
        let target = pra_core::BootPartition::parse(&partition).ok_or_else(|| {
            format!("Partition « {partition} » refusée : seules « boot » et « init_boot » sont autorisées.")
        })?;
        // Guard 1: the file must look like a real boot image.
        let head = read_head(std::path::Path::new(&img_path), 8)?;
        if !pra_core::looks_like_boot_image(&head) {
            return Err("L'image à flasher n'a pas le magic « ANDROID! » : ce n'est pas une image boot valide. Reprends la récupération du boot patché.".into());
        }
        // Guard 2: never flash boot on a locked bootloader (it would fail).
        let gv = adb::run(&fb, ["getvar", "unlocked"]).map_err(|e| e.to_string())?;
        if matches!(
            pra_core::fastboot::unlock_state(&format!("{}{}", gv.stdout, gv.stderr)),
            pra_core::fastboot::UnlockState::Locked
        ) {
            return Err("Bootloader VERROUILLÉ : je refuse de flasher (ça échouerait). Déverrouille le bootloader d'abord.".into());
        }
        let mut args: Vec<String> = Vec::new();
        if let Some(s) = &serial {
            args.push("-s".into());
            args.push(s.clone());
        }
        args.push("flash".into());
        args.push(target.fastboot_name().into());
        args.push(img_path.clone());
        let out = adb::run(&fb, &args).map_err(|e| e.to_string())?;
        let flash = format!("{}{}", out.stdout, out.stderr);
        log(
            &app,
            format!("fastboot flash {} → {}", target.fastboot_name(), flash.trim()),
        );
        if !out.ok {
            return Err(flash);
        }

        let mut rargs: Vec<String> = Vec::new();
        if let Some(s) = &serial {
            rargs.push("-s".into());
            rargs.push(s.clone());
        }
        rargs.push("reboot".into());
        let r = adb::run(&fb, &rargs).map_err(|e| e.to_string())?;
        log(&app, "fastboot reboot → envoyé");
        Ok(format!("{flash}\n{}{}", r.stdout, r.stderr))
    })
    .await
}

#[tauri::command]
pub async fn reboot_from_fastboot(app: AppHandle) -> Result<(), String> {
    spawn(move || {
        let fb = require_fastboot(&app)?;
        adb::run(&fb, ["reboot"]).map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn verify_root(app: AppHandle, serial: String) -> Result<String, String> {
    spawn(move || {
        let adb = require_adb(&app)?;
        let out = adb::run(&adb, ["-s", serial.as_str(), "shell", "pm", "path", "com.topjohnwu.magisk"])
            .map_err(|e| e.to_string())?;
        let combined = format!("{}{}", out.stdout, out.stderr);
        if combined.contains("package:") {
            Ok("Magisk est installé. Ouvre l'app Magisk : si elle indique « Installé » (et non « N/A »), le root est actif. 🎉".into())
        } else {
            Err("Magisk introuvable sur le téléphone. Reprends l'étape d'installation de Magisk.".into())
        }
    })
    .await
}

// ---------------------------------------------------------------------------
// Shell helpers
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    spawn(move || {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            std::process::Command::new("cmd")
                .args(["/C", "start", "", url.as_str()])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        #[cfg(not(windows))]
        {
            let _ = &url;
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn open_data_folder(app: AppHandle) -> Result<(), String> {
    spawn(move || {
        let dir = tools::data_dir(&app);
        std::fs::create_dir_all(&dir).ok();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            std::process::Command::new("explorer")
                .arg(&dir)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })
    .await
}
