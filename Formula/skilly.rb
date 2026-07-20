class Skilly < Formula
  desc "Manage agent skills"
  homepage "https://github.com/xelandernt/skilly"
  version "0.0.34"
  license "MIT"

  livecheck do
    url :stable
    strategy :github_latest
  end

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.34/skilly-0.0.34-aarch64-apple-darwin.tar.gz"
      sha256 "607eab8a306f790c25b18985ef3a7edf14b1ee7a8675c74d48e5618f82d19a15"
    else
      url "https://github.com/xelandernt/skilly/releases/download/0.0.34/skilly-0.0.34-x86_64-apple-darwin.tar.gz"
      sha256 "4782023ccbd8e7e6de03fa5704ef886d2f4d4e58954546398bc84d3d91ec7c2e"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.34/skilly-0.0.34-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "195f3568971057b1fe2465e747757c2d0ac75f3b7e2b02722285b283663ddda2"
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
