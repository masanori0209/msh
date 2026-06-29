use crate::error::Result;

const TOPICS: &[(&str, &str)] = &[
    ("cd", "change directory: cd [path]"),
    ("export", "set environment variable: export NAME=value"),
    ("alias", "define command alias: alias name=value"),
    ("source", "load a script file: source path"),
    ("pushd", "push directory onto stack: pushd [path]"),
    ("popd", "pop directory from stack: popd"),
    ("dirs", "list directory stack: dirs"),
    (
        "history",
        "show or filter history: history [-n count] [-g pattern]",
    ),
    (
        "prompt",
        "configure prompt interactively: prompt config | prompt preview",
    ),
    ("exit", "exit shell: exit [code]"),
    ("help", "show help: help [topic]"),
];

pub fn run(args: &[String]) -> Result<()> {
    if args.is_empty() {
        print_overview();
        return Ok(());
    }

    let topic = args[0].as_str();
    if let Some((name, description)) = TOPICS.iter().find(|(name, _)| *name == topic) {
        println!("{name}: {description}");
        return Ok(());
    }

    println!("unknown help topic: {topic}");
    print_overview();
    Ok(())
}

fn print_overview() {
    println!("msh — minimal shell");
    println!();
    println!("Builtins:");
    for (name, description) in TOPICS {
        println!("  {name:<8} {description}");
    }
    println!();
    println!("Tips:");
    println!("  Tab          completion");
    println!("  Ctrl+R       history search (case-insensitive preview)");
    println!("  history -g   filter history like fzf");
    println!("  empty Enter  show quick tips");
    println!("  help topic   builtin details");
}
