# msh インストール

> バージョン: v0.7.2

## クイックスタート

### GitHub Releases（推奨）

[Releases](https://github.com/m2lab/msh/releases) から OS/arch に合うバイナリを取得:

```bash
# macOS (Apple Silicon) の例
curl -fsSL -o msh \
  https://github.com/m2lab/msh/releases/download/v0.7.2/msh-macos-aarch64
chmod +x msh
sudo install -m755 msh /usr/local/bin/msh
```

Linux x86_64: `msh-linux-x86_64`

### cargo install（Rust 環境がある場合）

```bash
cargo install --path msh --locked
# または crates.io 公開後:
# cargo install msh
```

### Homebrew（tap）

```bash
# リポジトリを tap として追加（初回のみ）
brew tap m2lab/msh https://github.com/m2lab/msh
brew install msh
```

ローカル Formula のみ試す場合:

```bash
brew install --formula Formula/msh.rb
```

> 初回リリース前は `Formula/msh.rb` の sha256 がプレースホルダです。
> `shasum -a 256` で Release バイナリのハッシュを更新してください。

## 既定シェル化（任意）

```bash
# msh を login shell 候補に登録
sudo sh -c 'echo /usr/local/bin/msh >> /etc/shells'
chsh -s /usr/local/bin/msh
```

**注意**: 本番デフォルト化前に `./scripts/dotfiles-compat.sh` で
代表 dotfiles パターンが通ることを確認してください。

## 設定

```bash
mkdir -p ~/.config/msh
# テンプレートは msh 初回起動時のオンボーディング、または:
# docs/compatibility.md の default_config_template を参照
```

## 検証

```bash
./scripts/check.sh              # 開発者向け
./scripts/compat-score.sh       # 互換スコア
./scripts/dotfiles-compat.sh    # dotfiles 回帰
```

## 関連

- [positioning-report.md](./positioning-report.md)
- [memory-optimization.md](./memory-optimization.md)
- [roadmap.md](./roadmap.md) — エコシステム整備方針
