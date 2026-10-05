//! Thin, platform-aware runner for the `adb` / `fastboot` executables.
//!
//! All parsing of the output lives in the `pra_core` crate; this module only
//! spawns the process and hands back the captured streams.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

/// Captured result of a single `adb` / `fastboot` invocation.
pub struct CmdOutput {
    pub stdout: String,
    pub stderr: String,
    pub ok: bool,
}

/// Build a `Command` that never flashes a console window on Windows.
fn base(exe: &Path) -> Command {
    // `mut` is only needed on Windows (for creation_flags below).
    #[allow(unused_mut)]
    let mut c = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: keep adb/fastboot from popping cmd windows.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Run `exe` with `args`, capturing stdout/stderr as lossy UTF-8.
pub fn run<I, S>(exe: &Path, args: I) -> std::io::Result<CmdOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let out = base(exe).args(args).output()?;
    Ok(CmdOutput {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        ok: out.status.success(),
    })
}
