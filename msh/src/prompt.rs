use crate::config::Theme;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const CYAN: &str = "\x1b[36m";

pub struct Cache {
    cwd: PathBuf,
    cwd_display: String,
    branch: Option<String>,
    head_mtime: Option<SystemTime>,
}

impl Cache {
    pub fn new() -> Self {
        Self {
            cwd: PathBuf::new(),
            cwd_display: String::new(),
            branch: None,
            head_mtime: None,
        }
    }

    pub fn refresh(&mut self) {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("?"));
        if self.cwd != cwd {
            self.cwd = cwd.clone();
            self.cwd_display = compact_path(&cwd);
            self.branch = None;
            self.head_mtime = None;
        }

        if self.branch.is_none() || git_head_changed(&self.head_mtime) {
            self.branch = read_git_branch(&mut self.head_mtime);
        }
    }
}

pub fn render(last_status: i32, cache: &mut Cache, theme: Theme) -> String {
    cache.refresh();

    let (brand, path, ok, err, dim) = theme_colors(theme);
    let mut prompt = String::new();

    if let Some(branch) = &cache.branch {
        prompt.push_str(YELLOW);
        prompt.push('(');
        prompt.push_str(branch);
        prompt.push(')');
        prompt.push_str(RESET);
        prompt.push(' ');
    }

    prompt.push_str(BOLD);
    prompt.push_str(brand);
    prompt.push_str("msh");
    prompt.push_str(RESET);

    prompt.push_str(dim);
    prompt.push(':');
    prompt.push_str(RESET);

    prompt.push_str(path);
    prompt.push_str(&cache.cwd_display);
    prompt.push_str(RESET);

    let status_color = if last_status == 0 { ok } else { err };
    prompt.push(' ');
    prompt.push_str(status_color);
    prompt.push_str(&last_status.to_string());
    prompt.push_str(RESET);

    prompt.push_str(dim);
    prompt.push_str(" ❯ ");
    prompt.push_str(RESET);

    prompt
}

fn theme_colors(
    theme: Theme,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    match theme {
        Theme::Default => (BLUE, CYAN, GREEN, RED, DIM),
        Theme::Minimal => (DIM, DIM, GREEN, RED, DIM),
    }
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
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

fn read_git_branch(head_mtime: &mut Option<SystemTime>) -> Option<String> {
    let head_path = Path::new(".git/HEAD");
    let metadata = fs::metadata(head_path).ok()?;
    *head_mtime = metadata.modified().ok();
    let head = fs::read_to_string(head_path).ok()?;
    let head = head.trim();
    if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
        Some(branch.to_string())
    } else {
        Some(head.chars().take(7).collect())
    }
}

fn git_head_changed(previous: &Option<SystemTime>) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    let Ok(metadata) = fs::metadata(".git/HEAD") else {
        return true;
    };
    metadata.modified().ok().as_ref() != Some(previous)
}

#[cfg(test)]
mod tests {
    use super::compact_path;
    use std::path::PathBuf;

    #[test]
    fn compact_home_path() {
        if let Ok(home) = std::env::var("HOME") {
            let path = PathBuf::from(&home).join("dev");
            assert_eq!(compact_path(&path), "~/dev");
        }
    }
}
