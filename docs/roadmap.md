# msh 開発ロードマップ

> 最終更新: 2026-06-28

---

## 現状（As-Is）

### 既存コードの問題点

現在の `msh/src/` は REPL の最小試作のみ。

| 項目 | 状態 |
|---|---|
| 外部コマンド実行 | △ `spawn` のみ（wait あり） |
| 組み込みコマンド | △ `cd`, `exit` のみ |
| パイプ / リダイレクト | ✗ 未実装 |
| 環境変数 | ✗ 未実装 |
| ジョブ制御 | ✗ 未実装 |
| 行編集 / 補完 | ✗ 素の `read_line` |
| エラーハンドリング | △ `eprintln` 散在 |
| テスト | △ prompt のみ |
| edition | 2018（古い） |

### Phase 0 の方針: クリーンスタート

**既存の `main.rs` / `prompt.rs` は参考程度に留め、Phase 0 で作り直す。**

残すもの:
- リポジトリ構成（`msh/` crate, Docker Compose）
- プロジェクト名 `msh`
- 基本的な開発フロー（`docker compose` + `cargo`）

捨てるもの:
- 現在の REPL 実装
- `colored` / `whoami` 依存（Phase 3 以降で必要に応じて再導入）
- edition 2018

---

## ロードマップ概要

```
Phase 0 ──► Phase 1 ──► Phase 2 ──► Phase 3 ──► Phase 4 ──► Phase 5 ──► Phase 6
 基盤        MVP         シェル        UX          性能        互換        拡張
 リセット    コア        機能          GUI的CUI    最適化      スクリプト   プラグイン
 1週          2-3週       2-3週         4-6週       2-3週       4週+        継続
```

---

## Phase 0: 基盤リセット

**目標**: クリーンな土台を作り、以降の開発速度を確保する。

### タスク

- [x] `main.rs` を空の REPL 骨格に置き換え
- [x] `prompt.rs` を削除（Phase 3 で再設計）
- [x] `Cargo.toml` を更新
  - edition = `"2021"`
  - 依存: `thiserror`
- [x] ディレクトリ構成を確定

```
msh/
├── src/
│   ├── main.rs          # エントリポイント
│   ├── shell.rs         # REPL ループ
│   ├── exec.rs          # コマンド実行
│   ├── builtins/        # 組み込みコマンド
│   │   mod.rs
│   │   cd.rs
│   │   exit.rs
│   │   pwd.rs
│   │   export.rs
│   │   echo.rs
│   └── error.rs         # エラー型
├── tests/
│   └── basic.rs
└── benches/             # Phase 4 で追加
```

- [x] `error.rs` に `thiserror` ベースの統一エラー型
- [x] CI 追加（`cargo test`, `cargo clippy`, `cargo fmt`）
- [x] README 更新（ビジョン・ビルド手順）

### 完了条件
- `cargo test` が通る
- 空ループ REPL が起動し `exit` で終了できる

---

## Phase 1: MVP — 最小実行可能シェル

**目標**: 「コマンドを打って実行できる」状態。

### タスク

- [x] REPL ループ（読み取り → 解析 → 実行 → 繰り返し）
- [x] トークナイザ（空白区切り + クォート対応）
- [x] 外部コマンド実行（`Command::status`）
- [x] 組み込みコマンド
  - [x] `exit [code]`
  - [x] `cd [dir]`
  - [x] `pwd`
  - [x] `echo [-n] args...`
  - [x] `export VAR=val`
- [x] `$?` 終了ステータス保持
- [x] 統合テスト（echo, cd, 外部コマンド）

### 完了条件
- `ls`, `echo hello`, `cd /tmp && pwd`, `exit` が動作
- 終了コードが正しく伝播

---

## Phase 2: シェル機能

**目標**: 日常使いに必要なシェル機能を揃える。

### タスク

- [x] パイプライン（`cmd1 | cmd2 | cmd3`）
- [x] リダイレクト
  - [x] `>`, `>>`, `<`, `2>`, `&>`
- [x] 環境変数
  - [x] 展開 `$VAR`, `${VAR}`
  - [x] 起動時 `.msh_env` / `.mshrc` 読み込み
- [x] 組み込みコマンド追加
  - [x] `source` / `.`
  - [x] `which`
  - [x] `alias`（メモリ内）
- [x] ジョブ制御（基本）
  - [x] フォアグラウンド / バックグラウンド `&`
  - [x] `Ctrl+C` シグナルハンドリング
- [x] ワイルドカード glob（`*`, `?`）

### 完了条件
- `ls | grep foo | wc -l` が動作
- `echo hello > /tmp/out.txt` が動作
- バックグラウンドジョブ `sleep 10 &` が動作

---

## Phase 3: UX — GUI 的 CUI（差別化の核心）

**目標**: Fish を超える触り心地。ここが msh の存在意義。

### 技術選定（候補）

| 用途 | 候補 crate | 備考 |
|---|---|---|
| 行編集 | [reedline](https://crates.io/crates/reedline) | Nushell 系、高機能 |
| 行編集 | [rustyline](https://crates.io/crates/rustyline) | 実績豊富、軽量 |
| ハイライト | 自前 + `nu-ansi-term` | トークン種別で色分け |
| 補完 | 自前エンジン | `$PATH` スキャン + キャッシュ |

> **推奨**: まず `rustyline` で PoC → 不足があれば `reedline` に移行

### タスク

- [x] 行エディタ統合（履歴、カーソル移動、削除）
- [x] シンタックスハイライト
  - コマンド / フラグ / パス / 演算子 / 文字列
- [x] Tab 補完
  - コマンド名（`$PATH`）
  - ファイルパス
  - 組み込みコマンド
- [x] インライン autosuggestion（履歴 + パスベース）
- [x] 履歴検索（Ctrl+R 強化版）
- [x] プロンプト再設計
  - git ブランチ表示
  - 終了コードに応じた色
  - 実行時間表示（閾値超え時）
- [x] 親切なエラーメッセージ
  - 「`cd` → ディレクトリが見つかりません: `{path}`」
  - 類似コマンド提案（「`sl` → `ls` ですか？」）

### 完了条件
- 初見ユーザーが README なしで Tab 補完・履歴・ハイライトを使える
- Fish との UX 比較で「同等以上」と主観評価

---

## Phase 4: 性能・省メモリ

**目標**: ウリを数字で証明する。

### タスク

- [x] ベンチマーク基盤（`criterion` + `hyperfine` スクリプト）
- [x] 計測項目
  - [x] 起動時間（コールド / ウォーム）
  - [x] アイドル RSS
  - [x] 補完レイテンシ
  - [x] パイプラインオーバーヘッド（パースベンチで近似）
- [x] 最適化
  - [x] `$PATH` キャッシュ（TTL + fingerprint）
  - [x] プロンプト描画の差分更新
  - [x] 履歴の上限設定
  - [x] 遅延初期化（対話機能のみ）
- [x] リリースビルド最適化
  - [x] `lto = true`, `codegen-units = 1`, `strip = true`, `opt-level = 3`
- [x] ベンチ結果を `docs/benchmarks.md` に記録

### 完了条件
- [x] 起動 < 30ms（release, ローカル SSD）
- [x] アイドル RSS < 8 MB（v0.7.2 で 5.92MB）
- [ ] Fish 比で起動 20% 以上速い（交互計測では zsh と同等、bash が最速）

---

## Phase 5: 互換性・設定（Zsh 同等）

**目標**: Zsh 並みの移行しやすさ。一般的な `.bashrc` / スクリプトの 80%+ が動く。

詳細: [compatibility.md](./compatibility.md)

### タスク

#### L2 構文
- [x] コマンドチェイン: `&&`, `||`, `;`
- [x] コマンド置換: `$()`, `` ` ``
- [x] クォート中の展開ルール（Bash 準拠・単一引用符リテラル）
- [x] 終了コード `$?`
- [x] ヒアドキュメント `<<EOF`（基本）

#### L3 スクリプト
- [x] 関数定義 `name() { ... }`
- [x] 制御構造: `if` / `for` / `while` / `case`
- [x] 配列（基本）
- [x] `local`, `return`

#### L4 移行
- [x] 設定ファイル `~/.config/msh/config.toml`
- [x] `.bashrc` / `.zshrc` / `.mshrc` 読み込み順序
- [x] `msh --compat bash|zsh` フラグ
- [x] 未対応構文の明示エラー + 回避策
- [x] 互換性テストスイート（代表スニペット 80%+）
- [x] 移行ガイド `docs/migration.md`

### 完了条件
- 代表 `.bashrc` スニペット通过率 ≥ 80%
- 設定ファイル 1 つで全カスタマイズ可能
- 未対応構文すべてに回避策を表示

---

## Phase 6: UX 強化・拡張

**目標**: Bash / Zsh / Fish を超える UX。初心者が迷わない。

### UX 強化（Fish 超え）
- [x] ファジー補完（コマンド・パス・git ブランチ）
- [x] 補完候補に説明テキスト（コマンドの用途）
- [x] 履歴検索プレビュー（Ctrl+R 強化 — 大文字小文字無視 + インラインプレビュー）
- [x] ディレクトリ文脈 autosuggestion（v0.7.3 — このディレクトリで実際に打ったコマンドを優先提案、無ければグローバル履歴フォールバック）
- [x] 空 Enter / `help` で操作ヒント
- [x] エラーに具体例・回避策（日本語/英語切替）
- [x] 初回起動オンボーディング（任意スキップ可）

### 拡張

- [x] プラグイン API 設計（[plugin-api.md](./plugin-api.md) — L1 スクリプト / L2 WASM 草案）
- [x] テーマシステム（config.toml `theme`）
- [x] 組み込み便利機能
  - [x] ディレクトリスタック（`pushd` / `popd`）
  - [x] セッション復元（`session_restore = true` → `session.json`）
  - [x] 外部ツール連携（atuin `history_backend`、fzf 的 `history -g`）
- [x] プラグイン読み込み（`~/.config/msh/plugins/*.msh`）
- [x] **L2 WASM PoC**（v0.7.4）: `plugin list` / `plugin run` + `~/.config/msh/plugins/<name>/plugin.toml` + 外部 `wasmtime` 委譲（[plugin-api.md](./plugin-api.md)）
- [ ] パッケージマネージャ（`msh install` — 将来）
- [ ] ドキュメントサイト

### AI 連携（差別化・[ai-integration.md](./ai-integration.md)）

bash/zsh/fish が持たないネイティブ AI 連携。核（軽量・低依存・安全）を壊さない設計。

- [x] **A-1 基盤**（v0.7.3）: `[ai]` 設定・curl 委譲クライアント（新規依存ゼロ・バイナリ +64KB）・プロバイダ抽象（Claude/OpenAI/Gemini）・依存なし JSON パーサ・`ai <prompt>` 表示専用組み込み（実行しない安全枠）
- [x] **ローカル/他 LLM**（v0.7.3）: Ollama ネイティブ（keyless）+ OpenAI 互換 `base_url`（LM Studio / llama.cpp / vLLM / groq / openrouter 等）・`api_key_env` 空でキー不要
- [x] **A-2**（v0.7.3）: NL→コマンド提案（`# 自然文`→入力欄へ挿入・編集可）／コマンド失敗の AI 解説／`explain`（直前 or 指定コマンド）
- [ ] A-3: AI autosuggestion フォールバック
- [x] **B-2**（v0.7.4）: エージェント安全実行 `msh --agent`（Safe/Caution/Destructive 分類・`--agent-dry-run`・`--agent-force`・構造化 JSON）
- [x] **B-3**（v0.7.4 PoC）: MCP サーバ `msh --mcp`（stdio JSON-RPC・`tools/call` → agent ゲート + `msh_run`）

---

## マイルストーン

| マイルストーン | Phase | 成果物 |
|---|---|---|
| **v0.1.0** | Phase 0-1 | 最小 REPL、組み込みコマンド |
| **v0.2.0** | Phase 2 | パイプ・リダイレクト |
| **v0.3.0** | Phase 3 | 対話 UX（ハイライト・補完） |
| **v0.4.0** | Phase 4 | 性能ベンチ公開 |
| **v0.6.0** | Phase 5-6 | 配列・ヒアドキュメント・UX 強化 |
| **v0.7.0** | Phase 6 | セッション復元・履歴強化・プラグイン API 設計 |
| **v0.7.1** | Phase 6 | L4 互換強化（`[[ ]]`・`set -e/-u`・算術展開・`$PIPESTATUS`・プレーン代入）、互換スコア自動計測 88% |
| **v0.7.2** | Phase 6 | L5 連想配列 `declare -A`、省メモリ化（バイナリ 816KB・依存 111） |
| **v0.7.3** | Phase 6 | パラメータ展開高度形（`${var:-}`/`#`/`%`/`//`/`^^`/部分文字列/間接参照）・`$RANDOM`、互換 98% |
| **v0.7.4** | Phase 6 | `${var:=}` 永続代入・`<( )` プロセス置換・WASM PoC・`--agent`/`--mcp`、互換 **100%** |
| **v1.0.0** | Phase 6 | 安定版、WASM プラグイン本番 ABI、エコシステム整備 |

---

## L4 互換強化（v0.7.1, 実装済）

- [x] `[[ ]]` 条件式（単項・文字列・数値・否定） — `src/cond.rs`
- [x] プレーン代入 `VAR=value`（`export` なし）
- [x] `A && B || C` 短絡チェイン修正
- [x] 空クォート保持（`[[ -n "$X" ]]`）
- [x] `set -e`（errexit）/ `set -u`（nounset）
- [x] 算術展開 `$(( ))`（四則・剰余・括弧・変数）
- [x] `$PIPESTATUS[n]`
- [x] 互換スコア自動計測（`scripts/compat-score.sh` → [compat-score.md](./compat-score.md)）
- [x] マルチシェルベンチ（`scripts/benchmark.sh` — bash/zsh/fish + hyperfine フォールバック）
- [x] サブコマンド引数補完（git/cargo/docker/npm の第 1 引数 — `src/complete.rs`）
- [ ] インライン `cmd; while ...; do ...; done`（前置コマンド付き複合文の同一行 — 複数行なら可）

## L5 高度構文・省メモリ（v0.7.2, 実装済）

- [x] 連想配列 `declare -A` / `typeset -A`
  - `m[key]=value` 代入、`${m[key]}` 取得、添字の変数展開 `${m[$k]}`
  - `${!m[@]}`（キー一覧）/ `${#m[@]}`（要素数）/ `${m[@]}`（全値）
  - 添字付き配列の要素代入 `arr[2]=z`（スパース埋め）
- [x] 省メモリ・軽量化（[memory-optimization.md](./memory-optimization.md)）
  - [x] `panic = "abort"`（バイナリ/RSS 削減）
  - [x] rustyline `default-features=false`＋regex 除去（Ctrl+R 大小無視は自前実装）
  - [x] `serde`/`serde_json` 廃止（session は自前シリアライズ）
  - [x] 展開ホットパスの env クローン除去
  - 結果: バイナリ **2.0MB→816KB**、依存 124→111、起動時 RSS 6.39→5.92MB

### L5 残課題
- [x] インライン複合文 `cmd; while ...; do ...; done`（前置コマンド付き・同一行） — v0.7.2+
- [x] パラメータ展開の高度形 — v0.7.3
  - `${var:-default}` / `${var:+alt}` / `${var:?msg}`（`:` 有無で空判定切替）
  - `${var#pat}` / `${var##pat}` / `${var%pat}` / `${var%%pat}`（glob 前後除去）
  - `${var/pat/rep}` / `${var//pat/rep}`（置換）/ `${var^^}` / `${var,,}`（大小）
  - `${var:offset:length}`（部分文字列）/ `${#var}`（長さ）/ `${!var}`（間接参照）
- [x] `$RANDOM` — v0.7.3（依存なしの xorshift）
- [ ] 連想配列の複合初期化 `declare -A m=([k]=v [k2]=v2)`
- [ ] 添字の算術評価 `${arr[i+1]}`
- [x] `${var:=default}` の永続代入 — v0.7.4
- [ ] `$SECONDS` / `$LINENO`
- [x] プロセス置換 `<( )` — v0.7.4（一時ファイル + subshell 出力）
- [ ] プロセス置換 `>( )`（bash 委譲）

## エコシステム整備方針（本番デフォルトシェル化に必要）

「日常移行が実用域」から「本番デフォルトシェル」へ進むには、機能だけでなく
**配布・拡張・信頼**の3軸を整える。優先度順:

| 軸 | 施策 | 状態 |
|---|---|---|
| 配布 | Homebrew tap / `cargo install` / GitHub Releases バイナリ | [x] CI + Formula 雛形 ([installation.md](./installation.md)) |
| 配布 | musl 静的リンク（Linux）・各 arch CI ビルド | [ ] Linux x86_64 Release のみ |
| 拡張 | WASM プラグイン ABI 確定 | [x] PoC（wasmtime 委譲・`plugin` 組み込み） |
| 拡張 | 補完定義の外部化 | [ ] |
| 信頼 | dotfiles 互換回帰 CI 常設 | [x] `scripts/dotfiles-compat.sh` + CI |
| 信頼 | インライン `cmd; while...done` | [x] v0.7.2+ |
| 信頼 | POSIX モード | [ ] |
| 周知 | ドキュメントサイト | [ ] |
| 周知 | `chsh` 手順 | [x] [installation.md](./installation.md) |

> 方針: **「軽量・安全・移行容易」を配布体験で証明する**。
> 816KB 単一バイナリ＋設定1ファイルという強みを、Homebrew/Release 配布で前面に出す。
> 本番デフォルト化は「互換回帰テストの CI 常設」と「WASM 拡張」が揃ってから v1.0 で訴求。

## 次のアクション

1. **互換(L5)** — `$SECONDS`/`$LINENO`、プロセス置換 `>( )`
2. **配布** — Homebrew tap・GitHub Releases バイナリ（CI でクロスビルド）
3. **拡張** — WASM WIT ABI 確定（PoC 完了済）
4. **信頼** — 実物 dotfiles コーパスの互換回帰を CI に常設
5. **AI** — A-3 autosuggestion フォールバック

```bash
# 開発環境起動
docker compose up -d

# Phase 0 完了確認
docker compose exec msh cargo test
docker compose exec msh cargo run
```

---

## リスクと対策

| リスク | 影響 | 対策 |
|---|---|---|
| 行エディタ crate の機能不足 | Phase 3 遅延 | 早期 PoC（Phase 1 並行） |
| POSIX 互換の深い穴 | Phase 5 膨張 | 互換範囲を明示的に限定 |
| 一人開発の速度限界 | 全体遅延 | Phase 3 までを MVP と定義 |
| Fish との UX 差が出ない | 差別化失敗 | Phase 4 の速度で差別化 |
