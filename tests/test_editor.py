import struct

import pytest

from axefx_cli.editor import decode_cache, sections
from axefx_cli.errors import AxeError
from axefx_cli.protocol import coverage


def cache_bytes(revision=4):
    header = struct.pack('<IIH', 32, 6, revision)
    if revision == 4:
        header += b'authored-test'.ljust(32, b'\0')
    label = b'Authored option'
    record = struct.pack('<HHHffffI', 1, 0, 99, 0., 1., 10., .001, 1)
    record += struct.pack('<I', len(label)) + label + struct.pack('<HII', 7, 1, 42)
    return header + struct.pack('<III', 1, 10, 1) + record


@pytest.mark.parametrize('revision', [3, 4])
def test_cache_import_retains_provisional_semantics_and_source_hash(tmp_path, revision):
    path = tmp_path / 'authored.cache'; path.write_bytes(cache_bytes(revision))
    parsed = decode_cache(path)
    assert parsed['parsed_bytes'] == len(path.read_bytes())
    assert parsed['firmware'] == {'major': 32, 'minor': 6}
    p = parsed['effects'][0]['parameters'][0]
    assert p['unknown16'] == 99 and p['extra_values'] == [42]
    assert p['numeric_fields'][:3] == [0, 1, 10]
    assert not p['editable_semantics_verified'] and len(parsed['sha256']) == 64


@pytest.mark.parametrize('data', [b'', cache_bytes()[:-1], cache_bytes() + b'unknown', b'\0' * 8])
def test_truncated_or_unknown_cache_is_rejected(tmp_path, data):
    path = tmp_path / 'bad.cache'; path.write_bytes(data)
    with pytest.raises(AxeError):
        decode_cache(path)


def test_protocol_does_not_confuse_framing_with_hardware_acceptance():
    result = coverage(); rows = {r['id']: r for r in result['entries']}
    assert not result['complete'] and not result['private_protocol_complete']
    assert rows['editor-0009']['hardware_validation'] == 'unverified'
    assert not rows['preset-transfer']['write_implemented']
    assert 'checksumless' in rows['push-data']['frame']
