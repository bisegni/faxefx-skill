"""Short-lived, serialized read-only captures for the UI."""
import copy
import threading
from datetime import datetime, timezone

from .backends import USBBackend, MockBackend, ReplayBackend
from .design import Designs, from_capture
from .errors import AxeError, require
from .storage import Store


class DeviceSession:
    def __init__(self, store, backend='usb', input_name=None, output_name=None, replay_file=None, factory=None):
        self.store, self.backend = store, backend
        self.input_name, self.output_name, self.replay_file = input_name, output_name, replay_file
        self.factory = factory
        self.lock = threading.RLock()
        self.worker = None
        self.state = {'status': 'idle', 'ports': {'inputs': [], 'outputs': []}, 'error': None,
                      'captured_at': None, 'project': None, 'device': None, 'preset': None}

    def snapshot(self):
        with self.lock:
            return copy.deepcopy(self.state)

    def start(self, input_name=None, output_name=None):
        with self.lock:
            require(not self.worker or not self.worker.is_alive(), 'DEVICE_BUSY', 'A capture is already running')
            self.input_name = input_name or self.input_name
            self.output_name = output_name or self.output_name
            self.state.update(status='connecting', error=None)
            self.worker = threading.Thread(target=self.capture, daemon=True)
            self.worker.start()
        return self.snapshot()

    def make_backend(self):
        if self.factory:
            return self.factory(self.input_name, self.output_name)
        if self.backend == 'mock':
            return MockBackend(self.store)
        if self.backend == 'replay':
            require(self.replay_file, 'INVALID_ARGUMENTS', 'Replay requires a selected traffic file')
            return ReplayBackend(self.store, self.replay_file)
        return USBBackend(self.store, self.input_name, self.output_name)

    def capture(self):
        backend = None
        try:
            lock_store = Store() if self.backend == 'usb' else self.store
            with lock_store.lock('usb-midi' if self.backend == 'usb' else 'ui-capture'):
                backend = self.make_backend()
                if self.backend == 'usb':
                    ports = backend.devices()
                    with self.lock:
                        self.state['ports'] = ports
                    # Only an unambiguous Axe-Fx pair is eligible for automatic selection.
                    for kind, attr in [('inputs', 'input_name'), ('outputs', 'output_name')]:
                        chosen = getattr(self, attr)
                        candidates = [p for p in ports[kind] if 'axe' in p.lower() and 'fx' in p.lower()]
                        if chosen:
                            require(chosen in ports[kind], 'DEVICE_DISCONNECTED', 'Selected MIDI endpoint is unavailable', endpoint=chosen)
                        elif len(candidates) == 1:
                            setattr(self, attr, candidates[0])
                        else:
                            raise AxeError('SELECT_ENDPOINTS', 'Select the Axe-Fx MIDI input and output, then retry')
                    # USBBackend opens lazily; assign the selected endpoints before reading.
                    backend.input_name, backend.output_name = self.input_name, self.output_name
                with self.lock:
                    self.state.update(status='reading', input=self.input_name, output=self.output_name)
                state = backend.read_state() if self.backend == 'mock' else backend.read_state(include_geometry=True)
                design = from_capture(state)
                captured_at = datetime.now(timezone.utc).isoformat()
                design['captured_at'] = captured_at
                artifact = Designs(self.store).create(design)
                with self.lock:
                    self.state.update(status='ready', project=artifact['project'], captured_at=captured_at,
                                      device=state['device'], preset={k: state['current'].get(k) for k in
                                                                   ('name', 'slot', 'active_scene')}, error=None)
        except Exception as exc:
            error = {'code': exc.code if isinstance(exc, AxeError) else 'CAPTURE_FAILED', 'message': str(exc)}
            with self.lock:
                self.state.update(status='disconnected' if self.state['project'] else 'failed', error=error)
        finally:
            if backend:
                backend.close()

    def close(self):
        if self.worker:
            self.worker.join(timeout=5)
