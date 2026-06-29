#!/usr/bin/env bash
# エージェント向け msh ラッパ — 1 引数に shell 一行を受け取り `msh --agent -c` で実行する。
# Claude Code の CLAUDE_CODE_SHELL_PREFIX や CI から利用。
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
  echo "msh-agent-exec: msh binary not found (set MSH_BIN or run: cd msh && cargo build)" >&2
  exit 127
}

MSH="$(resolve_msh)"
CMD="${1:-}"

if [[ -z "${CMD}" ]]; then
  echo "msh-agent-exec: expected one shell command argument" >&2
  exit 2
fi

export MSH_SKIP_RC="${MSH_SKIP_RC:-1}"
exec "${MSH}" --agent -c "${CMD}"
