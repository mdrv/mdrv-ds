use crate::dlog;
use crate::host::runtime_dir;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::Duration;

/// CLI entry: `<app> daemon start [--foreground|-f]`. Detached by default;
/// foreground runs the daemon in this process (main falls through to normal
/// startup). Returns Err when already running.
pub fn cli_daemon_start(app: &str, foreground: bool) -> Result<(), String> {
    let path = runtime_dir().join(format!("{app}.sock"));
    if socket_live(&path) {
        return Err("daemon is already running".into());
    }
    if foreground {
        return Ok(()); // caller proceeds with normal startup
    }
    spawn_overlay().map_err(|e| format!("launch daemon: {e}"))?;
    for _ in 0..60 {
        std::thread::sleep(Duration::from_millis(50));
        if socket_live(&path) {
            println!("{app}: daemon started");
            return Ok(());
        }
    }
    Err("daemon did not come up".into())
}

/// Side-effect-free liveness check: connecting to a bound unix socket
/// succeeds and the daemon's per-connection reader just sees EOF. The
/// old probe SENT A REAL "toggle" — so every systemd respawn of a
/// desktop unit toggled whatever suite instance owned the socket (the
/// gamescope show/hide cycling, 2026-08-31).
fn socket_live(path: &std::path::Path) -> bool {
    std::os::unix::net::UnixStream::connect(path).is_ok()
}

/// CLI entry: `<app> daemon stop` — SIGTERM every matching process except
/// ourselves. Returns how many were signalled. /proc comm truncates at 15
/// bytes, so match on the app-name prefix.
pub fn cli_daemon_stop(app: &str) -> Result<usize, String> {
    let me = std::process::id();
    let prefix: String = app.chars().take(15).collect();
    let mut n = 0usize;
    let entries = std::fs::read_dir("/proc").map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if pid == me {
            continue;
        }
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        if comm.trim().starts_with(&prefix) {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            n += 1;
        }
    }
    Ok(n)
}

/// CLI entry: `<app> toggle` — poke a running overlay instance (e.g. from
/// an mdrv-ds PS+R2 chord). Auto-launches a detached overlay when none is
/// running (or only a stale socket remains).
pub fn cli_toggle(app: &str) -> Result<(), String> {
    let path = runtime_dir().join(format!("{app}.sock"));
    if send_toggle(&path).is_ok() {
        dlog!("toggle: delivered to running instance");
        return Ok(());
    }
    // Stale socket / no instance — start one and wait for it to bind.
    spawn_overlay().map_err(|e| format!("launch overlay: {e}"))?;
    let mut last = String::from("no instance was running");
    for _ in 0..60 {
        std::thread::sleep(Duration::from_millis(50));
        match send_toggle(&path) {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
    }
    Err(format!("overlay did not come up ({last})"))
}

pub(crate) fn send_toggle(path: &std::path::Path) -> Result<(), String> {
    send_line(path, "toggle")
}

/// CLI entry: `<app> help` — ask a running overlay instance to show its
/// help panel. Never spawns an instance (help only makes sense live).
pub fn cli_help(app: &str) -> Result<(), String> {
    let path = runtime_dir().join(format!("{app}.sock"));
    send_line(&path, "help").map_err(|e| format!("no running {app} instance ({e})"))
}

fn send_line(path: &std::path::Path, line: &str) -> Result<(), String> {
    let mut s =
        UnixStream::connect(path).map_err(|e| format!("connect {}: {e}", path.display()))?;
    let _ = s.set_write_timeout(Some(Duration::from_millis(300)));
    s.write_all(format!("{line}\n").as_bytes())
        .map_err(|e| format!("write: {e}"))
}

fn spawn_overlay() -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    // Foreground mode inside the detached child (falls through to startup).
    cmd.args(["daemon", "start", "--foreground"]);
    // Fully detach: own session/process group so callers (shells, systemd
    // units, chord children) never wait on the overlay daemon.
    cmd.process_group(0);
    // Chords inherit the proxy's environment; make sure the child can reach
    // the compositor even when WAYLAND_DISPLAY is absent there.
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        cmd.env("WAYLAND_DISPLAY", "wayland-1");
    }
    if std::env::var_os("XDG_RUNTIME_DIR").is_none() {
        cmd.env("XDG_RUNTIME_DIR", crate::host::runtime_dir());
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn()?;
    Ok(())
}
