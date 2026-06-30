class Skilly < Formula
  desc "Manage agent skills"
  homepage "https://github.com/xelandernt/skilly"
  version "0.0.32"
  license "MIT"

  livecheck do
    url :stable
    strategy :github_latest
  end

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.32/skilly-0.0.32-aarch64-apple-darwin.tar.gz"
      sha256 "168753f5bfb8cd53beaad39fae183337548b2494e06b077152f430cb86e4ff86"
    else
      url "https://github.com/xelandernt/skilly/releases/download/0.0.32/skilly-0.0.32-x86_64-apple-darwin.tar.gz"
      sha256 "6db888a347ff055185e69441333aff1cb52c8ff250c52dc77efbe390e180a52f"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.32/skilly-0.0.32-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "2cc700bf4ac68ab3d60ac1d2f5ee1df8233002cdce979abb2d79c8b4a89919d7"
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
