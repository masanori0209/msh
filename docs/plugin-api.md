# msh プラグイン API

> バージョン: v0.7.0（Phase 6）

## 概要

msh は 2 段階の拡張モデルを採用しています。

| 段階 | 方式 | 状態 |
|---|---|---|
| **L1 スクリプトプラグイン** | `~/.config/msh/plugins/*.msh` | ✅ 利用可能 |
| **L2 WASM プラグイン** | 動的モジュール + サンドボックス | 📋 設計中（v1.0 目標） |

---

## L1: スクリプトプラグイン（現行）

### 読み込み順

1. `~/.msh_env`
2. `~/.config/msh/config.toml`
3. 互換 rc（`--compat bash|zsh` 時）
4. `~/.mshrc`
5. **`~/.config/msh/plugins/*.msh`**（ファイル名順）
6. `./.mshrc`

### プラグイン例

`~/.config/msh/plugins/git-aliases.msh`:

```bash
alias gs='git status'
alias gc='git commit'
export MSH_PLUGIN_GIT=1
```

### 制約

- シェル本体と同一プロセスで実行（信頼できるコードのみ配置）
- 組み込み・関数・alias・export の定義が主用途
- 未対応構文は [compatibility.md](./compatibility.md) を参照

---

## L2: WASM プラグイン（将来）

### 設計方針（草案）

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│  msh core   │────►│  plugin host │────►│ WASM module │
│  (Rust)     │     │  (wit ABI)   │     │  (sandbox)  │
└─────────────┘     └──────────────┘     └─────────────┘
```

- **WIT インターフェース**: 補完候補、プロンプトセグメント、組み込みコマンド登録
- **サンドボックス**: ファイルシステム・ネットワークは明示許可のみ
- **配布**: `msh install <name>`（Phase 6 将来 — レジストリ未実装）

### 想定 API（未実装）

| フック | 用途 |
|---|---|
| `on_complete` | カスタム Tab 補完 |
| `on_prompt` | プロンプト右側情報 |
| `register_builtin` | 新組み込みコマンド |
| `on_preexec` / `on_precmd` | 実行前後フック |

---

## 外部ツール連携

### atuin（履歴）

`~/.config/msh/config.toml`:

```toml
history_backend = "atuin"
```

対話起動時に `atuin init -s msh` を評価します。atuin 未インストール時はエラーを表示します。

### fzf 的フィルタ

組み込み `history` で代替:

```bash
history -g cargo    # cargo を含む履歴
history -n 50       # 直近 50 件
```

---

## パッケージマネージャ（将来）

```bash
msh install starship   # 未実装
```

v1.0 までに WASM レジストリと合わせて検討します。
