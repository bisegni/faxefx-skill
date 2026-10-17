# CLI reference

Global options precede the command: `--backend usb|mock|replay`, `--state-dir DIR`,
`--format json|text`, `--input FULL_NAME`, `--output FULL_NAME`, `--replay-file PATH`.
Default backend is USB and default output is JSON. Never use USB to test this
package when the user has requested mock/replay testing only.

## Setup

Find `tools/axefx.py` in the active repository. Run it directly with a Python 3.11
environment; do not install an app, build a wheel, or publish to PyPI. The skill
depends on this repository checkout. No CLI executable on PATH is required.

For setup use `uv venv --python 3.11 .venv-cli`, then
`uv pip install --python .venv-cli/bin/python -r config/requirements/cli.txt`.
Run `.venv-cli/bin/python tools/axefx.py --version` from the repository root.
On Windows use `.venv-cli/Scripts/python.exe`. With that environment activated,
the command prefix below is `python tools/axefx.py`. From another working directory
use the absolute interpreter and script paths. Third-party dependencies can be
installed normally; this project itself is not packaged or distributed via PyPI.
The local plugin contains only skill metadata/instructions and needs no MCP.

Windows requires the appropriate Fractal MIDI/USB driver; Linux needs ALSA MIDI
access and device enumeration. Native transports are implemented, but this
release was tested locally with mock/replay only. Report actual missing driver,
dependency, endpoint or permission errors; do not claim cross-platform USB tests.

## Inspection

```sh
python tools/axefx.py schema
python tools/axefx.py capabilities
python tools/axefx.py device list
python tools/axefx.py preset inspect
python tools/axefx.py preset geometry
python tools/axefx.py block list
python tools/axefx.py block inspect amp1
python tools/axefx.py catalog --type amp --parameter gain
python tools/axefx.py parameter get amp1 gain
python tools/axefx.py channel inspect amp1
python tools/axefx.py scene inspect --scene 2
python tools/axefx.py modifier inspect
python tools/axefx.py controller inspect
```

JSON has `schema_version`, `ok`, `backend`, `data` or `error`, and `warnings`.
Errors return exit code 2 with a stable error code. `--help` and `--version` are
text exceptions. Mock/replay results always identify that no USB was contacted.
Incomplete real/captured observations use null unavailable fields and
`complete: false`; empty lists must not be inferred from these nulls.

USB `preset inspect`, `preset geometry` and block inspection retrieve the
physical grid and cables. Use `--format text preset geometry`, with global
options before the command. Results use
`complete_geometry: true` independently of full-preset `complete`. Shunts have
position-based refs such as `shunt-r3c9`; `wire-rNcM` identifies a cable endpoint
in an empty cell, not an inserted block. Status-only system entries retain
`position: null`. Unknown effect IDs retain `block-N` refs. Two matching grid
reads and unchanged preset/scene/block status guard each topology inspection.
Replay advertises this capability only if the file contains a grid query/reply;
the original bundled fixture has none. Unsupported reply layouts fail explicitly.
See [routing guidance](routing.md) for faithful diagrams and tone placement.

Run CLI commands sequentially for a single device/state directory. Concurrent
commands return `DEVICE_BUSY`; retry inspection after the other command ends.
USB uses a conservative shared lock even if different artifact directories are
selected. This lock cannot exclude writes made through Axe-Edit or the front panel.

`catalog` lists types/instances. `catalog --type TYPE` returns the small authored
`minimal-mock-v1` definitions; `--parameter NAME` narrows output. Firmware is null.
There are 18 recognized types, with no editable NAM Player schema. Vendor caches,
UI labels and full model lists are not bundled. `Mock Clean`, `Mock Stereo` and
similar model options are simulation placeholders, not hardware model selections.
Current USB amp knobs expose the tested 0–10 display conversion. Other conversion
rules remain unresolved; `--raw` is a numeric parameter value, not MIDI.
Raw conversion is implemented only for the six listed amp knobs. Do not use
`raw: true` for delay/reverb or other simulated parameters: their numeric values
use authored `mock` units. Discover parameter names/options from catalog
rather than assuming that every block calls its model selector `model`.

## Preview and apply

For revisioned abstract designs, browser editing, editor/cache evidence imports,
SVG/PNG setting sheets, and local audio references, read
[the designer workflow](designer.md) and `docs/DESIGNER.md` in the checkout.
These local tools never silently contact a device. `design preview` is the bridge
to the existing exact-plan approval flow; `design compile` alone is diagnostic.

Convenience commands always preview:

```sh
python tools/axefx.py parameter set amp1 gain --value 6
python tools/axefx.py block add delay --ref delay1 --row 2 --column 6
python tools/axefx.py block connect cab1 delay1
python tools/axefx.py preset new --empty --name "Warm Clean"
python tools/axefx.py preset copy current --name "Warm Clean"
python tools/axefx.py preset save --slot 500
```

A batch proposal contains only `operations`; discover required/optional fields
using `schema`. JSON files are input data, never code. Example for a mock preset:

```json
{
  "operations": [
    {"op": "parameter.set", "block": "amp1", "parameter": "gain", "value": 3},
    {"op": "block.add", "type": "delay", "ref": "delay1", "row": 2, "column": 6},
    {"op": "block.connect", "from": "cab1", "to": "delay1"}
  ]
}
```

```sh
python tools/axefx.py --backend mock --state-dir /ABSOLUTE/DEMO/DIR edit plan --file .local/changes.json
# Show the preview and obtain user approval before the following command.
python tools/axefx.py --backend mock --state-dir /ABSOLUTE/DEMO/DIR edit apply PLAN_ID --approved
```

Plan IDs are immutable content hashes. Changing a plan file invalidates it.
Approval applies to one unchanged plan, including generated shunts. No preview
changes the device or simulated device state. Applying a plan stores a snapshot
and journal before attempting writes. Save must be a separate one-operation
batch; occupied slots require `overwrite: true` in that approved preview.

Moving a connected block may require explicit disconnect/reconnect operations
in the batch because physical cables must join adjacent columns. Routing never
replaces occupied cells or unrelated cables. Disconnecting a branching shunt
route requires identifying a physical edge explicitly.

Scenes select shared block channels; they do not own independent knob banks.
Modifier references must be `modifier1` through `modifier32`; inspect existing
assignments before choosing an available reference.
Manual edits to a modifier-controlled parameter are rejected in mock unless the
modifier is explicitly removed first. Modifier/controller schemas are simulated
until their wire formats are recovered; no global MIDI settings are edited.

## Recovery

```sh
python tools/axefx.py edit inspect PLAN_ID
python tools/axefx.py edit journal PLAN_ID
python tools/axefx.py snapshot inspect SNAPSHOT_ID
python tools/axefx.py edit restore SNAPSHOT_ID
```

Restore creates another preview; user approval is required to apply it. Full
USB edit-buffer snapshots are not yet implemented. USB numeric edits capture
only their targeted parameters plus preset/scene/channel context. The CLI
refuses to use those snapshots for full preset or topology restoration.

Mock restore changes only the edit buffer and preserves later saved presets.
To recover an overwritten saved destination, preview
`python tools/axefx.py --backend mock edit restore SNAPSHOT_ID --source-slot N`,
apply that approved buffer restoration, then separately preview and approve
`preset save --slot N --overwrite`. The snapshot before the original save holds
the destination backup. Restoration does not silently undo stored-slot changes.

`STALE_PLAN`/`CONTEXT_CHANGED`: inspect again, explain any changes, and create a
fresh preview. `APPLY_FAILED`: read the returned journal, completed/attempted
operations and restore status; never assert that the preset is unchanged when
restoration is unresolved. Previously attempted plans cannot be replayed.
