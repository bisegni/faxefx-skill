# Architecture

This project adds a docked AppKit chat panel to a separate copy
of Axe-Edit III. The installed app is neither patched nor re-signed. The panel
contains native Objective-C++ chat connected to the installed Codex or Claude
CLI through its existing local account login. No API-key entry is involved. The agent can inspect the connected Axe-Fx and use scoped native
controls to change parameters, bypass, channels and scenes. Audio processing
is not connected.

`scripts/patch-axe-edit` finds the installed Axe-Edit III, compiles a
universal arm64/x86_64 library from `src/`, copies the app to
`~/Applications/Axe-Edit III Chat.app` (asking before replacing an earlier
copy), adds a local launcher, and applies ad-hoc signatures to the copy. The
copy omits the vendor's hardened runtime configuration to load this library.
The installed app keeps its original signature and runtime configuration, and
the script verifies its hash is unchanged. No system security setting is
changed.

| Path | Role |
| --- | --- |
| `src/ui/ChatDock.m` | Finds the Axe-Edit window and docks the Tone Assistant panel beside it |
| `src/ui/ChatPanel.mm` | Chat view: header, transcript, composer, busy indicator |
| `src/agent/ChatAgent.mm` | Local Codex (App Server) and Claude (stream-json) sessions |
| `src/agent/MCPServer.mm` | Private loopback MCP server exposing the Axe-Fx tools to Claude |
| `src/editor/EditorTools.mm` | Tool schemas, system prompt, inspection and guarded mutations |
| `src/editor/NativeBridge.mm` | Version-pinned native parameter writes through Axe-Edit's command path |
| `src/ui/AudioStrip.mm` | Audio section: input picker, record/meter, import/drop, clip list |
| `src/audio/AudioAnalysis.mm` | Local measurements and reference/current comparison (AVFoundation + Accelerate) |
| `src/audio/AudioClips.mm`, `AudioRecorder.mm` | Clip library and capture from a chosen input |
| `src/audio/AudioTools.mm` | Read-only agent tools: `axefx_audio_list/analyze/compare` |
| `tests/` | Fixture tests run by `scripts/test` |

The copied editor keeps Axe-Edit's ordinary device communications and may share
the vendor's existing user preferences. A copied bundle is not a sandbox or a
separate hardware session. The injected library opens no MIDI connection and
sends parameter changes through Axe-Edit's own Undo-aware command path
(`NativeBridge.mm`), so the editor's normal device queue carries them. The local agents communicate with their normal
services using their existing logins. Avoid running multiple editors against
the device when testing editing behavior.

## Chat

Choose **Codex** or **Claude**, then **Connect**. Enter a message and click
**Send**. **Stop** interrupts a running reply. **Disconnect** releases the local
agent; reconnecting resumes its saved conversation. Provider switching requires
disconnecting first. Codex uses App Server over stdio; Claude uses the local
authenticated CLI with streaming JSON output. An empty Codex session that has
no saved rollout starts afresh on reconnect; nonempty history is retained if
the remote session cannot be resumed.

Install and sign in to the desired CLI before connecting (`codex login` or
`claude auth login`). Tested runtimes are Codex 0.166.0 and Claude Code 2.1.318.
This panel removes inherited API-key variables from child environments, keeps
the local login, and strips the sidebar's injected-library variables from those
children. Codex built-in tools are disabled and verified; other MCP servers and
skills are disabled for the conversation. Claude runs restricted, with built-in
tools, hooks, slash commands and unrelated MCP configuration disabled. These
controls apply only to the child sessions; user settings are not rewritten.

History and provider session IDs are stored separately per provider under
`~/Library/Application Support/AxeStudioEmbeddedChat/{Codex,Claude}/conversation.json`.
Conversation files use atomic writes and owner-only file permissions. Prompts
travel through stdin, not process arguments or the layout log. The panel sends
entered chat messages and the actual Axe-Edit context requested through its
scoped tools. Audio is not attached.

`ChatPanel.mm` owns AppKit controls. `ChatAgent.mm` implements the local-agent
protocols.

The chat lives in its own **Tone Assistant** panel (`src/ui/ChatDock.m`). Axe-Edit's
window is left completely stock: its JUCE peer drives live resizing with a
fixed 4:3 constrainer, and every attempt to share that window with a pane
fought it (snapping, then a frozen resize when JUCE's frame requests were
held). The panel is attached as a child window flush against the editor's
right edge, or its left edge when the screen has no room, so it moves,
minimizes and changes Spaces with the editor and re-matches its height after
editor resizes. Its width (260-700 points) is user-adjustable and remembered.
**Detach window** turns it into a free window, with its position remembered;
**Dock to Axe-Edit** snaps it back. Chat remains available when Axe-Edit is
disconnected from the hardware.

The earlier in-window sidebar (v1-v8, then a split-pane attempt) is
superseded; its rows in the table below are historical.

Tested with installed Axe-Edit III 1.15.03 on arm64 macOS:

| Check | Result |
| --- | --- |
| Compile universal sidebar library | PASS |
| Copied bundle strict signature verification | PASS |
| Sidebar inside the original window | PASS, visually checked |
| Type into sidebar field | PASS, accessibility readback: `Sidebar focus works` |
| Select original Amp 1 block | PASS, original knob editor displayed |
| Read original knob labels and values | PASS, Gain 2.51 and Bass 3.32 exposed through accessibility |
| Collapse, resize using original grip, reopen | PASS, original editor reflows and typed text remains |
| v8 sidebar toggle height | PASS, editor remains 1200 x 900 while host changes between 1500 and 1236 points wide |
| v8 resize with sidebar open | PASS, editor 1100 x 825 and sidebar 300 x 825, visually checked |
| v8 dragging and resizing acceptance | PASS for exercised interactions; user reported it seems to work |
| Frame conversion regression with KVO observation | PASS, 10000 calls and observer cleanup without recursion |
| Native AppKit sizing and editor aspect regression | PASS, no fake native margin and 11 widths in expanded/collapsed modes |
| Installed executable unchanged | PASS, SHA-256 below |
| Docked panel: editor resize/move, panel width, detach and re-dock | PASS, user-checked 2026-11-05; geometry self-test |
| AI knob writes through the agent | PASS, user-checked live 2026-11-05 (native bridge) |
| Hardware value readback after an edit | UNVERIFIED; not attempted |
| Connected chat/audio backend in the initial layout proof | NOT IMPLEMENTED at that stage; chat is implemented above |
| x86_64 runtime, full screen, dialogs, long sessions | UNVERIFIED |

Installed executable SHA-256:
`647ba24fb04c2cfcbcb972b32f338b0422eb87c90ca7898cf26f39e9c5bd8435`

The patcher records the source app's path, version and hash in the copy's
`Contents/Resources/AxeChatManifest.json`.
Attachment and panel logs are in `~/Library/Logs/AxeEditSidebarProof.log`.
Application updates can change the window/view structure; this proof does not
establish a stable vendor extension interface.
