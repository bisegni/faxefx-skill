"""Agent-facing commands. Mutations create previews; apply requires approval."""
import argparse
import copy
import json
import sys

from . import SCHEMA_VERSION, __version__
from .backends import MockBackend, ReplayBackend, USBBackend
from .catalog import CATALOG, READ_AMP, effect_type, mock_value, parameter, summary
from .engine import Engine
from .errors import AxeError, require
from .model import block, inspect
from .operations import FIELDS, MODIFIER_FIELDS, SOURCES, scene
from .storage import Store, load


class Parser(argparse.ArgumentParser):
    def error(self,message):
        raise AxeError('INVALID_ARGUMENTS',message)


def value(text):
    try:return json.loads(text,parse_constant=lambda x: (_ for _ in ()).throw(ValueError(x)))
    except ValueError:return text


def parser():
    p=Parser(prog='axefx.py',description='Inspect presets and plan approved edits. USB is the default; tests use --backend mock/replay.')
    p.add_argument('--version',action='version',version=__version__)
    p.add_argument('--backend',choices=('usb','mock','replay'),default='usb')
    p.add_argument('--state-dir',help='Artifact/mock state directory; defaults to the platform user data directory')
    p.add_argument('--format',choices=('json','text'),default='json')
    p.add_argument('--replay-file',help='traffic.jsonl capture for replay; replay never sends USB traffic')
    p.add_argument('--input',help='Full MIDI input endpoint name')
    p.add_argument('--output',help='Full MIDI output endpoint name')
    groups=p.add_subparsers(dest='group',required=True)
    groups.add_parser('capabilities');groups.add_parser('schema')
    c=groups.add_parser('catalog');c.add_argument('--type');c.add_argument('--parameter')
    def actions(name,names):
        root=groups.add_parser(name);sub=root.add_subparsers(dest='action',required=True)
        return {n:sub.add_parser(n) for n in names}
    actions('device',['list','inspect'])
    a=actions('preset',['inspect','geometry','new','copy','rename','save'])
    a['new'].add_argument('--name',required=True);a['new'].add_argument('--empty',action='store_true',required=True)
    a['copy'].add_argument('source',help='current or saved slot number');a['copy'].add_argument('--name',required=True)
    a['rename'].add_argument('--name',required=True)
    a['save'].add_argument('--slot',required=True,type=int);a['save'].add_argument('--overwrite',action='store_true')
    a=actions('block',['list','inspect','add','remove','move','connect','disconnect','bypass'])
    a['inspect'].add_argument('block');a['inspect'].add_argument('--parameter')
    a['add'].add_argument('type');a['add'].add_argument('--ref')
    for verb in ('add','move'):
        a[verb].add_argument('--row',type=int,required=True);a[verb].add_argument('--column',type=int,required=True)
    for verb in ('remove','move','bypass'):a[verb].add_argument('block')
    a['bypass'].add_argument('--state',choices=('on','off'),required=True)
    for verb in ('connect','disconnect'):
        a[verb].add_argument('source');a[verb].add_argument('target')
    a=actions('parameter',['get','set'])
    for verb in a:
        a[verb].add_argument('block');a[verb].add_argument('parameter');a[verb].add_argument('--channel',choices=list('ABCD'))
    a['set'].add_argument('--value',required=True,type=value);a['set'].add_argument('--raw',action='store_true')
    a=actions('channel',['inspect','set','copy'])
    for verb in a:a[verb].add_argument('block')
    a['inspect'].add_argument('--channel',choices=list('ABCD'))
    a['set'].add_argument('--channel',required=True,choices=list('ABCD'))
    a['copy'].add_argument('--from',dest='source',required=True,choices=list('ABCD'))
    a['copy'].add_argument('--to',dest='target',required=True,choices=list('ABCD'))
    a=actions('scene',['inspect','set','select','copy'])
    a['inspect'].add_argument('--scene',type=int)
    for verb in ('set','select'):a[verb].add_argument('scene',type=int)
    a['set'].add_argument('--name')
    for field in ('channels','bypass','controllers'):a['set'].add_argument('--'+field,type=value)
    a['copy'].add_argument('--from',dest='source',required=True,type=int);a['copy'].add_argument('--to',dest='target',required=True,type=int)
    a=actions('modifier',['inspect','set','remove'])
    a['inspect'].add_argument('modifier',nargs='?')
    for verb in ('set','remove'):a[verb].add_argument('modifier')
    a['set'].add_argument('--block',required=True);a['set'].add_argument('--parameter',required=True)
    a['set'].add_argument('--source',required=True,choices=SOURCES);a['set'].add_argument('--settings',type=value,default={})
    a=actions('controller',['inspect','set'])
    a['inspect'].add_argument('controller',nargs='?');a['set'].add_argument('controller');a['set'].add_argument('--settings',type=value,required=True)
    a=actions('snapshot',['create','inspect']);a['inspect'].add_argument('id')
    a=actions('edit',['plan','apply','restore','inspect','journal'])
    a['plan'].add_argument('--file',required=True,help='JSON object with an operations array, or an operations array')
    for verb in ('apply','restore','inspect','journal'):a[verb].add_argument('id')
    a['restore'].add_argument('--source-slot',type=int,help='Restore this saved-slot backup into the edit buffer, preserving stored presets')
    a['apply'].add_argument('--approved',action='store_true',help='Caller acknowledges user approval of this exact immutable plan')
    return p


def command_operation(a):
    g,v=a.group,getattr(a,'action',None)
    if g=='preset':
        if v=='new':return {'op':'preset.new','name':a.name}
        if v=='copy':return {'op':'preset.copy','source':a.source,'name':a.name}
        if v=='rename':return {'op':'preset.rename','name':a.name}
        if v=='save':return {'op':'preset.save','slot':a.slot,'overwrite':a.overwrite}
    if g=='block':
        if v=='add':return {**{'op':'block.add','type':a.type,'row':a.row,'column':a.column},**({'ref':a.ref} if a.ref else {})}
        if v=='remove':return {'op':'block.remove','block':a.block}
        if v=='move':return {'op':'block.move','block':a.block,'row':a.row,'column':a.column}
        if v in ('connect','disconnect'):return {'op':'block.'+v,'from':a.source,'to':a.target}
        if v=='bypass':return {'op':'block.bypass','block':a.block,'bypass':a.state=='on'}
    if g=='parameter' and v=='set':return {**{'op':'parameter.set','block':a.block,'parameter':a.parameter,'value':a.value,'raw':a.raw},**({'channel':a.channel} if a.channel else {})}
    if g=='channel':
        if v=='set':return {'op':'channel.set','block':a.block,'channel':a.channel}
        if v=='copy':return {'op':'channel.copy','block':a.block,'from':a.source,'to':a.target}
    if g=='scene':
        if v=='select':return {'op':'scene.select','scene':a.scene}
        if v=='copy':return {'op':'scene.copy','from':a.source,'to':a.target}
        if v=='set':return {'op':'scene.set','scene':a.scene,**{k:getattr(a,k) for k in ('name','channels','bypass','controllers') if getattr(a,k) is not None}}
    if g=='modifier':
        if v=='set':return {'op':'modifier.set','modifier':a.modifier,'block':a.block,'parameter':a.parameter,'source':a.source,'settings':a.settings}
        if v=='remove':return {'op':'modifier.remove','modifier':a.modifier}
    if g=='controller' and v=='set':return {'op':'controller.set','controller':a.controller,'settings':a.settings}
    return None


def schemas():
    return {'schema_version':SCHEMA_VERSION,'operations':{k:{'required':['op',*sorted(r)],'optional':sorted(o)} for k,(r,o) in FIELDS.items()},
            'proposal':{'operations':'array of typed operations; 1–256 entries; saving must be a separate batch'},
            'result':{'schema_version':'string','ok':'boolean','backend':'usb|mock|replay','data':'object on success','error':'code/message/details on failure','warnings':'array'},
            'coordinates':{'rows':'one-based 1–6','columns':'one-based 1–14','scenes':'1–8','channels':'A–D','preset_slots':'device numbers 0–1023'},
            'modifier_sources':SOURCES,'modifier_ids':[f'modifier{i}' for i in range(1,33)],'modifier_settings':sorted(MODIFIER_FIELDS),
            'raw_parameter_support':{'amp':['gain','bass','mid','treble','master','presence']},
            'mock_parameter_units':'Only tested amp knobs use a raw/display conversion. Other numeric values are provisional mock-cache units, not verified physical or normalized device values.',
            'controller_settings':{'lfo1/lfo2':['rate','depth','shape'],'envelope':['attack','release','gain'],
                                   'sequencer':['steps','rate'],'pitch':['minimum','maximum'],'manual1..5':['value'],'scene-controller1/2':['values']},
            'approval':'Mutation commands preview; edit apply ID --approved acknowledges user approval of that immutable plan. This flag does not independently authenticate a user event.',
            'unknown_fields':'Operation fields not listed are rejected. No raw MIDI/USB interface is exposed.'}


def execute(a,backend,store):
    engine=Engine(backend,store)
    if a.group=='capabilities':return backend.capabilities()
    if a.group=='schema':return schemas()
    if a.group=='catalog':
        require(a.type is not None or a.parameter is None,'INVALID_ARGUMENTS','--parameter requires --type')
        t=effect_type(a.type) if a.type else None
        if a.parameter:
            definition=next(p for p in summary(t)['parameters'] if p['id']==parameter(t,a.parameter)['id'])
            return {'firmware':CATALOG['firmware'],'type':t['alias'],'parameter':definition}
        return summary(t)
    if a.group=='edit':
        if a.action=='plan':
            proposal=load(a.file)
            if isinstance(proposal,dict):
                require(set(proposal)=={'operations'},'INVALID_PLAN','Proposal object must contain only operations')
                proposal=proposal['operations']
            return engine.plan(proposal)
        if a.action=='apply':return engine.apply(a.id,a.approved)
        if a.action=='restore':return engine.plan([{'op':'snapshot.restore','snapshot_id':a.id,
                                                 **({'source_slot':a.source_slot} if a.source_slot is not None else {})}])
        if a.action=='inspect':return store.get('plans',a.id)
        if a.action=='journal':
            store.get('plans',a.id)
            return load(store.root/'journals'/(a.id+'.json'))
    if a.group=='snapshot':
        return engine.snapshot() if a.action=='create' else store.get('snapshots',a.id)
    op=command_operation(a)
    if op:return engine.plan([op])
    if a.group=='device' and a.action=='list':return backend.devices()
    if a.group=='preset' and a.action=='geometry':
        require('geometry' in backend.capabilities()['read'],'PROTOCOL_UNRESOLVED','Grid/cable retrieval is not recovered on this backend')
    if a.group in ('modifier','controller'):
        require(a.group+'s' in backend.capabilities()['read'],'PROTOCOL_UNRESOLVED','This preset inspection is unresolved over USB/replay')
    if a.group=='parameter' or (a.group=='block' and a.action=='inspect' and a.parameter):
        if backend.name=='mock':
            state=backend.read_state();b=block(state['current'],a.block);t=effect_type(b['type_id']);p=parameter(t,a.parameter)
            ch=getattr(a,'channel',None) or b['active_channel']
            require(ch in b['channels'],'INVALID_CHANNEL','Channel does not exist')
            current=b['channels'][ch]['parameters'].get(p['name'])
            if current is None:
                initial=p['options'][0] if p['options'] else p['numeric_fields'][0]
                current=mock_value(t,p['name'],initial)
            return {'block':b['ref'],'channel':ch,'parameter':p['name'],'definition':p,'current':current,'simulated':True}
        require(getattr(a,'channel',None) is None,'PROTOCOL_UNRESOLVED','Use the current channel for USB reads')
        return {'block':a.block,'parameter':a.parameter,'current':backend.parameter_value(a.block,a.parameter)}
    state=backend.read_state();p=state['current']
    if a.group=='device':return {'device':state['device'],'capabilities':backend.capabilities()}
    if a.group=='preset':
        result=inspect(p)
        if a.action=='geometry':return {k:result[k] for k in ('geometry','cells','blocks','connections')}
        return result
    if a.group=='block':
        result=inspect(p)
        if a.action=='list':return {'blocks':result['blocks'],'complete_geometry':p['complete']}
        b=block(result,a.block)
        definitions=summary(effect_type(b['type_id'])) if b['type_id'] is not None else None
        if definitions:
            definitions['parameters']=[{k:record[k] for k in ('id','name','label','usb_get','usb_set')} for record in definitions['parameters']]
        return {**b,'catalog':definitions,'catalog_firmware':CATALOG['firmware'],'device_firmware':state['device']['firmware']}
    if a.group=='channel':
        b=block(p,a.block);ch=a.channel or b['active_channel']
        require(ch in b['channels'],'INVALID_CHANNEL','Channel does not exist')
        require(p['complete'] or ch==b['active_channel'],'PROTOCOL_UNRESOLVED','Inactive-channel contents are unresolved')
        return {'block':b['ref'],'channel':ch,'content':b['channels'][ch],'complete':p['complete']}
    if a.group=='scene':
        number=a.scene or p['active_scene']
        if p['scenes'] is not None:return copy.deepcopy(scene(p,number))
        require(number==p['active_scene'],'PROTOCOL_UNRESOLVED','Other scene contents are unresolved')
        return {'number':number,'name':p['active_scene_name'],'channels':{b['ref']:b['active_channel'] for b in p['blocks']},
                'bypass':{b['ref']:b['bypass'] for b in p['blocks']},'complete':False}
    if a.group=='modifier':
        if a.modifier:
            matches=[m for m in p['modifiers'] if m['ref']==a.modifier]
            require(len(matches)==1,'INVALID_MODIFIER','Modifier not present')
            return matches[0]
        return {'modifiers':p['modifiers']}
    if a.group=='controller':
        if a.controller:
            require(a.controller in p['controllers'],'INVALID_CONTROLLER','Controller has no explicit simulated settings')
            return {'controller':a.controller,'settings':p['controllers'][a.controller]}
        return {'controllers':p['controllers']}
    raise AxeError('INVALID_ARGUMENTS','Unsupported command')


def text_output(result):
    if not result['ok']:
        return result['error']['code']+': '+result['error']['message']
    data=result['data']
    lines=[f"backend={result['backend']} schema={SCHEMA_VERSION}"]
    if isinstance(data,dict) and 'cells' in data:
        rows=data['cells'];width=max(8,max((len(cell or '.') for row in rows for cell in row),default=0))
        lines.append('     '+' '.join(str(i).center(width) for i in range(1,len(rows[0])+1)))
        for i,row in enumerate(rows,1):lines.append(f'{i:>3} |'+'|'.join((cell or '.').center(width) for cell in row)+'|')
        lines.extend(f"{e['from']} -> {e['to']}" for e in data['connections'])
    else:lines.append(json.dumps(data,indent=2,allow_nan=False))
    lines.extend('Warning: '+w for w in result['warnings'])
    return '\n'.join(lines)


def main(argv=None):
    args=None;backend=None
    try:
        args=parser().parse_args(argv)
        # Pure discovery commands have no device or artifact-store side effects.
        if args.group in ('schema','catalog'):
            data=schemas() if args.group=='schema' else execute(args,None,None)
            warnings=[]
        else:
            store=Store(args.state_dir)
            if args.backend=='mock':backend=MockBackend(store)
            elif args.backend=='replay':
                require(bool(args.replay_file),'INVALID_ARGUMENTS','--backend replay requires --replay-file')
                backend=ReplayBackend(store,args.replay_file)
            else:backend=USBBackend(store,args.input,args.output)
            lock_store=Store() if args.backend=='usb' else store
            lock_identity='usb-midi' if args.backend=='usb' else backend.identity
            with lock_store.lock(lock_identity):data=execute(args,backend,store)
            warnings=[]
            if args.backend=='mock':warnings=['Simulated device. No USB traffic was sent; mock behavior is not hardware verification.']
            elif args.backend=='replay':warnings=['File replay only. No USB traffic was sent; file contents do not establish hardware verification.']
            else:warnings=['New setters are hardware-unverified. Geometry, full snapshots and structural wire operations remain unresolved.']
        result={'schema_version':SCHEMA_VERSION,'ok':True,'backend':args.backend,'data':data,'warnings':warnings}
        code=0
    except AxeError as exc:
        result={'schema_version':SCHEMA_VERSION,'ok':False,'backend':args.backend if args else None,
                'error':{'code':exc.code,'message':str(exc),'details':exc.details},'warnings':[]}
        code=2
    except (ValueError,TypeError,KeyError,OSError) as exc:
        result={'schema_version':SCHEMA_VERSION,'ok':False,'backend':args.backend if args else None,
                'error':{'code':'INVALID_INPUT','message':str(exc),'details':{}},'warnings':[]};code=2
    finally:
        if backend:backend.close()
    print(text_output(result) if args and args.format=='text' else json.dumps(result,allow_nan=False))
    return code


if __name__=='__main__':
    raise SystemExit(main())
