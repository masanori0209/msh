use crate::config::Language;
use std::fs;
use std::path::PathBuf;

pub fn maybe_show(config_dir: &PathBuf, language: Language) {
    let marker = config_dir.join(".onboarded");
    if marker.is_file() {
        return;
    }

    println!("{}", title(language));
    println!();
    println!("{}", section_daily(language));
    println!("  Tab          {}", msg(language, "補完", "completion"));
    println!(
        "  Ctrl+R       {}",
        msg(language, "履歴検索", "history search")
    );
    println!(
        "  help         {}",
        msg(language, "組み込みコマンド", "builtins")
    );
    println!(
        "  prompt config  {}",
        msg(
            language,
            "プロンプト設定（8 ステップ）",
            "prompt setup (8 steps)"
        )
    );
    println!();
    println!("{}", section_agent(language));
    println!(
        "  msh setup    {}",
        msg(language, "初回セットアップ", "first-time setup")
    );
    println!(
        "  msh doctor   {}",
        msg(language, "設定・MCP 検証", "verify config & MCP")
    );
    println!(
        "  msh --agent  {}",
        msg(language, "安全 + JSON 実行", "safe JSON execution")
    );
    println!();
    println!("{}", section_config(language));
    println!("  ~/.config/msh/config.toml");
    println!("  docs/agent-integration.md");
    println!();

    let _ = fs::create_dir_all(config_dir);
    let _ = fs::write(marker, "1");
}

pub fn quick_tip(language: Language) {
    println!(
        "{}",
        msg(
            language,
            "tip: `help` · Tab 補完 · `msh setup` · `prompt config`",
            "tip: `help` · Tab completion · `msh setup` · `prompt config`"
        )
    );
}

fn title(language: Language) -> String {
    match language {
        Language::Ja => "msh へようこそ — 日常シェル + エージェント向け実行".into(),
        Language::En => "Welcome to msh — daily shell + agent-ready execution".into(),
    }
}

fn section_daily(language: Language) -> String {
    match language {
        Language::Ja => "【日常】".into(),
        Language::En => "[Daily]".into(),
    }
}

fn section_agent(language: Language) -> String {
    match language {
        Language::Ja => "【エージェント / Cursor · Claude · Codex】".into(),
        Language::En => "[Agents / Cursor · Claude · Codex]".into(),
    }
}

fn section_config(language: Language) -> String {
    match language {
        Language::Ja => "【設定】".into(),
        Language::En => "[Config]".into(),
    }
}

fn msg(language: Language, ja: &str, en: &str) -> String {
    match language {
        Language::Ja => ja.to_string(),
        Language::En => en.to_string(),
    }
}
