//! mdrv-ds-clock — on-demand full-screen digital clock (screensaver style).
//!
//! Not a daemon: `mdrv-ds-clock toggle` (also the bare default) either
//! summons the clock (spawning a detached compositor client) or kills the
//! running one. Bound to a DualSense PS-chord in `config.toml [chords]`
//! and a Hyprland key in `~/.config/hypr/hyprland.lua`.
//!
//! Surface: transparent layer-shell overlay (above fullscreen games on
//! desktop compositors), permanently click-through with no keyboard grab
//! — it never disturbs the game; the toggle verbs are the only way out.
//!
//! Config: `~/.config/mdrv-ds/clock.toml` (all optional):
//!
//! ```toml
//! seconds    = true          # :SS suffix (opt out with false)
//! hour12     = false         # 24-hour notation by default
//! gmt_offset = "+07:00"      # omit = local time; also `7` / `"-0530"`
//! color      = "#f2f2f2"     # RRGGBB or RRGGBBAA
//! font       = "B612 Mono"   # family name, or an absolute .ttf/.otf path
//! size       = 0.28          # text height as a fraction of screen height
//! weight     = 400           # 100 thin .. 900 black (nearest face wins)
//! tabular    = true          # fixed-width digits (OpenType `tnum`); opt
//!                           # out with false if the font looks wrong
//! ```

use std::io::Write as _;

use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
use gpui::prelude::*;
use gpui::{
    div, px, rgba, size, App, Bounds, FontFeatures, FontWeight, Rgba, SharedString,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
};
use mdrv_ds_shell::clock::{ClockFmt, ClockSection};
use mdrv_ds_shell::host::runtime_dir;

const APP: &str = "mdrv-ds-clock";

fn main() {
    match std::env::args().nth(1).as_deref() {
        None | Some("toggle") => toggle(),
        Some("stop") => stop(),
        Some("show") => show(),
        Some("help") | Some("--help") | Some("-h") => {
            println!(
                "{APP} — on-demand full-screen digital clock\n\
                 \x20 {APP} [toggle]   show the clock, or kill it if shown\n\
                 \x20 {APP} stop       kill a shown clock\n\
                 \x20 {APP} show       run the clock in this process (used by toggle)\n\
                 Config: ~/.config/mdrv-ds/clock.toml"
            );
        }
        Some(other) => {
            eprintln!("{APP}: unknown verb {other:?} (try: toggle | stop | show)");
            std::process::exit(2);
        }
    }
}

// ---- CLI verbs --------------------------------------------------------------

fn sock_path() -> std::path::PathBuf {
    runtime_dir().join(format!("{APP}.sock"))
}

fn send_line(line: &str) -> Result<(), String> {
    let mut s = std::os::unix::net::UnixStream::connect(sock_path())
        .map_err(|e| format!("connect: {e}"))?;
    let _ = s.set_write_timeout(Some(std::time::Duration::from_millis(300)));
    s.write_all(format!("{line}\n").as_bytes()).map_err(|e| format!("write: {e}"))
}

/// Toggle: a live instance dies, otherwise a detached one is summoned.
fn toggle() {
    if send_line("stop").is_ok() {
        println!("{APP}: hidden");
        return;
    }
    if let Err(e) = spawn_detached() {
        eprintln!("{APP}: {e}");
        std::process::exit(1);
    }
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if send_line("ping").is_ok() {
            println!("{APP}: shown");
            return;
        }
    }
    eprintln!("{APP}: clock did not come up");
    std::process::exit(1);
}

fn stop() {
    match send_line("stop") {
        Ok(()) => println!("{APP}: hidden"),
        Err(e) => {
            eprintln!("{APP}: no running clock ({e})");
            std::process::exit(1);
        }
    }
}

/// Detached `show` child: own process group, no controlling tty — chord
/// children and keybinds must never wait on the clock process.
fn spawn_detached() -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.arg("show");
    cmd.process_group(0);
    // Chords/keybinds may run without a compositor env; mirror the suite's
    // daemon spawner fallbacks so the clock always reaches the session.
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        cmd.env("WAYLAND_DISPLAY", "wayland-1");
    }
    if std::env::var_os("XDG_RUNTIME_DIR").is_none() {
        cmd.env("XDG_RUNTIME_DIR", runtime_dir());
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn()?;
    Ok(())
}

// ---- config -----------------------------------------------------------------

#[derive(Debug, Clone)]
struct ClockCfg {
    fmt: ClockFmt,
    color: Rgba,
    /// Raw `font` value: family name or absolute font-file path.
    font: Option<String>,
    /// Text height as a fraction of screen height.
    size_frac: f32,
    /// Font weight (100 thin .. 900 black, 400 normal). The text system
    /// picks the closest face the family actually ships.
    weight: FontWeight,
    /// Fixed-width digits via the OpenType `tnum` feature, so the string
    /// width never jitters between ticks. Fonts without tabular figures
    /// (and mono fonts, which need none) simply ignore it.
    tabular: bool,
}

fn clock_toml_path() -> std::path::PathBuf {
    mdrv_ds_shell::conf::config_path().with_file_name("clock.toml")
}

impl ClockCfg {
    fn load() -> Self {
        let text = std::fs::read_to_string(clock_toml_path()).unwrap_or_default();
        let sec: ClockSection =
            toml::from_str(&text).unwrap_or_default();
        let fmt = sec.to_fmt();

        #[derive(serde::Deserialize, Default)]
        #[serde(default)]
        struct Extra {
            color: Option<String>,
            font: Option<String>,
            size: Option<f32>,
            weight: Option<f32>,
            tabular: Option<bool>,
        }
        let extra: Extra = toml::from_str(&text).unwrap_or_default();

        Self {
            fmt,
            color: extra.color.as_deref().and_then(parse_hex).unwrap_or(rgba(0xFFFF_FFFF)),
            font: extra.font.filter(|f| !f.trim().is_empty()),
            size_frac: extra.size.unwrap_or(0.28).clamp(0.05, 0.60),
            weight: FontWeight(extra.weight.unwrap_or(400.0).clamp(100.0, 900.0)),
            tabular: extra.tabular.unwrap_or(true),
        }
    }
}

/// `"#RRGGBB"` / `"#RRGGBBAA"` / `"RRGGBB"` → gpui color.
fn parse_hex(s: &str) -> Option<Rgba> {
    let h = s.strip_prefix('#').unwrap_or(s);
    if h.len() != 6 && h.len() != 8 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    let packed = if h.len() == 8 { v } else { v << 8 | 0xFF };
    Some(rgba(packed))
}

// ---- control socket ---------------------------------------------------------

/// Serve `{APP}.sock`: any `stop`/`toggle` line ends the process; other
/// lines are just liveness pings.
fn spawn_ctrl_socket() {
    use std::io::BufRead;
    let path = sock_path();
    let _ = std::fs::remove_file(&path);
    let listener = match std::os::unix::net::UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{APP}: socket bind failed: {e}");
            return;
        }
    };
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(stream) = conn else { continue };
            for line in std::io::BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                match line.trim() {
                    "stop" | "toggle" | "quit" => {
                        let _ = std::fs::remove_file(sock_path());
                        std::process::exit(0);
                    }
                    _ => {}
                }
            }
        }
    });
}

// ---- GPUI app ---------------------------------------------------------------

fn in_gamescope() -> bool {
    if std::env::var("MDRV_DS_SESSION").is_ok_and(|v| v == "gamescope") {
        return true;
    }
    std::env::var("WAYLAND_DISPLAY")
        .map(|v| v.starts_with("gamescope"))
        .unwrap_or(false)
}

struct ClockView {
    cfg: ClockCfg,
    /// Resolved family (None = gpui default font).
    family: Option<SharedString>,
    text: String,
}

impl ClockView {
    fn new(cfg: ClockCfg, family: Option<SharedString>, cx: &mut Context<Self>) -> Self {
        let text = cfg.fmt.now();
        let v = Self { cfg, family, text };
        // Real-time ticker: 4 Hz is plenty for a seconds display; only
        // re-render when the string actually changed.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(250))
                .await;
            if this
                .update(cx, |s, cx| {
                    let t = s.cfg.fmt.now();
                    if t != s.text {
                        s.text = t;
                        cx.notify();
                    }
                })
                .is_err()
            {
                return; // window gone — stop ticking
            }
        })
        .detach();
        v
    }
}

impl Render for ClockView {
    fn render(&mut self, window: &mut gpui::Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Scale the text off the live surface size: layer-shell anchors
        // stretch us to the output, and bounds() reflects that.
        let h = f32::from(window.bounds().size.height).max(1.0);
        let size = px(h * self.cfg.size_frac);
        let mut el = div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(size)
            .line_height(px(f32::from(size) * 1.2))
            .font_weight(self.cfg.weight)
            .text_color(self.cfg.color)
            .child(self.text.clone());
        if self.cfg.tabular {
            el = el.font_features(FontFeatures(std::sync::Arc::new(vec![(
                "tnum".into(),
                1,
            )])));
        }
        if let Some(f) = &self.family {
            el = el.font_family(f.clone());
        }
        el
    }
}

fn show() {
    eprintln!("{APP}: starting (pid {})", std::process::id());
    spawn_ctrl_socket();

    let cfg = ClockCfg::load();
    let font_spec = cfg.font.clone();
    gpui_platform::application().run(move |cx: &mut App| {
        let family = resolve_font(cx, font_spec.as_deref());
        let view_cfg = cfg.clone();
        let view_family = family.clone();

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: gpui::point(px(0.), px(0.)),
                // All-four anchors + 0x0 = compositor-stretched fullscreen.
                size: size(px(0.), px(0.)),
            })),
            window_background: WindowBackgroundAppearance::Transparent,
            show: true,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: APP.into(),
                // Overlay sits above fullscreen games on desktop
                // compositors; Top is the gamescope-safe layer.
                layer: if in_gamescope() { Layer::Top } else { Layer::Overlay },
                // Passive screensaver: never take the keyboard, never
                // catch the pointer — the toggle verbs are the only exit.
                keyboard_interactivity: KeyboardInteractivity::None,
                anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                ..Default::default()
            }),
            ..Default::default()
        };

        let _ = cx
            .open_window(options, move |window, _cx| {
                // Click-through from the first frame.
                window.set_input_region(Some(&[]));
                _cx.new(|cx| ClockView::new(view_cfg, view_family, cx))
            })
            .expect("open clock window");

        eprintln!("{APP}: window opened");
    });
}

// ---- font resolution ---------------------------------------------------------

/// Resolve the configured font to a family name:
/// * unset  → first available of a mono-first candidate list
///   (stable digits matter for a clock)
/// * `/abs/path.ttf` → bytes loaded via `add_fonts`, family detected from
///   the before/after `all_font_names` diff
/// * other  → used verbatim as a family name
fn resolve_font(cx: &App, configured: Option<&str>) -> Option<SharedString> {
    let ts = cx.text_system();
    if let Some(spec) = configured {
        if spec.starts_with('/') {
            let before = ts.all_font_names();
            let bytes = std::fs::read(spec).ok()?;
            let _ = ts.add_fonts(vec![std::borrow::Cow::Owned(bytes)]);
            let fresh = ts
                .all_font_names()
                .into_iter()
                .find(|n| !before.contains(n))?;
            eprintln!("{APP}: loaded font file {spec} (family {fresh:?})");
            return Some(fresh.into());
        }
        return Some(spec.into());
    }
    const MONO_CANDIDATES: [&str; 7] = [
        "B612 Mono",
        "DejaVu Sans Mono",
        "DM Mono",
        "Noto Sans Mono",
        "Liberation Mono",
        "JetBrains Mono",
        "Adwaita Mono",
    ];
    let names = ts.all_font_names();
    MONO_CANDIDATES
        .iter()
        .find(|c| names.iter().any(|n| n == *c))
        .map(|c| (*c).into())
}
