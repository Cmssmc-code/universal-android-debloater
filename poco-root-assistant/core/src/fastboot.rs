//! Parsing of `fastboot` output.
//!
//! `fastboot` prints variable queries and progress to **stderr**, often with a
//! `(bootloader)` prefix, so the parsers here accept both forms and don't care
//! which stream the text came from.

use serde::{Deserialize, Serialize};

/// Bootloader lock status derived from `fastboot getvar unlocked`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnlockState {
    Unlocked,
    Locked,
    Unknown,
}

/// Extract a variable value from `fastboot getvar <var>` output.
///
/// Handles both `unlocked: yes` and `(bootloader) unlocked: yes`.
pub fn parse_getvar(output: &str, var: &str) -> Option<String> {
    let needle = format!("{var}:");
    for line in output.lines() {
        let line = line.trim().trim_start_matches("(bootloader)").trim();
        if let Some(rest) = line.strip_prefix(&needle) {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Interpret `fastboot getvar unlocked` output.
pub fn unlock_state(output: &str) -> UnlockState {
    match parse_getvar(output, "unlocked").as_deref() {
        Some("yes") => UnlockState::Unlocked,
        Some("no") => UnlockState::Locked,
        _ => UnlockState::Unknown,
    }
}

/// Serials currently in fastboot mode, from `fastboot devices`.
pub fn parse_fastboot_devices(output: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('<') {
            continue;
        }
        let mut parts = line.split_whitespace();
        if let (Some(serial), Some(kind)) = (parts.next(), parts.next()) {
            if kind == "fastboot" {
                out.push(serial.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unlocked_plain() {
        let out = "unlocked: yes\nfinished. total time: 0.001s\n";
        assert_eq!(parse_getvar(out, "unlocked").as_deref(), Some("yes"));
        assert_eq!(unlock_state(out), UnlockState::Unlocked);
    }

    #[test]
    fn parses_unlocked_bootloader_prefix() {
        let out = "(bootloader) unlocked: no\nfinished. total time: 0.000s\n";
        assert_eq!(unlock_state(out), UnlockState::Locked);
    }

    #[test]
    fn unknown_when_absent() {
        assert_eq!(unlock_state("getvar:unlocked FAILED (remote: ...)\n"), UnlockState::Unknown);
    }

    #[test]
    fn parses_fastboot_devices() {
        let out = "1a2b3c4d\tfastboot\n";
        let d = parse_fastboot_devices(out);
        assert_eq!(d, vec!["1a2b3c4d".to_string()]);
    }

    #[test]
    fn ignores_waiting_line() {
        let out = "< waiting for any device >\n1a2b3c4d\tfastboot\n";
        assert_eq!(parse_fastboot_devices(out), vec!["1a2b3c4d".to_string()]);
    }
}
