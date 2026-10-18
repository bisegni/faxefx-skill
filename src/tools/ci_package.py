#!/usr/bin/env python3
"""Build CI packages in an isolated measurement-only Python environment."""
import os
import platform
import subprocess
import sys
import venv
from pathlib import Path
root = Path(__file__).resolve().parents[2]
os.chdir(root)
venv.EnvBuilder(with_pip=True).create(root / '.venv-package')
python = root / '.venv-package' / ('Scripts/python.exe' if os.name == 'nt' else 'bin/python')
subprocess.run([str(python), '-m', 'pip', 'install', '-r', 'src/audio-worker/requirements.txt', 'pyinstaller==6.20.0'], check=True)
subprocess.run([str(python), 'src/tools/package.py'], check=True)
stage = root / 'dist' / f'axe-studio-{sys.platform}-{platform.machine()}-release'
subprocess.run([str(python), 'src/tools/check_package.py', str(stage)], check=True)
app = stage / ('Axe Studio.app/Contents/Resources' if sys.platform == 'darwin' else 'Axe Studio/resources')
worker = app / 'audio-worker/axe-audio-worker' / ('axe-audio-worker.exe' if sys.platform == 'win32' else 'axe-audio-worker')
env = dict(os.environ, AXE_AUDIO_WORKER=str(worker))
subprocess.run(['cargo', 'test', '-p', 'axe-audio', '--test', 'artifacts', '--', '--include-ignored'], env=env, check=True)
