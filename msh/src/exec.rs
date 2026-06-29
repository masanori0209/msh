use crate::builtins::{self, BuiltinAction};
use crate::error::{MshError, Result};
use crate::expand;
use crate::parse::{Arg, CommandSpec, OpenMode, ParsedLine, Stream};
use std::collections::HashMap;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{self, Command, Stdio};
use std::sync::{Arc, LazyLock, Mutex};

static FOREGROUND_PID: LazyLock<Arc<Mutex<Option<u32>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(None)));

pub fn install_signal_handler() {
    let pid_store = Arc::clone(&FOREGROUND_PID);
    let _ = ctrlc::set_handler(move || {
        if let Ok(guard) = pid_store.lock() {
            if let Some(pid) = *guard {
                #[cfg(unix)]
                {
                    use nix::sys::signal::{kill, Signal};
                    use nix::unistd::Pid;
                    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGINT);
                }
            }
        }
    });
}

pub fn run_parsed(parsed: &ParsedLine, heredocs: &HashMap<String, String>) -> Result<i32> {
    let statuses = run_parsed_statuses(parsed, heredocs)?;
    Ok(statuses.last().copied().unwrap_or(0))
}

/// パイプライン各ステージの終了ステータスを順番に返す（`$PIPESTATUS` 用）。
pub fn run_parsed_statuses(
    parsed: &ParsedLine,
    heredocs: &HashMap<String, String>,
) -> Result<Vec<i32>> {
    if parsed.pipeline.is_empty() {
        return Ok(vec![0]);
    }

    if parsed.background {
        return Ok(vec![spawn_background(parsed, heredocs)?]);
    }

    run_pipeline(&parsed.pipeline, heredocs)
}

fn run_pipeline(pipeline: &[CommandSpec], heredocs: &HashMap<String, String>) -> Result<Vec<i32>> {
    let n = pipeline.len();
    if n == 1 {
        return Ok(vec![run_stage(&pipeline[0], None, None, heredocs)?]);
    }

    let mut pipes: Vec<(Option<io::PipeReader>, Option<io::PipeWriter>)> = Vec::new();
    for _ in 0..n - 1 {
        let (reader, writer) = io::pipe()?;
        pipes.push((Some(reader), Some(writer)));
    }

    let mut children: Vec<Option<process::Child>> = Vec::with_capacity(n);

    for (i, spec) in pipeline.iter().enumerate() {
        let stdin = if i == 0 { None } else { pipes[i - 1].0.take() };
        let stdout = if i + 1 == n { None } else { pipes[i].1.take() };

        match spawn_stage(spec, stdin, stdout, heredocs) {
            Ok(child) => children.push(Some(child)),
            // bash 互換: 不在コマンドはステージ 127 とし、他段の起動は続行する。
            Err(MshError::CommandNotFound(cmd)) => {
                eprintln!("msh: command not found: {cmd}");
                children.push(None);
            }
            Err(e) => return Err(e),
        }
    }

    let mut statuses = Vec::with_capacity(n);
    for child in children.iter_mut() {
        clear_foreground_pid();
        match child {
            Some(child) => statuses.push(child.wait()?.code().unwrap_or(1)),
            None => statuses.push(127),
        }
    }

    Ok(statuses)
}

fn run_stage(
    spec: &CommandSpec,
    stdin: Option<io::PipeReader>,
    stdout: Option<io::PipeWriter>,
    heredocs: &HashMap<String, String>,
) -> Result<i32> {
    match spawn_stage(spec, stdin, stdout, heredocs) {
        Ok(mut child) => {
            clear_foreground_pid();
            Ok(child.wait()?.code().unwrap_or(1))
        }
        // bash 互換: コマンド不在は致命的エラーにせず、ステータス 127 として
        // 通常の制御フロー（`;` `||` `&&` 等）を続行できるようにする。
        Err(MshError::CommandNotFound(cmd)) => {
            eprintln!("msh: command not found: {cmd}");
            Ok(127)
        }
        Err(e) => Err(e),
    }
}

fn spawn_stage(
    spec: &CommandSpec,
    stdin: Option<io::PipeReader>,
    stdout: Option<io::PipeWriter>,
    heredocs: &HashMap<String, String>,
) -> Result<process::Child> {
    if spec.argv.is_empty() {
        return Err(MshError::ParseError("empty command".into()));
    }

    let program = &spec.argv[0].value;
    let args: Vec<&str> = spec.argv[1..]
        .iter()
        .map(|arg| arg.value.as_str())
        .collect();

    let mut cmd = if builtins::is_builtin(program) {
        let mut cmd = Command::new(current_exe()?);
        cmd.arg("--builtin").arg(program);
        for arg in &spec.argv[1..] {
            cmd.arg(&arg.value);
        }
        cmd
    } else {
        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd
    };

    apply_redirects(&mut cmd, &spec.redirects, heredocs)?;

    if let Some(stdin) = stdin {
        cmd.stdin(Stdio::from(stdin));
    }
    if let Some(stdout) = stdout {
        cmd.stdout(Stdio::from(stdout));
    }

    let child = cmd.spawn().map_err(|e| spawn_error(program, e))?;
    set_foreground_pid(child.id());
    Ok(child)
}

/// spawn 失敗を分類する。実行ファイル不在は bash 互換の `command not found` 扱い。
fn spawn_error(program: &str, err: io::Error) -> MshError {
    if err.kind() == io::ErrorKind::NotFound {
        MshError::CommandNotFound(program.to_string())
    } else {
        MshError::Io(err)
    }
}

fn spawn_background(parsed: &ParsedLine, heredocs: &HashMap<String, String>) -> Result<i32> {
    if parsed.pipeline.len() != 1 {
        return Err(MshError::ParseError(
            "background jobs support only a single command".into(),
        ));
    }

    let spec = &parsed.pipeline[0];
    if spec.argv.is_empty() {
        return Err(MshError::ParseError("empty command".into()));
    }

    let program = &spec.argv[0].value;
    let mut cmd = if builtins::is_builtin(program) {
        let mut cmd = Command::new(current_exe()?);
        cmd.arg("--builtin").arg(program);
        for arg in &spec.argv[1..] {
            cmd.arg(&arg.value);
        }
        cmd
    } else {
        let mut cmd = Command::new(program);
        for arg in &spec.argv[1..] {
            cmd.arg(&arg.value);
        }
        cmd
    };

    if spec.redirects.is_empty() {
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
    } else {
        apply_redirects(&mut cmd, &spec.redirects, heredocs)?;
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                eprintln!("msh: command not found: {program}");
                return Ok(127);
            }
            return Err(MshError::Io(e));
        }
    };
    let pid = child.id();
    clear_foreground_pid();
    let argv_display: Vec<_> = spec.argv.iter().map(|arg| arg.value.as_str()).collect();
    println!("[{}] {}", pid, argv_display.join(" "));

    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(0)
}

pub fn run_builtin_cli(program: &str, args: &[String]) -> Result<i32> {
    match builtins::run(program, args)? {
        BuiltinAction::Exit(code) => Ok(code),
        BuiltinAction::Continue => Ok(0),
    }
}

fn apply_redirects(
    cmd: &mut Command,
    redirects: &[crate::parse::Redirect],
    heredocs: &HashMap<String, String>,
) -> Result<()> {
    for redirect in redirects {
        match redirect.stream {
            Stream::Both => {
                let stdout = open_redirect(redirect, heredocs)?;
                let stderr = open_redirect(redirect, heredocs)?;
                cmd.stdout(Stdio::from(stdout));
                cmd.stderr(Stdio::from(stderr));
            }
            stream => {
                if redirect.heredoc && stream == Stream::Stdin {
                    let stdin = open_heredoc(redirect, heredocs)?;
                    cmd.stdin(Stdio::from(stdin));
                    continue;
                }
                let file = open_redirect(redirect, heredocs)?;
                match stream {
                    Stream::Stdin => {
                        cmd.stdin(Stdio::from(file));
                    }
                    Stream::Stdout => {
                        cmd.stdout(Stdio::from(file));
                    }
                    Stream::Stderr => {
                        cmd.stderr(Stdio::from(file));
                    }
                    Stream::Both => unreachable!(),
                }
            }
        }
    }
    Ok(())
}

fn open_heredoc(
    redirect: &crate::parse::Redirect,
    heredocs: &HashMap<String, String>,
) -> Result<io::PipeReader> {
    let delimiter = redirect
        .heredoc_delimiter()
        .ok_or_else(|| MshError::ParseError("invalid heredoc redirect".into()))?;
    let body = heredocs
        .get(delimiter)
        .ok_or_else(|| MshError::ParseError(format!("heredoc body missing for {delimiter}")))?;
    let (reader, mut writer) = io::pipe()?;
    writer.write_all(body.as_bytes())?;
    Ok(reader)
}

fn open_redirect(
    redirect: &crate::parse::Redirect,
    _heredocs: &HashMap<String, String>,
) -> Result<File> {
    let path = PathBuf::from(&redirect.path);
    match (&redirect.stream, &redirect.mode) {
        (Stream::Stdin, _) => File::open(path).map_err(Into::into),
        (_, OpenMode::Truncate) => OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(Into::into),
        (_, OpenMode::Append) => OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(Into::into),
    }
}

fn current_exe() -> Result<PathBuf> {
    env::current_exe().map_err(Into::into)
}

fn set_foreground_pid(pid: u32) {
    if let Ok(mut guard) = FOREGROUND_PID.lock() {
        *guard = Some(pid);
    }
}

fn clear_foreground_pid() {
    if let Ok(mut guard) = FOREGROUND_PID.lock() {
        *guard = None;
    }
}

pub fn expand_command_with(
    command: &CommandSpec,
    ctx: &expand::ExpandContext<'_>,
) -> Result<CommandSpec> {
    let mut argv = Vec::new();
    for word in &command.argv {
        if word.literal {
            argv.push(Arg {
                value: word.value.clone(),
                literal: true,
            });
            continue;
        }
        for expanded in expand::expand_word_with(&word.value, ctx)? {
            argv.push(Arg {
                value: expanded,
                literal: false,
            });
        }
    }

    let mut redirects = command.redirects.clone();
    for redirect in &mut redirects {
        if !redirect.heredoc {
            redirect.path = expand::expand_all(&redirect.path, ctx)?;
        }
    }

    Ok(CommandSpec { argv, redirects })
}

pub fn expand_command(command: &CommandSpec) -> Result<CommandSpec> {
    let empty_vars = std::collections::HashMap::new();
    let empty_arrays = std::collections::HashMap::new();
    let empty_assoc = expand::AssocArrays::new();
    let ctx = expand::ExpandContext {
        last_status: 0,
        shell_vars: &empty_vars,
        arrays: &empty_arrays,
        assoc: &empty_assoc,
        nounset: false,
        pending_assigns: None,
    };
    expand_command_with(command, &ctx)
}

pub fn expand_pipeline_with(
    parsed: &ParsedLine,
    ctx: &expand::ExpandContext<'_>,
) -> Result<ParsedLine> {
    let pipeline = parsed
        .pipeline
        .iter()
        .map(|stage| expand_command_with(stage, ctx))
        .collect::<Result<Vec<_>>>()?;

    Ok(ParsedLine {
        pipeline,
        background: parsed.background,
    })
}

pub fn expand_pipeline(parsed: &ParsedLine) -> Result<ParsedLine> {
    let empty_vars = std::collections::HashMap::new();
    let empty_arrays = std::collections::HashMap::new();
    let empty_assoc = expand::AssocArrays::new();
    let ctx = expand::ExpandContext {
        last_status: 0,
        shell_vars: &empty_vars,
        arrays: &empty_arrays,
        assoc: &empty_assoc,
        nounset: false,
        pending_assigns: None,
    };
    expand_pipeline_with(parsed, &ctx)
}
