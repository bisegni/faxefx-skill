"""Loopback-only draft workbench. There is deliberately no device apply route."""
import json
import os
import secrets
import signal
import subprocess
import sys
import tempfile
import threading
import uuid
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlsplit, unquote

from .audio import MAX_BYTES, asset_path, import_audio
from .catalog import summary, CATALOG
from .design import Designs, compile_design, validate_design
from .errors import AxeError, require
from .protocol import coverage
from .render import svg
from .storage import atomic_write
from .device_session import DeviceSession
from .chat import Chat

ROOT = Path(__file__).resolve().parents[2]
WEB = ROOT / 'web' / 'dist'


class Jobs:
    """Audio work runs in isolated local processes, so cancellation stops inference."""
    def __init__(self, store):
        self.store = store
        self.entries = {}
        self.lock = threading.Lock()

    def submit(self, request):
        require(set(request) <= {'audio_id', 'start', 'duration', 'separate'}, 'INVALID_JOB', 'Unknown audio job field')
        asset_path(self.store, request['audio_id'])
        start, duration = float(request.get('start', 0)), float(request.get('duration', 30))
        import math
        require(math.isfinite(start) and start >= 0 and math.isfinite(duration) and 0 < duration <= 120,
                'INVALID_EXCERPT', 'Choose a finite excerpt of at most 120 seconds')
        require(type(request.get('separate', False)) is bool, 'INVALID_JOB', 'separate must be boolean')
        with self.lock:
            require(not any(j['status'] == 'running' for j in self.entries.values()), 'JOB_BUSY', 'One audio job can run at a time')
            key = uuid.uuid4().hex
            args = [sys.executable, str(ROOT / 'tools/axefx.py'), '--state-dir', str(self.store.root),
                    'audio', 'analyze', '--audio-id', request['audio_id'], '--start', str(start), '--duration', str(duration)]
            if request.get('separate'):
                args.append('--separate')
            log = tempfile.TemporaryFile()
            process = subprocess.Popen(args, stdout=log, stderr=log, start_new_session=os.name != 'nt')
            self.entries[key] = {'id': key, 'status': 'running', 'process': process, 'log': log}
        threading.Thread(target=self.finish, args=(key,), daemon=True).start()
        return self.get(key)

    def finish(self, key):
        entry = self.entries[key]
        process = entry['process']; process.wait()
        with self.lock:
            log = entry.pop('log'); log.seek(max(0, log.seek(0, 2) - 1024 * 1024))
            lines = log.read().decode('utf8', 'replace').splitlines(); log.close()
            if entry['status'] == 'cancelled':
                return
            result = None
            for line in reversed(lines):
                try:
                    candidate = json.loads(line)
                    if isinstance(candidate, dict) and 'ok' in candidate:
                        result = candidate; break
                except ValueError:
                    continue
            entry.update(status='completed' if process.returncode == 0 and result and result['ok'] else 'failed',
                         result=result or {'ok': False, 'error': {'code': 'AUDIO_JOB_FAILED', 'message': 'Audio worker failed; run audio analyze in the CLI for diagnostics.'}})

    def get(self, key):
        with self.lock:
            require(key in self.entries, 'INVALID_JOB', 'Unknown audio job')
            return {k: v for k, v in self.entries[key].items() if k not in ('process', 'log')}

    def cancel(self, key):
        with self.lock:
            require(key in self.entries, 'INVALID_JOB', 'Unknown audio job')
            entry = self.entries[key]
            if entry['status'] == 'running':
                entry['status'] = 'cancelled'
                if os.name == 'nt':
                    entry['process'].terminate()
                else:
                    try:
                        os.killpg(entry['process'].pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
        return self.get(key)

    def close(self):
        for key in list(self.entries):
            self.cancel(key)
        for entry in self.entries.values():
            try:
                entry['process'].wait(timeout=3)
            except subprocess.TimeoutExpired:
                if os.name == 'nt':
                    entry['process'].kill()
                else:
                    os.killpg(entry['process'].pid, signal.SIGKILL)


class Workbench(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, store, project=None, backend='usb', port=0, input_name=None, output_name=None,
                 replay_file=None, device_factory=None, chat_factory=Chat):
        if project:
            Designs(store).get(project)
        self.store, self.project, self.backend = store, project, backend
        self.device = DeviceSession(store, backend, input_name, output_name, replay_file, device_factory)
        self.chat_factory, self.chats = chat_factory, {}
        self.session_lock = threading.RLock()
        self.token = secrets.token_urlsafe(32)
        self.jobs = Jobs(store)
        super().__init__(('127.0.0.1', port), Handler)
        self.origin = 'http://127.0.0.1:' + str(self.server_port)

    def server_close(self):
        self.jobs.close()
        self.device.close()
        for chat in self.chats.values():
            chat.close()
        super().server_close()

    def current(self):
        with self.session_lock:
            captured = self.device.snapshot()
            if captured['project']:
                self.project = captured['project']
            return self.project, captured

    def chat(self):
        project, _ = self.current()
        require(project, 'NO_CAPTURE', 'Read a device preset before starting chat')
        with self.session_lock:
            if project not in self.chats:
                self.chats[project] = self.chat_factory(self.store, project, self.backend)
            return self.chats[project]


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def send(self, status, data, mime='application/json'):
        body = json.dumps(data, allow_nan=False).encode() if mime == 'application/json' else data
        self.send_response(status)
        self.send_header('Content-Type', mime)
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Cache-Control', 'no-store')
        self.send_header('X-Content-Type-Options', 'nosniff')
        self.send_header('Referrer-Policy', 'no-referrer')
        self.send_header('Content-Security-Policy', "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; media-src 'self' blob:; connect-src 'self'; frame-ancestors 'none'")
        self.end_headers(); self.wfile.write(body)

    def authenticated(self):
        expected = self.server.origin
        require(self.headers.get('Host') == urlsplit(expected).netloc, 'FORBIDDEN', 'Unexpected host')
        require(self.headers.get('Origin') in (None, expected), 'FORBIDDEN', 'Unexpected origin')
        require(secrets.compare_digest(self.headers.get('Authorization', ''), 'Bearer ' + self.server.token),
                'FORBIDDEN', 'Open the session URL printed by design open')

    def body(self, limit=2 * 1024 * 1024):
        require('Transfer-Encoding' not in self.headers, 'INVALID_REQUEST', 'Chunked requests are not supported')
        size = int(self.headers.get('Content-Length', '0'))
        require(0 < size <= limit, 'REQUEST_TOO_LARGE', 'Request body exceeds limit')
        self.connection.settimeout(15)
        data = self.rfile.read(size)
        require(len(data) == size, 'INVALID_REQUEST', 'Truncated request')
        return data

    def do_GET(self):
        self.handle_request('GET')

    def do_POST(self):
        self.handle_request('POST')

    def handle_request(self, method):
        try:
            url = urlsplit(self.path); path = url.path; query = parse_qs(url.query)
            if path.startswith('/api/'):
                self.authenticated()
                data, mime = self.api(method, path, query)
                self.send(200, data, mime)
                return
            require(method == 'GET', 'NOT_FOUND', 'Unknown route')
            # Only bundled frontend files; source, artifacts and symlink escapes are not served.
            target = (WEB / ('index.html' if path == '/' else path.lstrip('/'))).resolve()
            require(target.is_relative_to(WEB.resolve()) and target.is_file(), 'NOT_FOUND', 'Frontend asset not found; run npm ci and npm run build in web/')
            import mimetypes
            mime = mimetypes.guess_type(str(target))[0] or 'application/octet-stream'
            self.send(200, target.read_bytes(), mime)
        except AxeError as exc:
            self.send(403 if exc.code == 'FORBIDDEN' else 409 if exc.code == 'REVISION_CONFLICT' else 404 if exc.code == 'NOT_FOUND' else 400,
                      {'ok': False, 'error': {'code': exc.code, 'message': str(exc), 'details': exc.details}})
        except (ValueError, TypeError, KeyError, OSError) as exc:
            self.send(400, {'ok': False, 'error': {'code': 'INVALID_REQUEST', 'message': str(exc)}})

    def api(self, method, path, query):
        store = self.server.store; designs = Designs(store)
        project, device = self.server.current()
        if method == 'GET':
            if path == '/api/state':
                return {**(designs.get(project) if project else {'project': None, 'design': None, 'revision': None}),
                        'device_session': device, 'backend': self.server.backend,
                        'catalog': {**summary(), 'types': [summary(t) for t in CATALOG['types']]},
                        'protocol': coverage(), 'hardware_apply_available': False}, 'application/json'
            if path == '/api/chat':
                return self.server.chat().snapshot(), 'application/json'
            if path == '/api/render':
                design = designs.get(self.server.project)['design']
                view = query.get('view', ['logical'])[0]
                source = design
                if view == 'captured':
                    require(design.get('captured'), 'GEOMETRY_UNAVAILABLE', 'This project has no hardware capture')
                    source = design['captured']
                elif view == 'physical':
                    compiled = compile_design(design, self.server.backend)
                    require(compiled['physical'], 'GEOMETRY_UNAVAILABLE', 'Compile has no physical grid')
                    source = compiled
                block_id = query.get('block', [None])[0]
                scene = query.get('scene', [None])[0]
                return svg(source, physical=view == 'physical', block_id=block_id, scene_number=int(scene) if scene else None).encode(), 'image/svg+xml'
            if path.startswith('/api/jobs/'):
                return self.server.jobs.get(path.rsplit('/', 1)[1]), 'application/json'
            if path.startswith('/api/audio/'):
                file = asset_path(store, path.rsplit('/', 1)[1])
                import mimetypes
                return file.read_bytes(), mimetypes.guess_type(str(file))[0] or 'application/octet-stream'
        if method == 'POST':
            if path == '/api/audio/import':
                filename = Path(unquote(self.headers.get('X-Filename', 'reference.wav'))).name
                suffix = Path(filename).suffix.lower()
                require(suffix in ('.wav', '.flac', '.aiff', '.aif', '.mp3', '.ogg', '.m4a'), 'INVALID_AUDIO', 'Unsupported audio format')
                payload = self.body(MAX_BYTES)
                with tempfile.NamedTemporaryFile(suffix=suffix, dir=store.root, delete=False) as temp:
                    temp.write(payload); name = Path(temp.name)
                try:
                    return import_audio(store, name, original_name=filename), 'application/json'
                finally:
                    name.unlink(missing_ok=True)
            body = json.loads(self.body(), parse_constant=lambda v: (_ for _ in ()).throw(ValueError(v)))
            require(isinstance(body, dict), 'INVALID_REQUEST', 'Expected a JSON object')
            if path == '/api/device/capture':
                require(set(body) <= {'input', 'output'}, 'INVALID_REQUEST', 'Expected MIDI input/output names')
                require(all(isinstance(v, str) and len(v) <= 1024 for v in body.values()), 'INVALID_REQUEST', 'Endpoint names must be text')
                require(not any(c.snapshot()['status'] in ('connecting', 'running') for c in self.server.chats.values()),
                        'CHAT_BUSY', 'Finish or stop the AI response before refreshing the device')
                return self.server.device.start(body.get('input'), body.get('output')), 'application/json'
            if path.startswith('/api/chat/'):
                require(body.get('project') == project, 'REVISION_CONFLICT', 'The current preset project changed')
                chat = self.server.chat()
                if path == '/api/chat/connect':
                    return chat.connect(), 'application/json'
                if path == '/api/chat/message':
                    return chat.start(body.get('text'), body.get('base'), body.get('selected_block'), body.get('analysis_ids')), 'application/json'
                if path == '/api/chat/interrupt':
                    return chat.interrupt(), 'application/json'
                if path == '/api/chat/decision':
                    require(type(body.get('accept')) is bool, 'INVALID_REQUEST', 'Choose accept or reject')
                    return chat.decide(body.get('id'), body['accept'], body.get('base')), 'application/json'
            if path == '/api/revision':
                require(set(body) == {'base', 'design'}, 'INVALID_REQUEST', 'Expected base and design')
                return designs.revise(self.server.project, body['base'], body['design']), 'application/json'
            if path == '/api/validate':
                return {'design': validate_design(body['design'])}, 'application/json'
            if path == '/api/compile':
                artifact = designs.get(self.server.project)
                require(body.get('revision') == artifact['revision'], 'REVISION_CONFLICT', 'Save/reload the current revision before compilation')
                result = compile_design(artifact['design'], self.server.backend)
                result.update(project=self.server.project, revision=artifact['revision'])
                return {**result, **store.put('compilations', result)}, 'application/json'
            if path == '/api/jobs':
                return self.server.jobs.submit(body), 'application/json'
            if path.startswith('/api/jobs/') and path.endswith('/cancel'):
                return self.server.jobs.cancel(path.split('/')[-2]), 'application/json'
            if path == '/api/audio/compare':
                from .audio import compare
                return compare(store, body['reference'], body['recording']), 'application/json'
        raise AxeError('NOT_FOUND', 'Unknown route; this API cannot apply MIDI edits')


def serve(store, project, backend, port=0, open_browser=True, **options):
    require((WEB / 'index.html').is_file(), 'FRONTEND_NOT_BUILT', 'Run npm ci and npm run build in web/ before design open')
    require(0 <= port <= 65535, 'INVALID_PORT', 'Choose port 0–65535')
    server = Workbench(store, project, backend, port, **options)
    url = server.origin + '/#' + server.token
    session = store.root / 'workbench' / ((project or 'device-' + str(server.server_port)) + '.json')
    atomic_write(session, {'url': url, 'pid': os.getpid(), 'project': project, 'backend': backend})
    print(json.dumps({'event': 'workbench_ready', 'url': url, 'project': project, 'backend': backend}), flush=True)
    if open_browser:
        webbrowser.open(url)
    if project is None:
        server.device.start()
    try:
        server.serve_forever(poll_interval=.25)
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close(); session.unlink(missing_ok=True)
    return {'status': 'closed', 'project': project}
