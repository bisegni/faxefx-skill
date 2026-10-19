# Mac manual handoff

This is an internal candidate, not a release or a claim of complete hardware
support. See CAPABILITIES.md before testing. Build identity, file hashes, signature
status and source-dirty flag are in the adjacent `manifest.json`. The zip has a
`.sha256` companion. The latest package always replaces `dist/axe-studio/` and
`dist/axe-studio.zip`.
Open `Axe Studio.app`; it bundles the audio interpreter and dependencies.

## Manual checklist

1. Launch without a terminal or system Python. Confirm the native window appears
   before discovery; USB failure must not silently choose mock.
2. Start the app before powering on the Axe-Fx. Confirm endpoint discovery finds it after power-on without restarting the app. If it fails, note the status before trying Retry. In Test mock, use Auto layout, resize the window and run Auto layout again. Verify smaller effect cards, clear cable gaps, all five panes, drag/swap them, save
   and reload a named layout, reset, and check 1280×850 plus your display scales.
3. Select Codex, verify explicit skill initialization, send a tonal direction,
   inspect streaming/tool activity and stop mid-turn. Confirm no later mutation
   follows Stop. Repeat with Claude using a supported session-only API key.
4. Adjust mock controls; confirm readback, Restore, explicit simulated Save
   destination, dirty-close Save/Restore/Leave/Cancel and persisted restart state.
5. Restart and switch providers. Confirm visible history remains, partial messages
   remain marked, and no mutation is replayed or duplicated.
6. In Reference audio & audition, select an OS audio input (Axe-Fx USB audio if
   available, or an interface), record a short guitar passage and stop. Confirm it
   appears as Your recording with a waveform. Analyze a bounded excerpt, then
   explicitly attach its summary before asking the agent to assess it. Raw audio
   remains local. Also import a reference file, compare with level matching, and
   check playback and measurements.
7. Connect Axe-Fx for routing/parameter inspection. Unknown values must remain
   unknown. For hardware Save, first back up a disposable destination in Axe-Edit.
   Press Save tone, check the explicit 0–1023 destination and confirm overwrite.
   The application checks the captured context, journals and sends one store,
   then waits 10 seconds. Confirm it reports **sent, persistence unverified**.
   Verify the destination on the device, then manually reload it and confirm the
   tone persists. Do not treat this request or a simulated Save as a backup.
   Hardware Restore and live parameter edits remain unavailable.
8. Check disconnects, provider failure, audio errors, keyboard access and display
   scaling. Report the exact step, provider version and visible error.

Hardware parameter writes, hardware Save persistence/acknowledgement and Restore, optional guitar separation,
real Claude authentication, cross-platform physical checks and full accessibility
remain open gates. Completing the checklist does not resolve the missing hardware
implementation. No destructive cleanup, main replacement or history rewrite is
performed before explicit acceptance of the completed application.


## Live synchronization candidate

Build-only candidate: `dist/axe-studio/Axe Studio.app`.
The implementation agent does not launch this candidate or access MIDI hardware.

Manual acceptance:

1. Quit competing editors, launch the candidate yourself and use Read USB.
2. Select Amp 1. Turn front-panel Gain; verify the device text and knob follow,
   with no parameter editing enabled in Axe Studio. Check channel and scene changes.
3. Scroll the inspector. Newly visible rows should read promptly; cached/offscreen
   rows must not appear newly confirmed merely because another block changed.
4. Select additional effect types. Device text may be shown with an unverified
   getter label; unknown numeric conversions must leave knobs unavailable.
5. Change the selected model and preset, then unplug/reconnect USB. Obsolete values
   must clear. Use Read USB after a timed-out connection.
6. Check explicit scene selection and the existing Save dialog. Confirm the Save
   result describes transmission only. Verify persistence on the device manually.

Automation verifies injected protocol behavior, packaging and code quality only;
front-panel latency, exact viewport behavior and hardware persistence remain open.
