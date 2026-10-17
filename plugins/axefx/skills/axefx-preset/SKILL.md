---
name: axefx-preset
description: Inspect Axe-Fx III presets, design tones from descriptions or local recordings, and refine revisioned block/cable drafts with a visual workbench through the Python CLI. Preview supported parameter, scene, channel, modifier and controller edits before approved application. Full automatic USB construction remains capability-gated. Not for firmware or CAB/NAM uploads.
---

# Axe-Fx III presets

Use `python tools/axefx.py` from the repository's Python 3.11 environment for every
device operation. This is a source-file CLI, not an installed app or PyPI package.
Resolve the repository from the active workspace and use an absolute script path
when running from another directory. Do not open MIDI/USB directly,
send raw SysEx, import transport internals, or invent parameter IDs or conversions.

Read [the CLI reference](references/cli.md) for commands, JSON proposals, setup,
and recovery. Discover the contract with `python tools/axefx.py schema` and inspect
`python tools/axefx.py capabilities` before advertising live operations.

For an abstract design, audio-reference request, professional diagram, or browser
canvas, read [the designer workflow](references/designer.md). The AI talks to the
Python backend through structured CLI commands; the browser uses the same backend
and revisions. Neither interface may invent wire formats or bypass approval.

## Open the UI first

For the integrated app workflow, run `python tools/axefx.py --state-dir PATH ui`.
The browser opens before the backend reads the connected USB preset. Let the UI
show connection errors and endpoint selectors; never substitute mock data.
Users can connect Codex and chat inside the app, review proposed differences,
and accept draft revisions there. In that embedded context, use the host's
`tone_*` tools only: they cannot apply/save the device or execute CLI/shell code.
Unknown captured bypass/channel values are null, not false or channel A.
Refresh creates a new capture/project and preserves prior drafts. Use the same
state directory when helping through an external CLI conversation.

## Workflow

1. Resolve the backend from the request. Normal hardware use defaults to USB.
   Use `--backend mock` for simulation or `--backend replay --replay-file PATH`
   for recorded reads. Never silently substitute either for a requested device.
2. Inspect the current preset, geometry, blocks and relevant catalog/parameters.
   For diagrams or tone construction, read
   [drawing and effect placement](references/routing.md). Lead with a short
   ASCII overview; draw the verified physical grid when showing a current preset.
   The bundled catalog uses authored mock definitions, not a firmware schema.
   Mock model names are placeholders, never real-device model selections.
   Missing geometry or wire
   semantics are capability gaps, not permission to guess a signal chain.
3. Translate the musical request into named operations. Prefer one batch plan
   for the desired edit-buffer changes. Consult catalog options/ranges and use
   CLI connection planning rather than manually generating cable MIDI packets.
4. Create a preview with `edit plan --file PATH` or a mutation convenience
   command. Show its before/after signal flow, value changes, shunt additions,
   warnings, and any destination overwrite. Ask the user to approve that
   concrete immutable plan. Permission to implement software, a general tone
   request, or a prior different plan is not approval to apply this preview.
5. Only after user approval, call `edit apply PLAN_ID --approved` with the same
   backend, endpoint selection and state directory. Reuse approval for that
   unchanged batch; do not ask separately for each parameter. If the plan becomes
   stale or changes, present its replacement and obtain fresh approval.
6. Report readback and restoration status, then support audition/refinement.
   Saving is a separate preview and approval with an explicit device slot.
   Never select an occupied destination or add overwrite permission on your own.

Writes are enabled normally; there is no experimental-write flag. The `--approved`
flag acknowledges approval supplied by the agent; it does not independently
authenticate a user event. Do not pass it speculatively or as part of a probe.

If an apply fails, inspect the journal and automatic snapshot first. Do not retry
the same plan or send a replacement blindly. A failed/unresolved restoration
means partial state may remain. Preview `edit restore SNAPSHOT_ID`, explain its
changes, and obtain approval before applying it. Artifacts survive disconnects.

## Current capability boundaries

- Mock supports full domain authoring and is clearly labeled simulated. Its
  parameter/controller behavior is not hardware evidence or audio simulation.
- Replay reads replies from the selected file and never writes. The bundled
  fixture is synthetic test data, not a hardware capture. Grid inspection is
  available only when the selected file includes a grid request/reply; never
  infer topology from block-status data alone.
- USB supports the recovered public reads, active amp getters, and read-only
  grid positions/cables through operation `002E`. `preset inspect`,
  `preset geometry`, and block inspection include verified topology, shunts,
  cable branches and merges. Check `complete_geometry` separately from
  `complete`: full parameter/scene coverage remains incomplete. Unsupported
  payload layouts and changing state return errors instead of guessed routing.
  Approved
  numeric writes for Gain/Bass/Mid/Treble/Master/Presence use statically recovered
  operation `0009`, read back results, and restore captured parameter values on
  failures where possible. These setters remain hardware-unverified.
- USB full-preset snapshots, structural editing, saving, scene edits,
  inactive-channel contents, modifiers and controllers are unresolved and return
  explicit errors. Do not claim these are available because they work in mock.

Never claim an exact recorded tone from parameter proposals or audio measurements
alone. Optional local audio analysis is not original-rig identification or audio
synthesis. Ask about guitar/pickups/monitoring only when it affects the
requested design. Device slots use numbers 0–1023; scenes use 1–8 and channels A–D.
