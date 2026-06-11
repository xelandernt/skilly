class Skilly < Formula
  desc "Manage agent skills"
  homepage "https://github.com/xelandernt/skilly"
  version "0.0.28"
  license "MIT"

  livecheck do
    url :stable
    strategy :github_latest
  end

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.28/skilly-0.0.28-aarch64-apple-darwin.tar.gz"
      sha256 "d3c57bbba35227bba5d1336631a50c9ad6b18df58ce91474ac6c168825682cb7"
    else
      url "https://github.com/xelandernt/skilly/releases/download/0.0.28/skilly-0.0.28-x86_64-apple-darwin.tar.gz"
      sha256 "97d4f7aac5d1442eaa5e283cf924d89d07e4e7fcb886068c95408f5bc01a2112"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.28/skilly-0.0.28-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "faba62c940eebe8abf2f738263f854cf3ea40c9b7a3a0705c3e5f04cf6db276b"
    else
      odie "skilly Homebrew packages currently support Linux x64 only"
    end
  end

  def install
    bin.install "skilly"
  end

  test do
    skills_dir = testpath/"skills"
    instructions = "# Instructions\n\nTest the Homebrew installation."

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

    list_output = shell_output("#{bin}/skilly list --directory #{skills_dir}")
    assert_match "sample-skill", list_output
  end
end
