//! Lightweight validation of Android boot images.
//!
//! Flashing the wrong file to the `boot` partition is the classic way to end
//! up in a boot loop, so before the app pushes a stock `boot.img` or flashes a
//! Magisk-patched image it checks the file actually looks like an Android boot
//! image. Both stock and Magisk-patched boot images start with the same magic.

/// 8-byte magic every Android boot image begins with.
pub const BOOT_MAGIC: &[u8] = b"ANDROID!";

/// True if `head` (the first bytes of a file) starts with the boot-image magic.
pub fn looks_like_boot_image(head: &[u8]) -> bool {
    head.starts_with(BOOT_MAGIC)
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
}
