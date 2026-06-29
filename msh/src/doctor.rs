//! `msh doctor` — 設定・agent・MCP の健全性チェック。

use crate::agent::{self, AgentOptions};
use crate::config::{AgentSettings, ShellConfig};
use crate::error::Result;
use crate::mcp;
use crate::shell::Shell;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone)]
pub struct Check {
    pub name: &'static str,
    pub status: CheckStatus,
    pub detail: String,
}

pub struct DoctorReport {
    pub checks: Vec<Check>,
}

impl DoctorReport {
    pub fn exit_code(&self) -> i32 {
        if self.checks.iter().any(|c| c.status == CheckStatus::Fail) {
            1
        } else {
            0
        }
    }
}

pub fn run(verbose: bool) -> Result<DoctorReport> {
    let mut checks = Vec::new();

    let home = match std::env::var("HOME") {
        Ok(h) => PathBuf::from(h),
        Err(_) => {
            checks.push(fail("HOME", "HOME is not set"));
            return Ok(DoctorReport { checks });
        }
    };

    checks.push(pass("HOME", home.display().to_string()));

    let config_path = home.join(".config/msh/config.toml");
    if config_path.is_file() {
        checks.push(pass("config.toml", config_path.display().to_string()));
        let config = ShellConfig::default();
        if config.agent.json_max_bytes > 0 {
            checks.push(pass(
                "agent.json_max_bytes",
                config.agent.json_max_bytes.to_string(),
            ));
        }
    } else {
        checks.push(warn("config.toml", "missing — run msh setup"));
    }

    match std::env::current_exe() {
        Ok(exe) => checks.push(pass("msh binary", exe.display().to_string())),
        Err(e) => checks.push(fail("msh binary", e.to_string())),
    }

    checks.extend(check_agent_gate());
    checks.extend(check_json_capture());
    checks.extend(check_mcp());

    checks.extend(check_integration(
        "Cursor MCP",
        &home.join(".cursor/mcp.json"),
        "\"msh\"",
    ));
    checks.extend(check_integration(
        "Claude MCP",
        &home.join(".claude/settings.json"),
        "\"msh\"",
    ));
    checks.extend(check_integration(
        "Codex MCP",
        &home.join(".codex/config.toml"),
        "[mcp_servers.msh]",
    ));

    if verbose {
        for c in &checks {
            eprintln!("{:?} {} — {}", c.status, c.name, c.detail);
        }
    }

    Ok(DoctorReport { checks })
}

pub fn report_json(report: &DoctorReport) -> String {
    let mut out = String::from("{\"checks\":[");
    for (i, c) in report.checks.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let status = match c.status {
            CheckStatus::Pass => "pass",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "fail",
        };
        out.push_str(&format!(
            "{{\"name\":\"{}\",\"status\":\"{status}\",\"detail\":\"{}\"}}",
            crate::command_json::json_escape(c.name),
            crate::command_json::json_escape(&c.detail)
        ));
    }
    out.push_str("],\"exit_code\":");
    out.push_str(&report.exit_code().to_string());
    out.push('}');
    out
}

pub fn print_report(report: &DoctorReport, language: crate::config::Language) {
    let title = match language {
        crate::config::Language::Ja => "msh doctor",
        crate::config::Language::En => "msh doctor",
    };
    println!("{title}");
    println!();

    for c in &report.checks {
        let icon = match c.status {
            CheckStatus::Pass => "✓",
            CheckStatus::Warn => "△",
            CheckStatus::Fail => "✗",
        };
        println!("  {icon} {} — {}", c.name, c.detail);
    }

    println!();
    match report.exit_code() {
        0 if report.checks.iter().any(|c| c.status == CheckStatus::Warn) => {
            println!(
                "{}",
                msg(
                    language,
                    "必須チェックは通過（警告あり）。msh setup で IDE 連携できます。",
                    "Required checks passed (with warnings). Run msh setup for IDE integration."
                )
            );
        }
        0 => {
            println!("{}", msg(language, "すべて OK", "All checks passed"));
        }
        _ => {
            println!(
                "{}",
                msg(
                    language,
                    "失敗あり — 上記を修正してから msh doctor を再実行",
                    "Failures detected — fix above and re-run msh doctor"
                )
            );
        }
    }
}

fn check_agent_gate() -> Vec<Check> {
    let settings = AgentSettings::default();
    let mut out = Vec::new();
    if agent::gate("echo ok", AgentOptions::default(), &settings).is_ok() {
        out.push(pass("agent gate (safe)", "echo allowed"));
    } else {
        out.push(fail("agent gate (safe)", "echo blocked unexpectedly"));
    }
    if agent::gate("rm -rf /tmp/x", AgentOptions::default(), &settings).is_err() {
        out.push(pass("agent gate (destructive)", "rm -rf blocked"));
    } else {
        out.push(fail("agent gate (destructive)", "rm -rf was not blocked"));
    }
    out
}

fn check_json_capture() -> Vec<Check> {
    let mut out = Vec::new();
    let config = ShellConfig::default();
    let mut shell = Shell::with_config(config);
    shell.init_for_agent();
    let (code, json) = shell.build_command_json("echo doctor-ok");
    if code == 0 && json.contains("doctor-ok") && json.contains("\"exit_code\":0") {
        out.push(pass("--json capture", "stdout + exit_code"));
    } else {
        out.push(fail("--json capture", format!("unexpected: {json}")));
    }
    out
}

fn check_mcp() -> Vec<Check> {
    let mut out = Vec::new();
    let mut shell = Shell::with_config(ShellConfig::default());
    shell.init_for_agent();

    match mcp::handle_line(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        &mut shell,
    ) {
        Ok(resp) if resp.contains("protocolVersion") => {
            out.push(pass("MCP initialize", "protocolVersion ok"));
        }
        Ok(resp) => out.push(fail("MCP initialize", resp)),
        Err(e) => out.push(fail("MCP initialize", e.to_string())),
    }

    match mcp::handle_line(
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        &mut shell,
    ) {
        Ok(resp) if resp.contains("msh_run") => out.push(pass("MCP tools/list", "msh_run")),
        Ok(resp) => out.push(fail("MCP tools/list", resp)),
        Err(e) => out.push(fail("MCP tools/list", e.to_string())),
    }

    match mcp::handle_line(
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"msh_run","arguments":{"command":"echo doctor-mcp"}}}"#,
        &mut shell,
    ) {
        Ok(resp) if resp.contains("doctor-mcp") => out.push(pass("MCP tools/call", "echo ok")),
        Ok(resp) => out.push(fail("MCP tools/call", resp)),
        Err(e) => out.push(fail("MCP tools/call", e.to_string())),
    }

    out
}

fn check_integration(name: &'static str, path: &Path, marker: &str) -> Vec<Check> {
    if !path.is_file() {
        return vec![warn(name, "not configured")];
    }
    match std::fs::read_to_string(path) {
        Ok(content) if content.contains(marker) => vec![pass(name, path.display().to_string())],
        Ok(_) => vec![warn(
            name,
            "file exists but msh entry missing — run msh setup",
        )],
        Err(e) => vec![warn(name, e.to_string())],
    }
}

fn pass(name: &'static str, detail: impl Into<String>) -> Check {
    Check {
        name,
        status: CheckStatus::Pass,
        detail: detail.into(),
    }
}

fn warn(name: &'static str, detail: impl Into<String>) -> Check {
    Check {
        name,
        status: CheckStatus::Warn,
        detail: detail.into(),
    }
}

fn fail(name: &'static str, detail: impl Into<String>) -> Check {
    Check {
        name,
        status: CheckStatus::Fail,
        detail: detail.into(),
    }
}

fn msg(language: crate::config::Language, ja: &str, en: &str) -> String {
    match language {
        crate::config::Language::Ja => ja.to_string(),
        crate::config::Language::En => en.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_core_checks_pass() {
        let report = run(false).expect("doctor");
        assert!(report
            .checks
            .iter()
            .any(|c| c.name == "agent gate (destructive)"));
        assert_eq!(
            report
                .checks
                .iter()
                .find(|c| c.name == "agent gate (destructive)")
                .map(|c| c.status),
            Some(CheckStatus::Pass)
        );
    }
}
