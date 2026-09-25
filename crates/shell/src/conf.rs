//! Shared suite config: surgical section editing of
//! `~/.config/mdrv-ds/config.toml`. Every mdrv-ds app keeps ONE config
//! file; an app owns one table (`settings`, `settings.audio`, …) and
//! edits only that section so comments and foreign tables survive
//! byte-for-byte.

use serde::de::DeserializeOwned;
use std::path::PathBuf;

/// The shared suite config: `~/.config/mdrv-ds/config.toml`.
pub fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into())).join(".config")
        });
    let _ = std::fs::create_dir_all(&base);
    base.join("mdrv-ds").join("config.toml")
}

/// Deserialize `[<table>]` from the shared config. `None` when the file
/// or table is missing — callers fall back to defaults (or a legacy
/// standalone file).
pub fn load_section<T: DeserializeOwned>(table: &str) -> Option<T> {
    let text = std::fs::read_to_string(config_path()).ok()?;
    let val = toml::from_str::<toml::Value>(&text).ok()?;
    // `[a.b]` nests: walk the dotted path down to the section table.
    let mut node = &val;
    for part in table.split('.') {
        node = node.get(part)?;
    }
    T::deserialize(node.clone()).ok()
}

/// Replace/append the `[<table>]` section with `body` (full section text
/// INCLUDING its `[table]` header). Every other line — comments, foreign
/// tables — survives byte-for-byte. Atomic (tmp + rename).
pub fn save_section(table: &str, body: &str) -> Result<(), String> {
    let path = config_path();
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();

    let header = format!("[{table}]");
    let mut body = body.to_string();
    if !body.ends_with('\n') {
        body.push('\n');
    }

    // Existing section (header at start of a line)? Replace from the
    // header up to — not including — the next top-level `[table]` line
    // or EOF.
    let start = text
        .match_indices(&header)
        .map(|(i, _)| i)
        .find(|&i| i == 0 || text.as_bytes()[i - 1] == b'\n');
    if let Some(start) = start {
        let end = text[start..]
            .find("\n[")
            .map(|i| start + i + 1)
            .unwrap_or(text.len());
        text.replace_range(start..end, &body);
    } else {
        // append at EOF (keep the file's trailing shape tidy)
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push('\n');
        text.push_str(&body);
    }

    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Cfg {
        alpha: u8,
        beta: String,
    }

    fn sandbox(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mdrv-shell-conf-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("mdrv-ds")).unwrap();
        // SAFETY: test-only, sequential tests.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &d) };
        config_path()
    }

    #[test]
    fn replace_preserves_foreign_tables_and_comments() {
        let path = sandbox("replace");
        std::fs::write(
            &path,
            "# top\ntransport = \"l2cap\"\n\n[ps]\nmode = 'swallow'\n\n# chord\n[chords]\neast = 'x'\n",
        )
        .unwrap();
        save_section("settings.audio", "[settings.audio]\n# audio\nalpha = 7\n").unwrap();
        let t = std::fs::read_to_string(&path).unwrap();
        assert!(t.contains("# top") && t.contains("[ps]") && t.contains("east = 'x'"));
        assert_eq!(t.matches("[settings.audio]").count(), 1);
        toml::from_str::<toml::Value>(&t).expect("valid toml");
        // second save replaces in place
        save_section("settings.audio", "[settings.audio]\nalpha = 9\n").unwrap();
        let t2 = std::fs::read_to_string(&path).unwrap();
        assert_eq!(t2.matches("[settings.audio]").count(), 1);
        assert!(t2.contains("alpha = 9"));
        assert!(t2.contains("east = 'x'"));
        // load_section round-trips
        let cfg: Cfg = load_section("settings.audio").unwrap();
        assert_eq!(cfg.alpha, 9);
        // saving a PARENT table must not corrupt the nested one (the
        // text-level "[settings]" header never matches "[settings.audio]")
        save_section("settings", "[settings]\nbeta = \"z\"\n").unwrap();
        let t3 = std::fs::read_to_string(&path).unwrap();
        assert_eq!(t3.matches("[settings.audio]").count(), 1, "audio kept");
        assert!(t3.contains("alpha = 9"), "audio values kept");
        assert_eq!(t3.matches("[settings]\n").count(), 1, "new parent table");
        toml::from_str::<toml::Value>(&t3).expect("valid toml");
    }
}
