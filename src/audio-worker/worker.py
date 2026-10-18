"""Standalone measurement worker. No legacy, MIDI or agent imports.

Rust supplies app-owned artifact paths over private inherited pipes. stdout is
reserved for bounded JSON-RPC 2.0 frames; errors do not include local paths.
"""
import json
import math
import sys
from pathlib import Path

VERSION = 1
MAX_FRAME = 4 * 1024 * 1024


def measure(path, artifact, start=0, duration=30):
    if (not isinstance(start, (int, float)) or isinstance(start, bool)
        or not math.isfinite(start) or start < 0
        or not isinstance(duration, (int, float)) or isinstance(duration, bool)
        or not math.isfinite(duration) or not 0 < duration <= 120):
        raise ValueError("Invalid excerpt")
    import numpy as np
    import soundfile as sf
    from scipy import signal
    if not Path(path).is_file() or not 0 < Path(path).stat().st_size <= 100 * 1024 * 1024:
        raise ValueError("Invalid audio artifact")
    with sf.SoundFile(path) as source:
        rate = source.samplerate
        if not 8000 <= rate <= 384000 or not 1 <= source.channels <= 8 or start >= len(source) / rate:
            raise ValueError("Unsupported audio format or excerpt")
        source.seek(int(start * rate))
        data = source.read(min(int(duration * rate), len(source) - source.tell()), dtype="float32", always_2d=True)
    if len(data) < rate // 4 or not np.isfinite(data).all():
        raise ValueError("Need 0.25 seconds of finite audio")
    processing = {"local_only": True, "separation": None, "source_rate": rate, "rate": rate, "analysis_rate": 24000}
    mono = data.mean(axis=1)
    # Analyze a bounded, resampled mono signal; preserve original audio for playback.
    if rate != 24000:
        common = math.gcd(rate, 24000)
        mono = signal.resample_poly(mono, 24000 // common, rate // common)
    peak = float(np.max(np.abs(data))); rms = float(np.sqrt(np.mean(data.astype('float64') ** 2)))
    if rms <= 1e-7:
        raise ValueError("Silent audio")
    freqs, power = signal.welch(mono, fs=24000, nperseg=min(4096, len(mono)))
    total = max(float(power.sum()), 1e-30)
    bands = [(20, 120, 'low'), (120, 500, 'low_mid'), (500, 2000, 'mid'), (2000, 6000, 'presence'), (6000, 12000, 'high')]
    energy = {name: float(power[(freqs >= low) & (freqs < high)].sum() / total) for low, high, name in bands}
    frames = mono[:len(mono) // 480 * 480].reshape(-1, 480)
    envelope = np.sqrt(np.mean(frames.astype('float64') ** 2, axis=1))
    stereo_correlation = None
    if data.shape[1] >= 2 and min(float(data[:, 0].std()), float(data[:, 1].std())) > 1e-8:
        stereo_correlation = float(np.corrcoef(data[:, 0], data[:, 1])[0, 1])
    report = {'schema_version': '1.0', 'kind': 'audio_analysis', 'artifact_id': artifact,
              'excerpt': {'start': start, 'duration': len(data) / rate}, 'processing': processing,
              'measurements': {'peak_dbfs': 20 * math.log10(max(peak, 1e-12)), 'rms_dbfs': 20 * math.log10(rms),
                               'crest_db': 20 * math.log10(max(peak / rms, 1e-12)),
                               'clipped_sample_fraction': float(np.mean(np.abs(data) >= .999)),
                               'spectral_centroid_hz': float((freqs * power).sum() / total), 'band_energy': energy,
                               'envelope_p10_p50_p90': [float(v) for v in np.percentile(envelope, [10, 50, 90])],
                               'stereo_correlation': stereo_correlation},
              'hypotheses': [], 'rig_identified': False,
              'limitations': ['Measurements are content/performance dependent; they do not identify an amp, cabinet or exact settings.',
                              'A mix or separated stem can include other instruments, layered guitars and separation artifacts.',
                              'Attack, sustain, delay and modulation require listening/context; no exact tone-match score is provided.']}
    return report


def dispatch(method, params):
    if method == "initialize":
        if params.get("protocol_version") != VERSION:
            raise ValueError("Incompatible audio protocol")
        return {"protocol_version": VERSION, "methods": ["measure"], "separation": False}
    if method == "measure":
        return measure(params["path"], params["artifact"], params.get("start", 0), params.get("duration", 30))
    raise ValueError("Unsupported audio method")


def main():
    initialized = False
    while True:
        frame = sys.stdin.buffer.readline(MAX_FRAME + 1)
        if not frame:
            return
        if len(frame) > MAX_FRAME:
            return
        request_id = None
        try:
            request = json.loads(frame)
            if not isinstance(request, dict):
                raise ValueError("Invalid request")
            request_id = request.get("id")
            if request.get("jsonrpc") != "2.0" or not isinstance(request_id, int) or isinstance(request_id, bool):
                raise ValueError("Invalid JSON-RPC envelope")
            method = request.get("method")
            params = request.get("params", {})
            if not isinstance(params, dict):
                raise ValueError("Invalid params")
            if method == "shutdown":
                return
            if method != "initialize" and not initialized:
                raise ValueError("Initialize first")
            result = dispatch(method, params)
            initialized |= method == "initialize"
            response = {"jsonrpc": "2.0", "id": request_id, "result": result}
        except Exception:
            response = {"jsonrpc": "2.0", "id": request_id, "error": {"code": -32602, "message": "Audio request failed"}}
        print(json.dumps(response, allow_nan=False), flush=True)


if __name__ == "__main__":
    main()
