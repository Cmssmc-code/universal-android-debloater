//! Parsing of `adb devices -l` and `adb shell getprop`, and the derived
//! [`DeviceInfo`] model shown in the UI.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{TARGET_CODENAME, TARGET_MODEL_PREFIX};

/// Connection state reported by `adb devices`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdbState {
    /// Ready for commands.
    Device,
    /// Plugged in but the "Allow USB debugging" prompt has not been accepted.
    Unauthorized,
    /// Seen by the daemon but not responding.
    Offline,
    Recovery,
    Sideload,
    Other(String),
}

impl AdbState {
    fn parse(token: &str) -> Self {
        match token {
            "device" => AdbState::Device,
            "unauthorized" => AdbState::Unauthorized,
            "offline" => AdbState::Offline,
            "recovery" => AdbState::Recovery,
            "sideload" => AdbState::Sideload,
            other => AdbState::Other(other.to_string()),
        }
    }
}

/// A single line of `adb devices -l`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdbDevice {
    pub serial: String,
    pub state: AdbState,
    /// `model:` field from `-l` output, when present.
    pub model: Option<String>,
}

/// Parse the output of `adb devices` or `adb devices -l`.
///
/// Header lines, daemon chatter (`* daemon ... *`) and blank lines are ignored.
pub fn parse_adb_devices(output: &str) -> Vec<AdbDevice> {
    let mut devices = Vec::new();
    for raw in output.lines() {
        let line = raw.trim();
        if line.is_empty()
            || line.starts_with('*')
            || line.starts_with("List of devices")
            || line.starts_with("adb server")
        {
            continue;
        }
        let mut parts = line.split_whitespace();
        let serial = match parts.next() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let state = match parts.next() {
            Some(s) => AdbState::parse(s),
            None => continue,
        };
        let model = parts
            .find(|t| t.starts_with("model:"))
            .map(|t| t.trim_start_matches("model:").replace('_', " "));
        devices.push(AdbDevice {
            serial,
            state,
            model,
        });
    }
    devices
}

/// Parse `adb shell getprop` output of the form `[key]: [value]`.
pub fn parse_getprop(output: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in output.lines() {
        let line = line.trim();
        if !line.starts_with('[') {
            continue;
        }
        if let Some(sep) = line.find("]: [") {
            let key = &line[1..sep];
            let rest = &line[sep + 4..];
            let value = rest.strip_suffix(']').unwrap_or(rest);
            if !key.is_empty() {
                map.insert(key.to_string(), value.to_string());
            }
        }
    }
    map
}

/// How well the app expects to support the connected device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    /// Exactly the device the app was designed for (POCO X6 5G / `garnet`).
    Target,
    /// Some other Android device: the generic flow may work but is untested.
    Generic,
    /// Not enough info to tell (no codename read yet).
    Unknown,
}

/// Human-facing summary of a connected device, built from `getprop`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub serial: String,
    pub codename: Option<String>,
    pub model: Option<String>,
    pub brand: Option<String>,
    pub android_version: Option<String>,
    pub sdk: Option<String>,
    pub security_patch: Option<String>,
    pub fingerprint: Option<String>,
    /// HyperOS / MIUI marketing version, when present.
    pub os_version: Option<String>,
    /// Exact firmware build id — the value you must match when fetching a
    /// stock `boot.img`. Mismatched builds are the #1 cause of boot loops.
    pub build_incremental: Option<String>,
    pub is_target: bool,
    pub support: SupportLevel,
}

impl DeviceInfo {
    pub fn from_props(serial: &str, props: &BTreeMap<String, String>) -> Self {
        let get = |keys: &[&str]| -> Option<String> {
            for k in keys {
                if let Some(v) = props.get(*k) {
                    if !v.is_empty() {
                        return Some(v.clone());
                    }
                }
            }
            None
        };

        let codename = get(&[
            "ro.product.device",
            "ro.product.vendor.device",
            "ro.product.odm.device",
            "ro.product.system.device",
        ]);
        let model = get(&["ro.product.model", "ro.product.vendor.model"]);
        let brand = get(&["ro.product.brand", "ro.product.vendor.brand"]);

        let is_target = codename
            .as_deref()
            .map(|c| c.eq_ignore_ascii_case(TARGET_CODENAME))
            .unwrap_or(false)
            || model
                .as_deref()
                .map(|m| m.to_ascii_uppercase().starts_with(TARGET_MODEL_PREFIX))
                .unwrap_or(false);

        let support = if is_target {
            SupportLevel::Target
        } else if codename.is_some() {
            SupportLevel::Generic
        } else {
            SupportLevel::Unknown
        };

        DeviceInfo {
            serial: serial.to_string(),
            codename,
            model,
            brand,
            android_version: get(&["ro.build.version.release"]),
            sdk: get(&["ro.build.version.sdk"]),
            security_patch: get(&["ro.build.version.security_patch"]),
            fingerprint: get(&["ro.build.fingerprint"]),
            os_version: get(&[
                "ro.mi.os.version.name",
                "ro.miui.ui.version.name",
                "ro.build.version.incremental",
            ]),
            build_incremental: get(&["ro.build.version.incremental"]),
            is_target,
            support,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_adb_devices_long_format() {
        let out = "List of devices attached\n\
                   1a2b3c4d               device usb:1-1 product:garnet model:23122PCD1G device:garnet transport_id:5\n\
                   emulator-5554          offline\n\
                   zz99                   unauthorized\n";
        let d = parse_adb_devices(out);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].serial, "1a2b3c4d");
        assert_eq!(d[0].state, AdbState::Device);
        assert_eq!(d[0].model.as_deref(), Some("23122PCD1G"));
        assert_eq!(d[1].state, AdbState::Offline);
        assert_eq!(d[2].state, AdbState::Unauthorized);
    }

    #[test]
    fn parses_adb_devices_short_and_ignores_noise() {
        let out = "* daemon not running; starting now at tcp:5037 *\n\
                   * daemon started successfully *\n\
                   List of devices attached\n\
                   abcdef\tdevice\n\n";
        let d = parse_adb_devices(out);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].serial, "abcdef");
        assert_eq!(d[0].state, AdbState::Device);
        assert_eq!(d[0].model, None);
    }

    #[test]
    fn parses_getprop_and_builds_target_device() {
        let out = "[ro.product.brand]: [POCO]\n\
                   [ro.product.model]: [23122PCD1G]\n\
                   [ro.product.device]: [garnet]\n\
                   [ro.build.version.release]: [14]\n\
                   [ro.build.version.sdk]: [34]\n\
                   [ro.build.version.security_patch]: [2024-09-01]\n\
                   [ro.mi.os.version.name]: [OS1.0.8.0.UNCMIXM]\n\
                   [ro.build.version.incremental]: [OS1.0.8.0.UNCMIXM]\n\
                   [persist.sys.timezone]: [Europe/Paris]\n";
        let props = parse_getprop(out);
        assert_eq!(props.get("ro.product.device").map(|s| s.as_str()), Some("garnet"));

        let info = DeviceInfo::from_props("1a2b3c4d", &props);
        assert_eq!(info.codename.as_deref(), Some("garnet"));
        assert_eq!(info.model.as_deref(), Some("23122PCD1G"));
        assert_eq!(info.android_version.as_deref(), Some("14"));
        assert!(info.is_target);
        assert_eq!(info.support, SupportLevel::Target);
        assert_eq!(info.os_version.as_deref(), Some("OS1.0.8.0.UNCMIXM"));
    }

    #[test]
    fn non_target_device_is_generic() {
        let out = "[ro.product.device]: [sunfish]\n[ro.product.model]: [Pixel 4a]\n";
        let props = parse_getprop(out);
        let info = DeviceInfo::from_props("serial", &props);
        assert!(!info.is_target);
        assert_eq!(info.support, SupportLevel::Generic);
    }

    #[test]
    fn empty_getprop_is_unknown() {
        let props = parse_getprop("");
        let info = DeviceInfo::from_props("serial", &props);
        assert_eq!(info.support, SupportLevel::Unknown);
    }

    #[test]
    fn target_detected_by_model_prefix_only() {
        let out = "[ro.product.model]: [23122PCD1G]\n";
        let props = parse_getprop(out);
        let info = DeviceInfo::from_props("serial", &props);
        assert!(info.is_target);
    }
}
