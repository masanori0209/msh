use crate::error::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// セッション復元状態。シリアライズは依存削減のため自前の行形式
/// （`cwd <path>` / `dir <path>`）を用いる。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionState {
    pub cwd: String,
    pub dir_stack: Vec<String>,
}

pub fn session_path(config_dir: &Path) -> PathBuf {
    config_dir.join("session.state")
}

pub fn legacy_session_path(config_dir: &Path) -> PathBuf {
    config_dir.join("session.json")
}

fn serialize(state: &SessionState) -> String {
    let mut out = String::new();
    if !state.cwd.is_empty() {
        out.push_str("cwd ");
        out.push_str(&state.cwd);
        out.push('\n');
    }
    for dir in &state.dir_stack {
        out.push_str("dir ");
        out.push_str(dir);
        out.push('\n');
    }
    out
}

fn deserialize(content: &str) -> SessionState {
    let mut state = SessionState::default();
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("cwd ") {
            state.cwd = value.to_string();
        } else if let Some(value) = line.strip_prefix("dir ") {
            state.dir_stack.push(value.to_string());
        }
    }
    state
}

/// 旧 `session.json`（serde 出力）を regex/serde なしで最小パースする。
fn parse_legacy_json(content: &str) -> Option<SessionState> {
    let cwd = extract_json_string_field(content, "cwd")?;
    let dir_stack = extract_json_string_array(content, "dir_stack");
    Some(SessionState { cwd, dir_stack })
}

fn extract_json_string_field(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let key_pos = json.find(&needle)?;
    let mut rest = json[key_pos + needle.len()..].trim_start();
    rest = rest.strip_prefix(':')?.trim_start();
    let (value, _) = parse_json_string(rest)?;
    Some(value)
}

fn extract_json_string_array(json: &str, key: &str) -> Vec<String> {
    let Some(values) = (|| {
        let needle = format!("\"{key}\"");
        let key_pos = json.find(&needle)?;
        let mut rest = json[key_pos + needle.len()..].trim_start();
        rest = rest.strip_prefix(':')?.trim_start();
        parse_json_string_array(rest)
    })() else {
        return Vec::new();
    };
    values
}

fn parse_json_string(input: &str) -> Option<(String, usize)> {
    let input = input.trim_start();
    if !input.starts_with('"') {
        return None;
    }
    let mut out = String::new();
    let mut escaped = false;
    let mut consumed = 1;
    for ch in input[1..].chars() {
        consumed += ch.len_utf8();
        if escaped {
            out.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                other => other,
            });
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some((out, consumed)),
            other => out.push(other),
        }
    }
    None
}

fn parse_json_string_array(input: &str) -> Option<Vec<String>> {
    let input = input.trim_start();
    if !input.starts_with('[') {
        return None;
    }
    let mut values = Vec::new();
    let mut rest = input[1..].trim_start();
    if rest.starts_with(']') {
        return Some(values);
    }
    loop {
        let (value, used) = parse_json_string(rest)?;
        values.push(value);
        rest = rest[used..].trim_start();
        if rest.starts_with(']') {
            return Some(values);
        }
        rest = rest.strip_prefix(',')?.trim_start();
    }
}

pub fn save(state: &SessionState, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serialize(state))?;
    Ok(())
}

pub fn load(path: &Path) -> Result<Option<SessionState>> {
    if path.is_file() {
        let content = fs::read_to_string(path)?;
        return Ok(Some(deserialize(&content)));
    }

    let legacy = legacy_session_path(path.parent().unwrap_or(path));
    if !legacy.is_file() {
        return Ok(None);
    }

    let content = fs::read_to_string(&legacy)?;
    let Some(state) = parse_legacy_json(&content) else {
        return Ok(None);
    };

    save(&state, path)?;
    let _ = fs::remove_file(&legacy);
    Ok(Some(state))
}

pub fn restore(state: &SessionState) -> Result<()> {
    let cwd = PathBuf::from(&state.cwd);
    if cwd.is_dir() {
        std::env::set_current_dir(cwd)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_session_state() {
        let state = SessionState {
            cwd: "/tmp/with space".into(),
            dir_stack: vec!["/a".into(), "/b/c".into()],
        };
        let parsed = deserialize(&serialize(&state));
        assert_eq!(parsed, state);
    }

    #[test]
    fn empty_content_is_default() {
        assert_eq!(deserialize(""), SessionState::default());
    }

    #[test]
    fn ignores_unknown_lines() {
        let parsed = deserialize("garbage\ncwd /home\ndir /x\n");
        assert_eq!(parsed.cwd, "/home");
        assert_eq!(parsed.dir_stack, vec!["/x".to_string()]);
    }

    #[test]
    fn parse_legacy_json_compact() {
        let json = r#"{"cwd":"/tmp/x","dir_stack":["/a","/b"]}"#;
        let state = parse_legacy_json(json).unwrap();
        assert_eq!(state.cwd, "/tmp/x");
        assert_eq!(state.dir_stack, vec!["/a", "/b"]);
    }

    #[test]
    fn parse_legacy_json_pretty() {
        let json = "{\n  \"cwd\": \"/home/user\",\n  \"dir_stack\": [\n    \"/var\",\n    \"/tmp\"\n  ]\n}\n";
        let state = parse_legacy_json(json).unwrap();
        assert_eq!(state.cwd, "/home/user");
        assert_eq!(state.dir_stack, vec!["/var", "/tmp"]);
    }

    #[test]
    fn load_migrates_legacy_json() {
        let dir = std::env::temp_dir().join(format!("msh-session-migrate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let legacy = dir.join("session.json");
        let state_path = dir.join("session.state");
        fs::write(&legacy, r#"{"cwd":"/tmp/migrate","dir_stack":["/stack"]}"#).unwrap();

        let loaded = load(&state_path).unwrap().unwrap();
        assert_eq!(loaded.cwd, "/tmp/migrate");
        assert_eq!(loaded.dir_stack, vec!["/stack"]);
        assert!(state_path.is_file());
        assert!(!legacy.exists());

        let again = load(&state_path).unwrap().unwrap();
        assert_eq!(again, loaded);

        let _ = fs::remove_dir_all(&dir);
    }
}
