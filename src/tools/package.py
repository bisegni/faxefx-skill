#!/usr/bin/env python3
"""Build-only helper. End users receive Rust plus a frozen audio interpreter."""
import argparse
import hashlib
import json
import platform
import plistlib
import shutil
import subprocess
import sys
from pathlib import Path


def run(*args):
    subprocess.run(args, check=True)


def verify_worker(worker, executable):
    request = json.dumps({
        'jsonrpc': '2.0', 'id': 1, 'method': 'initialize',
        'params': {'protocol_version': 1},
    }) + '\n'
    try:
        result = subprocess.run(
            [str(worker / executable)], input=request, text=True,
            capture_output=True, timeout=15, check=True,
        )
        reply = json.loads(result.stdout.splitlines()[0])
    except (OSError, subprocess.SubprocessError, ValueError, IndexError) as error:
        raise SystemExit(f'Frozen audio worker failed its startup check: {error}')
    if reply.get('id') != 1 or reply.get('result', {}).get('protocol_version') != 1:
        raise SystemExit('Frozen audio worker failed its protocol startup check')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--debug', action='store_true')
    parser.add_argument('--worker-dir', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    import os
    os.chdir(root)
    profile = 'debug' if args.debug else 'release'
    cargo_args = ['cargo', 'build', '-p', 'axe-desktop', '--locked']
    if not args.debug:
        cargo_args.append('--release')
    run(*cargo_args)
    worker = args.worker_dir
    if worker is None:
        run(sys.executable, '-m', 'PyInstaller', '--noconfirm', '--clean', '--onedir',
            '--name', 'axe-audio-worker', '--distpath', str(root / 'dist/audio-core'),
            '--workpath', str(root / '.local/audio-core-pyinstaller'),
            '--specpath', str(root / '.local/audio-core-pyinstaller'),
            str(root / 'src/audio-worker/worker.py'))
        worker = root / 'dist/audio-core/axe-audio-worker'
    worker = worker.resolve()
    executable = 'axe-desktop.exe' if sys.platform == 'win32' else 'axe-desktop'
    worker_executable = 'axe-audio-worker.exe' if sys.platform == 'win32' else 'axe-audio-worker'
    if not worker.is_dir() or not (worker / worker_executable).is_file():
        raise SystemExit('Frozen audio worker directory is missing')
    verify_worker(worker, worker_executable)
    stage = root / 'dist' / 'axe-studio'
    if stage.exists():
        # Remove only the generated staging tree, never user app data or legacy source.
        shutil.rmtree(stage)
    stage.mkdir(parents=True)
    if sys.platform == 'darwin':
        app = stage / 'Axe Studio.app'
        binary_dir = app / 'Contents/MacOS'
        resources = app / 'Contents/Resources'
        binary_dir.mkdir(parents=True)
        resources.mkdir(parents=True)
        info = {'CFBundleName': 'Axe Studio', 'CFBundleDisplayName': 'Axe Studio',
                'CFBundleIdentifier': 'org.axestudio.native', 'CFBundleExecutable': executable,
                'CFBundleVersion': '0.1.0', 'CFBundlePackageType': 'APPL',
                'NSHighResolutionCapable': True, 'LSMinimumSystemVersion': '12.0',
                'NSMicrophoneUsageDescription': (
                    'Axe Studio records user-started guitar samples for local analysis. When a sample is attached, the connected agent may request up to 30 seconds of its audio.'
                )}
        (app / 'Contents/Info.plist').write_bytes(plistlib.dumps(info))
    else:
        app = stage / 'Axe Studio'
        binary_dir = app
        resources = app / 'resources'
        resources.mkdir(parents=True)
    shutil.copy2(root / 'target' / profile / executable, binary_dir / executable)
    shutil.copytree(worker, resources / 'audio-worker/axe-audio-worker', symlinks=True)
    shutil.copytree(root / 'src/resources/axefx-live', resources / 'axefx-live')
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    dirty = bool(subprocess.check_output(
        ['git', 'status', '--porcelain', '--untracked-files=all'], text=True
    ).strip())
    manifest = {'version': 1, 'commit': commit, 'implementation_dirty': dirty,
                'platform': sys.platform, 'architecture': platform.machine(), 'profile': profile,
                'signing': 'Internal build; no Developer ID signing or notarization',
                'hardware_writes': True, 'hardware_parameter_writes': False, 'hardware_save': True,
                'hardware_save_verification': 'User-operated store request; persistence awaits manual hardware acceptance',
                'python': platform.python_version(),
                'files': {str(p.relative_to(app)): hashlib.sha256(p.read_bytes()).hexdigest()
                          for p in sorted(app.rglob('*')) if p.is_file()}}
    (resources / 'CAPABILITIES.md').write_text((root / 'src/CAPABILITIES.md').read_text())
    if sys.platform == 'darwin':
        run('codesign', '--force', '--deep', '--sign', '-', str(app))
        run('codesign', '--verify', '--deep', '--strict', str(app))
    for metadata in stage.rglob('.DS_Store'):
        metadata.unlink()
    manifest['files'] = {str(p.relative_to(app)): hashlib.sha256(p.read_bytes()).hexdigest()
                         for p in sorted(app.rglob('*')) if p.is_file()}
    for document in ['MAC_HANDOFF.md', 'VERIFICATION.md', 'CAPABILITIES.md']:
        shutil.copy2(root / 'src' / document, stage / document)
    (stage / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    # ditto preserves macOS bundle symlinks and signatures in zip archives.
    archive = stage.with_suffix('.zip')
    if sys.platform == 'darwin':
        run('ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', str(stage), str(archive))
    else:
        shutil.make_archive(str(stage), 'zip', root_dir=stage.parent, base_dir=stage.name)
    checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_suffix('.zip.sha256').write_text(f'{checksum}  {archive.name}\n')
    dist = stage.parent
    keep = {stage, archive, archive.with_suffix('.zip.sha256')}
    for path in dist.iterdir():
        if path not in keep:
            shutil.rmtree(path) if path.is_dir() else path.unlink()
    print(json.dumps({'application': str(app), 'archive': str(archive), 'sha256': checksum}))


if __name__ == '__main__':
    main()
