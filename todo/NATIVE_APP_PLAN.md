# Axe Studio: approved implementation contract

Status: implementation authorized 2026-10-17. This contract supersedes the prior
Python-backend proposal. The approved concept remains the visual reference.

![Approved visual direction](axe-studio-live-audition.png)

## Architecture

Rust/Iced 0.14 owns the desktop UI, MIDI/device service, transaction authority,
agent adapters and SQLite persistence. Independent domain, session/device, agent,
UI and audio modules communicate through typed intents and snapshots. Python is
restricted to a standalone bundled audio-analysis worker over versioned private
stdio JSON-RPC, without imports from the legacy application or access to MIDI.

Five movable, dockable, resizable panes remain visible: preset navigation, block
inspector, routing canvas, agent chat and audio audition. Fixed global controls
include identity/status, Save tone and Restore original. Minimum client area is
1280 by 850 logical pixels, subject to real-content and DPI acceptance. Layouts
must reject missing/duplicate panes and arrangements below pane minimum sizes.
EffectWidgetRegistry and generic unknown widgets remain independent of providers
and MIDI. Cable simulation is visual only, bounded and idle when settled.

## Startup and authority

Open the native window first. USB is the default; errors offer endpoint selection
and retry, with no automatic mock fallback. Explicit Test mock is isolated.
Read device/preset/routing/capabilities before explicit Codex or Claude selection.
One provider controls a session. Explicitly ingest bundled axefx-live skill in a
read-only initialization turn; fail closed when restrictions cannot be verified.
Codex uses App Server stdio; Claude uses its supported headless interface and an
authenticated loopback MCP exposing only app tools. No unrelated tools, shell,
file mutation, hooks, connectors or delegation. Turns have revocable authority.
Save and Restore are user actions, never agent tools. Persist visible transcript
before sending, checkpoint streams and preserve interruption/provider boundaries.

## Transactions and audio

Serialize manual and agent edits. Validate context/generation/capabilities/types,
ranges and automation constraints. Durably capture raw originals and journal
idempotency before writes. Sequential writes require readback. Stop on failure;
verify context-safe rollback or enter recovery required. Never replay writes on
restart/reconnect. Unknown originals block mutation. Restore only touched targets;
Save requires explicit destination and verified save/reload semantics, then a new
baseline. Dirty close/preset switch offers Save/Restore/Leave unsaved/Cancel.
Physical setter experiments require a designated test preset and verified backup.
Begin with six existing amp-control candidates; mock success is no hardware proof.

Standalone audio supports bounded local excerpts, approved measurements, optional
version/hash-managed guitar separation and descriptive comparisons. Rust owns
jobs/artifacts/playback. Only explicitly attached summaries reach providers.
No automatic uploads. No system Python dependency in packages.

## Ordered delivery and gates

A. Audit existing work, run Python/frontend checks, checkpoint to main, then push
feat/axe-studio-native. Preserve legacy source during development and exclude it
from new application packages.
B. Rust workspace/storage/contracts, isolated mock and evidence-backed device
reads, five-pane workspace/effect registry/cables. Read-only responsive gate.
C. Both adapter contracts, explicit skill loading, durable chat, scoped tools,
audio worker/playback/attachments. Deterministic tests and separate real-provider
read-only demonstrations.
D. Original-value journal, serialized transactions/readback/idempotency/recovery;
injected failure tests before physical writes. Save/reload has its own gate.
E. Rust/domain/protocol/state-machine, layout/widgets/cables, provider isolation,
startup, failure/recovery and authored audio tests. CI on macOS/Windows/Linux;
clean packages independent of legacy and system Python. Mac package first with
commit/checksum/capability inventory/limitations/manual checklist. Stop for user
acceptance covering both providers, live audition, controls/layouts/history/audio,
Save/Restore and recovery. Signing requires available release credentials.
F. Only after explicit acceptance: remove legacy except standalone audio, verify
clean build, preserve external recovery bundle, create fresh root and replace main
with expected remote commit and force-with-lease. Review superseded refs then.

Automated checks, native/manual acceptance, real provider verification and physical
hardware acceptance are separate. Windows/Linux packages and hardware acceptance
remain release gates. Unimplemented or unverified capabilities stay unavailable.
