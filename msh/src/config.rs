use crate::error::MshError;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ShellConfig {
    pub compat: CompatMode,
    pub load_bashrc: bool,
    pub load_zshrc: bool,
    pub language: Language,
    pub theme: Theme,
    pub prompt: PromptSettings,
    pub fuzzy_completion: bool,
    pub session_restore: bool,
    pub history_backend: HistoryBackend,
    pub ai: AiSettings,
    pub agent: AgentSettings,
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

/// エージェント / `--json` / `--agent` / `--mcp` 向け設定（`[agent]` セクション）。
#[derive(Debug, Clone)]
pub struct AgentSettings {
    pub json_max_bytes: usize,
    pub timeout_ms: u64,
    pub include_meta: bool,
    pub block_caution: bool,
    pub sandbox_root: Option<String>,
    pub allowlist: Vec<String>,
    pub audit_log: Option<String>,
    pub session_path: Option<String>,
    pub rc_mode: AgentRcMode,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            json_max_bytes: crate::command_json::DEFAULT_JSON_MAX_BYTES,
            timeout_ms: 0,
            include_meta: true,
            block_caution: false,
            sandbox_root: None,
            allowlist: Vec::new(),
            audit_log: None,
            session_path: None,
            rc_mode: AgentRcMode::Skip,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentRcMode {
    #[default]
    Skip,
    Minimal,
    Full,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptStyle {
    #[default]
    Default,
    Minimal,
    Powerline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptPreset {
    #[default]
    Msh,
    Classic,
    None,
    Pure,
    Rainbow,
    Nord,
    HighContrast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptSeparator {
    #[default]
    Chevron,
    Arrow,
    Round,
    Bar,
    Space,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeFormat {
    #[default]
    Hm24,
    Hm12,
    Hms24,
}

/// プロンプトセグメント（[prompt] セクション）。セグメント単位で config 制御。
#[derive(Debug, Clone)]
pub struct PromptSettings {
    pub style: PromptStyle,
    pub preset: PromptPreset,
    pub icons: bool,
    pub bold: bool,
    pub separator: PromptSeparator,
    pub newline: bool,
    pub show_shell: bool,
    pub show_user_host: bool,
    pub show_k8s: bool,
    pub k8s_show_namespace: bool,
    pub show_git: bool,
    pub show_git_dirty: bool,
    pub show_git_ahead_behind: bool,
    pub show_time: bool,
    pub time_format: TimeFormat,
    pub show_battery: bool,
    pub battery_hide_full: bool,
    pub show_duration: bool,
    pub transient_duration: bool,
    pub duration_min_ms: u64,
    pub show_exit_on_success: bool,
    pub colors: crate::prompt_color::PromptColors,
}

impl Default for PromptSettings {
    fn default() -> Self {
        Self {
            style: PromptStyle::Default,
            preset: PromptPreset::Msh,
            icons: true,
            bold: false,
            separator: PromptSeparator::Bar,
            newline: false,
            show_shell: false,
            show_user_host: false,
            show_k8s: false,
            k8s_show_namespace: true,
            show_git: true,
            show_git_dirty: true,
            show_git_ahead_behind: true,
            show_time: false,
            time_format: TimeFormat::Hm24,
            show_battery: false,
            battery_hide_full: true,
            show_duration: true,
            transient_duration: true,
            duration_min_ms: 50,
            show_exit_on_success: false,
            colors: crate::prompt_color::PromptColors::default(),
        }
    }
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
        apply_agent_env(&mut config);
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
            prompt: PromptSettings::default(),
            fuzzy_completion: true,
            session_restore: false,
            history_backend: HistoryBackend::Msh,
            ai: AiSettings::default(),
            agent: AgentSettings::default(),
        };

        let mut language_explicit = false;
        if let Some(path) = config_file_path() {
            if let Ok(content) = std::fs::read_to_string(path) {
                language_explicit = apply_toml(&mut config, &content);
            }
        }
        if !language_explicit {
            config.language = infer_language_from_locale();
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

/// Locale 文字列（`ja_JP.UTF-8` 等）から msh UI 言語を推論する。
fn language_from_locale(value: &str) -> Language {
    let base = value
        .split('.')
        .next()
        .unwrap_or(value)
        .split('@')
        .next()
        .unwrap_or(value);
    let lang = base
        .split('_')
        .next()
        .unwrap_or(base)
        .split('-')
        .next()
        .unwrap_or(base);
    match lang.to_ascii_lowercase().as_str() {
        "ja" => Language::Ja,
        _ => Language::En,
    }
}

/// `$LC_ALL` → `$LC_MESSAGES` → `$LANG` の順で msh UI 言語を推論する。
fn infer_language_from_locale() -> Language {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var) {
            let trimmed = value.trim();
            if trimmed.is_empty() || trimmed == "C" || trimmed == "POSIX" {
                continue;
            }
            return language_from_locale(trimmed);
        }
    }
    Language::En
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

fn apply_toml(config: &mut ShellConfig, content: &str) -> bool {
    let mut language_explicit = false;
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
        if section == "agent" {
            apply_agent_key(&mut config.agent, key, value);
            continue;
        }
        if section == "prompt" {
            apply_prompt_key(&mut config.prompt, key, value);
            continue;
        }
        if section.starts_with("prompt.colors.") {
            crate::prompt_color::apply_color_key(&mut config.prompt.colors, &section, key, value);
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
            "language" | "lang" => {
                config.language = parse_language(value);
                language_explicit = true;
            }
            "theme" => {
                config.theme = parse_theme(value);
                sync_prompt_style_from_theme(config);
            }
            "fuzzy_completion" => config.fuzzy_completion = value == "true",
            "session_restore" => config.session_restore = value == "true",
            "history_backend" => config.history_backend = parse_history_backend(value),
            _ => {}
        }
    }
    language_explicit
}

fn parse_prompt_style(value: &str) -> PromptStyle {
    match value.to_ascii_lowercase().as_str() {
        "minimal" => PromptStyle::Minimal,
        "powerline" => PromptStyle::Powerline,
        _ => PromptStyle::Default,
    }
}

fn sync_prompt_style_from_theme(config: &mut ShellConfig) {
    if matches!(config.theme, Theme::Minimal) && config.prompt.style == PromptStyle::Default {
        config.prompt.style = PromptStyle::Minimal;
    }
}

fn apply_prompt_key(prompt: &mut PromptSettings, key: &str, value: &str) {
    match key {
        "style" => prompt.style = parse_prompt_style(value),
        "preset" => prompt.preset = parse_prompt_preset(value),
        "icons" => prompt.icons = value == "true",
        "bold" => prompt.bold = value == "true",
        "separator" => prompt.separator = parse_prompt_separator(value),
        "newline" => prompt.newline = value == "true",
        "show_shell" => prompt.show_shell = value == "true",
        "show_user_host" => prompt.show_user_host = value == "true",
        "show_k8s" => prompt.show_k8s = value == "true",
        "k8s_show_namespace" => prompt.k8s_show_namespace = value == "true",
        "show_git" => prompt.show_git = value == "true",
        "show_git_dirty" => prompt.show_git_dirty = value == "true",
        "show_git_ahead_behind" => prompt.show_git_ahead_behind = value == "true",
        "show_time" => prompt.show_time = value == "true",
        "time_format" => prompt.time_format = parse_time_format(value),
        "show_battery" => prompt.show_battery = value == "true",
        "battery_hide_full" => prompt.battery_hide_full = value == "true",
        "show_duration" => prompt.show_duration = value == "true",
        "transient_duration" => prompt.transient_duration = value == "true",
        "duration_min_ms" => {
            if let Ok(ms) = value.parse::<u64>() {
                prompt.duration_min_ms = ms;
            }
        }
        "show_exit_on_success" => prompt.show_exit_on_success = value == "true",
        _ => {}
    }
}

fn parse_prompt_preset(value: &str) -> PromptPreset {
    match value.to_ascii_lowercase().as_str() {
        "none" | "off" => PromptPreset::None,
        "pure" => PromptPreset::Pure,
        "rainbow" => PromptPreset::Rainbow,
        "nord" => PromptPreset::Nord,
        "high-contrast" | "high_contrast" | "contrast" => PromptPreset::HighContrast,
        "classic" => PromptPreset::Classic,
        "msh" => PromptPreset::Msh,
        _ => PromptPreset::Msh,
    }
}

fn parse_prompt_separator(value: &str) -> PromptSeparator {
    match value.to_ascii_lowercase().as_str() {
        "arrow" => PromptSeparator::Arrow,
        "round" => PromptSeparator::Round,
        "bar" => PromptSeparator::Bar,
        "space" => PromptSeparator::Space,
        _ => PromptSeparator::Chevron,
    }
}

fn parse_time_format(value: &str) -> TimeFormat {
    match value.to_ascii_lowercase().as_str() {
        "12h" | "12" => TimeFormat::Hm12,
        "24h-s" | "24h_s" | "hms" => TimeFormat::Hms24,
        _ => TimeFormat::Hm24,
    }
}

fn apply_agent_key(agent: &mut AgentSettings, key: &str, value: &str) {
    match key {
        "json_max_bytes" => {
            if let Ok(n) = value.parse::<usize>() {
                agent.json_max_bytes = n;
            }
        }
        "timeout_ms" => {
            if let Ok(n) = value.parse::<u64>() {
                agent.timeout_ms = n;
            }
        }
        "include_meta" => agent.include_meta = value == "true",
        "block_caution" => agent.block_caution = value == "true",
        "sandbox_root" | "sandbox" => {
            agent.sandbox_root = (!value.is_empty()).then(|| value.to_string());
        }
        "allowlist" => agent.allowlist = parse_string_list(value),
        "audit_log" => agent.audit_log = (!value.is_empty()).then(|| value.to_string()),
        "session_path" | "session" => {
            agent.session_path = (!value.is_empty()).then(|| value.to_string());
        }
        "rc_mode" | "rc" => {
            if let Some(mode) = parse_agent_rc_mode(value) {
                agent.rc_mode = mode;
            }
        }
        _ => {}
    }
}

fn parse_string_list(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        trimmed[1..trimmed.len() - 1]
            .split(',')
            .filter_map(|part| {
                let part = trim_quotes(part.trim());
                if part.is_empty() {
                    None
                } else {
                    Some(part.to_string())
                }
            })
            .collect()
    } else if trimmed.is_empty() {
        Vec::new()
    } else {
        vec![trimmed.to_string()]
    }
}

fn parse_agent_rc_mode(value: &str) -> Option<AgentRcMode> {
    match value.to_ascii_lowercase().as_str() {
        "skip" | "none" | "off" => Some(AgentRcMode::Skip),
        "minimal" | "env" => Some(AgentRcMode::Minimal),
        "full" | "on" => Some(AgentRcMode::Full),
        _ => None,
    }
}

fn apply_agent_env(config: &mut ShellConfig) {
    let agent = &mut config.agent;
    if let Ok(v) = std::env::var("MSH_AGENT_JSON_MAX_BYTES") {
        if let Ok(n) = v.parse::<usize>() {
            agent.json_max_bytes = n;
        }
    }
    if let Ok(v) = std::env::var("MSH_AGENT_TIMEOUT_MS") {
        if let Ok(n) = v.parse::<u64>() {
            agent.timeout_ms = n;
        }
    }
    if let Ok(v) = std::env::var("MSH_AGENT_INCLUDE_META") {
        agent.include_meta = v == "1" || v.eq_ignore_ascii_case("true");
    }
    if let Ok(v) = std::env::var("MSH_AGENT_BLOCK_CAUTION") {
        agent.block_caution = v == "1" || v.eq_ignore_ascii_case("true");
    }
    if let Ok(v) = std::env::var("MSH_AGENT_SANDBOX") {
        if !v.is_empty() {
            agent.sandbox_root = Some(v);
        }
    }
    if let Ok(v) = std::env::var("MSH_AGENT_ALLOWLIST") {
        agent.allowlist = v
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
    }
    if let Ok(v) = std::env::var("MSH_AGENT_AUDIT_LOG") {
        if !v.is_empty() {
            agent.audit_log = Some(v);
        }
    }
    if let Ok(v) = std::env::var("MSH_AGENT_SESSION") {
        if !v.is_empty() {
            agent.session_path = Some(v);
        }
    }
    if let Ok(v) = std::env::var("MSH_AGENT_RC") {
        if let Some(mode) = parse_agent_rc_mode(&v) {
            agent.rc_mode = mode;
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
    // `<( )` はネイティブ対応。`>( )` は PoC 段階で bash 委譲を案内。
    if trimmed.contains(">(") {
        return Some(MshError::UnsupportedSyntax {
            feature: "process substitution >( )".into(),
            workaround: "use bash -c 'your command' or a named pipe".into(),
        });
    }
    None
}

pub fn default_config_template() -> &'static str {
    r#"# msh configuration
# compat = "msh"          # msh | bash | zsh
# language = "ja"         # en | ja（未設定時は $LC_ALL / $LC_MESSAGES / $LANG から推論）
# theme = "default"       # default | minimal（プロンプト色も連動）
# fuzzy_completion = true
# session_restore = false
# history_backend = "msh"     # msh | atuin
# load_bashrc = false
# load_zshrc = false
#
# [prompt]                    # 対話設定: msh --configure-prompt または `prompt config`
# style = "default"           # default | minimal | powerline
# preset = "msh"              # msh | classic | pure | rainbow | nord | high-contrast | none
# icons = true
# bold = false
# separator = "bar"         # chevron | arrow | round | bar | space
# newline = false             # プロンプト前に改行
# show_shell = true
# show_user_host = false
# show_k8s = false
# k8s_show_namespace = true
# show_git = true
# show_git_dirty = true
# show_git_ahead_behind = true
# show_time = false
# time_format = "24h"          # 24h | 12h | 24h-s
# show_battery = false
# battery_hide_full = true     # 100% 充電中は非表示
# show_duration = true
# transient_duration = true   # 成功時は遅いときだけ時間表示
# duration_min_ms = 50
# show_exit_on_success = false
#
# [prompt.colors.path]        # fg/bg: 色名 | hex RRGGBB | 0-255
# fg = "cyan"
# bg = "006064"
#
# [prompt.colors.git]
# fg = "black"
# bg = "yellow"

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
#
# [agent]                           # エージェント / --json / --agent / --mcp
# json_max_bytes = 65536            # stdout/stderr の JSON 上限（超過分は truncate）
# timeout_ms = 0                    # 0 = 無制限。子プロセスで再実行して kill
# include_meta = true                 # cwd / git_branch を JSON に含める
# block_caution = false               # true なら caution も --agent-force 必須
# sandbox_root = ""                   # 設定時は cwd/cd をこの配下に制限
# allowlist = ["echo", "cargo"]       # 空 = 制限なし
# audit_log = ""                      # JSON Lines 監査ログ
# session_path = ""                   # エージェントセッション cwd 永続化
# rc_mode = "skip"                    # skip | minimal | full
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
    map.insert(
        "prompt.style".into(),
        format!("{:?}", config.prompt.style).to_ascii_lowercase(),
    );
    map.insert(
        "prompt.preset".into(),
        format!("{:?}", config.prompt.preset).to_ascii_lowercase(),
    );
    map.insert("prompt.icons".into(), config.prompt.icons.to_string());
    map.insert(
        "prompt.show_duration".into(),
        config.prompt.show_duration.to_string(),
    );
    map
}

/// `msh setup` が書き込む推奨設定。
#[derive(Debug, Clone)]
pub struct SetupOptions {
    pub block_caution: bool,
    pub audit_log: PathBuf,
    pub session_path: PathBuf,
}

impl SetupOptions {
    pub fn from_home(home: &Path) -> Self {
        Self {
            block_caution: false,
            audit_log: home.join(".local/state/msh-agent.jsonl"),
            session_path: home.join(".config/msh/agent.session"),
        }
    }

    pub fn strict(home: &Path) -> Self {
        Self {
            block_caution: true,
            audit_log: home.join(".local/state/msh-agent.jsonl"),
            session_path: home.join(".config/msh/agent.session"),
        }
    }
}

pub fn setup_config_toml(opts: &SetupOptions) -> String {
    let block = if opts.block_caution { "true" } else { "false" };
    format!(
        r#"# msh configuration (generated by msh setup)
# Docs: docs/agent-integration.md

[agent]
json_max_bytes = 65536
include_meta = true
block_caution = {block}
rc_mode = "skip"
audit_log = "{audit}"
session_path = "{session}"

[prompt]
preset = "msh"
style = "default"
icons = true
show_duration = true
duration_min_ms = 50
"#,
        audit = opts.audit_log.display(),
        session = opts.session_path.display(),
    )
}

/// `~/.config/msh/config.toml` に setup 推奨設定をマージ保存する。
pub fn save_setup_config(opts: &SetupOptions) -> Result<PathBuf, MshError> {
    use std::fs;
    use std::io::Write as _;

    let dir = ShellConfig::config_dir()
        .ok_or_else(|| MshError::ScriptError("HOME is not set; cannot save setup config".into()))?;
    fs::create_dir_all(&dir).map_err(MshError::Io)?;
    if let Some(parent) = opts.audit_log.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let path = dir.join("config.toml");
    let mut content = if path.is_file() {
        fs::read_to_string(&path).map_err(MshError::Io)?
    } else {
        default_config_template().to_string()
    };

    content = strip_agent_sections(&content);
    content = strip_prompt_sections(&content);
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push('\n');
    content.push_str(&setup_config_toml(opts));

    let mut file = fs::File::create(&path).map_err(MshError::Io)?;
    file.write_all(content.as_bytes()).map_err(MshError::Io)?;
    Ok(path)
}

fn strip_agent_sections(content: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let name = trimmed.trim_start_matches('[').trim_end_matches(']').trim();
            skipping = name == "agent";
            if !skipping {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(line);
            }
            continue;
        }
        if !skipping {
            out.push('\n');
            out.push_str(line);
        }
    }
    out
}

/// `[prompt]` ブロックを TOML 文字列化（保存用）。
pub fn prompt_settings_to_toml(prompt: &PromptSettings) -> String {
    let mut out = String::from("[prompt]\n");
    let _ = writeln!(
        out,
        "style = \"{}\"",
        format!("{:?}", prompt.style).to_ascii_lowercase()
    );
    let _ = writeln!(out, "preset = \"{}\"", preset_name(prompt.preset));
    let _ = writeln!(out, "icons = {}", prompt.icons);
    let _ = writeln!(out, "bold = {}", prompt.bold);
    let _ = writeln!(out, "separator = \"{}\"", separator_name(prompt.separator));
    let _ = writeln!(out, "newline = {}", prompt.newline);
    let _ = writeln!(out, "show_shell = {}", prompt.show_shell);
    let _ = writeln!(out, "show_user_host = {}", prompt.show_user_host);
    let _ = writeln!(out, "show_k8s = {}", prompt.show_k8s);
    let _ = writeln!(out, "k8s_show_namespace = {}", prompt.k8s_show_namespace);
    let _ = writeln!(out, "show_git = {}", prompt.show_git);
    let _ = writeln!(out, "show_git_dirty = {}", prompt.show_git_dirty);
    let _ = writeln!(
        out,
        "show_git_ahead_behind = {}",
        prompt.show_git_ahead_behind
    );
    let _ = writeln!(out, "show_time = {}", prompt.show_time);
    let _ = writeln!(
        out,
        "time_format = \"{}\"",
        time_format_name(prompt.time_format)
    );
    let _ = writeln!(out, "show_battery = {}", prompt.show_battery);
    let _ = writeln!(out, "battery_hide_full = {}", prompt.battery_hide_full);
    let _ = writeln!(out, "show_duration = {}", prompt.show_duration);
    let _ = writeln!(out, "transient_duration = {}", prompt.transient_duration);
    let _ = writeln!(out, "duration_min_ms = {}", prompt.duration_min_ms);
    let _ = writeln!(
        out,
        "show_exit_on_success = {}",
        prompt.show_exit_on_success
    );

    write_color_section(&mut out, "path", &prompt.colors.path);
    write_color_section(&mut out, "shell", &prompt.colors.shell);
    write_color_section(&mut out, "git", &prompt.colors.git);
    write_color_section(&mut out, "git_dirty", &prompt.colors.git_dirty);
    write_color_section(&mut out, "duration", &prompt.colors.duration);
    write_color_section(&mut out, "exit_ok", &prompt.colors.exit_ok);
    write_color_section(&mut out, "exit_err", &prompt.colors.exit_err);
    write_color_section(&mut out, "user", &prompt.colors.user);
    write_color_section(&mut out, "time", &prompt.colors.time);
    write_color_section(&mut out, "battery", &prompt.colors.battery);
    write_color_section(&mut out, "battery_low", &prompt.colors.battery_low);
    write_color_section(&mut out, "k8s", &prompt.colors.k8s);
    write_color_section(&mut out, "char", &prompt.colors.prompt_char);
    out
}

fn preset_name(preset: PromptPreset) -> &'static str {
    match preset {
        PromptPreset::Msh => "msh",
        PromptPreset::None => "none",
        PromptPreset::Classic => "classic",
        PromptPreset::Pure => "pure",
        PromptPreset::Rainbow => "rainbow",
        PromptPreset::Nord => "nord",
        PromptPreset::HighContrast => "high-contrast",
    }
}

fn separator_name(sep: PromptSeparator) -> &'static str {
    match sep {
        PromptSeparator::Chevron => "chevron",
        PromptSeparator::Arrow => "arrow",
        PromptSeparator::Round => "round",
        PromptSeparator::Bar => "bar",
        PromptSeparator::Space => "space",
    }
}

fn time_format_name(format: TimeFormat) -> &'static str {
    match format {
        TimeFormat::Hm24 => "24h",
        TimeFormat::Hm12 => "12h",
        TimeFormat::Hms24 => "24h-s",
    }
}

fn write_color_section(out: &mut String, name: &str, colors: &crate::prompt_color::SegmentColors) {
    if colors.fg.is_none() && colors.bg.is_none() {
        return;
    }
    out.push('\n');
    let _ = writeln!(out, "[prompt.colors.{name}]");
    if let Some(fg) = &colors.fg {
        let _ = writeln!(out, "fg = \"{fg}\"");
    }
    if let Some(bg) = &colors.bg {
        let _ = writeln!(out, "bg = \"{bg}\"");
    }
}

/// `~/.config/msh/config.toml` の `[prompt]` 系セクションを差し替え保存。
pub fn save_prompt_settings(prompt: &PromptSettings) -> Result<(), MshError> {
    use std::fs;
    use std::io::Write as _;

    let dir = ShellConfig::config_dir().ok_or_else(|| {
        MshError::ScriptError("HOME is not set; cannot save prompt config".into())
    })?;
    fs::create_dir_all(&dir).map_err(|e| MshError::ScriptError(e.to_string()))?;
    let path = dir.join("config.toml");

    let mut content = if path.is_file() {
        fs::read_to_string(&path).map_err(|e| MshError::ScriptError(e.to_string()))?
    } else {
        default_config_template().to_string()
    };

    content = strip_prompt_sections(&content);
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push('\n');
    content.push_str(&prompt_settings_to_toml(prompt));

    let mut file = fs::File::create(&path).map_err(|e| MshError::ScriptError(e.to_string()))?;
    file.write_all(content.as_bytes())
        .map_err(|e| MshError::ScriptError(e.to_string()))?;
    Ok(())
}

fn strip_prompt_sections(content: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let name = trimmed.trim_start_matches('[').trim_end_matches(']').trim();
            skipping = name == "prompt" || name.starts_with("prompt.");
            if !skipping {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(line);
            }
            continue;
        }
        if !skipping {
            out.push('\n');
            out.push_str(line);
        }
    }
    out
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
    fn language_from_locale_values() {
        assert_eq!(language_from_locale("ja_JP.UTF-8"), Language::Ja);
        assert_eq!(language_from_locale("ja-JP"), Language::Ja);
        assert_eq!(language_from_locale("en_US.UTF-8"), Language::En);
        assert_eq!(language_from_locale("C"), Language::En);
    }

    #[test]
    fn apply_toml_tracks_explicit_language() {
        let mut config = ShellConfig {
            compat: CompatMode::Msh,
            load_bashrc: false,
            load_zshrc: false,
            language: Language::En,
            theme: Theme::Default,
            prompt: PromptSettings::default(),
            fuzzy_completion: true,
            session_restore: false,
            history_backend: HistoryBackend::Msh,
            ai: AiSettings::default(),
            agent: AgentSettings::default(),
        };
        assert!(!apply_toml(&mut config, "theme = \"minimal\""));
        assert_eq!(config.language, Language::En);
        assert!(apply_toml(&mut config, "lang = \"ja\""));
        assert_eq!(config.language, Language::Ja);
    }

    #[test]
    fn apply_toml_settings() {
        let mut config = ShellConfig {
            compat: CompatMode::Msh,
            load_bashrc: false,
            load_zshrc: false,
            language: Language::En,
            theme: Theme::Default,
            prompt: PromptSettings::default(),
            fuzzy_completion: true,
            session_restore: false,
            history_backend: HistoryBackend::Msh,
            ai: AiSettings::default(),
            agent: AgentSettings::default(),
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
    fn apply_toml_prompt_section() {
        let mut config = ShellConfig::default();
        apply_toml(
            &mut config,
            "[prompt]\nstyle = \"powerline\"\nshow_user_host = true\nduration_min_ms = 500",
        );
        assert_eq!(config.prompt.style, PromptStyle::Powerline);
        assert!(config.prompt.show_user_host);
        assert_eq!(config.prompt.duration_min_ms, 500);
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
