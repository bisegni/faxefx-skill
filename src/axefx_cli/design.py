"""Revisioned logical designs and deterministic, capability-scoped compilation."""
import copy
import re
import uuid

from .catalog import effect_type, instance_ref
from .errors import AxeError, require
from .model import block, empty_preset, inspect
from .operations import apply_operation, name_check, validate_preset
from .storage import atomic_write, digest, load

DESIGN_VERSION = '1.0'
MAX_OPERATIONS = 16384


def starter(name='Untitled'):
    return {'design_version': DESIGN_VERSION, 'name': name, 'intent': '', 'mode': 'new',
            'nodes': [{'id': i, 'type': t, 'label': label, 'parameters': {}, 'channels': {},
                       'bypass': False, 'rationale': ''} for i, t, label in
                      [('input', 'input-1', 'Input 1'), ('amp', 'amp', 'Amplifier'),
                       ('cab', 'cab', 'Cabinet'), ('output', 'output-1', 'Output 1')]],
            'edges': [{'from': a, 'to': b} for a, b in [('input', 'amp'), ('amp', 'cab'), ('cab', 'output')]],
            'scenes': [], 'active_scene': 1, 'modifiers': [], 'controllers': {}, 'references': [],
            'target': None, 'captured': None, 'unknowns': []}


def validate_design(design):
    require(isinstance(design, dict) and design.get('design_version') == DESIGN_VERSION,
            'INVALID_DESIGN', 'Expected design_version 1.0')
    require(isinstance(design.get('name'), str) and 0 < len(design['name']) <= 120,
            'INVALID_DESIGN', 'Name must contain 1–120 characters')
    require(design.get('mode', 'new') in ('new', 'refine'), 'INVALID_DESIGN', 'mode must be new or refine')
    require(isinstance(design.get('intent', ''), str) and len(design.get('intent', '')) <= 12000,
            'INVALID_DESIGN', 'Tone brief must be text up to 12000 characters')
    nodes, edges = design.get('nodes'), design.get('edges')
    require(isinstance(nodes, list) and len(nodes) <= 84 and isinstance(edges, list) and len(edges) <= 1024,
            'INVALID_DESIGN', 'Provide up to 84 nodes and 1024 edges')
    ids = []
    for node in nodes:
        require(isinstance(node, dict) and isinstance(node.get('id'), str) and
                re.fullmatch(r'[A-Za-z][A-Za-z0-9_-]{0,63}', node['id']), 'INVALID_DESIGN', 'Invalid stable node ID')
        require(isinstance(node.get('type'), str) and bool(node['type']), 'INVALID_DESIGN', 'Node requires an effect type')
        require(len(node['type']) <= 64, 'INVALID_DESIGN', 'Effect type name is too long')
        for key, maximum in [('label', 120), ('rationale', 2000), ('model', 200), ('model_parameter', 100)]:
            require(node.get(key) is None or isinstance(node[key], str) and len(node[key]) <= maximum,
                    'INVALID_DESIGN', 'Invalid node text field', field=key)
        require(node.get('bypass') is None or type(node['bypass']) is bool,
                'INVALID_DESIGN', 'bypass must be boolean or unknown (null)')
        require(node.get('active_channel') in (None, 'A', 'B', 'C', 'D'),
                'INVALID_CHANNEL', 'Active channel must be A–D or unknown (null)')
        require(isinstance(node.get('parameters', {}), dict) and isinstance(node.get('channels', {}), dict),
                'INVALID_DESIGN', 'parameters and channels must be objects')
        require(set(node.get('channels', {})) <= set('ABCD'), 'INVALID_CHANNEL', 'Channels use A–D')
        for settings in node.get('channels', {}).values():
            require(isinstance(settings, dict), 'INVALID_DESIGN', 'Channel settings must be objects')
        if 'position' in node and node['position'] is not None:
            p = node['position']
            require(isinstance(p, dict) and type(p.get('row')) is int and type(p.get('column')) is int and
                    1 <= p['row'] <= 6 and 1 <= p['column'] <= 14, 'INVALID_POSITION', 'Position outside 6 × 14 grid')
        ids.append(node['id'])
    require(len(ids) == len(set(ids)), 'INVALID_DESIGN', 'Duplicate node IDs')
    pairs = set()
    for edge in edges:
        require(isinstance(edge, dict) and edge.get('from') in ids and edge.get('to') in ids and
                edge['from'] != edge['to'], 'INVALID_DESIGN', 'Cable must connect two different existing nodes')
        pair = (edge['from'], edge['to'])
        require(pair not in pairs, 'INVALID_DESIGN', 'Duplicate cable')
        pairs.add(pair)
    require(type(design.get('active_scene', 1)) is int and 1 <= design.get('active_scene', 1) <= 8,
            'INVALID_SCENE', 'Scenes use 1–8')
    require(isinstance(design.get('scenes', []), list) and len(design.get('scenes', [])) <= 8 and
            isinstance(design.get('modifiers', []), list) and len(design.get('modifiers', [])) <= 32 and
            isinstance(design.get('controllers', {}), dict) and isinstance(design.get('references', []), list),
            'INVALID_DESIGN', 'Invalid scene/modifier/controller/reference collections')
    numbers = set()
    for scene in design.get('scenes', []):
        require(isinstance(scene, dict) and type(scene.get('number')) is int and 1 <= scene['number'] <= 8,
                'INVALID_SCENE', 'Each scene needs a number from 1–8')
        require(scene['number'] not in numbers, 'INVALID_SCENE', 'Duplicate scene number'); numbers.add(scene['number'])
        require(isinstance(scene.get('name', ''), str), 'INVALID_SCENE', 'Scene name must be text')
        for field in ('channels', 'bypass', 'controllers'):
            require(isinstance(scene.get(field, {}), dict), 'INVALID_SCENE', 'Scene maps must be objects')
        require(set(scene.get('channels', {})) <= set(ids) and set(scene.get('bypass', {})) <= set(ids),
                'INVALID_SCENE', 'Scene refers to an absent logical block')
        require(all(v in ('A', 'B', 'C', 'D') for v in scene.get('channels', {}).values()), 'INVALID_CHANNEL', 'Scene channels use A–D')
        require(all(type(v) is bool for v in scene.get('bypass', {}).values()), 'INVALID_SCENE', 'Scene bypass values must be boolean')
    for modifier in design.get('modifiers', []):
        require(isinstance(modifier, dict) and {'ref', 'block', 'parameter', 'source'} <= modifier.keys() and
                modifier['block'] in ids and isinstance(modifier.get('settings', {}), dict),
                'INVALID_MODIFIER', 'Modifier needs ref, existing block, parameter, source and optional settings object')
    # Also reject nested NaN/Infinity and non-JSON values before writing artifacts.
    digest(design)
    return copy.deepcopy(design)


class Designs:
    def __init__(self, store):
        self.store = store

    def path(self, project):
        require(isinstance(project, str) and re.fullmatch(r'[0-9a-f]{32}', project), 'INVALID_PROJECT', 'Use a returned project ID')
        return self.store.root / 'projects' / (project + '.json')

    def create(self, design):
        design = validate_design(design)
        project = uuid.uuid4().hex
        with self.store.lock('design-revisions'):
            artifact = self.store.put('designs', {'kind': 'design_revision', 'project': project, 'parent': None, 'design': design})
            atomic_write(self.path(project), {'project': project, 'revision': artifact['id']})
        return {'project': project, 'revision': artifact['id'], 'design': design}

    def get(self, project, revision=None):
        head = load(self.path(project))
        key = revision or head['revision']
        artifact = self.store.get('designs', key)
        require(artifact['project'] == project, 'INVALID_REVISION', 'Revision belongs to a different project')
        return {'project': project, 'revision': key, 'head_revision': head['revision'], 'design': artifact['design']}

    def revise(self, project, base, design):
        design = validate_design(design)
        with self.store.lock('design-revisions'):
            head = self.get(project)
            if head['design'].get('captured'):
                for field in ('captured', 'target', 'captured_at', 'mode'):
                    require(design.get(field) == head['design'].get(field), 'CAPTURE_CHANGED',
                            'Captured evidence cannot be changed by a draft revision', field=field)
            require(head['revision'] == base, 'REVISION_CONFLICT', 'Draft is stale; inspect current revision before merging', current_revision=head['revision'])
            artifact = self.store.put('designs', {'kind': 'design_revision', 'project': project, 'parent': base, 'design': design})
            atomic_write(self.path(project), {'project': project, 'revision': artifact['id']})
        return {'project': project, 'revision': artifact['id'], 'design': design}


def from_capture(state):
    p = state['current']
    require(p.get('complete_geometry', p.get('complete', False)), 'GEOMETRY_UNAVAILABLE', 'Capture requires verified physical geometry')
    d = starter(p['name'])
    d.update(mode='refine', nodes=[], edges=copy.deepcopy(p['connections']), captured=copy.deepcopy(state),
             target=copy.deepcopy(state['device']), active_scene=p['active_scene'],
             scenes=[], modifiers=copy.deepcopy(p.get('modifiers') or []), controllers=copy.deepcopy(p.get('controllers') or {}),
             unknowns=[] if p['complete'] else ['Full parameters, inactive channels, scene contents, modifiers and controllers may be unavailable.'])
    for b in p['blocks']:
        if b.get('position') is None:
            continue
        channels = {ch: {name: v['value'] for name, v in content['parameters'].items() if v.get('value') is not None}
                    for ch, content in (b.get('channels') or {}).items()}
        d['nodes'].append({'id': b['ref'], 'type': b['type'], 'label': b['ref'], 'hardware_ref': b['ref'],
                           'position': copy.deepcopy(b['position']), 'channels': channels,
                           'parameters': {}, 'bypass': b['bypass'], 'active_channel': b['active_channel']})
    if p.get('scenes'):
        d['scenes'] = copy.deepcopy(p['scenes'])
    return validate_design(d)


def layout(design):
    """Stable topological levels; explicit positions are constraints, not hints."""
    nodes = design['nodes']
    ids = [n['id'] for n in nodes]
    incoming = {i: [] for i in ids}
    outgoing = {i: [] for i in ids}
    for edge in design['edges']:
        incoming[edge['to']].append(edge['from'])
        outgoing[edge['from']].append(edge['to'])
    levels, todo = {}, list(ids)
    while todo:
        ready = [i for i in todo if all(p in levels for p in incoming[i])]
        require(bool(ready), 'CYCLIC_DESIGN', 'Use explicit feedback blocks; physical cables run left to right')
        for i in ready:
            levels[i] = max((levels[p] + 1 for p in incoming[i]), default=1)
            todo.remove(i)
    positions, occupied = {}, set()
    dry_columns = {col for edge in design['edges']
                   for col in range(levels[edge['from']] + 1, levels[edge['to']])}
    # Place pinned nodes first, so auto-placement cannot steal a pinned cell.
    for node in nodes:
        if node.get('position'):
            p = node['position']; cell = (p['row'], p['column'])
            require(cell not in occupied, 'CELL_OCCUPIED', 'Pinned blocks share a cell')
            occupied.add(cell); positions[node['id']] = copy.deepcopy(p)
    for i in sorted(ids, key=lambda n: (levels[n], ids.index(n))):
        if i in positions:
            continue
        column = max([levels[i], *[positions[p]['column'] + 1 for p in incoming[i] if p in positions]])
        require(column <= 14, 'GRID_LIMIT', 'Design exceeds fourteen columns')
        preferred = (2, 4, 1, 5, 6, 3) if column in dry_columns else (3, 2, 4, 1, 5, 6)
        row = next((r for r in preferred if (r, column) not in occupied), None)
        require(row is not None, 'GRID_LIMIT', 'More than six blocks need the same column')
        positions[i] = {'row': row, 'column': column}; occupied.add((row, column))
    for edge in design['edges']:
        require(positions[edge['from']]['column'] < positions[edge['to']]['column'],
                'INVALID_CONNECTION', 'Pinned routing must run left to right', edge=edge)
    return positions


def compile_design(design, backend_name='usb', capabilities=None):
    d = validate_design(design)
    result = {'schema_version': '1.0', 'kind': 'compiled_design', 'backend': backend_name,
              'design_digest': digest(d), 'logical': d, 'physical': None, 'mapping': {},
              'operations': [], 'blockers': [], 'warnings': [], 'alternatives': [],
              'buildable': False, 'hardware_verified': False, 'dsp_cost': None}
    try:
        positions = layout(d)
        result['logical_positions'] = positions
        require(bool(d['nodes']), 'EMPTY_DESIGN', 'Add an input, effects and output before compiling')
        require(any(n['type'].startswith('input') for n in d['nodes']) and
                any(n['type'].startswith('output') for n in d['nodes']), 'MISSING_IO', 'Design needs an input and output')
        # Every node must participate in an input-to-output path. Never silently drop an unwired effect.
        forward = {n['id'] for n in d['nodes'] if n['type'].startswith('input')}
        backward = {n['id'] for n in d['nodes'] if n['type'].startswith('output')}
        for _ in d['nodes']:
            forward.update(e['to'] for e in d['edges'] if e['from'] in forward)
            backward.update(e['from'] for e in d['edges'] if e['to'] in backward)
        require(set(n['id'] for n in d['nodes']) <= forward & backward, 'DISCONNECTED_DESIGN', 'Every block must be on a complete input-to-output path')
        name_check(d['name'])
        if backend_name != 'mock':
            # No hardware serialization through authored mock definitions, even after a static catalog import.
            gaps = ['firmware-bound catalog semantics', 'full edit-buffer backup/restore', 'validated structural and model setters']
            if backend_name == 'replay':
                gaps.insert(0, 'replay cannot apply edits')
            raise AxeError('PROTOCOL_UNRESOLVED', 'Automatic hardware construction has not passed its protocol gates', missing=gaps)
        require(all(n.get('bypass', False) is not None and n.get('active_channel', 'A') is not None for n in d['nodes']),
                'UNKNOWN_SETTINGS', 'Resolve unknown bypass and channel values before mock compilation')
        ops, mapping, used = [], {}, set()
        original_nodes = {}
        if d.get('mode', 'new') == 'refine':
            require(d.get('captured') and d['captured']['device'].get('simulated'), 'INVALID_CAPTURE', 'Mock refinement requires a mock capture')
            state = copy.deepcopy(d['captured'])
            require(state['current']['complete'], 'INCOMPLETE_CAPTURE', 'Cannot rebuild unknown captured fields')
            original_nodes = {b['ref']: b for b in state['current']['blocks'] if b.get('position')}
            retained = {n.get('hardware_ref') for n in d['nodes']}
            # Explicitly clear cables before moving/reconnecting; preserve all other captured data.
            for edge in list(state['current']['connections']):
                ops.append({'op': 'block.disconnect', **edge})
            for ref in original_nodes:
                if ref not in retained:
                    ops.append({'op': 'block.remove', 'block': ref})
            for op in ops:
                apply_operation(state, op)
            ops.append({'op': 'preset.rename', 'name': d['name']})
            apply_operation(state, ops[-1])
        else:
            state = {'device': {'simulated': True}, 'current': empty_preset(), 'presets': {}, 'generation': 0}
            ops.append({'op': 'preset.new', 'name': d['name']})
            apply_operation(state, ops[-1])

        def emit(op):
            apply_operation(state, op)
            ops.append(op)

        # Removing or retargeting an assignment is explicit in the design.
        # Unchanged modifiers stay active and continue to protect their targets.
        desired_modifiers = {m['ref']: m for m in d.get('modifiers', [])}
        logical_to_existing = {n['id']: n.get('hardware_ref') for n in d['nodes']}
        for modifier in list(state['current']['modifiers']):
            desired = desired_modifiers.get(modifier['ref'])
            if desired is None or (logical_to_existing.get(desired['block']), desired['parameter']) != (modifier['block'], modifier['parameter']):
                emit({'op': 'modifier.remove', 'modifier': modifier['ref']})
        used.update(b['instance_id'] for b in state['current']['blocks'] if b['type'] != 'shunt')
        # Resolve retained moves after disconnecting cables. Swaps use a free
        # temporary cell and ordinary operations, so the executable preview
        # describes every intermediate change without discarding block settings.
        pending = {n['hardware_ref']: positions[n['id']] for n in d['nodes']
                   if d.get('mode') == 'refine' and n.get('hardware_ref') and
                   any(b['ref'] == n['hardware_ref'] for b in state['current']['blocks']) and
                   block(state['current'], n['hardware_ref'])['position'] != positions[n['id']]}
        while pending:
            occupied = {(b['position']['row'], b['position']['column']): b['ref'] for b in state['current']['blocks']}
            movable = next((ref for ref, p in pending.items() if (p['row'], p['column']) not in occupied), None)
            if movable is not None:
                emit({'op': 'block.move', 'block': movable, **pending.pop(movable)})
                continue
            temporary = next(({'row': row, 'column': col} for col in range(1, 15) for row in range(1, 7)
                              if (row, col) not in occupied), None)
            require(temporary is not None, 'NO_SAFE_MOVE', 'Occupied-cell swap needs a free temporary cell')
            ref = next(iter(pending))
            emit({'op': 'block.move', 'block': ref, **temporary})
        for n in d['nodes']:
            type_ = effect_type(n['type'])
            p = positions[n['id']]
            ref = n.get('hardware_ref') if d.get('mode') == 'refine' else None
            existing = next((b for b in state['current']['blocks'] if b['ref'] == ref), None)
            if existing:
                require(existing['type'] == type_['alias'], 'TYPE_MISMATCH', 'Changing a retained block type requires a new block')
                require(existing['position'] == p, 'MOVE_ORDER_UNRESOLVED', 'Retained block did not reach its planned cell')
            else:
                if type_['alias'] == 'shunt':
                    ref = f"shunt-r{p['row']}c{p['column']}"
                else:
                    candidate = next((i for i in type_['instances'] if i['id'] not in used), None)
                    require(candidate is not None, 'INSTANCE_UNAVAILABLE', 'No available effect instance', type=n['type'])
                    ref = instance_ref(type_, candidate); used.add(candidate['id'])
                emit({'op': 'block.add', 'type': type_['alias'], 'ref': ref, **p})
            mapping[n['id']] = ref
        for n in d['nodes']:
            ref = mapping[n['id']]
            settings = copy.deepcopy(n.get('channels', {}))
            active = n.get('active_channel', 'A')
            require(active in 'ABCD' and len(active) == 1, 'INVALID_CHANNEL', 'Active channel must be A–D')
            settings.setdefault(active, {}).update(n.get('parameters', {}))
            if 'model' in n and n['model'] is not None:
                settings[active][n.get('model_parameter', 'model')] = n['model']
            for channel, values in settings.items():
                for name, value in values.items():
                    current = block(state['current'], ref)['channels'][channel]['parameters'].get(name)
                    if current is not None and current.get('value') == value:
                        continue
                    emit({'op': 'parameter.set', 'block': ref, 'channel': channel, 'parameter': name, 'value': value})
            if block(state['current'], ref)['active_channel'] != active:
                emit({'op': 'channel.set', 'block': ref, 'channel': active})
            if block(state['current'], ref)['bypass'] != n.get('bypass', False):
                emit({'op': 'block.bypass', 'block': ref, 'bypass': n.get('bypass', False)})
        for edge in d['edges']:
            emit({'op': 'block.connect', 'from': mapping[edge['from']], 'to': mapping[edge['to']]})
        # A node's default channel/bypass is the baseline for every scene;
        # explicit scene maps override it. Keep logical and physical previews identical.
        baseline = [n for n in d['nodes'] if d.get('mode') != 'refine' or n.get('hardware_ref') not in original_nodes]
        if baseline:
            for number in range(1, 9):
                emit({'op': 'scene.set', 'scene': number,
                      'channels': {mapping[n['id']]: n.get('active_channel', 'A') for n in baseline},
                      'bypass': {mapping[n['id']]: n.get('bypass', False) for n in baseline}})
        scene_numbers = set()
        for s in d.get('scenes', []):
            number = s['number']
            require(number not in scene_numbers, 'INVALID_SCENE', 'Duplicate scene numbers'); scene_numbers.add(number)
            require(type(number) is int and 1 <= number <= 8, 'INVALID_SCENE', 'Scenes use 1–8')
            current_scene = state['current']['scenes'][number - 1]
            patch = {'op': 'scene.set', 'scene': number}
            if 'name' in s and s['name'] != current_scene['name']:
                patch['name'] = s['name']
            for field in ('channels', 'bypass', 'controllers'):
                values = {mapping[k] if field != 'controllers' else k: v for k, v in s.get(field, {}).items()}
                changes = {k: v for k, v in values.items() if current_scene[field].get(k) != v}
                if changes:
                    patch[field] = changes
            if len(patch) > 2:
                emit(patch)
        for controller, settings in d.get('controllers', {}).items():
            emit({'op': 'controller.set', 'controller': controller, 'settings': settings})
        for m in d.get('modifiers', []):
            emit({'op': 'modifier.set', 'modifier': m['ref'], 'block': mapping[m['block']],
                  'parameter': m['parameter'], 'source': m['source'], 'settings': m.get('settings', {})})
        emit({'op': 'scene.select', 'scene': d.get('active_scene', 1)})
        require(len(ops) <= MAX_OPERATIONS, 'PLAN_LIMIT', 'Compiled design exceeds operation limit', maximum=MAX_OPERATIONS)
        validate_preset(state['current'])
        result.update(physical=inspect(state['current']), mapping=mapping, operations=ops, buildable=True)
        result['warnings'].append('Simulation only; mock models and units are not hardware selections or audio synthesis.')
        result['warnings'].append('DSP load and audible result have not been measured.')
    except (AxeError, KeyError, TypeError, ValueError) as exc:
        result['blockers'].append({'code': getattr(exc, 'code', 'INVALID_DESIGN'), 'message': str(exc), 'details': getattr(exc, 'details', {})})
        result['alternatives'] = [{'action': 'revise_design', 'description': 'Keep this revision and create an explicitly reviewed feasible alternative.'}]
    return result
