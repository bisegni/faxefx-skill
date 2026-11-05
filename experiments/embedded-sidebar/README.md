# Embedded Axe-Edit sidebar proof

This local macOS experiment embeds an AppKit sidebar inside a separate copy of
Axe-Edit III. The installed app is neither patched nor re-signed. This is a
layout and focus proof; chat, audio processing, and AI parameter control are not
implemented here.

Run from the repository root:

```sh
python3 experiments/embedded-sidebar/build.py --output .local/axe-edit-sidebar-layout-v8
open '.local/axe-edit-sidebar-layout-v8/Axe-Edit III Sidebar Proof.app'
```

The builder refuses to overwrite an existing output directory. It compiles a
universal arm64/x86_64 library, copies the installed app, adds a local launcher,
and applies ad-hoc signatures to the copy. The copy omits the vendor's hardened
runtime configuration to load this library. The installed app keeps its original
signature and runtime configuration. No system security setting is changed.

The copied editor keeps Axe-Edit's ordinary device communications and may share
the vendor's existing user preferences. A copied bundle is not a sandbox or a
separate hardware session. The injected library itself has no MIDI, network,
provider, or parameter-write code. Avoid running multiple editors against the
device when testing editing behavior.

The host wraps the original JUCE native view and reserves 300 points for an
AppKit sidebar. **Hide sidebar** reduces it to a 36-point toggle strip. Window
resizing uses the original editor's bottom-right resize grip, which moves with
the editor area. Toggling preserves the editor's dimensions and changes the
window width. The host accounts separately for the editor's 4:3 constraint and
the sidebar width. The placeholder field sends nothing.

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
```

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
| Connected chat/audio backend | NOT IMPLEMENTED |
| x86_64 runtime, full screen, dialogs, long sessions | UNVERIFIED |

Installed executable SHA-256:
`647ba24fb04c2cfcbcb972b32f338b0422eb87c90ca7898cf26f39e9c5bd8435`

The builder records identity in the chosen output directory's `manifest.json`.
Attachment and layout logs are in `~/Library/Logs/AxeEditSidebarProof.log`.
Application updates can change the window/view structure; this proof does not
establish a stable vendor extension interface.
