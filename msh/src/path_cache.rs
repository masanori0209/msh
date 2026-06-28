use crate::builtins;
use crate::descriptions;
use crate::hints;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct CompletionCandidate {
    pub replacement: String,
    pub display: String,
}

struct PathCache {
    commands: Vec<String>,
    path_fingerprint: u64,
    loaded_at: Instant,
}

static PATH_CACHE: OnceLock<RwLock<PathCache>> = OnceLock::new();

pub fn command_list(aliases: &HashMap<String, String>) -> Vec<String> {
    let mut commands = path_commands();
    commands.extend(builtins::NAMES.iter().map(|name| (*name).to_string()));
    commands.extend(aliases.keys().cloned());
    commands.sort_unstable();
    commands.dedup();
    commands
}

pub fn complete_commands(
    prefix: &str,
    aliases: &HashMap<String, String>,
    fuzzy: bool,
) -> Vec<CompletionCandidate> {
    let commands = command_list(aliases);
    let prefix_matches: Vec<_> = if prefix.is_empty() {
        commands.clone()
    } else {
        let start = commands.partition_point(|cmd| cmd.as_str() < prefix);
        commands
            .iter()
            .skip(start)
            .take_while(|cmd| cmd.starts_with(prefix))
            .cloned()
            .collect()
    };

    let mut results: Vec<CompletionCandidate> =
        prefix_matches.iter().map(|cmd| to_candidate(cmd)).collect();

    if results.is_empty() && fuzzy && prefix.len() >= 2 {
        let mut scored: Vec<(usize, String)> = commands
            .into_iter()
            .map(|cmd| (hints::levenshtein(prefix, &cmd), cmd))
            .filter(|(distance, _)| *distance <= 2)
            .collect();
        scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        results = scored
            .into_iter()
            .take(8)
            .map(|(_, cmd)| to_candidate(&cmd))
            .collect();
    }

    results
}

fn to_candidate(name: &str) -> CompletionCandidate {
    let description = descriptions::describe(name).unwrap_or("");
    let display = if description.is_empty() {
        name.to_string()
    } else {
        format!("{name:<12} {description}")
    };
    CompletionCandidate {
        replacement: name.to_string(),
        display,
    }
}

fn path_commands() -> Vec<String> {
    let cache = PATH_CACHE.get_or_init(|| {
        RwLock::new(PathCache {
            commands: Vec::new(),
            path_fingerprint: 0,
            loaded_at: Instant::now() - CACHE_TTL,
        })
    });

    let path_var = std::env::var("PATH").unwrap_or_default();
    let fingerprint = hash_path(&path_var);
    {
        let read = cache.read().expect("path cache poisoned");
        if read.path_fingerprint == fingerprint
            && read.loaded_at.elapsed() < CACHE_TTL
            && !read.commands.is_empty()
        {
            return read.commands.clone();
        }
    }

    let commands = scan_path(&path_var);
    let mut write = cache.write().expect("path cache poisoned");
    write.commands = commands.clone();
    write.path_fingerprint = fingerprint;
    write.loaded_at = Instant::now();
    commands
}

fn scan_path(path_var: &str) -> Vec<String> {
    let mut commands = Vec::new();
    for dir in path_var.split(':').filter(|entry| !entry.is_empty()) {
        let Ok(entries) = fs::read_dir(Path::new(dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
                    commands.push(name.to_string());
                }
            }
        }
    }
    commands.sort_unstable();
    commands.dedup();
    commands
}

fn hash_path(path_var: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path_var.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn complete_commands_prefix() {
        let aliases = HashMap::new();
        let matches = complete_commands("ec", &aliases, false);
        assert!(matches.iter().any(|cmd| cmd.replacement == "echo"));
    }

    #[test]
    fn fuzzy_complete() {
        let aliases = HashMap::new();
        let matches = complete_commands("ecoh", &aliases, true);
        assert!(matches.iter().any(|cmd| cmd.replacement == "echo"));
    }
}
