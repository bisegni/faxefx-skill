import json
import threading
import urllib.error
import urllib.request

import pytest

from axefx_cli.design import Designs, starter
from axefx_cli.storage import Store
from axefx_cli.workbench import Workbench


@pytest.fixture
def server(tmp_path):
    store = Store(tmp_path); project = Designs(store).create(starter())
    server = Workbench(store, project['project'], 'mock')
    thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
    yield server, project
    server.shutdown(); server.server_close(); thread.join(timeout=2)


def request(server, path, body=None, token=True, origin=None):
    headers = {'Content-Type': 'application/json'}
    if token:
        headers['Authorization'] = 'Bearer ' + server.token
    if origin:
        headers['Origin'] = origin
    req = urllib.request.Request(server.origin + path, data=json.dumps(body).encode() if body is not None else None, headers=headers)
    return urllib.request.urlopen(req, timeout=3)


def test_api_requires_session_and_rejects_cross_origin(server):
    s, _ = server
    for token, origin in [(False, None), (True, 'https://unrelated.example')]:
        with pytest.raises(urllib.error.HTTPError) as error:
            request(s, '/api/state', token=token, origin=origin)
        assert error.value.code == 403
    state = json.load(request(s, '/api/state'))
    assert not state['hardware_apply_available'] and state['backend'] == 'mock'


def test_browser_revision_roundtrip_conflict_and_no_apply_route(server):
    s, p = server; d = p['design']; d['intent'] = 'A spacious clean'
    revised = json.load(request(s, '/api/revision', {'base': p['revision'], 'design': d}))
    assert revised['revision'] != p['revision']
    with pytest.raises(urllib.error.HTTPError) as error:
        request(s, '/api/revision', {'base': p['revision'], 'design': d})
    assert error.value.code == 409
    compiled = json.load(request(s, '/api/compile', {'revision': revised['revision']}))
    assert compiled['buildable'] and compiled['physical']['connections']
    with pytest.raises(urllib.error.HTTPError) as error:
        request(s, '/api/apply', {'approved': True})
    assert error.value.code == 404
    assert not (s.store.root / 'mock-device.json').exists()


def test_http_svg_and_artifact_paths_cannot_escape(server):
    s, _ = server
    svg = request(s, '/api/render').read()
    assert b'PROPOSED logical design' in svg
    with pytest.raises(urllib.error.HTTPError):
        request(s, '/api/audio/../../secrets')
    with pytest.raises(urllib.error.HTTPError):
        request(s, '/api/compile', {'revision': 'stale'})


def test_audio_upload_runs_local_worker_and_returns_measurements(server, tmp_path):
    pytest.importorskip('numpy'); pytest.importorskip('scipy'); pytest.importorskip('soundfile')
    import math
    import struct
    import time
    import wave
    s, _ = server
    path = tmp_path / 'authored.wav'
    with wave.open(str(path), 'wb') as output:
        output.setnchannels(1); output.setsampwidth(2); output.setframerate(24000)
        output.writeframes(b''.join(struct.pack('<h', int(8000 * math.sin(2 * math.pi * 440 * i / 24000))) for i in range(24000)))
    req = urllib.request.Request(s.origin + '/api/audio/import', data=path.read_bytes(),
                                 headers={'Authorization': 'Bearer ' + s.token, 'X-Filename': 'authored.wav'}, method='POST')
    imported = json.load(urllib.request.urlopen(req, timeout=3))
    assert s.store.get('audio-assets', imported['audio_id'])['original_name'] == 'authored.wav'
    job = json.load(request(s, '/api/jobs', {'audio_id': imported['audio_id'], 'duration': 1}))
    deadline = time.monotonic() + 15
    while job['status'] == 'running' and time.monotonic() < deadline:
        time.sleep(.1)
        job = json.load(request(s, '/api/jobs/' + job['id']))
    assert job['status'] == 'completed', job
    assert abs(job['result']['data']['measurements']['spectral_centroid_hz'] - 440) < 5
    assert not (s.store.root / 'mock-device.json').exists()
