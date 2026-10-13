"""Typed domain operations shared by previews and the stateful mock."""
import copy

from .catalog import effect_type, finite, parameter
from .errors import AxeError, require
from .model import at, add_block, block, check_position, connect, empty_preset, ensure_channel, routes, set_parameter

FIELDS = {
    'block.add': ({'type', 'row', 'column'}, {'ref'}),
    'block.remove': ({'block'}, set()),
    'block.move': ({'block', 'row', 'column'}, set()),
    'block.connect': ({'from', 'to'}, set()),
    'block.disconnect': ({'from', 'to'}, set()),
    'block.bypass': ({'block', 'bypass'}, set()),
    'parameter.set': ({'block', 'parameter', 'value'}, {'channel', 'raw'}),
    'channel.set': ({'block', 'channel'}, set()),
    'channel.copy': ({'block', 'from', 'to'}, set()),
    'scene.set': ({'scene'}, {'name', 'channels', 'bypass', 'controllers'}),
    'scene.select': ({'scene'}, set()),
    'scene.copy': ({'from', 'to'}, set()),
    'modifier.set': ({'modifier', 'block', 'parameter', 'source'}, {'settings'}),
    'modifier.remove': ({'modifier'}, set()),
    'controller.set': ({'controller', 'settings'}, set()),
    'preset.new': ({'name'}, set()),
    'preset.copy': ({'source', 'name'}, set()),
    'preset.rename': ({'name'}, set()),
    'preset.save': ({'slot'}, {'overwrite'}),
    'snapshot.restore': ({'snapshot_id'}, {'source_slot'}),
}
SOURCES = ([f'external{i}' for i in range(1,17)] + [f'manual{i}' for i in range(1,6)] +
           ['lfo1', 'lfo2', 'envelope', 'sequencer', 'pitch', 'scene-controller1', 'scene-controller2'])
MODIFIER_FIELDS = {'minimum', 'maximum', 'start', 'end', 'mid', 'slope', 'scale', 'offset', 'damping', 'auto-engage'}


def validate_operation(op):
    require(isinstance(op, dict) and op.get('op') in FIELDS, 'INVALID_OPERATION', 'Choose an operation from schema')
    required, optional = FIELDS[op['op']]
    require(required <= op.keys() and not (op.keys() - required - optional - {'op'}),
            'INVALID_OPERATION', 'Missing or unknown operation fields', required=sorted(required), optional=sorted(optional))
    require(not ('raw' in op and type(op['raw']) is not bool), 'INVALID_OPERATION', 'raw must be boolean')
    return op


def scene(preset, number):
    require(type(number) is int and 1 <= number <= 8, 'INVALID_SCENE', 'Scenes use numbers 1 through 8')
    return preset['scenes'][number-1]


def name_check(name):
    require(isinstance(name, str) and name and len(name.encode('ascii', 'replace')) <= 31 and
            name.isascii() and all(32 <= ord(c) < 127 for c in name), 'INVALID_NAME', 'Use 1–31 printable ASCII characters')


def modifier_active(preset, reference, param):
    return any(m['block'] == reference and m['parameter'] == param for m in preset['modifiers'])


def set_active_scene(p):
    s = scene(p, p['active_scene'])
    for b in p['blocks']:
        b['active_channel'] = s['channels'][b['ref']]
        if not modifier_active(p,b['ref'],'bypass'):
            b['bypass'] = s['bypass'][b['ref']]


def sync_scene_controllers(p):
    for name in ('scene-controller1','scene-controller2'):
        if name in p['controllers'] or any(name in s['controllers'] for s in p['scenes']):
            p['controllers'].setdefault(name,{})['values']=[s['controllers'].get(name,0) for s in p['scenes']]


def controller_settings(name, settings):
    require(isinstance(settings, dict) and bool(settings), 'INVALID_CONTROLLER', 'settings must be a nonempty object')
    if name.startswith('scene-controller'):
        allowed = {'values'}
    elif name.startswith('manual'):
        allowed = {'value'}
    elif name.startswith('lfo'):
        allowed = {'rate', 'depth', 'shape'}
    elif name == 'envelope':
        allowed = {'attack', 'release', 'gain'}
    elif name == 'sequencer':
        allowed = {'steps', 'rate'}
    elif name == 'pitch':
        allowed = {'minimum', 'maximum'}
    else:
        raise AxeError('INVALID_CONTROLLER', 'Only preset-local controllers are editable', controller=name)
    require(name in SOURCES and set(settings) <= allowed, 'INVALID_CONTROLLER', 'Unknown controller or setting', allowed=sorted(allowed))
    for key, value in settings.items():
        if key == 'shape':
            require(value in ('sine','triangle','square','random'), 'INVALID_CONTROLLER', 'Unknown simulated LFO shape')
        elif key in ('steps','values'):
            require(isinstance(value,list) and (len(value)==8 if key=='values' else 1<=len(value)<=32),
                    'INVALID_CONTROLLER', 'Scene values need eight entries; sequencer needs 1–32 steps')
            require(all(0<=finite(v)<=1 for v in value), 'INVALID_CONTROLLER', 'Controller values must be 0–1')
        else:
            limit = 1 if key in ('value','depth','minimum','maximum') else 10000
            require(0 <= finite(value) <= limit, 'INVALID_CONTROLLER', 'Outside simulated controller range', field=key, maximum=limit)
    return copy.deepcopy(settings)


def apply_operation(state, op, snapshots=None):
    validate_operation(op)
    p = state['current']
    kind = op['op']
    note = {}
    if kind == 'block.add':
        b = add_block(p, op['type'], op['row'], op['column'], op.get('ref'))
        note['block'] = b['ref']
    elif kind == 'block.remove':
        b = block(p,op['block']); ref=b['ref']
        p['blocks'].remove(b)
        p['connections'] = [e for e in p['connections'] if ref not in (e['from'],e['to'])]
        p['modifiers'] = [m for m in p['modifiers'] if m['block'] != ref]
        for s in p['scenes']:
            s['channels'].pop(ref,None); s['bypass'].pop(ref,None)
    elif kind == 'block.move':
        b = block(p,op['block']); check_position(p,op['row'],op['column'])
        occupied=at(p,op['row'],op['column'])
        require(occupied is None or occupied is b,'CELL_OCCUPIED','Move does not swap occupied cells')
        b['position']={'row':op['row'],'column':op['column']}
    elif kind == 'block.connect':
        source,target=block(p,op['from'])['ref'],block(p,op['to'])['ref']
        note=connect(p,source,target)
        if note['existing']:
            return {**note,'no_op':True}
    elif kind == 'block.disconnect':
        source,target=block(p,op['from'])['ref'],block(p,op['to'])['ref']
        paths=routes(p,source,target)
        require(len(paths)==1,'AMBIGUOUS_CONNECTION','Specify an existing unambiguous cable/route',paths=paths)
        path=paths[0]
        # Branching routes require physical cable removal, not implicit path destruction.
        require(all(sum(e['from']==ref or e['to']==ref for e in p['connections'])==2 for ref in path[1:-1]),
                'AMBIGUOUS_CONNECTION','Route has branches; disconnect a physical cable explicitly')
        for a,z in zip(path,path[1:]):
            p['connections'].remove({'from':a,'to':z})
        note['retained_shunts']=path[1:-1]
    elif kind == 'parameter.set':
        b=block(p,op['block']); name=parameter(effect_type(b['type_id']),op['parameter'])['name']
        require(not modifier_active(p,b['ref'],name),'MODIFIER_ACTIVE','Remove the modifier explicitly before a manual parameter edit')
        set_parameter(p,b['ref'],name,op['value'],op.get('channel'),op.get('raw',False))
    elif kind == 'block.bypass':
        b=block(p,op['block'])
        require(type(op['bypass']) is bool,'INVALID_VALUE','bypass must be boolean')
        require(not modifier_active(p,b['ref'],'bypass'),'MODIFIER_ACTIVE','Bypass is modifier-controlled')
        b['bypass']=op['bypass'];scene(p,p['active_scene'])['bypass'][b['ref']]=b['bypass']
    elif kind == 'channel.set':
        b=block(p,op['block']);ensure_channel(b,op['channel'])
        b['active_channel']=op['channel'];scene(p,p['active_scene'])['channels'][b['ref']]=op['channel']
    elif kind == 'channel.copy':
        b=block(p,op['block']);ensure_channel(b,op['from']);ensure_channel(b,op['to'])
        b['channels'][op['to']]=copy.deepcopy(b['channels'][op['from']])
    elif kind == 'scene.select':
        scene(p,op['scene']);p['active_scene']=op['scene'];set_active_scene(p)
    elif kind == 'scene.copy':
        source=copy.deepcopy(scene(p,op['from']));target=scene(p,op['to'])
        source['number']=target['number'];p['scenes'][target['number']-1]=source;set_active_scene(p);sync_scene_controllers(p)
    elif kind == 'scene.set':
        s=scene(p,op['scene'])
        if 'name' in op:
            require(isinstance(op['name'],str) and len(op['name'])<=31 and op['name'].isascii() and all(32<=ord(c)<127 for c in op['name']),
                    'INVALID_NAME','Scene name must be printable ASCII, at most 31 characters')
            s['name']=op['name']
        for field in ('channels','bypass','controllers'):
            if field not in op:continue
            require(isinstance(op[field],dict),'INVALID_SCENE',field+' must be an object')
            for ref,value in op[field].items():
                if field=='controllers':
                    require(ref in ('scene-controller1','scene-controller2') and 0<=finite(value)<=1,
                            'INVALID_CONTROLLER','Scene controller value must be 0–1')
                else:
                    b=block(p,ref);ref=b['ref']
                    if field=='channels':ensure_channel(b,value)
                    else:
                        require(type(value) is bool,'INVALID_VALUE','bypass must be boolean')
                        require(not modifier_active(p,ref,'bypass'),'MODIFIER_ACTIVE','Scene bypass is overridden by a modifier')
                s[field][ref]=value
        set_active_scene(p);sync_scene_controllers(p)
    elif kind == 'modifier.set':
        b=block(p,op['block']);name=op['parameter']
        if name!='bypass':name=parameter(effect_type(b['type_id']),name)['name']
        require(op['source'] in SOURCES,'INVALID_MODIFIER','Unknown modifier source',sources=SOURCES)
        ref=op['modifier']
        require(ref in [f'modifier{i}' for i in range(1,33)],'INVALID_MODIFIER','Use modifier1 through modifier32')
        settings=op.get('settings',{})
        require(isinstance(settings,dict) and set(settings)<=MODIFIER_FIELDS,'INVALID_MODIFIER','Unknown modifier settings',allowed=sorted(MODIFIER_FIELDS))
        for key,value in settings.items():
            if key=='auto-engage':require(type(value) is bool,'INVALID_MODIFIER','auto-engage must be boolean')
            else:
                bound=10000 if key=='damping' else 1
                require(0<=finite(value)<=bound,'INVALID_MODIFIER','Outside simulated modifier bounds',field=key)
        require(settings.get('minimum',0)<=settings.get('maximum',1),'INVALID_MODIFIER','minimum exceeds maximum')
        require(not any(m['block']==b['ref'] and m['parameter']==name and m['ref']!=ref for m in p['modifiers']),
                'MODIFIER_ACTIVE','Parameter already has another modifier')
        p['modifiers']=[m for m in p['modifiers'] if m['ref']!=ref]
        p['modifiers'].append({'ref':ref,'block':b['ref'],'parameter':name,'source':op['source'],'settings':copy.deepcopy(settings)})
    elif kind == 'modifier.remove':
        require(any(m['ref']==op['modifier'] for m in p['modifiers']),'INVALID_MODIFIER','Modifier does not exist')
        p['modifiers']=[m for m in p['modifiers'] if m['ref']!=op['modifier']]
    elif kind == 'controller.set':
        settings=controller_settings(op['controller'],op['settings'])
        p['controllers'].setdefault(op['controller'],{}).update(settings)
        if op['controller'].startswith('scene-controller') and 'values' in settings:
            for s,v in zip(p['scenes'],settings['values']):s['controllers'][op['controller']]=v
    elif kind == 'preset.new':
        name_check(op['name']);state['current']=empty_preset(op['name'])
    elif kind == 'preset.copy':
        name_check(op['name'])
        if op['source']=='current':source=p
        else:
            require(str(op['source']) in state['presets'],'PRESET_NOT_FOUND','Source slot is not present in the simulated library')
            source=state['presets'][str(op['source'])]
        state['current']=copy.deepcopy(source);state['current']['slot']=None;state['current']['name']=op['name']
    elif kind == 'preset.rename':
        name_check(op['name']);p['name']=op['name']
    elif kind == 'preset.save':
        require(type(op['slot']) is int and 0<=op['slot']<=1023,'INVALID_SLOT','Preset slots use device numbers 0–1023')
        require(type(op.get('overwrite',False)) is bool,'INVALID_VALUE','overwrite must be boolean')
        require(str(op['slot']) not in state['presets'] or op.get('overwrite',False),'SLOT_OCCUPIED',
                'Saving to an occupied slot requires explicit overwrite approval',slot=op['slot'])
        p['slot']=op['slot'];p['unsaved']=False;state['presets'][str(op['slot'])]=copy.deepcopy(p)
    elif kind == 'snapshot.restore':
        require(snapshots is not None and op['snapshot_id'] in snapshots,'INVALID_ARTIFACT','Snapshot not supplied to planner')
        old=copy.deepcopy(snapshots[op['snapshot_id']]['state'])
        require(old['device']==state['device'],'DEVICE_CHANGED','Snapshot belongs to another device')
        if 'source_slot' in op:
            slot=op['source_slot']
            require(type(slot) is int and 0<=slot<=1023,'INVALID_SLOT','Source slot must be 0–1023')
            require(str(slot) in old['presets'],'PRESET_NOT_FOUND','Snapshot has no backup for this slot',slot=slot)
            restored=copy.deepcopy(old['presets'][str(slot)])
        else:
            restored=copy.deepcopy(old['current'])
        # Restoring the edit buffer never erases later saved presets. Occupied
        # destinations are restored only through another explicit save preview.
        saved=copy.deepcopy(state['presets'].get(str(restored['slot'])))
        contents=copy.deepcopy(restored);contents.pop('unsaved',None)
        if saved is not None:saved.pop('unsaved',None)
        restored['unsaved']=contents!=saved
        state['current']=restored
        note={'scope':'edit-buffer','saved_presets_preserved':True,'source_slot':op.get('source_slot')}
    if kind not in ('preset.save','snapshot.restore'):
        state['current']['unsaved']=True
    state['generation']+=1
    return note


def validate_preset(p):
    if not p.get('complete'):
        return
    for edge in p['connections']:
        a,z=block(p,edge['from']),block(p,edge['to'])
        require(z['position']['column']==a['position']['column']+1,'INVALID_CONNECTION',
                'Physical cables must join adjacent columns; disconnect/reconnect when moving blocks',connection=edge)
    require(len({(b['position']['row'],b['position']['column']) for b in p['blocks']})==len(p['blocks']),
            'CELL_OCCUPIED','Two blocks occupy the same cell')
