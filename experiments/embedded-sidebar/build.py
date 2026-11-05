"""Create an isolated local Axe-Edit copy with native embedded chat.

Never overwrites the installed app or an existing output directory.
The copied app uses local ad-hoc signatures; it is not a vendor release.
"""
import hashlib
import argparse
import json
from pathlib import Path
import plistlib
import subprocess


def run(*args):
    subprocess.run(args, check=True)


root = Path(__file__).resolve().parents[2]
source = Path('/Applications/Axe-Edit III.app')
parser = argparse.ArgumentParser()
parser.add_argument('--output', type=Path, default=root / '.local/axe-edit-native-chat')
args = parser.parse_args()
destination = args.output.resolve()
if destination.exists():
    raise SystemExit(f'Refusing to overwrite {destination}')
destination.mkdir(parents=True)
app = destination / 'Axe-Edit III Chat.app'
run('ditto', str(source), str(app))
binary = app / 'Contents/MacOS/Axe-Edit III'
original = binary.with_name('Axe-Edit III Original')
before = hashlib.sha256(binary.read_bytes()).hexdigest()
binary.rename(original)
library = app / 'Contents/Frameworks/AxeSidebarProof.dylib'
library.parent.mkdir(parents=True, exist_ok=True)
run('xcrun', 'clang++', '-dynamiclib', '-fobjc-arc', '-fblocks', '-std=c++17', '-x', 'objective-c++',
    '-arch', 'arm64', '-arch', 'x86_64', '-mmacosx-version-min=12.0',
    '-framework', 'Cocoa', str(Path(__file__).with_name('Sidebar.m')),
    str(Path(__file__).with_name('ChatAgent.mm')), str(Path(__file__).with_name('ChatPanel.mm')),
    '-o', str(library))
# A launcher sets the library path only for the copied executable.
binary.write_text('''#!/bin/sh
AXE_PROOF_MACOS_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
export DYLD_INSERT_LIBRARIES="$AXE_PROOF_MACOS_DIR/../Frameworks/AxeSidebarProof.dylib"
exec "$AXE_PROOF_MACOS_DIR/Axe-Edit III Original" "$@"
''')
binary.chmod(0o755)
info = app / 'Contents/Info.plist'
metadata = plistlib.loads(info.read_bytes())
metadata['CFBundleIdentifier'] = 'org.axestudio.axe-edit-chat'
metadata['CFBundleName'] = 'Axe-Edit III Chat'
metadata['CFBundleDisplayName'] = 'Axe-Edit III Chat'
info.write_bytes(plistlib.dumps(metadata))
# Remove inherited runtime restrictions only on the copy, then sign its code.
run('codesign', '--force', '--sign', '-', '--options', '0', str(original))
run('codesign', '--force', '--sign', '-', str(library))
run('codesign', '--force', '--deep', '--sign', '-', '--options', '0', str(app))
run('codesign', '--verify', '--deep', '--strict', str(app))
after = hashlib.sha256((source / 'Contents/MacOS/Axe-Edit III').read_bytes()).hexdigest()
if before != after:
    raise SystemExit('Installed binary identity unexpectedly changed')
manifest = {'app': str(app), 'installed_binary_sha256': before,
            'installed_binary_unchanged': before == after,
            'purpose': 'Native embedded chat using local Codex/Claude agents; no device-control or audio bridge',
            'note': 'Axe-Edit itself retains its normal device communication and user preferences.'}
(destination / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps(manifest, indent=2))
