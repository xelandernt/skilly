class Skilly < Formula
  desc "Manage agent skills"
  homepage "https://github.com/xelandernt/skilly"
  version "0.0.36"
  license "MIT"

  livecheck do
    url :stable
    strategy :github_latest
  end

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.36/skilly-0.0.36-aarch64-apple-darwin.tar.gz"
      sha256 "085b1575841dc30a5dd1686550a93b9422ee6cc5ef01971f1af2b02eaab38f15"
    else
      url "https://github.com/xelandernt/skilly/releases/download/0.0.36/skilly-0.0.36-x86_64-apple-darwin.tar.gz"
      sha256 "0cf3b98250a362bf14efe7506f711178506fb0eccb04a01aca0ee93ccd024010"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.36/skilly-0.0.36-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "26c6b8c0fde6a9ece3a1b7f24b5dcdb144d29bbd7e77c640c9385b1b07097631"
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
