# msh 移行ガイド

Bash / Zsh から msh へ移行するための手順。

---

## クイックスタート

```bash
# Bash 互換モードで起動（~/.bashrc を読み込む）
msh --compat bash

# Zsh 互換モード
msh --compat zsh

# 通常モード（~/.mshrc のみ）
msh
```

---

## 設定ファイル

`~/.config/msh/config.toml`:

```toml
compat = "msh"       # msh | bash | zsh
load_bashrc = false
load_zshrc = false
```

`--compat` フラグは起動時に上書きします。環境変数 `MSH_COMPAT=bash` でも指定可能です。

---

## 読み込み順序

1. `~/.msh_env` — 環境変数
2. `~/.bashrc` — `--compat bash` または `load_bashrc = true` のとき
3. `~/.zshrc` — `--compat zsh` または `load_zshrc = true` のとき
4. `~/.mshrc` — msh 固有設定
5. `./.mshrc` — プロジェクトローカル

---

## 対応済み構文（L2–L3）

| 構文 | 例 |
|---|---|
| チェイン | `cmd1 && cmd2`, `cmd1 \|\| cmd2`, `cmd1; cmd2` |
| コマンド置換 | `$(echo hi)`, `` `echo hi` `` |
| 終了コード | `$?` |
| 関数 | `name() { ... }` |
| 制御構造 | `if/then/fi`, `for/in/do/done`, `while/do/done`, `case/in/esac` |
| 配列 | `arr=(a b c)`, `${arr[0]}`, `${arr[@]}`, `${#arr[@]}` |
| ヒアドキュメント | `cat <<EOF ... EOF` |
| ローカル変数 | `local VAR=val` |
| 関数からの戻り | `return [code]` |

## UX（v0.6）

| 機能 | 設定 |
|---|---|
| ファジー補完 | `fuzzy_completion = true` |
| 日本語エラー | `language = "ja"` または `MSH_LANG=ja` |
| テーマ | `theme = "default"` / `"minimal"` |
| プラグイン | `~/.config/msh/plugins/*.msh` |
| ヘルプ | `help` / 空 Enter |

---

## 未対応構文と回避策

| 未対応 | 回避策 |
|---|---|
| `[[ ... ]]` | `[ ... ]` または `if command; then` |
| `$(( ... ))` | `expr` や `bc` を使う |
| `<()` / `>()` | 名前付きパイプや一時ファイル |
| 連想配列 | Bash に委譲: `bash -c '...'` |

未対応構文を実行すると、機能名と回避策付きのエラーが表示されます。

---

## よくある .bashrc パターン

```bash
# ✅ 動作
export PATH="$HOME/bin:$PATH"
alias ll='ls -la'
[ -f ~/.bashrc.local ] && source ~/.bashrc.local
cd /tmp && git status
for f in *.txt; do echo "$f"; done

# ⚠️ 要変更
[[ -f file ]] && echo ok    # → [ -f file ] && echo ok
value=$((1 + 2))            # → value=$(expr 1 + 2)
```

---

## テスト

```bash
cargo test --test compat
```

代表スニペットの互換性テストスイートが `msh/tests/compat.rs` にあります。
