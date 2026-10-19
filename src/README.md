# Axe Studio native application

Axe Studio is an Iced 0.14 desktop application. Rust owns the window, MIDI,
transactions, provider integration, storage, audio artifacts and playback. A
bundled standalone Python process performs local audio measurements over private
versioned stdio JSON-RPC. End users do not install Python or start a server.

The Rust workspace is the application entry point. The Python audio worker and
native packaging scripts are its only Python support components. The root
[README](../README.md) describes setup, dependencies and repository layout.

Start development with `cargo run -p axe-desktop`. A device must be selected
before choosing Codex or Claude. USB failure offers endpoint selection and retry;
Test mock is an explicit separate target. The provider chooser is shown each
launch. The initialization turn explicitly loads the bundled skill, exercises a
read-only app tool and verifies its version before chat is enabled.

## Components

- `axe-domain`: typed contexts, targets, parameters and unknown values.
- `axe-session`: SQLite journals/originals, serialized guarded transactions,
  partial failure recovery, persistent isolated mock and fixed USB read queries.
- `axe-agents`: Codex App Server and Claude headless adapters, normalized events,
  revocable per-turn tools, scoped authenticated loopback MCP and interruption.
- `axe-runtime`: one in-process authority for UI and agent commands, durable chat,
  provider sessions and explicit analysis attachments; exclusive application lock.
- `axe-audio`: user-started system-input capture, content-addressed bounded
  imports, decoding, waveform, excerpt matching, playback and supervised
  frozen-worker jobs. Raw recordings stay local; only attached summaries reach
  the assistant.
- `axe-desktop`: five dockable/resizable panes, named layouts, effect registry,
  generic unknown effects, damped cables, chat, audio and Save/Restore controls.

App-owned SQLite retains visible messages and tool activity, including interrupted
responses. Restart never replays a mutation. Mock edit buffers and originals are
stored separately from hardware sessions. Save and Restore are user commands;
providers cannot call them. Closing drains an active turn before reading dirty
state and offers Save, Restore, Leave unsaved or Cancel when needed.

See [CAPABILITIES.md](CAPABILITIES.md) for enforced support boundaries and
[MAC_HANDOFF.md](MAC_HANDOFF.md) for the manual checklist. Hardware parameter writes,
hardware Restore and optional guitar separation are unavailable. Hardware Save
sends the recovered store request once to an explicitly selected destination after
context checks; the app reports transmission with persistence unverified. Mock
Save persists a simulated destination and is not proof of device saving.

## Build and verification

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
python -m pytest -q src/audio-worker
python src/tools/ci_package.py
```

The last commands are developer commands in an isolated environment. The package
script freezes the pinned audio dependencies/interpreter, builds Rust, copies
only app resources, generates a hash manifest and archives the candidate. Mac
packages receive an ad-hoc signature; Developer ID/notarization needs release
credentials. CI defines macOS, Windows and Linux build/test/package jobs.

Supported provider prerequisites are Codex 0.163.x with its authenticated runtime,
or Claude Code 2.1.x >=2.1.286 and an Anthropic API key. Claude credentials can be
entered for the current application session; they are not saved. Consumer
subscription access is not assumed. Additional protocol versions need compatibility
tests. Claude's real authenticated gate remains pending a user credential.
