use crate::builtins;
use crate::config::Language;
use crate::error::MshError;

pub fn format_error(err: &MshError, language: Language) -> String {
    match language {
        Language::Ja => format_error_ja(err),
        Language::En => format_error_en(err),
    }
}

fn format_error_en(err: &MshError) -> String {
    match err {
        MshError::CommandNotFound(command) => {
            let mut message = format!("command not found: {command}");
            if let Some(suggestion) = suggest_command(command) {
                message.push_str(&format!("\nDid you mean `{suggestion}`?"));
                message.push_str(&format!("\nExample: `{suggestion} --help`"));
            }
            message
        }
        MshError::DirNotFound(path) => {
            format!("cd: no such file or directory: {path}\nExample: cd ~")
        }
        MshError::InvalidExport(value) => {
            format!("export: invalid syntax: {value}\nExample: export PATH=\"/usr/bin:$PATH\"")
        }
        MshError::ParseError(message) => format!("parse error: {message}"),
        MshError::AliasError(message) => format!("alias: {message}"),
        MshError::UnsupportedSyntax {
            feature,
            workaround,
        } => {
            format!("unsupported syntax: {feature}\nworkaround: {workaround}")
        }
        MshError::ScriptError(message) => format!("script error: {message}"),
        MshError::Io(error) => format!("IO error: {error}"),
    }
}

fn format_error_ja(err: &MshError) -> String {
    match err {
        MshError::CommandNotFound(command) => {
            let mut message = format!("コマンドが見つかりません: {command}");
            if let Some(suggestion) = suggest_command(command) {
                message.push_str(&format!("\nもしかして `{suggestion}` ですか？"));
                message.push_str(&format!("\n例: `{suggestion} --help`"));
            }
            message
        }
        MshError::DirNotFound(path) => {
            format!("cd: ディレクトリが見つかりません: {path}\n例: cd ~")
        }
        MshError::InvalidExport(value) => {
            format!("export: 構文が不正です: {value}\n例: export PATH=\"/usr/bin:$PATH\"")
        }
        MshError::ParseError(message) => format!("解析エラー: {message}"),
        MshError::AliasError(message) => format!("alias エラー: {message}"),
        MshError::UnsupportedSyntax {
            feature,
            workaround,
        } => {
            format!("未対応の構文: {feature}\n回避策: {workaround}")
        }
        MshError::ScriptError(message) => format!("スクリプトエラー: {message}"),
        MshError::Io(error) => format!("IO エラー: {error}"),
    }
}

pub fn suggest_command(input: &str) -> Option<&'static str> {
    for (typo, suggestion) in TYPO_MAP {
        if input == *typo {
            return Some(suggestion);
        }
    }

    let mut best: Option<(&str, usize)> = None;

    for candidate in builtins::NAMES
        .iter()
        .copied()
        .chain(COMMON_COMMANDS.iter().copied())
    {
        if candidate == input {
            return None;
        }
        let distance = levenshtein(input, candidate);
        if distance > 0
            && distance <= 2
            && best.is_none_or(|(_, best_distance)| distance < best_distance)
        {
            best = Some((candidate, distance));
        }
    }

    best.map(|(name, _)| name)
}

pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];

    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            curr[j + 1] = if ca == cb {
                prev[j]
            } else {
                1 + prev[j].min(prev[j + 1]).min(curr[j])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b.len()]
}

const TYPO_MAP: &[(&str, &str)] = &[("sl", "ls"), ("claer", "clear")];

const COMMON_COMMANDS: &[&str] = &[
    "ls", "cat", "grep", "git", "cargo", "docker", "mkdir", "rm", "cp", "mv",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Language;

    #[test]
    fn suggest_ls_for_sl() {
        assert_eq!(suggest_command("sl"), Some("ls"));
    }

    #[test]
    fn japanese_command_not_found() {
        let message = format_error(&MshError::CommandNotFound("nosuch".into()), Language::Ja);
        assert!(message.contains("コマンドが見つかりません"));
    }
}
