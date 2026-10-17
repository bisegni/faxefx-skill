# AI preset designer

The skill guides the conversational AI. The Python backend stores revisions,
compiles designs, analyzes local recordings, and owns MIDI. The browser edits the
same designs through a loopback API; it has no device-apply endpoint.

```text
You -> AI + skill -> CLI --+--> shared Python services -> MIDI -> Axe-Fx
You -> browser -> local API+         |
                             revisioned artifacts
```

## Start the workbench

From the checkout, prepare the ordinary Python environment as described in the
README. Build the optional browser interface once:

```sh
cd web
npm ci
npm run build
cd ..
.venv-cli/bin/python tools/axefx.py --backend mock --state-dir .local/studio design create --name "Studio Clean"
```

Use the returned project ID in subsequent commands. Keep the same state directory.
For a richer authored example, use `design import --file
docs/examples/clean-space.design.json` instead. It includes parallel wet branches
and two scenes and is explicitly simulation-only.

```sh
.venv-cli/bin/python tools/axefx.py --backend mock --state-dir .local/studio design open PROJECT_ID
```

This opens a session URL on `127.0.0.1` and keeps the process running until Ctrl+C.
Use `--no-browser` to print the URL without opening it. The frontend is served
locally; it does not load fonts, scripts, or audio from a CDN. A session token in
the URL fragment protects API access. Closing the server cancels its audio jobs.

The canvas supports block/cable editing, undo/redo, an inspector, and forms for
scenes, channels, controllers and modifiers, with advanced JSON for complete
performance data. Double-click a cable to remove
it. Logical canvas positions are independent of physical grid coordinates.
Captured blocks retain their physical positions. The full JSON editor supports
all draft scene/channel/modifier/controller fields exposed by `schema`.

Save draft creates an immutable revision and advances a project head. Chat and
browser edits share that head. A stale save returns `REVISION_CONFLICT`; the
browser keeps the unsaved draft for downloading and explicit merging.

## AI and CLI workflow

```sh
python tools/axefx.py --state-dir .local/studio design inspect PROJECT_ID
python tools/axefx.py --state-dir .local/studio design revise PROJECT_ID --base REVISION_ID --file draft.json
python tools/axefx.py --backend mock --state-dir .local/studio design compile PROJECT_ID
python tools/axefx.py --backend mock --state-dir .local/studio design preview PROJECT_ID
```

`design create --file draft.json` and `design import --file draft.json` accept the
same design shape. `create` without a file starts a four-block logical draft;
it does not initialize a device preset. `compile` returns a retained compilation
artifact and `buildable`, `blockers`, `warnings`, `mapping`, `physical`, and
`operations`. It does not send MIDI. A blocked compilation is a successful
diagnostic result (`ok: true`, `buildable: false`).

`design preview` produces an ordinary immutable edit plan, bound to both the
device state and the current design revision. Show its changes and obtain user
approval before `edit apply PLAN_ID --approved`. A changed design revokes that
preview even if the device is unchanged. Applying to mock is simulation, not a
hardware build. Saving remains a separate approved `preset save` operation.

There is no separate embedded AI provider or API key. The AI host reads the design
and analysis artifacts, proposes a revision using the CLI, and explains its
reasoning to the user. The browser is a companion to that conversation.

## Design contract

The design is a JSON object, separate from executable `{"operations": [...]}`
proposals. `schema` documents both. Example:

```json
{
  "design_version": "1.0",
  "name": "Clean Space",
  "intent": "A responsive clean tone with a parallel wet delay",
  "mode": "new",
  "nodes": [
    {"id": "in", "type": "input-1"},
    {"id": "amp", "type": "amp", "parameters": {"gain": 3}, "rationale": "Keep picking dynamics"},
    {"id": "cab", "type": "cab"},
    {"id": "echo", "type": "delay", "parameters": {"time": 0.35, "mix": 1}},
    {"id": "out", "type": "output-1"}
  ],
  "edges": [
    {"from": "in", "to": "amp"},
    {"from": "amp", "to": "cab"},
    {"from": "cab", "to": "out"},
    {"from": "cab", "to": "echo"},
    {"from": "echo", "to": "out"}
  ],
  "scenes": [],
  "active_scene": 1,
  "modifiers": [],
  "controllers": {},
  "references": []
}
```

These delay values use mock units. They are not a USB parameter recipe. Real
model choices and conversions require a validated firmware-specific profile.

Stable node IDs belong to the logical design. Compilation maps them to physical
effect references. Nodes support `parameters`, `channels` keyed A–D,
`active_channel`, `bypass`, optional `model`/`model_parameter`, and optional pinned
`position` with one-based row/column. `ui_position` controls only the canvas.
Scene channel/bypass maps and modifier `block` fields use logical IDs. Modifier
entries use `ref`, `block`, `parameter`, `source`, and `settings`. Controller names
and settings use the existing `schema` contract.

The mock compiler allocates effects and shunts deterministically and produces
physical adjacent-column cables. Cycles, unavailable instances, impossible pins,
unwired blocks, and invalid settings produce blockers. It retains the original
design and offers an explicit revision rather than silently changing the tone.
DSP load and audible results are not predicted.

## Current-preset capture and images

```sh
python tools/axefx.py --state-dir .local/studio design capture
python tools/axefx.py --state-dir .local/studio design render PROJECT_ID --output .local/preset.svg
python tools/axefx.py --state-dir .local/studio design render PROJECT_ID --physical --output .local/grid.svg
python tools/axefx.py --state-dir .local/studio design render PROJECT_ID --block amp --output .local/amp.svg
```

`capture` uses the selected backend. The default USB backend reads verified
geometry; missing settings remain unknown. It never substitutes mock data.
`--physical` renders the immutable capture when present, otherwise the compiled
physical preview. It is not a diagram of unapplied draft changes on the device.
SVG export has no optional Python dependency. CLI PNG export requires
`config/requirements/visual.txt` (Pillow); the browser exports PNG directly.
Use `--scene 2` to render a proposed scene's bypass/channel state. The browser
exports its selected scene. A partial capture cannot show unobserved scenes.

## Local audio reference

Install optional measurements into the interpreter that runs the workbench:

```sh
uv pip install --python .venv-cli/bin/python -r config/requirements/audio.txt
python tools/axefx.py --state-dir .local/studio audio import --file reference.wav
python tools/axefx.py --state-dir .local/studio audio analyze --audio-id AUDIO_ID --start 35 --duration 20
python tools/axefx.py --state-dir .local/studio audio inspect ANALYSIS_ID
python tools/axefx.py --state-dir .local/studio audio compare REFERENCE_ANALYSIS_ID RECORDING_ANALYSIS_ID
```

Files are copied into content-checked local storage. Excerpts are bounded to 120
seconds, with a default of 30 seconds from the start. WAV/FLAC are preferred;
decoding other accepted extensions depends on the local soundfile installation.
No recording is uploaded. Reports contain spectral bands, centroid, RMS, crest,
clipping fraction, envelope statistics and stereo correlation. They identify no
amp or exact rig and provide no unsupported tone-accuracy percentage.

For mixed recordings, optional `--separate` uses `audio-separator` with
`htdemucs_6s.yaml`. Install `config/requirements/separation.txt` in a dedicated
environment if desired, also including ordinary CLI requirements. The workbench
uses its own interpreter for workers. Separation may download model weights into
the local state directory. The package version, model name and available config/
weight SHA-256 identities are recorded. The pinned 0.41.1 CPU extra includes ONNX
Runtime even when Demucs runs through Torch/MPS. This runtime was exercised locally
on an authored signal; musical separation quality still requires acceptance with
the user's recordings. Ordinary measurement tests do not establish that quality.

Listen to the extracted guitar before adopting it as the target. It may contain
multiple guitars, other instruments, or artifacts. Attach the analysis to the
design and ask the AI to propose a chain based on measurements plus musical
context. After an approved build, import a recording of your playing for
descriptive comparison and another reviewable revision. Automatic reamping,
parameter searches, and automatic hardware Tone Match capture are not enabled.
The browser A/B players match excerpt RMS levels using attenuation only; they do
not boost either recording. Compare similar performances and confirm by ear.

## Protocol and local catalog evidence

```sh
python tools/axefx.py protocol coverage
python tools/axefx.py protocol inspect-editor --app '/Applications/Axe-Edit III.app'
python tools/axefx.py --state-dir .local/studio catalog import-editor --app '/Applications/Axe-Edit III.app' --cache '/explicit/path/effectDefinitions_10_32p6.cache'
python tools/axefx.py --state-dir .local/studio catalog inspect-import --id CATALOG_ID
python tools/axefx.py --state-dir .local/studio catalog inspect-import --id CATALOG_ID --type 10 --parameter 0
python tools/axefx.py --state-dir .local/studio catalog inspect-import --id CATALOG_ID --symbol DISTORT_DRIVE
```

The importer reads bounded cache revisions 3/4, requires exact EOF, retains raw
flags and neutral numeric fields, and extracts ARM64 symbolic ID tables. It does
not enable model setters or infer conversions. The selected cache's firmware is
not evidence of the connected device firmware. No implicit newest-cache choice
is made. The source executable/cache identities accompany the artifact.
Inspection defaults to a compact type inventory; use numeric `--type` and
`--parameter` to retrieve selected definitions and
model label tables without dumping the full catalog into the AI context.
`--symbol` retrieves an enumeration separately; numeric equality alone does not
establish a symbol's effect-type association or conversion.

The static inspector recognizes the previously analyzed executable by SHA-256;
other builds require fresh builder recovery. On macOS, `otool` inventories its
direct builder callers and retains preceding instructions. Semantic classification
and indirect paths remain explicitly unresolved. This is not complete private
protocol recovery. Public inventory includes setters not implemented by the CLI,
standard MIDI, and checksumless push-message exceptions.

## Acceptance status

Implemented: revisioned local designs, capability-scoped compilation, full mock
domain previews, deterministic images, local browser editing, audio measurements,
an optional separation adapter, and read-only editor/catalog evidence tools.

Pending: full private-protocol semantics, firmware identity, complete hardware
edit-buffer backup/restore, structural/model/scene/channel/modifier/controller
writes, save/reload, and live acceptance. USB compilation returns explicit blockers
until these gates are passed. Static catalog import cannot remove those blockers.
The existing six numeric amp setters retain their existing approval/readback
contract and their hardware-unverified status.

Completion of automatic construction requires a designated test preset, a verified
backup/restore round trip, serial/parallel builds, scene/channel/modifier readback,
interrupted-build recovery, an approved save/reload, and preservation of unrelated
presets. Tone quality separately requires user listening acceptance.
