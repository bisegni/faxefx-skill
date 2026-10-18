# Implementation verification — 2026-10-17

Automated implementation checks and manual/hardware acceptance are separate.

| Check | Evidence |
| --- | --- |
| Existing Python/frontend baseline | 110 Python tests, 5 frontend tests and frontend production build passed before native integration |
| Rust workspace | 53 passed, 1 frozen-worker integration test ignored by default; `cargo test --workspace --locked` passed |
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
IDs for 174 block instances across 42 families. USB capture establishes routing,
scene and block status and creates unavailable catalogued rows. Live synchronization
then requests parameters visible in the selected inspector. The inspector shows the
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

## Compact routing and late USB discovery

Effect cards are capped at 64 × 46 logical pixels, centered in the original
14 × 6 device grid, with subdued cell boundaries, readable labels and no raw
routing footer. An offline geometry test covers every cell at 700 × 300,
1128 × 730 and 1800 × 1000 canvas sizes, checking bounds and non-overlap.

Discovery uses persistent MIDI clients on a dedicated thread. On macOS that
thread services its CoreFoundation run loop between scans; the two-second UI
subscription remains stable while requests are in flight. Missing endpoints
clear stale selections. Injected catalog tests cover an initially absent device,
a transient scan failure and later arrival without opening MIDI. Physical
power-on after app launch and visual acceptance remain manual checks; no GUI
application or hardware was used for this verification.


## Live synchronization — 2026-10-18

The backend owns one persistent MIDI input/output pair. Capture, selected-block
reads, scene selection and Save serialize on that owner. One solicited read is
outstanding, packets are spaced by at least 25 ms, and the UI coalesces background
work to one pending task. Explicit scene and Save intents suspend new background
tasks and wait only for the in-progress read. Save keeps its existing repeated
preflight, durable intent, one store transmission and bounded settling window.

The selected block's opaque uint32 `0x0084` token is checked on a 100 ms schedule.
Viewport sensors report visible parameter rows. Selection and newly visible rows
start reads immediately; unchanged tokens do not read parameters. New tokens are
retained while a refresh acknowledges its starting token. Scene, block channels
and grid identity are checked on a 500 ms schedule. Context changes discard affected
readings. Device text is preferred; only existing validated amp conversions move
knobs. `0x001B` decoding has not been established here and is not synthesized.
Other catalogued `0x001A` replies remain explicitly unverified and read-only.

Confirmed rows are published to backend inspection after each validated reply and
returned directly to the UI; no slow snapshot recapture is required. The 25 ms UI
scheduler targets rendering within 100 ms. Wall-clock rendering and front-panel
latency require manual acceptance. Cached readings expose token, observation time,
connection/context generation and freshness through agent inspection. UI rows label
cached values. UI epochs, selection revisions and request ordering reject obsolete
results. Physical disconnection invalidates readings after a failed read; retries
back off. A timed-out MIDI connection is retired until explicit Read USB reconnect,
since this protocol has no nonce with which to distinguish a late matching reply.

Injected transport tests cover token identity and changes during refresh, scrolling,
selection, unrelated blocks, scene/channel/model/preset changes, stale replies,
disconnection/reconnection, unavailable conversions, queue/rate bounds, and shared
scene/Save/poll ordering without mutation replay. These checks do not prove device
behavior. No app launch or hardware access forms part of automated verification.

Automated results for this candidate: workspace tests **65 passed, 1 ignored**;
strict workspace/all-target Clippy, formatting and diff checks passed. The ignored
test is the separately invoked frozen-worker integration check. Twelve new tests
exercise the synchronization and persistent packet transport boundaries.

The release `live-sync` package passed all 152 manifest hash checks and the
standalone-worker module-boundary check. The packaged frozen-worker integration
was invoked separately: both artifact tests passed with no system Python on PATH.
The Mac bundle is ad-hoc signed; Developer ID signing/notarization and physical
acceptance remain open.
