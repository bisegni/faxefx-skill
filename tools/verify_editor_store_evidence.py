#!/usr/bin/env python3
"""Verify the recovered Axe-Edit store path offline. Never executes Axe-Edit or MIDI."""
import argparse
import json
from pathlib import Path
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'src'))
from axefx_cli.editor import ANALYZED_SHA256, app_identity, sections
from axefx_cli.codec import editor_frame


def verify(app, slot):
    source, identity = app_identity(app)
    if identity['sha256'] != ANALYZED_SHA256:
        raise ValueError('Different editor build; these address-bound findings do not apply')
    data, table = sections(source)

    def check(condition, detail):
        if not condition:
            raise ValueError('Static evidence mismatch: ' + detail)

    def read(address, length):
        section = next(s for s in table if s['address'] <= address
                       and address + length <= s['address'] + s['size'])
        start = section['offset'] + address - section['address']
        return data[start:start + length]

    def word(address):
        return struct.unpack('<I', read(address, 4))[0]

    def mov(address, register, value):
        instruction = word(address)
        check(instruction & 0xffe00000 == 0x52800000, hex(address))
        check(instruction & 31 == register, hex(address))
        check((instruction >> 5) & 65535 == value, hex(address))

    def branch(address, target, link):
        instruction = word(address)
        check(instruction >> 26 == (0b100101 if link else 0b000101), hex(address))
        displacement = instruction & 0x3ffffff
        if displacement & 0x2000000:
            displacement -= 0x4000000
        check(address + displacement * 4 == target, hex(address))

    def pair_store(address, first, second, base, offset):
        instruction = word(address)
        check(instruction & 0xffc00000 == 0x29000000, hex(address))
        check((instruction & 31, (instruction >> 10) & 31,
                (instruction >> 5) & 31, ((instruction >> 15) & 127) * 4
                ) == (first, second, base, offset), hex(address))

    check(read(0x10057adf0, 12) == b'Save Preset\0', 'Save Preset label')
    for address, step in [(0x1000cf250, 0), (0x1000cf260, 0x11), (0x1000cf270, 1)]:
        mov(address, 1, step)
        branch(address + 8, 0x10008bc74, True)
    entry = struct.unpack('<H', read(0x10057e926 + 0x11 * 2, 2))[0]
    check(0x1000d2c74 + entry * 4 == 0x1000d3f44, 'step 0x11 jump table')
    branch(0x1000d3f58, 0x10026b274, True)
    mov(0x1000d3f64, 2, 1)
    mov(0x1000d3f68, 3, 10000)
    branch(0x1000d3f6c, 0x1000d53a8, True)
    mov(0x10026b27c, 9, 0x26)
    pair_store(0x10026b28c, 8, 31, 9, 0)  # operation, zero target
    pair_store(0x10026b290, 31, 2, 9, 8)  # zero parameter, destination argument
    instruction = word(0x10026b298)
    check(instruction & 0xffc00000 == 0xb9000000, 'data length store')
    check((instruction & 31, (instruction >> 5) & 31,
            ((instruction >> 10) & 4095) * 4) == (31, 2, 20), 'zero data length')
    branch(0x10026b29c, 0x10026aba4, False)
    mov(0x1002de89c, 0, 0x830)
    mov(0x1002de8a8, 1, 0x830)
    branch(0x1002de8ac, 0x100555554, True)  # bzero of the request/reply buffer
    return {
        'editor': identity,
        'evidence': 'static ARM64 instruction and jump-table verification; no target execution',
        'save_step': '0x11', 'dispatcher_case': '0x1000d3f44',
        'wrapper': '0x10026b274', 'builder': '0x10026aba4',
        'outer_command': '01', 'inner_operation': '0026',
        'fields': {'target': 0, 'parameter': 0, 'value_bits': 'destination argument', 'data_length': 0},
        'request_timeout_ms': 10000,
        'hardware_verified': False,
        'initial_index': 0,
        'unresolved': ['preserved index after arbitrary prior commands', 'wire acknowledgement/status contract',
                       'destination numbering and save/reload persistence on hardware'],
        'synthetic_candidate': {
            'slot': slot, 'index_assumption': 0,
            'hex': editor_frame(0x26, value_bits=slot).hex(' '),
            'warning': 'Constructed offline with index=0; not an observed packet or authorization to transmit',
        },
    }


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', required=True, help='Installed Axe-Edit III.app to read')
    parser.add_argument('--slot', type=int, default=0, help='Synthetic example destination, 0-1023')
    args = parser.parse_args()
    if not 0 <= args.slot <= 1023:
        parser.error('--slot must be between 0 and 1023')
    print(json.dumps(verify(args.app, args.slot), indent=2))
