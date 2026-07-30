class Skilly < Formula
  desc "Manage agent skills"
  homepage "https://github.com/xelandernt/skilly"
  version "0.0.35"
  license "MIT"

  livecheck do
    url :stable
    strategy :github_latest
  end

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.35/skilly-0.0.35-aarch64-apple-darwin.tar.gz"
      sha256 "9f684b35928a14f20a3335a09ffba4918c6b313b56a710cc48e918b6fead7d09"
    else
      url "https://github.com/xelandernt/skilly/releases/download/0.0.35/skilly-0.0.35-x86_64-apple-darwin.tar.gz"
      sha256 "fe213e07d22fb2b1697b50750f0edd42e67c7d286ac9d4e219e004459028049b"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.35/skilly-0.0.35-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "415a4933ad86832ba4a77ec247d1ddc279bbc5457ed810a0da78478a4f1bd99f"
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
