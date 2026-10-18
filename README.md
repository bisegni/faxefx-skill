# Axe Studio

Axe Studio is a native Rust/Iced desktop application for Axe-Fx III sessions.
Rust owns the UI, persistent MIDI connection, device state, provider integration,
storage, audio imports and playback. The bundled Python worker performs local
audio measurements only; it does not provide the application UI or device backend.

```text
Rust / Iced UI ── Rust backend ── Axe-Fx III MIDI
                       ├─────── Codex / Claude
                       └─────── Local audio measurement worker
```

## Run

```sh
cargo run -p axe-desktop --locked
```

Select MIDI endpoints and use **Read USB**, or explicitly choose **Test mock**.
Choose a provider after loading a device snapshot. The app keeps mock and hardware
state separate and never substitutes simulated values for a failed USB connection.
Audio analysis requires the packaged worker; development builds can set
`AXE_AUDIO_WORKER` to its executable path.

The selected block uses live token-driven parameter reads. Only validated numeric
conversions move knobs; unknown values remain unavailable or explicitly unverified.
Hardware parameter editing and Restore are disabled. Hardware Save sends one
user-operated store request after repeated context checks and durable journaling;
persistence still requires manual verification on the device.

See [native architecture and workflow](src/README.md),
[capabilities](src/CAPABILITIES.md), [verification](src/VERIFICATION.md),
[Mac acceptance checklist](src/MAC_HANDOFF.md), and
[source boundaries](docs/PROVENANCE.md).

## Repository

| Path | Purpose |
| --- | --- |
| `src/crates/` | Rust UI, backend, MIDI, agent adapters, persistence and audio |
| `src/resources/` | App-scoped agent skill and static effect identities |
| `src/audio-worker/` | Isolated audio measurement worker and its tests |
| `src/tools/` | Native package build and integrity checks |
| `docs/` | Source boundaries and recovered device-protocol evidence |
| `.github/workflows/native.yml` | Rust, worker and package CI |

The Python audio worker and Python package tools are support components of this
native application. There is no Python CLI, web server, browser frontend or
standalone preset plugin in the source tree.

## Verify and package

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

For worker verification and a complete package, use Python 3.11 in an isolated
developer environment:

```sh
python3 -m venv .venv-native-audio
.venv-native-audio/bin/python -m pip install -r src/audio-worker/requirements.txt pytest pyinstaller==6.20.0
.venv-native-audio/bin/python -m pytest -q src/audio-worker
.venv-native-audio/bin/python src/tools/package.py --candidate-tag local
```

On Windows, use `.venv-native-audio\Scripts\python.exe` for these developer commands.
The packager prints the generated candidate directory. Verify it with
`src/tools/check_package.py <candidate-directory>` using the same interpreter.
CI uses `src/tools/ci_package.py` to prepare its isolated packaging environment.
End users receive a frozen worker and do not install Python or Node.js.
Mac bundles receive an ad-hoc signature; Developer ID signing and notarization
remain separate release steps. Verification never substitutes for hardware acceptance.

## External Software

Direct Rust dependencies and all transitive versions are resolved in
[Cargo.lock](Cargo.lock); the manifests in `src/crates/` define required features.

| Software | Version / source | Purpose |
| --- | --- | --- |
| Rust / Cargo | Stable; [rustup](https://rustup.rs/) | Build and verify the Rust workspace |
| Iced | 0.14.0; [iced-rs](https://github.com/iced-rs/iced) | Native UI and rendering |
| Tokio | 1.x; [Tokio](https://tokio.rs/) | Async UI tasks |
| Serde / serde_json | 1.x; [Serde](https://serde.rs/) | Typed serialization and JSON protocols |
| thiserror | 2.x; [repository](https://github.com/dtolnay/thiserror) | Typed errors |
| rusqlite / SQLite | 0.38; [repository](https://github.com/rusqlite/rusqlite) | Bundled database and durable application state |
| midir | 0.10; [repository](https://github.com/Boddlnagg/midir) | MIDI input/output |
| directories | 6.x; [repository](https://github.com/dirs-dev/directories-rs) | Platform application-data locations |
| rfd | 0.15; [repository](https://github.com/PolyMeilex/rfd) | Native file dialogs |
| uuid | 1.x; [repository](https://github.com/uuid-rs/uuid) | Session and artifact identifiers |
| tiny_http | 0.12; [repository](https://github.com/tiny-http/tiny-http) | Scoped loopback agent-tool transport |
| sha2 | 0.10; [RustCrypto](https://github.com/RustCrypto/hashes) | Artifact integrity |
| rodio | 0.21; [repository](https://github.com/RustAudio/rodio) | Local decoding and playback |
| Python | 3.11 for developer packaging; [python.org](https://www.python.org/) | Isolated audio worker and package scripts |
| NumPy / SciPy / SoundFile | 2.4.6 / 1.17.1 / 0.13.1; [NumPy](https://numpy.org/), [SciPy](https://scipy.org/), [SoundFile](https://python-soundfile.readthedocs.io/) | Worker measurements; pinned in `src/audio-worker/requirements.txt` |
| PyInstaller | 6.20.0; [PyInstaller](https://pyinstaller.org/) | Freeze the worker and interpreter for distribution |
| pytest | Developer test dependency; [pytest](https://docs.pytest.org/) | Worker tests |
| Codex | App Server 0.163.x; [OpenAI Codex](https://github.com/openai/codex) | Optional authenticated provider runtime |
| Claude Code | 2.1.x, minimum 2.1.286; [Claude Code](https://code.claude.com/docs/) | Optional headless provider with an Anthropic API key |
| OS build libraries | Linux: ALSA, udev, xkbcommon development packages | Native audio, device and window integration; installed by CI |
| macOS bundle tools | System `codesign`, `ditto`, `plutil` | Local Mac packaging and signature checks |

Provider authentication stays outside source control. Local audio is not uploaded
automatically; agents receive only explicitly attached summaries.
