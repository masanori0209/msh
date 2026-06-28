use crate::error::{MshError, Result};
use std::env;
use std::path::Path;

pub fn run(args: &[String]) -> Result<()> {
    let target = args.first().map(String::as_str).unwrap_or("~");

    let path = if target == "~" {
        env::var("HOME").map_err(|_| MshError::DirNotFound("~ (HOME not set)".into()))?
    } else {
        target.to_string()
    };

    let path = Path::new(&path);
    if !path.exists() {
        return Err(MshError::DirNotFound(path.display().to_string()));
    }

    env::set_current_dir(path)?;
    Ok(())
}
