//! プロンプト色のパース・プリセット解決。

use crate::config::{PromptPreset, PromptSeparator, PromptSettings, PromptStyle, Theme};
use std::fmt::Write as _;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SegmentColors {
    pub fg: Option<String>,
    pub bg: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptColors {
    pub shell: SegmentColors,
    pub path: SegmentColors,
    pub k8s: SegmentColors,
    pub git: SegmentColors,
    pub git_dirty: SegmentColors,
    pub time: SegmentColors,
    pub battery: SegmentColors,
    pub battery_low: SegmentColors,
    pub duration: SegmentColors,
    pub exit_ok: SegmentColors,
    pub exit_err: SegmentColors,
    pub user: SegmentColors,
    pub prompt_char: SegmentColors,
}

#[derive(Debug, Clone)]
pub struct ResolvedStyle {
    pub fg: String,
    pub bg: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedPalette {
    pub shell: ResolvedStyle,
    pub path: ResolvedStyle,
    pub k8s: ResolvedStyle,
    pub git: ResolvedStyle,
    pub git_dirty: ResolvedStyle,
    pub time: ResolvedStyle,
    pub battery: ResolvedStyle,
    pub battery_low: ResolvedStyle,
    pub duration: ResolvedStyle,
    pub exit_ok: ResolvedStyle,
    pub exit_err: ResolvedStyle,
    pub user: ResolvedStyle,
    pub prompt_char: ResolvedStyle,
    pub dim: String,
    pub separator: String,
    pub bold: bool,
}

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";

pub fn resolve_palette(settings: &PromptSettings, theme: Theme) -> ResolvedPalette {
    let base = preset_base(settings.preset, settings.style, theme);
    let merged = merge_overrides(base, &settings.colors);
    let separator = separator_glyph(settings.separator, settings.icons);
    let bold = !matches!(settings.style, PromptStyle::Minimal)
        && !matches!(theme, Theme::Minimal)
        && settings.bold;

    ResolvedPalette {
        shell: resolve_pair(&merged.shell, bold),
        path: resolve_pair(&merged.path, bold),
        k8s: resolve_pair(&merged.k8s, bold),
        git: resolve_pair(&merged.git, bold),
        git_dirty: resolve_pair(&merged.git_dirty, bold),
        time: resolve_pair(&merged.time, bold),
        battery: resolve_pair(&merged.battery, bold),
        battery_low: resolve_pair(&merged.battery_low, bold),
        duration: resolve_pair(&merged.duration, bold),
        exit_ok: resolve_pair(&merged.exit_ok, bold),
        exit_err: resolve_pair(&merged.exit_err, bold),
        user: resolve_pair(&merged.user, bold),
        prompt_char: resolve_pair(&merged.prompt_char, false),
        dim: DIM.to_string(),
        separator,
        bold,
    }
}

fn resolve_pair(pair: &SegmentColors, bold: bool) -> ResolvedStyle {
    let mut fg = pair.fg.as_deref().map(parse_color).unwrap_or_default();
    let bg = pair.bg.as_deref().map(parse_bg_color).unwrap_or_default();
    if bold && !fg.is_empty() {
        fg.push_str(BOLD);
    }
    ResolvedStyle { fg, bg }
}

fn merge_overrides(mut base: PromptColors, overrides: &PromptColors) -> PromptColors {
    merge_segment(&mut base.shell, &overrides.shell);
    merge_segment(&mut base.path, &overrides.path);
    merge_segment(&mut base.k8s, &overrides.k8s);
    merge_segment(&mut base.git, &overrides.git);
    merge_segment(&mut base.git_dirty, &overrides.git_dirty);
    merge_segment(&mut base.time, &overrides.time);
    merge_segment(&mut base.battery, &overrides.battery);
    merge_segment(&mut base.battery_low, &overrides.battery_low);
    merge_segment(&mut base.duration, &overrides.duration);
    merge_segment(&mut base.exit_ok, &overrides.exit_ok);
    merge_segment(&mut base.exit_err, &overrides.exit_err);
    merge_segment(&mut base.user, &overrides.user);
    merge_segment(&mut base.prompt_char, &overrides.prompt_char);
    base
}

fn merge_segment(base: &mut SegmentColors, over: &SegmentColors) {
    if over.fg.is_some() {
        base.fg.clone_from(&over.fg);
    }
    if over.bg.is_some() {
        base.bg.clone_from(&over.bg);
    }
}

fn preset_base(preset: PromptPreset, style: PromptStyle, theme: Theme) -> PromptColors {
    let mut colors = if matches!(theme, Theme::Minimal) || matches!(style, PromptStyle::Minimal) {
        PromptColors {
            shell: seg("bright-black", None),
            path: seg("white", None),
            k8s: seg("bright-cyan", None),
            git: seg("bright-black", None),
            git_dirty: seg("yellow", None),
            time: seg("bright-black", None),
            battery: seg("green", None),
            battery_low: seg("red", None),
            duration: seg("bright-black", None),
            exit_ok: seg("green", None),
            exit_err: seg("red", None),
            user: seg("bright-black", None),
            prompt_char: seg("bright-cyan", None),
        }
    } else {
        match preset {
            PromptPreset::Msh => msh_palette(style),
            PromptPreset::None | PromptPreset::Classic => classic_palette(style),
            PromptPreset::Pure => pure_palette(),
            PromptPreset::Rainbow => rainbow_palette(style),
            PromptPreset::Nord => nord_palette(style),
            PromptPreset::HighContrast => high_contrast_palette(style),
        }
    };
    enrich_extra_segments(&mut colors, style);
    colors
}

fn enrich_extra_segments(p: &mut PromptColors, style: PromptStyle) {
    let powerline = matches!(style, PromptStyle::Powerline);
    let slate = Some("236");
    if p.k8s.fg.is_none() && p.k8s.bg.is_none() {
        p.k8s = seg("109", if powerline { slate } else { None });
    }
    if p.time.fg.is_none() && p.time.bg.is_none() {
        p.time = seg("245", if powerline { slate } else { None });
    }
    if p.battery.fg.is_none() && p.battery.bg.is_none() {
        p.battery = seg("108", if powerline { slate } else { None });
    }
    if p.battery_low.fg.is_none() && p.battery_low.bg.is_none() {
        p.battery_low = seg("203", if powerline { slate } else { None });
    }
}

/// msh 標準プリセット。落ち着いた単色ベース + アクセント fg。
fn msh_palette(style: PromptStyle) -> PromptColors {
    if matches!(style, PromptStyle::Powerline) {
        let bar = Some("236");
        PromptColors {
            shell: seg("245", bar),
            path: seg("117", bar),
            k8s: seg("109", bar),
            git: seg("108", bar),
            git_dirty: seg("214", bar),
            time: seg("245", bar),
            battery: seg("108", bar),
            battery_low: seg("203", bar),
            duration: seg("139", bar),
            exit_ok: seg("108", None),
            exit_err: seg("203", None),
            user: seg("245", bar),
            prompt_char: seg("117", None),
        }
    } else {
        PromptColors {
            shell: seg("245", None),
            path: seg("117", None),
            k8s: seg("109", None),
            git: seg("108", None),
            git_dirty: seg("214", None),
            time: seg("245", None),
            battery: seg("108", None),
            battery_low: seg("203", None),
            duration: seg("245", None),
            exit_ok: seg("108", None),
            exit_err: seg("203", None),
            user: seg("245", None),
            prompt_char: seg("117", None),
        }
    }
}

fn classic_palette(style: PromptStyle) -> PromptColors {
    if matches!(style, PromptStyle::Powerline) {
        let bar = Some("238");
        PromptColors {
            shell: seg("254", bar),
            path: seg("117", bar),
            git: seg("108", bar),
            git_dirty: seg("214", bar),
            duration: seg("139", bar),
            exit_ok: seg("108", None),
            exit_err: seg("203", None),
            user: seg("254", bar),
            prompt_char: seg("117", None),
            ..Default::default()
        }
    } else {
        PromptColors {
            shell: seg("245", None),
            path: seg("117", None),
            git: seg("108", None),
            git_dirty: seg("214", None),
            duration: seg("245", None),
            exit_ok: seg("108", None),
            exit_err: seg("203", None),
            user: seg("245", None),
            prompt_char: seg("117", None),
            ..Default::default()
        }
    }
}

fn pure_palette() -> PromptColors {
    PromptColors {
        shell: seg("bright-black", None),
        path: seg("white", None),
        git: seg("bright-black", None),
        git_dirty: seg("yellow", None),
        duration: seg("bright-black", None),
        exit_ok: seg("green", None),
        exit_err: seg("red", None),
        user: seg("bright-black", None),
        prompt_char: seg("cyan", None),
        ..Default::default()
    }
}

fn rainbow_palette(style: PromptStyle) -> PromptColors {
    if matches!(style, PromptStyle::Powerline) {
        PromptColors {
            shell: seg("black", Some("33")),
            path: seg("black", Some("39")),
            git: seg("black", Some("220")),
            git_dirty: seg("black", Some("208")),
            duration: seg("black", Some("135")),
            exit_ok: seg("82", None),
            exit_err: seg("196", None),
            user: seg("black", Some("27")),
            prompt_char: seg("51", None),
            ..Default::default()
        }
    } else {
        PromptColors {
            shell: seg("33", None),
            path: seg("39", None),
            git: seg("220", None),
            git_dirty: seg("208", None),
            duration: seg("135", None),
            exit_ok: seg("82", None),
            exit_err: seg("196", None),
            user: seg("27", None),
            prompt_char: seg("51", None),
            ..Default::default()
        }
    }
}

fn nord_palette(style: PromptStyle) -> PromptColors {
    if matches!(style, PromptStyle::Powerline) {
        let bar = Some("236");
        PromptColors {
            shell: seg("#d8dee9", bar),
            path: seg("#88c0d0", bar),
            git: seg("#a3be8c", bar),
            git_dirty: seg("#ebcb8b", bar),
            duration: seg("#b48ead", bar),
            exit_ok: seg("#a3be8c", None),
            exit_err: seg("#bf616a", None),
            user: seg("#d8dee9", bar),
            prompt_char: seg("#88c0d0", None),
            ..Default::default()
        }
    } else {
        PromptColors {
            shell: seg("#81a1c1", None),
            path: seg("#88c0d0", None),
            git: seg("#a3be8c", None),
            git_dirty: seg("#ebcb8b", None),
            duration: seg("#b48ead", None),
            exit_ok: seg("#a3be8c", None),
            exit_err: seg("#bf616a", None),
            user: seg("#81a1c1", None),
            prompt_char: seg("#88c0d0", None),
            ..Default::default()
        }
    }
}

fn high_contrast_palette(style: PromptStyle) -> PromptColors {
    if matches!(style, PromptStyle::Powerline) {
        PromptColors {
            shell: seg("black", Some("white")),
            path: seg("black", Some("bright-cyan")),
            git: seg("black", Some("bright-yellow")),
            git_dirty: seg("black", Some("214")),
            duration: seg("black", Some("bright-magenta")),
            exit_ok: seg("bright-green", None),
            exit_err: seg("bright-red", None),
            user: seg("black", Some("white")),
            prompt_char: seg("bright-white", None),
            ..Default::default()
        }
    } else {
        PromptColors {
            shell: seg("bright-white", None),
            path: seg("bright-cyan", None),
            git: seg("bright-yellow", None),
            git_dirty: seg("214", None),
            duration: seg("bright-magenta", None),
            exit_ok: seg("bright-green", None),
            exit_err: seg("bright-red", None),
            user: seg("bright-white", None),
            prompt_char: seg("bright-white", None),
            ..Default::default()
        }
    }
}

fn seg(fg: &str, bg: Option<&str>) -> SegmentColors {
    SegmentColors {
        fg: Some(fg.to_string()),
        bg: bg.map(str::to_string),
    }
}

fn separator_glyph(sep: PromptSeparator, icons: bool) -> String {
    match (sep, icons) {
        (PromptSeparator::Arrow, true) => "\u{e0b0}".to_string(),
        (PromptSeparator::Chevron, true) => "\u{e0b2}".to_string(),
        (PromptSeparator::Round, true) => "\u{e0b4}".to_string(),
        (PromptSeparator::Bar, _) => "|".to_string(),
        (PromptSeparator::Space, _) => " ".to_string(),
        (_, false) => "›".to_string(),
    }
}

/// 色名・256 番号・#RRGGBB を ANSI エスケープに変換。
pub fn parse_color(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("default") || s.eq_ignore_ascii_case("none") {
        return String::new();
    }
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex(hex);
    }
    if let Ok(n) = s.parse::<u8>() {
        return format!("\x1b[38;5;{n}m");
    }
    if let Some(name) = named_color(s) {
        return format!("\x1b[{name}m");
    }
    String::new()
}

pub fn parse_bg_color(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("default") || s.eq_ignore_ascii_case("none") {
        return String::new();
    }
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex_bg(hex);
    }
    if let Ok(n) = s.parse::<u8>() {
        return format!("\x1b[48;5;{n}m");
    }
    if let Some(code) = named_color_code(s) {
        return format!("\x1b[{}m", code + 10);
    }
    String::new()
}

fn parse_hex(hex: &str) -> String {
    let (r, g, b) = match parse_rgb(hex) {
        Some(rgb) => rgb,
        None => return String::new(),
    };
    format!("\x1b[38;2;{r};{g};{b}m")
}

fn parse_hex_bg(hex: &str) -> String {
    let (r, g, b) = match parse_rgb(hex) {
        Some(rgb) => rgb,
        None => return String::new(),
    };
    format!("\x1b[48;2;{r};{g};{b}m")
}

fn parse_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim();
    let bytes = match hex.len() {
        6 => hex,
        3 => return expand_short_hex(hex),
        _ => return None,
    };
    let r = u8::from_str_radix(&bytes[0..2], 16).ok()?;
    let g = u8::from_str_radix(&bytes[2..4], 16).ok()?;
    let b = u8::from_str_radix(&bytes[4..6], 16).ok()?;
    Some((r, g, b))
}

fn expand_short_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let mut full = String::with_capacity(6);
    for c in hex.chars() {
        let _ = write!(full, "{c}{c}");
    }
    parse_rgb(&full)
}

fn named_color(name: &str) -> Option<u8> {
    named_color_code(name)
}

fn named_color_code(name: &str) -> Option<u8> {
    match name.to_ascii_lowercase().as_str() {
        "black" => Some(30),
        "red" => Some(31),
        "green" => Some(32),
        "yellow" => Some(33),
        "blue" => Some(34),
        "magenta" => Some(35),
        "cyan" => Some(36),
        "white" => Some(37),
        "gray" | "grey" | "bright-black" => Some(90),
        "bright-red" => Some(91),
        "bright-green" => Some(92),
        "bright-yellow" => Some(93),
        "bright-blue" => Some(94),
        "bright-magenta" => Some(95),
        "bright-cyan" => Some(96),
        "bright-white" => Some(97),
        _ => None,
    }
}

pub fn wrap_style(style: &ResolvedStyle, text: &str) -> String {
    let mut out = String::with_capacity(style.fg.len() + style.bg.len() + text.len() + 8);
    out.push_str(&style.bg);
    out.push_str(&style.fg);
    out.push_str(text);
    out.push_str(RESET);
    out
}

pub fn apply_color_key(colors: &mut PromptColors, section: &str, key: &str, value: &str) {
    let Some(name) = section.strip_prefix("prompt.colors.") else {
        return;
    };
    let target = match name {
        "shell" => &mut colors.shell,
        "path" => &mut colors.path,
        "git" => &mut colors.git,
        "git_dirty" => &mut colors.git_dirty,
        "time" => &mut colors.time,
        "battery" => &mut colors.battery,
        "battery_low" => &mut colors.battery_low,
        "k8s" => &mut colors.k8s,
        "duration" => &mut colors.duration,
        "exit_ok" => &mut colors.exit_ok,
        "exit_err" => &mut colors.exit_err,
        "user" => &mut colors.user,
        "char" | "prompt_char" => &mut colors.prompt_char,
        _ => return,
    };
    match key {
        "fg" | "foreground" => target.fg = Some(value.to_string()),
        "bg" | "background" => target.bg = Some(value.to_string()),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PromptSettings, PromptStyle};

    #[test]
    fn parse_named_and_hex_colors() {
        assert!(parse_color("cyan").contains("[36"));
        assert!(parse_color("#ff00aa").contains("[38;2;255;0;170"));
        assert!(parse_color("214").contains("[38;5;214"));
    }

    #[test]
    fn msh_preset_uses_soft_powerline_bar() {
        let settings = PromptSettings {
            preset: PromptPreset::Msh,
            style: PromptStyle::Powerline,
            ..PromptSettings::default()
        };
        let palette = resolve_palette(&settings, Theme::Default);
        assert!(palette.path.bg.contains("[48;5;236"));
    }
}
