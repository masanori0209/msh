use crate::config::{
    save_prompt_settings, Language, PromptPreset, PromptSeparator, PromptSettings, PromptStyle,
    Theme,
};
use crate::error::{MshError, Result};
use crate::prompt::{self, RenderContext};
use crate::prompt_color::apply_color_key;
use std::io::{self, BufRead, Write};

pub fn run(
    config: &mut crate::config::ShellConfig,
    cache: &mut prompt::Cache,
    language: Language,
) -> Result<bool> {
    let mut settings = config.prompt.clone();
    let mut dirty = false;
    let stdin = io::stdin();
    let mut lines = stdin.lock();

    loop {
        print_preview(&settings, config.theme, cache);
        print_menu(language);
        print!("> ");
        io::stdout().flush().ok();
        let choice = read_line(&mut lines)?;
        let choice = choice.trim();

        match choice {
            "1" => {
                settings.style = pick_style(&mut lines, language, settings.style)?;
                dirty = true;
            }
            "2" => {
                settings.preset = pick_preset(&mut lines, language, settings.preset)?;
                dirty = true;
            }
            "3" => {
                settings.separator = pick_separator(&mut lines, language, settings.separator)?;
                dirty = true;
            }
            "4" => {
                toggle_bool(
                    &mut settings.icons,
                    &mut lines,
                    language,
                    &msg(language, "アイコン", "Icons"),
                )?;
                dirty = true;
            }
            "5" => {
                toggle_bool(
                    &mut settings.show_git,
                    &mut lines,
                    language,
                    &msg(language, "Git セグメント", "Git segment"),
                )?;
                dirty = true;
            }
            "6" => {
                toggle_bool(
                    &mut settings.show_duration,
                    &mut lines,
                    language,
                    &msg(language, "実行時間", "Duration"),
                )?;
                dirty = true;
            }
            "7" => {
                settings.duration_min_ms =
                    pick_duration_threshold(&mut lines, language, settings.duration_min_ms)?;
                dirty = true;
            }
            "8" => {
                toggle_bool(
                    &mut settings.newline,
                    &mut lines,
                    language,
                    &msg(language, "プロンプト前改行", "Newline before prompt"),
                )?;
                dirty = true;
            }
            "9" => {
                customize_segment_color(&mut settings, &mut lines, language)?;
                dirty = true;
            }
            "10" => {
                toggle_bool(
                    &mut settings.show_time,
                    &mut lines,
                    language,
                    &msg(language, "時刻", "Time"),
                )?;
                dirty = true;
            }
            "11" => {
                toggle_bool(
                    &mut settings.show_battery,
                    &mut lines,
                    language,
                    &msg(language, "バッテリー", "Battery"),
                )?;
                dirty = true;
            }
            "12" => {
                toggle_bool(
                    &mut settings.show_k8s,
                    &mut lines,
                    language,
                    &msg(language, "K8s context", "K8s context"),
                )?;
                dirty = true;
            }
            "s" | "S" => {
                if !dirty {
                    println!("{}", msg(language, "変更はありません。", "No changes."));
                    continue;
                }
                save_prompt_settings(&settings)?;
                config.prompt = settings.clone();
                println!(
                    "{}",
                    msg(
                        language,
                        "保存しました: ~/.config/msh/config.toml",
                        "Saved: ~/.config/msh/config.toml"
                    )
                );
                return Ok(true);
            }
            "q" | "Q" | "" => {
                if dirty {
                    print!(
                        "{}",
                        msg(
                            language,
                            "保存せず終了? [y/N] ",
                            "Quit without saving? [y/N] "
                        )
                    );
                    io::stdout().flush().ok();
                    let ans = read_line(&mut lines)?;
                    if ans.trim().eq_ignore_ascii_case("y") {
                        return Ok(false);
                    }
                    continue;
                }
                return Ok(false);
            }
            "p" | "P" => {}
            _ => println!("{}", msg(language, "無効な選択です。", "Invalid choice.")),
        }
    }
}

fn print_preview(settings: &PromptSettings, theme: Theme, cache: &mut prompt::Cache) {
    let sample = prompt::render(RenderContext {
        last_status: 0,
        last_duration: std::time::Duration::from_millis(320),
        cache,
        settings,
        theme,
    });
    println!();
    println!("--- preview ---");
    println!("{sample}");
    println!("---------------");
}

fn print_menu(language: Language) {
    println!();
    println!("{}", msg(language, "プロンプト設定", "Prompt setup"));
    println!(
        "  1. {}",
        msg(language, "スタイル (default/minimal/powerline)", "Style")
    );
    println!("  2. {}", msg(language, "カラープリセット", "Color preset"));
    println!(
        "  3. {}",
        msg(language, "セパレータ形状", "Separator shape")
    );
    println!("  4. {}", msg(language, "アイコン ON/OFF", "Icons ON/OFF"));
    println!(
        "  5. {}",
        msg(language, "Git 表示 ON/OFF", "Git segment ON/OFF")
    );
    println!(
        "  6. {}",
        msg(language, "実行時間 ON/OFF", "Duration ON/OFF")
    );
    println!(
        "  7. {}",
        msg(language, "実行時間の最小 ms", "Duration minimum ms")
    );
    println!("  8. {}", msg(language, "改行 ON/OFF", "Newline ON/OFF"));
    println!(
        "  9. {}",
        msg(
            language,
            "セグメント色 (fg/bg) を個別指定",
            "Custom segment colors (fg/bg)"
        )
    );
    println!(" 10. {}", msg(language, "時刻 ON/OFF", "Time ON/OFF"));
    println!(
        " 11. {}",
        msg(language, "バッテリー ON/OFF", "Battery ON/OFF")
    );
    println!(
        " 12. {}",
        msg(language, "K8s context ON/OFF", "K8s context ON/OFF")
    );
    println!(
        "  p. {}",
        msg(language, "プレビュー再表示", "Refresh preview")
    );
    println!("  s. {}", msg(language, "保存して終了", "Save and exit"));
    println!("  q. {}", msg(language, "終了", "Quit"));
}

fn pick_style(
    lines: &mut impl BufRead,
    language: Language,
    current: PromptStyle,
) -> Result<PromptStyle> {
    println!(
        "{}: default | minimal | powerline (now: {:?})",
        msg(language, "スタイル", "Style"),
        current
    );
    print!("> ");
    io::stdout().flush().ok();
    let v = read_line(lines)?;
    Ok(match v.trim().to_ascii_lowercase().as_str() {
        "minimal" => PromptStyle::Minimal,
        "powerline" | "blocks" => PromptStyle::Powerline,
        _ => PromptStyle::Default,
    })
}

fn pick_preset(
    lines: &mut impl BufRead,
    language: Language,
    current: PromptPreset,
) -> Result<PromptPreset> {
    println!(
        "{}: msh | classic | pure | rainbow | nord | high-contrast | none (now: {:?})",
        msg(language, "カラーテーマ", "Color theme"),
        current
    );
    print!("> ");
    io::stdout().flush().ok();
    let v = read_line(lines)?;
    Ok(match v.trim().to_ascii_lowercase().as_str() {
        "none" | "off" => PromptPreset::None,
        "pure" => PromptPreset::Pure,
        "rainbow" => PromptPreset::Rainbow,
        "nord" => PromptPreset::Nord,
        "high-contrast" | "contrast" => PromptPreset::HighContrast,
        "classic" => PromptPreset::Classic,
        _ => PromptPreset::Msh,
    })
}

fn pick_separator(
    lines: &mut impl BufRead,
    language: Language,
    current: PromptSeparator,
) -> Result<PromptSeparator> {
    println!(
        "{}: chevron | arrow | round | bar | space (now: {:?})",
        msg(language, "セパレータ", "Separator"),
        current
    );
    print!("> ");
    io::stdout().flush().ok();
    let v = read_line(lines)?;
    Ok(match v.trim().to_ascii_lowercase().as_str() {
        "arrow" => PromptSeparator::Arrow,
        "round" => PromptSeparator::Round,
        "bar" => PromptSeparator::Bar,
        "space" => PromptSeparator::Space,
        _ => PromptSeparator::Chevron,
    })
}

fn pick_duration_threshold(
    lines: &mut impl BufRead,
    language: Language,
    current: u64,
) -> Result<u64> {
    println!(
        "{} (now: {current}): ",
        msg(
            language,
            "最小表示時間 ms (0=常に表示)",
            "Minimum duration ms (0=always show)"
        )
    );
    print!("> ");
    io::stdout().flush().ok();
    let v = read_line(lines)?;
    Ok(v.trim().parse().unwrap_or(current))
}

fn toggle_bool(
    value: &mut bool,
    lines: &mut impl BufRead,
    language: Language,
    label: &str,
) -> Result<()> {
    println!(
        "{}: {} [y/n] (now: {})",
        label,
        msg(language, "有効", "Enable"),
        if *value { "on" } else { "off" }
    );
    print!("> ");
    io::stdout().flush().ok();
    let v = read_line(lines)?;
    match v.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" | "true" | "1" | "on" => *value = true,
        "n" | "no" | "false" | "0" | "off" => *value = false,
        "" => {}
        _ => println!("{}", msg(language, "変更なし", "Unchanged")),
    }
    Ok(())
}

fn customize_segment_color(
    settings: &mut PromptSettings,
    lines: &mut impl BufRead,
    language: Language,
) -> Result<()> {
    println!(
        "{}: shell | path | git | git_dirty | time | battery | battery_low | k8s | duration | exit_ok | exit_err | user | char",
        msg(language, "セグメント名", "Segment name")
    );
    print!("> ");
    io::stdout().flush().ok();
    let segment = read_line(lines)?.trim().to_string();
    if segment.is_empty() {
        return Ok(());
    }
    let section = format!("prompt.colors.{segment}");
    println!(
        "{}",
        msg(
            language,
            "前景色 fg (空=スキップ)",
            "Foreground fg (empty=skip)"
        )
    );
    print!("> ");
    io::stdout().flush().ok();
    let fg = read_line(lines)?;
    if !fg.trim().is_empty() {
        apply_color_key(&mut settings.colors, &section, "fg", fg.trim());
    }
    println!(
        "{}",
        msg(
            language,
            "背景色 bg (空=スキップ)",
            "Background bg (empty=skip)"
        )
    );
    print!("> ");
    io::stdout().flush().ok();
    let bg = read_line(lines)?;
    if !bg.trim().is_empty() {
        apply_color_key(&mut settings.colors, &section, "bg", bg.trim());
    }
    Ok(())
}

fn read_line(lines: &mut impl BufRead) -> Result<String> {
    let mut buf = String::new();
    lines
        .read_line(&mut buf)
        .map_err(|e| MshError::ScriptError(e.to_string()))?;
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
    fn customize_applies_path_fg() {
        let mut settings = PromptSettings::default();
        apply_color_key(&mut settings.colors, "prompt.colors.path", "fg", "#88c0d0");
        assert_eq!(settings.colors.path.fg.as_deref(), Some("#88c0d0"));
    }
}
