# エージェント統合ガイド — Cursor / Claude Code / Codex / OpenClaw

> 最終更新: 2026-06-29 / 対象: msh v0.7.4+  
> 関連: [agent-shell-positioning.md](./agent-shell-positioning.md) · [ai-integration.md](./ai-integration.md) · [installation.md](./installation.md)

Coding agent から msh を **structured shell + 安全ゲート** として使うための設定例集。

---

## 0. 前提

### msh を入れる

```bash
cd msh && cargo build --release
sudo install -m755 target/release/msh /usr/local/bin/msh
chmod +x scripts/msh-mcp.sh scripts/msh-agent-shell.sh scripts/msh-agent-exec.sh
sudo install -m755 scripts/msh-agent-shell.sh /usr/local/bin/msh-agent-shell.sh
msh setup --yes --skip-integrations   # config.toml 生成
msh doctor                              # agent / MCP 検証
```

検証:

```bash
MSH_SKIP_RC=1 msh --json -c 'echo ok'
MSH_SKIP_RC=1 msh --agent -c 'rm -rf /tmp/x'   # → blocked
./scripts/verify-mcp.sh
```

### 3 つの統合レベル

| レベル | コマンド | 向く場面 |
|---|---|---|
| **L1 観測** | `msh --json -c '...'` | stdout/exit/duration を JSON で欲しい |
| **L2 安全** | `msh --agent -c '...'` | 破壊的操作を shell 層で block |
| **L3 MCP** | `msh --mcp` | IDE/CLI から `msh_run` ツール 1 本 |

**推奨**: IDE 連携は **L3 MCP**。subprocess shell 差し替えは **L2 + ラッパ**。

### 共通 env（任意）

| 変数 | 意味 |
|---|---|
| `MSH_SKIP_RC=1` | rc 読み込みをスキップ（エージェント向け） |
| `MSH_AGENT_BLOCK_CAUTION=1` | `rm` 等 caution も block |
| `MSH_AGENT_SANDBOX=/path/to/project` | cwd/cd を配下に制限 |
| `MSH_AGENT_SESSION=~/.config/msh/agent.session` | `-c` 間で cwd 永続 |
| `MSH_AGENT_AUDIT_LOG=~/.local/state/msh-agent.jsonl` | 監査ログ |

`~/.config/msh/config.toml` の `[agent]` でも同内容を設定可能（[agent-shell-positioning.md](./agent-shell-positioning.md) §8 参照）。

---

## 1. Cursor

Cursor Agent は **bash subprocess** と **MCP** のハイブリッド。**MCP 推奨**。

### 1.1 MCP（推奨 · Level 3）

#### プロジェクト同梱（本リポジトリ）

`.cursor/mcp.json` が同梱済み:

```json
{
  "mcpServers": {
    "msh": {
      "command": "${workspaceFolder}/scripts/msh-mcp.sh",
      "args": [],
      "env": { "MSH_SKIP_RC": "1" }
    }
  }
}
```

```bash
cd msh && cargo build
./scripts/verify-mcp.sh
```

Cursor → **Settings → MCP** で `msh` が緑 → **Reload Window**。

#### グローバル（全プロジェクト）

`~/.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "msh": {
      "command": "/usr/local/bin/msh",
      "args": ["--mcp"],
      "env": {
        "MSH_SKIP_RC": "1",
        "MSH_AGENT_AUDIT_LOG": "/Users/you/.local/state/msh-agent.jsonl"
      }
    }
  }
}
```

テンプレ: [examples/cursor-mcp.global.json](./examples/cursor-mcp.global.json)

#### サンドボックス付き

```json
{
  "mcpServers": {
    "msh": {
      "command": "/usr/local/bin/msh",
      "args": ["--mcp"],
      "env": {
        "MSH_SKIP_RC": "1",
        "MSH_AGENT_SANDBOX": "${workspaceFolder}",
        "MSH_AGENT_BLOCK_CAUTION": "1"
      }
    }
  }
}
```

> `${workspaceFolder}` は Cursor MCP で展開される。展開されない場合は絶対パスを指定。

### 1.2 Agent の shell コマンド（Level 2 · 上級）

Cursor の Agent が内部で bash を叩く経路は **製品側固定** のため、完全置換は不可。代わりに:

- **MCP `msh_run` を優先**させる（Rules / AGENTS.md で指示）
- ターミナル統合シェルを msh にする（人間操作用）

**Cursor Rules 例**（`.cursor/rules/msh-agent.mdc`）:

```markdown
---
description: shell 実行は msh MCP を優先
alwaysApply: true
---

- シェルコマンドは Cursor 内蔵 bash ではなく MCP ツール `msh_run` を使う
- 破壊的操作（rm -rf, git reset --hard）は dry_run で分類確認してから実行
- 大出力は msh JSON の truncate メタ（stdout_bytes）を確認する
```

### 1.3 検証

```bash
./scripts/verify-mcp.sh
# Cursor Agent に「msh_run で pwd を実行して」と依頼
```

---

## 2. Claude Code

Claude Code は **Bash ツール** + **MCP** + **`CLAUDE_CODE_SHELL`** で shell を制御。

### 2.1 MCP（推奨 · Level 3）

#### ユーザー設定 `~/.claude/settings.json`

```json
{
  "env": {
    "MSH_SKIP_RC": "1"
  },
  "mcpServers": {
    "msh": {
      "command": "/usr/local/bin/msh",
      "args": ["--mcp"],
      "env": {
        "MSH_SKIP_RC": "1"
      }
    }
  }
}
```

テンプレ: [examples/claude-settings.json](./examples/claude-settings.json)

#### プロジェクト設定 `.claude/settings.json`

```json
{
  "env": {
    "MSH_AGENT_SANDBOX": "/Users/you/project",
    "MSH_AGENT_BLOCK_CAUTION": "1"
  },
  "mcpServers": {
    "msh": {
      "command": "/path/to/project/scripts/msh-mcp.sh",
      "args": []
    }
  }
}
```

起動: `claude`（プロジェクトルートで）

### 2.2 Bash ツールの shell 差し替え（Level 2）

公式 env: [`CLAUDE_CODE_SHELL`](https://code.claude.com/docs/en/env-vars) — shell バイナリを上書き。

#### 方法 A: msh ラッパを shell に指定

```json
{
  "env": {
    "MSH_SKIP_RC": "1",
    "CLAUDE_CODE_SHELL": "/usr/local/bin/msh-agent-shell.sh"
  }
}
```

`scripts/msh-agent-shell.sh` を `/usr/local/bin/` にインストール済みであること。

#### 方法 B: SHELL_PREFIX で監査・安全実行

[`CLAUDE_CODE_SHELL_PREFIX`](https://code.claude.com/docs/en/env-vars) — 全 shell 起動の前にラッパを挟む。

`~/.claude/settings.json`:

```json
{
  "env": {
    "MSH_SKIP_RC": "1",
    "CLAUDE_CODE_SHELL_PREFIX": "/path/to/msh/scripts/msh-agent-exec.sh"
  }
}
```

ラッパは `$1` に Claude が組み立てた **shell 一行** を受け取り `msh --agent -c` へ転送。

> **注意**: Bash ツールの stdout は **JSON 1 行** になる。Claude はパースできるが、人間がターミナルで見る場合は `--json` ではなく MCP 利用を推奨。

### 2.3 CLAUDE.md / プロジェクト指示

```markdown
## Shell

- 可能なら MCP `msh_run` を使う（構造化 + 安全ゲート）
- 直接 Bash する場合は `cargo test` 等 read-only を優先
- `rm`, `git reset --hard` は `--agent-force` なしでは block される
```

### 2.4 制限

- **Windows**: `CLAUDE_CODE_SHELL` が無視される報告あり（Git Bash 固定）。Windows は MCP のみ現実的。
- msh の `--json` / `--agent` は **Unix のみ**。

---

## 3. OpenAI Codex（CLI / IDE）

Codex は **`~/.codex/config.toml`** + **`.codex/config.toml`**（trusted project）+ **`AGENTS.md`**。

### 3.1 MCP（推奨 · Level 3）

`~/.codex/config.toml`:

```toml
approval_policy = "on-request"
sandbox_mode = "workspace-write"

[mcp_servers.msh]
command = "/usr/local/bin/msh"
args = ["--mcp"]

[mcp_servers.msh.env]
MSH_SKIP_RC = "1"
MSH_AGENT_AUDIT_LOG = "/Users/you/.local/state/msh-agent.jsonl"
```

プロジェクト override `.codex/config.toml`:

```toml
[mcp_servers.msh.env]
MSH_AGENT_SANDBOX = "/absolute/path/to/this/repo"
MSH_AGENT_BLOCK_CAUTION = "1"
```

テンプレ: [examples/codex-config.toml](./examples/codex-config.toml)

### 3.2 shell 環境変数

```toml
[shell_environment_policy]
inherit = true
set = { MSH_SKIP_RC = "1", MSH_AGENT_JSON_MAX_BYTES = "65536" }
```

### 3.3 AGENTS.md に書く指示

リポジトリ `AGENTS.md`（Codex / Cursor 共通）:

```markdown
## Shell / msh

- シェル実行は MCP `msh_run` を優先する
- 検証: `MSH_SKIP_RC=1 msh --agent -c 'cargo test'`
- 破壊的コマンドは msh が block する。force が必要な場合は人間に確認
- 設定: `~/.config/msh/config.toml` の `[agent]` セクション
```

Codex の AGENTS.md 読み込み: [OpenAI Developers — AGENTS.md](https://developers.openai.com/codex/guides/agents-md)

### 3.4 プロファイル例

```toml
[profiles.safe-agent]
approval_policy = "on-request"
sandbox_mode = "workspace-write"

[profiles.safe-agent.shell_environment_policy]
set = { MSH_SKIP_RC = "1" }
```

```bash
codex --profile safe-agent
```

---

## 4. OpenClaw

OpenClaw は **`openclaw mcp set`** で MCP サーバを登録。設定は `~/.openclaw/openclaw.json`（または CLI が表示するパス）。

### 4.1 CLI で登録

```bash
openclaw mcp set msh '{
  "command": "/usr/local/bin/msh",
  "args": ["--mcp"],
  "env": {
    "MSH_SKIP_RC": "1"
  }
}'

openclaw mcp doctor msh --probe
openclaw mcp list
```

リポジトリ内ビルドを使う場合:

```bash
openclaw mcp set msh "{
  \"command\": \"$(pwd)/scripts/msh-mcp.sh\",
  \"args\": []
}"
```

### 4.2 設定 JSON 断片

`mcp.servers` に手書きする場合: [examples/openclaw-mcp.json](./examples/openclaw-mcp.json)

```json
{
  "mcp": {
    "servers": {
      "msh": {
        "command": "/usr/local/bin/msh",
        "args": ["--mcp"],
        "env": { "MSH_SKIP_RC": "1" },
        "enabled": true
      }
    }
  }
}
```

### 4.3 サンドボックス runtime

サンドボックス agent で MCP ツールを使う場合、`bundle-mcp` を許可:

```json
{
  "tools": {
    "sandbox": {
      "tools": {
        "alsoAllow": ["bundle-mcp"]
      }
    }
  }
}
```

公式: [OpenClaw MCP CLI](https://docs.openclaw.ai/cli/mcp)

### 4.4 逆方向（OpenClaw を MCP サーバとして公開）

Claude Code / Cursor から OpenClaw を叩く場合は `openclaw mcp serve`（OpenClaw 側）。msh 連携とは別経路。

---

## 5. その他（参考）

| ツール | 統合方法 | 備考 |
|---|---|---|
| **Windsurf** | `~/.codeium/windsurf/mcp_config.json` 等（Cursor と同型 MCP） | MCP `msh` エントリを追加 |
| **Cline / Roo Code** | VS Code `settings.json` の MCP セクション | stdio `msh --mcp` |
| **Aider** | `--shell-cmd` なし · 直接 subprocess | `msh --agent -c` を手動指定する程度 |
| **GitHub Copilot CLI** | 独自 sandbox | msh 非対応。参考のみ |

Windsurf MCP 例:

```json
{
  "mcpServers": {
    "msh": {
      "command": "/usr/local/bin/msh",
      "args": ["--mcp"],
      "env": { "MSH_SKIP_RC": "1" }
    }
  }
}
```

---

## 6. 製品別クイック選択

| あなたの環境 | 最初にやること |
|---|---|
| **Cursor** | `.cursor/mcp.json` + `./scripts/verify-mcp.sh` |
| **Claude Code** | `~/.claude/settings.json` に MCP + 任意で `CLAUDE_CODE_SHELL` |
| **Codex** | `~/.codex/config.toml` に `[mcp_servers.msh]` + AGENTS.md |
| **OpenClaw** | `openclaw mcp set msh ...` + `doctor --probe` |

---

## 7. ステートフル session（`-c` 毎起動 vs 連続作業）

| 方式 | 設定 | 効果 |
|---|---|---|
| **MCP** | 追加設定不要 | `msh --mcp` プロセス内で cwd 維持 |
| **session ファイル** | `--agent-session PATH` | 別プロセス間で cwd 永続 |
| **Claude Code** | Bash ツールはセッション cwd 維持 | MCP 併用時は msh 側 session が独立 |

```bash
# ファイル session 例
msh --agent-session ~/.config/msh/agent.session --agent -c 'cd src && pwd'
msh --agent-session ~/.config/msh/agent.session --agent -c 'pwd'   # → .../src
```

---

## 8. トラブルシュート

| 症状 | 対処 |
|---|---|
| MCP に msh が出ない | `./scripts/verify-mcp.sh` · パス確認 · Reload Window |
| `msh binary not found` | `MSH_BIN=/path/to/msh` を MCP env に追加 |
| 出力が JSON で読みにくい | MCP 利用（agent がパース）か `--json` 不使用 |
| Claude Bash が JSON | MCP に切替、または SHELL_PREFIX を外す |
| cwd がリセットされる | MCP を使う（ステートフル）か `--agent-session` |
| OpenClaw で tools が見えない | `bundle-mcp` 許可 · `openclaw mcp doctor --probe` |

---

## 9. 関連ファイル

| パス | 用途 |
|---|---|
| `scripts/msh-mcp.sh` | Cursor 用 MCP ラッパ |
| `scripts/msh-agent-shell.sh` | Claude `CLAUDE_CODE_SHELL` 用 |
| `scripts/msh-agent-exec.sh` | Claude `CLAUDE_CODE_SHELL_PREFIX` 用 |
| `scripts/verify-mcp.sh` | MCP smoke test |
| `docs/examples/*` | コピペ用設定テンプレ |
| `.cursor/mcp.json` | 本リポジトリ Cursor 設定 |

---

## 10. 参照

- [agent-shell-positioning.md](./agent-shell-positioning.md) — Track B 設計
- [ai-integration.md](./ai-integration.md) — `--json` / `--agent` API
- [Claude Code env vars](https://code.claude.com/docs/en/env-vars)
- [Codex config.toml](https://developers.openai.com/codex/config-reference)
- [OpenClaw MCP](https://docs.openclaw.ai/cli/mcp)
