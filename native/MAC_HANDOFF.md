# Mac manual handoff

This is an internal candidate, not a release or a claim of complete hardware
support. See CAPABILITIES.md before testing. Build identity, file hashes, signature
status and source-dirty flag are in the adjacent `manifest.json`. The zip has a
`.sha256` companion. Packages are generated under `dist/axe-studio-darwin-arm64-release/`.
Open `Axe Studio.app`; it bundles the audio interpreter and dependencies.

## Manual checklist

1. Launch without a terminal or system Python. Confirm the native window appears
   before discovery; USB failure must not silently choose mock.
2. Start the app before powering on the Axe-Fx. Confirm endpoint discovery finds it after power-on; use Retry if needed. In Test mock, use Auto layout, resize the window and run Auto layout again. Verify all five panes, drag/swap them, save
   and reload a named layout, reset, and check 1280×850 plus your display scales.
3. Select Codex, verify explicit skill initialization, send a tonal direction,
   inspect streaming/tool activity and stop mid-turn. Confirm no later mutation
   follows Stop. Repeat with Claude using a supported session-only API key.
4. Adjust mock controls; confirm readback, Restore, explicit simulated Save
   destination, dirty-close Save/Restore/Leave/Cancel and persisted restart state.
5. Restart and switch providers. Confirm visible history remains, partial messages
   remain marked, and no mutation is replayed or duplicated.
6. Import reference and recording files, choose bounded excerpts, play/pause/stop,
   compare with level matching, inspect measurements and attach only selected
   summaries. Ordinary import/analysis stays local; attached summaries go to the
   selected provider when requested through app tools.
7. Connect Axe-Fx for read-only routing/amp inspection. Unknown values must remain
   unknown. Hardware Save, Restore and live edits must remain unavailable in this
   candidate. Do not treat simulated Save as a restorable hardware backup.
8. Check disconnects, provider failure, audio errors, keyboard access and display
   scaling. Report the exact step, provider version and visible error.

Hardware parameter writes, hardware Save/reload/Restore, optional guitar separation,
real Claude authentication, cross-platform physical checks and full accessibility
remain open gates. Completing the checklist does not resolve the missing hardware
implementation. No destructive cleanup, main replacement or history rewrite is
performed before explicit acceptance of the completed application.
