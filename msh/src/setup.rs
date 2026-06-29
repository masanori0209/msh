//! `msh setup` — 初回セットアップウィザード（daily + agent + IDE 連携）。

use crate::config::{self, Language, SetupOptions};
use crate::error::{MshError, Result};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default)]
pub struct SetupFlags {
    pub yes: bool,
    pub strict: bool,
    pub skip_integrations: bool,
}

pub struct SetupResult {
    pub config_path: PathBuf,
    pub cursor: bool,
    pub claude: bool,
    pub codex: bool,
}

pub fn run(language: Language, flags: SetupFlags) -> Result<SetupResult> {
    let home = home_dir()?;
    let msh_bin = resolve_msh_bin()?;

    let opts = if flags.strict {
        SetupOptions::strict(&home)
    } else {
        SetupOptions::from_home(&home)
    };

    let mut opts = opts;
    let mut integrate_cursor = true;
    let mut integrate_claude = true;
    let mut integrate_codex = true;

    if flags.yes {
        if flags.strict {
            opts.block_caution = true;
        }
        if flags.skip_integrations {
            integrate_cursor = false;
            integrate_claude = false;
            integrate_codex = false;
        }
    } else {
        print_banner(language);
        opts.block_caution = ask_strict_mode(&mut io::stdin().lock(), language)?;
        if !ask_yes(
            &mut io::stdin().lock(),
            language,
            &msg(
                language,
                "Cursor / Claude Code / Codex に MCP 連携を追加しますか？",
                "Add MCP integration for Cursor / Claude Code / Codex?",
            ),
            true,
        )? {
            integrate_cursor = false;
            integrate_claude = false;
            integrate_codex = false;
        } else {
            integrate_cursor = agent_dir_available(&home, ".cursor");
            integrate_claude = agent_dir_available(&home, ".claude");
            integrate_codex = agent_dir_available(&home, ".codex");
            if integrate_cursor {
                integrate_cursor = ask_yes(
                    &mut io::stdin().lock(),
                    language,
                    "  Cursor (~/.cursor/mcp.json)",
                    true,
                )?;
            }
            if integrate_claude {
                integrate_claude = ask_yes(
                    &mut io::stdin().lock(),
                    language,
                    "  Claude Code (~/.claude/settings.json)",
                    true,
                )?;
            }
            if integrate_codex {
                integrate_codex = ask_yes(
                    &mut io::stdin().lock(),
                    language,
                    "  Codex (~/.codex/config.toml)",
                    true,
                )?;
            }
        }
    }

    let config_path = config::save_setup_config(&opts)?;

    let cursor = if integrate_cursor {
        integrate_cursor_mcp(&home, &msh_bin)?
    } else {
        false
    };
    let claude = if integrate_claude {
        integrate_claude_mcp(&home, &msh_bin)?
    } else {
        false
    };
    let codex = if integrate_codex {
        integrate_codex_mcp(&home, &msh_bin)?
    } else {
        false
    };

    print_summary(
        language,
        &config_path,
        &msh_bin,
        cursor,
        claude,
        codex,
        opts.block_caution,
    );
    Ok(SetupResult {
        config_path,
        cursor,
        claude,
        codex,
    })
}

fn home_dir() -> Result<PathBuf> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| MshError::ScriptError("HOME is not set".into()))
}

pub fn resolve_msh_bin() -> Result<PathBuf> {
    if let Ok(bin) = std::env::var("MSH_BIN") {
        let path = PathBuf::from(&bin);
        if path.is_file() {
            return Ok(path);
        }
    }
    std::env::current_exe().map_err(|e| MshError::ScriptError(format!("current_exe: {e}")))
}

fn agent_dir_available(home: &Path, dir: &str) -> bool {
    home.join(dir).is_dir() || home.join(dir).is_file()
}

fn mcp_server_entry(msh_bin: &Path) -> String {
    format!(
        r#""msh": {{
      "command": "{}",
      "args": ["--mcp"],
      "env": {{ "MSH_SKIP_RC": "1" }}
    }}"#,
        json_escape(&msh_bin.display().to_string())
    )
}

fn integrate_cursor_mcp(home: &Path, msh_bin: &Path) -> Result<bool> {
    let path = home.join(".cursor/mcp.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(MshError::Io)?;
    }
    merge_mcp_json_file(&path, &mcp_server_entry(msh_bin))
}

fn integrate_claude_mcp(home: &Path, msh_bin: &Path) -> Result<bool> {
    let path = home.join(".claude/settings.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(MshError::Io)?;
    }
    merge_mcp_json_file(&path, &mcp_server_entry(msh_bin))
}

fn merge_mcp_json_file(path: &Path, server_entry: &str) -> Result<bool> {
    if path.is_file() {
        let content = fs::read_to_string(path).map_err(MshError::Io)?;
        if content.contains("\"msh\"") {
            return Ok(false);
        }
        if let Some(updated) = inject_mcp_server(&content, server_entry) {
            fs::write(path, updated).map_err(MshError::Io)?;
            return Ok(true);
        }
    }
    let body = format!("{{\n  \"mcpServers\": {{\n    {server_entry}\n  }}\n}}\n");
    fs::write(path, body).map_err(MshError::Io)?;
    Ok(true)
}

fn inject_mcp_server(content: &str, server_entry: &str) -> Option<String> {
    if content.contains("\"msh\"") {
        return None;
    }
    if let Some(key_pos) = content.find("\"mcpServers\"") {
        let rest = &content[key_pos..];
        if let Some(rel) = rest.find('{') {
            let insert_at = key_pos + rel + 1;
            let mut out = String::with_capacity(content.len() + server_entry.len() + 8);
            out.push_str(&content[..insert_at]);
            if !content[..insert_at].ends_with('{') {
                return None;
            }
            let tail = &content[insert_at..];
            if tail.trim_start().starts_with('}') {
                out.push_str("\n    ");
                out.push_str(server_entry);
                out.push('\n');
            } else {
                out.push_str("\n    ");
                out.push_str(server_entry);
                out.push(',');
            }
            out.push_str(tail);
            return Some(out);
        }
    }
    None
}

fn integrate_codex_mcp(home: &Path, msh_bin: &Path) -> Result<bool> {
    let path = home.join(".codex/config.toml");
    if path.is_file() {
        let content = fs::read_to_string(&path).map_err(MshError::Io)?;
        if content.contains("[mcp_servers.msh]") {
            return Ok(false);
        }
        let block = codex_mcp_block(msh_bin);
        let mut out = content;
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&block);
        fs::write(&path, out).map_err(MshError::Io)?;
        return Ok(true);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(MshError::Io)?;
    }
    fs::write(&path, codex_mcp_block(msh_bin)).map_err(MshError::Io)?;
    Ok(true)
}

fn codex_mcp_block(msh_bin: &Path) -> String {
    format!(
        r#"[mcp_servers.msh]
command = "{}"
args = ["--mcp"]

[mcp_servers.msh.env]
MSH_SKIP_RC = "1"
"#,
        msh_bin.display()
    )
}

fn json_escape(s: &str) -> String {
    crate::command_json::json_escape(s)
}

fn print_banner(language: Language) {
    println!("{}", msg(language, "msh セットアップ", "msh setup"));
    println!(
        "{}",
        msg(
            language,
            "日常シェル + エージェント向け設定を ~/.config/msh/config.toml に書き込みます。",
            "Configure daily shell + agent defaults in ~/.config/msh/config.toml."
        )
    );
    println!();
}

fn print_summary(
    language: Language,
    config_path: &Path,
    msh_bin: &Path,
    cursor: bool,
    claude: bool,
    codex: bool,
    strict: bool,
) {
    println!();
    println!("{}", msg(language, "完了", "Done"));
    println!("  config: {}", config_path.display());
    println!("  msh:    {}", msh_bin.display());
    if strict {
        println!(
            "  {}",
            msg(
                language,
                "安全: caution コマンドも block",
                "Safety: caution commands blocked"
            )
        );
    }
    if cursor {
        println!("  Cursor MCP: updated");
    }
    if claude {
        println!("  Claude MCP: updated");
    }
    if codex {
        println!("  Codex MCP:  updated");
    }
    println!();
    println!(
        "{}",
        msg(
            language,
            "次: msh doctor で検証 → msh で日常利用",
            "Next: run msh doctor, then msh for daily use"
        )
    );
    if cursor || claude {
        println!(
            "{}",
            msg(
                language,
                "IDE 連携後は Cursor Reload / claude 再起動",
                "Reload Cursor or restart claude after IDE integration"
            )
        );
    }
    println!(
        "{}",
        msg(
            language,
            "エージェント: docs/agent-integration.md",
            "Agents: see docs/agent-integration.md"
        )
    );
}

fn ask_strict_mode(lines: &mut impl BufRead, language: Language) -> Result<bool> {
    println!(
        "{}",
        msg(language, "エージェント安全モード:", "Agent safety mode:")
    );
    println!(
        "  1. {}",
        msg(
            language,
            "標準 (destructive のみ block)",
            "Standard (block destructive only)"
        )
    );
    println!(
        "  2. {}",
        msg(
            language,
            "厳格 (caution も block — rm 等)",
            "Strict (also block caution — rm, etc.)"
        )
    );
    print!("> ");
    io::stdout().flush().ok();
    let line = read_line(lines)?;
    Ok(line.trim() == "2")
}

fn ask_yes(
    lines: &mut impl BufRead,
    _language: Language,
    prompt: &str,
    default_yes: bool,
) -> Result<bool> {
    let hint = if default_yes { "[Y/n]" } else { "[y/N]" };
    print!("{prompt} {hint} ");
    io::stdout().flush().ok();
    let line = read_line(lines)?;
    let t = line.trim();
    if t.is_empty() {
        return Ok(default_yes);
    }
    Ok(matches!(
        t.to_ascii_lowercase().as_str(),
        "y" | "yes" | "はい"
    ))
}

fn read_line(lines: &mut impl BufRead) -> Result<String> {
    let mut buf = String::new();
    lines.read_line(&mut buf).map_err(MshError::Io)?;
    Ok(buf)
}

fn msg(language: Language, ja: &str, en: &str) -> String {
    match language {
        Language::Ja => ja.to_string(),
        Language::En => en.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inject_mcp_server_into_existing() {
        let input = r#"{"mcpServers": {"other": {}}}"#;
        let entry = r#""msh": {"command": "/usr/local/bin/msh"}"#;
        let out = inject_mcp_server(input, entry).unwrap();
        assert!(out.contains("\"msh\""));
        assert!(out.contains("other"));
    }

    #[test]
    fn inject_skips_duplicate() {
        let input = r#"{"mcpServers": {"msh": {}}}"#;
        assert!(inject_mcp_server(input, r#""msh": {}"#).is_none());
    }
}
