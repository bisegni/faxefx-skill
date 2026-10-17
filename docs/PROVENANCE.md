# Source and repository boundaries

This checkout contains the project-written Python CLI, Codex/Claude skills,
user guides, tests and small authored mock definitions. No project software
license is selected yet.

The bundled catalog is `minimal-mock-v1`, recognizing 18 types with no firmware
version claim. Mock model names and parameter ranges are simulation choices.
Numeric interoperability IDs do not make a complete hardware schema; USB writes
remain restricted to the six recovered amp controls, still hardware-unverified.

The optional `catalog import-editor` command stores user-selected installed
editor/cache definitions in the local artifact store with source SHA-256 values.
These local imports are not bundled catalog data and do not establish validated
units, instance limits or device compatibility. Audio imports and model weights
also stay in local state. Use an ignored state directory, never tracked source.

`tests/fixtures/synthetic-traffic.jsonl` is authored deterministic test data,
identified by its first metadata record. It contains no captured device bytes.
Replay verifies file parsing and behavior, not real hardware interoperability.

Copied vendor binaries/resources, extracted XML, firmware, disassembly, bulk
catalogs, analysis tools/results/reports/diagrams and real hardware captures are
absent from this checkout. They are preserved in a private archive outside the
repository for recovery; do not upload that archive with the source. The unused
research environment is archived there too. The active `.venv-cli` environment
and `.local` runtime/cache directories are ignored by Git.

This boundary does not certify legal clearance. Review rights to newly added
data/assets and choose a project license before public distribution. Dependency
sources are listed in the README; dependencies are installed locally rather than
vendored. `.gitignore` helps exclude private data, but `git add -f` bypasses it.
