# Drawing presets and placing effects

Read this reference when showing a preset's signal path or designing a tone.
Use the repository CLI for all device reads and previews.

## Draw the current preset from evidence

Run sequentially with the same backend and endpoints:

```sh
python tools/axefx.py preset inspect
python tools/axefx.py --format text preset geometry
```

The JSON supplies identity, scene, channels, bypass, positions and physical
`connections`; text supplies a six-row, fourteen-column ASCII grid and cable list.
Coordinates are one-based. Inspect `capabilities` first for replay: a capture
without a grid reply cannot supply routing. A failed USB grid read is a missing
observation, never authorization to switch to mock or sketch a plausible chain.

Lead with the preset name, device slot and active scene, then a simple ASCII
overview. Use `complete_geometry: true` to decide whether routing is known.
`complete: false` still applies when parameters, inactive channels or full scene
contents are unavailable; it does not invalidate successfully retrieved geometry.

- Keep every arrow grounded in a returned `connections` edge. Adjacent cells
  have no implied cable. Show unconnected blocks as unconnected.
- Preserve rows/columns in the full grid. For a compact overview, abbreviate
  names and collapse only shunts with exactly one incoming and one outgoing
  cable. Retain explicit junctions at every split and merge. When a drawing
  becomes ambiguous, include the CLI's physical cable list.
- Keep bypassed effects in the drawing and mark them, for example `[Drive1*]`
  with `* = bypassed`. Do not infer their bypass mode or whether tails remain.
  Include active channel letters where useful; do not guess missing settings.
- Use `===` for shunts and `+` for empty-cell cable endpoints. An empty-cell
  endpoint is not an effect or proof that a complete audio path exists.
- Keep unknown IDs labeled `block-N`; do not invent effect/model names.
  Status entries with `position: null` belong outside the audio-grid drawing.
- Label hardware reads, file replay and mock simulation clearly. Label an
  unexecuted layout `PROPOSED`. Never present a tone proposal as the current grid.

If an image accompanies the ASCII drawing, generate a deterministic SVG/PNG
from the same positions and edges. Reuse the same bypass and unknown-ID legend;
do not use a generated illustration to invent or approximate wiring. Keep the
initial explanation short and link larger diagrams for inspection.

## Choose positions while building a tone

Choose the musical order before choosing cells. Start with this conventional
proposal and adapt it to the requested sound; do not add every optional effect:

```text
PROPOSED logical order (no device changes)
[Input] -> [Comp] -> [Drive] -> [Amp] -> [Cab] -> [Mod] -> [Delay] -> [Reverb] -> [Out]
```

Place a drive before the amp to change how the amp breaks up. Compression before
the amp controls picking dynamics; post-amp compression serves a different
leveling purpose. Prefer post-amp/cab delay and reverb for clear repeats and
space; pre-amp delay or spring reverb is a deliberate alternative when the tone
calls for distorted repeats or vintage behavior. Modulation before gain gives a
different interaction than post-gain modulation. Explain the choice briefly.

Match the amp/cab path to the monitoring setup. Preserve an established real
amp or guitar-cab feed. Ask about monitoring only when it materially changes
the design. Check the actual stereo/mono behavior before placing a summing
block after stereo effects; a position alone does not prove stereo preservation.

For an empty proposed grid, row 3 leaves space above and below for branches.
Keep Input near column 1, Amp/Cab near columns 5/6, and Output near column 14,
with pre effects to the left and modulation/time effects to the right. These
are layout preferences, not fixed addresses. Adapt to existing occupancy and
the requested blocks; retain useful free columns for later additions.

On an existing preset, preserve working positions and unrelated branches.
Use spare cells before proposing a wholesale rearrangement. Put parallel paths
on adjacent rows, align their split and merge columns, and minimize crossing
cables. Routing is left-to-right between adjacent columns; gaps need explicit
shunts and cables. Use `block.connect`/batch planning to discover the generated
shunts, and inspect the exact preview rather than inventing intermediate routes.

Prefer a serial chain for a simple tone. For independent wet effects, a conceptual
parallel layout is:

```text
PROPOSED logical routing (physical shunts/cells resolved by the plan)
[Amp/Cab] --+----------------------+--> [Out]
           +--> [Delay, wet only] -+
           +--> [Reverb, wet only]-+
```

In a parallel wet-only design, avoid duplicating the dry signal in each branch:
request 100% wet and balance branch level when the real block supports it.
Choose bypass behavior to preserve the intended dry path and tails. Translate
musical percentages into documented CLI units; do not assume `100` is a valid
numeric value or that mock Mix/Level definitions establish USB support.
Check sum levels, stereo behavior and the requested spillover when those
parameters are available. Do not assert they were checked from geometry alone.
See the vendor's [Blocks Guide](https://www.fractalaudio.com/downloads/manuals/fas-guides/Fractal-Audio-Blocks-Guide.pdf)
for block-specific mix, bypass and routing behavior.

## Preview and verify the placement

For supported editing backends, put related blocks and connections in one batch.
Show before/after routing, coordinates, generated shunts, bypass/channel changes
and the immutable plan ID before requesting approval. After applying an approved
plan, inspect again and draw the readback; do not reuse the proposed diagram as
proof of success.

USB currently retrieves topology but cannot apply block placements or cable
edits. For a hardware tone that requires them, supply an explicitly proposed
layout and manual Axe-Edit/front-panel steps, with coordinates and cable pairs.
Do not call a mock apply a hardware build. Re-read USB geometry after the user
implements those steps to verify the resulting layout. Numeric amp edits still
require approval of their own concrete preview; saving remains a separate action.
