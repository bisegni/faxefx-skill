import copy
import threading

import pytest

from axefx_cli.backends import MockBackend
from axefx_cli.design import Designs
from axefx_cli.device_session import DeviceSession
from axefx_cli.errors import AxeError
from axefx_cli.storage import Store


class ReadDevice:
    def __init__(self, state, ports=None, error=None):
        self.state, self.error = state, error
        self.ports = ports or {'inputs': ['Axe-Fx III'], 'outputs': ['Axe-Fx III']}
        self.closed = False

    def devices(self):
        return self.ports

    def read_state(self, include_geometry=False):
        assert include_geometry
        if self.error:
            raise self.error
        return copy.deepcopy(self.state)

    def close(self):
        self.closed = True


def wait(session):
    session.worker.join(timeout=3)
    assert not session.worker.is_alive()
    return session.snapshot()


def test_capture_refresh_and_disconnect_preserve_prior_project(tmp_path, monkeypatch):
    store = Store(tmp_path); state = MockBackend(store).read_state()
    state['device']['simulated'] = False
    state['current']['complete'] = False
    state['current']['complete_geometry'] = True
    monkeypatch.setattr('axefx_cli.device_session.Store', lambda: store)
    devices = []
    def factory(*_):
        device = ReadDevice(state); devices.append(device); return device
    session = DeviceSession(store, factory=factory)
    session.start(); first = wait(session)
    assert first['status'] == 'ready' and devices[0].closed and first['captured_at']
    session.start(); second = wait(session)
    assert second['project'] != first['project']
    assert Designs(store).get(first['project'])['design']['captured']['current']['complete'] is False
    session.factory = lambda *_: ReadDevice(state, error=AxeError('USB_TIMEOUT', 'Disconnected'))
    session.start(); failed = wait(session)
    assert failed['status'] == 'disconnected' and failed['project'] == second['project']


def test_ambiguous_ports_require_explicit_selection_and_release_lock(tmp_path, monkeypatch):
    store = Store(tmp_path); state = MockBackend(store).read_state()
    monkeypatch.setattr('axefx_cli.device_session.Store', lambda: store)
    ports = {'inputs': ['Axe-Fx 1', 'Axe-Fx 2'], 'outputs': ['Axe-Fx 1', 'Axe-Fx 2']}
    session = DeviceSession(store, factory=lambda *_: ReadDevice(state, ports))
    session.start(); assert wait(session)['error']['code'] == 'SELECT_ENDPOINTS'
    with store.lock('usb-midi'):
        pass
    session.start('Axe-Fx 2', 'Axe-Fx 2'); assert wait(session)['status'] == 'ready'
    session.start('Missing', 'Axe-Fx 2'); assert wait(session)['error']['code'] == 'DEVICE_DISCONNECTED'


def test_capture_busy_does_not_launch_concurrent_reads(tmp_path, monkeypatch):
    store = Store(tmp_path); state = MockBackend(store).read_state()
    monkeypatch.setattr('axefx_cli.device_session.Store', lambda: store)
    entered, release = threading.Event(), threading.Event()
    device = ReadDevice(state)
    def read(**_):
        entered.set(); release.wait(timeout=3); return state
    device.read_state = read
    session = DeviceSession(store, factory=lambda *_: device)
    session.start(); assert entered.wait(2)
    with pytest.raises(AxeError, match='already running'):
        session.start()
    release.set(); assert wait(session)['status'] == 'ready'
