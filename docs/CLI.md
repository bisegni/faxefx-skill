# Manual CLI guide

Run the examples from the repository root. The CLI runs directly from
`tools/axefx.py`; only third-party dependencies are installed.

## Set up the Python environment

Use Python 3.11 for the tested USB dependency and create the CLI environment:

```sh
uv venv --python 3.11 .venv-cli
uv pip install --python .venv-cli/bin/python -r config/requirements/cli.txt
source .venv-cli/bin/activate
python tools/axefx.py --version
python tools/axefx.py --help
```

`uv` is optional. With Python 3.11 installed, the equivalent is:

```sh
python3.11 -m venv .venv-cli
source .venv-cli/bin/activate
python -m pip install -r config/requirements/cli.txt
```

On Windows PowerShell, without activating the environment:

```powershell
py -3.11 -m venv .venv-cli
& .\.venv-cli\Scripts\python.exe -m pip install -r config/requirements/cli.txt
& .\.venv-cli\Scripts\python.exe .\tools\axefx.py --help
```

The remaining examples use `python tools/axefx.py` with the environment active.
You can instead use `.venv-cli/bin/python tools/axefx.py` on macOS/Linux or the
Windows interpreter path above. From another directory, use absolute interpreter
and script paths. Only third-party dependencies are installed, not this project.

## Choose a backend

| Backend | Behavior |
|---|---|
| `--backend mock` | Stateful simulation with geometry, routing, blocks, parameters, scenes, channels, modifiers, controllers, new/copy/save and restore. Sends no USB traffic. |
| `--backend replay --replay-file PATH` | Replies from the selected file. Read-only; cannot invent geometry or new operations. The bundled fixture is synthetic. |
| `--backend usb` (default) | Recovered public reads and active amp getters; approved numeric amp setters are implemented from static evidence but hardware-unverified. |

USB geometry, complete preset snapshots, structural edits, preset saving,
scene/channel edits, modifiers and controllers remain **protocol-unresolved**.
They return explicit errors. A working mock is not proof of those USB operations.
No live-device test is part of this implementation phase.

Global options go **before** the command. Run commands sequentially for one device.
JSON is the default output; `--format text` is easier to read by hand.

## Inspect a simulated preset by hand

Start with mock mode and a dedicated artifact directory:

```sh
python tools/axefx.py --backend mock --state-dir .local/demo capabilities
python tools/axefx.py --backend mock --state-dir .local/demo --format text preset geometry
python tools/axefx.py --backend mock --state-dir .local/demo block list
python tools/axefx.py --backend mock --state-dir .local/demo block inspect amp1
python tools/axefx.py --backend mock --state-dir .local/demo parameter get amp1 gain
python tools/axefx.py catalog --type amp --parameter gain
python tools/axefx.py schema
```

The initial **Mock Clean** signal chain is:

```text
row 1: [Input 1] -> [Amp 1] -> [Cab 1] -> [Output 1]
rows 2-6: empty
```

`preset inspect` returns the complete preset, including geometry, physical cables
and per-block incoming/outgoing connections. Coordinates start at 1; scene numbers
are 1-8, channels are A-D and preset slots use device numbers 0-1023.

`catalog` lists types/available instances; `--type TYPE --parameter NAME` narrows
a definition. The bundled `minimal-mock-v1` catalog recognizes 18 types, with
mock editing for common effects and I/O; NAM Player has no editable definition.
Its firmware is null. Model options such as `Mock Clean` are synthetic and do not
select hardware models. The six amp knobs keep their observed 0–10 conversion;
other numeric parameters use authored `mock` units/ranges. Full vendor
parameter/model lists are not bundled. Discover available names/options from
the CLI; create fresh previews after changing catalog versions.

## Preview and apply a change by hand

```sh
python tools/axefx.py --backend mock --state-dir .local/demo parameter set amp1 gain --value 6
```

This returns a preview with `approval_required: true`, before/after state, and an
`id` such as a 64-character content hash. It does **not** change the mock preset.
Copy that exact `id`. When you accept the preview yourself, run:

```sh
python tools/axefx.py --backend mock --state-dir .local/demo edit apply PLAN_ID --approved
python tools/axefx.py --backend mock --state-dir .local/demo parameter get amp1 gain
```

Replace `PLAN_ID` with the returned hash; do not type the placeholder literally.
The second command verifies the resulting value. A changed/stale plan is rejected,
and an already attempted plan cannot be replayed blindly.

In AI use, the skill shows the preview and obtains your approval before passing
`--approved`. The flag acknowledges that approval; it cannot authenticate a user
conversation by itself. One approval covers the unchanged batch, not unrelated
future edits. No experimental-write enable switch is required.

## Change the signal chain in one preview

Related block additions and connections belong in one batch; separate preview
commands do not make their proposed blocks available to subsequent commands.
Create `.local/changes.json` with:

```json
{
  "operations": [
    {"op": "block.disconnect", "from": "cab1", "to": "output1"},
    {"op": "block.move", "block": "output1", "row": 1, "column": 8},
    {"op": "block.add", "type": "delay", "ref": "delay1", "row": 1, "column": 6},
    {"op": "block.add", "type": "reverb", "ref": "reverb1", "row": 1, "column": 7},
    {"op": "block.connect", "from": "cab1", "to": "delay1"},
    {"op": "block.connect", "from": "delay1", "to": "reverb1"},
    {"op": "block.connect", "from": "reverb1", "to": "output1"}
  ]
}
```

```sh
python tools/axefx.py --backend mock --state-dir .local/demo edit plan --file .local/changes.json
# Read the preview; after accepting it, use the returned ID:
python tools/axefx.py --backend mock --state-dir .local/demo edit apply PLAN_ID --approved
python tools/axefx.py --backend mock --state-dir .local/demo --format text preset geometry
```

The connection planner adds shunts in columns 4 and 5. Existing unrelated blocks
and cables remain intact. Moves never silently swap occupied cells. Branching
routes require an unambiguous physical cable when disconnecting.

`preset new --empty --name "Warm Ambient"` previews clearing the edit buffer for
new authoring; `preset copy current --name "Warm Ambient"` previews a clone.
For a complete new preset, put `preset.new`, block additions and connections into
one JSON batch. `schema` lists every operation's required and optional fields.

Other preview commands include:

```sh
python tools/axefx.py --backend mock --state-dir .local/demo channel copy amp1 --from A --to B
python tools/axefx.py --backend mock --state-dir .local/demo scene set 2 --name "Clean" --channels '{"amp1":"B"}'
python tools/axefx.py --backend mock --state-dir .local/demo modifier set modifier1 --block amp1 --parameter gain --source external1 --settings '{"minimum":0,"maximum":1}'
python tools/axefx.py --backend mock --state-dir .local/demo controller set lfo1 --settings '{"rate":2,"depth":0.8,"shape":"sine"}'
```

These are separate previews. Combine dependent edits in a batch. Modifier IDs are
`modifier1`-`modifier32`; inspect existing assignments first. Scenes share channel
parameters rather than owning independent knob banks. Manual mock edits to a
modifier-controlled parameter require removing its modifier explicitly first.

## Save and recover

Saving is a **separate** preview after auditioning:

```sh
python tools/axefx.py --backend mock --state-dir .local/demo preset save --slot 10
python tools/axefx.py --backend mock --state-dir .local/demo edit apply SAVE_PLAN_ID --approved
```

Choose the destination yourself. An occupied slot returns `SLOT_OCCUPIED`; use
`preset save --slot N --overwrite` only when you explicitly intend to replace it.
The approved preview includes the destination backup. Template sources are
preserved unless their slot is explicitly approved as an overwrite destination.

Applying stores an automatic before snapshot and operation journal. The result
reports the snapshot ID. To inspect/recover:

```sh
python tools/axefx.py --backend mock --state-dir .local/demo edit journal PLAN_ID
python tools/axefx.py --backend mock --state-dir .local/demo snapshot inspect SNAPSHOT_ID
python tools/axefx.py --backend mock --state-dir .local/demo edit restore SNAPSHOT_ID
```

Restore creates another preview. Review it and apply its returned plan ID with
`--approved`. Failure results distinguish verified restoration from unresolved
partial state; do not assume that a disconnected device was restored. Artifacts
survive process interruption. Changed external context can prevent restoration.
USB numeric snapshots cover only touched parameters, not an entire preset.

Mock snapshot restore changes the edit buffer and preserves presets saved after
the snapshot. To recover an overwritten destination, use the snapshot returned
by its original save and preview the backed-up slot into the edit buffer:

```sh
python tools/axefx.py --backend mock --state-dir .local/demo edit restore SNAPSHOT_ID --source-slot 10
```

Apply that approved restoration plan, then separately preview/approve
`preset save --slot 10 --overwrite`. Stored presets are never silently reverted
by an edit-buffer restore.

## Read recorded or live device data

Replay the authored synthetic test fixture without touching hardware. These
responses are test data, not observations from a real unit:

```sh
python tools/axefx.py --backend replay --replay-file tests/fixtures/synthetic-traffic.jsonl preset inspect
python tools/axefx.py --backend replay --replay-file tests/fixtures/synthetic-traffic.jsonl parameter get amp1 gain
```

For deliberate live USB use:

```sh
python tools/axefx.py device list
python tools/axefx.py --input "Axe-Fx III" --output "Axe-Fx III" preset inspect
python tools/axefx.py --input "Axe-Fx III" --output "Axe-Fx III" parameter get amp1 gain
```

Use full names returned by `device list`; names differ across platforms. USB uses
CoreMIDI, Windows Multimedia or ALSA MIDI, and needs the appropriate driver/access.
Help, schema, catalog and capabilities never connect to hardware. USB inspection
sends read-only queries. Unsupported structural operations fail before connecting.
Unknown observations are null and `complete: false`, not invented empty grids.

An approved USB amp edit is previewed with `parameter set amp1 gain --value 6`
and applied through the same `edit apply` workflow. Setters remain hardware-
unverified; inspect the warnings and accept that specific preview before applying.
The CLI checks the current knob conversion, captures originals, and reads back
writes. Firmware identity and modifier assignments are unresolved. Avoid other
editors/front-panel changes during an apply. CLI USB calls share a lock, but the
lock cannot exclude those external writers.

