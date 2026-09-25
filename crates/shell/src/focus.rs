//! Input-focus arbitration between mdrv-ds overlays.
//!
//! Every daemon runs its own evdev reader on the same (virtual) pad node,
//! so with two overlays open both would react to gamepad input. The shown
//! overlay CLAIMS input focus (a tiny owner file in the runtime dir);
//! non-owners drop pad events in the shell reader. Releasing on hide lets
//! the underlying overlay (e.g. the launcher carousel) regain input.

use std::io::Write;
use std::path::PathBuf;

fn focus_path() -> PathBuf {
    crate::host::runtime_dir().join("mdrv-ds-input-focus")
}

fn owner_line() -> Option<String> {
    std::fs::read_to_string(focus_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn split(line: &str) -> Option<(&str, i32)> {
    let (app, pid) = line.rsplit_once(' ')?;
    let pid = pid.parse::<i32>().ok()?;
    Some((app, pid))
}

fn owner_alive(line: &str) -> bool {
    match split(line) {
        Some((_, pid)) => {
            let rc = unsafe { libc::kill(pid, 0) };
            rc == 0
                || rc == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        }
        None => false, // malformed line: treat as no owner
    }
}

/// Claim input focus for `app` (last shown wins).
pub fn claim(app: &str) {
    let line = format!("{app} {}", std::process::id());
    let path = focus_path();
    let tmp = path.with_extension("focus.tmp");
    if let Ok(mut f) = std::fs::File::create(&tmp) {
        if f.write_all(line.as_bytes()).is_ok() && f.sync_all().is_ok() {
            let _ = std::fs::rename(&tmp, &path);
            return;
        }
    }
    eprintln!("{app}: focus claim failed");
}

/// Release focus, but only if `app` still owns it.
pub fn release(app: &str) {
    if let Some(line) = owner_line() {
        if split(&line).is_some_and(|(owner, _)| owner == app) {
            let _ = std::fs::remove_file(focus_path());
        }
    }
}

/// May `app` receive local pad events? True when no live owner exists or
/// the owner IS `app`.
pub fn is_owner(app: &str) -> bool {
    match owner_line() {
        None => true,
        Some(line) => !owner_alive(&line) || split(&line).is_some_and(|(o, _)| o == app),
    }
}
