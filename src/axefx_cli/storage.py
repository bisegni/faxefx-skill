"""Atomic artifacts and process locking. No device state is stored on import."""
import hashlib
import json
import os
import tempfile
from pathlib import Path

from filelock import FileLock, Timeout
from platformdirs import user_data_path

from .errors import AxeError, require


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def load(path):
    try:
        with Path(path).open(encoding='utf8') as stream:
            return json.load(stream, parse_constant=lambda x: (_ for _ in ()).throw(ValueError(x)))
    except (OSError, ValueError) as exc:
        raise AxeError('INVALID_ARTIFACT', 'Cannot read valid JSON artifact', path=str(path), reason=str(exc)) from exc


def atomic_write(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix='.axefx-', dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(canonical(value) + b'\n')
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


class Store:
    def __init__(self, root=None):
        self.root = Path(root) if root else user_data_path('axefx-cli', appauthor=False)
        self.root.mkdir(parents=True, exist_ok=True)

    def lock(self, identity):
        lock = FileLock(str(self.root / (digest(identity) + '.lock')))
        try:
            return lock.acquire(timeout=0)
        except Timeout as exc:
            raise AxeError('DEVICE_BUSY', 'Another CLI process is using this device') from exc

    def put(self, kind, value):
        key = digest(value)
        path = self.root / kind / (key + '.json')
        if path.exists():
            require(load(path) == value, 'ARTIFACT_CONFLICT', 'Artifact hash conflict')
        else:
            atomic_write(path, value)
        return dict(id=key, path=str(path))

    def get(self, kind, key):
        require(isinstance(key, str) and len(key) == 64 and all(c in '0123456789abcdef' for c in key),
                'INVALID_ARTIFACT', 'Use the artifact ID returned by the CLI')
        value = load(self.root / kind / (key + '.json'))
        require(digest(value) == key, 'ARTIFACT_CHANGED', 'Immutable artifact was modified', id=key)
        return value

    def journal(self, key, value):
        atomic_write(self.root / 'journals' / (key + '.json'), value)
