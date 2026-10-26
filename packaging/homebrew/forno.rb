# Homebrew formula template. Publish in a tap (e.g. OWNER/homebrew-tap) and
# fill in the release URLs and sha256 values after tagging a release.
class Forno < Formula
  desc "Bake plain HTML into fast static sites (single-binary SSG)"
  homepage "https://github.com/OWNER/forno"
  version "0.1.0"
  license "ISC"

  on_macos do
    on_arm do
      url "https://github.com/OWNER/forno/releases/download/v0.1.0/forno-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_ME"
    end
    on_intel do
      url "https://github.com/OWNER/forno/releases/download/v0.1.0/forno-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_ME"
    end
  end
  on_linux do
    on_arm do
      url "https://github.com/OWNER/forno/releases/download/v0.1.0/forno-aarch64-unknown-linux-musl.tar.gz"
      sha256 "REPLACE_ME"
    end
    on_intel do
      url "https://github.com/OWNER/forno/releases/download/v0.1.0/forno-x86_64-unknown-linux-musl.tar.gz"
      sha256 "REPLACE_ME"
    end
  end

  def install
    bin.install "forno"
  end

  test do
    system bin/"forno", "init", "--name", "Test"
    system bin/"forno", "check"
  end
end
