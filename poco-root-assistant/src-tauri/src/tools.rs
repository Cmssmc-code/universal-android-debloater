//! First-run tool management: download and cache Google Platform-Tools
//! (adb + fastboot) and the latest Magisk APK into the app data directory.
//!
//! Nothing here is bundled into the repo: the official binaries are fetched
//! from their canonical sources on first run and cached locally. That keeps
//! the installer small, always ships the latest versions, and avoids
//! redistributing third-party binaries.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter, Manager};

/// Official Google Platform-Tools (adb + fastboot) for Windows.
pub const PLATFORM_TOOLS_URL: &str =
    "https://dl.google.com/android/repository/platform-tools-latest-windows.zip";
/// GitHub API endpoint for the latest Magisk release.
pub const MAGISK_API: &str = "https://api.github.com/repos/topjohnwu/Magisk/releases/latest";
/// Known-good fallback if the GitHub API is unreachable.
pub const MAGISK_FALLBACK_URL: &str =
    "https://github.com/topjohnwu/Magisk/releases/download/v28.1/Magisk-v28.1.apk";

pub fn data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("poco-root-assistant"))
}

pub fn adb_path(app: &AppHandle) -> PathBuf {
    data_dir(app).join("platform-tools").join("adb.exe")
}

pub fn fastboot_path(app: &AppHandle) -> PathBuf {
    data_dir(app).join("platform-tools").join("fastboot.exe")
}

pub fn magisk_apk_path(app: &AppHandle) -> PathBuf {
    data_dir(app).join("magisk").join("Magisk.apk")
}

pub fn tools_ready(app: &AppHandle) -> bool {
    adb_path(app).exists() && fastboot_path(app).exists()
}

fn emit_progress(app: &AppHandle, stage: &str, pct: u8, message: &str) {
    let _ = app.emit(
        "tool-progress",
        serde_json::json!({ "stage": stage, "pct": pct, "message": message }),
    );
}

fn http_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent("PocoRootAssistant/0.1 (+https://github.com/Cmssmc-code)")
        .build()
        .expect("http client")
}

/// Stream a URL to `dest`, emitting percentage progress events.
fn download(app: &AppHandle, url: &str, dest: &Path, stage: &str) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let client = http_client();
    let mut resp = client.get(url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} pour {url}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 65536];
    let mut downloaded: u64 = 0;
    let mut last_pct: i32 = -1;
    loop {
        let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        downloaded += n as u64;
        if total > 0 {
            let pct = ((downloaded * 100) / total) as i32;
            if pct != last_pct {
                last_pct = pct;
                emit_progress(app, stage, pct as u8, &format!("Téléchargement… {pct}%"));
            }
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// Extract a zip archive into `dest`, guarding against path traversal.
fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let f = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let rel = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        let outpath = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = fs::File::create(&outpath).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn latest_magisk_url() -> Result<String, String> {
    let client = http_client();
    let value: serde_json::Value = client
        .get(MAGISK_API)
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    let assets = value
        .get("assets")
        .and_then(|a| a.as_array())
        .ok_or("réponse GitHub sans « assets »")?;
    let name_of = |a: &serde_json::Value| a.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
    let names: Vec<String> = assets.iter().map(name_of).collect();
    // Only the real Magisk app: never a stub/debug build.
    let wanted = pra_core::magisk::pick_magisk_apk(names.iter().map(|s| s.as_str()))
        .ok_or("aucun APK Magisk reconnu dans la dernière release")?;
    for asset in assets {
        if name_of(asset) == wanted {
            if let Some(url) = asset.get("browser_download_url").and_then(|u| u.as_str()) {
                return Ok(url.to_string());
            }
        }
    }
    Err("APK Magisk sans lien de téléchargement".into())
}

/// A real Magisk APK is a zip of several MB. Reject (and delete) anything else,
/// such as an HTML error page, so a bad file is never cached and installed.
fn validate_apk(path: &Path) -> Result<(), String> {
    let len = fs::metadata(path).map_err(|e| e.to_string())?.len();
    let mut head = [0u8; 4];
    let head_ok = fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && pra_core::magisk::looks_like_apk(&head);
    if len < 1_000_000 || !head_ok {
        let _ = fs::remove_file(path);
        return Err(
            "Le fichier Magisk téléchargé est invalide (ce n'est pas un APK complet). Relance « Préparer les outils »."
                .into(),
        );
    }
    Ok(())
}

/// Ensure Platform-Tools and the Magisk APK are present, downloading what is
/// missing. Emits `tool-progress` and a final `tools` event.
pub fn ensure_tools(app: &AppHandle) -> Result<(), String> {
    let dir = data_dir(app);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    if !tools_ready(app) {
        emit_progress(app, "platform-tools", 0, "Préparation des Platform-Tools (adb, fastboot)…");
        let zip = dir.join("platform-tools.zip");
        download(app, PLATFORM_TOOLS_URL, &zip, "platform-tools")?;
        emit_progress(app, "platform-tools", 100, "Extraction des Platform-Tools…");
        extract_zip(&zip, &dir)?;
        let _ = fs::remove_file(&zip);
    }

    let apk = magisk_apk_path(app);
    if !apk.exists() {
        emit_progress(app, "magisk", 0, "Récupération de la dernière version de Magisk…");
        let url = latest_magisk_url().unwrap_or_else(|_| MAGISK_FALLBACK_URL.to_string());
        download(app, &url, &apk, "magisk")?;
        validate_apk(&apk)?;
    }

    emit_progress(app, "done", 100, "Outils prêts ✅");
    let _ = app.emit("tools", serde_json::json!({ "ready": true }));
    Ok(())
}
