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
- Codex: App Server 0.163.x, explicit bundled skill, isolated tools and streamed chat.
- Claude: compatible headless 2.1.x (minimum 2.1.286), bare/restricted mode,
  authenticated loopback MCP and session-only API authentication. A supported
  Anthropic API key is required; consumer subscription login is not assumed.
- App-owned conversation and activity survive restart. No queued mutation is replayed.
- Local bounded audio import, waveform preview, playback, selected excerpts,
  attenuation-only A/B matching, measurements and explicit summary attachments.

Hardware mutations, hardware original restoration, device Save/reload, structural
routing edits and optional guitar separation are unavailable in this candidate.
Hardware setter conversions alone do not establish firmware identity, modifier
protection or complete backup/restoration. No fabricated save packet is sent.
Physical verification is performed by the user; no physical write experiment was
performed by the implementation agent. Windows/Linux hardware checks, all DPI and
accessibility acceptance, and Developer ID signing/notarization remain separate.
