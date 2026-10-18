# Axe Studio candidate capabilities

Rust owns UI, MIDI, scoped agent tools, transactions, persistence, artifacts and
playback. The bundled Python worker performs local measurements only.

- USB: fixed read-only routing/scene/status plus catalogued parameter getters
  for recognized block instances. The bundled static Axe-Edit 1.15.00 catalog
  names 174 block instances and 42 parameter families. Eight amp getters have
  verified semantics; other getter replies are preserved as raw device values
  and remain unverified until physical testing. Static metadata firmware match
  is unknown. Unknown effects/parameters are never guessed.
- Test mock: serialized parameter batches with originals, readback, rollback,
  cancellation, Restore original and explicitly selected simulated Save destination.
- Hardware Save tone: explicit overwrite destination (0–1023), captured endpoint
  binding, repeated preset/scene/routing/channel preflight, durable request journal,
  one store transmission and a 10-second settling window. The result is explicitly
  **sent, persistence unverified**. Echoes are not acknowledgements; there is no
  automatic retry, reload or mutation replay. Test on a backed-up disposable slot.
- Codex: App Server 0.163.x, explicit bundled skill, isolated tools and streamed chat.
- Claude: compatible headless 2.1.x (minimum 2.1.286), bare/restricted mode,
  authenticated loopback MCP and session-only API authentication. A supported
  Anthropic API key is required; consumer subscription login is not assumed.
- App-owned conversation and activity survive restart. No queued mutation is replayed.
- Local bounded audio import, waveform preview, playback, selected excerpts,
  attenuation-only A/B matching, measurements and explicit summary attachments.

Hardware parameter mutations, hardware original restoration, verified device
Save/reload, structural
routing edits and optional guitar separation are unavailable in this candidate.
Hardware setter conversions alone do not establish firmware identity, modifier
protection or complete backup/restoration. No fabricated save packet is sent.
Physical verification is performed by the user; no physical write experiment was
performed by the implementation agent. Windows/Linux hardware checks, all DPI and
accessibility acceptance, and Developer ID signing/notarization remain separate.

Static reverse engineering identifies editor operation `0x0026` inside outer
command `0x01` as Axe-Edit's preset-store request, with the destination in the
32-bit value field. It powers the user-operated Save action; acknowledgement handling
and save/reload persistence remain unverified. See the
[instruction trace and offline verifier](../docs/AXE_EDIT_STORE_PROTOCOL.md).
