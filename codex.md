# codex.md

OpenAI Codex / その他のエージェント向けの指示は、ベンダー非依存の正典
[AGENTS.md](./AGENTS.md) を参照してください。

Codex は標準で `AGENTS.md` を読み込みます。このファイルは互換性のためのパススルーです。

主要コマンド（詳細は AGENTS.md）:

```bash
./scripts/check.sh        # fmt + clippy + test を一括検証
cd msh && cargo test      # テストのみ
```
