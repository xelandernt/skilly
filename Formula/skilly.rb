class Skilly < Formula
  desc "Manage agent skills"
  homepage "https://github.com/xelandernt/skilly"
  version "0.0.33"
  license "MIT"

  livecheck do
    url :stable
    strategy :github_latest
  end

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.33/skilly-0.0.33-aarch64-apple-darwin.tar.gz"
      sha256 "db4a8d0abda8a60be88480f00fe128abff3f854b9d4c493c65198572a108e310"
    else
      url "https://github.com/xelandernt/skilly/releases/download/0.0.33/skilly-0.0.33-x86_64-apple-darwin.tar.gz"
      sha256 "7b9a6f814302ba7daa2b71c1a50180252f5cca2c7450bf010937fb708bca9f38"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.33/skilly-0.0.33-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "319a9b7487648ee72b6121df42dfcaaf496b619a7125e193bfdeb307dc8cf9b0"
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
