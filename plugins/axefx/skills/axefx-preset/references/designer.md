# AI design and visual workbench

Use the same Python CLI for design artifacts, local audio analysis, and device
operations. Read `docs/DESIGNER.md` in the checkout for complete contracts and
setup. Browser and AI share revisioned artifacts; there is no second AI provider
inside the application.

## From a description

1. Inspect `schema`, `capabilities`, and `protocol coverage`. For a real device,
   do not translate through mock model names or ranges.
2. For refinement, `design capture` records the current physical grid. For a new
   idea, `design create --name NAME` creates a logical draft without touching the
   device. Create/import a design file to propose a custom chain.
3. Use stable node IDs, named settings, explicit edges and short musical rationale.
   Preserve any unknown captured data. Use scene/channel/modifier/controller
   fields documented by `schema`; scenes select shared block channels.
4. `design revise PROJECT --base REVISION --file PATH` saves a replacement revision.
   On `REVISION_CONFLICT`, inspect both versions and merge explicitly. Do not
   overwrite newer browser changes.
5. `design compile PROJECT` reports buildability and blockers without sending
   MIDI. Show a concise ASCII chain plus `design render` SVG/PNG and relevant
   block setting sheets. Do not portray logical layout as observed hardware.
6. `design open PROJECT` opens the optional local browser canvas. The frontend
   needs its documented npm build; the CLI itself still runs from source.
7. When supported, `design preview PROJECT` creates an ordinary immutable plan.
   Obtain approval of that exact plan before `edit apply ID --approved`.
   A newer design revision invalidates the old design-bound plan. Saving remains
   a separate preview with an explicit destination.

Full USB construction remains blocked by unresolved firmware/catalog semantics,
backup/restore and structural setters. Show those blockers and alternatives.
Do not call mock success an Axe-Fx build or use raw MIDI to work around a gate.

## From a recording

Import the user-selected local file with `audio import --file PATH`, then analyze
a relevant excerpt using `audio analyze --audio-id ID --start SECONDS --duration
SECONDS`. Choose the target guitar part and excerpt with the user when ambiguous.
Full mixes can use optional `--separate`, requiring the optional local separation
runtime/model. Never silently upload audio or install a different cloud service.

Have the user audition the separated guitar. Attach analysis/audio IDs to the
design's `references`, with the excerpt and uncertainty. Read the report before
proposing a chain. Measurements are not listening evidence or original-rig
identification. Clearly distinguish measured spectral/dynamic properties from
hypotheses about amp family, saturation, modulation, delay and room sound.

Use actual validated model choices when available; otherwise retain model
preferences as unresolved design intent. Ask about pickups and monitoring only
when they materially affect the proposal. Do not manufacture exact settings or
promise an identical recorded tone.

After an approved hardware build, import/analyze a recorded result and use
`audio compare REFERENCE_ANALYSIS_ID RECORDING_ANALYSIS_ID`. Explain differences
and level-matching guidance, then propose another revision. User listening is
the acceptance criterion; there is no automated tone-accuracy score or automatic
reamping/search in this release.
