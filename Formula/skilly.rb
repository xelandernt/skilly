class Skilly < Formula
  desc "Manage agent skills"
  homepage "https://github.com/xelandernt/skilly"
  version "0.0.31"
  license "MIT"

  livecheck do
    url :stable
    strategy :github_latest
  end

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.31/skilly-0.0.31-aarch64-apple-darwin.tar.gz"
      sha256 "72cf9bc7f748a4d8d237dede3221498cea0b23a35624933565b0d9a65cb19b17"
    else
      url "https://github.com/xelandernt/skilly/releases/download/0.0.31/skilly-0.0.31-x86_64-apple-darwin.tar.gz"
      sha256 "8e5eeea84c49d82299046325ccea1452b63bb3b7fedaf0e883cf2176d259d702"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.31/skilly-0.0.31-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "2d94046f249109003e3c8ecda7793ab27610b8f8d330fa49b16c64004c570c5d"
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
