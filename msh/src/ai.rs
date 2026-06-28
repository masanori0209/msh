//! AI 連携の基盤（A-1）。
//!
//! 設計方針:
//! - **依存を増やさない**: HTTP は `curl` サブプロセスに委譲（reqwest/tokio を持ち込まない）。
//! - **秘密を漏らさない**: API キー・リクエストボディは argv に出さず、curl 設定ファイル
//!   （`-K`、パーミッション 0600）経由で渡し、ボディは stdin (`data = "@-"`) で流す。
//! - **安全**: このモジュールはテキストを返すだけで、コマンドを実行しない。
//! - **オプトイン**: `ai.enabled = false`（デフォルト）の間は呼び出し側が経路に入れない。

use crate::config::{AiProvider, AiSettings};
use crate::error::{MshError, Result};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub struct AiClient<'a> {
    settings: &'a AiSettings,
}

struct AiRequest {
    url: String,
    headers: Vec<String>,
    body: String,
}

impl<'a> AiClient<'a> {
    pub fn new(settings: &'a AiSettings) -> Self {
        Self { settings }
    }

    /// system / user プロンプトを送り、モデルの応答テキストを返す。
    pub fn complete(&self, system: &str, user: &str) -> Result<String> {
        if !self.settings.enabled {
            return Err(MshError::ScriptError(
                "ai: 無効です。config.toml の [ai] enabled = true で有効化してください".into(),
            ));
        }
        let key = self.resolve_key()?;
        let request = build_request(self.settings, key.as_deref(), system, user);
        let raw = run_curl(&request)?;
        extract_text(self.settings.provider, &raw)
    }

    /// API キーを解決する。`api_key_env` が空、または keyless 可能なプロバイダで
    /// 環境変数が未設定の場合は None（認証なし）を許容する。
    fn resolve_key(&self) -> Result<Option<String>> {
        if self.settings.api_key_env.is_empty() {
            return Ok(None);
        }
        match std::env::var(&self.settings.api_key_env) {
            Ok(key) => Ok(Some(key)),
            Err(_) if self.settings.provider.allows_keyless() => Ok(None),
            Err(_) => Err(MshError::ScriptError(format!(
                "ai: API キーが環境変数 {} に見つかりません",
                self.settings.api_key_env
            ))),
        }
    }
}

/// プロバイダ別に URL・ヘッダ・JSON ボディを組み立てる（純関数・テスト対象）。
/// `key` が None の場合は認証ヘッダ/クエリを付けない（ローカル LLM 等）。
fn build_request(settings: &AiSettings, key: Option<&str>, system: &str, user: &str) -> AiRequest {
    match settings.provider {
        AiProvider::Claude => {
            let url = settings
                .base_url
                .clone()
                .unwrap_or_else(|| "https://api.anthropic.com/v1/messages".into());
            let body = format!(
                "{{\"model\":\"{model}\",\"max_tokens\":{max},\"system\":\"{sys}\",\"messages\":[{{\"role\":\"user\",\"content\":\"{usr}\"}}]}}",
                model = escape_json(&settings.model),
                max = settings.max_tokens,
                sys = escape_json(system),
                usr = escape_json(user),
            );
            let mut headers = vec![
                "anthropic-version: 2023-06-01".into(),
                "content-type: application/json".into(),
            ];
            if let Some(key) = key {
                headers.push(format!("x-api-key: {key}"));
            }
            AiRequest { url, headers, body }
        }
        AiProvider::OpenAi => {
            let url = settings
                .base_url
                .clone()
                .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".into());
            let body = format!(
                "{{\"model\":\"{model}\",\"max_tokens\":{max},\"messages\":[{{\"role\":\"system\",\"content\":\"{sys}\"}},{{\"role\":\"user\",\"content\":\"{usr}\"}}]}}",
                model = escape_json(&settings.model),
                max = settings.max_tokens,
                sys = escape_json(system),
                usr = escape_json(user),
            );
            let mut headers = vec!["content-type: application/json".to_string()];
            if let Some(key) = key {
                headers.push(format!("authorization: Bearer {key}"));
            }
            AiRequest { url, headers, body }
        }
        AiProvider::Gemini => {
            let base = settings.base_url.clone().unwrap_or_else(|| {
                format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
                    settings.model
                )
            });
            let url = match key {
                Some(key) => format!("{base}?key={key}"),
                None => base,
            };
            let body = format!(
                "{{\"systemInstruction\":{{\"parts\":[{{\"text\":\"{sys}\"}}]}},\"contents\":[{{\"role\":\"user\",\"parts\":[{{\"text\":\"{usr}\"}}]}}]}}",
                sys = escape_json(system),
                usr = escape_json(user),
            );
            AiRequest {
                url,
                headers: vec!["content-type: application/json".into()],
                body,
            }
        }
        AiProvider::Ollama => {
            let base = settings
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434".into());
            let url = format!("{}/api/chat", base.trim_end_matches('/'));
            let body = format!(
                "{{\"model\":\"{model}\",\"stream\":false,\"messages\":[{{\"role\":\"system\",\"content\":\"{sys}\"}},{{\"role\":\"user\",\"content\":\"{usr}\"}}]}}",
                model = escape_json(&settings.model),
                sys = escape_json(system),
                usr = escape_json(user),
            );
            let mut headers = vec!["content-type: application/json".to_string()];
            if let Some(key) = key {
                headers.push(format!("authorization: Bearer {key}"));
            }
            AiRequest { url, headers, body }
        }
    }
}

/// curl 設定ファイル経由でリクエストを実行する（キー・ボディを argv に出さない）。
fn run_curl(request: &AiRequest) -> Result<String> {
    let config = render_curl_config(request);
    let config_path = write_temp_config(&config)?;

    let result = (|| {
        let mut child = Command::new("curl")
            .arg("-sS")
            .arg("-K")
            .arg(&config_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| MshError::ScriptError(format!("ai: curl 起動に失敗: {e}")))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(request.body.as_bytes())
                .map_err(|e| MshError::ScriptError(format!("ai: リクエスト送信に失敗: {e}")))?;
        }

        let output = child
            .wait_with_output()
            .map_err(|e| MshError::ScriptError(format!("ai: curl 実行に失敗: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MshError::ScriptError(format!(
                "ai: curl がエラー終了しました: {}",
                stderr.trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    })();

    let _ = std::fs::remove_file(&config_path);
    result
}

fn render_curl_config(request: &AiRequest) -> String {
    let mut config = String::new();
    config.push_str(&format!("url = \"{}\"\n", escape_curl_conf(&request.url)));
    config.push_str("request = \"POST\"\n");
    for header in &request.headers {
        config.push_str(&format!("header = \"{}\"\n", escape_curl_conf(header)));
    }
    // ボディは stdin から読み込む（argv・設定ファイルに平文で残さない）。
    config.push_str("data = \"@-\"\n");
    config
}

fn write_temp_config(contents: &str) -> Result<PathBuf> {
    let mut path = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    path.push(format!("msh-ai-{}-{}.curl", std::process::id(), nanos));

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| MshError::ScriptError(format!("ai: 一時ファイル作成に失敗: {e}")))?;
    file.write_all(contents.as_bytes())
        .map_err(|e| MshError::ScriptError(format!("ai: 一時ファイル書き込みに失敗: {e}")))?;
    Ok(path)
}

/// プロバイダ別レスポンスからテキストを抽出する（純関数・テスト対象）。
fn extract_text(provider: AiProvider, raw: &str) -> Result<String> {
    let json = Json::parse(raw)
        .map_err(|e| MshError::ScriptError(format!("ai: 応答の解析に失敗: {e}")))?;

    // 共通: API エラーフィールドを優先的に拾う。
    if let Some(message) = json
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(Json::as_str)
    {
        return Err(MshError::ScriptError(format!("ai: API エラー: {message}")));
    }

    let text = match provider {
        AiProvider::Claude => json
            .get("content")
            .and_then(|c| c.index(0))
            .and_then(|c| c.get("text"))
            .and_then(Json::as_str),
        AiProvider::OpenAi => json
            .get("choices")
            .and_then(|c| c.index(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(Json::as_str),
        AiProvider::Gemini => json
            .get("candidates")
            .and_then(|c| c.index(0))
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.index(0))
            .and_then(|p| p.get("text"))
            .and_then(Json::as_str),
        AiProvider::Ollama => json
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(Json::as_str),
    };

    text.map(|s| s.trim().to_string()).ok_or_else(|| {
        let snippet: String = raw.chars().take(200).collect();
        MshError::ScriptError(format!(
            "ai: 応答からテキストを取得できませんでした: {snippet}"
        ))
    })
}

fn escape_json(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    for ch in input.chars() {
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

fn escape_curl_conf(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

// --- 依存なしの最小 JSON パーサ（応答の抽出用） ---

#[derive(Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn parse(input: &str) -> std::result::Result<Json, String> {
        let chars: Vec<char> = input.chars().collect();
        let mut pos = 0;
        skip_ws(&chars, &mut pos);
        let value = parse_value(&chars, &mut pos)?;
        skip_ws(&chars, &mut pos);
        Ok(value)
    }

    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn index(&self, i: usize) -> Option<&Json> {
        match self {
            Json::Arr(items) => items.get(i),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
}

fn skip_ws(chars: &[char], pos: &mut usize) {
    while let Some(&c) = chars.get(*pos) {
        if c.is_whitespace() {
            *pos += 1;
        } else {
            break;
        }
    }
}

fn parse_value(chars: &[char], pos: &mut usize) -> std::result::Result<Json, String> {
    skip_ws(chars, pos);
    match chars.get(*pos) {
        Some('{') => parse_object(chars, pos),
        Some('[') => parse_array(chars, pos),
        Some('"') => parse_string(chars, pos).map(Json::Str),
        Some('t') | Some('f') => parse_bool(chars, pos),
        Some('n') => parse_null(chars, pos),
        Some(c) if *c == '-' || c.is_ascii_digit() => parse_number(chars, pos),
        other => Err(format!("unexpected token: {other:?}")),
    }
}

fn parse_object(chars: &[char], pos: &mut usize) -> std::result::Result<Json, String> {
    *pos += 1; // consume '{'
    let mut entries = Vec::new();
    skip_ws(chars, pos);
    if chars.get(*pos) == Some(&'}') {
        *pos += 1;
        return Ok(Json::Obj(entries));
    }
    loop {
        skip_ws(chars, pos);
        let key = parse_string(chars, pos)?;
        skip_ws(chars, pos);
        if chars.get(*pos) != Some(&':') {
            return Err("expected ':' in object".into());
        }
        *pos += 1;
        let value = parse_value(chars, pos)?;
        entries.push((key, value));
        skip_ws(chars, pos);
        match chars.get(*pos) {
            Some(',') => {
                *pos += 1;
            }
            Some('}') => {
                *pos += 1;
                break;
            }
            other => return Err(format!("expected ',' or '}}' in object, got {other:?}")),
        }
    }
    Ok(Json::Obj(entries))
}

fn parse_array(chars: &[char], pos: &mut usize) -> std::result::Result<Json, String> {
    *pos += 1; // consume '['
    let mut items = Vec::new();
    skip_ws(chars, pos);
    if chars.get(*pos) == Some(&']') {
        *pos += 1;
        return Ok(Json::Arr(items));
    }
    loop {
        let value = parse_value(chars, pos)?;
        items.push(value);
        skip_ws(chars, pos);
        match chars.get(*pos) {
            Some(',') => {
                *pos += 1;
            }
            Some(']') => {
                *pos += 1;
                break;
            }
            other => return Err(format!("expected ',' or ']' in array, got {other:?}")),
        }
    }
    Ok(Json::Arr(items))
}

fn parse_string(chars: &[char], pos: &mut usize) -> std::result::Result<String, String> {
    if chars.get(*pos) != Some(&'"') {
        return Err("expected string".into());
    }
    *pos += 1;
    let mut out = String::new();
    while let Some(&c) = chars.get(*pos) {
        *pos += 1;
        match c {
            '"' => return Ok(out),
            '\\' => {
                let esc = chars.get(*pos).copied().ok_or("unterminated escape")?;
                *pos += 1;
                match esc {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000C}'),
                    'u' => {
                        let hex: String = chars.get(*pos..*pos + 4).unwrap_or(&[]).iter().collect();
                        *pos += 4;
                        let code =
                            u32::from_str_radix(&hex, 16).map_err(|_| "invalid \\u escape")?;
                        out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                    }
                    other => out.push(other),
                }
            }
            c => out.push(c),
        }
    }
    Err("unterminated string".into())
}

fn parse_bool(chars: &[char], pos: &mut usize) -> std::result::Result<Json, String> {
    let rest: String = chars.get(*pos..).unwrap_or(&[]).iter().collect();
    if rest.starts_with("true") {
        *pos += 4;
        Ok(Json::Bool(true))
    } else if rest.starts_with("false") {
        *pos += 5;
        Ok(Json::Bool(false))
    } else {
        Err("invalid boolean".into())
    }
}

fn parse_null(chars: &[char], pos: &mut usize) -> std::result::Result<Json, String> {
    let rest: String = chars.get(*pos..).unwrap_or(&[]).iter().collect();
    if rest.starts_with("null") {
        *pos += 4;
        Ok(Json::Null)
    } else {
        Err("invalid null".into())
    }
}

fn parse_number(chars: &[char], pos: &mut usize) -> std::result::Result<Json, String> {
    let start = *pos;
    while let Some(&c) = chars.get(*pos) {
        if c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E') {
            *pos += 1;
        } else {
            break;
        }
    }
    let literal: String = chars[start..*pos].iter().collect();
    literal
        .parse::<f64>()
        .map(Json::Num)
        .map_err(|_| format!("invalid number: {literal}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(provider: AiProvider) -> AiSettings {
        AiSettings {
            enabled: true,
            provider,
            model: "test-model".into(),
            api_key_env: "X".into(),
            base_url: None,
            max_tokens: 100,
        }
    }

    #[test]
    fn json_parses_nested() {
        let json = Json::parse(r#"{"a":[{"b":"c"},1,true,null]}"#).unwrap();
        assert_eq!(
            json.get("a")
                .and_then(|a| a.index(0))
                .and_then(|o| o.get("b"))
                .and_then(Json::as_str),
            Some("c")
        );
    }

    #[test]
    fn json_handles_escapes() {
        let json = Json::parse(r#"{"t":"a\nb\t\"q\""}"#).unwrap();
        assert_eq!(json.get("t").and_then(Json::as_str), Some("a\nb\t\"q\""));
    }

    #[test]
    fn escape_json_escapes_quotes_and_newlines() {
        assert_eq!(escape_json("a\"b\nc"), "a\\\"b\\nc");
    }

    #[test]
    fn build_request_claude_shape() {
        let s = settings(AiProvider::Claude);
        let req = build_request(&s, Some("secret"), "sys", "hi");
        assert!(req.url.contains("anthropic.com"));
        assert!(req.headers.iter().any(|h| h == "x-api-key: secret"));
        assert!(req.body.contains("\"role\":\"user\""));
        assert!(req.body.contains("\"max_tokens\":100"));
    }

    #[test]
    fn build_request_gemini_puts_key_in_url() {
        let s = settings(AiProvider::Gemini);
        let req = build_request(&s, Some("k123"), "sys", "hi");
        assert!(req.url.contains("key=k123"));
        assert!(req.url.contains("test-model:generateContent"));
    }

    #[test]
    fn extract_text_per_provider() {
        let claude = r#"{"content":[{"type":"text","text":" hello "}]}"#;
        assert_eq!(extract_text(AiProvider::Claude, claude).unwrap(), "hello");

        let openai = r#"{"choices":[{"message":{"role":"assistant","content":"hi there"}}]}"#;
        assert_eq!(
            extract_text(AiProvider::OpenAi, openai).unwrap(),
            "hi there"
        );

        let gemini = r#"{"candidates":[{"content":{"parts":[{"text":"yo"}]}}]}"#;
        assert_eq!(extract_text(AiProvider::Gemini, gemini).unwrap(), "yo");

        let ollama = r#"{"message":{"role":"assistant","content":"hey"}}"#;
        assert_eq!(extract_text(AiProvider::Ollama, ollama).unwrap(), "hey");
    }

    #[test]
    fn build_request_ollama_is_keyless_and_local() {
        let mut s = settings(AiProvider::Ollama);
        s.base_url = Some("http://localhost:11434".into());
        let req = build_request(&s, None, "sys", "hi");
        assert_eq!(req.url, "http://localhost:11434/api/chat");
        assert!(req.body.contains("\"stream\":false"));
        assert!(!req.headers.iter().any(|h| h.starts_with("authorization")));
    }

    #[test]
    fn build_request_openai_omits_auth_when_keyless() {
        let s = settings(AiProvider::OpenAi);
        let req = build_request(&s, None, "sys", "hi");
        assert!(!req.headers.iter().any(|h| h.starts_with("authorization")));
    }

    #[test]
    fn extract_text_surfaces_api_error() {
        let err = r#"{"error":{"message":"invalid api key"}}"#;
        let result = extract_text(AiProvider::Claude, err);
        assert!(result.is_err());
    }

    #[test]
    fn render_curl_config_uses_stdin_body() {
        let req = build_request(&settings(AiProvider::Claude), Some("k"), "s", "u");
        let conf = render_curl_config(&req);
        assert!(conf.contains("data = \"@-\""));
        assert!(conf.contains("request = \"POST\""));
    }
}
