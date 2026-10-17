import math
import struct
import wave

import pytest

from axefx_cli.audio import analyze, asset_path, compare, import_audio
from axefx_cli.errors import AxeError
from axefx_cli.storage import Store


def tone(path, frequency=440, amplitude=.3):
    with wave.open(str(path), 'wb') as f:
        f.setnchannels(1); f.setsampwidth(2); f.setframerate(24000)
        f.writeframes(b''.join(struct.pack('<h', int(amplitude * 32767 * math.sin(2 * math.pi * frequency * i / 24000))) for i in range(24000)))


def test_audio_asset_detects_change_and_rejects_traversal(tmp_path):
    store = Store(tmp_path / 'state'); path = tmp_path / 'test.wav'; tone(path)
    imported = import_audio(store, path)
    assert path.exists() and asset_path(store, imported['audio_id']).read_bytes() == path.read_bytes()
    with pytest.raises(AxeError):
        asset_path(store, '../outside')
    asset_path(store, imported['audio_id']).write_bytes(b'changed')
    with pytest.raises(AxeError, match='changed'):
        asset_path(store, imported['audio_id'])


def test_measured_frequency_and_level_comparison(tmp_path):
    pytest.importorskip('numpy'); pytest.importorskip('scipy'); pytest.importorskip('soundfile')
    store = Store(tmp_path / 'state'); a = tmp_path / 'a.wav'; b = tmp_path / 'b.wav'
    tone(a, amplitude=.3); tone(b, amplitude=.15)
    ar = analyze(store, import_audio(store, a)['audio_id'], duration=1)
    br = analyze(store, import_audio(store, b)['audio_id'], duration=1)
    assert abs(ar['measurements']['spectral_centroid_hz'] - 440) < 5
    result = compare(store, ar['id'], br['id'])
    assert abs(result['recording_gain_db_for_rms_match'] - 6.02) < .02
    assert abs(result['centroid_difference_hz']) < 1 and not ar['rig_identified']
    assert ar['processing']['local_only']


def test_silence_and_invalid_excerpt(tmp_path):
    pytest.importorskip('numpy'); pytest.importorskip('scipy'); pytest.importorskip('soundfile')
    store = Store(tmp_path / 'state'); path = tmp_path / 'silence.wav'; tone(path, amplitude=0)
    key = import_audio(store, path)['audio_id']
    with pytest.raises(AxeError, match='silent'):
        analyze(store, key)
    for start, duration in [(float('nan'), 1), (0, float('inf')), (0, 121), (10, 1)]:
        with pytest.raises(AxeError):
            analyze(store, key, start, duration)


def test_separation_adapter_retains_stem_and_keeps_stdout_clean(tmp_path, monkeypatch, capsys):
    pytest.importorskip('numpy'); pytest.importorskip('scipy'); pytest.importorskip('soundfile')
    import importlib.metadata
    import shutil
    import sys
    import types
    from pathlib import Path

    class AuthoredSeparator:
        def __init__(self, **kwargs):
            self.output = Path(kwargs['output_dir'])

        def load_model(self, model_filename):
            assert model_filename == 'htdemucs_6s.yaml'
            print('Authored separator log')

        def separate(self, source):
            target = self.output / 'Guitar.wav'; shutil.copyfile(source, target)
            return ['Guitar.wav']

    package = types.ModuleType('audio_separator'); module = types.ModuleType('audio_separator.separator')
    module.Separator = AuthoredSeparator
    monkeypatch.setitem(sys.modules, 'audio_separator', package)
    monkeypatch.setitem(sys.modules, 'audio_separator.separator', module)
    original = importlib.metadata.version
    monkeypatch.setattr(importlib.metadata, 'version', lambda name: 'authored-test' if name == 'audio-separator' else original(name))
    store = Store(tmp_path / 'state'); path = tmp_path / 'reference.wav'; tone(path)
    report = analyze(store, import_audio(store, path)['audio_id'], duration=1, separate=True)
    separation = report['processing']['separation']
    assert separation['version'] == 'authored-test' and separation['user_audition_required']
    assert asset_path(store, separation['guitar_audio_id']).exists()
    captured = capsys.readouterr(); assert captured.out == '' and 'Authored separator log' in captured.err
