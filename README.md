# Axe-Edit Tone Assistant

A guitar-tone chat assistant docked to **Fractal Audio's Axe-Edit III**. It uses
the Codex or Claude CLI already logged in on your Mac. Describe the sound you
want, and the agent inspects the connected Axe-Fx III preset and changes knobs,
bypass, channels and scenes for you, through Axe-Edit itself.

It runs as a separately signed **copy** of Axe-Edit. Your installed Axe-Edit is
never modified.

## Requirements

- macOS on Apple silicon.
- Axe-Edit III **1.15.03**. Other versions can still chat and inspect, but knob
  changes are disabled (see Safety).
- Xcode Command Line Tools: `xcode-select --install`.
- A logged-in [Codex](https://github.com/openai/codex) CLI (`codex login`), or
  Claude Code (`claude auth login`).

## Install

```sh
scripts/patch-axe-edit
```

The script:
1. Finds Axe-Edit: `/Applications`, then `~/Applications`, then Spotlight.
2. Checks its version and code hash.
3. Builds the chat library from `src/`.
4. Installs `~/Applications/Axe-Edit III Chat.app`.

Re-run it after pulling changes; it asks before replacing the previous copy.

| Option | Effect |
| --- | --- |
| `--dry-run` | Only report what it found |
| `--app PATH` | Use a specific Axe-Edit |
| `--output PATH` | Install somewhere else |
| `--force` | Replace without asking |
| `--yes` | Accept prompts |

## Use

1. **Launch:** quit the regular Axe-Edit (only one editor should talk to the
   Axe-Fx), then `open ~/Applications/"Axe-Edit III Chat.app"`.
2. **Connect:** the **Tone Assistant** panel docks to the editor's right edge.
   Choose Codex or Claude and press **Connect**.
3. **Ask:** for example *"warmer clean for scene 2"* or *"set Amp 1 gain to
   6"*. Return sends; Shift-Return starts a new line. The spinner shows while
   the agent works, and the round button stops it.
4. **Match a tone from audio:** open the **Audio** section under the status
   line.
   - **Import the reference:** click the file icon, or drop files onto it.
     Ideally use isolated guitar.
   - **Record your tone:** pick your input (the Axe-Fx III USB audio is
     preselected when present), press ● and play the same part. Each clip
     can be marked **Ref** or **Mine**.
   - **Ask the agent:** for example *"compare my tone to the reference"*. It
     measures both clips locally, explains the differences and proposes
     specific knob changes. It applies them only after you agree, then asks
     for a new take to compare again.
   - **Privacy:** audio never leaves your Mac; the agent sees only numeric
     measurements.
5. **Start over:** the pencil icon starts a new chat with a fresh agent
   session. Audio clips are kept.
6. **Arrange:** drag the panel's edge to widen it. The title-bar icon detaches
   it into a free window and docks it back.

## Safety

- **Knob changes are normal Axe-Edit edits.** They go through the same command
  as typing into a knob, so **⌘Z undoes them**. The agent never saves or stores
  presets; saving stays up to you.
- **Writes are pinned to Axe-Edit III 1.15.03 arm64.** On any other build the
  library disables writes by itself, after checking the code hash, type
  information and vtable.
- **Every write is checked first and read back after.** Each write requires
  the current value and a range verified against Axe-Edit's own firmware
  definitions. Model selection and list parameters are refused.
- **Chat stays local.** It goes only to your local CLI's normal service.
  History is kept in `~/Library/Application Support/AxeStudioEmbeddedChat/`.

## Development

```sh
scripts/test                 # fixture tests, no Axe-Edit or hardware needed
scripts/patch-axe-edit       # rebuild and reinstall the copy
```

Layout and design: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md). The
reverse-engineering evidence behind native writes is in
[`docs/AXE_EDIT_PARAMETER_UPDATE_HANDOFF.md`](docs/AXE_EDIT_PARAMETER_UPDATE_HANDOFF.md).
Logs are in `~/Library/Logs/AxeEditSidebarProof.log`.

macOS asks for microphone permission the first time you record. Re-running
the patcher re-signs the copy, so macOS may ask again.

## Uninstall

Delete `~/Applications/Axe-Edit III Chat.app`. Optionally also delete
`~/Library/Application Support/AxeStudioEmbeddedChat/`, which holds chat
history and audio clips.

This project is not affiliated with Fractal Audio Systems.
