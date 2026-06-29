#!/usr/bin/env bash
# Claude Code 等向け — CLAUDE_CODE_SHELL に指定する msh ラッパ。
# `-c 'cmd'` 形式の subprocess を `msh --agent -c` に転送する。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

resolve_msh() {
  if [[ -n "${MSH_BIN:-}" && -x "${MSH_BIN}" ]]; then
    echo "${MSH_BIN}"
    return
  fi
  local candidate
  for candidate in \
    "${ROOT}/msh/target/release/msh" \
    "${ROOT}/msh/target/debug/msh"; do
    if [[ -x "${candidate}" ]]; then
      echo "${candidate}"
      return
    fi
  done
  if command -v msh >/dev/null 2>&1; then
    command -v msh
    return
  fi
  echo "msh-agent-shell: msh binary not found" >&2
  exit 127
}

MSH="$(resolve_msh)"
export MSH_SKIP_RC="${MSH_SKIP_RC:-1}"

if [[ "${1:-}" == "-c" && -n "${2:-}" ]]; then
  exec "${MSH}" --agent -c "$2"
fi

exec "${MSH}" "$@"
