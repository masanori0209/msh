#!/usr/bin/env bash
# Cursor MCP 用ラッパ — リポジトリ内の msh バイナリを解決して `msh --mcp` を起動する。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [[ -n "${MSH_BIN:-}" && -x "${MSH_BIN}" ]]; then
  exec env MSH_SKIP_RC=1 "${MSH_BIN}" --mcp
fi

for candidate in \
  "${ROOT}/msh/target/release/msh" \
  "${ROOT}/msh/target/debug/msh"; do
  if [[ -x "${candidate}" ]]; then
    exec env MSH_SKIP_RC=1 "${candidate}" --mcp
  fi
done

if command -v msh >/dev/null 2>&1; then
  exec env MSH_SKIP_RC=1 msh --mcp
fi

echo "msh-mcp: msh binary not found." >&2
echo "  cd msh && cargo build" >&2
echo "  or set MSH_BIN=/path/to/msh" >&2
exit 1
