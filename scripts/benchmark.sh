#!/usr/bin/env bash
# msh 起動ベンチマーク — msh / bash / zsh / fish を比較する。
# hyperfine があればそれを使い、無ければ簡易ループ計測にフォールバックする。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CRATE="$ROOT/msh"
MSH="$CRATE/target/release/msh"
export MSH_SKIP_RC=1

cd "$CRATE"
cargo build --release --quiet

# 比較対象（存在するものだけ）
declare -a NAMES=()
declare -a CMDS=()
add() { if command -v "$1" >/dev/null 2>&1 || [[ -x "$1" ]]; then NAMES+=("$2"); CMDS+=("$3"); fi; }

add "$MSH" "msh"  "MSH_SKIP_RC=1 $MSH -c exit"
add bash  "bash" "bash --norc -c exit"
add zsh   "zsh"  "zsh -f -c exit"
add fish  "fish" "fish --no-config -c exit"

if command -v hyperfine >/dev/null 2>&1; then
  echo "==> hyperfine で計測"
  args=(--warmup 10 --export-markdown "$ROOT/docs/benchmarks-hyperfine.md")
  for i in "${!NAMES[@]}"; do
    args+=(--command-name "${NAMES[$i]}" "${CMDS[$i]}")
  done
  hyperfine "${args[@]}"
  echo
  echo "結果: docs/benchmarks-hyperfine.md"
else
  echo "==> hyperfine 未インストール → 簡易ループ計測（100 回平均, ウォーム）"
  echo
  printf "%-6s %12s\n" "shell" "avg(ms)"
  printf "%-6s %12s\n" "-----" "------------"
  ITER=100
  for i in "${!NAMES[@]}"; do
    cmd="${CMDS[$i]}"
    # ウォームアップ
    for _ in 1 2 3; do eval "$cmd" >/dev/null 2>&1 || true; done
    start=$(python3 -c 'import time; print(time.time())')
    for _ in $(seq "$ITER"); do eval "$cmd" >/dev/null 2>&1 || true; done
    end=$(python3 -c 'import time; print(time.time())')
    avg=$(python3 -c "print(f'{(($end-$start)/$ITER)*1000:.2f}')")
    printf "%-6s %12s\n" "${NAMES[$i]}" "$avg"
  done
  echo
  echo "（正確な比較には hyperfine を推奨: brew install hyperfine）"
fi
