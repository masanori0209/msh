use crate::builtins;

pub fn builtin_description(name: &str) -> Option<&'static str> {
    match name {
        "exit" => Some("exit the shell"),
        "cd" => Some("change directory"),
        "pwd" => Some("print working directory"),
        "echo" => Some("print arguments"),
        "export" => Some("set environment variables"),
        "alias" => Some("define aliases"),
        "source" | "." => Some("load a script file"),
        "which" => Some("locate a command"),
        "help" => Some("show help"),
        "pushd" => Some("push directory on stack"),
        "popd" => Some("pop directory from stack"),
        "dirs" => Some("list directory stack"),
        "history" => Some("show or filter command history"),
        "ai" => Some("ask the configured AI model (prints, never executes)"),
        "explain" => Some("explain the previous or given command via AI"),
        "prompt" => Some("configure or preview the interactive prompt"),
        _ => None,
    }
}

pub fn common_command_description(name: &str) -> Option<&'static str> {
    match name {
        "ls" => Some("list files"),
        "git" => Some("version control"),
        "cargo" => Some("Rust package manager"),
        "docker" => Some("container runtime"),
        "grep" => Some("search text"),
        _ => None,
    }
}

pub fn describe(name: &str) -> Option<&'static str> {
    builtin_description(name).or_else(|| common_command_description(name))
}

pub fn all_builtin_names() -> impl Iterator<Item = &'static str> {
    builtins::NAMES.iter().copied()
}
