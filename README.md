# mdrv-ds

DualSense (and DualShock 4) over Bluetooth for gaming — proxy daemon,
audio-haptics streaming, and companion tools. Workspace covering the
Linux daemon today, the Windows daemon (import pending), and the shared
protocol core.

Full mechanics, traps, and per-topic deep dives:
`/x/m/v270/mdrv-ds/` (00-overview, 10-haptics-over-bluetooth,
40-writing-pad-proxies, 50-audio-sink-jack-routing, 70-xinput, …).

## Layout

| Crate            | Package               | Kind          | Notes                                                                                           |
| ---------------- | --------------------- | ------------- | ----------------------------------------------------------------------------------------------- |
| `crates/core`    | `mdrv-ds-core` 0.0.1  | lib           | platform-free protocol dialect + media/input math; publishable                                  |
| `crates/linux`   | `mdrv-ds-linux` 0.0.1 | bin `mdrv-ds` | the Linux daemon (BT L2CAP owner, uhid virtual pad, PipeWire capture → Opus → haptics)          |
| `crates/windows` | —                     | scaffold      | import from github.com/mdrv/mdrv-ds-windows; stays out of default-members until it cross-builds |

Versioning: all crates 0.0.1 until the first crates.io publish.

## Build & deploy (Linux)

    make release    # cargo build --release + setcap the daemon binary

`crates/linux` deliberately keeps `[[bin]] name = "mdrv-ds"` and the
workspace builds into `target/`, so the existing systemd units
(`deploy/mdrv-ds.service`, ExecStart `/g/mdrv-ds/target/release/mdrv-ds`
with an ExecStartPre setcap of `cap_net_bind_service,cap_net_raw,
cap_sys_ptrace`) keep working unchanged. The daemon needs those caps to
bind PSM 0x11/0x13; cargo strips file capabilities on every rebuild —
the unit's ExecStartPre re-applies them (or use `make release`).

    systemctl --user restart mdrv-ds.service

Example config: `config/config.toml.example` → `~/.config/mdrv-ds/config.toml`.

## Publishing (crates.io)

`mdrv-ds-core` is publish-ready (leaf crate, no deps, MIT). The daemon
bins are `publish = false` (system-specific paths/units). When publishing
new versions: strictly leaf-first (`mdrv-ds-core` has no internal deps
today), and mind crates.io's new-crate rate limit (~2 per 20 min) —
playbook: `/x/m/v270/gpui-ce/gpui-ce-sync.md`.

## History

The pre-monorepo Linux daemon lives on (archived) at
github.com/mdrv/mdrv-ds-linux; this repo supersedes it at the same
`/g/mdrv-ds` path so deployments don't move.
