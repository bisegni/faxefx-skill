# Axe-Fx III Python CLI and AI skill

Inspect and edit Axe-Fx III presets through `python tools/axefx.py`. Codex and
Claude Code use the same high-level CLI and require approval of the exact preview
before applying changes. The project runs from source; no application build or
PyPI installation is needed.

```text
Your prompt -> Codex / Claude skill -> Python CLI
                                      |-> mock device
                                      |-> recorded traffic
                                      |-> USB MIDI
```

## Quick start

Run from this checkout with Python 3.11 and optional `uv`:

```sh
uv venv --python 3.11 .venv-cli
uv pip install --python .venv-cli/bin/python -r config/requirements/cli.txt
source .venv-cli/bin/activate
python tools/axefx.py --backend mock --state-dir .local/demo --format text preset geometry
```

The example uses simulation and sends no USB traffic. For pip, Windows and full
manual commands, see [the CLI guide](docs/CLI.md).

## Install and use the AI skill

| Client | Project skill | Invocation |
|---|---|---|
| Codex | `.agents/skills/axefx-preset/SKILL.md` | `$axefx-preset` |
| Claude Code | `.claude/skills/axefx-preset/SKILL.md` | `/axefx-preset` |

[Installation instructions](docs/SKILL_INSTALLATION.md) cover project-local setup,
personal installation and optional plugins for both clients. Start with an
explicit mock request: “Inspect my preset and preview a warmer clean tone.”

## Capabilities and limits

The optional **Tone workbench** adds shared AI/browser design revisions, editable
blocks and cables, deterministic logical/physical previews, block setting sheets,
and local audio-reference measurements. The Python backend serves both interfaces;
the browser never applies MIDI edits. See [the designer guide](docs/DESIGNER.md)
for setup, examples, protocol coverage and exact hardware gates.

```sh
python tools/axefx.py protocol coverage
python tools/axefx.py --backend mock --state-dir .local/studio design create --name "Studio Clean"
# Use the returned project ID; build web/ as documented before opening the UI.
python tools/axefx.py --backend mock --state-dir .local/studio design open PROJECT_ID
```

Mock supports preset geometry, routing, named parameters, scenes, channels,
modifiers, controllers and save/restore using the small authored catalog. Replay
reads replies from a selected file; the bundled example is synthetic test data.
USB retrieves grid positions, shunts and physical cable branches/merges, alongside
the recovered public/active-amp reads and six statically recovered
amp setters; these setters remain hardware-unverified. Real grid editing,
full-preset snapshots and saving are unresolved.

Draw the connected device's current grid without changing the preset:

```sh
.venv-cli/bin/python tools/axefx.py --format text preset geometry
```

The skill includes [diagram and tone-placement guidance](plugins/axefx/skills/axefx-preset/references/routing.md).

The bundled catalog recognizes 18 types with authored simulation choices, not
complete vendor model/parameter lists. See [the CLI guide](docs/CLI.md)
and [provenance](docs/PROVENANCE.md) for coverage and repository boundaries.
No project software license is selected yet.

## Repository layout

```text
config/                 dependency lists and pytest configuration
src/axefx_cli/          Python library and authored mock definitions
tools/axefx.py          direct CLI entrypoint
tests/                  mock/replay tests and authored synthetic fixture
plugins/axefx/          shared skill and both plugin manifests
docs/                   CLI guide, skill installation and provenance
.local/                 ignored state and cache
```

The root contains this README plus the folders above. Hidden Git/client metadata
and the existing Python environments stay in their conventional locations.
Analysis tools, reports, diagrams, hardware captures and the research environment
are removed from this checkout. Copied vendor binaries/resources, firmware and
bulk catalogs remain outside this Git tree.

## Development checks

```sh
uv pip install --python .venv-cli/bin/python -r config/requirements/dev.txt
.venv-cli/bin/python -m pytest -c config/pytest.ini --rootdir=. -q
```

Run from the repository root; the explicit config path keeps pytest settings
under `config/`. Tests use mock/replay and injected transports, without USB.
Cross-platform CI is configured; remote execution is unverified.

## External Software

| Dependency | Version | Source | Purpose |
|---|---|---|---|
| Python | 3.11 baseline | [python.org](https://www.python.org/) | Direct CLI runtime; no application installation. |
| filelock | `>=3,<4` | [filelock](https://py-filelock.readthedocs.io/) | Cross-process CLI operation lock. |
| platformdirs | `>=4,<5` | [platformdirs](https://platformdirs.readthedocs.io/) | Platform-specific artifact directories. |
| python-rtmidi | `1.5.8` | [maintainer documentation](https://spotlightkid.github.io/python-rtmidi/rtmidi.html) | Native MIDI transport; imported lazily for USB use. |
| uv | optional; locally `0.12.1` | [Astral uv](https://docs.astral.sh/uv/) | Environment/dependency setup; not required at CLI runtime. |
| Codex CLI / desktop client | optional; CLI syntax checked with `0.162.1` | [Codex](https://developers.openai.com/codex/) | AI host for the Codex skill; not needed for manual CLI use. |
| Claude Code | optional; syntax checked with `2.1.296` | [Claude Code](https://code.claude.com/docs/en/overview) | AI host for the Claude skill; not needed for manual CLI use. |
| pytest | `>=8,<10`, development only | [pytest](https://docs.pytest.org/) | Mock/replay test runner. |
| PyYAML | `>=6,<7`, development only | [PyYAML](https://pyyaml.org/) | Skill metadata validation. |
| Fractal USB/MIDI driver | platform/vendor-specific | [Fractal downloads](https://www.fractalaudio.com/axe-fx-iii-downloads/) | Windows device access; CLI does not install drivers. |
| React / React Flow | locked in `web/package-lock.json` | [React](https://react.dev/) / [React Flow](https://reactflow.dev/) | Optional local visual designer. |
| Vite / TypeScript / Vitest | locked in `web/package-lock.json` | [Vite](https://vite.dev/) / [TypeScript](https://www.typescriptlang.org/) / [Vitest](https://vitest.dev/) | Frontend build and graph tests. |
| NumPy / SciPy / soundfile | optional audio requirements | [NumPy](https://numpy.org/) / [SciPy](https://scipy.org/) / [soundfile](https://python-soundfile.readthedocs.io/) | Local audio decoding and measurements. |
| audio-separator | optional separation requirements | [maintainer repository](https://github.com/uziproj/audio-separator) | Local guitar stem extraction; model inference needs separate acceptance. |
| Pillow | optional visual requirements | [Pillow](https://pillow.readthedocs.io/) | CLI PNG export without a native Cairo dependency. |

