import json
import subprocess
import sys
from pathlib import Path

import pytest

from axefx_cli.backends import USBTransport
from axefx_cli.cli import main
from axefx_cli.storage import Store


def run(capsys,*arguments):
    code=main(list(arguments));result=json.loads(capsys.readouterr().out)
    return code,result


def test_discovery_does_not_touch_usb_or_require_extra(tmp_path,capsys,monkeypatch):
    def no_usb(*args,**kwargs):raise AssertionError('No hardware permitted')
    monkeypatch.setattr(USBTransport,'__init__',no_usb)
    monkeypatch.setattr(USBTransport,'enumerate_ports',no_usb)
    for command in (['schema'],['catalog'],['catalog','--type','amp','--parameter','gain'],['--state-dir',str(tmp_path),'capabilities']):
        code,result=run(capsys,*command)
        assert code==0 and result['ok']
    code,result=run(capsys,'--state-dir',str(tmp_path),'block','connect','amp1','cab1')
    assert code==2 and result['error']['code']=='PROTOCOL_UNRESOLVED'
    code,result=run(capsys,'--state-dir',str(tmp_path),'parameter','set','amp1','model','--value','Mock Clean')
    assert code==2 and result['error']['code']=='CONVERSION_UNRESOLVED'
    code,result=run(capsys,'--state-dir',str(tmp_path),'edit','apply','0'*64)
    assert result['error']['code']=='APPROVAL_REQUIRED'


def test_json_preview_apply_get_and_machine_errors(tmp_path,capsys):
    prefix=['--backend','mock','--state-dir',str(tmp_path)]
    code,result=run(capsys,*prefix,'parameter','set','amp1','gain','--value','6')
    assert code==0 and result['data']['approval_required']
    key=result['data']['id']
    _,before=run(capsys,*prefix,'parameter','get','amp1','gain')
    assert before['data']['current']['value']==5
    _,result=run(capsys,*prefix,'edit','apply',key,'--approved')
    assert result['data']['status']=='applied'
    _,after=run(capsys,*prefix,'parameter','get','amp1','gain')
    assert after['data']['current']['value']==6
    code,result=run(capsys,*prefix,'parameter','set','amp1','gain','--value','1e400')
    assert code==2 and result['error']['code']=='INVALID_VALUE'
    code,result=run(capsys,*prefix,'bogus')
    assert code==2 and result['error']['code']=='INVALID_ARGUMENTS'


def test_concurrent_process_lock(tmp_path):
    store=Store(tmp_path)
    with store.lock('mock-axefx3'):
        entrypoint=Path(__file__).resolve().parent.parent/'tools/axefx.py'
        result=subprocess.run([sys.executable,str(entrypoint),'--backend','mock','--state-dir',str(tmp_path),'preset','inspect'],capture_output=True,text=True)
    assert result.returncode==2 and json.loads(result.stdout)['error']['code']=='DEVICE_BUSY'


def test_lock_released_when_context_proxy_is_retained(tmp_path):
    store=Store(tmp_path);context=store.lock('same-device')
    with context:pass
    with store.lock('same-device'):pass


def test_unknown_capture_and_nan_proposal_fail_structurally(tmp_path,capsys):
    capture=tmp_path/'capture.jsonl';capture.write_text('{"direction":"rx","hex":"00"}\n')
    code,result=run(capsys,'--backend','replay','--state-dir',str(tmp_path),'--replay-file',str(capture),'preset','inspect')
    assert code==2 and result['error']['code']=='INVALID_CAPTURE'
    proposal=tmp_path/'bad.json';proposal.write_text('{"operations":[{"op":"parameter.set","block":"amp1","parameter":"gain","value":NaN}]}')
    code,result=run(capsys,'--backend','mock','--state-dir',str(tmp_path),'edit','plan','--file',str(proposal))
    assert code==2 and result['error']['code']=='INVALID_ARTIFACT'


def test_text_grid(tmp_path,capsys):
    assert main(['--backend','mock','--state-dir',str(tmp_path),'--format','text','preset','geometry'])==0
    text=capsys.readouterr().out
    assert 'amp1' in text and 'input1 -> amp1' in text and 'Simulated device' in text


def test_plugin_and_skill_metadata():
    root=Path(__file__).resolve().parent.parent
    manifest=json.loads((root/'plugins/axefx/.codex-plugin/plugin.json').read_text())
    assert manifest['name']=='axefx'
    assert (root/'plugins/axefx'/manifest['skills'].removeprefix('./')/'axefx-preset/SKILL.md').is_file()
    marketplace=json.loads((root/'.agents/plugins/marketplace.json').read_text())
    source=marketplace['plugins'][0]['source']['path'].removeprefix('./')
    assert (root/source/'.codex-plugin/plugin.json').is_file()


def test_design_and_protocol_discovery_never_open_usb(tmp_path, capsys, monkeypatch):
    import axefx_cli.backends as backends
    monkeypatch.setattr(backends.USBBackend, 'connection', lambda *_: (_ for _ in ()).throw(AssertionError('USB opened by local authoring')))
    prefix = ['--state-dir', str(tmp_path)]
    assert main(prefix + ['design', 'create', '--name', 'A draft']) == 0
    project = json.loads(capsys.readouterr().out)['data']['project']
    assert main(prefix + ['design', 'compile', project]) == 0
    compiled = json.loads(capsys.readouterr().out)['data']
    assert not compiled['buildable'] and compiled['backend'] == 'usb'
    assert compiled['blockers'][0]['code'] == 'PROTOCOL_UNRESOLVED'
    assert main(prefix + ['protocol', 'decode', '--hex', 'f0 00 01 74 10 10 f7']) == 0
    assert json.loads(capsys.readouterr().out)['data']['kind'] == 'tempo_push'


def test_imported_catalog_queries_do_not_bind_symbols_by_numeric_coincidence(tmp_path, capsys):
    from axefx_cli.storage import Store
    artifact = Store(tmp_path).put('editor-catalogs', {'editor': {'version': 'authored'},
        'definitions': {'firmware': {'major': 1, 'minor': 0}, 'effects': [
            {'type_id': 13, 'parameters': [{'id': 1, 'labels': ['Authored choice'], 'special_record': False, 'editable_semantics_verified': False}]}]},
        'symbols': {'ids': {'DISTORT_DRIVE': 1}}, 'warning': 'Unverified authored test'})
    prefix = ['--state-dir', str(tmp_path), 'catalog', 'inspect-import', '--id', artifact['id']]
    assert main(prefix) == 0
    assert len(json.loads(capsys.readouterr().out)['data']['types']) == 1
    assert main(prefix + ['--type', '13', '--parameter', '1']) == 0
    assert json.loads(capsys.readouterr().out)['data']['parameter']['labels'] == ['Authored choice']
    assert main(prefix + ['--type', '13', '--parameter', 'DISTORT_DRIVE']) == 2
    assert json.loads(capsys.readouterr().out)['error']['code'] == 'SYMBOL_SCOPE_UNRESOLVED'
    assert main(prefix + ['--symbol', 'DISTORT_DRIVE']) == 0
    assert json.loads(capsys.readouterr().out)['data']['type_binding_verified'] is False
