use crate::error::{MshError, Result};
use std::collections::HashMap;

pub fn run(args: &[String], aliases: &mut HashMap<String, String>) -> Result<()> {
    if args.is_empty() {
        for (name, value) in aliases {
            println!("alias {name}='{value}'");
        }
        return Ok(());
    }

    for arg in args {
        let Some((name, value)) = arg.split_once('=') else {
            if let Some(value) = aliases.get(arg) {
                println!("alias {arg}='{value}'");
                continue;
            }
            return Err(MshError::AliasError(format!("alias '{arg}' not found")));
        };

        if name.is_empty() {
            return Err(MshError::AliasError("alias name is empty".into()));
        }

        aliases.insert(name.to_string(), value.to_string());
    }

    Ok(())
}
