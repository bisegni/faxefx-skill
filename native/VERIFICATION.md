# Implementation verification — 2026-10-17

Automated implementation checks and manual/hardware acceptance are separate.

| Check | Evidence |
| --- | --- |
| Existing Python/frontend baseline | 110 Python tests, 5 frontend tests and frontend production build passed before native integration |
| Rust workspace | 39 passed; frozen-worker integration test ignored by default and passed separately |
| Rust quality | rustfmt and strict Clippy passed; final run recorded with candidate |
| Adapter contracts | Both providers exercised through deterministic subprocess fixtures, including explicit skill handshake, scoped tools, interruption and provider boundaries |
| Real Codex | Installed 0.163.0 explicitly initialized skill, inspected authored mock; a second run changed only mock gain 5→6.2 with readback, then restored 5 |
| Real Claude | Installed 2.1.302 recognized; live run blocked by absent API credential; deterministic contract passed |
| Standalone Python audio | 3 authored-signal/isolation tests passed |
| Frozen audio worker | Authored sine measurement passed with cleared environment and no system Python on PATH |
| Package integrity | Debug package: 152 file hashes verified; frozen archive excludes legacy modules; release manifest accompanies final candidate |
| Native UI smoke | All panes visible; mock manual gain change and Restore verified; explicit destination dialog; Codex initialization/streaming; WAV import/waveform and bundled analysis exercised |
| USB | Fixed read queries implemented; no physical write experiment performed |
| Hardware Save/Restore | Unavailable; setter context/modifier protection and save/reload gate unresolved |
| Minimum size/DPI/accessibility | Layout constraints tested; real display-scale acceptance pending; toolkit content currently lacks an AX widget tree |
| Windows/Linux | CI matrix configured; remote results are separate evidence |

The minimum client area is 1280×850 logical pixels. Screenshot smoke used a larger
window and does not prove all DPI or minimum-size combinations. The native image
uses the approved dark material direction with recessed controls, effect accents,
vector icons, a dotted canvas and animated sagging cables. Local QA screenshot:
`output/axe-studio-integrated.png`.

The user will perform physical testing by hand. Hardware mutations remain disabled
until protocol/capability and verified-backup gates pass; manual testing cannot
exercise unsupported functions in this candidate. Optional guitar separation and
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

## Connection-aware routing refinement

Auto layout now collapses routing strongly connected components, assigns layers
from the directed connection graph, and applies barycentric ordering to reduce
cable crossings. Input blocks anchor at the left and output blocks at the right.
Effect cards are smaller (up to 78×66 logical pixels) and size with the measured
canvas viewport. Auto layout uses that same measured viewport after window resizing.
