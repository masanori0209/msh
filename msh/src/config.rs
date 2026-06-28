use crate::error::MshError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ShellConfig {
    pub compat: CompatMode,
    pub load_bashrc: bool,
    pub load_zshrc: bool,
    pub language: Language,
    pub theme: Theme,
    pub fuzzy_completion: bool,
    pub session_restore: bool,
    pub history_backend: HistoryBackend,
    pub ai: AiSettings,
}

/// AI 連携設定（`[ai]` セクション）。デフォルトは無効で、OFF 時は経路に一切入らない。
/// API キーは環境変数参照のみ（平文保存しない）。
#[derive(Debug, Clone)]
pub struct AiSettings {
    pub enabled: bool,
    pub provider: AiProvider,
    pub model: String,
    pub api_key_env: String,
    pub base_url: Option<String>,
    pub max_tokens: u32,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: AiProvider::Claude,
            model: "claude-3-5-haiku-latest".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            base_url: None,
            max_tokens: 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiProvider {
    #[default]
    Claude,
    OpenAi,
    Gemini,
    /// Ollama ネイティブ API（ローカル LLM・既定で認証不要）。
    Ollama,
}

impl AiProvider {
    /// API キーが無くても動作しうるか（ローカル LLM 等）。
    pub fn allows_keyless(self) -> bool {
        matches!(self, AiProvider::Ollama)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistoryBackend {
    #[default]
    Msh,
    Atuin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompatMode {
    #[default]
    Msh,
    Bash,
    Zsh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    En,
    Ja,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Default,
    Minimal,
}

impl ShellConfig {
    pub fn from_env_and_args() -> Self {
        let mut config = Self::default_from_file();
        if let Ok(mode) = std::env::var("MSH_COMPAT") {
            config.compat = parse_compat(&mode).unwrap_or(CompatMode::Msh);
        }
        if let Ok(lang) = std::env::var("MSH_LANG") {
            config.language = parse_language(&lang);
        }
        config
    }

    pub fn with_compat_flag(mut self, flag: Option<&str>) -> Self {
        if let Some(mode) = flag {
            self.compat = parse_compat(mode).unwrap_or(CompatMode::Msh);
        }
        self
    }

    fn default_from_file() -> Self {
        let mut config = Self {
            compat: CompatMode::Msh,
            load_bashrc: false,
            load_zshrc: false,
            language: Language::En,
            theme: Theme::Default,
            fuzzy_completion: true,
            session_restore: false,
            history_backend: HistoryBackend::Msh,
            ai: AiSettings::default(),
        };

        if let Some(path) = config_file_path() {
            if let Ok(content) = std::fs::read_to_string(path) {
                apply_toml(&mut config, &content);
            }
        }

        match config.compat {
            CompatMode::Bash => config.load_bashrc = true,
            CompatMode::Zsh => config.load_zshrc = true,
            CompatMode::Msh => {}
        }

        config
    }

    pub fn config_dir() -> Option<PathBuf> {
        std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join(".config").join("msh"))
    }
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self::default_from_file()
    }
}

fn config_file_path() -> Option<PathBuf> {
    ShellConfig::config_dir()
        .map(|dir| dir.join("config.toml"))
        .filter(|path| path.is_file())
}

fn parse_compat(value: &str) -> Option<CompatMode> {
    match value.to_ascii_lowercase().as_str() {
        "bash" => Some(CompatMode::Bash),
        "zsh" => Some(CompatMode::Zsh),
        "msh" => Some(CompatMode::Msh),
        _ => None,
    }
}

fn parse_language(value: &str) -> Language {
    match value.to_ascii_lowercase().as_str() {
        "ja" | "jp" | "japanese" => Language::Ja,
        _ => Language::En,
    }
}

fn parse_theme(value: &str) -> Theme {
    match value.to_ascii_lowercase().as_str() {
        "minimal" => Theme::Minimal,
        _ => Theme::Default,
    }
}

fn parse_history_backend(value: &str) -> HistoryBackend {
    match value.to_ascii_lowercase().as_str() {
        "atuin" => HistoryBackend::Atuin,
        _ => HistoryBackend::Msh,
    }
}

fn apply_toml(config: &mut ShellConfig, content: &str) {
    let mut section = String::new();
    for line in content.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim().to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = trim_quotes(value.trim());

        if section == "ai" {
            apply_ai_key(&mut config.ai, key, value);
            continue;
        }

        match key {
            "compat" => {
                if let Some(mode) = parse_compat(value) {
                    config.compat = mode;
                }
            }
            "load_bashrc" => config.load_bashrc = value == "true",
            "load_zshrc" => config.load_zshrc = value == "true",
            "language" | "lang" => config.language = parse_language(value),
            "theme" => config.theme = parse_theme(value),
            "fuzzy_completion" => config.fuzzy_completion = value == "true",
            "session_restore" => config.session_restore = value == "true",
            "history_backend" => config.history_backend = parse_history_backend(value),
            _ => {}
        }
    }
}

fn apply_ai_key(ai: &mut AiSettings, key: &str, value: &str) {
    match key {
        "enabled" => ai.enabled = value == "true",
        "provider" => {
            if let Some(provider) = parse_ai_provider(value) {
                ai.provider = provider;
            }
        }
        "model" => ai.model = value.to_string(),
        "api_key_env" => ai.api_key_env = value.to_string(),
        "base_url" => ai.base_url = (!value.is_empty()).then(|| value.to_string()),
        "max_tokens" => {
            if let Ok(n) = value.parse::<u32>() {
                ai.max_tokens = n;
            }
        }
        _ => {}
    }
}

fn parse_ai_provider(value: &str) -> Option<AiProvider> {
    match value.to_ascii_lowercase().as_str() {
        "claude" | "anthropic" => Some(AiProvider::Claude),
        // OpenAI 互換は base_url 上書きで LM Studio / llama.cpp / vLLM / groq /
        // openrouter / together などローカル・他社エンドポイントを全てカバーする。
        "openai" | "gpt" | "openai-compatible" | "compat" | "local" => Some(AiProvider::OpenAi),
        "gemini" | "google" => Some(AiProvider::Gemini),
        // Gemma は Gemini 系の軽量オープンモデル。標準のローカル実行系は Ollama なので
        // "gemma" は Ollama 経路のエイリアスとして扱う（model に gemma3 等を指定）。
        "ollama" | "gemma" => Some(AiProvider::Ollama),
        _ => None,
    }
}

fn trim_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(value)
}

pub fn detect_unsupported(line: &str) -> Option<MshError> {
    let trimmed = line.trim();
    if trimmed.contains("<(") || trimmed.contains(">(") {
        return Some(MshError::UnsupportedSyntax {
            feature: "process substitution".into(),
            workaround: "use named pipes or temporary files".into(),
        });
    }
    None
}

pub fn default_config_template() -> &'static str {
    r#"# msh configuration
# compat = "msh"          # msh | bash | zsh
# language = "ja"         # en | ja
# theme = "default"       # default | minimal
# fuzzy_completion = true
# session_restore = false
# history_backend = "msh"     # msh | atuin
# load_bashrc = false
# load_zshrc = false

# [ai]                              # AI 連携（デフォルト無効・OFF 時は一切呼ばれない）
# enabled = false
# provider = "claude"              # claude | openai | gemini | ollama
# model = "claude-3-5-haiku-latest"
# api_key_env = "ANTHROPIC_API_KEY" # API キーは環境変数参照のみ（平文保存しない）
# max_tokens = 1024
# base_url = ""                    # 任意: 互換エンドポイント上書き
#
# --- ローカル LLM（Ollama・認証不要）の例 ---
# provider = "ollama"
# model = "llama3.1"
# api_key_env = ""                 # 空ならキー不要
# base_url = "http://localhost:11434"
#
# --- Gemma（Gemini 系の省メモリ・ローカル / 要: ollama pull gemma3:1b）---
# provider = "gemma"              # = ollama 経路のエイリアス
# model = "gemma3:1b"             # 省メモリ重視。精度優先なら gemma3 / gemma3:4b
# api_key_env = ""
# base_url = "http://localhost:11434"
#
# --- OpenAI 互換サーバ（LM Studio / llama.cpp / vLLM / groq / openrouter 等）---
# provider = "openai"             # 互換 API は openai プロバイダ + base_url で対応
# model = "your-model"
# api_key_env = "LOCAL_API_KEY"   # 不要なら "" 、必要なら任意の env 名
# base_url = "http://localhost:1234/v1"
"#
}

pub fn describe_config(config: &ShellConfig) -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert(
        "compat".into(),
        match config.compat {
            CompatMode::Msh => "msh".into(),
            CompatMode::Bash => "bash".into(),
            CompatMode::Zsh => "zsh".into(),
        },
    );
    map.insert(
        "language".into(),
        match config.language {
            Language::En => "en".into(),
            Language::Ja => "ja".into(),
        },
    );
    map.insert("load_bashrc".into(), config.load_bashrc.to_string());
    map.insert("load_zshrc".into(), config.load_zshrc.to_string());
    map.insert("session_restore".into(), config.session_restore.to_string());
    map.insert(
        "history_backend".into(),
        match config.history_backend {
            HistoryBackend::Msh => "msh".into(),
            HistoryBackend::Atuin => "atuin".into(),
        },
    );
    map.insert("ai.enabled".into(), config.ai.enabled.to_string());
    map.insert(
        "ai.provider".into(),
        match config.ai.provider {
            AiProvider::Claude => "claude".into(),
            AiProvider::OpenAi => "openai".into(),
            AiProvider::Gemini => "gemini".into(),
            AiProvider::Ollama => "ollama".into(),
        },
    );
    map.insert("ai.model".into(), config.ai.model.clone());
    map
}

pub fn plugin_paths(home: &Path) -> Vec<PathBuf> {
    let plugins_dir = home.join(".config").join("msh").join("plugins");
    let Ok(entries) = std::fs::read_dir(plugins_dir) else {
        return Vec::new();
    };

    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "msh"))
        .collect();
    paths.sort();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_compat_values() {
        assert_eq!(parse_compat("bash"), Some(CompatMode::Bash));
        assert_eq!(parse_compat("zsh"), Some(CompatMode::Zsh));
    }

    #[test]
    fn apply_toml_settings() {
        let mut config = ShellConfig {
            compat: CompatMode::Msh,
            load_bashrc: false,
            load_zshrc: false,
            language: Language::En,
            theme: Theme::Default,
            fuzzy_completion: true,
            session_restore: false,
            history_backend: HistoryBackend::Msh,
            ai: AiSettings::default(),
        };
        apply_toml(
            &mut config,
            "compat = \"bash\"\nlanguage = \"ja\"\ntheme = \"minimal\"\nfuzzy_completion = false",
        );
        assert_eq!(config.compat, CompatMode::Bash);
        assert_eq!(config.language, Language::Ja);
        assert_eq!(config.theme, Theme::Minimal);
        assert!(!config.fuzzy_completion);
    }

    #[test]
    fn apply_toml_ai_section() {
        let mut config = ShellConfig::default();
        apply_toml(
            &mut config,
            "[ai]\nenabled = true\nprovider = \"openai\"\nmodel = \"gpt-4o-mini\"\nmax_tokens = 256",
        );
        assert!(config.ai.enabled);
        assert_eq!(config.ai.provider, AiProvider::OpenAi);
        assert_eq!(config.ai.model, "gpt-4o-mini");
        assert_eq!(config.ai.max_tokens, 256);
    }

    #[test]
    fn ai_section_does_not_leak_to_toplevel() {
        // [ai] 内の model 等が top-level に誤適用されないこと。
        let mut config = ShellConfig::default();
        apply_toml(&mut config, "language = \"ja\"\n[ai]\nmodel = \"x\"");
        assert_eq!(config.language, Language::Ja);
        assert_eq!(config.ai.model, "x");
    }

    #[test]
    fn gemma_provider_is_ollama_alias_and_keyless() {
        let mut config = ShellConfig::default();
        apply_toml(
            &mut config,
            "[ai]\nprovider = \"gemma\"\nmodel = \"gemma3:1b\"",
        );
        assert_eq!(config.ai.provider, AiProvider::Ollama);
        assert!(config.ai.provider.allows_keyless());
        assert_eq!(config.ai.model, "gemma3:1b");
    }
}
