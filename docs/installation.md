# msh インストール

> バージョン: v0.7.4

## クイックスタート

インストール後、**初回セットアップ**を実行:

```bash
msh setup          # 対話式（config.toml + 任意で IDE MCP 連携）
msh setup --yes --skip-integrations   # 非対話（config のみ）
msh doctor         # 設定・agent ゲート・MCP の健全性チェック
```

`msh` を初めて対話起動すると、オンボーディングで `setup` / `doctor` も案内されます。

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
msh setup    # ~/.config/msh/config.toml を生成（推奨）
# 手動の場合:
mkdir -p ~/.config/msh
# テンプレートは msh setup 出力、または docs/compatibility.md の default_config_template
```

## 検証

```bash
./scripts/check.sh              # 開発者向け
./scripts/compat-score.sh       # 互換スコア
./scripts/dotfiles-compat.sh    # dotfiles 回帰
./scripts/verify-mcp.sh         # Cursor MCP smoke test
```

## Cursor MCP（`msh --mcp`）

プロジェクトに `.cursor/mcp.json` が同梱されています。初回は msh をビルドしてください。

```bash
cd msh && cargo build
./scripts/verify-mcp.sh   # 緑なら Cursor Settings → MCP で msh を確認
```

詳細: [agent-shell-positioning.md](./agent-shell-positioning.md) §5.3 · [agent-integration.md](./agent-integration.md)（Cursor / Claude Code / Codex / OpenClaw 設定例）

## 関連

- [positioning-report.md](./positioning-report.md)
- [memory-optimization.md](./memory-optimization.md)
- [roadmap.md](./roadmap.md) — エコシステム整備方針
