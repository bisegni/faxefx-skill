# Source and repository boundaries

This checkout contains the Rust/Iced Axe Studio application, its authored tests,
app-scoped agent skill, static effect identities, local audio measurement worker
and native package tooling. Workspace crates declare the MIT license in
`Cargo.toml`. External dependencies are installed through Cargo and the isolated
worker environment; their resolved versions appear in `Cargo.lock` and
`native/audio-worker/requirements.txt`.

The static effect identities in `native/resources/axe-effects/identities.json`
include their provenance. Static names and parameter IDs do not establish firmware
compatibility, numeric units or write capabilities. The authored protocol fixtures
in `native/crates/session/tests/fixtures/` are test inputs, not hardware acceptance.
TestMock values remain isolated simulations.

The bundled audio worker performs local measurements over a private JSON-RPC
boundary. It does not own MIDI, UI, application state or provider conversations.
No audio is uploaded automatically. User recordings, databases, credentials,
private research and device captures do not belong in tracked source.

Generated builds, packages, Python environments and local working state are ignored
by Git. The package manifest records included files and hashes; package verification
checks the frozen-worker boundary. Device-protocol evidence lives in
[AXE_EDIT_STORE_PROTOCOL.md](AXE_EDIT_STORE_PROTOCOL.md). Hardware and manual UI
acceptance remain separate from automated tests and static protocol recovery.
