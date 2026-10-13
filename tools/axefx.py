#!/usr/bin/env python3
"""Run the repository-local Axe-Fx CLI directly, without installing an app."""
import sys
from pathlib import Path

if sys.version_info < (3, 11):
    raise SystemExit('Use Python 3.11 or newer; the USB dependency is tested with Python 3.11.')

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'src'))

from axefx_cli.cli import main


if __name__ == '__main__':
    raise SystemExit(main())
