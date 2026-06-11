class Skilly < Formula
  desc "Manage agent skills"
  homepage "https://github.com/xelandernt/skilly"
  version "0.0.29"
  license "MIT"

  livecheck do
    url :stable
    strategy :github_latest
  end

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.29/skilly-0.0.29-aarch64-apple-darwin.tar.gz"
      sha256 "24a149d09f6f634eec3eb02a339047a8cf25f51e547908f9abc3478b9920ad52"
    else
      url "https://github.com/xelandernt/skilly/releases/download/0.0.29/skilly-0.0.29-x86_64-apple-darwin.tar.gz"
      sha256 "0b1ef8243fc2beead4a777554eda8f4f8c3b9cec0992788ad005683575cc0f2f"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/xelandernt/skilly/releases/download/0.0.29/skilly-0.0.29-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0989f5baa7b888b45a7239d86719934f1babfc9f621d9f0779dd29664c3e548e"
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
