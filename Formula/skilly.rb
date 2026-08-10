class Skilly < Formula
  desc "Manage agent skills"
  homepage "https://github.com/xelandernt/skilly"
  version "0.0.37"
  license "MIT"

  livecheck do
    url :stable
    strategy :github_latest
  end

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.37/skilly-0.0.37-aarch64-apple-darwin.tar.gz"
      sha256 "fcdeebea484e6c13d61cf798e5175c78b5192caddade2b66e190fe7a33ae641f"
    else
      url "https://github.com/xelandernt/skilly/releases/download/0.0.37/skilly-0.0.37-x86_64-apple-darwin.tar.gz"
      sha256 "8f32a8a7ef1109c288d4ad82006ef81e6fddc00a0690e450d8746eb66e29c5ca"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.37/skilly-0.0.37-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "ef36d9071c09ae2bbfc46da89d5e61a51d7cf31b32fdce26d54047abfe4ac68a"
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
