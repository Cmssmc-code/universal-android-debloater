//! Pure logic for the Poco Root Assistant.
//!
//! This crate contains **no** I/O: it only parses the textual output of
//! `adb` / `fastboot` and describes the rooting flow. Keeping it pure makes
//! every important decision in the app unit-testable on any platform, which
//! matters a lot for a tool that flashes partitions on a real phone.
//!
//! The Tauri layer (`src-tauri`) is the only place that actually spawns
//! processes and talks to the network; it delegates all parsing and
//! decision-making to the functions here.

pub mod device;
pub mod fastboot;
pub mod flow;
pub mod magisk;

pub use device::{parse_adb_devices, parse_getprop, AdbDevice, AdbState, DeviceInfo, SupportLevel};
pub use fastboot::{parse_fastboot_devices, unlock_state, UnlockState};
pub use flow::{root_plan, Automation, Step};
pub use magisk::find_patched_images;

/// Codename of the POCO X6 5G (shared with the Redmi Note 13 Pro 5G).
pub const TARGET_CODENAME: &str = "garnet";

/// Model-number prefix of the POCO X6 5G global variant (23122PCD1G, etc.).
pub const TARGET_MODEL_PREFIX: &str = "23122PCD";
