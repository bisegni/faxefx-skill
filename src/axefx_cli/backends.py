"""Stateful mock, captured traffic replay, and lazy cross-platform USB MIDI."""
import copy
import json
import math
import platform
import queue
import struct
import threading
import time
import uuid
from pathlib import Path

from .catalog import READ_AMP, SCALED_AMP, effect_type, finite, instance_ref, parameter, type_for_instance
from .codec import decode_editor, editor_frame, grid_query, integer, public_queries, unframe
from .errors import AxeError, require
from .grid import attach_grid, decode_grid
from .model import block, fixture_state
from .operations import FIELDS, apply_operation, validate_operation, validate_preset
from .storage import atomic_write, digest, load

ALL_OPS = list(FIELDS)


def validate_usb_packet(packet):
    """Last-boundary guard for fixed read queries and numeric amp writes only."""
    try:
        command,_=unframe(packet)
        if packet in public_queries().values():return
        if packet==grid_query():return
        require(command==1,'UNSUPPORTED_PACKET','Only fixed public/grid reads and amp parameter operations are implemented')
        d=decode_editor(packet)
        require(d['effect'] in (58,59,60,61) and d['index']==0 and not d['data_hex'],
                'UNSUPPORTED_PACKET','Invalid numeric amp operation fields')
        allowed={parameter(effect_type('amp'),name)['id'] for name in (READ_AMP if d['operation']==26 else SCALED_AMP)}
        require(d['parameter'] in allowed,'UNSUPPORTED_PACKET','Unsupported numeric amp parameter ID')
        require((d['operation']==26 and d['value_bits']==0) or
                (d['operation']==9 and math.isfinite(d['value_float']) and 0<=d['value_float']<=1),
                'UNSUPPORTED_PACKET','Only fixed getters or finite 0–1 numeric setters are implemented')
    except ValueError as exc:
        raise AxeError('UNSUPPORTED_PACKET','Malformed compiled packet',reason=str(exc)) from exc


class MockBackend:
    name = 'mock'
    identity = 'mock-axefx3'
    def __init__(self, store):
        self.store = store
        self.path = store.root / 'mock-device.json'

    def devices(self):
        return [fixture_state()['device']]

    def read_state(self, operations=()):
        return load(self.path) if self.path.exists() else fixture_state()

    def capabilities(self):
        return {'read': ['device','preset','geometry','blocks','parameters','channels','scenes','modifiers','controllers'],
                'edit': ALL_OPS, 'writes_enabled': True, 'verification': 'simulated',
                'hardware_verified': False, 'complete_snapshot': True}

    def check_operations(self, operations):
        for op in operations:validate_operation(op)

    def simulate(self, state, operations, snapshots):
        predicted=copy.deepcopy(state);notes=[]
        for op in operations:
            notes.append(apply_operation(predicted,op,snapshots))
            validate_preset(predicted['current'])
        return predicted,notes

    def apply(self, operation, snapshots):
        state=self.read_state()
        apply_operation(state,operation,snapshots)
        atomic_write(self.path,state)

    def restore(self, original, operations):
        restored=copy.deepcopy(original)
        restored['generation']=self.read_state()['generation']+1
        atomic_write(self.path,restored)

    def equivalent(self, expected, actual):
        a,b=copy.deepcopy(expected),copy.deepcopy(actual)
        a.pop('generation',None);b.pop('generation',None)
        return a==b

    def close(self):pass


class ReplayTransport:
    def __init__(self, path):
        self.path=Path(path)
        self.replies={};pending=None
        try:
            for line in self.path.read_text().splitlines():
                event=json.loads(line)
                if event.get('direction')=='tx':
                    pending=bytes.fromhex(event['hex']);unframe(pending)
                elif event.get('direction')=='rx' and pending is not None:
                    reply=bytes.fromhex(event['hex']);unframe(reply)
                    self.replies.setdefault(pending,[]).append(reply);pending=None
        except (OSError, ValueError, KeyError) as exc:
            raise AxeError('INVALID_CAPTURE','Capture is not valid traffic JSONL',reason=str(exc)) from exc
        require(bool(self.replies),'INVALID_CAPTURE','Capture contains no checksum-valid request/reply pairs')
        self.positions={}

    def query(self, packet):
        unframe(packet)
        require(packet in self.replies,'REPLAY_MISS','No captured reply for this request; replay never synthesizes hardware replies')
        position=self.positions.get(packet,0)
        # Repeated inspection returns the last recorded reply after exhausting this request.
        reply=self.replies[packet][min(position,len(self.replies[packet])-1)]
        self.positions[packet]=position+1
        return reply

    def send(self, packet):
        raise AxeError('READ_ONLY_BACKEND','Recorded-traffic replay cannot apply edits')

    def close(self):pass


class USBTransport:
    """No import/enumeration/connect happens until explicitly constructed."""
    def __init__(self, store, input_name=None, output_name=None, timeout=3):
        try:
            import rtmidi
        except ImportError as exc:
            raise AxeError('USB_DEPENDENCY_MISSING','Install config/requirements/cli.txt into your Python 3.11 environment') from exc
        api={'Darwin':rtmidi.API_MACOSX_CORE,'Windows':rtmidi.API_WINDOWS_MM,'Linux':rtmidi.API_LINUX_ALSA}.get(platform.system())
        require(api in rtmidi.get_compiled_api(),'USB_BACKEND_UNAVAILABLE','Native MIDI backend not compiled/available',platform=platform.system())
        self.input=self.output=None;self.log=None
        self.events=queue.Queue(maxsize=128);self.pending=bytearray();self.timeout=timeout
        self.receive_error=None;self.log_lock=threading.Lock()
        try:
            self.input=rtmidi.MidiIn(api);self.output=rtmidi.MidiOut(api)
            ins,outs=self.input.get_ports(),self.output.get_ports()
            def choose(names, selected):
                indexes=[i for i,n in enumerate(names) if n==selected] if selected else [i for i,n in enumerate(names) if 'axe-fx iii' in n.lower()]
                require(len(indexes)==1,'AMBIGUOUS_DEVICE','Select exactly one Axe-Fx III MIDI endpoint by its full name',ports=names,selected=selected)
                return indexes[0]
            i,o=choose(ins,input_name),choose(outs,output_name)
            self.port_names={'input':ins[i],'output':outs[o]}
            logdir=store.root/'traffic';logdir.mkdir(exist_ok=True)
            self.log=(logdir/(uuid.uuid4().hex+'.jsonl')).open('x',encoding='utf8')
            self.input.ignore_types(sysex=False,timing=True,active_sense=True)
            self.input.set_callback(self.receive)
            self.input.open_port(i);self.output.open_port(o)
        except AxeError:
            self.close();raise
        except Exception as exc:
            self.close();raise AxeError('USB_OPEN_FAILED','Cannot open paired MIDI endpoints',reason=str(exc)) from exc

    @staticmethod
    def enumerate_ports():
        try:
            import rtmidi
            api={'Darwin':rtmidi.API_MACOSX_CORE,'Windows':rtmidi.API_WINDOWS_MM,'Linux':rtmidi.API_LINUX_ALSA}.get(platform.system())
            require(api in rtmidi.get_compiled_api(),'USB_BACKEND_UNAVAILABLE','Native MIDI backend unavailable')
            i,o=rtmidi.MidiIn(api),rtmidi.MidiOut(api)
            try:return {'inputs':i.get_ports(),'outputs':o.get_ports()}
            finally:i.close_port();o.close_port()
        except ImportError as exc:
            raise AxeError('USB_DEPENDENCY_MISSING','Install config/requirements/cli.txt') from exc
        except AxeError:raise
        except Exception as exc:
            raise AxeError('USB_BACKEND_UNAVAILABLE','Cannot enumerate native MIDI ports',reason=str(exc)) from exc

    def record(self, direction, data):
        if self.log:
            with self.log_lock:
                self.log.write(json.dumps({'direction':direction,'monotonic':time.monotonic(),'hex':bytes(data).hex(' ')})+'\n')
                self.log.flush()

    def enqueue(self,event):
        try:self.events.put_nowait(event)
        except queue.Full:self.receive_error=AxeError('RECEIVE_OVERFLOW','Bounded MIDI receive queue overflowed; stop and inspect the connection')

    def receive(self, event, _data):
        try:
            message,_delta=event
            for b in message:
                if b>=0xf8:continue
                if b==0xf0:self.pending=bytearray([b])
                elif self.pending:
                    if b>=0x80 and b!=0xf7:
                        self.enqueue(AxeError('INVALID_REPLY','Unexpected status inside SysEx'));self.pending.clear();continue
                    self.pending.append(b)
                    if len(self.pending)>1024*1024:
                        self.enqueue(AxeError('INVALID_REPLY','SysEx exceeds bounded receive buffer'));self.pending.clear()
                    elif b==0xf7:
                        packet=bytes(self.pending);self.pending.clear();self.record('rx',packet);self.enqueue(packet)
        except Exception as exc:
            self.enqueue(AxeError('USB_RECEIVE_FAILED','Receive callback failed',reason=str(exc)))

    def send(self, packet):
        validate_usb_packet(packet)
        self.record('tx',packet)
        try:self.output.send_message(list(packet))
        except Exception as exc:
            raise AxeError('USB_SEND_FAILED','MIDI send result is uncertain',reason=str(exc)) from exc
        time.sleep(0.05)

    def query(self, packet):
        self.receive_error=None
        for _ in range(128):
            try:self.events.get_nowait()
            except queue.Empty:break
        command,_=unframe(packet)
        expected=decode_editor(packet) if command==1 else None
        self.send(packet);deadline=time.monotonic()+self.timeout
        while time.monotonic()<deadline:
            if self.receive_error:raise self.receive_error
            try:reply=self.events.get(timeout=max(.001,deadline-time.monotonic()))
            except queue.Empty:break
            if isinstance(reply,AxeError):raise reply
            try:
                rc,_=unframe(reply)
                if rc!=command:continue
                if expected:
                    d=decode_editor(reply)
                    if any(d[k]!=expected[k] for k in ('operation','effect','parameter')):continue
                return reply
            except ValueError as exc:
                raise AxeError('INVALID_REPLY','Malformed or checksum-invalid reply',reason=str(exc)) from exc
        raise AxeError('REPLY_TIMEOUT','No matching reply before request deadline',timeout_seconds=self.timeout)

    def close(self):
        if self.input is not None:
            try:self.input.cancel_callback();self.input.close_port()
            except Exception:pass
            self.input=None
        if self.output is not None:
            try:self.output.close_port()
            except Exception:pass
            self.output=None
        if self.log:
            self.log.close();self.log=None


class USBBackend:
    name='usb'
    def __init__(self,store,input_name=None,output_name=None,transport=None):
        self.store=store;self.input_name=input_name;self.output_name=output_name;self.transport=transport
        self.guard_state=None;self.guard_operations=()
        self.identity={'backend':self.name,'input':input_name,'output':output_name}

    def connection(self):
        if self.transport is None:self.transport=USBTransport(self.store,self.input_name,self.output_name)
        return self.transport

    def devices(self):
        return USBTransport.enumerate_ports()

    def capabilities(self):
        return {'read':['preset_identity','active_scene','scene_name','tempo','block_status','active_amp_parameters','geometry','connections'],
                'edit':['parameter.set'], 'writes_enabled':True, 'verification':'reads_hardware_verified; setter_static_only',
                'hardware_verified':False, 'complete_snapshot':False,
                'parameter_set_names':sorted(SCALED_AMP),'unresolved':['firmware_identification','full_snapshot','block_edits','scenes','channels','modifiers','controllers','copy','save'],
                'warning':'No experimental switch. Approved numeric amp writes are implemented but hardware-unverified. Other wire formats are unresolved.'}

    def check_operations(self,operations):
        for op in operations:
            validate_operation(op)
            require(op['op']=='parameter.set','PROTOCOL_UNRESOLVED',
                    'This USB operation lacks established wire semantics or full snapshot support',operation=op['op'])
            require(op['parameter'] in SCALED_AMP,'CONVERSION_UNRESOLVED','Only tested amp knob conversions are supported over USB',supported=sorted(SCALED_AMP))

    def query_public(self,name):
        reply=self.connection().query(public_queries()[name]);command,payload=unframe(reply)
        expected=unframe(public_queries()[name])[0]
        require(command==expected,'INVALID_REPLY','Wrong public reply command')
        return reply,payload

    def read_geometry(self):
        return decode_grid(self.connection().query(grid_query()))

    def parameter_value(self,ref,name):
        # Confirm existence/current channel through status; never assume an amp is present.
        state=self.read_state([{'op':'parameter.set','block':ref,'parameter':name,'value':0}],validate=False)
        b=block(state['current'],ref)
        return b['channels'][b['active_channel']]['parameters'][name]

    def read_parameter(self,b,name):
        require(b['type']=='amp' and name in READ_AMP,'PROTOCOL_UNRESOLVED','Getter is verified only for listed amp parameters',supported=sorted(READ_AMP))
        p=parameter(effect_type('amp'),name)
        reply=self.connection().query(editor_frame(0x1a,b['instance_id'],p['id']))
        try:d=decode_editor(reply)
        except ValueError as exc:raise AxeError('INVALID_REPLY','Invalid editor reply',reason=str(exc)) from exc
        require(d['operation']==0x1a and d['effect']==b['instance_id'] and d['parameter']==p['id'] and d['index']==0,
                'DEVICE_STATUS_ERROR','Parameter reply is not a matching successful getter',reply=d)
        raw=d['value_float'];finite(raw)
        text=bytes.fromhex(d['data_hex']).split(b'\0',1)[0].decode('utf8','replace')
        value=raw*10 if name in SCALED_AMP else None
        if name in SCALED_AMP:
            try:display_value=float(text)
            except ValueError:display_value=math.inf
            require(0<=raw<=1 and abs(display_value-value)<.02,'CONVERSION_UNRESOLVED',
                    'Connected device does not match the tested knob conversion',parameter=name,raw=raw,display=text)
        return {'value':value,'raw_value':raw,'raw_bits':d['value_bits'],'display':text,
                'unit':'knob' if name in SCALED_AMP else 'device-text','semantics':'captured' if self.name=='replay' else 'device_read'}

    def read_state(self,operations=(),validate=True,include_geometry=False):
        if validate:self.check_operations(operations)
        messages={};values={}
        for name in ('preset_name','scene','scene_name','tempo','status'):
            messages[name],values[name]=self.query_public(name)
        require(len(values['preset_name'])==34 and len(values['scene'])==1 and len(values['scene_name'])==33 and
                len(values['tempo'])==2 and len(values['status'])%3==0,'INVALID_REPLY','Unexpected public payload length')
        active=values['scene'][0]+1
        require(1<=active<=8,'INVALID_REPLY','Scene outside supported range')
        require(values['scene_name'][0]+1==active,'CONTEXT_CHANGED','Scene name and selected scene do not match')
        p={'slot':integer(values['preset_name'][:2]),'name':values['preset_name'][2:].split(b'\0',1)[0].decode('ascii','replace').rstrip(),
           'geometry':None,'blocks':[],'connections':None,'scenes':None,'active_scene':active,
           'active_scene_name':values['scene_name'][1:].split(b'\0',1)[0].decode('ascii','replace').rstrip(),
           'modifiers':None,'controllers':None,'unsaved':None,'complete':False,'complete_geometry':False,'tempo':integer(values['tempo'])}
        data=values['status']
        for i in range(0,len(data),3):
            ident=integer(data[i:i+2]);bits=data[i+2];ch=(bits>>1)&7;count=(bits>>4)&7
            try:t,inst=type_for_instance(ident);alias=t['alias'];ref=instance_ref(t,inst);tid=t['id']
            except AxeError:alias='unknown';ref='block-'+str(ident);tid=None
            require(ch<max(count,1) and count<=4,'INVALID_REPLY','Unexpected block channel status')
            p['blocks'].append({'ref':ref,'instance_id':ident,'type_id':tid,'type':alias,'position':None,
                                'active_channel':'ABCD'[ch],'bypass':bool(bits&1),
                                'channels':{c:{'parameters':{}} for c in 'ABCD'[:max(count,1)]}})
        grid=self.read_geometry() if include_geometry else None
        if grid is not None:attach_grid(p,grid)
        for op in operations:
            b=block(p,op['block']);channel=op.get('channel') or b['active_channel']
            require(channel==b['active_channel'],'PROTOCOL_UNRESOLVED','Inactive-channel getters/setters are unresolved over USB')
            b['channels'][channel]['parameters'][op['parameter']]=self.read_parameter(b,op['parameter'])
        # Compare decoded geometry, ignoring the editor's changing UI cursor.
        if grid is not None:
            require(self.read_geometry()==grid,'CONTEXT_CHANGED','Grid changed while capturing; retry inspection')
        # Guard against selection/channel changes while sampling the snapshot.
        for name in ('preset_name','scene','status'):
            again,_=self.query_public(name)
            require(again==messages[name],'CONTEXT_CHANGED','Device state changed while capturing; retry inspection')
        return {'device':{'id':'axe-fx-iii','model':'Axe-Fx III','firmware':None,'endpoints':self.identity,
                          'simulated':False,'captured':self.name=='replay'},'current':p,'presets':None,'generation':0}

    def simulate(self,state,operations,snapshots):
        predicted=copy.deepcopy(state);notes=[]
        for op in operations:
            b=block(predicted['current'],op['block']);channel=op.get('channel') or b['active_channel'];finite(op['value'])
            raw=op['value'] if op.get('raw',False) else op['value']/10
            require(0<=raw<=1,'INVALID_VALUE','Amp knob must be 0–10 (raw 0–1)')
            raw=struct.unpack('<f',struct.pack('<f',raw))[0]
            b['channels'][channel]['parameters'][op['parameter']].update(value=raw*10,raw_value=raw,raw_bits=struct.unpack('<I',struct.pack('<f',raw))[0])
            notes.append({'block':b['ref'],'parameter':op['parameter'],'warning':'Setter hardware-unverified; modifier assignment and firmware identity are unresolved. Readback/restoration will be checked.'})
        return predicted,notes

    def apply(self,operation,snapshots):
        current=self.read_state(self.guard_operations or [operation]);b=block(current['current'],operation['block'])
        if self.guard_state is not None:
            require(self.equivalent(self.guard_state,current),'CONTEXT_CHANGED','State changed before the next write; stopped')
        expected,_=self.simulate(current,[operation],snapshots)
        value=block(expected['current'],b['ref'])['channels'][b['active_channel']]['parameters'][operation['parameter']]
        self.connection().send(editor_frame(9,b['instance_id'],parameter(effect_type('amp'),operation['parameter'])['id'],value_bits=value['raw_bits']))
        actual=self.read_parameter(b,operation['parameter'])
        require(abs(actual['raw_value']-value['raw_value'])<=1e-6,'READBACK_MISMATCH','Parameter did not match requested raw value',expected=value,actual=actual)

    def equivalent(self,expected,actual):
        a,b=copy.deepcopy(expected),copy.deepcopy(actual)
        for state in (a,b):
            for effect in state['current']['blocks']:
                for channel in effect['channels'].values():
                    for value in channel['parameters'].values():
                        value.pop('display',None);value.pop('value',None)
        return a==b

    def restore(self,original,operations):
        self.guard_state=None;self.guard_operations=()
        actual=self.read_state(operations)
        def context(s):
            p=s['current'];return s['device'],p['slot'],p['name'],p['active_scene'],[(b['instance_id'],b['active_channel'],b['bypass']) for b in p['blocks']]
        require(context(original)==context(actual),'CONTEXT_CHANGED','Restoration refused after device/preset/channel context changed')
        unique={(op['block'],op['parameter']) for op in operations}
        for ref,name in sorted(unique):
            b=block(original['current'],ref);ch=b['active_channel'];raw=b['channels'][ch]['parameters'][name]['raw_value']
            self.apply({'op':'parameter.set','block':ref,'parameter':name,'value':raw,'raw':True,'channel':ch},{})

    def close(self):
        if self.transport:self.transport.close()


class ReplayBackend(USBBackend):
    name='replay'
    def __init__(self,store,path):
        super().__init__(store,transport=ReplayTransport(path))
        import hashlib
        self.identity={'backend':'replay','capture':str(Path(path).resolve()),'capture_sha256':hashlib.sha256(Path(path).read_bytes()).hexdigest()}

    def devices(self):return [{'source':'replay file','capture':self.identity['capture']}]

    def capabilities(self):
        result=super().capabilities();result.update(writes_enabled=False,edit=[],verification='file_replay',hardware_verified=False)
        if grid_query() not in self.transport.replies:
            result['read']=[r for r in result['read'] if r not in ('geometry','connections')]
            result['unresolved']+=['geometry','connections']
        return result

    def apply(self,operation,snapshots):
        raise AxeError('READ_ONLY_BACKEND','Replay cannot apply edits; use the stateful mock for editing tests')
