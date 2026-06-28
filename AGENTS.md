# AGENTS.md

> msh — Rust 製の対話型シェル。速度・省メモリ・GUI 的 CUI 体験を目指す。
> このファイルは AI コーディングエージェント向けの正典です。人間向けは [README.md](./README.md)。

## プロジェクト概要

- **言語/エディション**: Rust (edition 2021)、ツールチェイン stable
- **crate**: `msh/`（lib `msh` + bin `msh`）
- **現行バージョン**: v0.7.2（Phase 6 進行中、L5 連想配列・省メモリ化済）
- **設計ドキュメント**: [docs/roadmap.md](./docs/roadmap.md) / [docs/compatibility.md](./docs/compatibility.md) / [docs/positioning-report.md](./docs/positioning-report.md)

## ディレクトリ構成

| パス | 役割 |
|---|---|
| `msh/src/shell.rs` | REPL・実行エンジン（最大ファイル。編集前に該当箇所を読む） |
| `msh/src/parse.rs` `expand.rs` `heredoc.rs` | 字句解析・展開・ヒアドキュメント |
| `msh/src/exec.rs` | プロセス起動・パイプ・リダイレクト |
| `msh/src/builtins/` | 組み込みコマンド（`mod.rs` の `NAMES` に登録が必要） |
| `msh/src/config.rs` | `config.toml` パース・設定 |
| `msh/src/line_editor.rs` `complete.rs` `highlight.rs` `hints.rs` | 対話 UX |
| `msh/tests/` | 統合テスト（`basic.rs`・`compat.rs`） |
| `msh/benches/` | Criterion ベンチ |
| `scripts/` | `check.sh`（検証ループ）・`benchmark.sh`（hyperfine） |

## コマンド（必ずこの正確なコマンドを使う）

作業は `msh/` ディレクトリ基準。**ローカル `cargo` が最も安定**（Docker は rustc 1.85 で home crate 問題あり）。

```bash
# 検証ループ（コミット前は必ずこれをパスさせる）
./scripts/check.sh                 # fmt --check + clippy -D warnings + test を一括実行

# 個別
cd msh
cargo fmt                          # 整形（編集後に実行）
cargo clippy -- -D warnings        # lint（警告ゼロが必須）
cargo test                         # 全テスト
cargo test --test compat           # 互換テストのみ
cargo test <name>                  # 単一テスト（例: cargo test heredoc_inline）
cargo build --release              # リリースビルド
cargo bench --bench shell_bench    # マイクロベンチ
```

Docker を使う場合（ユーザールール: Docker Compose があれば優先）:

```bash
docker compose up -d
docker compose exec msh cargo test
```

## コード規約（言語デフォルトと異なる点のみ）

- **clippy 警告ゼロ**: `-D warnings` でゲートしている。`#[allow(...)]` は理由コメント必須。
- **エラー型**: `crate::error::MshError` に集約。`?` で伝播。新エラーは `error.rs` に variant 追加。外部クレート由来エラーは `.map_err(|e| MshError::ScriptError(e.to_string()))` 等で変換（`serde_json` には `From` 未実装）。
- **コメント**: コードを言い換えるだけのコメントは書かない。非自明な意図・制約のみ。
- **組み込み追加時**: `builtins/<name>.rs` 作成 → `builtins/mod.rs` の `NAMES` と `run` に登録 → 必要なら `descriptions.rs`・`help.rs`・`needs_shell_context` も更新。
- **設定追加時**: `config.rs` の `ShellConfig` フィールド + `apply_toml` + `describe_config` + `default_config_template` を揃える。
- **新規モジュール**: `lib.rs` に `pub mod` 追加。

## テスト方針

- ロジック追加時は **ユニットテスト**（同ファイル `#[cfg(test)]`）を、ユーザー可視な挙動は **統合テスト**（`tests/`）を追加する。
- シェル挙動の互換性は `tests/compat.rs` に `run_c("...")` で 1 ケース追加（実バイナリを `-c` 起動して stdout/exit code を検証）。
- モックは使わない。実バイナリ・実ファイルシステム（tempfile）で検証する。
- 期待値は具体的に（exit code と stdout の両方）。

## ループエンジニアリング（自己検証）

エージェントは「編集 → 検証 → 修正」を**緑になるまで自走**する。詳細は [.cursor/rules/loop-engineering.mdc](./.cursor/rules/loop-engineering.mdc)。

1. 変更後に必ず `./scripts/check.sh` を実行
2. 失敗したら原因を特定し修正、再実行（推測で終わらせない）
3. ベンチに影響しうる変更は `cargo bench` で回帰確認
4. ドキュメント（roadmap / benchmarks / positioning-report）も実態に合わせて更新

## Git / PR 規約

- **Conventional Commits**: `feat:` `fix:` `docs:` `refactor:` `test:` `chore:` `perf:` `bench:`
- コミットは**ユーザーが明示的に依頼したときのみ**作成する。
- PR 前に `./scripts/check.sh` がパスしていること。
- バージョンは `msh/Cargo.toml` で管理。機能追加時はマイルストーン（roadmap.md）も更新。

## 境界（触ってはいけない / 注意）

- `/tmp/target`・`target/` — ビルド成果物。コミットしない。
- `.msh_history`・`session.json`・`~/.config/msh/` — ユーザーデータ。テストで汚さない。
- `docs/competitive-analysis.md` `docs/vision.md` — 戦略文書。数値の捏造をしない（計測値はベンチ実行で裏取り）。
- 秘密情報（`.env` 等）はコミットしない。

## 応答言語

- ユーザーへの応答・コミットメッセージ本文・ドキュメントは **日本語**。
