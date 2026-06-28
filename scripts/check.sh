#!/usr/bin/env bash
# msh 検証ループ — fmt + clippy + test を一括で実行する単一ゲート。
# エージェント / 開発者はコミット前にこれをパスさせる。
#
# 使い方:
#   ./scripts/check.sh           # fmt --check + clippy + test
#   ./scripts/check.sh --fix     # 先に cargo fmt で整形してから検証
#   ./scripts/check.sh --bench   # 上記に加えてベンチも実行
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CRATE="$ROOT/msh"

FIX=0
RUN_BENCH=0
for arg in "$@"; do
  case "$arg" in
    --fix) FIX=1 ;;
    --bench) RUN_BENCH=1 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

cd "$CRATE"

step() { printf '\n\033[1;34m==> %s\033[0m\n' "$1"; }

if [[ "$FIX" -eq 1 ]]; then
  step "cargo fmt (整形)"
  cargo fmt
fi

step "cargo fmt --check (整形チェック)"
cargo fmt --check

step "cargo clippy -- -D warnings (lint)"
cargo clippy --all-targets -- -D warnings

step "cargo test (テスト)"
cargo test

if [[ "$RUN_BENCH" -eq 1 ]]; then
  step "cargo bench (ベンチ)"
  cargo bench --bench shell_bench
fi

printf '\n\033[1;32m✓ すべてのチェックを通過しました\033[0m\n'
