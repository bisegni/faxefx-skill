# Development foundation verification — 2026-10-17

This records partial implementation evidence, not completed product acceptance.

| Check | Evidence |
| --- | --- |
| Existing Python baseline | 110 passed (`PYTHONPATH=src .venv-cli/bin/python -m pytest -q`) |
| Existing frontend baseline | 5 passed; TypeScript/Vite production build passed |
| Rust workspace | 25 tests passed, including one authored cross-language grid fixture |
| Rust quality | Workspace rustfmt and Clippy with `-D warnings` passed |
| Standalone audio | 3 tests passed: authored sine measurements, silence/bounds, stdio tool isolation |
| Mac launch | Debug executable launched as an unsigned development .app |
| Native visual smoke | All five panes rendered; explicit mock selection showed authored routing; Codex choice remained gated |
| Actual USB capture | Not verified in this run; discovery required endpoint selection |
| Physical writes/Save/Restore | Not performed; hardware mutation explicitly disabled |
| Real Codex/Claude | Not implemented or demonstrated; normalized contracts only |
| Minimum size/multiple DPI/accessibility | Not accepted; native AX tree currently exposes window chrome but not toolkit content |
| Cross-platform CI | Matrix defined; outcomes must be inspected separately |
| Installable audio/runtime package | Not implemented; preview packager does not constitute the required Mac handoff |

The native mock screenshot is local at `output/axe-studio-native-mock.png`.
The remaining implementation and acceptance gates are listed in `native/README.md`.
Cargo reports a transitive future-incompatibility notice for `block 0.1.6`; current
compilation, tests and Clippy pass. This does not establish future toolchain support.

No legacy source has been removed and no history has been rewritten. The prior
work/design checkpoint is `655ef6f` on main. All native implementation stays on
`feat/axe-studio-native`. User acceptance is still required before cleanup.
