#!/bin/sh
# Prints the Homebrew formula for a welp release, using the checksums the
# release workflow published. Usage: packaging/brew-formula.sh 0.3.0
set -e
version="$1"
[ -n "$version" ] || { echo "usage: $0 <version>" >&2; exit 2; }
base="https://github.com/Oliv3rmoon/welp/releases/download/v$version"
sum() { curl -fsSL "$base/welp-v$version-$1.tar.gz.sha256" | cut -d' ' -f1; }
cat <<RUBY
class Welp < Formula
  desc "Fixes the command you just typed wrong. A fast successor to thefuck"
  homepage "https://github.com/Oliv3rmoon/welp"
  version "$version"
  license "MIT"

  on_macos do
    on_arm do
      url "$base/welp-v$version-aarch64-apple-darwin.tar.gz"
      sha256 "$(sum aarch64-apple-darwin)"
    end
    on_intel do
      url "$base/welp-v$version-x86_64-apple-darwin.tar.gz"
      sha256 "$(sum x86_64-apple-darwin)"
    end
  end

  on_linux do
    on_arm do
      url "$base/welp-v$version-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "$(sum aarch64-unknown-linux-gnu)"
    end
    on_intel do
      url "$base/welp-v$version-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "$(sum x86_64-unknown-linux-gnu)"
    end
  end

  def install
    bin.install "welp"
  end

  def caveats
    <<~EOS
      Add welp to your shell (zsh shown; also bash, fish, tcsh, powershell):
        eval "\$(welp init zsh)"
      Coming from thefuck? This moves your alias and settings over:
        welp migrate
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/welp --version")
    assert_equal "git status", shell_output("#{bin}/welp fix 'gti status' --yes 2>/dev/null").strip
  end
end
RUBY
