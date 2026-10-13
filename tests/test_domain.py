import copy

import pytest

from axefx_cli.backends import MockBackend
from axefx_cli.engine import Engine
from axefx_cli.errors import AxeError
from axefx_cli.model import block, inspect
from axefx_cli.storage import Store, atomic_write, load


@pytest.fixture
def system(tmp_path):
    store=Store(tmp_path);backend=MockBackend(store)
    return store,backend,Engine(backend,store)


def apply(engine,operations):
    return engine.apply(engine.plan(operations)['id'],True)


def test_preview_and_approval_never_mutate(system):
    store,backend,engine=system;original=backend.read_state()
    plan=engine.plan([{'op':'parameter.set','block':'amp1','parameter':'gain','value':7}])
    assert backend.read_state()==original and not backend.path.exists()
    with pytest.raises(AxeError,match='approval'):engine.apply(plan['id'])
    assert backend.read_state()==original and not (store.root/'journals').exists()
    result=engine.apply(plan['id'],True)
    assert result['status']=='applied'
    assert block(backend.read_state()['current'],'amp1')['channels']['A']['parameters']['gain']['value']==7
    with pytest.raises(AxeError) as error:engine.apply(plan['id'],True)
    assert error.value.code=='PLAN_ALREADY_ATTEMPTED'


def test_immutable_and_stale_plans(system):
    store,backend,engine=system
    plan=engine.plan([{'op':'preset.rename','name':'Proposed'}])
    apply(engine,[{'op':'preset.rename','name':'Changed elsewhere'}])
    with pytest.raises(AxeError) as error:engine.apply(plan['id'],True)
    assert error.value.code=='STALE_PLAN'
    artifact=load(plan['path']);artifact['operations'][0]['name']='Tampered';atomic_write(plan['path'],artifact)
    with pytest.raises(AxeError) as error:engine.apply(plan['id'],True)
    assert error.value.code=='ARTIFACT_CHANGED'


def test_empty_authoring_shunts_and_save_preserve_source(system):
    store,backend,engine=system;source=copy.deepcopy(backend.read_state()['presets']['0'])
    operations=[{'op':'preset.new','name':'Ambient'},
                {'op':'block.add','type':'input','row':2,'column':1},
                {'op':'block.add','type':'amp','row':2,'column':3},
                {'op':'block.add','type':'cab','row':2,'column':4},
                {'op':'block.add','type':'delay','row':2,'column':6},
                {'op':'block.add','type':'reverb','row':2,'column':7},
                {'op':'block.add','type':'output','row':2,'column':9},
                {'op':'block.connect','from':'input1','to':'amp1'},
                {'op':'block.connect','from':'amp1','to':'cab1'},
                {'op':'block.connect','from':'cab1','to':'delay1'},
                {'op':'block.connect','from':'delay1','to':'reverb1'},
                {'op':'block.connect','from':'reverb1','to':'output1'}]
    result=apply(engine,operations)
    p=inspect(result['current']);assert p['geometry']=={'rows':6,'columns':14}
    assert len(p['blocks'])==9 and len(p['connections'])==8
    assert p['cells'][1][1]=='shunt-r2c2'
    assert block(p,'amp1')['incoming']==['shunt-r2c2']
    assert backend.read_state()['presets']['0']==source
    apply(engine,[{'op':'preset.save','slot':1}])
    assert backend.read_state()['presets']['0']==source
    assert backend.read_state()['presets']['1']['name']=='Ambient'
    assert backend.read_state()['current']['unsaved'] is False


def test_connect_idempotent_and_branch_disconnect(system):
    _,backend,engine=system
    old=backend.read_state()
    plan=engine.plan([{'op':'block.connect','from':'amp1','to':'cab1'}])
    assert plan['notes'][0]['no_op']
    engine.apply(plan['id'],True);assert backend.read_state()==old
    apply(engine,[{'op':'block.add','type':'delay','row':2,'column':5},
                  {'op':'block.connect','from':'amp1','to':'delay1'}])
    p=backend.read_state()['current']
    assert {'from':'amp1','to':'cab1'} in p['connections']
    assert any(e['from']=='amp1' and e['to']=='shunt-r1c3' for e in p['connections']) is False
    route_plan=engine.plan([{'op':'block.disconnect','from':'amp1','to':'delay1'}])
    engine.apply(route_plan['id'],True)
    assert {'from':'amp1','to':'cab1'} in backend.read_state()['current']['connections']


def test_collision_unavailable_invalid_values_and_unknown_fields(system):
    _,backend,engine=system;old=backend.read_state()
    cases=[({'op':'block.add','type':'delay','row':1,'column':2},'CELL_OCCUPIED'),
           ({'op':'block.move','block':'amp1','row':1,'column':3},'CELL_OCCUPIED'),
           ({'op':'block.add','type':'amp','ref':'amp1','row':2,'column':2},'INSTANCE_UNAVAILABLE'),
           ({'op':'parameter.set','block':'amp1','parameter':'gain','value':11},'INVALID_VALUE'),
           ({'op':'parameter.set','block':'amp1','parameter':'gain','value':float('nan')},'INVALID_VALUE'),
           ({'op':'preset.rename','name':'Safe','sysex':'f0'},'INVALID_OPERATION'),
           ({'op':'raw.send','bytes':[240]},'INVALID_OPERATION'),
           ({'op':'block.connect','from':'cab1','to':'amp1'},'INVALID_CONNECTION')]
    for op,code in cases:
        with pytest.raises(AxeError) as error:engine.plan([op])
        assert error.value.code==code
    assert backend.read_state()==old


def test_scene_channel_modifier_and_controller_semantics(system):
    _,backend,engine=system
    apply(engine,[{'op':'parameter.set','block':'amp1','parameter':'gain','value':2,'channel':'B'},
                  {'op':'scene.set','scene':2,'name':'Clean','channels':{'amp1':'B'},'bypass':{'cab1':True}},
                  {'op':'scene.select','scene':2},
                  {'op':'controller.set','controller':'lfo1','settings':{'rate':2,'depth':.8,'shape':'sine'}},
                  {'op':'controller.set','controller':'scene-controller1','settings':{'values':[.1]*8}},
                  {'op':'modifier.set','modifier':'modifier1','block':'amp1','parameter':'gain','source':'external1','settings':{'minimum':0,'maximum':1}}])
    p=backend.read_state()['current'];amp=block(p,'amp1')
    assert amp['active_channel']=='B' and amp['channels']['A']['parameters']['gain']['value']==5
    assert amp['channels']['B']['parameters']['gain']['value']==2
    assert block(p,'cab1')['bypass'] is True
    assert p['scenes'][1]['controllers']['scene-controller1']==.1
    with pytest.raises(AxeError) as error:engine.plan([{'op':'parameter.set','block':'amp1','parameter':'gain','value':3}])
    assert error.value.code=='MODIFIER_ACTIVE'
    apply(engine,[{'op':'modifier.remove','modifier':'modifier1'},
                  {'op':'channel.copy','block':'amp1','from':'B','to':'C'},
                  {'op':'scene.copy','from':2,'to':3}])
    assert block(backend.read_state()['current'],'amp1')['channels']['C']['parameters']['gain']['value']==2


def test_authored_models_and_effect_parameters_preserve_preview_boundary(system):
    _,backend,engine=system
    original=backend.read_state()
    operations=[{'op':'parameter.set','block':'amp1','parameter':'model','value':'Mock Clean'},
                {'op':'block.add','type':'delay','row':2,'column':5},
                {'op':'parameter.set','block':'delay1','parameter':'model','value':'Mock Stereo'},
                {'op':'parameter.set','block':'delay1','parameter':'time','value':.45},
                {'op':'parameter.set','block':'delay1','parameter':'mix','value':.25}]
    plan=engine.plan(operations)
    assert backend.read_state()==original
    engine.apply(plan['id'],True)
    delay=block(backend.read_state()['current'],'delay1')
    assert delay['channels']['A']['parameters']['model']['value']=='Mock Stereo'
    assert delay['channels']['A']['parameters']['time']['unit']=='mock'
    with pytest.raises(AxeError) as error:
        engine.plan([{'op':'parameter.set','block':'amp1','parameter':'model','value':'Unlisted Model'}])
    assert error.value.code=='INVALID_VALUE'


def test_snapshot_restore_copy_and_overwrite(system):
    store,backend,engine=system
    original=backend.read_state();snap=engine.snapshot()
    apply(engine,[{'op':'preset.copy','source':'0','name':'Clone'},
                  {'op':'preset.rename','name':'Changed'}])
    with pytest.raises(AxeError) as error:engine.plan([{'op':'preset.save','slot':0}])
    assert error.value.code=='SLOT_OCCUPIED'
    plan=engine.plan([{'op':'preset.save','slot':0,'overwrite':True}])
    assert plan['destination_backup']==original['presets']['0']
    engine.apply(plan['id'],True)
    restore=engine.plan([{'op':'snapshot.restore','snapshot_id':snap['id']}])
    engine.apply(restore['id'],True)
    assert backend.read_state()['current']['name']==original['current']['name']
    assert backend.read_state()['presets']['0']['name']=='Changed'
    assert backend.read_state()['current']['unsaved'] is True
    apply(engine,[{'op':'preset.save','slot':0,'overwrite':True}])
    assert backend.equivalent(original,backend.read_state())
    with pytest.raises(AxeError) as error:engine.plan([{'op':'preset.rename','name':'X'},{'op':'preset.save','slot':2}])
    assert error.value.code=='SAVE_SEPARATE'


def test_failure_after_mutation_rolls_back_and_retains_journal(system,monkeypatch):
    store,backend,engine=system;original=backend.read_state()
    real=backend.apply;count=0
    def fail_second(op,snapshots):
        nonlocal count
        real(op,snapshots);count+=1
        if count==2:raise AxeError('DISCONNECT','Lost acknowledgement after changing state')
    monkeypatch.setattr(backend,'apply',fail_second)
    plan=engine.plan([{'op':'preset.rename','name':'First'},{'op':'parameter.set','block':'amp1','parameter':'gain','value':8}])
    with pytest.raises(AxeError) as error:engine.apply(plan['id'],True)
    assert error.value.code=='APPLY_FAILED' and error.value.details['restore_status']=='verified'
    assert backend.equivalent(original,backend.read_state())
    journal=load(error.value.details['journal']);assert journal['completed']==[0] and journal['attempted']==1
    assert store.get('snapshots',journal['snapshot']['id'])['state']==original


def test_restore_failure_reported_without_false_success(system,monkeypatch):
    _,backend,engine=system
    def fail(*args):raise AxeError('DISCONNECT','Disconnected')
    monkeypatch.setattr(backend,'apply',fail);monkeypatch.setattr(backend,'restore',fail)
    plan=engine.plan([{'op':'preset.rename','name':'X'}])
    with pytest.raises(AxeError) as error:engine.apply(plan['id'],True)
    assert error.value.details['restore_status']=='failed_or_unresolved'


def test_restore_preserves_later_saves_and_explicit_destination_backup(system):
    _,backend,engine=system
    apply(engine,[{'op':'preset.rename','name':'Before overwrite'}])
    save=apply(engine,[{'op':'preset.save','slot':0,'overwrite':True}])
    apply(engine,[{'op':'preset.rename','name':'Later'}])
    apply(engine,[{'op':'preset.save','slot':2}])
    before=copy.deepcopy(backend.read_state()['presets'])
    restored=apply(engine,[{'op':'snapshot.restore','snapshot_id':save['snapshot']['id'],'source_slot':0}])
    assert restored['current']['name']=='Mock Clean'
    assert backend.read_state()['presets']==before
    apply(engine,[{'op':'preset.save','slot':0,'overwrite':True}])
    assert backend.read_state()['presets']['0']['name']=='Mock Clean'
    assert backend.read_state()['presets']['2']['name']=='Later'
