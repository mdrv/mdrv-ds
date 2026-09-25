use crate::host::ShellEvent;
use crate::Tx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadButton {
    Up,
    Down,
    Left,
    Right,
    Cross,
    Circle,
    Square,
    Triangle,
    Options,
    Share,
    L1,
    R1,
    L3,
    R3,
    PS,
}

#[derive(Clone, Copy, Debug)]
pub struct PadEvent {
    pub button: PadButton,
    pub pressed: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PadStick {
    pub x: f32,
    pub y: f32,
}

impl std::str::FromStr for PadButton {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "up" => PadButton::Up,
            "down" => PadButton::Down,
            "left" => PadButton::Left,
            "right" => PadButton::Right,
            "cross" => PadButton::Cross,
            "circle" => PadButton::Circle,
            "square" => PadButton::Square,
            "triangle" => PadButton::Triangle,
            "share" => PadButton::Share,
            "options" => PadButton::Options,
            "l1" => PadButton::L1,
            "r1" => PadButton::R1,
            "l3" => PadButton::L3,
            "r3" => PadButton::R3,
            "ps" | "home" => PadButton::PS,
            _ => return Err(()),
        })
    }
}

impl PadButton {
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            PadButton::Up => "up",
            PadButton::Down => "down",
            PadButton::Left => "left",
            PadButton::Right => "right",
            PadButton::Cross => "cross",
            PadButton::Circle => "circle",
            PadButton::Square => "square",
            PadButton::Triangle => "triangle",
            PadButton::Share => "share",
            PadButton::Options => "options",
            PadButton::L1 => "l1",
            PadButton::R1 => "r1",
            PadButton::L3 => "l3",
            PadButton::R3 => "r3",
            PadButton::PS => "ps",
        }
    }
}

const BTN_SOUTH: u16 = 0x130; // cross
const BTN_EAST: u16 = 0x131; // circle
const BTN_NORTH: u16 = 0x133; // triangle
const BTN_WEST: u16 = 0x134; // square
const BTN_TL: u16 = 0x136; // L1
const BTN_TR: u16 = 0x137; // R1
const BTN_SELECT: u16 = 0x13a; // share
const BTN_START: u16 = 0x13b; // options
const BTN_THUMBL: u16 = 0x13d;
const BTN_THUMBR: u16 = 0x13e;
const KEY_HOMEPAGE: u16 = 0x12f; // PS button over BT

const ABS_X: u16 = 0x00; // left stick
const ABS_Y: u16 = 0x01;
const ABS_HAT0X: u16 = 0x10;
const ABS_HAT0Y: u16 = 0x11;

fn map_key(code: u16) -> Option<PadButton> {
    Some(match code {
        BTN_SOUTH => PadButton::Cross,
        BTN_EAST => PadButton::Circle,
        BTN_WEST => PadButton::Square,
        BTN_NORTH => PadButton::Triangle,
        BTN_SELECT => PadButton::Share,
        BTN_START => PadButton::Options,
        BTN_TL => PadButton::L1,
        BTN_TR => PadButton::R1,
        BTN_THUMBL => PadButton::L3,
        BTN_THUMBR => PadButton::R3,
        KEY_HOMEPAGE => PadButton::PS,
        _ => return None,
    })
}

fn map_hat(axis: u16, value: i32) -> Option<PadEvent> {
    let pressed = value != 0;
    let button = match (axis, value.signum()) {
        (ABS_HAT0X, -1) => PadButton::Left,
        (ABS_HAT0X, 1) => PadButton::Right,
        (ABS_HAT0Y, -1) => PadButton::Up,
        (ABS_HAT0Y, 1) => PadButton::Down,
        // release events carry signum 0 — emit release for the axis' neutral pair
        (ABS_HAT0X, _) => {
            return Some(PadEvent {
                button: PadButton::Left,
                pressed: false,
            })
        }
        _ => {
            return Some(PadEvent {
                button: PadButton::Up,
                pressed: false,
            })
        }
    };
    Some(PadEvent { button, pressed })
}

fn send_button<E: From<ShellEvent>>(tx: &Tx<E>, button: PadButton, pressed: bool) {
    let _ = tx.unbounded_send(
        ShellEvent::Pad {
            button,
            pressed,
            local: true,
        }
        .into(),
    );
}

fn send_stick<E: From<ShellEvent>>(tx: &Tx<E>, st: PadStick) {
    let _ = tx.unbounded_send(ShellEvent::Stick { x: st.x, y: st.y }.into());
}

/// Spawn the evdev reader thread for the first DualSense-shaped device.
/// Retries every 2 s when no pad node exists yet (pads connect late).
pub fn spawn<E: From<ShellEvent> + Send + 'static>(app: &'static str, tx: Tx<E>) {
    std::thread::spawn(move || loop {
        if let Err(e) = run(app, &tx) {
            eprintln!("{app}: pad reader idle: {e}");
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    });
}

fn run<E: From<ShellEvent>>(app: &str, tx: &Tx<E>) -> Result<(), String> {
    use evdev::{Device, KeyCode};

    let mut dev: Option<Device> = None;
    for entry in evdev::enumerate() {
        let (_, d) = entry;
        let name = d.name().unwrap_or("").to_lowercase();
        let keys = d.supported_keys();
        // The virtual pad exposes sibling nodes ("… Touchpad", "… Motion
        // Sensors") whose names ALSO contain "dualsense" — a loose name
        // match grabs whichever enumerates first (readdir order changes
        // across boots). Require actual gamepad buttons and skip the
        // auxiliary nodes by name.
        let is_pad = (name.contains("dualsense") || name.contains("wireless controller"))
            && !name.contains("touchpad")
            && !name.contains("motion")
            && !name.contains("sensor")
            && keys.map_or(false, |k| k.contains(KeyCode::BTN_EAST));
        if is_pad && dev.is_none() {
            dev = Some(d);
        }
    }
    let mut dev = dev.ok_or("no DualSense gamepad node found")?;
    let mut st = PadStick { x: 0.0, y: 0.0 };

    loop {
        // Input-focus gate: with several overlays open, only the one that
        // claimed focus may react to local pad input (checked once per
        // event batch — cheap and plenty granular).
        let allowed = crate::focus::is_owner(app);
        for ev in dev.fetch_events().map_err(|e| e.to_string())? {
            if !allowed {
                continue;
            }
            match ev.destructure() {
                evdev::EventSummary::Key(_, code, val) => {
                    if let Some(b) = map_key(code.0) {
                        send_button(tx, b, val != 0);
                    }
                }
                evdev::EventSummary::AbsoluteAxis(_, axis, val) => match axis.0 {
                    ABS_X | ABS_Y => {
                        let v = (val as f32 / 32767.0).clamp(-1.0, 1.0);
                        if axis.0 == ABS_X {
                            st.x = v;
                        } else {
                            st.y = v;
                        }
                        send_stick(tx, st);
                    }
                    ABS_HAT0X | ABS_HAT0Y => {
                        if let Some(e) = map_hat(axis.0, val) {
                            send_button(tx, e.button, e.pressed);
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
}
