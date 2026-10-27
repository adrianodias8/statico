# Homebrew formula template. Publish in a tap (e.g. adrianodias8/homebrew-tap) and
# fill in the release URLs and sha256 values after tagging a release.
class Statico < Formula
  desc "Plain HTML into fast static sites (single-binary SSG)"
  homepage "https://github.com/adrianodias8/statico"
  version "0.1.0"
  license "ISC"

  on_macos do
    on_arm do
      url "https://github.com/adrianodias8/statico/releases/download/v0.1.0/statico-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_ME"
    end
    on_intel do
      url "https://github.com/adrianodias8/statico/releases/download/v0.1.0/statico-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_ME"
    end
  end
  on_linux do
    on_arm do
      url "https://github.com/adrianodias8/statico/releases/download/v0.1.0/statico-aarch64-unknown-linux-musl.tar.gz"
      sha256 "REPLACE_ME"
    end
    on_intel do
      url "https://github.com/adrianodias8/statico/releases/download/v0.1.0/statico-x86_64-unknown-linux-musl.tar.gz"
      sha256 "REPLACE_ME"
    end
  end

  def install
    bin.install "statico"
  end

  test do
    system bin/"statico", "init", "--name", "Test"
    system bin/"statico", "check"
  end
end
