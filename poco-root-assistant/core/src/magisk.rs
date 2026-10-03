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
}
