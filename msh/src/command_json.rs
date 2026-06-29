//! `--json` / `--agent` / MCP 共用の構造化出力。

use crate::error::MshError;
use std::path::Path;

pub const DEFAULT_JSON_MAX_BYTES: usize = 65_536;

#[derive(Debug, Clone)]
pub struct JsonOutputOptions {
    pub max_bytes: usize,
    pub include_meta: bool,
}

impl Default for JsonOutputOptions {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_JSON_MAX_BYTES,
            include_meta: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CommandMeta {
    pub cwd: String,
    pub git_branch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TruncatedOutput {
    pub text: String,
    pub truncated: bool,
    pub original_bytes: usize,
}

pub fn truncate_output(text: &str, max_bytes: usize) -> TruncatedOutput {
    if max_bytes == 0 || text.len() <= max_bytes {
        return TruncatedOutput {
            text: text.to_string(),
            truncated: false,
            original_bytes: text.len(),
        };
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    TruncatedOutput {
        text: text[..end].to_string(),
        truncated: true,
        original_bytes: text.len(),
    }
}

pub fn collect_meta() -> CommandMeta {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    CommandMeta {
        cwd: cwd.clone(),
        git_branch: detect_git_branch(Path::new(&cwd)),
    }
}

fn detect_git_branch(cwd: &Path) -> Option<String> {
    let head = cwd.join(".git/HEAD");
    let content = std::fs::read_to_string(head).ok()?;
    let content = content.trim();
    if let Some(branch) = content.strip_prefix("ref: refs/heads/") {
        return Some(branch.to_string());
    }
    if content.len() == 40 && content.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some("detached".into());
    }
    None
}

#[derive(Debug, Clone)]
pub struct CommandJsonInput<'a> {
    pub command: &'a str,
    pub exit_code: i32,
    pub duration_ms: u128,
    pub stdout: &'a str,
    pub stderr: &'a str,
    pub error: Option<&'a MshError>,
    pub opts: JsonOutputOptions,
    pub extra_fields: String,
}

pub fn build_command_json(input: &CommandJsonInput<'_>) -> String {
    let out = truncate_output(input.stdout, input.opts.max_bytes);
    let err = truncate_output(input.stderr, input.opts.max_bytes);

    let mut json = String::with_capacity(512 + out.text.len() + err.text.len());
    json.push('{');
    json.push_str(&format!("\"command\":\"{}\",", json_escape(input.command)));
    json.push_str(&format!(
        "\"exit_code\":{exit_code},",
        exit_code = input.exit_code
    ));
    json.push_str(&format!("\"duration_ms\":{},", input.duration_ms));

    if input.opts.include_meta {
        let meta = collect_meta();
        json.push_str(&format!("\"cwd\":\"{}\",", json_escape(&meta.cwd)));
        if let Some(branch) = &meta.git_branch {
            json.push_str(&format!("\"git_branch\":\"{}\",", json_escape(branch)));
        }
    }

    append_stream_field(&mut json, "stdout", &out);
    append_stream_field(&mut json, "stderr", &err);

    if let Some(err) = input.error {
        append_structured_error(&mut json, err);
    }

    if !input.extra_fields.is_empty() {
        json.push(',');
        json.push_str(input.extra_fields.trim_start_matches(','));
    }

    json.push('}');
    json
}

pub fn build_blocked_json(
    error: &MshError,
    assessment: Option<&crate::agent::AgentAssessment>,
) -> String {
    let mut json = String::from("{\"action\":\"blocked\"");
    if let Some(a) = assessment {
        json.push_str(&format!(
            ",\"risk\":\"{}\"",
            crate::agent::risk_label(a.risk)
        ));
        if !a.reason.is_empty() {
            json.push_str(&format!(",\"reason\":\"{}\"", json_escape(&a.reason)));
        }
    }
    append_structured_error(&mut json, error);
    json.push('}');
    json
}

pub fn build_agent_dry_run_json(
    command: &str,
    assessment: &crate::agent::AgentAssessment,
) -> String {
    format!(
        "{{\"action\":\"dry_run\",\"risk\":\"{}\",\"command\":\"{}\"}}",
        crate::agent::risk_label(assessment.risk),
        json_escape(command)
    )
}

pub fn build_timeout_json(command: &str, timeout_ms: u64) -> String {
    format!(
        "{{\"action\":\"timeout\",\"exit_code\":124,\"command\":\"{}\",\"timeout_ms\":{timeout_ms}}}",
        json_escape(command)
    )
}

fn append_stream_field(json: &mut String, name: &str, out: &TruncatedOutput) {
    json.push_str(&format!("\"{name}\":\"{}\",", json_escape(&out.text)));
    json.push_str(&format!("\"{name}_bytes\":{},", out.original_bytes));
    json.push_str(&format!(
        "\"{name}_truncated\":{}",
        if out.truncated { "true" } else { "false" }
    ));
}

fn append_structured_error(json: &mut String, err: &MshError) {
    json.push_str(&format!(
        ",\"error\":{{\"kind\":\"{}\",\"message\":\"{}\"",
        err.kind(),
        json_escape(&err.to_string())
    ));
    if let Some(workaround) = err.workaround() {
        json.push_str(&format!(",\"workaround\":\"{}\"", json_escape(workaround)));
    }
    if let Some(suggestion) = err.suggestion() {
        json.push_str(&format!(",\"suggestion\":\"{}\"", json_escape(&suggestion)));
    }
    json.push_str("}}");
}

pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_char_boundary() {
        let text = "あいう".repeat(30_000);
        let out = truncate_output(&text, 100);
        assert!(out.truncated);
        assert!(out.text.len() <= 100);
        assert!(std::str::from_utf8(out.text.as_bytes()).is_ok());
    }

    #[test]
    fn structured_error_in_json() {
        let err = MshError::CommandNotFound("nosuch".into());
        let json = build_command_json(&CommandJsonInput {
            command: "nosuch",
            exit_code: 127,
            duration_ms: 1,
            stdout: "",
            stderr: "",
            error: Some(&err),
            opts: JsonOutputOptions::default(),
            extra_fields: String::new(),
        });
        assert!(json.contains("\"kind\":\"command_not_found\""));
    }
}
