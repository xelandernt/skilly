#!/usr/bin/env python3

from __future__ import annotations

import re
import sys
from pathlib import Path


def update_formula(
    version: str,
    linux_x64_checksum: str,
    macos_x64_checksum: str,
    macos_arm64_checksum: str,
) -> None:
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError(f"Unsupported version format: {version}")
    checksums = {
        "linux-x64": linux_x64_checksum,
        "macos-x64": macos_x64_checksum,
        "macos-arm64": macos_arm64_checksum,
    }
    for checksum in checksums.values():
        if not re.fullmatch(r"[0-9a-f]{64}", checksum):
            raise ValueError("Checksum must be a 64-character lowercase SHA256 hex digest")

    formula_path = Path(__file__).resolve().parents[1] / "Formula" / "skilly.rb"
    content = formula_path.read_text(encoding="utf-8")

    updated = f"""class Skilly < Formula
  desc "Manage agent skills"
  homepage "https://github.com/xelandernt/skilly"
  version "{version}"
  license "MIT"

  livecheck do
    url :stable
    strategy :github_latest
  end

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/xelandernt/skilly/releases/download/{version}/skilly-{version}-aarch64-apple-darwin.tar.gz"
      sha256 "{macos_arm64_checksum}"
    else
      url "https://github.com/xelandernt/skilly/releases/download/{version}/skilly-{version}-x86_64-apple-darwin.tar.gz"
      sha256 "{macos_x64_checksum}"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/xelandernt/skilly/releases/download/{version}/skilly-{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "{linux_x64_checksum}"
    else
      odie "skilly Homebrew packages currently support Linux x64 only"
    end
  end

  def install
    bin.install "skilly"
  end

  test do
    skills_dir = testpath/"skills"
    instructions = "# Instructions\\n\\nTest the Homebrew installation."

    system bin/"skilly",
      "create",
      "sample-skill",
      "--description",
      "Use when testing the Homebrew package.",
      "--instructions",
      instructions,
      "--directory",
      skills_dir
    assert_predicate skills_dir/"sample-skill/SKILL.md", :exist?

    list_output = shell_output("#{{bin}}/skilly list --directory #{{skills_dir}}")
    assert_match "sample-skill", list_output
  end
end
"""

    if updated == content:
        return

    formula_path.write_text(updated, encoding="utf-8")


def main(argv: list[str]) -> int:
    if len(argv) != 5:
        print(
            "usage: update_homebrew_formula.py <version> <linux-x64-sha256> <macos-x64-sha256> <macos-arm64-sha256>",
            file=sys.stderr,
        )
        return 2

    update_formula(argv[1], argv[2], argv[3], argv[4])
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
