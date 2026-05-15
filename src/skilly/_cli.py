from __future__ import annotations

import sys

from ._bridge import run_cli


def main() -> None:
    raise SystemExit(run_cli(sys.argv[1:]))
