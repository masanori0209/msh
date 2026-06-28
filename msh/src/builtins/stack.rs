use crate::error::{MshError, Result};
use std::env;
use std::path::Path;

pub fn push(stack: &mut Vec<String>, args: &[String]) -> Result<()> {
    let current = env::current_dir()
        .map_err(MshError::Io)?
        .to_string_lossy()
        .into_owned();
    stack.push(current);

    let target = args.first().map(String::as_str).unwrap_or("~");
    cd_to(target)?;
    Ok(())
}

pub fn pop(stack: &mut Vec<String>) -> Result<()> {
    let Some(path) = stack.pop() else {
        return Err(MshError::ScriptError("directory stack empty".into()));
    };
    env::set_current_dir(&path)?;
    Ok(())
}

pub fn dirs(stack: &[String]) -> Result<()> {
    let cwd = env::current_dir()
        .map_err(MshError::Io)?
        .display()
        .to_string();
    for (index, path) in stack.iter().enumerate() {
        println!("{index}\t{path}");
    }
    println!("*\t{cwd}");
    Ok(())
}

fn cd_to(target: &str) -> Result<()> {
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
