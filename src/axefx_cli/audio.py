"""Optional local reference analysis. Measurements are not rig identification."""
import hashlib
import math
import shutil
import tempfile
from pathlib import Path

from .errors import AxeError, require

MAX_BYTES = 100 * 1024 * 1024


def import_audio(store, path, original_name=None):
    path = Path(path).expanduser().resolve()
    require(path.is_file() and 0 < path.stat().st_size <= MAX_BYTES, 'INVALID_AUDIO', 'Choose an audio file up to 100 MiB')
    extension = path.suffix.lower()
    original_name = original_name or path.name
    require(isinstance(original_name, str) and len(original_name) <= 255 and Path(original_name).name == original_name,
            'INVALID_AUDIO', 'Use an audio filename without directories')
    require(extension in ('.wav', '.flac', '.aif', '.aiff', '.mp3', '.ogg', '.m4a'), 'INVALID_AUDIO', 'Unsupported audio extension')
    content_hash = hashlib.sha256(path.read_bytes()).hexdigest()
    target = store.root / 'audio' / (content_hash + extension)
    target.parent.mkdir(parents=True, exist_ok=True)
    if not target.exists():
        with tempfile.NamedTemporaryFile(dir=target.parent, delete=False) as stream:
            temporary = Path(stream.name)
        try:
            shutil.copyfile(path, temporary)
            require(hashlib.sha256(temporary.read_bytes()).hexdigest() == content_hash, 'AUDIO_CHANGED', 'Audio changed during import')
            temporary.replace(target)
        finally:
            temporary.unlink(missing_ok=True)
    artifact = store.put('audio-assets', {'sha256': content_hash, 'extension': extension, 'bytes': target.stat().st_size,
                                         'original_name': original_name, 'local_only': True})
    return {**artifact, 'audio_id': artifact['id']}


def asset_path(store, key):
    item = store.get('audio-assets', key)
    path = store.root / 'audio' / (item['sha256'] + item['extension'])
    require(path.is_file() and hashlib.sha256(path.read_bytes()).hexdigest() == item['sha256'],
            'AUDIO_CHANGED', 'Imported audio is missing or changed')
    return path


def dependencies():
    try:
        import numpy as np
        import soundfile as sf
        from scipy import signal
    except ImportError as exc:
        raise AxeError('OPTIONAL_DEPENDENCY', 'Local audio analysis requires config/requirements/audio.txt', missing=exc.name) from exc
    return np, sf, signal


def analyze(store, key, start=0, duration=30, separate=False):
    require(isinstance(start, (int, float)) and math.isfinite(start) and start >= 0 and
            isinstance(duration, (int, float)) and math.isfinite(duration) and 0 < duration <= 120,
            'INVALID_EXCERPT', 'Choose a finite start and duration from 0 to 120 seconds')
    np, sf, signal = dependencies()
    path = asset_path(store, key)
    try:
        with sf.SoundFile(path) as source:
            require(source.channels <= 8 and 8000 <= source.samplerate <= 384000, 'INVALID_AUDIO', 'Unsupported sample rate or channel count')
            rate = source.samplerate
            require(start < len(source) / rate, 'INVALID_EXCERPT', 'Excerpt starts after audio ends')
            source.seek(int(start * rate))
            data = source.read(min(int(duration * rate), len(source) - source.tell()), dtype='float32', always_2d=True)
    except (RuntimeError, sf.LibsndfileError) as exc:
        raise AxeError('AUDIO_DECODE_FAILED', 'Cannot decode this file; export WAV or FLAC locally', reason=str(exc)) from exc
    require(len(data) >= rate // 4 and np.isfinite(data).all(), 'INVALID_AUDIO', 'Need at least 0.25 seconds of finite audio')
    processing = {'local_only': True, 'separation': None, 'source_rate': rate, 'rate': rate, 'analysis_rate': 24000}
    if separate:
        try:
            from audio_separator.separator import Separator
            import importlib.metadata
        except ImportError as exc:
            raise AxeError('OPTIONAL_DEPENDENCY', 'Guitar separation requires config/requirements/separation.txt', missing=exc.name, reason=str(exc)) from exc
        with tempfile.TemporaryDirectory(prefix='axefx-separation-', dir=store.root) as temp:
            excerpt = Path(temp) / 'excerpt.wav'
            sf.write(excerpt, data, rate, subtype='FLOAT')
            import contextlib
            import sys
            with contextlib.redirect_stdout(sys.stderr):
                separator = Separator(output_dir=temp, model_file_dir=str(store.root / 'audio-models'),
                                      output_format='WAV', output_single_stem='Guitar')
                separator.load_model(model_filename='htdemucs_6s.yaml')
                outputs = separator.separate(str(excerpt))
            guitar = next((Path(p) for p in outputs if 'guitar' in Path(p).name.lower()), None)
            require(guitar is not None, 'SEPARATION_FAILED', 'Separator did not return a guitar stem')
            if not guitar.is_absolute():
                guitar = Path(temp) / guitar
            require(guitar.resolve().is_relative_to(Path(temp).resolve()), 'SEPARATION_FAILED', 'Unexpected separator output path')
            separated = import_audio(store, guitar)
            data, rate = sf.read(guitar, dtype='float32', always_2d=True)
            require(np.isfinite(data).all() and len(data) >= rate // 4, 'SEPARATION_FAILED', 'Invalid separated audio')
            model_files = []
            model_dir = store.root / 'audio-models'; config = model_dir / 'htdemucs_6s.yaml'
            if config.is_file():
                import re
                import yaml
                model_config = yaml.safe_load(config.read_text())
                identities = [config]
                if isinstance(model_config, dict):
                    for name in model_config.get('models', []):
                        if isinstance(name, str) and re.fullmatch(r'[0-9a-f]{8}', name):
                            identities.extend(sorted(model_dir.glob(name + '-*.th')))
                for file in identities:
                    checksum = hashlib.sha256()
                    with file.open('rb') as stream:
                        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
                            checksum.update(chunk)
                    model_files.append({'name': file.name, 'bytes': file.stat().st_size, 'sha256': checksum.hexdigest()})
            processing.update(rate=rate, separation={'package': 'audio-separator', 'version': importlib.metadata.version('audio-separator'),
                                          'model': 'htdemucs_6s.yaml', 'guitar_audio_id': separated['audio_id'],
                                          'model_files': model_files, 'user_audition_required': True})
    mono = data.mean(axis=1)
    # Analyze a bounded, resampled mono signal; preserve original audio for playback.
    if rate != 24000:
        common = math.gcd(rate, 24000)
        mono = signal.resample_poly(mono, 24000 // common, rate // common)
    peak = float(np.max(np.abs(data))); rms = float(np.sqrt(np.mean(data.astype('float64') ** 2)))
    require(rms > 1e-7, 'SILENT_AUDIO', 'Reference is silent or too quiet for useful measurements')
    freqs, power = signal.welch(mono, fs=24000, nperseg=min(4096, len(mono)))
    total = max(float(power.sum()), 1e-30)
    bands = [(20, 120, 'low'), (120, 500, 'low_mid'), (500, 2000, 'mid'), (2000, 6000, 'presence'), (6000, 12000, 'high')]
    energy = {name: float(power[(freqs >= low) & (freqs < high)].sum() / total) for low, high, name in bands}
    frames = mono[:len(mono) // 480 * 480].reshape(-1, 480)
    envelope = np.sqrt(np.mean(frames.astype('float64') ** 2, axis=1))
    stereo_correlation = None
    if data.shape[1] >= 2 and min(float(data[:, 0].std()), float(data[:, 1].std())) > 1e-8:
        stereo_correlation = float(np.corrcoef(data[:, 0], data[:, 1])[0, 1])
    report = {'schema_version': '1.0', 'kind': 'audio_analysis', 'audio_id': key,
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
    return {**store.put('audio-analyses', report), **report}


def compare(store, reference, recording):
    a = store.get('audio-analyses', reference); b = store.get('audio-analyses', recording)
    am, bm = a['measurements'], b['measurements']
    return {'schema_version': '1.0', 'kind': 'audio_comparison', 'reference': reference, 'recording': recording,
            'recording_gain_db_for_rms_match': am['rms_dbfs'] - bm['rms_dbfs'],
            'band_energy_difference': {k: bm['band_energy'][k] - v for k, v in am['band_energy'].items()},
            'centroid_difference_hz': bm['spectral_centroid_hz'] - am['spectral_centroid_hz'],
            'crest_difference_db': bm['crest_db'] - am['crest_db'],
            'limitation': 'Descriptive comparison only; different notes, playing and separation artifacts affect these values. Confirm revisions by listening.'}
