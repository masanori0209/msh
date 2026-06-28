# msh 省メモリ化レポート

> 計測日: 2026-06-28 / バージョン: v0.7.2 / 環境: macOS (Apple Silicon), release ビルド
> 計測: `/usr/bin/time -l env MSH_SKIP_RC=1 ./target/release/msh -c exit`

## サマリー（実施済みの効果）

| 指標 | Before (v0.7.0) | After (v0.7.2) | 差分 |
|---|---|---|---|
| バイナリサイズ | 2.0 MB | **816 KB** | **−60%** |
| 起動時 RSS | 6.39 MB | **6.22 MB** | −3% |
| 依存クレート数（推移的） | 124 | **109** | −15 |

> 最適化のみのバイナリは 800KB。最終 816KB は同時実装した L5 連想配列のコードを含む。
| ウォーム起動 | ~3.5 ms（zsh 同等） | ~3.5 ms（維持） | ±0 |

bash 比 RSS は依然 ~3.1 倍（6.22 MB / 1.99 MB）。これは **Rust ランタイムの構造的下限**（後述）で、bash 並み 2 MB は非目標。**8 MB 以内 KPI は達成**。

---

## 要因分析

`MSH_SKIP_RC=1 -c exit`（対話エディタ未初期化）でも RSS は ~6 MB。
バイナリは 2 MB なので、差分 ~4 MB は実行時ベースライン。

| 層 | 概算 | 削減可否 |
|---|---|---|
| Rust std ランタイム + macOS dyld + malloc zone | ~4 MB | ✗ 構造的（C 製 bash との差の主因） |
| マップされたコードページ（バイナリ） | ~2 MB | △ コード削減で縮小（RSS 影響は小） |
| msh 自身のヒープ（HashMap 等） | < 0.5 MB | ○ アロケーション削減 |

**重要**: RSS の主因は **コードサイズではなく Rust ランタイム固定費**。
`opt-level="s"` でバイナリを 1.6 MB に縮めても RSS は変わらず（6.12 MB）、起動だけ遅化（3.37ms）したため不採用。

---

## 実施した打ち手

### 1. `panic = "abort"`（release プロファイル）
巻き戻し（unwinding）機構とランディングパッドを除去。
- 効果: バイナリ 2.0M→1.8M、RSS 6.39→6.08 MB
- リスク: パニック時に巻き戻さず即 abort。msh はエラーを `Result` で扱うため実害なし。

### 2. rustyline 依存の機能 trim（最大の打ち手）
`default-features = false` にし、必要な `with-file-history` のみ有効化。
`custom-bindings`(radix_trie)・`with-dirs`(home) を除去。

さらに **`case_insensitive_history_search` を除去**（`regex` を引き込む最大要因）。
代わりに Ctrl+R プレビューの大文字小文字無視を `line_editor.rs` で自前実装
（`History::get` を逆順走査して `to_ascii_lowercase().starts_with`）。
- 効果: **バイナリ 1.8M→832K**（regex コードがリリースバイナリから消滅）、依存 −6
- 確認: `cargo tree -i regex` → criterion(dev) のみ。リリースバイナリは regex 非搭載。
- 機能: 大文字小文字無視のインライン履歴プレビューは維持。

### 3. `serde` / `serde_json` 廃止（session を自前シリアライズ）
セッション復元（`session.state`）は `{cwd, dir_stack}` のみ。
JSON をやめ、`cwd <path>` / `dir <path>` の行形式を `session.rs` に手書き。
- 効果: バイナリ 832K→800K、依存 113→111（serde/serde_json + itoa/ryu 等）

### 4. `thiserror` / `once_cell` 廃止
手書き `Display`/`Error` と `std::sync::LazyLock` に置換。
- 効果: 依存 111→109（推移的）、バイナリ微減

### 5. 展開ホットパスの env クローン削減
`expand_pipeline` / `expand_word_list` が**コマンドごとに**シェル変数・配列の
`HashMap` を丸ごと clone していた（借用回避のための防御的コピー）。
コマンド置換（`&mut self`）完了後は `&ctx` しか使わないため、
`self.current_scope()` / `self.current_arrays()` を直接借用に変更。
- 効果: コマンド実行のたびに発生していた 2 回の全 env コピーを除去（作業セット・CPU 削減）。

### 6. `-c` パスでセッション復元を省略
`init_for_command()` を追加。非対話 `-c` では `session.state` の読み込み・cwd 上書きを行わない。
- 効果: 単発スクリプトの副作用低減（RSS への影響は限定的）

---

## 今後の打ち手（未実施・優先度順）

| 打ち手 | 想定効果 | リスク/コスト |
|---|---|---|
| ~~`thiserror` 廃止~~ | ✅ 実施済み | — |
| 文字列インターン / `Box<str>` 化（env 値） | ヒープ断片化低減 | 中 |
| `expand_pipeline` の `parsed.clone()` 回避（in-place 展開） | コマンドごとの clone 削減 | 中 |
| 履歴をメモリ上限つきリングバッファに | 長時間運用時の上限保証 | 低 |
| `#[global_allocator]` に軽量 allocator 検証 | macOS は逆効果の可能性大 | 高（要計測） |
| musl 静的リンク（Linux 配布） | 起動・配布性 | 中（macOS 非対象） |

> RSS の構造的下限（Rust ランタイム ~4–5MB）は、対話シェルの安全性・保守性との
> トレードオフ。bash 並みの 2MB は非目標とし、**8MB 以内の維持**を KPI とする。

---

## 再現手順

```bash
cd msh
cargo build --release
ls -lh target/release/msh                                   # バイナリサイズ
/usr/bin/time -l env MSH_SKIP_RC=1 ./target/release/msh -c exit  # RSS
cargo tree -i regex                                         # regex の出所確認
../scripts/benchmark.sh                                     # 起動比較
```
