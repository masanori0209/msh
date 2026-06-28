use crate::error::{MshError, Result};
use std::fs;
use std::path::PathBuf;

const DEFAULT_LIMIT: usize = 20;

pub fn run(args: &[String]) -> Result<()> {
    let mut pattern: Option<String> = None;
    let mut limit = DEFAULT_LIMIT;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-g" | "--grep" => {
                pattern = Some(
                    args.get(i + 1)
                        .ok_or_else(|| MshError::ParseError("history: pattern required".into()))?
                        .clone(),
                );
                i += 2;
            }
            "-n" => {
                limit = args
                    .get(i + 1)
                    .ok_or_else(|| MshError::ParseError("history: count required".into()))?
                    .parse()
                    .map_err(|_| MshError::ParseError("history: invalid count".into()))?;
                i += 2;
            }
            other => {
                return Err(MshError::ParseError(format!(
                    "history: unknown option '{other}'"
                )));
            }
        }
    }

    let entries = read_history_entries()?;
    let filtered: Vec<_> = entries
        .into_iter()
        .rev()
        .filter(|entry| match &pattern {
            Some(pat) => entry
                .to_ascii_lowercase()
                .contains(&pat.to_ascii_lowercase()),
            None => true,
        })
        .take(limit)
        .collect();

    for entry in filtered.into_iter().rev() {
        println!("{entry}");
    }

    Ok(())
}

fn read_history_entries() -> Result<Vec<String>> {
    let Some(home) = std::env::var("HOME").ok() else {
        return Ok(Vec::new());
    };
    let path = PathBuf::from(home).join(".msh_history");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    Ok(content.lines().map(str::to_string).collect())
}
