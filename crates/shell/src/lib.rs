//! Shared plumbing for the mdrv-ds overlay daemons: pad reading (evdev),
//! the proxy bridge host (toggle socket + game-input pause/resume), daemon
//! CLI verbs, the suite theme palette, and numpad geometry helpers.
//!
//! Every helper is generic over the app's event enum `E: From<ShellEvent>`
//! and takes the app name (e.g. `"mdrv-ds-audio"`) which names the toggle
//! socket `$XDG_RUNTIME_DIR/<app>.sock` and prefixes diagnostics.

pub mod clock;
pub mod conf;
pub mod geom;

// Linux-only suite glue: unix-socket daemon control, evdev pad reading,
// focus arbitration via signals.
#[cfg(target_os = "linux")]
pub mod daemon;
#[cfg(target_os = "linux")]
pub mod focus;
#[cfg(target_os = "linux")]
pub mod host;
#[cfg(target_os = "linux")]
pub mod pad;

#[cfg(target_os = "linux")]
pub use host::ShellEvent;
#[cfg(target_os = "linux")]
pub use pad::PadButton;

/// Event-channel sender type shared by every shell helper.
pub type Tx<E> = futures::channel::mpsc::UnboundedSender<E>;

#[macro_export]
macro_rules! dlog {
    ($($arg:tt)*) => {
        if std::env::var_os("MDRV_DS_SHELL_DEBUG").is_some() {
            eprintln!($($arg)*);
        }
    };
}
