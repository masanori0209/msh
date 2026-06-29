# msh ポジショニングレポート

> 計測日: 2026-06-29  
> バージョン: **v0.7.4**  
> 環境: macOS (Apple Silicon), release ビルド, `MSH_SKIP_RC=1`

---

## エグゼクティブサマリー

msh は **「Zsh 移行を意識した互換性 × Fish 寄りの対話 UX × Rust 単一バイナリ × ネイティブ AI / エージェント連携」** を軸に位置づけるシェル。

**現在地（一言）**: 日常シェルとして **Fish に近い UX + bash/zsh スクリプト互換 100%** を両立しつつ、**エージェント向け structured shell（`--json` / `--agent` / `--mcp`）** で独自の第 3 軸を持つ。Warp のような GUI ターミナルではなく、**CLI ネイティブで軽量**な代替。

| 軸 | 現状 | 備考 |
|---|---|---|
| 互換 | **100% (43/43)** | `${var:=}`・`<( )` 含む |
| テスト | **155/155** | lib 94 + basic 15 + compat 46 |
| バイナリ | **~1.0 MB** | 単一実行ファイル・新規 HTTP 依存なし |
| 起動 RSS | **~6.0 MB** | bash 比 ~3.2×（Rust 下限帯） |
| 対話 UX | **○+** | 補完・autosuggest・セグメントプロンプト・対話設定 |
| AI（人間） | **◎** | `ai` / `explain` / `#` 提案 |
| AI（エージェント） | **◎ PoC** | `--json` / `--agent` / `--mcp` |
| 配布・エコ | **△** | Release タグ・WASM 本番 ABI 未完了 |

---

## 1. 市場上の立ち位置

### 1.1 3 層モデル

AI × ターミナルは隣接領域が 3 つに分かれる（詳細: [agent-shell-positioning.md](./agent-shell-positioning.md)）。

| 層 | 代表 | msh との関係 |
|---|---|---|
| **① AI 付きターミナルアプリ** | Warp, Amazon Q in terminal | **非競合** — GUI レイヤ。シェル本体は bash/zsh |
| **② AI を使えるシェル（人間向け）** | fish-ai 系, msh Track A | **部分競合** — msh は curl 委譲で依存ゼロ |
| **③ エージェントの実行レイヤ** | Claude Code / Cursor → bash | **差別化の芯** — msh Track B |

### 1.2 従来シェルとの 2 軸

```
        高 UX（対話・補完・プロンプト・エラー）
          │
    Fish ─┤
          │        ★ msh v0.7.4
          │           （UX + 互換 + AI + 軽量バイナリ）
    Zsh ──┤
          │
    Bash ─┤
          │
        低 UX ────────────────────────── 高 互換性
              Fish              Bash/Zsh/msh

※ 第 3 軸（図外）: エージェント向け structured shell — msh のみ前進
```

**msh が狙う空白**:

- Fish の弱点（bash 非互換）を **互換 100%** で補う
- Zsh+Oh-My-Zsh の弱点（設定肥大・遅い）を **config.toml 1 枚 + デフォルト UX** で補う
- bash の弱点（UX・AI）を **ネイティブ機能** で補う
- エージェント市場では **bash subprocess の代替** として `--json` / 安全ゲート / MCP

### 1.3 直接競合ではない参考

| プロジェクト | 関係 |
|---|---|
| Nushell | データパイプライン特化。POSIX 互換は非目標 |
| Starship / p10k 等 | 外部プロンプト。msh は **内蔵セグメントプロンプト** + 対話設定 |
| atuin | 履歴バックエンド連携候補（`history_backend = "atuin"`） |
| wasmtime | WASM プラグイン実行の外部委譲（PoC 段階） |

---

## 2. ターゲットユーザー

| ペルソナ | ニーズ | msh の訴求 |
|---|---|---|
| **Zsh 疲れユーザー** | 速い・設定少ない・見た目良い | 1 ファイル設定・ファジー補完・プロンプト対話設定 |
| **Fish 卒業検討者** | UX は欲しいが bash スクリプトも動かしたい | 互換 100% + Fish 級 autosuggest |
| **Rust / インフラエンジニア** | 単一バイナリ・再現性 | ~1 MB バイナリ・CI ゲート・dotfiles 回帰 |
| **AI コーディングエージェント利用者** | 安全・パース可能な shell 出力 | `--json` / `--agent` / Cursor 同梱 MCP |
| **ローカル LLM ユーザー** | オフライン AI | Ollama / Gemma エイリアス・keyless |

**非ターゲット（現時点）**:

- GUI ターミナル一体型 AI（Warp ユーザー）
- 100% POSIX / 全 bash 方言（`>( )`・`$SECONDS` 等は未対応）
- プラグインエコシステム成熟度を最優先する Zsh パワーユーザー

---

## 3. 差別化マトリクス（5 段階: ◎4 / ○3 / △2 / ✗1 / —0）

| 観点 | Bash | Zsh | Fish | **msh v0.7.4** |
|---|---|---|---|---|
| 起動速度 | ◎ | ○ | ○ | **○**（実用帯・bash より遅い） |
| メモリ (RSS) | ◎ ~1.9MB | ○ ~2.4MB | ○ | **△+ ~6.0MB**（バイナリ自体は ~1MB） |
| スクリプト互換 | ◎ | ◎ | △ | **◎ 100%**（計測 43 ケース） |
| 対話 UX | △ | ○ | ◎ | **○+**（コア機能揃い・Fish Web UI には及ばず） |
| プロンプト | △ | ○※ | ○ | **○+**（内蔵・セグメント・対話設定） |
| 設定の簡潔さ | ○ | △ | ○ | **◎** `config.toml` 1 枚 |
| **AI（人間）** | — | — | △ | **◎** |
| **AI（エージェント）** | — | — | — | **◎ PoC** |
| 拡張性 | △ | ◎ | ○ | **△+** WASM PoC |
| エコシステム | ◎ | ◎ | ○ | **✗** Release / プラグイン本番未整備 |

※ Zsh は Oh-My-Zsh / Starship 等に依存することが多い

---

## 4. 製品としての強み（v0.7.4 時点）

### 4.1 互換・信頼

- 互換スコア **100% (43/43)** — [compat-score.md](./compat-score.md)
- テスト **155 件** 全通過、`clippy -D warnings` ゲート
- dotfiles 必須ゲート **7/7 (100%)**
- command-not-found **127**、大出力 `--json` デッドロック修正済み

### 4.2 対話 UX

- シンタックスハイライト・ファジー補完・説明付き補完
- Ctrl+R 大小文字無視・ディレクトリ文脈 autosuggestion
- 日英エラー・オンボーディング・未対応構文の明示 + 回避策
- **内蔵プロンプト**: セグメント（path / git / 時刻 / バッテリー / K8s / duration）
- **`prompt config` / `--configure-prompt`**: 対話式カスタマイズ → `config.toml` 保存
- カラーテーマ `msh` 標準（落ち着いた単色ベース）

### 4.3 省メモリ・単一バイナリ

| 指標 | 計測値 (2026-06-29) |
|---|---|
| release バイナリ | **1,033,824 B (~1.0 MB)** |
| 起動 RSS (`-c exit`) | **6,291,456 B (~6.0 MB)** |
| bash RSS 比 | **~3.2×** |
| Cargo.lock ランタイム | **98 パッケージ**（AI 追加後も reqwest/tokio なし） |

Phase 4 KPI（RSS < 8 MB）: ✅ 達成

### 4.4 AI 連携（独自）

| トラック | 機能 | 状態 |
|---|---|---|
| **A-1** | `[ai]` + curl 委譲 + `ai` | ✅ |
| **A-2** | `#` 提案 / `explain` | ✅ |
| **B-1** | `--json -c` 構造化出力 | ✅ |
| **B-2** | `--agent` 安全ゲート | ✅ |
| **B-3** | `--mcp` stdio MCP | ✅ PoC |
| A-3 | AI autosuggest フォールバック | 計画 |

設計原則: デフォルト OFF・自動実行しない・API キーは env のみ。

---

## 5. 弱み・ギャップ

| 領域 | 現状 | 影響 |
|---|---|---|
| **RSS** | bash 比 ~3.2× | コンテナ極限省メモリ用途では bash 優位 |
| **互換の深さ** | `>( )`・`$SECONDS`/`$LINENO` 等 | 一部 dotfiles / 高度脚本で bash 委譲 |
| **UX  polish** | Fish / Warp 比 | Web 設定 UI・リッチ autosuggest ソース不足 |
| **プロンプト** | 内蔵・カスタム可 | Starship 級のエコシステムは未構築 |
| **配布** | GitHub Release タグ未実行 | 一般インストール摩擦 |
| **WASM** | PoC（wasmtime 委譲） | 本番 ABI・プラグイン市場なし |
| **エージェント普及** | Cursor MCP 同梱は PoC | agent 側の msh ネイティブ対応はこれから |

---

## 6. ポジショニングステートメント

### 6.1 対外メッセージ（案）

> **msh** — Rust 製の対話型シェル。  
> Fish のような毎日使いやすさと、bash/zsh スクリプト互換を 1 つの ~1MB バイナリに。  
> 人間向け AI 支援と、コーディングエージェント向け structured shell を標準装備。

### 6.2 競合別の一言

| vs | msh の位置 |
|---|---|
| **bash** | UX・AI・プロンプトで上、起動 RSS では下 |
| **zsh** | 設定レス UX + 同等互換を目指す軽量代替 |
| **fish** | 同等 UX 志向 + **スクリプト互換で上** |
| **Warp** | CLI ネイティブ・軽量・エージェント backend 寄り |
| **Claude Code の bash** | 同じ subprocess モデルで **安全・JSON・MCP** を shell 本体化 |

### 6.3 短期〜中期ゴール

| 期間 | ゴール | 進捗感 |
|---|---|---|
| **短期** | 日常シェルとして Zsh/Fish からの移行候補 | **~85%** — 互換・UX コアは揃った |
| **中期** | エージェント default shell 候補 | **~60%** — API は PoC、普及はこれから |
| **長期** | AI 時代の「標準 CLI シェル」 | **~40%** — カテゴリ自体が形成中 |

---

## 7. ロードマップ上の次の一手

| 優先 | 内容 | ポジションへの効果 |
|---|---|---|
| P0 | **v0.7.4 GitHub Release** + Homebrew sha 更新 | 配布ギャップ解消 |
| P1 | **agent 統合ドキュメント** + Cursor/Claude 設定例 | ✅ [agent-integration.md](./agent-integration.md) |
| P2 | `$SECONDS` / `>( )` or 明示委譲 | dotfiles 深部互換 |
| P3 | WASM 本番 ABI | 拡張性 ○ へ |
| P4 | A-3 AI autosuggest | UX ◎ へ |
| P5 | プロンプト右側・transient 等 | UX polish |

---

## 8. 計測根拠

```bash
./scripts/check.sh              # 155 tests
./scripts/compat-score.sh       # 43/43
MSH_SKIP_RC=1 /usr/bin/time -l ./msh/target/release/msh -c exit
ls -la msh/target/release/msh   # バイナリサイズ
```

| シェル | max RSS (`-c exit`) |
|---|---|
| bash | 1,982,464 B (~1.9 MB) |
| zsh | 2,473,984 B (~2.4 MB) |
| **msh** | **6,291,456 B (~6.0 MB)** |

---

## 参照

- [agent-integration.md](./agent-integration.md) — 製品別設定例
- [agent-shell-positioning.md](./agent-shell-positioning.md) — AI シェルカテゴリ・Track A/B
- [ai-integration.md](./ai-integration.md) — AI 設計
- [compat-score.md](./compat-score.md) — 互換計測
- [benchmarks.md](./benchmarks.md) — ベンチ手順
- [competitive-analysis.md](./competitive-analysis.md) — 競合詳細
- [roadmap.md](./roadmap.md) — フェーズ計画
