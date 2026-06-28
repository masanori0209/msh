use crate::error::{MshError, Result};
use std::env;

pub fn run(args: &[String]) -> Result<()> {
    if args.is_empty() {
        for (key, value) in env::vars() {
            println!("export {}={}", key, value);
        }
        return Ok(());
    }

    for arg in args {
        let Some((key, value)) = arg.split_once('=') else {
            return Err(MshError::InvalidExport(format!(
                "expected NAME=VALUE, got '{arg}'"
            )));
        };

        if key.is_empty() {
            return Err(MshError::InvalidExport(format!(
                "variable name is empty in '{arg}'"
            )));
        }

        env::set_var(key, value);
    }

    Ok(())
}
