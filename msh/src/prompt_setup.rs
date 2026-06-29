use crate::config::{
    save_prompt_settings, Language, PromptPreset, PromptSeparator, PromptSettings, PromptStyle,
    Theme,
};
use crate::error::{MshError, Result};
use crate::prompt::{self, RenderContext};
use crate::prompt_color::apply_color_key;
use std::io::{self, BufRead, Write};

const WIZARD_STEPS: usize = 8;

pub fn run(
    config: &mut crate::config::ShellConfig,
    cache: &mut prompt::Cache,
    language: Language,
) -> Result<bool> {
    print_preview(&config.prompt, config.theme, cache);
    println!();
    println!(
        "{}",
        msg(language, "msh プロンプト設定", "msh prompt setup")
    );
    println!(
        "  1. {}",
        msg(
            language,
            "ガイド付きセットアップ（推奨 · 8 ステップ）",
            "Guided setup (recommended · 8 steps)"
        )
    );
    println!(
        "  2. {}",
        msg(
            language,
            "詳細メニュー — 項目ごとに編集",
            "Advanced menu — edit individual options"
        )
    );
    println!("  q. {}", msg(language, "終了", "Quit"));
    print!("{}", msg(language, "> [Enter=1] ", "> [Enter=1] "));
    io::stdout().flush().ok();

    let stdin = io::stdin();
    let mut lines = stdin.lock();
    let choice = read_line(&mut lines)?;
    match choice.trim().to_ascii_lowercase().as_str() {
        "" | "1" => run_wizard(config, cache, language, &mut lines),
        "2" | "a" | "advanced" => run_advanced_menu(config, cache, language, &mut lines),
        "q" | "quit" => Ok(false),
        _ => {
            println!("{}", msg(language, "無効な選択です。", "Invalid choice."));
            Ok(false)
        }
    }
}

fn run_wizard(
    config: &mut crate::config::ShellConfig,
    cache: &mut prompt::Cache,
    language: Language,
    lines: &mut impl BufRead,
) -> Result<bool> {
    let mut settings = config.prompt.clone();

    print_step(language, 1, msg(language, "全体の雰囲気", "Overall look"));
    println!(
        "  1. {}",
        msg(language, "msh 標準（バランス型）", "msh default (balanced)")
    );
    println!(
        "  2. {}",
        msg(language, "Classic（ターミナル定番）", "Classic terminal")
    );
    println!(
        "  3. {}",
        msg(
            language,
            "Pure（ミニマル・省スペース）",
            "Pure (minimal, compact)"
        )
    );
    println!(
        "  4. {}",
        msg(
            language,
            "Rainbow（カラフル Powerline）",
            "Rainbow (colorful powerline)"
        )
    );
    println!(
        "  5. {}",
        msg(language, "Nord（落ち着いた青灰）", "Nord (cool blue-gray)")
    );
    println!(
        "  6. {}",
        msg(language, "High contrast（高コントラスト）", "High contrast")
    );
    let vibe = ask_number(lines, language, 1, 6, 1)?;
    apply_theme_vibe(&mut settings, vibe);
    print_preview(&settings, config.theme, cache);

    print_step(language, 2, msg(language, "プロンプト形状", "Prompt shape"));
    println!(
        "  1. {}",
        msg(language, "Default（フラット）", "Default (flat segments)")
    );
    println!(
        "  2. {}",
        msg(
            language,
            "Powerline（Powerlevel10k 風ブロック）",
            "Powerline (Powerlevel10k-like blocks)"
        )
    );
    println!(
        "  3. {}",
        msg(
            language,
            "Minimal（1 行コンパクト）",
            "Minimal (single-line compact)"
        )
    );
    let shape = ask_number(
        lines,
        language,
        1,
        3,
        if settings.preset == PromptPreset::Pure {
            3
        } else if settings.preset == PromptPreset::Rainbow || settings.preset == PromptPreset::Nord
        {
            2
        } else {
            1
        },
    )?;
    apply_prompt_shape(&mut settings, shape);
    print_preview(&settings, config.theme, cache);

    if settings.style == PromptStyle::Powerline {
        print_step(language, 3, msg(language, "区切り文字", "Separator"));
        println!(
            "  1. Chevron  ›  ({})",
            msg(language, "Powerlevel10k 風", "Powerlevel10k-like")
        );
        println!("  2. Arrow     ❯");
        println!("  3. Round     ●");
        println!("  4. Bar       |");
        println!("  5. Space     (gap)");
        let sep = ask_number(lines, language, 1, 5, 1)?;
        settings.separator = match sep {
            2 => PromptSeparator::Arrow,
            3 => PromptSeparator::Round,
            4 => PromptSeparator::Bar,
            5 => PromptSeparator::Space,
            _ => PromptSeparator::Chevron,
        };
        print_preview(&settings, config.theme, cache);
    } else {
        wizard_skip_note(
            language,
            3,
            msg(
                language,
                "Powerline 以外は区切り不要",
                "Skipped — not using Powerline",
            ),
        );
    }

    print_step(language, 4, msg(language, "アイコン", "Icons"));
    settings.icons = ask_yes(
        lines,
        language,
        &msg(
            language,
            "セグメントにアイコンを表示しますか？",
            "Show icons in segments?",
        ),
        settings.icons,
    )?;
    print_preview(&settings, config.theme, cache);

    print_step(language, 5, msg(language, "Git 情報", "Git info"));
    settings.show_git = ask_yes(
        lines,
        language,
        &msg(
            language,
            "ブランチ・dirty 状態を表示しますか？",
            "Show branch and dirty state?",
        ),
        settings.show_git,
    )?;
    print_preview(&settings, config.theme, cache);

    print_step(
        language,
        6,
        msg(language, "コマンド実行時間", "Command duration"),
    );
    settings.show_duration = ask_yes(
        lines,
        language,
        &msg(
            language,
            "実行時間を表示しますか？",
            "Show command duration?",
        ),
        settings.show_duration,
    )?;
    if settings.show_duration {
        println!(
            "  1. {}",
            msg(
                language,
                "遅いときだけ（50ms 以上）",
                "Only when slow (50ms+)"
            )
        );
        println!(
            "  2. {}",
            msg(
                language,
                "かなり遅いとき（200ms 以上）",
                "When noticeably slow (200ms+)"
            )
        );
        println!("  3. {}", msg(language, "常に表示", "Always show"));
        let threshold = ask_number(lines, language, 1, 3, 1)?;
        settings.transient_duration = threshold != 3;
        settings.duration_min_ms = match threshold {
            2 => 200,
            3 => 0,
            _ => 50,
        };
    }
    print_preview(&settings, config.theme, cache);

    print_step(language, 7, msg(language, "改行", "Newline"));
    settings.newline = ask_yes(
        lines,
        language,
        &msg(
            language,
            "プロンプトの前に空行を入れますか？",
            "Add a blank line before the prompt?",
        ),
        settings.newline,
    )?;
    print_preview(&settings, config.theme, cache);

    print_step(
        language,
        8,
        msg(language, "追加セグメント（任意）", "Optional segments"),
    );
    println!(
        "{}",
        msg(
            language,
            "各項目 Enter で現在値を維持",
            "Press Enter on each to keep the current default"
        )
    );
    settings.show_time = ask_yes(
        lines,
        language,
        &msg(language, "時刻を表示しますか？", "Show time?"),
        settings.show_time,
    )?;
    settings.show_battery = ask_yes(
        lines,
        language,
        &msg(language, "バッテリー残量を表示しますか？", "Show battery?"),
        settings.show_battery,
    )?;
    settings.show_k8s = ask_yes(
        lines,
        language,
        &msg(
            language,
            "Kubernetes context を表示しますか？",
            "Show Kubernetes context?",
        ),
        settings.show_k8s,
    )?;
    print_preview(&settings, config.theme, cache);

    println!();
    println!("{}", msg(language, "=== 確認 ===", "=== Confirm ==="));
    print_preview(&settings, config.theme, cache);
    println!("  s. {}", msg(language, "保存して終了", "Save and exit"));
    println!(
        "  a. {}",
        msg(
            language,
            "詳細メニューで微調整",
            "Fine-tune in advanced menu"
        )
    );
    println!(
        "  r. {}",
        msg(language, "最初からやり直す", "Restart wizard")
    );
    println!(
        "  q. {}",
        msg(language, "保存せず終了", "Quit without saving")
    );
    print!("{}", msg(language, "> [Enter=s] ", "> [Enter=s] "));
    io::stdout().flush().ok();
    let choice = read_line(lines)?;
    match choice.trim().to_ascii_lowercase().as_str() {
        "" | "s" | "save" | "y" | "yes" => {
            save_prompt_settings(&settings)?;
            config.prompt = settings;
            println!(
                "{}",
                msg(
                    language,
                    "保存しました: ~/.config/msh/config.toml",
                    "Saved: ~/.config/msh/config.toml"
                )
            );
            Ok(true)
        }
        "a" | "advanced" => {
            config.prompt = settings;
            run_advanced_menu(config, cache, language, lines)
        }
        "r" | "restart" => run_wizard(config, cache, language, lines),
        _ => Ok(false),
    }
}

fn run_advanced_menu(
    config: &mut crate::config::ShellConfig,
    cache: &mut prompt::Cache,
    language: Language,
    lines: &mut impl BufRead,
) -> Result<bool> {
    let mut settings = config.prompt.clone();
    let mut dirty = false;

    loop {
        print_preview(&settings, config.theme, cache);
        print_advanced_menu(language);
        print!("> ");
        io::stdout().flush().ok();
        let choice = read_line(lines)?;
        let choice = choice.trim();

        match choice {
            "1" => {
                settings.style = pick_style(lines, language, settings.style)?;
                dirty = true;
            }
            "2" => {
                settings.preset = pick_preset(lines, language, settings.preset)?;
                dirty = true;
            }
            "3" => {
                settings.separator = pick_separator(lines, language, settings.separator)?;
                dirty = true;
            }
            "4" => {
                toggle_bool(
                    &mut settings.icons,
                    lines,
                    language,
                    &msg(language, "アイコン", "Icons"),
                )?;
                dirty = true;
            }
            "5" => {
                toggle_bool(
                    &mut settings.show_git,
                    lines,
                    language,
                    &msg(language, "Git セグメント", "Git segment"),
                )?;
                dirty = true;
            }
            "6" => {
                toggle_bool(
                    &mut settings.show_duration,
                    lines,
                    language,
                    &msg(language, "実行時間", "Duration"),
                )?;
                dirty = true;
            }
            "7" => {
                settings.duration_min_ms =
                    pick_duration_threshold(lines, language, settings.duration_min_ms)?;
                dirty = true;
            }
            "8" => {
                toggle_bool(
                    &mut settings.newline,
                    lines,
                    language,
                    &msg(language, "プロンプト前改行", "Newline before prompt"),
                )?;
                dirty = true;
            }
            "9" => {
                customize_segment_color(&mut settings, lines, language)?;
                dirty = true;
            }
            "10" => {
                toggle_bool(
                    &mut settings.show_time,
                    lines,
                    language,
                    &msg(language, "時刻", "Time"),
                )?;
                dirty = true;
            }
            "11" => {
                toggle_bool(
                    &mut settings.show_battery,
                    lines,
                    language,
                    &msg(language, "バッテリー", "Battery"),
                )?;
                dirty = true;
            }
            "12" => {
                toggle_bool(
                    &mut settings.show_k8s,
                    lines,
                    language,
                    &msg(language, "K8s context", "K8s context"),
                )?;
                dirty = true;
            }
            "w" | "W" => return run_wizard(config, cache, language, lines),
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
                    let ans = read_line(lines)?;
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

pub fn apply_theme_vibe(settings: &mut PromptSettings, choice: u8) {
    match choice {
        2 => {
            settings.preset = PromptPreset::Classic;
            settings.style = PromptStyle::Default;
            settings.separator = PromptSeparator::Bar;
        }
        3 => {
            settings.preset = PromptPreset::Pure;
            settings.style = PromptStyle::Minimal;
            settings.icons = false;
            settings.show_duration = false;
        }
        4 => {
            settings.preset = PromptPreset::Rainbow;
            settings.style = PromptStyle::Powerline;
            settings.separator = PromptSeparator::Chevron;
        }
        5 => {
            settings.preset = PromptPreset::Nord;
            settings.style = PromptStyle::Powerline;
            settings.separator = PromptSeparator::Chevron;
        }
        6 => {
            settings.preset = PromptPreset::HighContrast;
            settings.style = PromptStyle::Powerline;
            settings.separator = PromptSeparator::Bar;
        }
        _ => {
            settings.preset = PromptPreset::Msh;
            settings.style = PromptStyle::Default;
            settings.separator = PromptSeparator::Bar;
        }
    }
}

pub fn apply_prompt_shape(settings: &mut PromptSettings, choice: u8) {
    match choice {
        2 => {
            settings.style = PromptStyle::Powerline;
            if settings.separator == PromptSeparator::Space {
                settings.separator = PromptSeparator::Chevron;
            }
        }
        3 => settings.style = PromptStyle::Minimal,
        _ => settings.style = PromptStyle::Default,
    }
}

fn print_step(language: Language, step: usize, title: String) {
    println!();
    println!(
        "--- {} {step}/{WIZARD_STEPS}: {title} ---",
        msg(language, "ステップ", "Step")
    );
}

fn wizard_skip_note(language: Language, step: usize, reason: String) {
    println!();
    println!(
        "--- {} {step}/{WIZARD_STEPS}: {} ---",
        msg(language, "ステップ", "Step"),
        msg(language, "スキップ", "Skipped")
    );
    println!("  {reason}");
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

fn print_advanced_menu(language: Language) {
    println!();
    println!("{}", msg(language, "詳細メニュー", "Advanced menu"));
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
        "  w. {}",
        msg(language, "ガイド付きセットアップへ", "Back to guided setup")
    );
    println!(
        "  p. {}",
        msg(language, "プレビュー再表示", "Refresh preview")
    );
    println!("  s. {}", msg(language, "保存して終了", "Save and exit"));
    println!("  q. {}", msg(language, "終了", "Quit"));
}

fn ask_yes(
    lines: &mut impl BufRead,
    _language: Language,
    prompt: &str,
    default: bool,
) -> Result<bool> {
    let hint = if default { "[Y/n]" } else { "[y/N]" };
    print!("{prompt} {hint} ");
    io::stdout().flush().ok();
    let line = read_line(lines)?;
    let t = line.trim();
    if t.is_empty() {
        return Ok(default);
    }
    Ok(matches!(
        t.to_ascii_lowercase().as_str(),
        "y" | "yes" | "true" | "1" | "on" | "はい"
    ))
}

fn ask_number(
    lines: &mut impl BufRead,
    language: Language,
    min: u8,
    max: u8,
    default: u8,
) -> Result<u8> {
    print!(
        "{} [{default}] ",
        msg(language, "番号を入力", "Enter number")
    );
    io::stdout().flush().ok();
    let line = read_line(lines)?;
    let t = line.trim();
    if t.is_empty() {
        return Ok(default);
    }
    let Ok(n) = t.parse::<u8>() else {
        println!(
            "{}",
            msg(
                language,
                "無効な入力 — 既定値を使用",
                "Invalid — using default"
            )
        );
        return Ok(default);
    };
    if (min..=max).contains(&n) {
        Ok(n)
    } else {
        println!(
            "{}",
            msg(
                language,
                "範囲外 — 既定値を使用",
                "Out of range — using default"
            )
        );
        Ok(default)
    }
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

    #[test]
    fn theme_vibe_rainbow_uses_powerline() {
        let mut settings = PromptSettings::default();
        apply_theme_vibe(&mut settings, 4);
        assert_eq!(settings.preset, PromptPreset::Rainbow);
        assert_eq!(settings.style, PromptStyle::Powerline);
        assert_eq!(settings.separator, PromptSeparator::Chevron);
    }

    #[test]
    fn theme_vibe_pure_is_minimal() {
        let mut settings = PromptSettings::default();
        apply_theme_vibe(&mut settings, 3);
        assert_eq!(settings.preset, PromptPreset::Pure);
        assert_eq!(settings.style, PromptStyle::Minimal);
        assert!(!settings.icons);
    }

    #[test]
    fn prompt_shape_powerline_sets_chevron_when_space() {
        let mut settings = PromptSettings {
            separator: PromptSeparator::Space,
            ..Default::default()
        };
        apply_prompt_shape(&mut settings, 2);
        assert_eq!(settings.style, PromptStyle::Powerline);
        assert_eq!(settings.separator, PromptSeparator::Chevron);
    }
}
