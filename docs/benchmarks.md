# msh ベンチマーク結果

> 計測日: 2026-06-28  
> バージョン: **v0.7.0**  
> 環境: macOS (Apple Silicon), release ビルド, `MSH_SKIP_RC=1`（rc 読み込みスキップ）

## サマリー

| 指標 | 目標 (Phase 4) | 結果 (v0.7.2) | 判定 |
|---|---|---|---|
| ウォーム起動 (`-c exit`) | < 30ms | **~3.5 ms**（zsh 同等） | ✅ |
| アイドル RSS | < 8 MB | **~5.9 MB** | ✅ |
| バイナリサイズ | — | **816 KB** | — |
| 補完レイテンシ (prefix `ec`) | < 100ms | **~120 µs** | ✅ |
| パース (`echo \| wc`) | — | **~1.0 µs** | — |
| 互換スコア | L4 80%+ | **91% (32/35)** | ✅ |
| テスト (unit+basic+compat) | — | **82 件全通過** | ✅ |

## 起動時間・メモリ

```bash
/usr/bin/time -l env MSH_SKIP_RC=1 ./target/release/msh -c exit
```

### コールド起動（max RSS）

| シェル | max RSS |
|---|---|
| **msh v0.7** | **6,356,992 B (~6.1 MB)** |
| bash | ~1,982,464 B (~1.9 MB) |
| zsh | ~2,457,600 B (~2.4 MB) |

### ウォーム起動（`scripts/benchmark.sh`, 交互計測の中央値, `-c exit`）

| シェル | 中央値 |
|---|---|
| bash | **~2.6 ms** |
| **msh** | **~3.7 ms** |
| zsh | ~3.5 ms |

> **所見**: ウォーム起動で **msh は zsh とほぼ同等**（誤差範囲で前後する）。bash は C 製で最速。
> 旧記載の「~0.31s」は単発コールド計測の誤差、「2.86ms で zsh 超え」は計測手法/負荷由来の楽観値だったため訂正。
> 絶対値はシステム負荷で変動するため、**交互計測の中央値**で比較すること。

### バイナリサイズ・メモリ（v0.7.2 で最適化）

| 指標 | v0.7.0 | v0.7.2 | 差分 |
|---|---|---|---|
| バイナリ | 2.0 MB | **816 KB** | −60% |
| 起動時 RSS | 6.39 MB | **5.92 MB** | −7% |
| 依存クレート | 124 | **111** | −13 |

主因は `case_insensitive_history_search`（regex）除去＋自前実装、`panic="abort"`、rustyline 機能 trim。詳細は [memory-optimization.md](./memory-optimization.md)。

### 外部比較（hyperfine）

```bash
./scripts/benchmark.sh   # hyperfine 要インストール
```

## Criterion マイクロベンチ

```bash
cd msh && cargo bench --bench shell_bench
```

| ベンチマーク | v0.4 | v0.7 中央値 | 備考 |
|---|---|---|---|
| `complete_commands_ec` | ~142 µs | **~120 µs** | PATH キャッシュ |
| `expand_vars` | ~299 ns | **~382 ns** | |
| `highlight_line` | ~146 ns | **~152 ns** | |
| `parse_redirect_pipeline` | ~701 ns | **~1.17 µs** | |

HTML レポート: `msh/target/criterion/report/index.html`

## 再現手順

```bash
cd msh
cargo build --release
cargo test
cargo bench --bench shell_bench
env MSH_SKIP_RC=1 /usr/bin/time -l ./target/release/msh -c exit
```

## v0.7 で追加された依存

| crate | 用途 |
|---|---|
| serde / serde_json | セッション復元 (`session.json`) |
| rustyline `case_insensitive_history_search` | Ctrl+R 強化 |
