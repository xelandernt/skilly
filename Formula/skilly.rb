class Skilly < Formula
  desc "Manage agent skills"
  homepage "https://github.com/xelandernt/skilly"
  version "0.0.30"
  license "MIT"

  livecheck do
    url :stable
    strategy :github_latest
  end

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.30/skilly-0.0.30-aarch64-apple-darwin.tar.gz"
      sha256 "7934b73c90ca7b4358488cd41c47ac6f6009ee0351c28d7ae1266d25f4a6ff18"
    else
      url "https://github.com/xelandernt/skilly/releases/download/0.0.30/skilly-0.0.30-x86_64-apple-darwin.tar.gz"
      sha256 "37fe67292903d0f808ecb6c41203317f12b9fb8eb126cd7b92f7aeb7b77c4816"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.30/skilly-0.0.30-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "6b323b1a89e17d9282564c99da4c0d408870b6886615bbd123cfdb99cdf5a977"
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
