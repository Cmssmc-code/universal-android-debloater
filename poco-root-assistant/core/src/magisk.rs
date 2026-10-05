//! Helpers for locating the Magisk-patched boot image on the phone.
//!
//! After "Select and Patch a File" in the Magisk app, the result lands in
//! `/sdcard/Download/magisk_patched_<random>.img`. The app lists that folder
//! with `ls -t` (newest first) and feeds the output here.

/// Return every `magisk_patched*.img` entry, preserving input order.
///
/// Accepts both bare file names and full paths. Pass the output of
/// `ls -t /sdcard/Download/` so that index 0 is the most recently patched
/// image.
pub fn find_patched_images(ls_output: &str) -> Vec<String> {
    ls_output
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .filter(|l| {
            let name = l.rsplit('/').next().unwrap_or(l).to_ascii_lowercase();
            name.starts_with("magisk_patched") && name.ends_with(".img")
        })
        .map(|l| l.to_string())
        .collect()
}

/// Convenience: the newest patched image, if any.
pub fn latest_patched_image(ls_output: &str) -> Option<String> {
    find_patched_images(ls_output).into_iter().next()
}

/// Pick the real Magisk app from a GitHub release's asset names.
///
/// A release can carry several `.apk` files (the app itself plus helper builds
/// such as a stub or a debug build), so taking "the first .apk" can install the
/// wrong one. Accept only names that start with `magisk`, end with `.apk` and
/// are not a stub/debug build. Returns the first match, or `None` so the caller
/// can fall back to a pinned known-good download.
pub fn pick_magisk_apk<'a>(asset_names: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    asset_names.into_iter().find(|name| {
        let n = name.to_ascii_lowercase();
        n.starts_with("magisk")
            && n.ends_with(".apk")
            && !n.contains("stub")
            && !n.contains("debug")
    })
}

/// True if `head` starts with the zip signature every APK has (`PK\x03\x04`).
/// Used to reject an HTML error page or a truncated download saved as an APK.
pub fn looks_like_apk(head: &[u8]) -> bool {
    head.starts_with(b"PK\x03\x04")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_patched_images_newest_first() {
        let out = "magisk_patched_9F3kd.img\n\
                   magisk_patched_aa11B.img\n\
                   Telegram/\n\
                   screenshot.png\n\
                   boot.img\n";
        let imgs = find_patched_images(out);
        assert_eq!(imgs.len(), 2);
        assert_eq!(latest_patched_image(out).as_deref(), Some("magisk_patched_9F3kd.img"));
    }

    #[test]
    fn handles_full_paths_and_case() {
        let out = "/sdcard/Download/Magisk_Patched_XYZ.IMG\n";
        assert_eq!(find_patched_images(out).len(), 1);
    }

    #[test]
    fn none_when_absent() {
        assert_eq!(latest_patched_image("boot.img\nvbmeta.img\n"), None);
    }

    #[test]
    fn picks_the_real_magisk_apk() {
        let names = ["app-debug.apk", "stub-release.apk", "uninstall.zip", "Magisk-v28.1.apk"];
        assert_eq!(pick_magisk_apk(names), Some("Magisk-v28.1.apk"));
    }

    #[test]
    fn refuses_stub_and_debug_builds() {
        assert_eq!(pick_magisk_apk(["stub-release.apk", "app-debug.apk"]), None);
        assert_eq!(pick_magisk_apk(["Magisk-stub.apk", "Magisk-debug.apk"]), None);
        assert_eq!(pick_magisk_apk(["notes.txt"]), None);
    }

    #[test]
    fn recognises_apk_zip_signature() {
        assert!(looks_like_apk(b"PK\x03\x04rest"));
        assert!(!looks_like_apk(b"<!DOCTYPE html>"));
        assert!(!looks_like_apk(b"PK"));
        assert!(!looks_like_apk(&[]));
    }
}
