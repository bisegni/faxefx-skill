import pytest

from axefx_cli.codec import HEADER, decode_public, frame, seven


def test_checksumless_din_push_is_distinct_from_transmitted_tap_and_tuner():
    assert decode_public(HEADER + b'\x10\xf7')['kind'] == 'tempo_push'
    assert decode_public(HEADER + bytes([0x11, 9, 0, 62, 0xf7]))['cents'] == -1
    assert decode_public(frame(0x10), 'tx')['kind'] == 'tap_tempo'
    assert decode_public(frame(0x11, b'\x01'), 'tx')['value'] == 1
    with pytest.raises(ValueError):
        decode_public(HEADER + b'\x10\xf7', 'tx')


@pytest.mark.parametrize('opcode,payload,kind', [
    (0x0a, seven(58, 2) + b'\x01', 'bypass'),
    (0x0b, seven(62, 2) + b'\x02', 'channel'),
    (0x0c, b'\x03', 'scene'), (0x0f, b'\x21', 'looper'),
    (0x14, seven(120, 2), 'tempo'),
])
def test_public_state_packets_have_independent_typed_decode(opcode, payload, kind):
    result = decode_public(frame(opcode, payload))
    assert result['kind'] == kind and result['checksum']
    with pytest.raises(ValueError):
        decode_public(frame(opcode, payload + b'\0'))


def test_names_status_and_bad_checksums():
    result = decode_public(frame(0x0d, seven(442, 2) + b'Example\0private'.ljust(32, b'\0')))
    assert result['index'] == 442 and result['name'] == 'Example'
    status = decode_public(frame(0x13, seven(58, 2) + b'\x43'))
    assert status['effects'] == [{'effect': 58, 'bypass': True, 'channel_index': 1, 'channel_count': 4}]
    with pytest.raises(ValueError, match='checksum'):
        decode_public(bytes.fromhex('f0 00 01 74 10 13 00 f7'))
