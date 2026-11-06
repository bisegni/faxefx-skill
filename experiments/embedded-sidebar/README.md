# Embedded Axe-Edit chat

This local macOS experiment embeds an AppKit sidebar inside a separate copy of
Axe-Edit III. The installed app is neither patched nor re-signed. The right pane
contains native Objective-C++ chat connected to the installed Codex or Claude
CLI through its existing local account login. No Rust helper or API-key entry
is involved. The agent can inspect the connected Axe-Fx and use scoped native
controls to change parameters, bypass, channels and scenes. Audio processing
is not connected.

Run from the repository root:

```sh
python3 experiments/embedded-sidebar/build.py --output .local/axe-edit-tone-agent
open '.local/axe-edit-tone-agent/Axe-Edit III Chat.app'
```

Choose a new output path for another build; the builder refuses to overwrite an
existing output directory. It compiles a
universal arm64/x86_64 library, copies the installed app, adds a local launcher,
and applies ad-hoc signatures to the copy. The copy omits the vendor's hardened
runtime configuration to load this library. The installed app keeps its original
signature and runtime configuration. No system security setting is changed.

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
protocols, based on the existing Rust adapters in `src/crates/agents/`.

The host wraps the original JUCE native view and reserves 300 points for an
AppKit sidebar. **Hide sidebar** reduces it to a 36-point toggle strip. Window
resizing uses the original editor's bottom-right resize grip, which moves with
the editor area. Toggling preserves the editor's dimensions and changes the
window width. The host accounts separately for the editor's 4:3 constraint and
the sidebar width. Chat remains available when Axe-Edit is disconnected from
the hardware.

The initial wrapper had a resize feedback loop. The v2 experiment also crashed
while dragging because its superclass lookup recursively called the injected
method after a runtime subclass was added. Those builds are superseded by v8.
The revised hook preserves the existing window class, captures original method
implementations, and distinguishes JUCE's frame conversions from AppKit's
internal frame calculations. Only the wrapped main window receives the custom
size constraint; other windows retain their saved implementations.

Run the focused native regression checks without launching the copied editor:

```sh
xcrun clang -fobjc-arc -fblocks -framework Cocoa experiments/embedded-sidebar/SelfTest.m -o .local/sidebar-layout-selftest
.local/sidebar-layout-selftest
xcrun clang++ -std=c++17 -fobjc-arc -fblocks -framework Cocoa experiments/embedded-sidebar/ChatAgentTest.mm experiments/embedded-sidebar/EditorTools.mm experiments/embedded-sidebar/NativeBridge.mm experiments/embedded-sidebar/MCPServer.mm -o .local/sidebar-chat-test
.local/sidebar-chat-test
xcrun clang++ -std=c++17 -fobjc-arc -fblocks -framework Cocoa experiments/embedded-sidebar/ChatPanelTest.mm experiments/embedded-sidebar/ChatAgent.mm experiments/embedded-sidebar/EditorTools.mm experiments/embedded-sidebar/NativeBridge.mm experiments/embedded-sidebar/MCPServer.mm -o .local/sidebar-chat-panel-test
.local/sidebar-chat-panel-test
```

Optional live checks, which send one fixed smoke-test message using the local
account and use isolated temporary conversation directories:

```sh
.local/sidebar-chat-test Codex
.local/sidebar-chat-test Claude
```

## Axe-Fx tone tools

`EditorTools.mm` supplies the guitar-tone system prompt and seven tools:

| Tool | Behavior |
| --- | --- |
| `axefx_inspect` | Read connected preset, scene, grid, selected effect/channel and visible numeric parameters |
| `axefx_select_block` | Select an existing grid effect using its inspected label |
| `axefx_set_parameter` | Change one visible numeric parameter with expected-value and context checks |
| `axefx_set_bypass` | Set the selected block's bypass state |
| `axefx_select_channel` | Select an existing A-D channel |
| `axefx_select_scene` | Select one of the eight existing scene slots |
| `axefx_name_scene` | Rename the current scene |

A tone request authorizes changes to the current edit buffer. The prompt tells
agents to inspect first, use observed controls, make small musical changes,
read back results, and ask for playing feedback. Build scenes using the fixed
eight slots, bypass states, channels and parameters. Parameter edits may affect
other scenes that use the same block channel, following normal Axe-Fx behavior.

The bridge serializes writes, rejects stale context and expected values, checks
turn authority, and retains operation IDs to prevent duplicate writes. Stop
revokes future tool actions immediately; it cannot undo a write already sent.
The process-local operation ledger does not replay mutations on restart.

Parameter writes use `NativeBridge.mm`, pinned to Axe-Edit III 1.15.03 arm64.
At startup it checks the executable's `__text` SHA-256, the `A3ComponentComm`
RTTI and vtable slot; any mismatch disables writes. For a knob it follows the
accessibility element to its `A3RotaryKnob` (RTTI-checked) and reads the
knob's own effect ID, parameter ID and display range, then its owning
`A3EffectEditorContent` and Undo context. The conversion comes from Axe-Edit's
current effect-definition cache (`raw = display / scale`) and is used only
when the knob's displayed range equals the cache range times its scale. The
write is the same command a typed knob entry issues (op `0x09`, flag 0, editor
Undo context), so Cmd-Z reverts it. Model selection (parameter 0), list
parameters and non-knob controls are refused; `axefx_inspect` reports
`writable` and `not_writable_reason` per parameter. No mouse, keyboard or
text-input synthesis is used. A readback mismatch sends the previously shown
value back and is reported. Results distinguish accepted sends from verified
editor readback. Full evidence: `docs/AXE_EDIT_PARAMETER_UPDATE_HANDOFF.md`. The scope is
`axe_edit_ui`; independent hardware getter and saved-preset persistence are not
verified by this bridge.

Effect model selection, block insertion/deletion, cable editing, preset Store,
raw MIDI, global settings and firmware operations are unavailable. The model
picker draws its rows without accessible model identities, so the agent cannot
choose an unverified row by guessed coordinates. Use the original UI for these
operations. Never run multiple editors against the device during editing tests.

Codex receives native App Server dynamic tools. Claude receives only these tools
through a bearer-authenticated loopback MCP endpoint; its token changes each
turn, browser Origin requests are rejected, and the private configuration is
removed on disconnect. No additional MIDI transport is opened.

Additional focused checks:

```sh
xcrun clang++ -std=c++17 -fobjc-arc -fblocks -framework Cocoa experiments/embedded-sidebar/EditorToolsTest.mm experiments/embedded-sidebar/EditorTools.mm experiments/embedded-sidebar/NativeBridge.mm -o .local/editor-tools-test
.local/editor-tools-test
xcrun clang++ -std=c++17 -fobjc-arc -fblocks -framework Cocoa experiments/embedded-sidebar/MCPServerTest.mm experiments/embedded-sidebar/MCPServer.mm experiments/embedded-sidebar/EditorTools.mm experiments/embedded-sidebar/NativeBridge.mm -o .local/sidebar-mcp-test
.local/sidebar-mcp-test
.local/sidebar-chat-test Codex --tools
.local/sidebar-chat-test Claude --tools
```

The last two checks use real local-agent logins and isolated temporary chat
history. They call inspection with no editor attached and never mutate hardware.
Fixture tests are not device evidence.

## Device-tool acceptance, 2026-11-05

| Check | Result |
| --- | --- |
| Native-bridge fail-closed, conversion guard, stale context, Stop, disconnect, idempotency and readback-mismatch restore | PASS, authored fixtures |
| Native write/restore, Amp 1 Gain 2.51 → 2.61 → 2.51 via the editor command path | PASS, live, editor UI readback |
| Loopback MCP authentication, Origin rejection, tool listing/call, main-thread dispatch and old-token rejection | PASS, local transport test |
| Codex and Claude invoke the native inspection tool and resume chat | PASS, real local CLIs, no editor attached |
| Connected preset/grid/scene/selected Amp numeric values | PASS, live Axe-Edit UI |
| Parameter change and restoration through the local agent | Pending final live acceptance |
| Scene/channel/bypass changes and scene naming | Pending final live acceptance |
| Independent hardware value getter, saved persistence, x86_64 runtime | UNVERIFIED |

## Chat acceptance, 2026-11-05

| Check | Result |
| --- | --- |
| Universal Objective-C++ build and copied bundle signature verification | PASS |
| Native stream/final deduplication, wrong-thread rejection, stopped-delta rejection | PASS |
| Composer and transcript widths at compact and normal sizes | PASS |
| Local Codex login, reply, disconnect, empty-session recovery and saved resume | PASS, live CLI |
| Local Claude login, reply, disconnect and saved resume | PASS, live CLI |
| Native pane Codex connection and reply | PASS, visually and accessibility-verified `PANEL_CHAT_OK` |
| Native pane Claude selection, connection and reply | PASS, visually and accessibility-verified `CLAUDE_PANEL_OK` |
| Existing window geometry regression checks | PASS |
| Live Stop timing and long sessions | UNVERIFIED; cancellation event rejection is covered by the focused native test |
| Device context, AI knob writes | See the device-tool acceptance below |
| Audio processing and audio sharing | NOT IMPLEMENTED |

## Observed acceptance, 2026-11-04

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
| AI or programmatic knob writes | UNVERIFIED; not attempted |
| Hardware value readback after an edit | UNVERIFIED; not attempted |
| Connected chat/audio backend in the initial layout proof | NOT IMPLEMENTED at that stage; chat is implemented above |
| x86_64 runtime, full screen, dialogs, long sessions | UNVERIFIED |

Installed executable SHA-256:
`647ba24fb04c2cfcbcb972b32f338b0422eb87c90ca7898cf26f39e9c5bd8435`

The builder records identity in the chosen output directory's `manifest.json`.
Attachment and layout logs are in `~/Library/Logs/AxeEditSidebarProof.log`.
Application updates can change the window/view structure; this proof does not
establish a stable vendor extension interface.
