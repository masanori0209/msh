use crate::config::{PromptSettings, PromptStyle, Theme, TimeFormat};
use crate::prompt_color::{resolve_palette, wrap_style, ResolvedPalette, ResolvedStyle};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

const RESET: &str = "\x1b[0m";
const TIME_TTL: Duration = Duration::from_secs(15);
const BATTERY_TTL: Duration = Duration::from_secs(60);
const K8S_TTL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    pub branch: String,
    pub dirty: bool,
    pub staged: bool,
    pub untracked: bool,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryStatus {
    pub percent: u8,
    pub charging: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct K8sStatus {
    pub context: String,
    pub namespace: String,
}

pub struct Cache {
    cwd: PathBuf,
    cwd_display: String,
    git: Option<GitStatus>,
    git_stamp: Option<SystemTime>,
    time_label: String,
    time_updated: Option<Instant>,
    battery: Option<BatteryStatus>,
    battery_updated: Option<Instant>,
    k8s: Option<K8sStatus>,
    k8s_updated: Option<Instant>,
    k8s_env_key: String,
}

pub struct RenderContext<'a> {
    pub last_status: i32,
    pub last_duration: Duration,
    pub cache: &'a mut Cache,
    pub settings: &'a PromptSettings,
    pub theme: Theme,
}

impl Cache {
    pub fn new() -> Self {
        Self {
            cwd: PathBuf::new(),
            cwd_display: String::new(),
            git: None,
            git_stamp: None,
            time_label: String::new(),
            time_updated: None,
            battery: None,
            battery_updated: None,
            k8s: None,
            k8s_updated: None,
            k8s_env_key: String::new(),
        }
    }

    pub fn refresh(&mut self, settings: &PromptSettings) {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("?"));
        if self.cwd != cwd {
            self.cwd = cwd.clone();
            self.cwd_display = compact_path(&cwd);
            self.git = None;
            self.git_stamp = None;
        }

        if settings.show_git && git_repo_changed(self.git_stamp) {
            self.git = read_git_status();
            self.git_stamp = git_repo_stamp();
        }

        if settings.show_time && stale(self.time_updated, TIME_TTL) {
            self.time_label = read_time_label(settings.time_format);
            self.time_updated = Some(Instant::now());
        }

        if settings.show_battery && stale(self.battery_updated, BATTERY_TTL) {
            self.battery = read_battery_status();
            self.battery_updated = Some(Instant::now());
        }

        if settings.show_k8s {
            let env_key = kube_config_fingerprint();
            if env_key != self.k8s_env_key || stale(self.k8s_updated, K8S_TTL) {
                self.k8s = read_k8s_status(settings.k8s_show_namespace);
                self.k8s_env_key = env_key;
                self.k8s_updated = Some(Instant::now());
            }
        }
    }
}

fn stale(updated: Option<Instant>, ttl: Duration) -> bool {
    updated.is_none_or(|t| t.elapsed() >= ttl)
}

pub fn render(ctx: RenderContext<'_>) -> String {
    ctx.cache.refresh(ctx.settings);
    let palette = resolve_palette(ctx.settings, ctx.theme);
    let mut prompt = match ctx.settings.style {
        PromptStyle::Powerline => render_powerline(&ctx, &palette),
        PromptStyle::Minimal => render_plain(&ctx, &palette, true),
        PromptStyle::Default => render_plain(&ctx, &palette, false),
    };
    if ctx.settings.newline {
        prompt.insert(0, '\n');
    }
    prompt
}

fn render_plain(ctx: &RenderContext<'_>, palette: &ResolvedPalette, minimal: bool) -> String {
    let mut prompt = String::new();
    let mut started = false;

    if ctx.settings.show_user_host {
        prompt.push_str(&wrap_style(
            &palette.user,
            &user_host_label(ctx.settings.icons),
        ));
        started = true;
    }

    if !minimal && ctx.settings.show_shell {
        push_plain_sep(&mut prompt, palette, minimal, &mut started);
        prompt.push_str(&wrap_style(
            &palette.shell,
            &shell_label(ctx.settings.icons),
        ));
    }

    push_plain_sep(&mut prompt, palette, minimal, &mut started);
    prompt.push_str(&wrap_style(
        &palette.path,
        &path_label(&ctx.cache.cwd_display, ctx.settings.icons),
    ));

    append_k8s_time_battery_segments(
        ctx,
        palette,
        SegmentSink::Plain {
            prompt: &mut prompt,
            palette,
            minimal,
            started: &mut started,
        },
    );

    if ctx.settings.show_git {
        if let Some(git) = &ctx.cache.git {
            let style = git_style(git, palette);
            push_plain_sep(&mut prompt, palette, minimal, &mut started);
            prompt.push_str(&wrap_style(&style, &format_git(git, ctx.settings)));
        }
    }

    if let Some(label) = duration_label_for(ctx) {
        push_plain_sep(&mut prompt, palette, minimal, &mut started);
        prompt.push_str(&wrap_style(
            &palette.duration,
            &duration_label(&label, ctx.settings.icons),
        ));
    }

    if should_show_exit(ctx.last_status, ctx.settings) {
        let style = if ctx.last_status == 0 {
            &palette.exit_ok
        } else {
            &palette.exit_err
        };
        push_plain_sep(&mut prompt, palette, minimal, &mut started);
        prompt.push_str(&wrap_style(
            style,
            &exit_label(ctx.last_status, ctx.settings.icons),
        ));
    }

    push_plain_sep(&mut prompt, palette, minimal, &mut started);
    prompt.push_str(&wrap_style(
        &palette.prompt_char,
        prompt_glyph(ctx.settings.icons),
    ));
    prompt.push(' ');
    prompt
}

fn push_plain_sep(out: &mut String, palette: &ResolvedPalette, minimal: bool, started: &mut bool) {
    if !*started {
        *started = true;
        return;
    }
    if minimal {
        out.push(' ');
    } else {
        out.push_str(&palette.dim);
        out.push_str(" · ");
        out.push_str(RESET);
    }
}

fn render_powerline(ctx: &RenderContext<'_>, palette: &ResolvedPalette) -> String {
    let mut prompt = String::new();
    let sep = &palette.separator;
    let mut first = true;
    let mut prev_bg: Option<String> = None;

    if ctx.settings.show_user_host {
        push_powerline(
            &mut prompt,
            &user_host_label(ctx.settings.icons),
            &palette.user,
            sep,
            &mut first,
            &mut prev_bg,
        );
    }

    if ctx.settings.show_shell {
        push_powerline(
            &mut prompt,
            &shell_label(ctx.settings.icons),
            &palette.shell,
            sep,
            &mut first,
            &mut prev_bg,
        );
    }

    push_powerline(
        &mut prompt,
        &path_label(&ctx.cache.cwd_display, ctx.settings.icons),
        &palette.path,
        sep,
        &mut first,
        &mut prev_bg,
    );

    append_k8s_time_battery_segments(
        ctx,
        palette,
        SegmentSink::Powerline {
            prompt: &mut prompt,
            sep,
            first: &mut first,
            prev_bg: &mut prev_bg,
        },
    );

    if ctx.settings.show_git {
        if let Some(git) = &ctx.cache.git {
            let style = git_style(git, palette);
            push_powerline(
                &mut prompt,
                &format_git(git, ctx.settings),
                &style,
                sep,
                &mut first,
                &mut prev_bg,
            );
        }
    }

    if let Some(label) = duration_label_for(ctx) {
        push_powerline(
            &mut prompt,
            &duration_label(&label, ctx.settings.icons),
            &palette.duration,
            sep,
            &mut first,
            &mut prev_bg,
        );
    }

    if should_show_exit(ctx.last_status, ctx.settings) {
        let style = if ctx.last_status == 0 {
            &palette.exit_ok
        } else {
            &palette.exit_err
        };
        push_powerline(
            &mut prompt,
            &exit_label(ctx.last_status, ctx.settings.icons),
            style,
            sep,
            &mut first,
            &mut prev_bg,
        );
    }

    if prev_bg.is_some() {
        prompt.push_str(RESET);
    }
    prompt.push_str(&wrap_style(
        &palette.prompt_char,
        prompt_glyph(ctx.settings.icons),
    ));
    prompt.push(' ');
    prompt
}

fn push_powerline(
    out: &mut String,
    text: &str,
    style: &ResolvedStyle,
    sep: &str,
    first: &mut bool,
    prev_bg: &mut Option<String>,
) {
    let padded = format!(" {text} ");
    let same_bar = prev_bg
        .as_ref()
        .is_some_and(|bg| !bg.is_empty() && bg == &style.bg);

    if *first {
        if !style.bg.is_empty() {
            out.push_str(&style.bg);
        }
        out.push_str(&style.fg);
        out.push_str(&padded);
    } else if same_bar {
        out.push_str(RESET);
        out.push_str(&style.bg);
        out.push_str(&style.fg);
        out.push_str(" ·");
        out.push_str(&padded);
    } else {
        out.push_str(RESET);
        out.push_str(&style.fg);
        out.push_str(sep);
        if !style.bg.is_empty() {
            out.push_str(&style.bg);
        }
        out.push_str(&style.fg);
        out.push_str(&padded);
        out.push_str(RESET);
        out.push_str(&style.fg);
        out.push_str(sep);
        out.push_str(RESET);
        *prev_bg = Some(style.bg.clone());
        *first = false;
        return;
    }

    *prev_bg = Some(style.bg.clone());
    *first = false;
}

enum SegmentSink<'a> {
    Plain {
        prompt: &'a mut String,
        palette: &'a ResolvedPalette,
        minimal: bool,
        started: &'a mut bool,
    },
    Powerline {
        prompt: &'a mut String,
        sep: &'a str,
        first: &'a mut bool,
        prev_bg: &'a mut Option<String>,
    },
}

fn append_k8s_time_battery_segments(
    ctx: &RenderContext<'_>,
    palette: &ResolvedPalette,
    mut sink: SegmentSink<'_>,
) {
    if ctx.settings.show_k8s {
        if let Some(k8s) = &ctx.cache.k8s {
            push_segment(&mut sink, &palette.k8s, &format_k8s(k8s, ctx.settings));
        }
    }

    if ctx.settings.show_time && !ctx.cache.time_label.is_empty() {
        push_segment(
            &mut sink,
            &palette.time,
            &time_label(&ctx.cache.time_label, ctx.settings.icons),
        );
    }

    if ctx.settings.show_battery {
        if let Some(battery) = &ctx.cache.battery {
            if should_show_battery(battery, ctx.settings) {
                let style = battery_style(battery, palette);
                push_segment(
                    &mut sink,
                    &style,
                    &format_battery(battery, ctx.settings.icons),
                );
            }
        }
    }
}

fn push_segment(sink: &mut SegmentSink<'_>, style: &ResolvedStyle, text: &str) {
    match sink {
        SegmentSink::Plain {
            prompt,
            palette,
            minimal,
            started,
        } => {
            push_plain_sep(prompt, palette, *minimal, started);
            prompt.push_str(&wrap_style(style, text));
        }
        SegmentSink::Powerline {
            prompt,
            sep,
            first,
            prev_bg,
        } => {
            push_powerline(prompt, text, style, sep, first, prev_bg);
        }
    }
}

fn battery_style(battery: &BatteryStatus, palette: &ResolvedPalette) -> ResolvedStyle {
    if battery.percent <= 20 && !battery.charging {
        palette.battery_low.clone()
    } else {
        palette.battery.clone()
    }
}

fn should_show_battery(battery: &BatteryStatus, settings: &PromptSettings) -> bool {
    !(settings.battery_hide_full && battery.percent >= 100 && battery.charging)
}

fn git_style(git: &GitStatus, palette: &ResolvedPalette) -> ResolvedStyle {
    if git.dirty || git.staged || git.untracked {
        palette.git_dirty.clone()
    } else {
        palette.git.clone()
    }
}

fn duration_label_for(ctx: &RenderContext<'_>) -> Option<String> {
    if !ctx.settings.show_duration {
        return None;
    }
    let min_ms = if ctx.settings.transient_duration && ctx.last_status == 0 {
        ctx.settings.duration_min_ms
    } else if ctx.last_status != 0 {
        0
    } else {
        ctx.settings.duration_min_ms
    };
    format_duration(ctx.last_duration, min_ms)
}

fn prompt_glyph(icons: bool) -> &'static str {
    if icons {
        "\u{f054}"
    } else {
        "›"
    }
}

fn should_show_exit(status: i32, settings: &PromptSettings) -> bool {
    settings.show_exit_on_success || status != 0
}

fn user_host_label(icons: bool) -> String {
    let user = env::var("USER").unwrap_or_else(|_| "?".into());
    let host = env::var("HOSTNAME")
        .or_else(|_| env::var("HOST"))
        .unwrap_or_else(|_| "localhost".into());
    let short_host = host.split('.').next().unwrap_or(&host);
    if icons {
        format!("\u{f007} {user}@{short_host}")
    } else {
        format!("{user}@{short_host}")
    }
}

fn shell_label(_icons: bool) -> String {
    "msh".to_string()
}

fn path_label(path: &str, icons: bool) -> String {
    if icons {
        format!("\u{f07b} {path}")
    } else {
        path.to_string()
    }
}

fn duration_label(label: &str, icons: bool) -> String {
    if icons {
        format!("\u{f017} {label}")
    } else {
        label.to_string()
    }
}

fn exit_label(status: i32, icons: bool) -> String {
    if icons {
        if status == 0 {
            format!("\u{f00c} {status}")
        } else {
            format!("\u{f00d} {status}")
        }
    } else {
        status.to_string()
    }
}

pub fn format_git(git: &GitStatus, settings: &PromptSettings) -> String {
    let mut out = String::new();
    out.push_str(&git.branch);
    if settings.show_git_dirty {
        if git.staged {
            out.push('+');
        }
        if git.dirty {
            out.push('*');
        }
        if git.untracked && !git.dirty && !git.staged {
            out.push('?');
        }
    }
    if settings.show_git_ahead_behind && (git.ahead > 0 || git.behind > 0) {
        out.push(' ');
        out.push('↑');
        out.push_str(&git.ahead.to_string());
        out.push(' ');
        out.push('↓');
        out.push_str(&git.behind.to_string());
    }
    out
}

pub fn format_duration(duration: Duration, min_ms: u64) -> Option<String> {
    let ms = duration.as_millis();
    if ms < min_ms as u128 {
        return None;
    }
    if ms < 1_000 {
        Some(format!("{ms}ms"))
    } else if ms < 60_000 {
        Some(format!("{:.1}s", ms as f64 / 1000.0))
    } else {
        let secs = duration.as_secs();
        Some(format!("{}m{:02}s", secs / 60, secs % 60))
    }
}

fn compact_path(path: &Path) -> String {
    if let Ok(home) = env::var("HOME") {
        let home = Path::new(&home);
        if path.starts_with(home) {
            let rest = path.strip_prefix(home).unwrap_or(path);
            if rest.as_os_str().is_empty() {
                return "~".into();
            }
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

fn git_repo_stamp() -> Option<SystemTime> {
    let head = fs::metadata(".git/HEAD").ok()?.modified().ok();
    let index = fs::metadata(".git/index")
        .ok()
        .and_then(|m| m.modified().ok());
    match (head, index) {
        (Some(h), Some(i)) => Some(if h > i { h } else { i }),
        (Some(h), None) => Some(h),
        (None, Some(i)) => Some(i),
        (None, None) => None,
    }
}

fn git_repo_changed(previous: Option<SystemTime>) -> bool {
    git_repo_stamp().as_ref() != previous.as_ref()
}

fn read_git_status() -> Option<GitStatus> {
    let output = Command::new("git")
        .args(["status", "-sb", "--porcelain"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_git_status_output(&text)
}

pub fn parse_git_status_output(text: &str) -> Option<GitStatus> {
    let mut lines = text.lines();
    let header = lines.next()?.trim();
    if !header.starts_with("## ") {
        return None;
    }
    let rest = header.strip_prefix("## ")?;
    let (branch_part, meta) = rest.split_once(' ').unwrap_or((rest, ""));
    let (branch, upstream) = branch_part
        .split_once("...")
        .map(|(b, u)| (b.to_string(), Some(u.to_string())))
        .unwrap_or((branch_part.to_string(), None));
    let _ = upstream;
    let (ahead, behind) = parse_ahead_behind(meta);

    let mut dirty = false;
    let mut staged = false;
    let mut untracked = false;
    for line in lines {
        if line.len() < 2 {
            continue;
        }
        let x = line.as_bytes()[0];
        let y = line.as_bytes()[1];
        if x == b'?' && y == b'?' {
            untracked = true;
            continue;
        }
        if x != b' ' && x != b'?' {
            staged = true;
        }
        if y != b' ' && y != b'?' {
            dirty = true;
        }
    }

    Some(GitStatus {
        branch,
        dirty,
        staged,
        untracked,
        ahead,
        behind,
    })
}

fn parse_ahead_behind(meta: &str) -> (u32, u32) {
    let mut ahead = 0;
    let mut behind = 0;
    if let Some(inner) = meta.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        for part in inner.split(',') {
            let part = part.trim();
            if let Some(n) = part.strip_prefix("ahead ") {
                ahead = n.trim().parse().unwrap_or(0);
            } else if let Some(n) = part.strip_prefix("behind ") {
                behind = n.trim().parse().unwrap_or(0);
            }
        }
    }
    (ahead, behind)
}

fn read_time_label(format: TimeFormat) -> String {
    let fmt = match format {
        TimeFormat::Hm24 => "+%H:%M",
        TimeFormat::Hm12 => "+%I:%M %p",
        TimeFormat::Hms24 => "+%H:%M:%S",
    };
    Command::new("date")
        .arg(fmt)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "??:??".into())
}

fn read_battery_status() -> Option<BatteryStatus> {
    if cfg!(target_os = "macos") {
        read_battery_macos()
    } else if cfg!(unix) {
        read_battery_linux()
    } else {
        None
    }
}

fn read_battery_macos() -> Option<BatteryStatus> {
    let output = Command::new("pmset").args(["-g", "batt"]).output().ok()?;
    parse_pmset_output(&String::from_utf8_lossy(&output.stdout))
}

fn read_battery_linux() -> Option<BatteryStatus> {
    let root = Path::new("/sys/class/power_supply");
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("BAT") {
            continue;
        }
        let cap_path = entry.path().join("capacity");
        let status_path = entry.path().join("status");
        let percent: u8 = fs::read_to_string(cap_path).ok()?.trim().parse().ok()?;
        let status = fs::read_to_string(status_path).ok()?;
        let charging = status.trim().eq_ignore_ascii_case("Charging")
            || status.trim().eq_ignore_ascii_case("Full");
        return Some(BatteryStatus { percent, charging });
    }
    None
}

pub fn parse_pmset_output(text: &str) -> Option<BatteryStatus> {
    let line = text.lines().find(|l| l.contains('%'))?;
    let percent: u8 = line
        .split('%')
        .next()?
        .split_whitespace()
        .next_back()?
        .parse()
        .ok()?;
    let lower = line.to_ascii_lowercase();
    let charging =
        !lower.contains("discharging") && (lower.contains("charging") || lower.contains("charged"));
    Some(BatteryStatus { percent, charging })
}

fn read_k8s_status(show_namespace: bool) -> Option<K8sStatus> {
    let ctx_out = Command::new("kubectl")
        .args(["config", "current-context"])
        .output()
        .ok()?;
    if !ctx_out.status.success() {
        return None;
    }
    let context = String::from_utf8_lossy(&ctx_out.stdout).trim().to_string();
    if context.is_empty() {
        return None;
    }

    let namespace = if show_namespace {
        Command::new("kubectl")
            .args(["config", "view", "--minify", "-o", "jsonpath={..namespace}"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "default".into())
    } else {
        String::new()
    };

    Some(K8sStatus { context, namespace })
}

fn kube_config_fingerprint() -> String {
    env::var("KUBECONFIG").unwrap_or_else(|_| {
        env::var("HOME")
            .map(|h| format!("{h}/.kube/config"))
            .unwrap_or_default()
    })
}

pub fn format_k8s(k8s: &K8sStatus, settings: &PromptSettings) -> String {
    let mut out = if settings.icons {
        format!("\u{f233} {}", k8s.context)
    } else {
        k8s.context.clone()
    };
    if settings.k8s_show_namespace && !k8s.namespace.is_empty() {
        out.push('/');
        out.push_str(&k8s.namespace);
    }
    out
}

pub fn format_battery(battery: &BatteryStatus, icons: bool) -> String {
    if icons {
        let icon = if battery.charging {
            "\u{f0e7}"
        } else if battery.percent <= 20 {
            "\u{f244}"
        } else if battery.percent <= 50 {
            "\u{f243}"
        } else if battery.percent <= 80 {
            "\u{f242}"
        } else {
            "\u{f240}"
        };
        format!("{icon} {}%", battery.percent)
    } else {
        let mark = if battery.charging { "+" } else { "" };
        format!("{mark}{}%", battery.percent)
    }
}

fn time_label(label: &str, icons: bool) -> String {
    if icons {
        format!("\u{f017} {label}")
    } else {
        label.to_string()
    }
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PromptSettings;

    #[test]
    fn compact_home_path() {
        if let Ok(home) = std::env::var("HOME") {
            let path = PathBuf::from(&home).join("dev");
            assert_eq!(compact_path(&path), "~/dev");
        }
    }

    #[test]
    fn format_duration_units() {
        assert_eq!(
            format_duration(Duration::from_millis(42), 0).as_deref(),
            Some("42ms")
        );
        assert_eq!(format_duration(Duration::from_millis(42), 100), None);
        assert_eq!(
            format_duration(Duration::from_millis(1500), 0).as_deref(),
            Some("1.5s")
        );
        assert_eq!(
            format_duration(Duration::from_secs(125), 0).as_deref(),
            Some("2m05s")
        );
    }

    #[test]
    fn parse_git_status_with_dirty_and_ahead() {
        let text = "## main...origin/main [ahead 2, behind 1]\n M src/prompt.rs\n?? new.txt\n";
        let git = parse_git_status_output(text).unwrap();
        assert_eq!(git.branch, "main");
        assert!(git.dirty);
        assert!(git.untracked);
        assert_eq!(git.ahead, 2);
        assert_eq!(git.behind, 1);
        let settings = PromptSettings::default();
        assert!(format_git(&git, &settings).contains("main*"));
        assert!(format_git(&git, &settings).contains('↑'));
    }

    #[test]
    fn hides_exit_zero_by_default() {
        let settings = PromptSettings::default();
        assert!(!should_show_exit(0, &settings));
        assert!(should_show_exit(1, &settings));
    }

    #[test]
    fn render_hides_fast_success_duration_by_default() {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let display = compact_path(&cwd);
        let mut cache = Cache::new();
        cache.cwd = cwd;
        cache.cwd_display = display;
        let settings = PromptSettings {
            show_git: false,
            show_duration: true,
            transient_duration: true,
            duration_min_ms: 50,
            ..PromptSettings::default()
        };
        let out = render(RenderContext {
            last_status: 0,
            last_duration: Duration::from_millis(10),
            cache: &mut cache,
            settings: &settings,
            theme: Theme::Default,
        });
        assert!(!out.contains("10ms"));
    }

    #[test]
    fn render_includes_duration_and_path() {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let display = compact_path(&cwd);
        let mut cache = Cache::new();
        cache.cwd = cwd;
        cache.cwd_display = display.clone();
        let settings = PromptSettings {
            show_git: false,
            show_duration: true,
            transient_duration: false,
            duration_min_ms: 0,
            ..PromptSettings::default()
        };
        let out = render(RenderContext {
            last_status: 0,
            last_duration: Duration::from_millis(250),
            cache: &mut cache,
            settings: &settings,
            theme: Theme::Default,
        });
        assert!(out.contains("250ms"));
        assert!(out.contains(&display));
    }

    #[test]
    fn parse_pmset_battery_line() {
        let text = "InternalBattery-0\t67%; discharging; 1:23 remaining\n";
        let b = parse_pmset_output(text).unwrap();
        assert_eq!(b.percent, 67);
        assert!(!b.charging);
    }

    #[test]
    fn format_k8s_with_namespace() {
        let k8s = K8sStatus {
            context: "prod".into(),
            namespace: "api".into(),
        };
        let settings = PromptSettings {
            show_k8s: true,
            k8s_show_namespace: true,
            icons: false,
            ..PromptSettings::default()
        };
        assert_eq!(format_k8s(&k8s, &settings), "prod/api");
    }

    #[test]
    fn battery_hide_full_when_charged() {
        let battery = BatteryStatus {
            percent: 100,
            charging: true,
        };
        let settings = PromptSettings::default();
        assert!(!should_show_battery(&battery, &settings));
    }
}
