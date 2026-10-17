import copy
import json
import xml.etree.ElementTree as ET

import pytest

from axefx_cli.backends import MockBackend
from axefx_cli.design import Designs, compile_design, from_capture, starter
from axefx_cli.engine import Engine
from axefx_cli.errors import AxeError
from axefx_cli.render import svg
from axefx_cli.storage import Store


def test_revisions_conflict_and_immutable_history(tmp_path):
    designs = Designs(Store(tmp_path)); original = designs.create(starter('Original'))
    changed = copy.deepcopy(original['design']); changed['intent'] = 'More space'
    revised = designs.revise(original['project'], original['revision'], changed)
    assert revised['revision'] != original['revision']
    assert designs.get(original['project'], original['revision'])['design']['intent'] == ''
    with pytest.raises(AxeError, match='stale'):
        designs.revise(original['project'], original['revision'], changed)
    with pytest.raises(AxeError):
        designs.get('../outside')


def test_parallel_design_compiles_and_applies_with_scenes_and_modifier(tmp_path):
    store = Store(tmp_path); backend = MockBackend(store); d = starter('Parallel Clean')
    d['nodes'][1].update(parameters={'gain': 3}, model='Mock Clean', channels={'B': {'gain': 7}})
    d['nodes'].append({'id': 'space', 'type': 'delay', 'parameters': {'mix': 1, 'time': .35}})
    d['edges'] += [{'from': 'cab', 'to': 'space'}, {'from': 'space', 'to': 'output'}]
    d['scenes'] = [{'number': 2, 'name': 'Lead', 'channels': {'amp': 'B'}, 'bypass': {'space': False}}]
    d['controllers'] = {'manual1': {'value': .4}}
    d['modifiers'] = [{'ref': 'modifier1', 'block': 'amp', 'parameter': 'gain', 'source': 'manual1', 'settings': {'minimum': .2, 'maximum': .8}}]
    compiled = compile_design(d, 'mock')
    assert compiled['buildable'], compiled['blockers']
    assert len(compiled['physical']['blocks']) > len(d['nodes'])  # dry path crosses the delay column via a real shunt
    assert compiled['physical']['modifiers'][0]['block'] == 'amp1'
    assert not (store.root / 'mock-device.json').exists()
    engine = Engine(backend, store); plan = engine.plan(compiled['operations'])
    applied = engine.apply(plan['id'], approved=True)
    assert applied['current']['name'] == 'Parallel Clean'
    assert applied['current']['scenes'][1]['channels']['amp1'] == 'B'
    assert backend.read_state()['presets']['0']['name'] == 'Mock Clean'


def test_usb_compile_never_substitutes_mock_catalog_or_creates_operations():
    d = starter('Hardware Clean'); d['nodes'][1]['model'] = 'Mock Clean'
    c = compile_design(d, 'usb')
    assert not c['buildable'] and not c['operations'] and c['physical'] is None
    assert c['blockers'][0]['code'] == 'PROTOCOL_UNRESOLVED'
    assert c['logical'] == d and c['logical_positions']


@pytest.mark.parametrize('change,expected', [
    (lambda d: d['edges'].append({'from': 'output', 'to': 'amp'}), 'CYCLIC_DESIGN'),
    (lambda d: d['edges'].pop(), 'DISCONNECTED_DESIGN'),
    (lambda d: d['nodes'][1].update(parameters={'gain': 200}), 'INVALID_VALUE'),
    (lambda d: d['nodes'][1].update(model='Invented amp'), 'INVALID_VALUE'),
])
def test_unbuildable_design_retains_intent_and_explains_blockers(change, expected):
    d = starter(); change(d); before = copy.deepcopy(d)
    c = compile_design(d, 'mock')
    assert c['blockers'][0]['code'] == expected
    assert not c['buildable'] and c['alternatives'] and d == before


def test_changed_design_revokes_executable_preview(tmp_path):
    store = Store(tmp_path); backend = MockBackend(store); designs = Designs(store)
    project = designs.create(starter()); compiled = compile_design(project['design'], 'mock')
    plan = Engine(backend, store).plan(compiled['operations'], provenance={'project': project['project'], 'revision': project['revision']})
    changed = project['design']; changed['intent'] = 'Different tone'
    designs.revise(project['project'], project['revision'], changed)
    with pytest.raises(AxeError, match='Design changed'):
        Engine(backend, store).apply(plan['id'], approved=True)
    assert not (store.root / 'mock-device.json').exists()


def test_captured_refinement_preserves_unrelated_mock_data(tmp_path):
    store = Store(tmp_path); backend = MockBackend(store); state = backend.read_state()
    d = from_capture(state); d['nodes'][1]['channels']['A']['gain'] = 4
    compiled = compile_design(d, 'mock')
    assert compiled['buildable'], compiled['blockers']
    assert compiled['physical']['blocks'][1]['channels']['A']['parameters']['gain']['value'] == 4
    assert compiled['physical']['blocks'][1]['channels']['A']['parameters']['master']['value'] == 10
    plan = Engine(backend, store).plan(compiled['operations']); Engine(backend, store).apply(plan['id'], approved=True)
    assert backend.read_state()['presets']['0'] == state['presets']['0']


def test_svg_is_valid_escaped_and_keeps_bypass_and_edges():
    d = starter('A & B'); d['nodes'][1].update(label='<Amp>', bypass=True)
    image = svg(d); root = ET.fromstring(image)
    assert root.attrib['aria-label'] == 'A & B'
    assert '&lt;Amp&gt;' in image and 'bypassed' in image
    assert image.count('marker-end="url(#arrow)"') == 3
    assert svg(d) == image
    card = svg(d, block_id='amp')
    ET.fromstring(card); assert '&lt;Amp&gt;' in card
    d['nodes'][1]['label'] = '&' * 100
    ET.fromstring(svg(d))


def test_png_export_uses_the_same_graph_without_native_libraries(tmp_path):
    image = pytest.importorskip('PIL.Image')
    from axefx_cli.render import render_file
    path = tmp_path / 'graph.png'
    result = render_file(starter(), path)
    with image.open(path) as drawing:
        assert drawing.width >= 900 and drawing.height >= 700
    assert result['format'] == 'png'


def test_full_design_operation_budget_does_not_truncate(tmp_path):
    store = Store(tmp_path); engine = Engine(MockBackend(store), store)
    operations = [{'op': 'parameter.set', 'block': 'amp1', 'parameter': 'gain', 'value': i % 10} for i in range(300)]
    plan = engine.plan(operations)
    assert len(plan['operations']) == 300


def test_scene_baselines_match_logical_and_compiled_views():
    d = starter(); d['nodes'][1]['bypass'] = True; d['nodes'][1]['active_channel'] = 'B'
    d['scenes'] = [{'number': 2, 'channels': {'amp': 'C'}, 'bypass': {'amp': False}}]
    compiled = compile_design(d, 'mock'); assert compiled['buildable']
    assert compiled['physical']['scenes'][2]['bypass']['amp1'] is True
    assert compiled['physical']['scenes'][2]['channels']['amp1'] == 'B'
    from axefx_cli.render import graph
    logical = next(n for n in graph(d, scene_number=2)[1] if n['id'] == 'amp')
    physical = next(n for n in graph(compiled, physical=True, scene_number=2)[1] if n['id'] == 'amp1')
    assert logical['bypass'] == physical['bypass'] == False
    assert logical['active_channel'] == physical['active_channel'] == 'C'


def test_parallel_branch_leaves_the_direct_dry_lane_visible():
    from axefx_cli.design import layout
    d = starter(); d['nodes'].append({'id': 'delay', 'type': 'delay'})
    d['edges'] += [{'from': 'cab', 'to': 'delay'}, {'from': 'delay', 'to': 'output'}]
    positions = layout(d)
    assert positions['delay']['row'] != positions['cab']['row']
    assert positions['output']['row'] == positions['cab']['row']


def test_refinement_moves_preserve_channel_contents_and_rewire(tmp_path):
    store = Store(tmp_path); backend = MockBackend(store); d = from_capture(backend.read_state())
    # Move the whole chain to another row without replacing blocks.
    for n in d['nodes']:
        n['position']['row'] = 4
    compiled = compile_design(d, 'mock')
    assert compiled['buildable'], compiled['blockers']
    assert sum(op['op'] == 'block.move' for op in compiled['operations']) == 4
    plan = Engine(backend, store).plan(compiled['operations'])
    result = Engine(backend, store).apply(plan['id'], approved=True)
    assert all(b['position']['row'] == 4 for b in result['current']['blocks'])
    amp = next(b for b in result['current']['blocks'] if b['ref'] == 'amp1')
    assert amp['channels']['A']['parameters']['gain']['value'] == 5


def test_refinement_preserves_modifier_protection_until_explicit_removal(tmp_path):
    store = Store(tmp_path); backend = MockBackend(store); engine = Engine(backend, store)
    plan = engine.plan([{'op': 'modifier.set', 'modifier': 'modifier1', 'block': 'amp1',
                         'parameter': 'gain', 'source': 'manual1', 'settings': {'minimum': .2, 'maximum': .8}}])
    engine.apply(plan['id'], approved=True)
    d = from_capture(backend.read_state())
    compiled = compile_design(d, 'mock')
    assert compiled['buildable'], compiled['blockers']
    assert compiled['physical']['modifiers'][0]['ref'] == 'modifier1'
    d['nodes'][1]['channels']['A']['gain'] = 6
    assert compile_design(d, 'mock')['blockers'][0]['code'] == 'MODIFIER_ACTIVE'
    d['modifiers'] = []
    compiled = compile_design(d, 'mock')
    assert compiled['buildable'], compiled['blockers']
    assert not compiled['physical']['modifiers']
