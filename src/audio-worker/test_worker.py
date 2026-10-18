import importlib.util
import json
import subprocess
import sys
from pathlib import Path

import numpy as np
import pytest
import soundfile as sf

spec = importlib.util.spec_from_file_location("audio_worker", Path(__file__).with_name("worker.py"))
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)


def test_authored_sine(tmp_path):
    rate = 48000
    x = .25 * np.sin(2 * np.pi * 1000 * np.arange(rate) / rate)
    path = tmp_path / "sine.wav"
    sf.write(path, x, rate, subtype="FLOAT")
    result = worker.measure(path, "authored", 0, 1)
    m = result["measurements"]
    assert abs(m["spectral_centroid_hz"] - 1000) < 2
    assert abs(m["crest_db"] - 3.0103) < .01
    assert m["band_energy"]["mid"] > .99
    assert not result["rig_identified"]


def test_silence_and_bounds(tmp_path):
    path = tmp_path / "silence.wav"
    sf.write(path, np.zeros(48000), 48000)
    with pytest.raises(ValueError):
        worker.measure(path, "silent")
    with pytest.raises(ValueError):
        worker.measure(path, "bad", duration=float("nan"))


def test_stdio_isolation():
    frames = [dict(jsonrpc="2.0", id=1, method="initialize", params={"protocol_version": 1}),
              dict(jsonrpc="2.0", id=2, method="midi_write", params={})]
    result = subprocess.run([sys.executable, str(Path(worker.__file__))], input="".join(json.dumps(x)+"\n" for x in frames), text=True, capture_output=True, check=True)
    replies = [json.loads(x) for x in result.stdout.splitlines()]
    assert replies[0]["result"]["protocol_version"] == 1
    assert "error" in replies[1]
