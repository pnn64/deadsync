// Behavior summary:
// * Windows: `shutdown.exe /s /t 0`
// * macOS: `osascript` driving System Events
// * Linux: `systemctl poweroff`
// * FreeBSD: `shutdown -p now`. Typically requires root
//
// Callers should call this *before* exiting the application's event loop so
// the OS shutdown can proceed in parallel with normal app teardown.

use std::io;

/// Requests an OS-level shutdown. Returns once the platform command has been
/// spawned (success) or the spawn has failed.
pub fn shutdown_host() -> io::Result<()> {
    spawn_shutdown()
}

#[cfg(target_os = "windows")]
fn spawn_shutdown() -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB.
    const DETACH_FLAGS: u32 = 0x0000_0008 | 0x0000_0200 | 0x0100_0000;

    let mut cmd = Command::new("shutdown.exe");
    cmd.args(["/s", "/t", "0"]);
    cmd.creation_flags(DETACH_FLAGS);
    spawn_cmd(cmd)
}

#[cfg(target_os = "macos")]
fn spawn_shutdown() -> io::Result<()> {
    use std::process::Command;

    let mut cmd = Command::new("osascript");
    cmd.args(["-e", "tell application \"System Events\" to shut down"]);
    spawn_cmd(cmd)
}

#[cfg(target_os = "linux")]
fn spawn_shutdown() -> io::Result<()> {
    use std::process::Command;

    let mut cmd = Command::new("systemctl");
    cmd.arg("poweroff");
    spawn_cmd(cmd)
}

#[cfg(target_os = "freebsd")]
fn spawn_shutdown() -> io::Result<()> {
    use std::process::Command;

    let mut cmd = Command::new("shutdown");
    cmd.args(["-p", "now"]);
    spawn_cmd(cmd)
}

#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd"
)))]
fn spawn_shutdown() -> io::Result<()> {
    log::warn!("power::shutdown_host is not implemented for this platform");
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "shutdown not implemented for this platform",
    ))
}

#[cfg(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd"
))]
fn spawn_cmd(mut cmd: std::process::Command) -> io::Result<()> {
    use std::process::Stdio;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match cmd.spawn() {
        Ok(_child) => Ok(()),
        Err(e) => {
            log::warn!("failed to spawn shutdown command: {e}");
            Err(e)
        }
    }
}
