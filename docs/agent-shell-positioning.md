# AI 用途シェル — カテゴリ整理とエージェント統合設計

> 最終更新: 2026-06-29 / 対象: msh v0.7.4  
> 関連: [ai-integration.md](./ai-integration.md) · [positioning-report.md](./positioning-report.md)

---

## 1. 結論（TL;DR）

| 問い | 答え |
|---|---|
| **「AI 用途のシェル」という立ち位置はあるか** | **ある**。ただし独立カテゴリ名は未固定で、現状は **3 つの隣接領域** に分散している |
| **msh の独自性** | **Track B（AI に使われるシェル）** — 構造化出力・安全ゲート・MCP を **shell 本体** に載せた製品はほぼない |
| **一般普及 vs ニッチ** | daily driver としての「AI シェル」は Warp 等と競合。**エージェント backend** としての需要の方が現実的 |
| **Cursor / Claude Code との関係** | 現状は **bash 固定** が主流。msh は **`--json` / `--agent` / `--mcp`** で drop-in 代替を目指す |

---

## 2. カテゴリマップ（3 層）

AI × ターミナルは、次の 3 層に分かれる。**同じ「AI シェル」と呼ばれても、解決する問題が違う。**

```
┌─────────────────────────────────────────────────────────────┐
│ ① AI 付きターミナルアプリ（GUI レイヤ）                      │
│    Warp, Amazon Q in terminal 等                             │
│    → シェル本体は bash/zsh。AI は UI に載る                  │
├─────────────────────────────────────────────────────────────┤
│ ② AI を使えるシェル（人間向け / Track A）                    │
│    msh: ai / explain / #→提案, fish-ai 系, ai-shell 等       │
│    → 人間がコマンドを学び・提案を編集して実行                │
├─────────────────────────────────────────────────────────────┤
│ ③ エージェントの実行レイヤ（Track B）                        │
│    現状: Claude Code / Cursor Agent → bash subprocess        │
│    msh: --json / --agent / --mcp                             │
│    → LLM が「観測可能・安全・機械可読」な shell front-end 欲しい │
└─────────────────────────────────────────────────────────────┘
```

### 2025–2026 の業界トレンド（参考）

エージェントのツール接続は **CLI 派** と **MCP 派** のハイブリッドが主流になりつつある。

| 方式 | 向く用途 | 弱点 |
|---|---|---|
| **CLI + shell** | ローカル処理、`gh`/`kubectl`/パイプ。LLM の学習データに乗っている | 出力パース・安全・監査は agent 実装依存 |
| **MCP** | OAuth・SaaS・監査・typed schema | スキーマ肥大でトークンコスト増 |
| **structured shell（msh B）** | shell 全体を 1 回の subprocess で観測 | エコシステム未成熟・agent 側対応が必要 |

msh は **③ を shell 本体で担う** ポジション。MCP ツールを 50 個載せるのではなく、**`msh_run` 1 本で shell 実行全体** をカバーする思想。

---

## 3. msh の 2 トラック

| トラック | 誰のため | 代表 API | 競合 |
|---|---|---|---|
| **A. AI を使えるシェル** | 人間 | `ai`, `explain`, `#` 提案 | Warp（UI）、fish-ai（部分） |
| **B. AI に使われるシェル** | エージェント | `--json`, `--agent`, `--mcp` | **ほぼ空白**（bash + agent 実装） |

**勝ち筋**: A は入口・UX 訴求。B は **差別化の芯**（エージェント統合で説明しやすい）。

---

## 4. エージェントが bash から欲しいもの

Coding agent（Cursor, Claude Code, Aider 等）が subprocess shell に求める要件:

| 要件 | bash の現状 | msh の対応 |
|---|---|---|
| **stdout / stderr / exit を安定取得** | agent 実装で pipe 処理。大出力で詰まりやすい | `--json` / `build_command_json`（一時ファイル経由） |
| **実行時間** | 自前計測 | JSON に `duration_ms` |
| **破壊的操作の抑止** | なし（`rm -rf` も通る） | `--agent` で Destructive を default block |
| **分類可能なリスク** | なし | `safe` / `caution` / `destructive` |
| **dry-run** | なし | `--agent-dry-run` / MCP `dry_run: true` |
| **機械パース** | テキスト混在 | 1 行 JSON |
| **MCP 公開** | なし | `msh --mcp` → `msh_run` |

---

## 5. 統合パターン（3 段）

エージェント側から msh を使う方法は、**信頼度・実装コスト** に応じて 3 段階。

### 5.1 Level 1 — `--json -c`（観測のみ）

**用途**: 結果をパースしたいだけ。安全ゲート不要。

```bash
MSH_SKIP_RC=1 msh --json -c 'cargo test 2>&1 | tail -5'
```

**stdout（1 行 JSON）**:

```json
{
  "command": "cargo test 2>&1 | tail -5",
  "exit_code": 0,
  "duration_ms": 1234,
  "stdout": "...",
  "stderr": ""
}
```

| 項目 | 値 |
|---|---|
| ゲート | なし |
| Cursor 設定 | 不要（agent の shell コマンドを `msh --json -c` に差し替え） |
| 向く場面 | CI ログ取得、テスト結果パース、ベンチ |

### 5.2 Level 2 — `--agent -c`（安全 + 観測）

**用途**: エージェントが自律実行するが、破壊的操作は止めたい。

```bash
MSH_SKIP_RC=1 msh --agent -c 'rm -rf /tmp/build'
# → blocked, exit 1

MSH_SKIP_RC=1 msh --agent -c 'cargo fmt --check'
# → executed + JSON（action/risk 付き）

MSH_SKIP_RC=1 msh --agent --agent-dry-run -c 'mv src dst'
# → dry_run のみ
```

**成功時 JSON 例**（`action` / `risk` が `--json` より増える）:

```json
{
  "command": "cargo fmt --check",
  "exit_code": 0,
  "duration_ms": 42,
  "stdout": "",
  "stderr": "",
  "action": "executed",
  "risk": "safe"
}
```

| リスク | デフォルト動作 |
|---|---|
| `safe` | 実行 |
| `caution` | 実行（将来: 確認フック拡張余地） |
| `destructive` | **blocked**（`--agent-force` / `MSH_AGENT_FORCE=1` でのみ実行） |

### 5.3 Level 3 — `--mcp`（stdio MCP サーバ）

**用途**: Cursor / Claude Desktop 等から **MCP ツール** として shell 実行を公開。

```bash
MSH_SKIP_RC=1 msh --mcp
```

| MCP メソッド | 内容 |
|---|---|
| `initialize` | protocol `2024-11-05`, server `msh` |
| `tools/list` | `msh_run` 1 本 |
| `tools/call` | `arguments.command` を agent ゲート後に実行 |

**`msh_run` 引数**:

| フィールド | 型 | 説明 |
|---|---|---|
| `command` | string | 実行する shell 一行 |
| `dry_run` | bool | 分類のみ |
| `force` | bool | Destructive も実行 |

#### 製品別設定例

**Cursor / Claude Code / Codex / OpenClaw** のコピペ可能な設定は [agent-integration.md](./agent-integration.md) を参照。

#### Cursor 設定（リポジトリ同梱）

本リポジトリには **`.cursor/mcp.json`** が含まれます（`${workspaceFolder}/scripts/msh-mcp.sh` 経由でローカル build の msh を起動）。

```bash
# 事前にビルド
cd msh && cargo build

# MCP smoke test（Cursor 接続前）
./scripts/verify-mcp.sh
```

Cursor で **Settings → MCP** に `msh` が表示されることを確認。変更後は **Reload Window**。

| ファイル | 用途 |
|---|---|
| `.cursor/mcp.json` | プロジェクト MCP 設定（コミット可） |
| `.cursor/mcp.json.example` | 同上（テンプレ副本） |
| `scripts/msh-mcp.sh` | msh バイナリ解決 + `--mcp` 起動 |
| `scripts/verify-mcp.sh` | initialize / tools/list / tools/call 検証 |

`MSH_BIN=/path/to/msh` でバイナリを明示可能。Release ビルド優先順: `MSH_BIN` → `target/release` → `target/debug` → `PATH` の `msh`。

#### 手動設定例（グローバル `~/.cursor/mcp.json`）

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

## 6. 競合との位置（2 軸）

```
                    人間が主役
                         ↑
     Fish UX ────────────┼──────── Warp（AI ターミナル）
     msh Track A        │
                         │
 bash/zsh ←──────────────┼──────────────→ AI ネイティブ度
                         │
     msh Track B        │   Claude Code / Cursor（bash 委譲）
                         │
                         ↓
                    エージェントが主役
```

| 製品 | 層 | msh との関係 |
|---|---|---|
| **Warp** | ① | ターミナル UI 競合。shell 置換ではない |
| **GitHub Copilot CLI** | 単機能 CLI | shell 全体ではない |
| **MCP terminal サーバ** | TUI 構造化 | 画面操作が主目的。msh とは別軸 |
| **bash + agent 実装** | ③ のデファクト | **msh B の直接競合**（暗黙） |

---

## 7. 採用シナリオ別ガイド

| シナリオ | 推奨 | 理由 |
|---|---|---|
| 個人の daily driver + AI 補助 | msh 対話 + Track A | Fish 互換 UX + bash スクリプト |
| Cursor agent の shell backend | `--agent` または `--mcp` | 構造化 + 安全 |
| CI / スクリプト | bash のまま | ポータビリティ |
| 社内 agent 基盤 | `--mcp` + 監査ログ拡張 | 1 ツールで統一 |
| サーバー `/bin/sh` | bash/dash | 変更不可 |

### 乗り換え判断（エージェント backend）

**向いている**

- agent が bash の stdout パースで苦しんでいる
- `rm -rf` 等の事故を shell 層で止めたい
- JSON 1 行で exit/stdout/stderr/duration が欲しい
- MCP で「shell 実行」を 1 ツールにまとめたい

**向いていない**

- agent が bash 固定で変更不可
- 複雑な `.zshrc` / bash 専用 toolchain に依存
- MCP スキーマより個別 SaaS MCP を優先したい

---

## 8. 現状ギャップとロードマップ

| 項目 | 状態 | 次 |
|---|---|---|
| `--json` / `--agent` | ✅ | truncate / meta / structured error |
| `--mcp` | ✅ PoC | ステートフル session（プロセス内 cwd） |
| Cursor 公式 integration | ✅ | `.cursor/mcp.json` + [agent-integration.md](./agent-integration.md) |
| Claude Code `SHELL` 差し替え | ✅ テンプレ | `msh-agent-shell.sh` + MCP |
| 監査ログ | ✅ | `MSH_AGENT_AUDIT_LOG` / `[agent] audit_log` |
| サンドボックス | ✅ | `MSH_AGENT_SANDBOX` / allowlist / caution gate |
| A-3 autosuggest | 計画 | Track A 強化 |

---

## 9. 意思決定フロー

```
エージェントが shell を叩く必要がある？
  ├─ No → 専用 CLI（gh, kubectl）or SaaS MCP
  └─ Yes
       ├─ 安全ゲートが必要？ → msh --agent または --mcp
       ├─ JSON パースだけ？ → msh --json
       └─ ポータビリティ最優先？ → bash（現状維持）
```

---

## 10. 参照実装

| モジュール | 役割 |
|---|---|
| `msh/src/agent.rs` | リスク分類・`gate()` |
| `msh/src/shell.rs` | `build_command_json`, `run_command_agent_json` |
| `msh/src/mcp.rs` | stdio JSON-RPC, `msh_run` |
| `msh/tests/basic.rs` | agent 統合テスト |

```bash
# 手動 smoke test
MSH_SKIP_RC=1 msh --json -c 'echo ok'
MSH_SKIP_RC=1 msh --agent -c 'rm -rf /'
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | MSH_SKIP_RC=1 msh --mcp
```

---

## 用語

| 用語 | 意味 |
|---|---|
| **Track A** | 人間が AI を使う（NL→提案、explain） |
| **Track B** | エージェントが shell を使う（JSON/MCP/ゲート） |
| **Agent backend** | LLM agent の subprocess shell としての msh |
| **Structured shell** | 実行結果を機械可読形式で返す shell front-end |
