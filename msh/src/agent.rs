//! B-2: エージェント安全実行 — 破壊的コマンドの分類と `--agent` ゲート。

use crate::config::AgentSettings;
use crate::error::MshError;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Safe,
    Caution,
    Destructive,
}

#[derive(Debug, Clone)]
pub struct AgentAssessment {
    pub risk: RiskLevel,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AgentOptions {
    pub dry_run: bool,
    pub force: bool,
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub command: String,
    pub action: &'static str,
    pub risk: RiskLevel,
    pub exit_code: Option<i32>,
    pub reason: Option<String>,
}

pub fn assess(command: &str) -> AgentAssessment {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return AgentAssessment {
            risk: RiskLevel::Safe,
            reason: String::new(),
        };
    }

    let lower = trimmed.to_ascii_lowercase();

    if matches_destructive(&lower) {
        return AgentAssessment {
            risk: RiskLevel::Destructive,
            reason: "destructive pattern detected (rm -rf, mkfs, dd, fork bomb, etc.)".into(),
        };
    }

    if matches_caution(&lower) {
        return AgentAssessment {
            risk: RiskLevel::Caution,
            reason: "potentially destructive command (rm, mv, chmod, curl|sh, etc.)".into(),
        };
    }

    AgentAssessment {
        risk: RiskLevel::Safe,
        reason: String::new(),
    }
}

pub fn risk_label(risk: RiskLevel) -> &'static str {
    match risk {
        RiskLevel::Safe => "safe",
        RiskLevel::Caution => "caution",
        RiskLevel::Destructive => "destructive",
    }
}

/// 実行前ゲート（allowlist / sandbox / リスク分類）。
pub fn gate(command: &str, opts: AgentOptions, settings: &AgentSettings) -> Result<(), MshError> {
    let force = opts.force || std::env::var("MSH_AGENT_FORCE").is_ok();
    if opts.dry_run {
        return Ok(());
    }

    check_allowlist(command, settings)?;
    if let Some(root) = settings.sandbox_root.as_deref() {
        verify_cwd_in_sandbox(root)?;
    }

    let assessment = assess(command);
    match assessment.risk {
        RiskLevel::Safe => Ok(()),
        RiskLevel::Caution if force || !settings.block_caution => Ok(()),
        RiskLevel::Caution => Err(MshError::ScriptError(format!(
            "agent: blocked caution command ({}) — use --agent-force or MSH_AGENT_FORCE=1",
            assessment.reason
        ))),
        RiskLevel::Destructive if force => Ok(()),
        RiskLevel::Destructive => Err(MshError::ScriptError(format!(
            "agent: blocked destructive command ({}) — use --agent-force or MSH_AGENT_FORCE=1",
            assessment.reason
        ))),
    }
}

pub fn check_allowlist(command: &str, settings: &AgentSettings) -> Result<(), MshError> {
    if settings.allowlist.is_empty() {
        return Ok(());
    }
    let first = first_command_word(command).unwrap_or("");
    if settings
        .allowlist
        .iter()
        .any(|allowed| command_matches_allow(first, allowed))
    {
        return Ok(());
    }
    Err(MshError::ScriptError(format!(
        "agent: command not in allowlist (first token: {first})"
    )))
}

fn command_matches_allow(first: &str, allowed: &str) -> bool {
    first == allowed || first.starts_with(&format!("{allowed}/"))
}

fn first_command_word(command: &str) -> Option<&str> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    // 先頭の env 代入 `VAR=val cmd` をスキップ。
    let mut rest = trimmed;
    while let Some((word, tail)) = rest.split_once(' ') {
        if word.contains('=') && !word.starts_with('=') {
            rest = tail.trim_start();
            continue;
        }
        return Some(word);
    }
    Some(rest)
}

pub fn resolve_path_in_sandbox(sandbox_root: &str, target: &Path) -> Result<PathBuf, MshError> {
    let sandbox = canonicalize_lossy(Path::new(sandbox_root))?;
    let resolved = if target.is_absolute() {
        target.to_path_buf()
    } else {
        std::env::current_dir().map_err(MshError::Io)?.join(target)
    };
    let resolved = canonicalize_lossy(&resolved)?;
    if !resolved.starts_with(&sandbox) {
        return Err(MshError::ScriptError(format!(
            "agent: path outside sandbox: {}",
            resolved.display()
        )));
    }
    Ok(resolved)
}

pub fn verify_cwd_in_sandbox(sandbox_root: &str) -> Result<(), MshError> {
    let cwd = std::env::current_dir().map_err(MshError::Io)?;
    resolve_path_in_sandbox(sandbox_root, &cwd)?;
    Ok(())
}

pub fn enforce_cd_sandbox(sandbox_root: &str, target: &Path) -> Result<(), MshError> {
    resolve_path_in_sandbox(sandbox_root, target)?;
    Ok(())
}

fn canonicalize_lossy(path: &Path) -> Result<PathBuf, MshError> {
    fs::canonicalize(path).or_else(|_| {
        if path.is_absolute() {
            Ok(path.to_path_buf())
        } else {
            Err(MshError::DirNotFound(path.display().to_string()))
        }
    })
}

pub fn write_audit(settings: &AgentSettings, entry: &AuditEntry) -> Result<(), MshError> {
    let Some(path) = settings.audit_log.as_deref() else {
        return Ok(());
    };
    let line = format!(
        "{{\"command\":\"{}\",\"action\":\"{}\",\"risk\":\"{}\"",
        json_escape(&entry.command),
        entry.action,
        risk_label(entry.risk)
    );
    let mut line = line;
    if let Some(code) = entry.exit_code {
        line.push_str(&format!(",\"exit_code\":{code}"));
    }
    if let Some(reason) = &entry.reason {
        line.push_str(&format!(",\"reason\":\"{}\"", json_escape(reason)));
    }
    line.push_str("}\n");
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(MshError::Io)?
        .write_all(line.as_bytes())
        .map_err(MshError::Io)
}

pub fn action_label(opts: AgentOptions, blocked: bool) -> &'static str {
    if blocked {
        "blocked"
    } else if opts.dry_run {
        "dry_run"
    } else {
        "executed"
    }
}

fn json_escape(s: &str) -> String {
    crate::command_json::json_escape(s)
}

fn matches_destructive(lower: &str) -> bool {
    const PATTERNS: &[&str] = &[
        "rm -rf",
        "rm -fr",
        "rm -r /",
        "rm -rf /",
        "rm -rf ~",
        "rm -rf /*",
        "mkfs.",
        "dd if=",
        ":(){ :|:&",
        "chmod -r 777",
        "chmod 777 /",
        "> /dev/sd",
        "shutdown ",
        "reboot",
        "init 0",
        "kill -9 1",
    ];
    PATTERNS.iter().any(|p| lower.contains(p))
        || (lower.contains("rm ") && lower.contains(" -") && lower.contains('f'))
}

fn matches_caution(lower: &str) -> bool {
    const START_PATTERNS: &[&str] = &["rm ", "mv ", "chmod ", "chown ", "curl ", "wget ", "sudo "];
    const CONTAINS_PATTERNS: &[&str] = &[
        " rm ",
        " mv ",
        " chmod ",
        " chown ",
        " curl ",
        " wget ",
        "| sh",
        "| bash",
        "sudo ",
        " git reset --hard",
        " git clean -f",
    ];
    START_PATTERNS.iter().any(|p| lower.starts_with(p))
        || CONTAINS_PATTERNS.iter().any(|p| lower.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentSettings;

    #[test]
    fn blocks_rm_rf_without_force() {
        let assessment = assess("rm -rf /tmp/foo");
        assert_eq!(assessment.risk, RiskLevel::Destructive);
        assert!(gate(
            "rm -rf /tmp/foo",
            AgentOptions::default(),
            &AgentSettings::default()
        )
        .is_err());
        assert!(gate(
            "rm -rf /tmp/foo",
            AgentOptions {
                force: true,
                ..Default::default()
            },
            &AgentSettings::default()
        )
        .is_ok());
    }

    #[test]
    fn allows_echo() {
        assert_eq!(assess("echo hi").risk, RiskLevel::Safe);
        assert!(gate(
            "echo hi",
            AgentOptions::default(),
            &AgentSettings::default()
        )
        .is_ok());
    }

    #[test]
    fn caution_rm_without_f() {
        assert_eq!(assess("rm file.txt").risk, RiskLevel::Caution);
        assert!(gate(
            "rm file.txt",
            AgentOptions::default(),
            &AgentSettings::default()
        )
        .is_ok());
    }

    #[test]
    fn block_caution_when_configured() {
        let settings = AgentSettings {
            block_caution: true,
            ..AgentSettings::default()
        };
        assert!(gate("rm file.txt", AgentOptions::default(), &settings).is_err());
    }

    #[test]
    fn allowlist_blocks_unknown_command() {
        let settings = AgentSettings {
            allowlist: vec!["echo".into(), "cargo".into()],
            ..AgentSettings::default()
        };
        assert!(gate("echo hi", AgentOptions::default(), &settings).is_ok());
        assert!(gate("ls", AgentOptions::default(), &settings).is_err());
    }

    #[test]
    fn first_word_skips_env_assignment() {
        assert_eq!(first_command_word("FOO=bar echo hi"), Some("echo"));
    }
}
