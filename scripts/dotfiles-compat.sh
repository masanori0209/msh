#!/usr/bin/env bash
# 代表的な .bashrc / .zshrc パターンを msh -c で検証し、
# docs/dotfiles-compat.md を生成する（CI 常設用）。
#
# 使い方:
#   ./scripts/dotfiles-compat.sh
#   ./scripts/dotfiles-compat.sh --no-build
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CRATE="$ROOT/msh"
MSH="$CRATE/target/release/msh"
OUT="$ROOT/docs/dotfiles-compat.md"

BUILD=1
[[ "${1:-}" == "--no-build" ]] && BUILD=0

if [[ "$BUILD" -eq 1 ]]; then
  echo "==> cargo build --release"
  (cd "$CRATE" && cargo build --release --quiet)
fi

export MSH_SKIP_RC=1

# タブ区切り: カテゴリ / 説明 / コマンド / 期待（空=exit 0 のみ）
IFS=$'\t'
CASES=(
"必須	PATH 追記	export PATH=\"\$HOME/bin:\$PATH\"; echo ok	ok"
"必須	条件 source	[ -f /etc/hosts ] && echo hostfile_ok	hostfile_ok"
"必須	関数定義	f() { echo fn; }; f	fn"
"必須	for ループ	for d in bin sbin; do echo \$d; done | tail -1	sbin"
"必須	if ディレクトリ	if [ -d /tmp ]; then echo tmpdir; fi	tmpdir"
"必須	連想配列	declare -A c; c[k]=v; echo \${c[k]}	v"
"必須	インライン while	i=0; while [ \$i -lt 1 ]; do echo w; i=1; done	w"
"推奨	HISTSIZE 代入	HISTSIZE=5000; echo \$HISTSIZE	5000"
"推奨	プレーン代入	EDITOR=vim; echo \$EDITOR	vim"
"推奨	[[ 条件	[[ -n \"x\" ]] && echo nn	nn"
"推奨	配列	ports=(80 443); echo \${ports[1]}	443"
"許容不可	shopt	shopt -s nullglob 2>/dev/null; echo shopt_ok	"
"許容不可	プロセス置換	cat <(echo x) 2>/dev/null	"
)

pass=0
fail=0
soft=0
rows=""

for entry in "${CASES[@]}"; do
  category=$(printf '%s' "$entry" | cut -f1)
  desc=$(printf '%s' "$entry" | cut -f2)
  cmd_raw=$(printf '%s' "$entry" | cut -f3)
  expect=$(printf '%s' "$entry" | cut -f4)
  cmd=$(printf '%b' "$cmd_raw")

  actual=$("$MSH" -c "$cmd" 2>/dev/null)
  code=$?

  ok=0
  mark="❌"
  if [[ "$category" == "許容不可" ]]; then
    # bash 固有 — 失敗または未実装は「期待どおり」
    if [[ $code -ne 0 ]] || [[ -z "$expect" && "$actual" == "" ]]; then
      ok=1
      mark="⏭"
      soft=$((soft + 1))
    fi
  elif [[ -z "$expect" ]]; then
    [[ $code -eq 0 ]] && ok=1 && mark="✅"
  else
    [[ $code -eq 0 && "$actual" == *"$expect"* ]] && ok=1 && mark="✅"
  fi

  if [[ $ok -eq 1 && "$category" != "許容不可" ]]; then
    pass=$((pass + 1))
  elif [[ $ok -eq 0 && "$category" != "許容不可" ]]; then
    fail=$((fail + 1))
  fi

  disp=$(printf '%s' "$cmd" | sed 's/|/\\|/g; s/	/ /g' | tr '\n' '⏎')
  rows+="| $category | $desc | \`$disp\` | $mark |"$'\n'
done

required_total=7
required_pass=$pass
# 必須のみ再カウント
required_pass=0
for entry in "${CASES[@]}"; do
  category=$(printf '%s' "$entry" | cut -f1)
  [[ "$category" != "必須" ]] && continue
  cmd=$(printf '%b' "$(printf '%s' "$entry" | cut -f3)")
  expect=$(printf '%s' "$entry" | cut -f4)
  actual=$("$MSH" -c "$cmd" 2>/dev/null)
  code=$?
  if [[ -z "$expect" ]]; then
    [[ $code -eq 0 ]] && required_pass=$((required_pass + 1))
  else
    [[ $code -eq 0 && "$actual" == *"$expect"* ]] && required_pass=$((required_pass + 1))
  fi
done

score=$(awk "BEGIN { printf \"%.0f\", ($required_pass/$required_total)*100 }")

{
  echo "# dotfiles 互換回帰"
  echo
  echo "> 自動生成: \`scripts/dotfiles-compat.sh\` / 計測日: $(date +%Y-%m-%d)"
  echo "> 目的: 代表的 \`.bashrc\` パターンが **msh -c** で通るか（本番デフォルトシェル化の信頼ゲート）"
  echo
  echo "## サマリー"
  echo
  echo "| 指標 | 値 |"
  echo "|---|---|"
  echo "| 必須パターン | $required_pass / $required_total |"
  echo "| **必須通過率** | **${score}%** |"
  echo "| 推奨+必須 通過 | $pass / $((pass + fail)) |"
  echo "| bash 固有（許容不可） | $soft 件スキップ |"
  echo
  echo "CI ゲート: 必須通過率 **≥ 85%**（現目標 100%）"
  echo
  echo "## ケース別"
  echo
  echo "| 区分 | パターン | コマンド | 結果 |"
  echo "|---|---|---|---|"
  printf "%s" "$rows"
} > "$OUT"

echo
echo "必須通過率: ${score}% ($required_pass/$required_total)"
echo "レポート: $OUT"

if [[ "$score" -lt 85 ]]; then
  echo "ERROR: dotfiles 必須通過率が 85% 未満" >&2
  exit 1
fi
