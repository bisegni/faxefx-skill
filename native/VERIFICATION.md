# Implementation verification — 2026-10-17

Automated implementation checks and manual/hardware acceptance are separate.

| Check | Evidence |
| --- | --- |
| Existing Python/frontend baseline | 110 Python tests, 5 frontend tests and frontend production build passed before native integration |
| Rust workspace | 51 passed, 1 frozen-worker integration test ignored by default; `cargo test --workspace --locked` passed |
| Rust quality | `cargo fmt --all`, strict workspace Clippy, and `git diff --check` passed |
| Adapter contracts | Both providers exercised through deterministic subprocess fixtures, including explicit skill handshake, scoped tools, interruption and provider boundaries |
| Real Codex | Installed 0.163.0 explicitly initialized skill, inspected authored mock; a second run changed only mock gain 5→6.2 with readback, then restored 5 |
| Real Claude | Installed 2.1.302 recognized; live run blocked by absent API credential; deterministic contract passed |
| Standalone Python audio | 3 authored-signal/isolation tests passed |
| Frozen audio worker | Authored sine measurement passed with cleared environment and no system Python on PATH |
| Package integrity | Debug package: 152 file hashes verified; frozen archive excludes legacy modules; release manifest accompanies final candidate |
| Native UI smoke | All panes visible; mock manual gain change and Restore verified; explicit destination dialog; Codex initialization/streaming; WAV import/waveform and bundled analysis exercised |
| USB | Fixed read queries implemented; no physical write experiment performed |
| Hardware Save/Restore | User-operated store transmission implemented with injected-wire tests; acknowledgement/persistence and physical acceptance pending. Hardware Restore remains unavailable. |
| Scene selection | Eight named scenes are queried and displayed; documented scene selection plus active-scene readback is implemented. Mock path is tested; USB behavior awaits the user's hand acceptance. |
| Minimum size/DPI/accessibility | Layout constraints tested; real display-scale acceptance pending; toolkit content currently lacks an AX widget tree |
| Windows/Linux | CI matrix configured; remote results are separate evidence |

Hardware store is tested with an injected wire only: exact `0x0026` packet and
integer destination, checksum, duplicate/stale/malformed status, changed preset,
scene and routing, failed intent persistence, preflight timeout, send/settling
disconnect, one transmission without retry, durable sent/uncertain outcomes and
restart without replay. The Save dialog uses an explicit overwrite destination;
its hardware result never says persistence was verified. The app was not launched
and no hardware command was sent by these checks.

## Catalogued block parameters

The current implementation embeds static Axe-Edit 1.15.00 names and parameter
IDs for 174 block instances across 42 families. On USB capture it requests every
catalogued parameter for each recognized placed block. The inspector shows the
human-readable name, returned text or raw number, and per-row read status; the
session and block agent tools receive the same snapshot. Unknown block IDs stay
unmapped. Only the existing eight amp getters have verified getter semantics;
other parameter reads are raw and unverified, and are never exposed as editable
controls. A physical Axe-Fx read-only test of these generic getters is still
required. The app was not opened by the implementation agent; the user will
perform manual testing.

The minimum client area is 1280×850 logical pixels. Screenshot smoke used a larger
window and does not prove all DPI or minimum-size combinations. The native image
uses the approved dark material direction with recessed controls, effect accents,
vector icons, a dotted canvas and animated sagging cables. Local QA screenshot:
`output/axe-studio-integrated.png`.

The user will perform physical testing by hand. Parameter writes and preset
Restore remain unavailable. Hardware Save sends operation `0x0026` once after
durable intent and repeated context preflight. It reports transmission only,
without clearing any hardware baseline or claiming verified persistence. Its
acknowledgement contract and physical save/reload acceptance remain pending.
Scene selection is separately implemented because its command
and readback are documented, but has not been exercised on hardware here. Optional guitar separation and
model download management remain unimplemented. No automated or mock result is
hardware acceptance. Legacy code and main history are preserved.

## Routing and connection follow-up

The current follow-up adds recurring endpoint discovery while no device snapshot is
loaded, recognizes Fractal Audio/Axe-Fx MIDI port naming variants, and fits the
initial block presentation to the routing pane viewport. The routing toolbar's
Auto layout action recalculates positions using the available window dimensions;
user drags continue to override individual computed positions. `cargo check -p
axe-desktop --locked` passed after this change. Hardware connection hot-plug and
visual layout at the user's exact viewport still need manual confirmation.

## Axe-Fx grid refinement

The screenshots supplied by the user show Axe-Edit's fixed 14-column by 6-row
work surface. Auto layout now preserves the captured device cell coordinates and
scales that grid to the native routing pane. Smaller effect cards stay within their
cells, including empty grid slots, and each connection attaches to the nearest
horizontal or vertical side. The Auto layout control restores these device
coordinates after a user moves blocks or resizes the pane. The app was not opened
for verification at the user's request; final visual acceptance is reserved for
the user.
