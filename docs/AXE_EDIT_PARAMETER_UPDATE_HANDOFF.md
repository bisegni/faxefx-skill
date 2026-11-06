# Axe-Edit parameter-update research handoff

> Paths below the update refer to the pre-cleanup layout (`experiments/embedded-sidebar/`, `src/crates/`); that code now lives in `src/` or was removed.

## Update, 2026-11-05 (later): implemented in `src/editor/NativeBridge.mm`

With renewed user authorization (passive observation, then one bounded
write/restore, no Store), the open questions below were answered:

| Question | Answer, with evidence |
| --- | --- |
| Live instance | The knob's own object chain, all RTTI-checked: AX `JUCEAccessibilityElement.handler` → `juce::AccessibilityHandler` (+0x08) → `A3RotaryKnob`; knob +0x30 → `A3EffectEditorContent` (primary vtable `0x1011682d8`); +0x118 is its `A3ComponentComm` base (vtable `0x1011685c0`, slot +0xa0 → `0x1000c8d9c`). |
| Undo context | `comm + 0x300` in all three observed sessions; checked by RTTI (`UndoManager`) before use. |
| Parameter identity | `A3RotaryKnob` +0x210 u16 effect ID, +0x212 u16 parameter ID (Amp 1: Gain 1, Bass 2, Mid 3, Treble 4, Master 5, Presence 20, Level 21, Balance 22, Input Trim 47, matching the `DISTORT_*` symbol tables). |
| Normal call | Typed entry: record `{effect, parameter, op 0x09, raw}`, flag 0, Undo context, main thread. Drag: op 0x52, flag 1. ParameterAction then re-invokes with no context (caller `0x1000e9c94`); Undo replays via `0x1000e9cf4`. The record's fourth word is not meaningful. |
| Conversion | Axe-Edit's effect-definition cache record `(raw_min, raw_max, scale, step)`; Gain is `0,1,10,0.001`, display 2.51 → raw 0.251 (observed). Not universal: e.g. Delay type 13 parameter 16 is `0.05,50,10`. The bridge requires the knob's own display range (+0x23c/+0x240) to equal raw range × scale. |
| Cache type per block | Executable's type registry (name/ID pairs ending at `0x10119fd48`, e.g. 10 Amplifier, 13 Delay, 25 Drive) plus `ID_*` symbol tables (four instances per type). |
| Live write | Amp 1 Gain 2.51 → 2.61 → 2.51 through the native call; editor UI read back both values. No Store. |

Remaining unverified: an independent hardware getter, whether Cmd-Z undoes a
bridge-issued write, x86_64, and other Axe-Edit or firmware versions (the
bridge disables itself on any other executable).

Date: 2026-11-05. Repository: `/Users/bisegni/dev/github/bisegni/faxefx-skill`.

## Current direction and authorization

The user wants the local Codex/Claude agent in the embedded right pane to edit
the actual Axe-Fx: parameters, effects and scenes, with a guitar-tone system
prompt. The user then explicitly stopped the accessibility/mouse/text-entry
implementation and requested reverse engineering of a better value-update
method. This handoff records that investigation. **Continue research, not the
abandoned UI automation approach. Do not treat this document as authorization
to call a native setter, send hardware writes, or save a preset.**

```text
Stopped: agent -> accessibility / double-click / text entry -> knob

Recommended candidate for further investigation:
agent -> typed native bridge -> A3ComponentComm -> Axe-Edit queue -> Axe-Fx
                                      |
                              ParameterAction / Undo
```

No successful live parameter write/restore round trip was verified. Earlier
bounded attempts targeted Amp 1 Gain, initially displayed as 2.51, with intended
2.61 then restoration to 2.51. The bridge rejected the numeric-editor identity
checks before sending a replacement value. Some attempts left a numeric editor
open or made the normal accessibility tree unavailable. A later inspection
reported `connected=false`; that is not independent proof of a hardware
disconnect. Do not infer final device state from that UI observation. No
Save/Store was performed during this work.

## Target identity

| Item | Verified value |
| --- | --- |
| Installed application | `/Applications/Axe-Edit III.app` |
| Executable | `/Applications/Axe-Edit III.app/Contents/MacOS/Axe-Edit III` |
| Version | 1.15.03, read from Info.plist |
| Universal executable SHA-256 | `647ba24fb04c2cfcbcb972b32f338b0422eb87c90ca7898cf26f39e9c5bd8435` |
| Analyzed architecture | arm64 |
| ARM64 slice offset / length | 19644416 / 19124768 bytes |
| Disassembler | REA 6.5.0, Hopper provider |
| Address convention below | Unslid ARM64 virtual addresses, not file offsets or live pointers |

The installed application was not patched or re-signed. Experimental copies
live under `.local/axe-edit-device-tools-v1` through `v7`. Do not launch one as
a shortcut for this research; these contain the incomplete UI-based bridge.

Old research concerns version **1.15.00**, executable SHA-256
`9a5b86e6e55b0bc8b64b44d1818434fa4e67fa023e5d0f4e1cb7e74eba741dd4`.
Its addresses must not be reused in 1.15.03 without verification.

## Confirmed static findings in 1.15.03

### Shared parameter-command entry point

`0x1000c8d9c` is a shared method on **A3ComponentComm**. This class identity is
supported by the constructor, RTTI and Mach-O chained fixups, rather than an
invented decompiler name:

- Constructor `0x1000ca848` installs vtable address point `0x101140108`.
- The vtable type-info pointer at `0x101140100` targets `0x10114b9d8`.
- Its name pointer at `0x10114b9e0` targets `0x10064f548`.
- Bytes at `0x10064f548` decode to `15A3ComponentComm`.
- Vtable slot `+0xa0`, at `0x1011401a8`, targets `0x1000c8d9c`.
- Numerous other vtables also reference this shared implementation.

The method receives an object pointer in `x0`, a parameter-command record in
`x1`, a flag in `w2`, and an optional action/Undo context in `x3`. **This is a
register-level observation, not a verified callable C++ declaration.**

Observed record reads establish this 12-byte leading layout:

| Offset | Read | Meaning supported by the serializer path |
| --- | --- | --- |
| `+0` | uint16 | effect/block instance ID |
| `+2` | uint16 | parameter ID |
| `+4` | uint32 | inner editor operation |
| `+8` | float32 | raw value |

At `0x1000c90a4..0x1000c90bc`, these fields are copied to the common editor
request buffer. The method then calls serializer `0x100309638` and queue
submission helper `0x1000ca518`. The flag influences pending-write/coalescing
behavior; its complete contract has not been established. Do not guess it.

### ParameterAction and Undo

RTTI identifies **ParameterAction** at string `0x10064f76d`. Chained fixups
connect this to type info `0x101140730` and vtable address point `0x101140700`.

- Action method `0x1000e9c50` reconstructs the new command and calls the owner's
  virtual method at `+0xa0`.
- Companion method `0x1000e9cac` reconstructs the prior command/value and calls
  the same virtual method. This is the supported basis for identifying an Undo
  path; it has not been exercised by this investigation.
- `0x1000c8d9c` has an optional-context branch that allocates this action,
  captures previous state and calls `0x10042f30c`.
- `0x10042f30c` invokes an action's virtual `+0x10` method and maintains action
  history, consistent with an UndoManager perform path.

Passing a null Undo context is a different branch. Do not claim that merely
calling the shared method automatically preserves normal editor Undo behavior.

### Existing queue and transport

`0x1000ca518` allocates a `0x2b0`-byte request object, constructs it with
`0xbb8` (3000), copies the message, tracks it in the owner and submits it through
`0x100285b94`. The timeout interpretation is consistent with the lower device
path, but complete cancellation/acknowledgment semantics remain unverified.

The lower generic sender is `0x10010f058`. Its leading 72 bytes uniquely match
the older sender at `0x1000fd458`, and current REA analysis confirms the field
stores, common serializer, request allocation, queue and response-wait path.
It accepts effect ID, parameter ID, operation and `s0` raw float. It is not the
preferred UI integration point: it is lower-level and waits for a result.

Current call sites pass inner operation **`0x0009`**. Earlier static research
identifies that operation as numeric parameter writing. This does not prove a
safe general setter for every parameter, modifier, effect model or channel.
The common serializer `0x100309638` packs the inner fields and forwards them to
the outer SysEx builder. Preset Store is a separate operation.

### Special cases exist above the shared method

`0x1001b42c8` forwards ordinary parameter commands to the shared method but
handles parameter ID zero specially, with additional editor operations `0x3a`
and `0x3b` in some branches. Its adjusting thunk is `0x1001b4ad4`.

This is evidence against treating effect model/type selection as an ordinary
knob write. The full semantic identity of that caller and those operations is
not established. Preserve the distinction; do not expose model selection yet.

## Recommendation and alternatives

| Approach | Assessment |
| --- | --- |
| Typed bridge into the actual editor's command/action layer | Best candidate for this embedded-sidebar architecture. Reuses the editor's queue and has an evidenced action/Undo path. Live object discovery, ABI, conversion, callbacks and thread contract still need proof. |
| Call a rotary widget's internal setter | Possible fallback research subject, but coupled to visible controls, subclasses and listener notification. No verified setter or component-pointer recovery was completed. |
| Send private editor SysEx from a second MIDI client | Poor fit while Axe-Edit owns the session. Risks competing queues and stale editor state. More appropriate to a separately owned transport after protocol and synchronization validation. |
| Accessibility setters, mouse events or typing | Abandoned at the user's request. They did not produce a verified live parameter edit. |

Fractal's official [third-party MIDI guide, revision 1.4](https://www.fractalaudio.com/downloads/misc/Axe-Fx%20III%20MIDI%20for%203rd%20Party%20Devices.pdf)
documents bypass, channel and scene set/get commands. It does not document the
generic private editor parameter-write path described above. A native bridge
is therefore version-specific reverse engineering, not a supported vendor SDK.

## Required next research, before implementation

1. Obtain the live A3ComponentComm/control-manager instance through an owned
   editor object relationship. Prove its type, lifetime, vtable and executable
   identity. Do not scan arbitrary memory and invoke the first matching pointer.
2. Passively observe the normal call chain for a user-operated knob change:
   record effect ID, parameter ID, raw value, flag, Undo context, calling thread,
   queued request and update notification. Do not synthesize an edit as part of
   this research-only handoff.
3. Establish the conversion between display values and raw values using current
   firmware metadata and the editor's actual conversion functions. Gain display
   `2.51` is not evidence that the wire/raw value should be `2.51`.
4. Establish active-channel binding, modifier handling, current preset identity,
   pending-request behavior and stale-context checks. Determine how editor state
   and visible controls refresh after the command completes.
5. Verify the action/Undo integration and distinguish queue acceptance, device
   acknowledgment, independent value readback and preset persistence.
6. Produce a narrow implementation proposal with a fail-closed executable hash
   guard and ASLR-aware address resolution. A future bounded write/restore test
   requires renewed user authorization following the stop in this conversation.

Unknowns: callable ABI, supported thread, live instance discovery, conversion
contract, complete record semantics, flag meanings, Undo context ownership,
modifier/model behavior, queue completion and hardware readback. No claim of a
ready-to-call native setter is justified yet.

## Evidence and reproducibility

Selected complete REA records are saved locally in:

`.local/axe-edit-parameter-research/evidence.json`

This is a roughly 0.9 MB local evidence artifact, not a committed source file.
It includes the target identity, pseudocode, assembly, relationships and stated
provider limitations. Hopper's public API does not expose complete function
body ranges; pseudocode is a recovered representation, not original source.
Hopper missed some data references that are present in chained fixups.

| Finding | REA Evidence ID |
| --- | --- |
| Shared method / action construction | `ev_6c0c4f103c3aeb1ddbc2c74954b4f1671d4dd9085a0db53368b1e1e9c9a2deb7` |
| ParameterAction new-value method | `ev_78f79e4e8c3ae9dcca97df9ec7adb400a5294d353d945f09fa22a110ab42a1b8` |
| ParameterAction prior-value method | `ev_18490a6633508dbbb2d3737d27ff76f335f4bea1b2552e20a3b4d16b8262e412` |
| Queue submission helper | `ev_2884ef80e186100b9c3610ef2d54b86801ef662b4676ccfb5da80bd47be990f7` |
| Lower sender | `ev_a85738574b8e6a786d42aef8ae054a5dd7300c88c9cdc7ba4cfd717ccefda56c` |
| Serializer | `ev_61cdef0e200e0767d64e76012bc97a61b2d4cfa5dfd3a03ea131012d61866a52` |
| A3ComponentComm name bytes | `ev_ff4f580e81d83b1b05489e98d5fc72d3fdb893f41a7d823a4de59dcca86be257` |
| Undo-style action execution | `ev_f2bcfabfcda6690fafacde114987084018a6f397099dcc188eccc4f766f21e4b` |

Read-only reproduction commands:

```sh
shasum -a 256 '/Applications/Axe-Edit III.app/Contents/MacOS/Axe-Edit III'
/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' '/Applications/Axe-Edit III.app/Contents/Info.plist'
xcrun dyld_info -fixups -arch arm64 '/Applications/Axe-Edit III.app/Contents/MacOS/Axe-Edit III'
```

In REA, open `/Applications/Axe-Edit III.app`, confirm the digest above, then
analyze `0x1000c8d9c`, `0x1000ca518`, `0x1000e9c50`, `0x1000e9cac`,
`0x10010f058` and `0x100309638`. Keep output focused: full dossiers can be large.

Historical local reference:
`/Users/bisegni/dev/github/bisegni/axefx-research-private/20261013T224618Z/before-cleanup/REPORT.md`.
Its 1.15.00 addresses and firmware-32.6 metadata are historical evidence only.
The current Rust implementation explicitly keeps
`hardware_parameter_writes:false` in `src/crates/runtime/src/lib.rs`.

## Working tree left for the next agent

Branch: `feat/axe-edit-editor-ui`. Chat-only baseline: `43530c1`.
At final handoff verification, HEAD is
`9a3edf1c062a1ce787d7d67222173438ff09a1b3` (`Add MCPServer and EditorTools test suite`).
The tool implementation was committed in the shared workspace while this
investigation was ongoing. This research agent did not run commit or push;
remote publication was not checked. Re-read Git state before continuing.

Additions included in that commit under `experiments/embedded-sidebar/`:
`EditorTools.h/.mm`, `EditorToolsTest.mm`, `MCPServer.h/.mm`,
`MCPServerTest.mm`. Modified integration files: `ChatAgent.mm`,
`ChatAgentTest.mm`, `ChatPanel.mm`, `Sidebar.m`, `build.py`, `README.md`.
The only visible untracked source artifact at handoff verification is this
handoff document; the evidence JSON is under ignored `.local/`.

The Objective-C++ implementation contains seven tool schemas and a guitar-tone
system prompt. Codex uses App Server dynamic tools; Claude uses a private
loopback MCP server. Both real local CLIs successfully called inspection in
isolated chat tests with no editor attached. Native fixture tests exercised
context checks, expected values, Stop, disconnect, duplicate operation IDs,
numeric-entry identity, bypass/channel/scene and naming behavior. Transport and
layout tests also passed at exercised revisions. **Those are not hardware
write proof, and the current tool implementation has not completed acceptance.**

`EditorTools.mm` still contains the abandoned mouse/text-input fallback. Do not
mistake the existence of tools, passing authored fixtures, or README capability
descriptions for a working production parameter bridge. Preserve the user's
work and inspect the diff before removing/replacing anything. Do not build,
launch, commit or push merely to conduct the next read-only research stage.
