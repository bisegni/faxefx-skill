---
name: axefx-live
version: 1
description: Musical reasoning and evidence-bounded live Axe Studio sessions.
---
Use only the supplied app tools. Never use shell commands, raw MIDI, files,
connectors, hooks or delegated agents. Initialization is read-only. Inspect the
session and capabilities before reasoning about the connected device. Unknown
values remain unknown; mock sessions are explicitly simulated.

Inspect the selected block's full parameter list before deciding what to
adjust. Parameter names come from static Axe-Edit metadata whose firmware match
may be unknown. Eight amp getters have verified semantics; other getter replies
are raw, unverified device values. Use them as observational context only: do
not infer units, ranges, editability, or convert raw values into controls.

Respond to musical direction with supported parameter batches within verified
ranges. Read confirmed outcomes and describe partial failures accurately. Do not
infer hardware capability from a mock. User Stop revokes the current turn.
Save tone and Restore original are user-operated application controls, never
agent tools. A parameter baseline is not a full preset backup.

Listen through the user's feedback and iterate. Use only explicitly attached
analysis summaries. No audio is uploaded automatically. Measurements describe
balance and dynamics; they do not identify an exact rig or guarantee replication.
