pub mod alias;
mod cd;
mod echo;
mod exit;
mod export;
mod help;
mod history;
mod plugin;
mod pwd;
pub mod source;
pub mod stack;
mod which;

use crate::error::Result;

pub enum BuiltinAction {
    Continue,
    Exit(i32),
}

pub const NAMES: &[&str] = &[
    "exit", "cd", "pwd", "echo", "export", "alias", "source", ".", "which", "help", "history",
    "pushd", "popd", "dirs", "ai", "explain", "prompt", "plugin",
];

pub fn is_builtin(name: &str) -> bool {
    NAMES.contains(&name)
}

pub fn needs_shell_context(name: &str) -> bool {
    matches!(
        name,
        "cd" | "export"
            | "alias"
            | "source"
            | "."
            | "exit"
            | "help"
            | "pushd"
            | "popd"
            | "dirs"
            | "ai"
            | "explain"
            | "prompt"
    )
}

pub fn run(name: &str, args: &[String]) -> Result<BuiltinAction> {
    match name {
        "exit" => Ok(BuiltinAction::Exit(exit::run(args)?)),
        "cd" => {
            cd::run(args)?;
            Ok(BuiltinAction::Continue)
        }
        "pwd" => {
            pwd::run()?;
            Ok(BuiltinAction::Continue)
        }
        "echo" => {
            echo::run(args)?;
            Ok(BuiltinAction::Continue)
        }
        "export" => {
            export::run(args)?;
            Ok(BuiltinAction::Continue)
        }
        "which" => {
            which::run(args)?;
            Ok(BuiltinAction::Continue)
        }
        "help" => {
            help::run(args)?;
            Ok(BuiltinAction::Continue)
        }
        "history" => {
            history::run(args)?;
            Ok(BuiltinAction::Continue)
        }
        "plugin" => Ok(BuiltinAction::Exit(plugin::run(args)?)),
        _ => unreachable!("unknown builtin: {name}"),
    }
}
