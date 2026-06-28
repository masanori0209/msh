use crate::error::{MshError, Result};
use std::fs;

pub fn read_lines(path: &str) -> Result<Vec<String>> {
    fs::read_to_string(path)
        .map(|content| content.lines().map(str::to_string).collect())
        .map_err(|e| MshError::ParseError(format!("source: cannot read {path}: {e}")))
}
