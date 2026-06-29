//! `--agent` / `--json` の timeout 付き実行（子プロセス再実行）。

use crate::agent::AgentOptions;
use crate::command_json;
use crate::config::ShellConfig;
use crate::shell::Shell;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const TIMEOUT_CHILD_ENV: &str = "MSH_AGENT_TIMEOUT_CHILD";

pub fn run_json_command(
    config: ShellConfig,
    command: &str,
    agent_mode: bool,
    agent_opts: AgentOptions,
) -> i32 {
    if config.agent.timeout_ms > 0 && std::env::var(TIMEOUT_CHILD_ENV).is_err() {
        return run_in_child_with_timeout(config, command, agent_mode, agent_opts);
    }

    let mut shell = Shell::with_config(config);
    shell.init_for_agent();
    if agent_mode {
        shell.run_command_agent_json(command, agent_opts)
    } else {
        shell.run_command_json(command)
    }
}

fn run_in_child_with_timeout(
    config: ShellConfig,
    command: &str,
    agent_mode: bool,
    agent_opts: AgentOptions,
) -> i32 {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return 1,
    };
    let timeout_ms = config.agent.timeout_ms;
    let mut cmd = Command::new(exe);
    cmd.env(TIMEOUT_CHILD_ENV, "1");
    if agent_mode {
        cmd.arg("--agent");
        if agent_opts.dry_run {
            cmd.arg("--agent-dry-run");
        }
        if agent_opts.force {
            cmd.arg("--agent-force");
        }
    } else {
        cmd.arg("--json");
    }
    if let Some(session) = &config.agent.session_path {
        cmd.arg("--agent-session").arg(session);
    }
    cmd.arg("-c").arg(command);
    cmd.stdout(Stdio::piped()).stderr(Stdio::inherit());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("msh: failed to spawn timeout child: {e}");
            return 1;
        }
    };

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_string(&mut stdout);
                }
                print!("{stdout}");
                return status.code().unwrap_or(1);
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("msh: timeout wait failed: {e}");
                return 1;
            }
        }
        if started.elapsed() >= Duration::from_millis(timeout_ms) {
            let _ = child.kill();
            let json = command_json::build_timeout_json(command, timeout_ms);
            println!("{json}");
            return 124;
        }
        thread::sleep(Duration::from_millis(20));
    }
}
