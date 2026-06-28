class Msh < Formula
  desc "Rust 製の高速・軽量対話型シェル"
  homepage "https://github.com/m2lab/msh"
  version "0.7.2"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/m2lab/msh/releases/download/v0.7.2/msh-macos-aarch64"
      sha256 "REPLACE_AFTER_FIRST_RELEASE"
    else
      url "https://github.com/m2lab/msh/releases/download/v0.7.2/msh-macos-x86_64"
      sha256 "REPLACE_AFTER_FIRST_RELEASE"
    end
  end

  def install
    arch = Hardware::CPU.arm? ? "msh-macos-aarch64" : "msh-macos-x86_64"
    bin.install arch => "msh"
  end

  def caveats
    <<~EOS
      既定シェルにする場合:
        sudo sh -c 'echo #{opt_bin}/msh >> /etc/shells'
        chsh -s #{opt_bin}/msh

      設定: ~/.config/msh/config.toml
      詳細: brew info msh または docs/installation.md
    EOS
  end

  test do
    assert_match "ok", shell_output("#{bin}/msh -c 'echo ok'")
  end
end
