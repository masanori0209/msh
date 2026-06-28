# CLAUDE.md

このプロジェクトのエージェント向け指示は [AGENTS.md](./AGENTS.md) に集約しています。

@AGENTS.md

---

## Claude Code 固有の補足

- 大きな変更の前に `TodoWrite` でタスクを分解し、1 つずつ `in_progress` → `completed` に進める。
- `msh/src/shell.rs` は大きいので、編集前に対象範囲を読んでから変更する。
- 検証は必ず `./scripts/check.sh` を 1 コマンドで回す（fmt + clippy + test）。
- 計測値（RSS・ベンチ）を文書に書くときは推測せず、実際にコマンドを実行して裏取りする。
