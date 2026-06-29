use msh::agent::AgentOptions;
use msh::agent_runner;
use msh::builtins;
use msh::config::{AgentRcMode, ShellConfig};
use msh::doctor;
use msh::exec;
use msh::hints;
use msh::mcp;
use msh::setup::{self, SetupFlags};
use msh::shell::Shell;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() >= 2 {
        match args[1].as_str() {
            "setup" | "--setup" => {
                let config = ShellConfig::from_env_and_args();
                let flags = parse_setup_flags(&args[2..]);
                match setup::run(config.language, flags) {
                    Ok(_) => std::process::exit(0),
                    Err(e) => {
                        eprintln!("{}", hints::format_error(&e, config.language));
                        std::process::exit(1);
                    }
                }
            }
            "doctor" | "--doctor" => {
                let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");
                let json = args.iter().any(|a| a == "--json");
                let config = ShellConfig::from_env_and_args();
                match doctor::run(verbose) {
                    Ok(report) => {
                        if json {
                            println!("{}", doctor::report_json(&report));
                        } else {
                            doctor::print_report(&report, config.language);
                        }
                        std::process::exit(report.exit_code());
                    }
                    Err(e) => {
                        eprintln!("{}", hints::format_error(&e, config.language));
                        std::process::exit(1);
                    }
                }
            }
            _ => {}
        }
    }

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

    if args.iter().any(|a| a == "--mcp") {
        if let Err(e) = mcp::run_server() {
            eprintln!("msh: {e}");
            std::process::exit(1);
        }
        return;
    }

    if args.iter().any(|a| a == "--configure-prompt") {
        let mut config = ShellConfig::from_env_and_args();
        let mut cache = msh::prompt::Cache::new();
        let lang = config.language;
        match msh::prompt_setup::run(&mut config, &mut cache, lang) {
            Ok(true) => {
                println!("Saved ~/.config/msh/config.toml");
            }
            Ok(false) => {}
            Err(e) => {
                eprintln!("{}", hints::format_error(&e, lang));
                std::process::exit(1);
            }
        }
        return;
    }

    let mut config = ShellConfig::from_env_and_args();
    let mut json_output = false;
    let mut agent_mode = false;
    let mut agent_opts = AgentOptions::default();
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
            "--agent" => {
                agent_mode = true;
                json_output = true;
                index += 1;
            }
            "--agent-dry-run" => {
                agent_mode = true;
                agent_opts.dry_run = true;
                json_output = true;
                index += 1;
            }
            "--agent-force" => {
                agent_opts.force = true;
                index += 1;
            }
            "--agent-session" => {
                if let Some(path) = args.get(index + 1) {
                    config.agent.session_path = Some(path.clone());
                    index += 2;
                } else {
                    eprintln!("msh: --agent-session requires a path");
                    std::process::exit(1);
                }
            }
            "--agent-rc" => {
                if let Some(mode) = args.get(index + 1) {
                    config.agent.rc_mode = match mode.to_ascii_lowercase().as_str() {
                        "skip" | "none" => AgentRcMode::Skip,
                        "minimal" | "env" => AgentRcMode::Minimal,
                        "full" => AgentRcMode::Full,
                        _ => {
                            eprintln!("msh: --agent-rc requires skip|minimal|full");
                            std::process::exit(1);
                        }
                    };
                    index += 2;
                } else {
                    eprintln!("msh: --agent-rc requires a mode");
                    std::process::exit(1);
                }
            }
            "-c" => {
                if let Some(command) = args.get(index + 1) {
                    let code = if json_output || agent_mode {
                        agent_runner::run_json_command(config, command, agent_mode, agent_opts)
                    } else {
                        let mut shell = Shell::with_config(config);
                        shell.init_for_command();
                        match shell.eval_line(command, false) {
                            Ok(builtins::BuiltinAction::Continue) => shell.last_status(),
                            Ok(builtins::BuiltinAction::Exit(code)) => code,
                            Err(e) => {
                                eprintln!("{}", hints::format_error(&e, msh::config::Language::En));
                                1
                            }
                        }
                    };
                    std::process::exit(code);
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

fn parse_setup_flags(args: &[String]) -> SetupFlags {
    SetupFlags {
        yes: args.iter().any(|a| a == "--yes" || a == "-y"),
        strict: args.iter().any(|a| a == "--strict"),
        skip_integrations: args.iter().any(|a| a == "--skip-integrations"),
    }
}
