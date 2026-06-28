# msh AI 連携設計

> 最終更新: 2026-06-29 / 対象バージョン: v0.7.3+（A-1 基盤・A-2・B-1 実装、ローカル/他 LLM 対応）

bash/zsh/fish が持たない **ネイティブ AI 連携**を、msh の核（軽量・低依存・安全）を壊さずに実現するための設計と段階計画。

---

## 2 つの方向

| 方向 | 意味 | 競合状況 |
|---|---|---|
| **A. AI を使えるシェル** | msh が LLM を呼び、人間を助ける（NL→コマンド・エラー解説等） | warp / fish-ai 等が部分的 |
| **B. AI に使われるシェル** | AI エージェントが msh を「最も扱いやすい実行環境」として使う | ほぼ空白地帯（msh の独自ポジション） |

A は差別化の入口、**B が独自ポジション**。両立で「AI エージェント時代の標準シェル」を狙う。

---

## 設計原則（不変条件）

1. **依存を増やさない** — HTTP は `curl` サブプロセスへ委譲。`reqwest`/`tokio` は持ち込まない。
   - 実測: A-1 実装で**依存数は不変（76）**、バイナリ +64KB（816→880KB）、新規 crate ゼロ。
2. **秘密を漏らさない** — API キー・リクエストボディは **argv に出さない**。
   - curl 設定ファイル（`-K`, パーミッション `0600`）に URL/ヘッダを書き、ボディは stdin (`data = "@-"`)。
   - キーは環境変数参照のみ（`config.toml` に平文保存しない）。
3. **安全（自動実行しない）** — AI 出力はテキストとして返すだけ。コマンド実行は人間（またはエージェントモードの確認ゲート）を介する。
4. **オプトイン・遅延** — `ai.enabled = false` がデフォルト。OFF 時はコード経路に一切入らず、通常操作を遅くしない。
5. **オフライン耐性** — AI 不達でもシェルは通常動作（degrade、ブロックしない）。

---

## A-1 基盤（実装済み）

### 設定（`config.toml` の `[ai]`）

```toml
[ai]
enabled = false                    # デフォルト無効
provider = "claude"                # claude | openai | gemini | ollama
model = "claude-3-5-haiku-latest"
api_key_env = "ANTHROPIC_API_KEY"  # キーは環境変数から取得（空ならキー不要）
max_tokens = 1024
# base_url = "..."                 # 任意: 互換エンドポイント上書き
```

#### ローカル LLM・他 LLM

| 形態 | 設定 | 備考 |
|---|---|---|
| Ollama（ローカル） | `provider = "ollama"`, `api_key_env = ""`, `base_url = "http://localhost:11434"` | ネイティブ `/api/chat`。認証不要 |
| **Gemma**（Gemini 系・省メモリ） | `provider = "gemma"`, `model = "gemma3:1b"`, `api_key_env = ""` | `gemma` は Ollama 経路のエイリアス。要 `ollama pull gemma3:1b` |
| LM Studio / llama.cpp / vLLM | `provider = "openai"`, `base_url = "http://localhost:1234/v1"` | OpenAI 互換 API として扱う |
| groq / openrouter / together 等 | `provider = "openai"`, `base_url = "..."`, `api_key_env = "..."` | OpenAI 互換 + キー指定 |

> **Gemma メモ**: Gemma は API ではなくオープン**モデル**。`gemma3:1b`（≈815MB・省メモリ）/ `gemma3` / `gemma3:4b` などを `ollama pull` して `model` に指定する。`provider = "gemma"` は `ollama` のエイリアス（同じネイティブ `/api/chat` を使用）。

`api_key_env` が空文字、または Ollama で env 未設定の場合は認証ヘッダを付けずに呼ぶ（keyless 許容）。

### モジュール構成

| パス | 役割 |
|---|---|
| `msh/src/config.rs` | `[ai]` セクションのパース（`AiSettings` / `AiProvider`） |
| `msh/src/ai.rs` | プロバイダ抽象・curl 委譲クライアント・依存なし JSON パーサ |
| `msh/src/shell.rs` | `ai <prompt>` 組み込み（応答を表示するだけ・**実行しない**） |

### プロバイダ抽象

`AiClient::complete(system, user)` が以下を吸収:

| プロバイダ | エンドポイント | 認証 | 応答パス |
|---|---|---|---|
| Claude | `/v1/messages` | `x-api-key` ヘッダ | `content[0].text` |
| OpenAI | `/v1/chat/completions` | `Authorization: Bearer`（任意） | `choices[0].message.content` |
| Gemini | `:generateContent` | URL クエリ `?key=` | `candidates[0].content.parts[0].text` |
| Ollama | `/api/chat`（`stream:false`） | なし（keyless） | `message.content` |
| Gemma | （= Ollama エイリアス） | なし（keyless） | `message.content` |

リクエスト組み立て (`build_request`) と応答抽出 (`extract_text`) は純関数としてユニットテスト済み（ネットワーク不要）。

### 使い方（A-1 / A-2）

```bash
export ANTHROPIC_API_KEY=sk-...
# config.toml で enabled = true にした上で:

# 1) ai: モデルに質問（表示のみ・実行しない）
ai このディレクトリの Rust ファイル数を数えるコマンドは?

# 2) # 自然文 → コマンド提案（対話時）。次の入力欄へ提案を挿入し、
#    Enter で実行・編集も可能（自動実行はしない）
# 大きい順にファイルサイズを表示

# 3) explain: 直前コマンド（失敗時は終了コードも添える）または指定コマンドを解説
tar -xzvf archive.tgz
explain                 # 直前コマンドを解説（失敗していれば原因と対処も）
explain rsync -avz src/ dst/   # 指定コマンドを解説
```

### 使い方（B-1: 構造化出力）

```bash
msh --json -c 'echo hi; ls /nope'
# {"command":"echo hi; ls /nope","exit_code":2,"duration_ms":7,
#  "stdout":"hi\n","stderr":"ls: /nope: No such file or directory\n"}
```

AI エージェントは `--json` で stdout/stderr/exit_code/duration_ms/command を 1 行 JSON として機械的に受け取れる（パース失敗時は `error` フィールドを付与）。

---

## 段階計画

### Track A: AI を使えるシェル

| フェーズ | 内容 | 状態 |
|---|---|---|
| **A-1** | `[ai]` 設定・curl 委譲クライアント・安全枠・`ai` 表示専用組み込み | ✅ 実装済み |
| **A-2** | NL→コマンド（`# コメント`→入力欄へ提案挿入）／コマンド失敗時の AI 解説／`explain` | ✅ 実装済み |
| A-3 | AI autosuggestion（履歴・dir 文脈で出せない時のフォールバック、レイテンシ隠蔽要検討） | 計画 |
| **+** | ローカル/他 LLM（Ollama ネイティブ・OpenAI 互換 base_url・keyless） | ✅ 実装済み |

### Track B: AI に使われるシェル

| フェーズ | 内容 | 状態 |
|---|---|---|
| **B-1** | 構造化出力 `msh --json -c '...'`（stdout/stderr/exit_code/duration_ms/command を 1 行 JSON） | ✅ 実装済み |
| B-2 | エージェント安全実行 `msh --agent`（破壊的コマンド検知・ドライラン・確認フック・構造化ログ） | 計画 |
| B-3 | MCP サーバ公開（Claude/Cursor から安全に shell を回す標準口） | 構想 |

---

## リスクと対策

| リスク | 対策 |
|---|---|
| バイナリ肥大・依存増 | curl 委譲を徹底。AI コードは新規 crate ゼロを維持 |
| 秘密情報の漏洩 | キーは env のみ・argv に出さない・curl 設定ファイル 0600・ボディ stdin |
| 危険なコマンド実行 | 自動実行しない。エージェントモードでも破壊的操作は確認ゲート必須 |
| プライバシー | 送信コンテキストを設定で制御・秘密の redaction・デフォルト最小送信（A-2 以降で実装） |
| レイテンシ | AI は遅延・オプトイン。通常操作経路に入れない。不達時は degrade |
