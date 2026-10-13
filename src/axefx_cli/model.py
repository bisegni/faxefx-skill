"""Preset-domain model, independent of MIDI transports."""
import copy

from .catalog import effect_type, instance_ref, mock_value, parameter
from .errors import require

CHANNELS = 'ABCD'


def empty_preset(name='Untitled', slot=None):
    return {'slot': slot, 'name': name, 'geometry': {'rows': 6, 'columns': 14},
            'blocks': [], 'connections': [], 'scenes': [
                {'number': i, 'name': '', 'channels': {}, 'bypass': {}, 'controllers': {}}
                for i in range(1, 9)], 'active_scene': 1, 'modifiers': [], 'controllers': {},
            'unknown_fields': {}, 'unsaved': True, 'complete': True}


def block(preset, ref):
    matches = [b for b in preset['blocks'] if b['ref'] == ref or str(b['instance_id']) == str(ref)]
    require(len(matches) == 1, 'UNKNOWN_BLOCK', 'Use a block reference from preset inspect', block=ref)
    return matches[0]


def check_position(preset, row, column):
    require(type(row) is int and type(column) is int and
            1 <= row <= preset['geometry']['rows'] and 1 <= column <= preset['geometry']['columns'],
            'INVALID_POSITION', 'Row/column is outside the preset grid', row=row, column=column)


def at(preset, row, column):
    return next((b for b in preset['blocks'] if b['position'] == {'row': row, 'column': column}), None)


def add_block(preset, type_name, row, column, ref=None):
    check_position(preset, row, column)
    require(at(preset, row, column) is None, 'CELL_OCCUPIED', 'Moving/adding never swaps or replaces another block')
    t = effect_type(type_name)
    require(t['alias'] not in ('null', 'global', 'controllers', 'modifier', 'tuner', 'ir-capture', 'performance', 'preset-fc'),
            'INVALID_BLOCK_TYPE', 'This object is not a supported signal-grid block')
    require(t['alias']=='shunt' or bool(t['parameters']), 'TYPE_SCHEMA_UNAVAILABLE',
            'No authored mock parameter definitions for this type; recognized IDs do not establish edit support')
    used = {b['instance_id'] for b in preset['blocks']}
    if t['alias'] == 'shunt':
        instance = {'id': t['instances'][0]['id'], 'ref': f'shunt-r{row}c{column}'}
        # Shunts have independent cell identities, not unique DSP effect instances.
        reference = instance['ref']
    else:
        candidates = [i for i in t['instances'] if i['id'] not in used and
                      (ref is None or instance_ref(t, i) == ref)]
        require(bool(candidates), 'INSTANCE_UNAVAILABLE', 'Requested effect instance is unavailable', type=t['alias'], ref=ref)
        instance = candidates[0]
        reference = instance_ref(t, instance)
    require(not any(b['ref'] == reference for b in preset['blocks']), 'INSTANCE_UNAVAILABLE', 'Block reference already exists')
    b = {'ref': reference, 'instance_id': instance['id'], 'type_id': t['id'], 'type': t['alias'],
         'position': {'row': row, 'column': column}, 'active_channel': 'A', 'bypass': False,
         'channels': {ch: {'parameters': {}} for ch in CHANNELS}, 'unknown_fields': {}}
    preset['blocks'].append(b)
    for scene in preset['scenes']:
        scene['channels'][reference] = 'A'
        scene['bypass'][reference] = False
    return b


def ensure_channel(b, channel):
    require(channel in b['channels'], 'INVALID_CHANNEL', 'Channel unavailable for this block', channel=channel)


def set_parameter(preset, ref, name, value, channel=None, raw=False):
    b = block(preset, ref)
    channel = channel or b['active_channel']
    ensure_channel(b, channel)
    t = effect_type(b['type_id'])
    p = parameter(t, name)
    b['channels'][channel]['parameters'][p['name']] = mock_value(t, p['name'], value, raw)
    return p['name']


def connect(preset, source, target):
    a, z = block(preset, source), block(preset, target)
    start, end = a['position']['column'], z['position']['column']
    require(start < end, 'INVALID_CONNECTION', 'Signal-grid cables run left to right')
    original = source
    added = []
    # Return existing route when it is unique; do not create a second route.
    paths = routes(preset, source, target)
    if len(paths) == 1:
        return {'route': paths[0], 'added': [], 'existing': True}
    require(not paths, 'AMBIGUOUS_CONNECTION', 'More than one existing route connects these blocks', paths=paths)
    for column in range(start+1, end):
        rows = sorted(range(1, preset['geometry']['rows']+1), key=lambda r: (abs(r-a['position']['row']), r))
        row = next((r for r in rows if at(preset, r, column) is None), None)
        require(row is not None, 'NO_SAFE_ROUTE', 'No empty intermediate cell; existing blocks/cables are preserved', column=column)
        b = add_block(preset, 'shunt', row, column)
        preset['connections'].append({'from': source, 'to': b['ref']})
        added.append(b['ref'])
        source = b['ref']
    edge = {'from': source, 'to': target}
    if edge not in preset['connections']:
        preset['connections'].append(edge)
    return {'route': [original, *added, target], 'added': added, 'existing': False}


def routes(preset, source, target):
    found = []
    def visit(node, path):
        if len(found) > 1:
            return
        for edge in preset['connections']:
            if edge['from'] != node or edge['to'] in path:
                continue
            next_ = edge['to']
            if next_ == target:
                found.append(path + [next_])
            elif block(preset, next_)['type'] == 'shunt':
                visit(next_, path+[next_])
    visit(source, [source])
    return found


def inspect(preset):
    p = copy.deepcopy(preset)
    for b in p['blocks']:
        b['incoming'] = None if p['connections'] is None else [e['from'] for e in p['connections'] if e['to'] == b['ref']]
        b['outgoing'] = None if p['connections'] is None else [e['to'] for e in p['connections'] if e['from'] == b['ref']]
    if p.get('geometry'):
        p['cells'] = [[next((b['ref'] for b in p['blocks'] if b['position'] == {'row': row, 'column': col}), None)
                       for col in range(1, p['geometry']['columns']+1)] for row in range(1,p['geometry']['rows']+1)]
    return p


def fixture_state():
    p = empty_preset('Mock Clean', 0)
    for t, r, c in [('input-1', 1, 1), ('amp', 1, 2), ('cab', 1, 3), ('output-1', 1, 4)]:
        add_block(p, t, r, c)
    for source, target in [('input1','amp1'),('amp1','cab1'),('cab1','output1')]:
        connect(p,source,target)
    for param in ('gain','bass','mid','treble','presence'):
        set_parameter(p,'amp1',param,5)
    set_parameter(p,'amp1','master',10)
    p['unsaved'] = False
    return {'device': {'id': 'mock-axefx3', 'model': 'Axe-Fx III (simulated)', 'firmware': None, 'simulated': True},
            'current': p, 'presets': {'0': copy.deepcopy(p)}, 'generation': 0}
