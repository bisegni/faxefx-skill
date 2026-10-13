#!/usr/bin/env python3
"""Offline Axe-Fx III framing codec. This module never opens or sends to MIDI ports."""
import argparse
import json
import struct
from functools import reduce
from operator import xor

HEADER = bytes.fromhex('f0 00 01 74 10')

def seven(value, count):
    if not 0 <= value < 1 << (7 * count):
        raise ValueError('integer does not fit field')
    return bytes((value >> (7 * i)) & 127 for i in range(count))

def integer(data):
    return sum(v << (7 * i) for i, v in enumerate(data))

def frame(command, payload=b''):
    if not 0 <= command < 128 or any(v > 127 for v in payload):
        raise ValueError('SysEx data must be 7-bit')
    body = HEADER + bytes([command]) + payload
    return body + bytes([reduce(xor, body, 0) & 127, 247])

def unframe(message):
    if len(message) < 8 or message[:5] != HEADER or message[-1] != 247:
        raise ValueError('invalid Axe-Fx III frame')
    if any(v > 127 for v in message[1:-1]):
        raise ValueError('non-7-bit data')
    if reduce(xor, message[:-2], 0) & 127 != message[-2]:
        raise ValueError('invalid checksum')
    return message[5], message[6:-2]

def pack_bytes(data):
    """MSB-first bit stream, padded in the last 7-bit byte (observed builder)."""
    out = bytearray()
    acc = bits = 0
    for value in data:
        acc = (acc << 8) | value
        bits += 8
        while bits >= 7:
            bits -= 7
            out.append((acc >> bits) & 127)
    if bits:
        out.append((acc << (7 - bits)) & 127)
    return bytes(out)

def unpack_bytes(data, length):
    if len(data) != (length * 8 + 6) // 7 or any(v > 127 for v in data):
        raise ValueError('invalid packed byte stream')
    acc = bits = 0
    out = bytearray()
    for value in data:
        acc = (acc << 7) | value
        bits += 7
        if bits >= 8:
            bits -= 8
            out.append((acc >> bits) & 255)
    if len(out) != length or (bits and acc & ((1 << bits) - 1)):
        raise ValueError('invalid packed padding')
    return bytes(out)

def editor_frame(operation, effect=0, parameter=0, value_bits=0, index=0, data=b''):
    """Field names describe the common parameter path; other operations reuse them."""
    if not 0 <= value_bits <= 0xffffffff:
        raise ValueError('value_bits must fit uint32')
    payload = (seven(operation, 2) + seven(effect, 2) + seven(parameter, 2)
               + seven(value_bits, 5) + seven(index, 2) + seven(len(data), 2)
               + pack_bytes(data))
    return frame(1, payload)

def decode_editor(message):
    command, p = unframe(message)
    if command != 1 or len(p) < 15:
        raise ValueError('not an editor envelope')
    value_bits = integer(p[6:11])
    if value_bits > 0xffffffff:
        raise ValueError('value field exceeds uint32')
    length = integer(p[13:15])
    return dict(operation=integer(p[:2]), effect=integer(p[2:4]),
                parameter=integer(p[4:6]), value_bits=value_bits,
                value_float=struct.unpack('<f', struct.pack('<I', value_bits))[0],
                index=integer(p[11:13]), data_hex=unpack_bytes(p[15:], length).hex())

def public_queries():
    return {
        'status': frame(0x13), 'scene': frame(0x0c, b'\x7f'),
        'preset_name': frame(0x0d, b'\x7f\x7f'),
        'scene_name': frame(0x0e, b'\x7f'),
        'tempo': frame(0x14, b'\x7f\x7f'),
        'amp1_bypass': frame(0x0a, seven(58, 2) + b'\x7f'),
        'amp1_channel': frame(0x0b, seven(58, 2) + b'\x7f'),
    }

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--decode', help='decode a hexadecimal editor packet')
    args = parser.parse_args()
    if args.decode:
        print(json.dumps(decode_editor(bytes.fromhex(args.decode)), indent=2))
    else:
        print(json.dumps({k: v.hex(' ').upper() for k, v in public_queries().items()}, indent=2))
