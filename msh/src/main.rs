use msh::builtins;
use msh::config::ShellConfig;
use msh::exec;
use msh::hints;
use msh::shell::Shell;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() >= 3 && args[1] == "--builtin" {
        let code = match exec::run_builtin_cli(&args[2], &args[3..]) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("{}", hints::format_error(&e, msh::config::Language::En));
                1
            }
        };
        std::process::exit(code);
    }

    let mut config = ShellConfig::from_env_and_args();
    let mut json_output = false;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--compat" => {
                let mode = args.get(index + 1).map(String::as_str);
                config = config.with_compat_flag(mode);
                index += 2;
            }
            "--json" => {
                json_output = true;
                index += 1;
            }
            "-c" => {
                if let Some(command) = args.get(index + 1) {
                    let mut shell = Shell::with_config(config);
                    shell.init_for_command();
                    if json_output {
                        std::process::exit(shell.run_command_json(command));
                    }
                    match shell.eval_line(command, false) {
                        Ok(builtins::BuiltinAction::Continue) => {
                            std::process::exit(shell.last_status())
                        }
                        Ok(builtins::BuiltinAction::Exit(code)) => std::process::exit(code),
                        Err(e) => {
                            eprintln!("{}", hints::format_error(&e, msh::config::Language::En));
                            std::process::exit(1);
                        }
                    }
                }
                eprintln!("msh: -c requires a command");
                std::process::exit(1);
            }
            _ => break,
        }
    }

    let mut shell = Shell::with_config(config);
    shell.init_interactive();
    let code = shell.run();
    std::process::exit(code);
}
