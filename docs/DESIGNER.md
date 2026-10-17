# AI preset designer

The Python backend reads the connected Axe-Fx, stores immutable captures and
revisioned drafts, analyzes local recordings, and serves the browser. Embedded
Codex chat uses draft-only application tools through the same services. External
AI clients can continue using the CLI skill.

```text
Browser → Python backend → read-only USB capture → Axe-Fx III
   │             │
   └→ chat → Codex App Server → scoped draft tools
                 │
          review proposal → accept revision
```

## Start the workbench

Prepare Python as described in the README, then build the browser interface:

```sh
cd web
npm ci
npm run build
cd ..
.venv-cli/bin/python tools/axefx.py --state-dir .local/studio ui
```

Connect your Axe-Fx III by USB and select the desired preset on the device.
`ui` defaults to USB, opens the browser before capture, and automatically reads
the current preset. No project ID or separate capture command is required.
`--input` and `--output` before `ui` can select exact endpoint names. Ambiguous
endpoints require a selection in the UI; errors never fall back to simulation.

The token-protected session runs on `127.0.0.1` until Ctrl+C. `ui --no-browser`
prints its URL instead. The frontend serves local assets without a CDN. Closing
the server stops audio jobs and its Codex subprocesses.

The connection strip reports capture state, preset identity, active scene, and
capture time. **Refresh from Axe-Fx** creates another project without deleting
the previous one. Unsaved browser changes require a save/discard choice first.
A failed refresh retains the previous capture, marked stale/disconnected.
Device queries are serialized with CLI operations through the existing USB lock;
no connection or device lock is held for the browser's lifetime. State polling
does not poll MIDI. Refresh after changing the device.

The canvas supports block/cable editing, undo/redo, block inspection, and
scene/channel/controller/modifier forms. Double-click a cable to remove it.
Logical positions are independent of physical grid coordinates. Captured
shunts, wires, and unidentified effect blocks retain their real positions.
Unknown bypass/channel values remain `null` and display as **Unknown**, including
exports. Unobserved scene settings never inherit an observed scene as a fact.

Saving creates an immutable revision and advances a shared project head.
Conflicts return `REVISION_CONFLICT` and preserve unsaved browser edits for
download. Draft revisions cannot alter raw capture evidence or device identity.

For an existing project, `design open PROJECT_ID` remains supported. An authored
simulation example is available in `docs/examples/clean-space.design.json` for
internal testing; it is not a hardware capture or hardware model recipe.

## Chat inside the app

Click **Connect Codex**. The backend starts `codex app-server --stdio` using the
installed CLI and its configured model/sign-in. The tested protocol contract is
Codex CLI **0.163.x**; other versions fail closed for chat without blocking
inspection. Run `codex login` outside the app if authentication is missing.
The workbench does not expose credentials or a general App Server proxy to HTTP.

Send a message after saving your latest draft. The selected block and explicitly
attached analysis summaries provide context. Responses stream into the panel;
**Stop response** interrupts the active turn. A project stores its own conversation
association and display history. Reloading the browser retains it; reconnecting
Codex resumes the conversation. This is separate from any existing desktop chat.

Codex can read the saved design/capture, protocol evidence, attached analysis
summaries, and compile diagnostics. `tone_propose` stages revision-bound changes;
it cannot save the draft itself. Review the before/after fields and choose
**Accept draft changes** or **Reject changes**. Acceptance uses the existing
revision lock/checks. Manual or external changes invalidate stale proposals.

The agent session disables shell, external connectors/plugins, hooks, delegation,
and browser/computer tools, and requests no execution environments plus a
confirmed read-only sandbox. The code-mode host remains enabled to dispatch the
scoped application tools. Host requests outside the draft-tool bridge are denied.
No device-apply/save tool is provided. Required restrictions are checked at
connection time; failed checks disable chat rather than widening permissions.

Messages, relevant preset context, and analysis summaries are sent to the
configured AI service. Raw audio stays local and is never attached automatically.
Full USB design construction remains blocked regardless of the chat's proposals.

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

External AI hosts read design/analysis artifacts and propose revisions using the
CLI. Embedded Codex chat uses the scoped tools above and needs no separate
provider/API-key configuration in this app.

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
