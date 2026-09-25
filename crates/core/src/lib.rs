//! # mdrv-ds-core
//!
//! Platform-free core of the mdrv-ds suite: everything both the Linux and
//! Windows daemons need and that has no OS dependency.
//!
//! ## Scope (extraction is incremental — both daemons carry working copies
//! until each piece moves here)
//!
//! - **Dialect**: DualSense output report builders — `0x31` (78 B kernel
//!   rumble/volume), `0x35` (334 B audio+haptics), `0x36` (398 B combined
//!   state+haptics+audio), `0x39` (547 B large audio), `0x02` (USB 48 B) —
//!   CRC-32 with the HIDP `0xA2` seed, sequence/counter families, and the
//!   10.667 ms (480/45000) slot-clock pacing discipline.
//! - **Media**: Opus CBR framing for the speaker/haptic stream, 48 kHz→45 kHz
//!   resampling, s8 haptic packing into audio reports.
//! - **Input math**: stick deadzones (inner/outer), chord/tap detection,
//!   touchpad gestures, calibration (NVS finetune 0x80/0x81, 0x82 recal).
//!
//! ## Out of scope (stays in the platform crates)
//!
//! Config schemas (the two daemons' schemas have diverged deliberately),
//! transports (L2CAP socket vs hidclass WriteFile), virtual-pad backends
//! (uhid vs "never virtualize" on Windows), mouse injection (uinput vs
//! SendInput).
//!
//! Reference documentation: `/x/m/v270/mdrv-ds/` (10-haptics-over-bluetooth,
//! 40-writing-pad-proxies, 50-audio-sink-jack-routing).
