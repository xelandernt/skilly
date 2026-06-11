#!/usr/bin/env python3

from __future__ import annotations

import re
import sys
from pathlib import Path


def update_formula(version: str, checksum: str) -> None:
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError(f"Unsupported version format: {version}")
    if not re.fullmatch(r"[0-9a-f]{64}", checksum):
        raise ValueError("Checksum must be a 64-character lowercase SHA256 hex digest")

    formula_path = Path(__file__).resolve().parents[1] / "Formula" / "skilly.rb"
    content = formula_path.read_text(encoding="utf-8")

    updated = re.sub(
        r'^  version "[^"]+"$',
        f'  version "{version}"',
        content,
        flags=re.MULTILINE,
    )
    updated = re.sub(
        r'(^  url "https://github\.com/xelandernt/skilly/archive/refs/tags/)[^"]+(\.tar\.gz"$)',
        rf'\g<1>{version}\g<2>',
        updated,
        flags=re.MULTILINE,
    )
    updated = re.sub(
        r'^  sha256 "[0-9a-f]{64}"$',
        f'  sha256 "{checksum}"',
        updated,
        flags=re.MULTILINE,
    )

    if updated == content:
        return

    formula_path.write_text(updated, encoding="utf-8")


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print("usage: update_homebrew_formula.py <version> <sha256>", file=sys.stderr)
        return 2

    update_formula(argv[1], argv[2])
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
