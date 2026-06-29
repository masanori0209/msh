use crate::error::Result;
use std::env;
use std::path::PathBuf;

pub fn run(args: &[String]) -> Result<i32> {
    let home = home_dir();
    match args.first().map(String::as_str) {
        None | Some("list") => {
            println!("{}", crate::plugin::list_text(&home));
            Ok(0)
        }
        Some("run") => {
            let name = args.get(1).ok_or_else(|| {
                crate::error::MshError::ScriptError(
                    "plugin run: usage: plugin run <name> [export]".into(),
                )
            })?;
            let invoke = args.get(2).map(String::as_str).unwrap_or("greet");
            let out = crate::plugin::run_plugin(&home, name, invoke)?;
            println!("{out}");
            Ok(0)
        }
        Some("help") | Some("-h") | Some("--help") => {
            print_help();
            Ok(0)
        }
        Some(other) => {
            print_help();
            Err(crate::error::MshError::ScriptError(format!(
                "plugin: unknown subcommand '{other}'"
            )))
        }
    }
}

fn print_help() {
    println!("usage: plugin list");
    println!("       plugin run <name> [wasm-export]");
    println!();
    println!("WASM plugins live in ~/.config/msh/plugins/<name>/plugin.toml + .wasm");
    println!("Requires wasmtime on PATH (https://wasmtime.dev/)");
}

fn home_dir() -> PathBuf {
    env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}
