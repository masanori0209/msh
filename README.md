# msh

Rust 製の対話型シェル。速度・省メモリ・GUI 的な CUI 体験を目指す。

詳細は [docs/](./docs/) を参照。AI コーディングエージェント向けの指示は [AGENTS.md](./AGENTS.md)。

## 開発環境

Docker Compose を使う。

```bash
docker compose up -d
```

## ビルド

```bash
docker compose exec msh cargo build
docker compose exec msh cargo build --release
```

## 実行

```bash
docker compose exec msh cargo run
# または
docker compose exec msh /tmp/target/debug/msh
```

## テスト / 検証

検証ループ（fmt + clippy + test）は 1 コマンドで実行できる。

```bash
./scripts/check.sh          # fmt --check + clippy -D warnings + test
./scripts/check.sh --fix    # 先に整形してから検証
./scripts/check.sh --bench  # ベンチも実行
```

個別に実行する場合:

```bash
cd msh
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

## 現状（v0.7.2）

- Phase 5–6 の主要機能（配列・ヒアドキュメント・UX 強化・セッション復元・履歴強化）
- L4 互換: `[[ ]]`・`set -e/-u`・算術展開 `$(( ))`・`$PIPESTATUS`・プレーン代入
- L5 互換: 連想配列 `declare -A`（`${m[k]}`・`${!m[@]}`・`${#m[@]}`）・添字要素代入 `arr[i]=v`
- 互換スコア **91% (32/35)**（`scripts/compat-score.sh`、[docs/compat-score.md](./docs/compat-score.md)）
- 省メモリ: バイナリ **816KB**（regex 除去・serde 廃止）・依存 111・起動時 RSS ~5.9MB（[docs/memory-optimization.md](./docs/memory-optimization.md)）
- ウォーム起動 ~3.5ms（**zsh と同等**、bash が最速）
- 競合ポジション: [docs/positioning-report.md](./docs/positioning-report.md)
- `MSH_SKIP_RC=1` — ベンチ用 rc スキップ

## ベンチマーク

```bash
cd msh && cargo build --release
cargo bench --bench shell_bench
./scripts/compat-score.sh   # 互換スコア計測 → docs/compat-score.md
./scripts/benchmark.sh      # 起動ベンチ（hyperfine 無ければ簡易計測）
```

## エージェント開発ハーネス

| ファイル | 用途 |
|---|---|
| [AGENTS.md](./AGENTS.md) | ベンダー非依存の正典（全エージェント共通） |
| [CLAUDE.md](./CLAUDE.md) / [codex.md](./codex.md) | 各ツール向けパススルー |
| `.cursor/rules/*.mdc` | Cursor プロジェクトルール（基本・Rust 規約・テスト・Git・ループ） |
| `scripts/check.sh` | 検証ループの単一ゲート |

## ロードマップ

[docs/roadmap.md](./docs/roadmap.md)
