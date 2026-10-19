#!/bin/sh
set -eu
if [ "$(uname -s)" != Darwin ]; then
    echo 'This package builder requires macOS.' >&2
    exit 1
fi
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
exec python3 "$root/src/tools/package.py" "$@"
