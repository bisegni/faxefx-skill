import copy
import time

import pytest

from axefx_cli.chat import Chat, CONFIG, DISABLED, RPC
from axefx_cli.design import Designs, starter
from axefx_cli.errors import AxeError
from axefx_cli.storage import Store


class FakeRPC:
    created = []
    config = {'features': {**{k: False for k in DISABLED}, 'code_mode_host': True}, 'mcp_servers': {'private-server': {}}}

    def __init__(self, event, tool):
        self.event, self.tool = event, tool
        self.requests = []; self.closed = False
        self.created.append(self)

    def send(self, value):
        self.requests.append(value)

    def request(self, method, params, **_):
        self.requests.append((method, params))
        if method == 'initialize':
            return {'userAgent': 'codex/0.163.0'}
        if method == 'account/read':
            return {'requiresOpenaiAuth': True, 'account': {'type': 'chatgpt'}}
        if method == 'config/read':
            return {'config': copy.deepcopy(self.config)}
        if method in ('thread/start', 'thread/resume'):
            assert params['config']['mcp_servers.private-server.enabled'] is False
            assert params['environments'] == [] and params['sandbox'] == 'read-only'
            return {'thread': {'id': 'thread-1'}, 'model': 'configured-model', 'sandbox': {'type': 'readOnly'}}
        if method == 'turn/start':
            self.event('turn/started', {'threadId': 'thread-1', 'turn': {'id': 'turn-1'}})
            return {'turn': {'id': 'turn-1'}}
        if method == 'turn/interrupt':
            self.event('turn/completed', {'threadId': 'thread-1', 'turn': {'id': 'turn-1', 'status': 'interrupted'}})
            return {}
        raise AssertionError(method)

    def close(self):
        self.closed = True


def wait(chat, expected):
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        state = chat.snapshot()
        if state['status'] == expected:
            return state
        time.sleep(.01)
    raise AssertionError(chat.snapshot())


@pytest.fixture
def chat(tmp_path):
    store = Store(tmp_path); project = Designs(store).create(starter())
    c = Chat(store, project['project'], rpc_factory=FakeRPC)
    c.connect(); wait(c, 'ready')
    yield c, project
    c.close()


def call(chat, name, args=None, **overrides):
    return chat.tool({'threadId': 'thread-1', 'turnId': 'turn-1', 'tool': name, 'arguments': args or {}, **overrides})


def start(chat, project):
    chat.start('Please make the draft warmer', project['revision'])
    deadline = time.monotonic() + 3
    while chat.snapshot()['turn'] is None and time.monotonic() < deadline:
        time.sleep(.01)
    assert chat.snapshot()['turn'] == 'turn-1'


def complete(chat):
    chat.event('turn/completed', {'threadId': 'thread-1', 'turn': {'id': 'turn-1', 'status': 'completed'}})


def test_streaming_tools_proposal_review_and_resume(chat):
    c, p = chat; start(c, p)
    c.event('item/agentMessage/delta', {'threadId': 'thread-1', 'itemId': 'answer', 'delta': 'A warmer '})
    c.event('item/agentMessage/delta', {'threadId': 'thread-1', 'itemId': 'answer', 'delta': 'clean.'})
    assert c.snapshot()['messages'][-1]['text'] == 'A warmer clean.'
    assert call(c, 'tone_read')['success']
    assert call(c, 'tone_evidence')['success']
    assert call(c, 'tone_compile')['success']
    for name in ('exec_command', 'apply_patch', 'edit.apply', 'preset.save'):
        assert not call(c, name)['success']
    assert not call(c, 'tone_read', threadId='other')['success']
    assert not call(c, 'tone_read', turnId='old')['success']
    assert not call(c, 'tone_analysis', {'id': '0' * 64})['success']
    design = copy.deepcopy(p['design']); design['intent'] = 'Warmer clean'
    assert call(c, 'tone_propose', {'base': p['revision'], 'design': design, 'summary': 'A warmer brief'})['success']
    proposal = c.snapshot()['proposal']
    assert Designs(c.store).get(c.project)['revision'] == p['revision']
    complete(c)
    c.decide(proposal['id'], True, p['revision'])
    assert Designs(c.store).get(c.project)['design']['intent'] == 'Warmer clean'
    c.close()
    resumed = Chat(c.store, c.project, rpc_factory=FakeRPC)
    resumed.connect(); wait(resumed, 'ready')
    assert any(req[0] == 'thread/resume' for req in resumed.rpc.requests if isinstance(req, tuple))
    assert resumed.snapshot()['messages'][-2]['text'] == 'A warmer clean.'
    resumed.close()


def test_reject_stale_accept_capture_protection_and_interrupt(chat):
    c, p = chat; start(c, p)
    design = copy.deepcopy(p['design']); design['intent'] = 'Proposed'
    args = {'base': p['revision'], 'design': design, 'summary': 'Change the tone brief'}
    assert call(c, 'tone_propose', args)['success']
    proposal = c.snapshot()['proposal']; complete(c)
    other = copy.deepcopy(p['design']); other['intent'] = 'Manual edit'
    Designs(c.store).revise(c.project, p['revision'], other)
    with pytest.raises(AxeError):
        c.decide(proposal['id'], True, p['revision'])
    c.decide(proposal['id'], False, p['revision'])
    assert Designs(c.store).get(c.project)['design']['intent'] == 'Manual edit'
    current = Designs(c.store).get(c.project)
    start(c, current)
    invalid = copy.deepcopy(current['design']); invalid['target'] = {'invented': True}
    assert not call(c, 'tone_propose', {'base': current['revision'], 'design': invalid, 'summary': 'Bad evidence'})['success']
    c.interrupt(); assert c.snapshot()['status'] == 'ready'
    assert not call(c, 'tone_read')['success']


def test_unsafe_config_and_protocol_fail_closed(tmp_path):
    class Unsafe(FakeRPC):
        config = {'features': {}}
    store = Store(tmp_path); project = Designs(store).create(starter())
    c = Chat(store, project['project'], rpc_factory=Unsafe)
    c.connect(); state = wait(c, 'failed')
    assert 'restrictions' in state['error']
    assert c.rpc is None or c.rpc.closed
    with pytest.raises(AxeError):
        c.start('Edit something', project['revision'])
    c.close()
    class Old(FakeRPC):
        def request(self, method, params, **kwargs):
            return {'userAgent': 'codex/0.100.0'} if method == 'initialize' else super().request(method, params, **kwargs)
    c = Chat(store, project['project'], rpc_factory=Old)
    c.connect(); assert '0.163' in wait(c, 'failed')['error']; c.close()


def test_rpc_subprocess_framing_disconnect_and_denial(tmp_path):
    import sys
    events = []
    program = '''import sys,json
for line in sys.stdin:
 m=json.loads(line)
 if m.get('method')=='probe':
  print(json.dumps({'id':77,'method':'item/commandExecution/requestApproval','params':{}}),flush=True)
  print(json.dumps({'id':m['id'],'result':{'ok':True}}),flush=True)
 elif m.get('id')==77:
  print(json.dumps({'method':'denied','params':m}),flush=True)
'''
    rpc = RPC(lambda name, params: events.append((name, params)), lambda _: None,
              command=[sys.executable, '-u', '-c', program])
    assert rpc.request('probe', {}) == {'ok': True}
    deadline = time.monotonic() + 3
    while not events and time.monotonic() < deadline:
        time.sleep(.01)
    assert events[0][0] == 'denied' and events[0][1]['error']['code'] == -32601
    rpc.close()
    with pytest.raises(AxeError):
        rpc.request('anything', {}, timeout=.1)


def test_top_level_patch_and_missing_auth(chat, tmp_path):
    c, p = chat; start(c, p)
    assert call(c, 'tone_propose', {'base': p['revision'], 'changes': {'intent': 'Warmer'},
                                  'summary': 'Adjust the brief only'})['success']
    assert c.snapshot()['proposal']['changes'] == [{'path': '/intent', 'before': '', 'after': 'Warmer'}]
    assert not call(c, 'tone_propose', {'base': p['revision'], 'changes': {'captured': {}},
                                      'summary': 'Replace evidence'})['success']
    complete(c)
    class SignedOut(FakeRPC):
        def request(self, method, params, **kwargs):
            return {'requiresOpenaiAuth': True, 'account': None} if method == 'account/read' else super().request(method, params, **kwargs)
    other = Chat(c.store, c.project, rpc_factory=SignedOut)
    other.connect(); state = wait(other, 'failed')
    assert 'codex login' in state['error']; other.close()
