# crates/windows — scaffold (not yet a workspace member)

The Windows daemon lives at github.com/mdrv/mdrv-ds-windows (developed on
the Ghost Spectre Windows 11 machine). It becomes `crates/windows` via:

    git subtree add --prefix=crates/windows mdrv-ds-windows main

(or a plain file import if history sharing is not wanted — the repo is ours).

Before adding it to `[workspace.members]`:

- It must keep building only under `--target x86_64-pc-windows-msvc`
  (deps: wasapi, audiopus_sys static, no pipewire) — keep it OUT of
  `default-members` so Linux `cargo check` stays green.
- Shared logic (report builders, CRC, pacing, opus/resample, deadzone/
  chord math) should move to `crates/core` instead of being duplicated.
- The ASI in-game shim (mdrv-ds.asi via Ultimate-ASI-Loader dinput8.dll,
  IAT hooks) is a second cdylib target in that repo — same crate family.

Status/traps for the Windows side: docs there cover BTHENUM routing,
VBS, 1 ms timers, and dead ends (20-dead-ends.md).
