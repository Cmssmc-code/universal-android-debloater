//! Lightweight validation and routing of Android boot images.
//!
//! Flashing the wrong file to the wrong partition is the classic way to end up
//! in a boot loop, so before the app pushes a stock image or flashes a
//! Magisk-patched one it checks that:
//!
//! * the file actually looks like an Android boot image (stock and
//!   Magisk-patched images both start with the same magic), and
//! * the image goes back to the partition it came from.
//!
//! The second point matters because Magisk patches a different file depending
//! on the device: phones that ship an `init_boot` partition need
//! `init_boot.img` patched and flashed to `init_boot`; older layouts use
//! `boot.img` and `boot`. Mixing them up is exactly what causes boot loops.

use serde::{Deserialize, Serialize};

/// 8-byte magic every Android boot image begins with.
pub const BOOT_MAGIC: &[u8] = b"ANDROID!";

/// True if `head` (the first bytes of a file) starts with the boot-image magic.
pub fn looks_like_boot_image(head: &[u8]) -> bool {
    head.starts_with(BOOT_MAGIC)
}

/// The only partitions this app will ever flash a patched image to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootPartition {
    Boot,
    InitBoot,
}

impl BootPartition {
    /// Name to hand to `fastboot flash`.
    pub fn fastboot_name(self) -> &'static str {
        match self {
            BootPartition::Boot => "boot",
            BootPartition::InitBoot => "init_boot",
        }
    }

    /// Strict whitelist parse: anything but the exact names is rejected, so an
    /// arbitrary string from the UI can never become a partition name.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "boot" => Some(BootPartition::Boot),
            "init_boot" => Some(BootPartition::InitBoot),
            _ => None,
        }
    }
}

/// Decide which partition an image file belongs to from its file name
/// (`boot.img` → `boot`, `init_boot.img` → `init_boot`).
///
/// Paths with `/` or `\` separators are accepted and only the last component is
/// looked at. Anything else (`vendor_boot.img`, `system.img`, a renamed file…)
/// returns `None` so the caller can refuse instead of guessing.
pub fn partition_for_image(file_name: &str) -> Option<BootPartition> {
    let name = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_name)
        .to_ascii_lowercase();
    if name.starts_with("init_boot") {
        Some(BootPartition::InitBoot)
    } else if name.starts_with("boot") {
        Some(BootPartition::Boot)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_android_boot_magic() {
        let mut data = Vec::from(BOOT_MAGIC);
        data.extend_from_slice(&[0u8; 32]);
        assert!(looks_like_boot_image(&data));
    }

    #[test]
    fn rejects_other_content() {
        assert!(!looks_like_boot_image(b"PK\x03\x04 zip here"));
        assert!(!looks_like_boot_image(b"not a boot image"));
    }

    #[test]
    fn rejects_too_short() {
        assert!(!looks_like_boot_image(b"ANDRO"));
        assert!(!looks_like_boot_image(&[]));
    }

    #[test]
    fn routes_by_file_name() {
        assert_eq!(partition_for_image("boot.img"), Some(BootPartition::Boot));
        assert_eq!(partition_for_image("init_boot.img"), Some(BootPartition::InitBoot));
        assert_eq!(
            partition_for_image(r"C:\roms\garnet\images\INIT_BOOT.IMG"),
            Some(BootPartition::InitBoot)
        );
        assert_eq!(
            partition_for_image("/home/u/garnet/images/boot.img"),
            Some(BootPartition::Boot)
        );
    }

    #[test]
    fn refuses_unrelated_names() {
        assert_eq!(partition_for_image("vendor_boot.img"), None);
        assert_eq!(partition_for_image("system.img"), None);
        assert_eq!(partition_for_image("magisk_patched_x.img"), None);
        assert_eq!(partition_for_image(""), None);
    }

    #[test]
    fn partition_names_are_whitelisted() {
        assert_eq!(BootPartition::parse("boot"), Some(BootPartition::Boot));
        assert_eq!(BootPartition::parse("init_boot"), Some(BootPartition::InitBoot));
        assert_eq!(BootPartition::parse("system"), None);
        assert_eq!(BootPartition::parse("boot; echo"), None);
        assert_eq!(BootPartition::parse("Boot"), None);
        assert_eq!(BootPartition::Boot.fastboot_name(), "boot");
        assert_eq!(BootPartition::InitBoot.fastboot_name(), "init_boot");
    }
}
