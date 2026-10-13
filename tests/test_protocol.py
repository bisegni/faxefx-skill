import json
import queue
import struct
from pathlib import Path

import pytest

from axefx_cli.backends import ReplayBackend, ReplayTransport, USBBackend, USBTransport, validate_usb_packet
from axefx_cli.codec import decode_editor, editor_frame, frame, integer, pack_bytes, public_queries, unframe, unpack_bytes
from axefx_cli.engine import Engine
from axefx_cli.errors import AxeError
from axefx_cli.storage import Store

CAPTURE=Path(__file__).parent/'fixtures/synthetic-traffic.jsonl'


def test_synthetic_replay_checksums_and_exact_gain_and_level_decode():
    records=[json.loads(line) for line in CAPTURE.read_text().splitlines()]
    assert records[0]['kind']=='synthetic-fixture'
    events=[r for r in records if r.get('direction') in ('tx','rx')]
    assert len(events)==42
    for event in events:unframe(bytes.fromhex(event['hex']))
    replay=ReplayTransport(CAPTURE)
    assert public_queries()['status'].hex()=='f0000174101306f7'
    reply=replay.query(editor_frame(26,58,1))
    decoded=decode_editor(reply)
    assert decoded['value_bits']==0x3f000000
    assert bytes.fromhex(decoded['data_hex']).split(b'\0',1)[0]==b'5.00'
    decoded=decode_editor(replay.query(editor_frame(26,58,21)))
    assert bytes.fromhex(decoded['data_hex']).split(b'\0',1)[0]==b'-12.0 dB'
    assert len(bytes.fromhex(decoded['data_hex']))==32
    assert any(bytes.fromhex(decoded['data_hex'])[9:])  # Preserve nonzero bytes after the terminator.


@pytest.mark.parametrize('length',[0,1,2,6,7,8,15,127,255,2048])
def test_binary_packing_and_malformed_frames(length):
    data=bytes((i*97+31)%256 for i in range(length))
    assert unpack_bytes(pack_bytes(data),length)==data
    packet=bytearray(editor_frame(26,58,1,data=data));packet[-2]^=1
    with pytest.raises(ValueError):unframe(bytes(packet))


def test_replay_reports_partial_geometry_and_never_invents_writes(tmp_path):
    store=Store(tmp_path);backend=ReplayBackend(store,CAPTURE)
    state=backend.read_state()
    assert state['current']['name']=='Synthetic Clean' and state['current']['active_scene']==2
    assert backend.capabilities()['verification']=='file_replay'
    assert state['current']['geometry'] is None and state['current']['connections'] is None
    assert state['current']['modifiers'] is None and state['current']['complete'] is False
    assert backend.parameter_value('amp1','gain')['display']=='5.00'
    with pytest.raises(AxeError) as error:backend.transport.query(editor_frame(26,58,6))
    assert error.value.code=='REPLAY_MISS'
    with pytest.raises(AxeError):backend.transport.send(editor_frame(9,58,1))
    plan=Engine(backend,store).plan([{'op':'parameter.set','block':'amp1','parameter':'gain','value':6}])
    with pytest.raises(AxeError) as error:Engine(backend,store).apply(plan['id'],True)
    assert error.value.code=='READ_ONLY_BACKEND'


class WireDevice:
    """Independent numeric device model. Parses sent field bytes, not domain ops.

    Public replies come from the authored synthetic fixture. Numeric read/write behavior is
    simulated only and must not be reported as hardware verification.
    """
    def __init__(self):
        self.capture=ReplayTransport(CAPTURE);self.bits={1:0x3f000000};self.writes=[]
        self.context_changed=False;self.fail_readback=False

    def query(self,packet):
        command,p=unframe(packet)
        if command!=1:
            if self.context_changed and command==0x0c:return frame(0x0c,b'\x02')
            if self.context_changed and command==0x0e:
                _,payload=unframe(self.capture.query(packet));return frame(command,b'\x02'+payload[1:])
            return self.capture.query(packet)
        operation=integer(p[:2]);effect=integer(p[2:4]);param=integer(p[4:6])
        assert operation==26 and effect==58
        if param not in self.bits:return self.capture.query(packet)
        bits=self.bits[param]
        if self.fail_readback and self.writes:
            self.fail_readback=False;bits=0x3f000000
        value=struct.unpack('<f',struct.pack('<I',bits))[0]
        text=f'{value*10:.2f}'.encode().ljust(32,b'\0')
        return editor_frame(26,58,param,value_bits=bits,data=text)

    def send(self,packet):
        command,p=unframe(packet)
        assert command==1 and integer(p[:2])==9 and integer(p[2:4])==58
        assert p[11:15]==b'\0\0\0\0'
        assert integer(p[4:6])==1
        self.writes.append(packet);self.bits[1]=integer(p[6:11])

    def close(self):pass


def test_approved_usb_adapter_compiles_numeric_setter_without_hardware(tmp_path):
    store=Store(tmp_path);wire=WireDevice();backend=USBBackend(store,transport=wire);engine=Engine(backend,store)
    plan=engine.plan([{'op':'parameter.set','block':'amp1','parameter':'gain','value':6}])
    assert wire.writes==[] and backend.capabilities()['writes_enabled'] is True
    with pytest.raises(AxeError):engine.apply(plan['id'])
    assert wire.writes==[]
    result=engine.apply(plan['id'],True)
    assert result['status']=='applied' and len(wire.writes)==1
    d=decode_editor(wire.writes[0])
    assert d['operation']==9 and d['effect']==58 and d['parameter']==1 and d['value_bits']==0x3f19999a
    snapshot=store.get('snapshots',result['snapshot']['id'])
    assert snapshot['scope']=='parameters'
    restore=engine.plan([{'op':'snapshot.restore','snapshot_id':result['snapshot']['id']}])
    engine.apply(restore['id'],True)
    assert wire.bits[1]==0x3f000000


def test_usb_mismatch_restores_original_numeric_value(tmp_path):
    store=Store(tmp_path);wire=WireDevice();backend=USBBackend(store,transport=wire);engine=Engine(backend,store)
    plan=engine.plan([{'op':'parameter.set','block':'amp1','parameter':'gain','value':6}])
    wire.fail_readback=True
    with pytest.raises(AxeError) as error:engine.apply(plan['id'],True)
    assert error.value.details['restore_status']=='verified'
    assert wire.bits[1]==0x3f000000 and len(wire.writes)==2


def test_usb_context_change_after_preview_sends_no_writes(tmp_path):
    store=Store(tmp_path);wire=WireDevice();backend=USBBackend(store,transport=wire);engine=Engine(backend,store)
    plan=engine.plan([{'op':'parameter.set','block':'amp1','parameter':'gain','value':6}])
    wire.context_changed=True
    with pytest.raises(AxeError) as error:engine.apply(plan['id'],True)
    assert error.value.code=='STALE_PLAN' and wire.writes==[]


def test_unknown_structural_operation_and_snapshot_are_blocked_before_usb(tmp_path):
    class NoTraffic:
        def query(self,*args):raise AssertionError('No USB allowed')
        def send(self,*args):raise AssertionError('No USB allowed')
    store=Store(tmp_path);engine=Engine(USBBackend(store,transport=NoTraffic()),store)
    with pytest.raises(AxeError) as error:engine.plan([{'op':'block.connect','from':'amp1','to':'cab1'}])
    assert error.value.code=='PROTOCOL_UNRESOLVED'
    with pytest.raises(AxeError) as error:engine.snapshot()
    assert error.value.code=='PROTOCOL_UNRESOLVED'


def transport_without_hardware(monkeypatch,reply=None):
    t=USBTransport.__new__(USBTransport)
    t.log=None;t.events=queue.Queue();t.pending=bytearray();t.timeout=.01
    class Output:
        def send_message(self,packet):
            if reply is not None:
                t.receive((list(reply[:4]),0),None);t.receive((list(reply[4:]),0),None)
    t.output=Output();monkeypatch.setattr('axefx_cli.backends.time.sleep',lambda _:None)
    return t


def test_transport_fragment_reassembly_and_realtime_interruption(monkeypatch):
    response=frame(0x0c,b'\x01')
    t=transport_without_hardware(monkeypatch,response[:6]+b'\xf8'+response[6:])
    assert t.query(public_queries()['scene'])==response


def test_transport_bad_checksum_and_timeout(monkeypatch):
    response=bytearray(frame(0x0c,b'\x01'));response[-2]^=1
    t=transport_without_hardware(monkeypatch,response)
    with pytest.raises(AxeError) as error:t.query(public_queries()['scene'])
    assert error.value.code=='INVALID_REPLY'
    t=transport_without_hardware(monkeypatch)
    with pytest.raises(AxeError) as error:t.query(public_queries()['scene'])
    assert error.value.code=='REPLY_TIMEOUT'


def test_wire_boundary_rejects_unknown_writes_and_transfer_frames():
    for packet in public_queries().values():validate_usb_packet(packet)
    validate_usb_packet(editor_frame(26,58,1))
    validate_usb_packet(editor_frame(9,58,1,value_bits=0x3f000000))
    for packet in (frame(0x77),frame(0x0c,b'\x00'),editor_frame(54,58),
                   editor_frame(9,58,0),editor_frame(9,58,1,value_bits=0x7fc00000),
                   editor_frame(9,58,1,value_bits=0x40000000),editor_frame(26,58,1,data=b'unknown')):
        with pytest.raises(AxeError) as error:validate_usb_packet(packet)
        assert error.value.code=='UNSUPPORTED_PACKET'


def test_receive_overflow_never_blocks_callback(monkeypatch):
    t=transport_without_hardware(monkeypatch);t.events=queue.Queue(maxsize=1)
    t.enqueue(frame(0x0c,b'\x01'));t.enqueue(frame(0x0c,b'\x02'))
    assert t.receive_error.code=='RECEIVE_OVERFLOW' and t.events.qsize()==1
