#!/usr/bin/env bash
# msh --mcp の MCP ハンドシェイク smoke test（Cursor 接続前の CI/ローカル検証）
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${ROOT}/msh"

echo "==> cargo build --quiet"
cargo build --quiet

MSH="${CARGO_TARGET_DIR:-target}/debug/msh"
export MSH_BIN="${ROOT}/msh/${MSH}"
WRAPPER="${ROOT}/scripts/msh-mcp.sh"

if [[ ! -x "${MSH_BIN}" ]]; then
  echo "error: msh binary not found at ${MSH_BIN}" >&2
  exit 1
fi
if [[ ! -x "${WRAPPER}" ]]; then
  echo "error: wrapper not executable: ${WRAPPER}" >&2
  exit 1
fi

run_mcp() {
  env MSH_BIN="${MSH_BIN}" MSH_SKIP_RC=1 "${WRAPPER}"
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

pass() {
  echo "PASS: $*"
}

echo "==> MCP handshake via scripts/msh-mcp.sh"
OUT="$(run_mcp <<'EOF'
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"verify-mcp","version":"1"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"msh_run","arguments":{"command":"echo mcp-verify-ok"}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"msh_run","arguments":{"command":"rm -rf /tmp/mcp-block-test"}}}
EOF
)"

echo "${OUT}"

echo "${OUT}" | grep -q '"protocolVersion":"2024-11-05"' || fail "initialize missing protocolVersion"
pass "initialize"

LINE_COUNT="$(printf '%s\n' "${OUT}" | sed '/^$/d' | wc -l | tr -d ' ')"
[[ "${LINE_COUNT}" -eq 4 ]] || fail "expected 4 JSON lines (notification has no response), got ${LINE_COUNT}"
pass "notifications/initialized silent"

echo "${OUT}" | grep -q 'msh_run' || fail "tools/list missing msh_run"
pass "tools/list"

echo "${OUT}" | grep -q 'mcp-verify-ok' || fail "tools/call echo failed"
echo "${OUT}" | grep -q '"text":"{' || fail "tools/call text must be JSON string (MCP spec)"
pass "tools/call echo"

echo "${OUT}" | grep -q 'blocked' || fail "destructive command should be blocked"
pass "tools/call destructive blocked"

echo
echo "All MCP smoke checks passed."
