# Axe-Edit III preset-store protocol recovery

Static recovery on 2026-10-17 identifies **outer SysEx command `0x01`, inner
editor operation `0x0026`** as the operation used by Axe-Edit's **Save Preset**
state machine. This is distinct from writing a `.syx` file to disk and from
the `0x77–0x79` preset-transfer family. Hardware acceptance and persistence
have not been tested by this investigation.

## Artifact and method

- `/Applications/Axe-Edit III.app`, version 1.15.00, ARM64 slice.
- Original universal executable SHA-256:
  `9a5b86e6e55b0bc8b64b44d1818434fa4e67fa023e5d0f4e1cb7e74eba741dd4`.
- REA/Hopper confirms common editor serialization at `0x10026aba4`, Evidence
  `ev_1419ba8e9a6e2e05c1286e73ea4f368b4191d699f5c91cf1ef7eba1150d3b78e`.
- REA/Hopper confirms state-machine sequence insertion at `0x10008bc74`,
  Evidence `ev_86de6eff9b639e04a5784a0f5919d789e8c16fc20e35cb34f27c1c42af24f3c5`.
- Focused ARM64 instruction reads and the embedded jump table establish the
  following chain. The recorded evidence identifies the executable hash,
  branch destinations, operation constant, field stores and timeout.
- No Axe-Edit process was executed and no MIDI endpoint was opened.

## Recovered chain

```text
"Save Preset" at 0x10057adf0
  registration at 0x1000cf250..0x1000cf278: sequence [0, 0x11, 1]
    dispatcher jump table at 0x10057e926, entry 0x11
      -> case 0x1000d3f44
         destination loaded from command object +0x198
         call 0x10026b274 at 0x1000d3f58
           -> operation 0x26, target 0, parameter 0,
              value_bits = destination argument, data_length 0
           -> tail call common builder 0x10026aba4 at 0x10026b29c
              -> outer command 0x01 via 0x10026cfa8
         queue request via 0x1000d53a8 with flag 1 and timeout 10000 ms
```

The label is attached to the same state-machine object whose sequence contains
step `0x11`. That step is an internal app identifier. It is **not** the wire
opcode. Following its branch to the common serializer establishes `0x0026`
as the on-wire editor operation.

The serializer writes six fields before optional binary data: operation
(two low-first 7-bit bytes), target (two), parameter (two), a 32-bit value
(five), index (two), and data length (two). It encloses the result in outer
command `0x01` with the usual Axe-Fx III header and XOR checksum.

The store wrapper resets target, parameter and data length and copies the
destination into the 32-bit value field **as integer bits, not a float**.
It does not reset the index field. The containing object's constructor at
`0x1002de89c..0x1002de8ac` allocates and zeroes its `0x830`-byte request/reply
buffer, establishing an initial index of zero. Its value after arbitrary prior
commands has not been exhaustively traced. Zero-index packets produced by the
Rust packet constructor are synthetic requests rather than captured traffic.

## Verify the native implementation offline

From the repository root:

```sh
cargo test -p axe-session --locked usb::store
```

These injected-transport tests check the integer destination, context preflight,
durable intent, one transmission, timeout/disconnect handling and serialized device
ownership. They do not execute Axe-Edit, open MIDI endpoints or independently
revalidate the instruction addresses against an installed executable. The static
trace above remains separate from hardware acceptance.

## Remaining verification

1. Complete generic request response matching/status and the preserved-index audit.
   A checksum-valid reply or echoed request must not be mistaken for a verified
   store. The app's retry policy is not a safe retry contract for our backend.
2. Confirm destination numbering against the device and explicit UI selection.
3. On a user-selected disposable preset with a verified backup, store, reload
   and verify parameter persistence. Report hardware acceptance separately.
4. Restore original is an app-owned transaction over captured original raw
   parameter values using the setter; this store operation does not establish
   full-preset backup/restore or modifier-safe parameter writes.

The native application's user-operated Save action now sends this request once
after repeated context checks and durable journaling. It explicitly reports
transmission with persistence unverified, waits 10 seconds before releasing device
ownership, and does not automatically reload or retry. This enables the user's
manual hardware test; it does not close the complete Save acceptance gate.
