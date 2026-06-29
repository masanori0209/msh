use std::io::Write;
use std::process::{Command, Stdio};

fn run_repl(input: &str) -> (String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_msh"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn msh");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (stdout, output.status.code().unwrap_or(1))
}

fn run_c(command: &str) -> (String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_msh"))
        .arg("-c")
        .arg(command)
        .output()
        .expect("failed to spawn msh -c");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (stdout, output.status.code().unwrap_or(1))
}

#[test]
fn setup_writes_config_noninteractive() {
    let home = std::env::temp_dir().join(format!("msh-setup-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create home");
    let config_dir = home.join(".config/msh");
    let output = Command::new(env!("CARGO_BIN_EXE_msh"))
        .env("HOME", &home)
        .args(["setup", "--yes", "--skip-integrations"])
        .output()
        .expect("spawn setup");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config_path = config_dir.join("config.toml");
    assert!(config_path.is_file(), "config.toml should exist");
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("[agent]"), "expected [agent] section");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn doctor_reports_core_checks() {
    let output = Command::new(env!("CARGO_BIN_EXE_msh"))
        .arg("doctor")
        .output()
        .expect("spawn doctor");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("agent gate (destructive)"));
    assert!(stdout.contains("MCP tools/call"));
}

#[test]
fn doctor_json_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_msh"))
        .args(["doctor", "--json"])
        .output()
        .expect("spawn doctor --json");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"checks\""));
    assert!(stdout.contains("\"exit_code\""));
}
#[test]
fn echo_hello() {
    let (stdout, code) = run_repl("echo hello\nexit\n");
    assert_eq!(code, 0);
    assert!(stdout.contains("hello"));
}

#[test]
fn pwd_prints_directory() {
    let (stdout, code) = run_repl("pwd\nexit\n");
    assert_eq!(code, 0);
    assert!(stdout.contains('/'));
}

#[test]
fn exit_with_code() {
    let (_, code) = run_repl("exit 42\n");
    assert_eq!(code, 42);
}

#[test]
fn unknown_command_returns_error() {
    // bash 互換: 不在コマンドは 127、引数なし exit は直前ステータス(=127)を返す。
    let (_, code) = run_repl("no_such_command_xyz\nexit\n");
    assert_eq!(code, 127);
}

#[test]
fn pipeline_wc() {
    let (stdout, code) = run_c("echo hello | wc -c");
    assert_eq!(code, 0);
    assert!(stdout.trim().ends_with('6'));
}

#[test]
fn redirect_stdout() {
    let path = std::env::temp_dir().join("msh_phase2_out.txt");
    let _ = std::fs::remove_file(&path);
    let command = format!("echo hello > {}", path.display());
    let (_, code) = run_c(&command);
    assert_eq!(code, 0);
    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content.trim(), "hello");
    let _ = std::fs::remove_file(path);
}

#[test]
fn expand_env_var() {
    std::env::set_var("MSH_ITEST", "world");
    let (stdout, code) = run_c("echo $MSH_ITEST");
    assert_eq!(code, 0);
    assert!(stdout.contains("world"));
}

#[test]
fn alias_expansion() {
    let (stdout, code) = run_repl("alias hi=echo\nhi there\nexit\n");
    assert_eq!(code, 0);
    assert!(stdout.contains("there"));
}

fn run_json(command: &str) -> (String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_msh"))
        .env("MSH_SKIP_RC", "1")
        .arg("--json")
        .arg("-c")
        .arg(command)
        .output()
        .expect("failed to spawn msh --json -c");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (stdout, output.status.code().unwrap_or(1))
}

#[test]
fn json_output_captures_stdout_and_exit() {
    let (stdout, code) = run_json("echo hi");
    assert_eq!(code, 0);
    let line = stdout.trim();
    assert!(line.starts_with('{') && line.ends_with('}'), "got: {line}");
    assert!(line.contains("\"stdout\":\"hi\\n\""), "got: {line}");
    assert!(line.contains("\"exit_code\":0"), "got: {line}");
    assert!(line.contains("\"duration_ms\":"), "got: {line}");
    assert!(line.contains("\"command\":\"echo hi\""), "got: {line}");
}

#[test]
fn json_output_reports_failure_and_stderr() {
    let (stdout, code) = run_json("ls /nonexistent_msh_dir_xyz");
    assert_ne!(code, 0);
    let line = stdout.trim();
    assert!(line.contains("\"exit_code\":"), "got: {line}");
    assert!(!line.contains("\"exit_code\":0"), "got: {line}");
    assert!(line.contains("\"stderr\":\""), "got: {line}");
    assert!(
        line.contains("No such file") || line.contains("nonexistent"),
        "got: {line}"
    );
}

#[test]
fn json_output_escapes_quotes() {
    let (stdout, code) = run_json("echo '\"quoted\"'");
    assert_eq!(code, 0);
    let line = stdout.trim();
    assert!(line.contains("\\\"quoted\\\""), "got: {line}");
}

#[test]
fn json_output_handles_large_output_without_deadlock() {
    // パイプバッファ(64KB)を大きく超える出力でもデッドロックしないこと（一時ファイル経由）。
    let (stdout, code) = run_json("seq 1 50000");
    assert_eq!(code, 0);
    let line = stdout.trim();
    assert!(
        line.starts_with('{') && line.ends_with('}'),
        "len={}",
        line.len()
    );
    assert!(
        line.contains("stdout_truncated") && line.contains("stdout_bytes"),
        "missing truncate metadata: {}",
        &line[..line.len().min(200)]
    );
}

#[test]
fn json_output_truncates_when_limit_set() {
    let output = Command::new(env!("CARGO_BIN_EXE_msh"))
        .env("MSH_SKIP_RC", "1")
        .env("MSH_AGENT_JSON_MAX_BYTES", "128")
        .arg("--json")
        .arg("-c")
        .arg("seq 1 50000")
        .output()
        .expect("spawn");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"stdout_truncated\":true"));
}

#[test]
fn json_output_includes_cwd_meta() {
    let (stdout, code) = run_json("pwd");
    assert_eq!(code, 0);
    assert!(stdout.contains("\"cwd\":\""), "got: {stdout}");
}

#[test]
fn agent_session_persists_cwd() {
    let session = std::env::temp_dir().join(format!("msh-agent-session-{}", std::process::id()));
    let _ = std::fs::remove_file(&session);
    let session_str = session.to_string_lossy();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_msh"));
    cmd.env("MSH_SKIP_RC", "1")
        .arg("--agent-session")
        .arg(session_str.as_ref())
        .arg("--agent")
        .arg("-c")
        .arg("cd /tmp && pwd");
    let out1 = cmd.output().expect("spawn 1");
    assert_eq!(out1.status.code(), Some(0));

    let out2 = Command::new(env!("CARGO_BIN_EXE_msh"))
        .env("MSH_SKIP_RC", "1")
        .arg("--agent-session")
        .arg(session_str.as_ref())
        .arg("--agent")
        .arg("-c")
        .arg("pwd")
        .output()
        .expect("spawn 2");
    let stdout = String::from_utf8_lossy(&out2.stdout);
    assert!(stdout.contains("/tmp"), "got: {stdout}");
    let _ = std::fs::remove_file(session);
}

#[test]
fn agent_blocks_caution_when_configured() {
    let (stdout, code) =
        run_agent_json_with_env("rm file.txt", &[("MSH_AGENT_BLOCK_CAUTION", "1")]);
    assert_ne!(code, 0);
    assert!(stdout.contains("blocked"), "got: {stdout}");
}

fn run_agent_json_with_env(command: &str, envs: &[(&str, &str)]) -> (String, i32) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_msh"));
    cmd.env("MSH_SKIP_RC", "1").arg("--agent");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.arg("-c").arg(command);
    let output = cmd.output().expect("failed to spawn msh --agent -c");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        output.status.code().unwrap_or(1),
    )
}

#[test]
fn command_substitution_handles_large_output() {
    // `$(...)` も大出力でデッドロックしないこと。
    // 末尾改行は除去されるため 288894 - 1 = 288893。
    let (stdout, code) = run_c("x=$(seq 1 50000); echo \"len=${#x}\"");
    assert_eq!(code, 0);
    assert!(stdout.contains("len=288893"), "got: {stdout}");
}

fn run_agent_json(command: &str, extra_args: &[&str]) -> (String, i32) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_msh"));
    cmd.env("MSH_SKIP_RC", "1").arg("--agent");
    for arg in extra_args {
        cmd.arg(arg);
    }
    cmd.arg("-c").arg(command);
    let output = cmd.output().expect("failed to spawn msh --agent -c");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (stdout, output.status.code().unwrap_or(1))
}

#[test]
fn agent_blocks_destructive_command() {
    let (stdout, code) = run_agent_json("rm -rf /tmp/should-not-run", &[]);
    assert_ne!(code, 0);
    assert!(stdout.contains("blocked"), "got: {stdout}");
}

#[test]
fn agent_dry_run_does_not_execute() {
    let (stdout, code) = run_agent_json("echo executed-marker", &["--agent-dry-run"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("\"action\":\"dry_run\""), "got: {stdout}");
    assert!(
        !stdout.contains("\"stdout\""),
        "dry-run must not capture command output: {stdout}"
    );
}
