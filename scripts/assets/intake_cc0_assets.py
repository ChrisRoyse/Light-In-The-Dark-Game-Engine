#!/usr/bin/env python3
"""Seed the external Light in the Dark CC0 asset vault.

This stable entrypoint delegates to focused modules under ``asset_intake``.
The intake contract stays fail-loud: only allowlisted CC0/permissive sources,
manifest-backed provenance, exact byte/hash verification, and no guessed asset
URLs.
"""

from __future__ import annotations

import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from asset_intake.cli import main, parse_args  # noqa: E402
from asset_intake.storage import assert_under  # noqa: E402

__all__ = ["assert_under", "main", "parse_args"]


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
