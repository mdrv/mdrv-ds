use crate::pad::PadButton;
use crate::{dlog, Tx};
use std::io::{BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Events arriving from the mdrv-ds proxy bridge (pad/stick streamed while
/// the game is paused), a sibling `toggle` invocation, or the local evdev
/// reader (`local`).
#[derive(Clone, Copy, Debug)]
pub enum ShellEvent {
    Toggle,
    Pad {
        button: PadButton,
        pressed: bool,
        local: bool,
    },
    Stick {
        x: f32,
        y: f32,
    },
    /// Show the app's help (suite-wide PS-button chord reference). Fired by
    /// the mdrv-ds proxy's PS-tap / 3 s-hold actions over the toggle socket.
    Help,
    /// Graceful daemon exit (`daemon stop` clients send this when the
    /// socket is owned by another process).
    Stop,
}

pub fn runtime_dir() -> std::path::PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from("/run/user").join(format!("{}", unsafe { libc_getuid() }))
        })
}

unsafe fn libc_getuid() -> u32 {
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    getuid()
}

/// Owns `$XDG_RUNTIME_DIR/<app>.sock` (receives `toggle` lines from
/// sibling CLI invocations, e.g. an mdrv-ds chord) and connects to the
/// proxy's bridge socket while the overlay is visible — pausing game input
/// and receiving pad/stick events in its place.
pub struct Host {
    app: &'static str,
    conn: Arc<Mutex<Option<UnixStream>>>,
    paused: Arc<AtomicBool>,
}

impl Host {
    pub fn spawn<E: From<ShellEvent> + Send + 'static>(app: &'static str, ctrl_tx: Tx<E>) -> Self {
        // toggle socket (server)
        std::thread::spawn(move || loop {
            if let Err(e) = serve_toggle(app, &ctrl_tx) {
                eprintln!("{app}: toggle socket: {e}");
            }
            std::thread::sleep(Duration::from_secs(2));
        });

        Host {
            app,
            conn: Arc::new(Mutex::new(None)),
            paused: Arc::new(AtomicBool::new(false)),
        }
    }

    fn send_line(line: &[u8]) -> Option<UnixStream> {
        let path = runtime_dir().join("mdrv-ds-bridge.sock");
        let mut s = UnixStream::connect(path).ok()?;
        let _ = s.set_write_timeout(Some(Duration::from_millis(300)));
        s.write_all(line).ok()?;
        Some(s)
    }

    /// Tell the proxy to stop forwarding pad input to the game. Keeps the
    /// connection open and spawns a reader that turns streamed
    /// `pad <button> <p>` / `stick <x> <y>` lines into events.
    pub fn pause<E: From<ShellEvent> + Send + 'static>(&self, ctrl_tx: Tx<E>) {
        self.drop_conn(); // never leak a previous connection
        self.paused.store(true, Ordering::SeqCst);
        dlog!("bridge: pausing game input…");
        let Some(s) = Self::send_line(b"pause\n") else {
            eprintln!(
                "{}: proxy bridge unavailable — game keeps the pad",
                self.app
            );
            return;
        };
        let Ok(rd) = s.try_clone() else { return };
        *self.conn.lock().unwrap() = Some(s);
        let conn = self.conn.clone();
        let paused = self.paused.clone();
        std::thread::spawn(move || {
            stream_bridge(rd, &ctrl_tx);
            // disconnect implies resume (proxy auto-resumes on EOF too)
            *conn.lock().unwrap() = None;
            paused.store(false, Ordering::SeqCst);
        });
    }

    pub fn resume(&self) {
        if self.paused.swap(false, Ordering::SeqCst) {
            dlog!("bridge: resuming game input");
            // The proxy is presence-based: closing the connection IS the
            // resume signal (its EOF watcher clears PAUSED). Do NOT open a
            // new connection here — a fresh accept re-latches PAUSED=true
            // for a moment and races the teardown.
            self.drop_conn();
        }
    }

    /// Close the bridge connection for real: `shutdown(Both)` kills the
    /// whole socket (including the reader thread's `try_clone` fd), so its
    /// blocked read returns and the proxy observes EOF. A plain drop of one
    /// clone is NOT enough — the lingering clone kept the proxy's EOF
    /// watcher blocked forever (PAUSED latched true, game starved of input).
    fn drop_conn(&self) {
        if let Some(s) = self.conn.lock().unwrap().take() {
            let _ = s.shutdown(Shutdown::Both);
            // s dropped here; reader thread's clone unblocks and exits
        }
    }
}

fn stream_bridge<E: From<ShellEvent>>(rd: UnixStream, tx: &Tx<E>) {
    for line in BufReader::new(rd).lines() {
        let Ok(line) = line else { break };
        let mut it = line.split_whitespace();
        match (it.next(), it.next(), it.next()) {
            (Some("pad"), Some(name), Some(p)) => {
                if let Some(button) = name.parse::<PadButton>().ok() {
                    let _ = tx.unbounded_send(
                        ShellEvent::Pad {
                            button,
                            pressed: p != "0",
                            local: false,
                        }
                        .into(),
                    );
                }
            }
            (Some("stick"), Some(x), Some(y)) => {
                if let (Ok(x), Ok(y)) = (x.parse::<f32>(), y.parse::<f32>()) {
                    let _ = tx.unbounded_send(ShellEvent::Stick { x, y }.into());
                }
            }
            _ => {}
        }
    }
}

fn serve_toggle<E: From<ShellEvent>>(app: &str, tx: &Tx<E>) -> Result<(), String> {
    let path = runtime_dir().join(format!("{app}.sock"));
    let _ = std::fs::remove_file(&path);
    let listener = std::os::unix::net::UnixListener::bind(&path).map_err(|e| e.to_string())?;
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        for line in BufReader::new(stream).lines() {
            match line.as_deref() {
                Ok("toggle") => {
                    dlog!("toggle socket: toggle received");
                    let _ = tx.unbounded_send(ShellEvent::Toggle.into());
                }
                Ok("help") => {
                    dlog!("toggle socket: help received");
                    let _ = tx.unbounded_send(ShellEvent::Help.into());
                }
                Ok("stop") => {
                    dlog!("toggle socket: stop received");
                    let _ = tx.unbounded_send(ShellEvent::Stop.into());
                }
                Ok(_) | Err(_) => break,
            }
        }
    }
    Ok(())
}
