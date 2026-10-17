# Axe Studio native development

This is an **incomplete development foundation**, not the Mac acceptance candidate.
The approved delivery contract is in `todo/NATIVE_APP_PLAN.md`. Legacy source is
preserved for reference and is not a dependency of the Cargo workspace or the
standalone audio worker.

From the repository root, run `cargo run -p axe-desktop`. The native window opens
before MIDI discovery. USB discovery/capture runs on background blocking workers;
only the fixed read-only grid and scene queries are exposed. If a unique Axe-Fx
pair is found, capture starts; otherwise select endpoints or explicitly choose
Test mock. A failed USB read never selects mock automatically. No hardware writes
are enabled. Save tone and Restore original remain disabled in the window.

The five panes support drag/dock/resize and named layout save/load. Proposals are
validated against pane minimums and retain the previous layout on failure.
Current layout is persisted in app-owned SQLite. Default minimum client size is
1280 × 850. Native manual/DPI/accessibility acceptance is still outstanding.

## Implemented boundaries

- `axe-domain`: contexts, explicit unknown values, verified-range validation.
- `axe-session`: SQLite FULL/WAL journals/originals/chat/settings; isolated mock;
  read-only MIDI capture; framing/grid decoder; mock transactions and restoration.
  Interrupted journals block subsequent writes and never auto-replay.
- `axe-agents`: normalized adapter contract, tool allowlist, startup and revocable
  turn guards, bundled `axefx-live` skill. No concrete provider runtime yet.
- `axe-audio`: versioned bounded audio-worker request encoder.
- `axe-desktop`: Iced 0.14 window, native routing canvas, generic effect registry,
  pane layout persistence/validation and settled visual sag with simple mode.
- `audio-worker`: standalone extracted local measurements over private JSON-RPC.
  NumPy/SciPy/SoundFile are pinned; no legacy/application/device imports.

The current cable model settles a bounded sag curve at draw time. Interactive
Verlet motion, hit-testing and loose-end dragging are not implemented. Hardware
capture retains unknown effect identities and parameters, rather than guessing
profiles. Parameter transactions are currently enabled only for injected/mock
transports. Originals are numeric mock values; bit-exact hardware originals,
modifier detection and setter capability profiles still require implementation.

## Outstanding delivery gates

Concrete Codex App Server and Claude headless adapters; protocol/version and tool
restriction compatibility; explicit runtime skill ingestion; live streaming and
chat checkpoint/resume UI; authenticated scoped Claude MCP; audio job supervision,
artifact import/playback/waveforms/A-B/attachments; model download management;
complete controls and dirty-close handling; serialized cancellable runtime writer;
physical profile validation, readback/Restore and save/reload; full layout keyboard,
DPI/accessibility checks; installable interpreter-bundled packages; clean-machine
checks; all OS hardware gates; signing when credentials exist. Optional separation
is currently reported unavailable by the standalone worker.

Physical setter experiments require a designated test preset and verified backup.
This development run performs no physical writes. No release or acceptance claim
follows from injected transport tests. CI defines the three-OS build/test matrix;
a local Mac pass is not evidence that remote Windows/Linux jobs have passed.

## Verification

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
python -m pytest -q native/audio-worker
```

Use an environment containing `native/audio-worker/requirements.txt` plus pytest
for the last command. These are developer commands, not end-user installation
instructions. Packaged distribution must include an interpreter; packaging remains
unfinished. Do not remove legacy code or rewrite main before explicit acceptance.
