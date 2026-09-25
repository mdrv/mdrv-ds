//! Shared clock formatting for the suite: the overlay panel footer clock
//! and the standalone `mdrv-ds-clock` screensaver both render time
//! through [`ClockFmt`].
//!
//! * The overlay reads the shared `[clock]` table from
//!   `~/.config/mdrv-ds/config.toml` ([`load_fmt`]).
//! * `mdrv-ds-clock` keeps its own `~/.config/mdrv-ds/clock.toml` and
//!   feeds the same [`ClockFmt`] from there.
//!
//! Defaults: local time, 24-hour, seconds shown (each can be opted out).

use serde::Deserialize;

/// Time-display options shared by both clock surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockFmt {
    /// Show `:SS`. Default `true` (opt OUT with `seconds = false`).
    pub seconds: bool,
    /// 12-hour AM/PM notation. Default `false` (24-hour).
    pub hour12: bool,
    /// Fixed GMT offset in minutes (`+07:30` = 450, `-05` = -300).
    /// `None` = local time (`localtime_r`, honors `$TZ`/`/etc/localtime`).
    pub offset_min: Option<i32>,
}

impl Default for ClockFmt {
    fn default() -> Self {
        Self {
            seconds: true,
            hour12: false,
            offset_min: None,
        }
    }
}

impl ClockFmt {
    /// Current wall time, formatted per these options.
    pub fn now(&self) -> String {
        unsafe {
            let t = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            let ok = match self.offset_min {
                // Fixed offset: shift the epoch, then read it as UTC —
                // that yields the offset zone's wall time.
                Some(off) => {
                    let shifted = t + (off as i64) * 60;
                    !libc::gmtime_r(&shifted, &mut tm).is_null()
                }
                None => !libc::localtime_r(&t, &mut tm).is_null(),
            };
            if !ok {
                return "--:--".into();
            }
            let (h, m, s) = (tm.tm_hour, tm.tm_min, tm.tm_sec);
            if self.hour12 {
                let ampm = if h < 12 { "AM" } else { "PM" };
                let h12 = match h % 12 {
                    0 => 12,
                    v => v,
                };
                if self.seconds {
                    format!("{h12:02}:{m:02}:{s:02} {ampm}")
                } else {
                    format!("{h12:02}:{m:02} {ampm}")
                }
            } else if self.seconds {
                format!("{h:02}:{m:02}:{s:02}")
            } else {
                format!("{h:02}:{m:02}")
            }
        }
    }
}

/// `[clock]` section shape (all-optional; absent table = defaults).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ClockSection {
    pub seconds: Option<bool>,
    pub hour12: Option<bool>,
    /// Integer hours (`7`, `-5`) or a string (`"+07:00"`, `"-0530"`, `"+9"`).
    pub gmt_offset: Option<OffsetValue>,
}

/// `gmt_offset` accepts both `7` (hours) and `"+07:30"` (string) forms.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OffsetValue {
    Hours(i64),
    Text(String),
}

impl ClockSection {
    pub fn to_fmt(&self) -> ClockFmt {
        let mut f = ClockFmt::default();
        if let Some(v) = self.seconds {
            f.seconds = v;
        }
        if let Some(v) = self.hour12 {
            f.hour12 = v;
        }
        match &self.gmt_offset {
            Some(OffsetValue::Hours(h)) => f.offset_min = Some((*h as i32).saturating_mul(60)),
            Some(OffsetValue::Text(s)) => f.offset_min = parse_offset(s),
            None => {}
        }
        f
    }
}

/// `[clock]` from the shared config; defaults when the table is absent.
pub fn load_fmt() -> ClockFmt {
    crate::conf::load_section::<ClockSection>("clock")
        .map(|s| s.to_fmt())
        .unwrap_or_default()
}

/// Parse a GMT offset: `"+07:00"`, `"-0530"`, `"+9"`, `"7"` → minutes.
/// Range-clamped to ±24 h. Returns `None` on garbage (caller keeps local).
pub fn parse_offset(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (sign, rest) = match s.as_bytes()[0] {
        b'+' => (1i32, &s[1..]),
        b'-' => (-1i32, &s[1..]),
        _ => (1i32, s),
    };
    let (hh, mm) = match rest.split_once(':') {
        Some((h, m)) => (h.parse::<i32>().ok()?, m.parse::<i32>().ok()?),
        None => match rest.len() {
            1 | 2 => (rest.parse::<i32>().ok()?, 0),
            // HHMM without the colon
            3 | 4 => (
                rest[..rest.len() - 2].parse::<i32>().ok()?,
                rest[rest.len() - 2..].parse::<i32>().ok()?,
            ),
            _ => return None,
        },
    };
    if !(0..24).contains(&hh) || mm > 59 {
        return None;
    }
    Some((sign * (hh * 60 + mm)).clamp(-24 * 60, 24 * 60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_forms() {
        assert_eq!(parse_offset("+07:00"), Some(420));
        assert_eq!(parse_offset("-5"), Some(-300));
        assert_eq!(parse_offset("+0530"), Some(330));
        assert_eq!(parse_offset("9"), Some(540));
        assert_eq!(parse_offset("-00:30"), Some(-30));
        assert_eq!(parse_offset("+25"), None, "out of range");
        assert_eq!(parse_offset("junk"), None);
        assert_eq!(parse_offset(""), None);
    }

    #[test]
    fn section_merges_defaults() {
        let s: ClockSection = toml::from_str("hour12 = true").unwrap();
        let f = s.to_fmt();
        assert!(f.seconds && f.hour12 && f.offset_min.is_none());
        let s: ClockSection = toml::from_str("seconds = false\ngmt_offset = 7").unwrap();
        let f = s.to_fmt();
        assert!(!f.seconds && !f.hour12 && f.offset_min == Some(420));
        let s: ClockSection = toml::from_str("gmt_offset = \"-05:30\"").unwrap();
        assert_eq!(s.to_fmt().offset_min, Some(-330));
    }

    #[test]
    fn formatting() {
        // UTC with a fixed offset exercises gmtime_r deterministically;
        // local time depends on the host zone.
        unsafe { std::env::set_var("TZ", "UTC0") };
        let f = ClockFmt {
            seconds: true,
            hour12: false,
            offset_min: Some(0),
        };
        let t = f.now();
        assert_eq!(t.len(), 8, "HH:MM:SS, got {t}");
        assert_eq!(&t[2..3], ":");
        let f = ClockFmt {
            seconds: false,
            hour12: false,
            offset_min: Some(0),
        };
        assert_eq!(f.now().len(), 5, "HH:MM");
        let f = ClockFmt {
            seconds: true,
            hour12: true,
            offset_min: Some(0),
        };
        let t = f.now();
        assert!(
            t.ends_with(" AM") || t.ends_with(" PM"),
            "12h suffix, got {t}"
        );
        // Offset arithmetic: +05:30 vs UTC differ by exactly that.
        let utc = ClockFmt {
            seconds: false,
            hour12: false,
            offset_min: Some(0),
        }
        .now();
        let ist = ClockFmt {
            seconds: false,
            hour12: false,
            offset_min: Some(330),
        }
        .now();
        let (u, i) = (utc.trim(), ist.trim());
        let uh = u[..2].parse::<i32>().unwrap();
        let ih = i[..2].parse::<i32>().unwrap();
        let diff = (ih - uh + 24) % 24;
        assert!(
            diff == 5 || diff == 6,
            "+05:30 zone hour delta, u={utc} i={ist}"
        );
    }
}
