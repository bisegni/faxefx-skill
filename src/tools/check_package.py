#!/usr/bin/env python3
"""Verify the generated package manifest and absence of legacy runtime modules."""
import hashlib
import json
import subprocess
import sys
from pathlib import Path

stage = Path(sys.argv[1]).resolve()
manifest = json.loads((stage / 'manifest.json').read_text())
app = stage / ('Axe Studio.app' if manifest['platform'] == 'darwin' else 'Axe Studio')
for name, expected in manifest['files'].items():
    path = app / name
    if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise SystemExit(f'Package checksum mismatch: {name}')
    if any(p in path.parts for p in ('axefx_cli', 'node_modules', 'web')):
        raise SystemExit(f'Legacy runtime dependency packaged: {name}')
worker = app / ('Contents/Resources' if manifest['platform'] == 'darwin' else 'resources') / 'audio-worker/axe-audio-worker' / ('axe-audio-worker.exe' if manifest['platform'] == 'win32' else 'axe-audio-worker')
from PyInstaller.archive.readers import CArchiveReader
archive = CArchiveReader(str(worker))
for name in archive.toc:
    if name.endswith('.pyz'):
        modules = archive.open_embedded_archive(name).toc
        if any(name.startswith('axefx_cli') for name in modules):
            raise SystemExit('Legacy application imported into frozen worker')
request = json.dumps({
    'jsonrpc': '2.0', 'id': 1, 'method': 'initialize',
    'params': {'protocol_version': 1},
}) + '\n'
try:
    result = subprocess.run([str(worker)], input=request, text=True,
                            capture_output=True, timeout=15, check=True)
    reply = json.loads(result.stdout.splitlines()[0])
except (OSError, subprocess.SubprocessError, ValueError, IndexError) as error:
    raise SystemExit(f'Packaged audio worker failed its startup check: {error}')
if reply.get('id') != 1 or reply.get('result', {}).get('protocol_version') != 1:
    raise SystemExit('Packaged audio worker protocol startup check failed')
print(f"Verified {len(manifest['files'])} packaged file hashes and standalone audio module boundary")
