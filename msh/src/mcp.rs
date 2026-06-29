//! B-3: 最小 MCP サーバ（stdio JSON-RPC）— Cursor / Claude から msh を安全に駆動。

use crate::agent::{self, AgentOptions};
use crate::config::ShellConfig;
use crate::error::MshError;
use crate::shell::Shell;
use std::io::{self, BufRead, Write};

const PROTOCOL_VERSION: &str = "2024-11-05";

/// stdio で MCP JSON-RPC を処理する（`msh --mcp`）。
/// 同一プロセス内で Shell 状態（cwd 等）を維持する。
pub fn run_server() -> Result<(), MshError> {
    let config = ShellConfig::from_env_and_args();
    let mut shell = Shell::with_config(config);
    shell.init_for_agent();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line.map_err(MshError::Io)?;
        if line.trim().is_empty() {
            continue;
        }
        let response = handle_line(&line, &mut shell)?;
        if response.is_empty() {
            continue;
        }
        writeln!(stdout, "{response}").map_err(MshError::Io)?;
        stdout.flush().map_err(MshError::Io)?;
    }
    Ok(())
}

/// stdio MCP 1 行処理（`msh doctor` からも利用）。
pub fn handle_line(line: &str, shell: &mut Shell) -> Result<String, MshError> {
    let id = extract_json_id(line);
    let method = extract_json_string_field(line, "method").unwrap_or_default();

    let result = match method.as_str() {
        "initialize" => ok_result(
            id,
            format!(
                r#"{{"protocolVersion":"{PROTOCOL_VERSION}","capabilities":{{"tools":{{}}}},"serverInfo":{{"name":"msh","version":"0.7.4"}}}}"#
            ),
        ),
        "notifications/initialized" | "initialized" => {
            if id.is_empty() {
                return Ok(String::new());
            }
            ok_result(id, "null".into())
        }
        "tools/list" => ok_result(
            id,
            r#"{"tools":[{"name":"msh_run","description":"Run a shell command via msh with agent safety and JSON capture","inputSchema":{"type":"object","properties":{"command":{"type":"string"},"dry_run":{"type":"boolean"},"force":{"type":"boolean"}},"required":["command"]}}]}"#.into(),
        ),
        "tools/call" => handle_tools_call(line, id, shell),
        "ping" => ok_result(id, r#"{"status":"ok"}"#.into()),
        _ => err_result(id, -32601, format!("method not found: {method}")),
    };
    Ok(result)
}

fn handle_tools_call(line: &str, id: String, shell: &mut Shell) -> String {
    let name = extract_nested_string(line, "params", "name").unwrap_or_default();
    if name != "msh_run" {
        return err_result(id, -32602, format!("unknown tool: {name}"));
    }

    let command = extract_nested_string(line, "arguments", "command")
        .or_else(|| extract_nested_string(line, "params", "command"))
        .unwrap_or_default();
    if command.is_empty() {
        return err_result(id, -32602, "missing command".into());
    }

    let dry_run = line.contains("\"dry_run\":true") || line.contains("\"dry_run\": true");
    let force = line.contains("\"force\":true") || line.contains("\"force\": true");
    let opts = AgentOptions { dry_run, force };

    let assessment = agent::assess(&command);
    let settings = shell.config.agent.clone();

    if opts.dry_run {
        let inner = crate::command_json::build_agent_dry_run_json(&command, &assessment);
        let _ = agent::write_audit(
            &settings,
            &agent::AuditEntry {
                command: command.clone(),
                action: "dry_run",
                risk: assessment.risk,
                exit_code: None,
                reason: Some(assessment.reason.clone()),
            },
        );
        return ok_result(id, tool_text_result(&inner));
    }

    if let Err(e) = agent::gate(&command, opts, &settings) {
        let inner = crate::command_json::build_blocked_json(&e, Some(&assessment));
        let _ = agent::write_audit(
            &settings,
            &agent::AuditEntry {
                command: command.clone(),
                action: "blocked",
                risk: assessment.risk,
                exit_code: Some(1),
                reason: Some(e.to_string()),
            },
        );
        return ok_result(id, tool_text_result(&inner));
    }

    let (_, json) = shell.build_command_json(&command);
    let mut json = json;
    if json.ends_with('}') {
        json.truncate(json.len() - 1);
        json.push_str(&format!(
            ",\"action\":\"executed\",\"risk\":\"{}\"}}",
            agent::risk_label(assessment.risk)
        ));
    }
    let _ = agent::write_audit(
        &settings,
        &agent::AuditEntry {
            command: command.clone(),
            action: "executed",
            risk: assessment.risk,
            exit_code: None,
            reason: None,
        },
    );
    shell.finalize_agent_session();
    ok_result(id, tool_text_result(&json))
}

/// MCP `CallToolResult`: `content[].text` は文字列（JSON 本文を escape して格納）。
fn tool_text_result(text: &str) -> String {
    format!(
        r#"{{"content":[{{"type":"text","text":"{}"}}]}}"#,
        json_escape(text)
    )
}

fn ok_result(id: String, result: String) -> String {
    if id.is_empty() {
        return String::new();
    }
    format!(r#"{{"jsonrpc":"2.0","id":{id},"result":{result}}}"#)
}

fn err_result(id: String, code: i32, message: String) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":{},"error":{{"code":{},"message":"{}"}}}}"#,
        if id.is_empty() { "null".into() } else { id },
        code,
        json_escape(&message)
    )
}

fn extract_json_id(line: &str) -> String {
    extract_json_number_field(line, "id").unwrap_or_default()
}

fn extract_json_number_field(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\":");
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();
    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '-')
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn extract_json_string_field(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\":");
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    let mut out = String::new();
    let mut chars = rest[1..].chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(out),
            '\\' => {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            c => out.push(c),
        }
    }
    None
}

fn extract_nested_string(json: &str, object_key: &str, field: &str) -> Option<String> {
    let pattern = format!("\"{object_key}\":");
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();
    extract_json_string_field(rest, field).or_else(|| {
        if let Some(brace) = rest.strip_prefix('{') {
            extract_json_string_field(brace, field)
        } else {
            None
        }
    })
}

fn json_escape(s: &str) -> String {
    crate::command_json::json_escape(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ShellConfig;

    fn test_shell() -> Shell {
        let mut shell = Shell::with_config(ShellConfig::default());
        shell.init_for_agent();
        shell
    }

    #[test]
    fn initialize_returns_capabilities() {
        let mut shell = test_shell();
        let resp = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            &mut shell,
        )
        .unwrap();
        assert!(resp.contains("protocolVersion"));
        assert!(resp.contains("msh"));
    }

    #[test]
    fn tools_list_includes_msh_run() {
        let mut shell = test_shell();
        let resp = handle_line(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
            &mut shell,
        )
        .unwrap();
        assert!(resp.contains("msh_run"));
    }

    #[test]
    fn tools_call_returns_text_as_json_string() {
        let mut shell = test_shell();
        let resp = handle_line(
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"msh_run","arguments":{"command":"echo mcp-test"}}}"#,
            &mut shell,
        )
        .unwrap();
        assert!(resp.contains(r#""type":"text""#));
        assert!(resp.contains(r#""text":"{\"command\""#) || resp.contains(r#""text":"{""#));
        assert!(resp.contains("mcp-test"));
    }

    #[test]
    fn initialized_notification_emits_no_response() {
        let mut shell = test_shell();
        let resp = handle_line(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#,
            &mut shell,
        )
        .unwrap();
        assert!(resp.is_empty());
    }

    #[test]
    fn mcp_session_preserves_cwd() {
        let mut shell = test_shell();
        let _ = handle_line(
            r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"msh_run","arguments":{"command":"cd /tmp && pwd"}}}"#,
            &mut shell,
        )
        .unwrap();
        let resp = handle_line(
            r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"msh_run","arguments":{"command":"pwd"}}}"#,
            &mut shell,
        )
        .unwrap();
        assert!(resp.contains("/tmp"));
    }
}
