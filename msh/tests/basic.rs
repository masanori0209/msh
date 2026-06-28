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
    assert!(line.contains("50000\\n"), "tail missing");
}

#[test]
fn command_substitution_handles_large_output() {
    // `$(...)` も大出力でデッドロックしないこと。
    // 末尾改行は除去されるため 288894 - 1 = 288893。
    let (stdout, code) = run_c("x=$(seq 1 50000); echo \"len=${#x}\"");
    assert_eq!(code, 0);
    assert!(stdout.contains("len=288893"), "got: {stdout}");
}
