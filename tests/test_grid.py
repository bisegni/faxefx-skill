"""Authored grid packets and injected USB tests; no real device bytes or I/O."""
import json
import struct
from pathlib import Path

import pytest

from axefx_cli.backends import ReplayBackend, ReplayTransport, USBBackend, validate_usb_packet
from axefx_cli.cli import main, text_output
from axefx_cli.codec import editor_frame, grid_query
from axefx_cli.errors import AxeError
from axefx_cli.grid import attach_grid, decode_grid
from axefx_cli.model import inspect
from axefx_cli.storage import Store

CAPTURE = Path(__file__).parent / 'fixtures/synthetic-traffic.jsonl'


def packet(cells=(), *, slot=7, name='Synthetic Clean', cursor=58):
    data = bytearray(640)
    struct.pack_into('<I', data, 0, slot)
    data[16:48] = name.encode('ascii').ljust(32, b'\0')
    for row, column, identifier, inputs in cells:
        struct.pack_into('<HH', data, 304 + ((column - 1) * 6 + row - 1) * 4, identifier, inputs)
    return editor_frame(46, value_bits=cursor, data=data)


def preset(*ids):
    return {'slot': 7, 'name': 'Synthetic Clean', 'active_scene': 2, 'geometry': None, 'connections': None,
            'complete': False, 'blocks': [
                {'ref': f'block-{i}', 'instance_id': i, 'type': 'unknown', 'position': None,
                 'active_channel': 'A', 'bypass': False, 'channels': {'A': {'parameters': {}}}}
                for i in ids]}


def test_column_major_corners_and_full_shunt_ids():
    # Literal locations exercise boundary cells without using the production offset constants.
    data = bytearray(640)
    data[:4] = b'\x07\0\0\0'
    data[16:48] = b'Synthetic Clean'.ljust(32, b'\0')
    data[304:308] = b'\x25\0\0\0'       # Input at r1 c1.
    data[328:332] = b'\x3a\0\x01\0'    # Amp at r1 c2, fed by row 1.
    data[636:640] = b'\x3f\x04\x3f\0' # Last shunt ID at r6 c14, all six rows.
    grid = decode_grid(editor_frame(46, data=data))
    assert grid['geometry'] == {'rows': 6, 'columns': 14}
    assert grid['records'][0] == {'row': 1, 'column': 1, 'instance_id': 37, 'input_mask': 0}
    assert grid['records'][6] == {'row': 1, 'column': 2, 'instance_id': 58, 'input_mask': 1}
    assert grid['records'][-1] == {'row': 6, 'column': 14, 'instance_id': 1087, 'input_mask': 63}


def test_parallel_split_merge_unwired_block_and_system_status():
    grid = decode_grid(packet([
        (3, 1, 37, 0), (3, 2, 58, 4),
        (3, 3, 1024, 4), (4, 3, 62, 4),
        (3, 4, 1028, 12), (4, 4, 1026, 4),
        (3, 5, 42, 12), (6, 14, 70, 0),
    ]))
    p = preset(37, 58, 62, 42, 70, 200)
    attach_grid(p, grid)
    result = inspect(p)
    assert result['complete'] is False and result['complete_geometry'] is True
    assert result['cells'][2][:5] == ['block-37', 'block-58', 'shunt-r3c3', 'shunt-r3c4', 'block-42']
    assert result['cells'][3][2:4] == ['block-62', 'shunt-r4c4']
    assert result['connections'] == [
        {'from': 'block-37', 'to': 'block-58'},
        {'from': 'block-58', 'to': 'shunt-r3c3'},
        {'from': 'block-58', 'to': 'block-62'},
        {'from': 'shunt-r3c3', 'to': 'shunt-r3c4'},
        {'from': 'block-62', 'to': 'shunt-r3c4'},
        {'from': 'shunt-r3c3', 'to': 'shunt-r4c4'},
        {'from': 'shunt-r3c4', 'to': 'block-42'},
        {'from': 'shunt-r4c4', 'to': 'block-42'},
    ]
    assert next(b for b in result['blocks'] if b['instance_id'] == 200)['position'] is None
    unwired = next(b for b in result['blocks'] if b['instance_id'] == 70)
    assert unwired['incoming'] == unwired['outgoing'] == []


def test_empty_cell_cable_endpoints_are_preserved():
    p = preset(37, 42)
    attach_grid(p, decode_grid(packet([(1, 1, 37, 0), (1, 2, 0, 1), (2, 3, 42, 3)])))
    result = inspect(p)
    assert result['cells'][0][1] == 'wire-r1c2'
    assert result['cells'][1][1] == 'wire-r2c2'
    assert result['connections'] == [
        {'from': 'block-37', 'to': 'wire-r1c2'},
        {'from': 'wire-r1c2', 'to': 'block-42'},
        {'from': 'wire-r2c2', 'to': 'block-42'},
    ]
    assert all(b['type'] == 'wire' for b in result['blocks'] if b['ref'].startswith('wire-'))


@pytest.mark.parametrize('bad', [
    editor_frame(46, data=bytes(639)), editor_frame(46, data=bytes(641)),
    editor_frame(46, index=1, data=bytes(640)), editor_frame(47, data=bytes(640)),
    packet([(1, 2, 42, 64)]), packet([(1, 1, 42, 1)]), packet(slot=1024),
])
def test_unknown_or_malformed_grid_is_never_guessed(bad):
    with pytest.raises(AxeError):
        decode_grid(bad)


def test_bad_checksum_is_rejected():
    bad = bytearray(packet()); bad[-2] ^= 1
    with pytest.raises(AxeError) as error:
        decode_grid(bytes(bad))
    assert error.value.code == 'INVALID_REPLY'


@pytest.mark.parametrize('reply,ids,code', [
    (packet(slot=8), (), 'CONTEXT_CHANGED'),
    (packet(name='Other Preset'), (), 'CONTEXT_CHANGED'),
    (packet([(1, 1, 58, 0)]), (), 'CONTEXT_CHANGED'),
    (packet([(1, 1, 58, 0), (1, 2, 58, 1)]), (58,), 'INVALID_REPLY'),
    (packet([(1, 1, 1024, 0), (1, 2, 1024, 1)]), (), 'INVALID_REPLY'),
])
def test_mixed_context_and_duplicate_instances_fail(reply, ids, code):
    with pytest.raises(AxeError) as error:
        attach_grid(preset(*ids), decode_grid(reply))
    assert error.value.code == code


class GridDevice:
    def __init__(self, replies):
        self.capture = ReplayTransport(CAPTURE)
        self.replies = list(replies)
        self.requests = []

    def query(self, request):
        validate_usb_packet(request)
        self.requests.append(request)
        if request == grid_query():
            return self.replies.pop(0)
        return self.capture.query(request)

    def send(self, request):
        raise AssertionError('Inspection must not send any mutation')

    def close(self):
        pass


def test_usb_joins_grid_and_status_ignoring_cursor_changes(tmp_path):
    cells = [(3, 1, 37, 0), (3, 2, 58, 4), (3, 3, 62, 4), (3, 4, 42, 4)]
    device = GridDevice([packet(cells, cursor=58), packet(cells, cursor=62)])
    backend = USBBackend(Store(tmp_path), transport=device)
    result = inspect(backend.read_state(include_geometry=True)['current'])
    assert result['cells'][2][:4] == ['input1', 'amp1', 'cab1', 'output1']
    assert result['complete_geometry'] and not result['complete']
    assert result['blocks'][0]['bypass'] is False and result['blocks'][0]['active_channel'] == 'B'
    assert device.requests.count(grid_query()) == 2


def test_usb_detects_routing_change_with_unchanged_public_status(tmp_path):
    first = packet([(3, 1, 37, 0), (3, 2, 58, 4)])
    second = packet([(3, 1, 37, 0), (3, 2, 58, 0)])
    device = GridDevice([first, second])
    with pytest.raises(AxeError) as error:
        USBBackend(Store(tmp_path), transport=device).read_state(include_geometry=True)
    assert error.value.code == 'CONTEXT_CHANGED'


def test_only_exact_read_request_is_allowed():
    assert grid_query().hex() == 'f000017410012e00000000000000000000000000003af7'
    validate_usb_packet(grid_query())
    for request in (editor_frame(46, effect=58), editor_frame(46, value_bits=1),
                    editor_frame(46, data=b'x'), editor_frame(46, index=1), editor_frame(53)):
        with pytest.raises(AxeError) as error:
            validate_usb_packet(request)
        assert error.value.code == 'UNSUPPORTED_PACKET'


def grid_capture(path):
    reply = packet([(3, 1, 37, 0), (3, 2, 58, 4), (3, 3, 62, 4), (3, 4, 42, 4)])
    path.write_text(CAPTURE.read_text() + json.dumps({'direction': 'tx', 'hex': grid_query().hex()}) + '\n' +
                    json.dumps({'direction': 'rx', 'hex': reply.hex()}) + '\n')


def test_replay_only_advertises_grid_when_it_has_a_grid_reply(tmp_path):
    without = ReplayBackend(Store(tmp_path / 'without'), CAPTURE)
    assert 'geometry' not in without.capabilities()['read']
    assert without.read_state()['current']['geometry'] is None
    capture = tmp_path / 'grid.jsonl'
    grid_capture(capture)
    with_grid = ReplayBackend(Store(tmp_path / 'with'), capture)
    assert 'geometry' in with_grid.capabilities()['read']
    assert with_grid.read_state(include_geometry=True)['current']['complete_geometry'] is True
    assert with_grid.capabilities()['edit'] == []


def test_geometry_json_text_and_block_list_through_cli(tmp_path, capsys):
    capture = tmp_path / 'grid.jsonl'
    grid_capture(capture)
    prefix = ['--backend', 'replay', '--state-dir', str(tmp_path / 'state'), '--replay-file', str(capture)]
    assert main(prefix + ['preset', 'geometry']) == 0
    result = json.loads(capsys.readouterr().out)['data']
    assert result['name'] == 'Synthetic Clean' and result['complete_geometry']
    assert result['cells'][2][:4] == ['input1', 'amp1', 'cab1', 'output1']
    assert main(prefix + ['--format', 'text', 'preset', 'geometry']) == 0
    text = capsys.readouterr().out
    assert 'Preset 7: Synthetic Clean | Scene 2' in text
    assert 'input1 -> amp1' in text
    assert main(prefix + ['block', 'list']) == 0
    assert json.loads(capsys.readouterr().out)['data']['complete_geometry'] is True
    assert main(prefix + ['preset', 'inspect']) == 0
    result = json.loads(capsys.readouterr().out)['data']
    assert result['complete_geometry'] is True and result['complete'] is False


def test_ascii_marks_bypass_without_removing_cables():
    p = preset(37, 58)
    p['blocks'][1]['bypass'] = True
    attach_grid(p, decode_grid(packet([(3, 1, 37, 0), (3, 2, 58, 4), (3, 3, 1028, 4)])))
    output = text_output({'ok': True, 'backend': 'usb', 'warnings': [], 'data': inspect(p)})
    assert 'block-58*' in output and '* bypassed' in output and '===' in output
    assert 'block-37 -> block-58' in output and 'block-58 -> shunt-r3c3' in output
