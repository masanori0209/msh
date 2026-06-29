use crate::builtins::{self, stack as dir_stack, BuiltinAction};
use crate::config::{self, HistoryBackend, ShellConfig};
use crate::error::MshError;
use crate::exec;
use crate::expand::{self, ExpandContext};
use crate::heredoc::{self, PendingHeredoc, PrepareResult};
use crate::line_editor::{self, LineEditor};
use crate::onboarding;
use crate::parse::{self, Arg, ChainOp, ParsedLine, ParsedScript};
use crate::prompt;
use crate::script::{self, PendingBlock, Stmt};
use crate::session::{self, SessionState};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::env;
use std::hash::{Hash, Hasher};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SLOW_COMMAND_THRESHOLD: Duration = Duration::from_millis(500);

enum LoopControl {
    None,
    Break,
    Continue,
    Return(i32),
}

pub struct Shell {
    pub aliases: HashMap<String, String>,
    pub config: ShellConfig,
    functions: HashMap<String, Vec<Stmt>>,
    shell_vars: Vec<HashMap<String, String>>,
    arrays: Vec<HashMap<String, Vec<String>>>,
    assoc: Vec<expand::AssocArrays>,
    dir_stack: Vec<String>,
    heredoc_bodies: HashMap<String, String>,
    last_status: i32,
    last_duration: Duration,
    prompt_cache: prompt::Cache,
    alias_fingerprint: u64,
    pending_block: Option<PendingBlock>,
    pending_heredoc: Option<PendingHeredoc>,
    errexit: bool,
    nounset: bool,
    suppress_errexit: u32,
    last_command: String,
    pending_prefill: Option<String>,
}

impl Shell {
    pub fn new() -> Self {
        Self::with_config(ShellConfig::from_env_and_args())
    }

    pub fn with_config(config: ShellConfig) -> Self {
        Self {
            aliases: HashMap::new(),
            config,
            functions: HashMap::new(),
            shell_vars: vec![HashMap::new()],
            arrays: vec![HashMap::new()],
            assoc: vec![HashMap::new()],
            dir_stack: Vec::new(),
            heredoc_bodies: HashMap::new(),
            last_status: 0,
            last_duration: Duration::ZERO,
            prompt_cache: prompt::Cache::new(),
            alias_fingerprint: 0,
            pending_block: None,
            pending_heredoc: None,
            errexit: false,
            nounset: false,
            suppress_errexit: 0,
            last_command: String::new(),
            pending_prefill: None,
        }
    }

    /// 次プロンプトに事前挿入するテキスト（NL→コマンド提案）を取り出す。
    pub fn take_prefill(&mut self) -> Option<String> {
        self.pending_prefill.take()
    }

    pub fn init(&mut self) {
        self.load_startup_files(true);
    }

    /// `-c` 等の非対話単発実行用。セッション復元を省略し cwd を上書きしない。
    pub fn init_for_command(&mut self) {
        self.load_startup_files(false);
    }

    /// エージェント向け起動（rc 戦略・セッション復元・sandbox env）。
    pub fn init_for_agent(&mut self) {
        match self.config.agent.rc_mode {
            crate::config::AgentRcMode::Skip => {}
            crate::config::AgentRcMode::Minimal => self.load_msh_env_only(),
            crate::config::AgentRcMode::Full => self.load_startup_files(false),
        }
        self.restore_agent_session();
        if let Some(root) = &self.config.agent.sandbox_root {
            if !root.is_empty() {
                std::env::set_var("MSH_AGENT_SANDBOX", root);
                if let Err(e) = crate::agent::verify_cwd_in_sandbox(root) {
                    line_editor::report_error(&e, self.config.language);
                }
            }
        }
    }

    pub fn finalize_agent_session(&mut self) {
        if let Some(root) = &self.config.agent.sandbox_root {
            if !root.is_empty() {
                if let Err(e) = crate::agent::verify_cwd_in_sandbox(root) {
                    line_editor::report_error(&e, self.config.language);
                }
            }
        }
        self.save_agent_session();
    }

    fn load_msh_env_only(&mut self) {
        if let Ok(home) = std::env::var("HOME") {
            let path = PathBuf::from(home).join(".msh_env");
            if path.is_file() {
                if let Err(e) = self.load_file(&path) {
                    line_editor::report_error(&e, self.config.language);
                }
            }
        }
    }

    fn restore_agent_session(&mut self) {
        let Some(path) = self.config.agent.session_path.clone() else {
            return;
        };
        if let Err(e) = self.restore_agent_session_from(&path) {
            line_editor::report_error(&e, self.config.language);
        }
    }

    fn save_agent_session(&self) {
        let Some(path) = self.config.agent.session_path.clone() else {
            return;
        };
        let _ = session::save(&self.session_state(), Path::new(&path));
    }

    fn restore_agent_session_from(&mut self, path: &str) -> Result<(), MshError> {
        let Some(state) = session::load(Path::new(path))? else {
            return Ok(());
        };
        session::restore(&state)?;
        self.dir_stack = state.dir_stack;
        Ok(())
    }

    fn load_startup_files(&mut self, restore_session: bool) {
        if env::var("MSH_SKIP_RC").is_err() {
            if let Err(e) = self.load_rc() {
                line_editor::report_error(&e, self.config.language);
            }
        }
        if restore_session && self.config.session_restore {
            if let Err(e) = self.restore_session() {
                line_editor::report_error(&e, self.config.language);
            }
        }
    }

    pub fn init_interactive(&mut self) {
        if let Some(config_dir) = ShellConfig::config_dir() {
            onboarding::maybe_show(&config_dir, self.config.language);
        }
        self.init();
        if let Err(e) = self.maybe_init_atuin() {
            line_editor::report_error(&e, self.config.language);
        }
        exec::install_signal_handler();
    }

    pub fn last_status(&self) -> i32 {
        self.last_status
    }

    pub fn run(&mut self) -> i32 {
        if line_editor::is_interactive_input() {
            self.run_interactive()
        } else {
            self.run_plain()
        }
    }

    fn run_plain(&mut self) -> i32 {
        loop {
            let prompt = prompt::render(prompt::RenderContext {
                last_status: self.last_status,
                last_duration: self.last_duration,
                cache: &mut self.prompt_cache,
                settings: &self.config.prompt,
                theme: self.config.theme,
            });
            let line = match line_editor::read_plain_line(&prompt) {
                Ok(Some(line)) => line,
                Ok(None) => {
                    self.save_session();
                    return self.last_status;
                }
                Err(e) => {
                    eprintln!("msh: {e}");
                    return 1;
                }
            };

            if !self.handle_line(&line, true) {
                self.save_session();
                return self.last_status;
            }
        }
    }

    fn run_interactive(&mut self) -> i32 {
        let mut editor = match LineEditor::new(&self.aliases, self.config.fuzzy_completion) {
            Ok(editor) => editor,
            Err(e) => {
                eprintln!("msh: failed to initialize line editor: {e}");
                return self.run_plain();
            }
        };

        loop {
            let fingerprint = alias_fingerprint(&self.aliases);
            if fingerprint != self.alias_fingerprint {
                editor.refresh(&self.aliases);
                self.alias_fingerprint = fingerprint;
            }

            let prompt = prompt::render(prompt::RenderContext {
                last_status: self.last_status,
                last_duration: self.last_duration,
                cache: &mut self.prompt_cache,
                settings: &self.config.prompt,
                theme: self.config.theme,
            });
            let prefill = self.take_prefill();
            let read = match prefill {
                Some(initial) => editor.read_line_with_initial(&prompt, &initial),
                None => editor.read_line(&prompt),
            };
            let line = match read {
                Ok(Some(line)) => line,
                Ok(None) => {
                    editor.save_history();
                    self.save_session();
                    return self.last_status;
                }
                Err(e) => {
                    eprintln!("msh: {e}");
                    editor.save_history();
                    self.save_session();
                    return 1;
                }
            };

            if !self.handle_line(&line, true) {
                editor.save_history();
                self.save_session();
                return self.last_status;
            }
        }
    }

    fn handle_line(&mut self, line: &str, interactive: bool) -> bool {
        match self.eval_line(line, interactive) {
            Ok(BuiltinAction::Continue) => {
                if interactive && self.last_duration >= SLOW_COMMAND_THRESHOLD {
                    eprintln!(
                        "\x1b[90m  completed in {:.2}s\x1b[0m",
                        self.last_duration.as_secs_f64()
                    );
                }
                true
            }
            Ok(BuiltinAction::Exit(code)) => {
                self.last_status = code;
                false
            }
            Err(e) => {
                line_editor::report_error(&e, self.config.language);
                self.last_status = 1;
                true
            }
        }
    }

    pub fn eval_line(&mut self, line: &str, interactive: bool) -> Result<BuiltinAction, MshError> {
        let line = line.trim();
        if line.is_empty() {
            if interactive {
                onboarding::quick_tip(self.config.language);
            }
            return Ok(BuiltinAction::Continue);
        }

        if line.starts_with('#') {
            // ai 有効・対話時は `# 自然文` を NL→コマンド提案として扱う。
            let request = line.trim_start_matches('#').trim();
            if interactive && self.config.ai.enabled && !request.is_empty() {
                self.suggest_from_comment(request)?;
            }
            return Ok(BuiltinAction::Continue);
        }

        // 直前コマンドとして記録（explain / ai 自身は除外し、参照対象を保つ）。
        if interactive {
            let head = line.split_whitespace().next().unwrap_or("");
            if !matches!(head, "explain" | "ai" | "prompt") {
                self.last_command = line.to_string();
            }
        }

        if let Some(err) = config::detect_unsupported(line) {
            return Err(err);
        }

        if let Some(mut pending) = self.pending_heredoc.take() {
            if let Some((input, bodies)) = heredoc::continue_pending(&mut pending, line)? {
                self.heredoc_bodies.extend(bodies);
                return self.eval_line(&input, interactive);
            }
            self.pending_heredoc = Some(pending);
            return Ok(BuiltinAction::Continue);
        }

        if let Some(block) = &mut self.pending_block {
            let mut block = block.clone();
            match script::continue_block(&mut block, line)? {
                Some(stmts) => {
                    self.pending_block = None;
                    let started = Instant::now();
                    let result = self.eval_stmts(&stmts);
                    if interactive {
                        self.last_duration = started.elapsed();
                    }
                    return result;
                }
                None => {
                    self.pending_block = Some(block);
                    return Ok(BuiltinAction::Continue);
                }
            }
        }

        if let Some(kind) = script::detect_block_start(line) {
            if needs_multiline_block(line, &kind) {
                self.pending_block = Some(PendingBlock {
                    kind,
                    lines: vec![line.to_string()],
                    depth: open_brace_depth(line),
                });
                return Ok(BuiltinAction::Continue);
            }
        }

        let started = Instant::now();
        let result = match heredoc::prepare(line)? {
            PrepareResult::NeedMore(pending) => {
                self.pending_heredoc = Some(pending);
                Ok(BuiltinAction::Continue)
            }
            PrepareResult::Ready { input, bodies } => {
                self.heredoc_bodies.extend(bodies);
                self.eval_line_inner(&input)
            }
            PrepareResult::Unchanged => self.eval_line_inner(line),
        };
        if interactive {
            self.last_duration = started.elapsed();
        }
        result
    }

    fn eval_line_inner(&mut self, line: &str) -> Result<BuiltinAction, MshError> {
        let trimmed = line.trim();
        let stmts = script::parse_inline_or_single(trimmed)?;
        if !stmts.is_empty() && !matches!(stmts[0], Stmt::Command(_)) {
            return self.eval_stmts(&stmts);
        }

        let script = parse::parse_script(line)?;
        if script.segments.is_empty() {
            return Ok(BuiltinAction::Continue);
        }

        self.eval_script(&script)
    }

    fn eval_script(&mut self, script: &ParsedScript) -> Result<BuiltinAction, MshError> {
        let mut action = BuiltinAction::Continue;
        let mut skip_next = false;

        for segment in &script.segments {
            if !skip_next {
                let stmts = script::parse_inline_or_single(&segment.source)?;
                if !stmts.is_empty() && !matches!(stmts[0], Stmt::Command(_)) {
                    action = self.eval_stmts(&stmts)?;
                } else {
                    action = self.eval_pipeline(&segment.pipeline)?;
                }

                if matches!(action, BuiltinAction::Exit(_)) {
                    break;
                }
            }

            // 次セグメントを実行するか（短絡）を、この区切り演算子と現在の終了ステータスで決める。
            skip_next = match segment.op {
                Some(ChainOp::And) => self.last_status != 0,
                Some(ChainOp::Or) => self.last_status == 0,
                Some(ChainOp::Semicolon) | None => false,
            };

            // errexit: && / || で連結されていない単純コマンドが失敗したら中断する。
            let standalone = matches!(segment.op, Some(ChainOp::Semicolon) | None);
            if self.errexit
                && self.suppress_errexit == 0
                && standalone
                && self.last_status != 0
                && !matches!(action, BuiltinAction::Exit(_))
            {
                return Ok(BuiltinAction::Exit(self.last_status));
            }
        }

        Ok(action)
    }

    fn eval_pipeline(&mut self, parsed: &ParsedLine) -> Result<BuiltinAction, MshError> {
        if !parsed.background && parsed.pipeline.len() == 1 && !parsed.pipeline[0].argv.is_empty() {
            if let Some((name, values)) = self.detect_array_assignment(&parsed.pipeline[0].argv) {
                let values =
                    self.expand_words(&values.iter().map(|v| v.to_string()).collect::<Vec<_>>())?;
                self.set_array(&name, values);
                self.last_status = 0;
                return Ok(BuiltinAction::Continue);
            }

            // 単語 1 個の要素代入 `name[key]=value`（連想配列/添字付き配列）。
            if parsed.pipeline[0].argv.len() == 1 {
                if let Some((name, key, value)) =
                    parse_element_assignment(&parsed.pipeline[0].argv[0].value)
                {
                    let key = self.expand_word_single(&key)?;
                    let value = self.expand_word_single(&value)?;
                    if self.current_assoc().contains_key(&name) {
                        self.set_assoc_element(&name, key, value);
                    } else {
                        self.set_indexed_element(&name, &key, value)?;
                    }
                    self.last_status = 0;
                    return Ok(BuiltinAction::Continue);
                }
            }
        }

        let mut parsed = self.expand_pipeline(parsed)?;
        self.apply_aliases(&mut parsed);

        if !parsed.background && parsed.pipeline.len() == 1 && !parsed.pipeline[0].argv.is_empty() {
            let argv = &parsed.pipeline[0].argv;
            let leading_assignments = argv
                .iter()
                .take_while(|arg| is_assignment_word(&arg.value))
                .count();
            if leading_assignments == argv.len() {
                for arg in argv {
                    if let Some((key, value)) = arg.value.split_once('=') {
                        self.set_var(key, value.to_string());
                    }
                }
                self.last_status = 0;
                return Ok(BuiltinAction::Continue);
            }

            let program = parsed.pipeline[0].argv[0].value.clone();
            let args: Vec<String> = parsed.pipeline[0].argv[1..]
                .iter()
                .map(|arg| arg.value.clone())
                .collect();

            if let Some(body) = self.functions.get(&program).cloned() {
                self.push_scope();
                let result = self.eval_stmts(&body);
                self.pop_scope();
                return result;
            }

            if program == "local" {
                self.run_local(args)?;
                self.last_status = 0;
                return Ok(BuiltinAction::Continue);
            }

            if program == "set" {
                self.run_set(&args)?;
                self.last_status = 0;
                return Ok(BuiltinAction::Continue);
            }

            if program == "declare" || program == "typeset" {
                self.run_declare(&args)?;
                self.last_status = 0;
                return Ok(BuiltinAction::Continue);
            }

            if matches!(program.as_str(), "pushd" | "popd" | "dirs") {
                self.run_dir_stack_builtin(&program, args)?;
                self.last_status = 0;
                return Ok(BuiltinAction::Continue);
            }

            if program == "[[" {
                let mut tokens = args;
                if tokens.last().map(String::as_str) == Some("]]") {
                    tokens.pop();
                }
                let status = crate::cond::eval(&tokens);
                if status == 2 {
                    return Err(MshError::ParseError(
                        "[[: unsupported or malformed conditional expression".into(),
                    ));
                }
                self.last_status = status;
                return Ok(BuiltinAction::Continue);
            }

            if builtins::is_builtin(&program) && !builtins::needs_shell_context(&program) {
                self.last_status = self.run_external(&parsed)?;
                return Ok(BuiltinAction::Continue);
            }

            if builtins::needs_shell_context(&program) {
                let action = self.run_context_builtin(&program, args)?;
                if !matches!(action, BuiltinAction::Exit(_)) {
                    self.last_status = 0;
                }
                return Ok(action);
            }
        }

        self.last_status = self.run_external(&parsed)?;
        Ok(BuiltinAction::Continue)
    }

    fn run_external(&mut self, parsed: &ParsedLine) -> Result<i32, MshError> {
        let statuses = exec::run_parsed_statuses(parsed, &self.heredoc_bodies)?;
        let pipestatus: Vec<String> = statuses.iter().map(|c| c.to_string()).collect();
        self.set_array("PIPESTATUS", pipestatus);
        Ok(statuses.last().copied().unwrap_or(0))
    }

    fn eval_stmts(&mut self, stmts: &[Stmt]) -> Result<BuiltinAction, MshError> {
        for stmt in stmts {
            match self.eval_stmt(stmt)? {
                LoopControl::Return(code) => return Ok(BuiltinAction::Exit(code)),
                LoopControl::Break | LoopControl::Continue => return Ok(BuiltinAction::Continue),
                LoopControl::None => {}
            }
        }
        Ok(BuiltinAction::Continue)
    }

    fn eval_stmt(&mut self, stmt: &Stmt) -> Result<LoopControl, MshError> {
        match stmt {
            Stmt::Command(line) => {
                let script = parse::parse_script(line)?;
                match self.eval_script(&script)? {
                    BuiltinAction::Continue => Ok(LoopControl::None),
                    BuiltinAction::Exit(code) => Ok(LoopControl::Return(code)),
                }
            }
            Stmt::FunctionDef { name, body } => {
                self.functions.insert(name.clone(), body.clone());
                Ok(LoopControl::None)
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                let status = self.eval_condition(condition)?;
                let body = if status == 0 {
                    then_body
                } else {
                    else_body.as_deref().unwrap_or(&[])
                };
                for inner in body {
                    match self.eval_stmt(inner)? {
                        LoopControl::Return(code) => return Ok(LoopControl::Return(code)),
                        other => {
                            if !matches!(other, LoopControl::None) {
                                return Ok(other);
                            }
                        }
                    }
                }
                Ok(LoopControl::None)
            }
            Stmt::For { var, items, body } => {
                let expanded_items = self.expand_words(items)?;
                'for_loop: for item in expanded_items {
                    self.set_var(var, item);
                    for inner in body {
                        match self.eval_stmt(inner)? {
                            LoopControl::Return(code) => return Ok(LoopControl::Return(code)),
                            LoopControl::Break => break 'for_loop,
                            LoopControl::Continue => continue 'for_loop,
                            LoopControl::None => {}
                        }
                    }
                }
                Ok(LoopControl::None)
            }
            Stmt::While { condition, body } => {
                let mut loop_status = 0;
                'while_loop: loop {
                    if self.eval_condition(condition)? != 0 {
                        self.last_status = loop_status;
                        break;
                    }
                    for inner in body {
                        match self.eval_stmt(inner)? {
                            LoopControl::Return(code) => return Ok(LoopControl::Return(code)),
                            LoopControl::Break => break 'while_loop,
                            LoopControl::Continue => continue 'while_loop,
                            LoopControl::None => {}
                        }
                    }
                    loop_status = self.last_status;
                }
                Ok(LoopControl::None)
            }
            Stmt::Case { word, arms } => {
                let value = self.expand_word_single(word)?;
                for arm in arms {
                    if arm
                        .patterns
                        .iter()
                        .any(|pattern| case_match(pattern, &value))
                    {
                        for inner in &arm.body {
                            match self.eval_stmt(inner)? {
                                LoopControl::Return(code) => return Ok(LoopControl::Return(code)),
                                LoopControl::Break => break,
                                LoopControl::Continue => continue,
                                LoopControl::None => {}
                            }
                        }
                        break;
                    }
                }
                Ok(LoopControl::None)
            }
            Stmt::Return { code } => {
                let status = if let Some(raw) = code {
                    self.expand_word_single(raw)?
                        .parse()
                        .unwrap_or(self.last_status)
                } else {
                    self.last_status
                };
                Ok(LoopControl::Return(status))
            }
            Stmt::Break => Ok(LoopControl::Break),
            Stmt::Continue => Ok(LoopControl::Continue),
        }
    }

    fn eval_condition(&mut self, condition: &str) -> Result<i32, MshError> {
        let script = parse::parse_script(condition)?;
        if script.segments.len() == 1 && script.segments[0].pipeline.pipeline.is_empty() {
            return Ok(1);
        }
        self.suppress_errexit += 1;
        let result = self.eval_script(&script);
        self.suppress_errexit -= 1;
        result?;
        Ok(self.last_status)
    }

    /// `--json -c` 用。コマンドを実行し stdout/stderr を捕捉して JSON 1 行で出力する。
    pub fn run_command_json(&mut self, command: &str) -> i32 {
        let (exit_code, json) = self.build_command_json(command);
        println!("{json}");
        self.finalize_agent_session();
        exit_code
    }

    /// JSON 1 行を組み立てる（`--json` / `--agent` / MCP 共用）。
    pub fn build_command_json(&mut self, command: &str) -> (i32, String) {
        let started = Instant::now();
        let (exit_code, stdout, stderr, error) = match self.capture_command(command) {
            Ok((code, out, err)) => (code, out, err, None),
            Err(e) => (1, String::new(), String::new(), Some(e)),
        };
        let duration_ms = started.elapsed().as_millis();
        let opts = self.json_output_options();
        let json =
            crate::command_json::build_command_json(&crate::command_json::CommandJsonInput {
                command,
                exit_code,
                duration_ms,
                stdout: &stdout,
                stderr: &stderr,
                error: error.as_ref(),
                opts,
                extra_fields: String::new(),
            });
        (exit_code, json)
    }

    fn json_output_options(&self) -> crate::command_json::JsonOutputOptions {
        crate::command_json::JsonOutputOptions {
            max_bytes: self.config.agent.json_max_bytes,
            include_meta: self.config.agent.include_meta,
        }
    }

    /// `--agent --json -c` 用。agent フィールド付き JSON を返す。
    pub fn run_command_agent_json(
        &mut self,
        command: &str,
        opts: crate::agent::AgentOptions,
    ) -> i32 {
        let settings = self.config.agent.clone();
        let force = opts.force || std::env::var("MSH_AGENT_FORCE").is_ok();
        let opts = crate::agent::AgentOptions {
            force,
            dry_run: opts.dry_run,
        };
        let assessment = crate::agent::assess(command);

        if opts.dry_run {
            let _ = crate::agent::write_audit(
                &settings,
                &crate::agent::AuditEntry {
                    command: command.to_string(),
                    action: "dry_run",
                    risk: assessment.risk,
                    exit_code: None,
                    reason: Some(assessment.reason.clone()),
                },
            );
            let json = crate::command_json::build_agent_dry_run_json(command, &assessment);
            println!("{json}");
            return 0;
        }

        if let Err(e) = crate::agent::gate(command, opts, &settings) {
            let _ = crate::agent::write_audit(
                &settings,
                &crate::agent::AuditEntry {
                    command: command.to_string(),
                    action: "blocked",
                    risk: assessment.risk,
                    exit_code: Some(1),
                    reason: Some(e.to_string()),
                },
            );
            let json = crate::command_json::build_blocked_json(&e, Some(&assessment));
            println!("{json}");
            return 1;
        }

        let (exit_code, mut json) = self.build_command_json(command);
        if json.ends_with('}') {
            json.truncate(json.len() - 1);
            json.push_str(&format!(
                ",\"action\":\"executed\",\"risk\":\"{}\"}}",
                crate::agent::risk_label(assessment.risk)
            ));
        }
        let _ = crate::agent::write_audit(
            &settings,
            &crate::agent::AuditEntry {
                command: command.to_string(),
                action: "executed",
                risk: assessment.risk,
                exit_code: Some(exit_code),
                reason: None,
            },
        );
        println!("{json}");
        self.finalize_agent_session();
        exit_code
    }

    /// stdout と stderr を同時に捕捉してコマンドを実行する（`--json` 用）。
    fn capture_command(&mut self, command: &str) -> Result<(i32, String, String), MshError> {
        #[cfg(unix)]
        {
            self.capture_with_tempfiles(command, true)
        }

        #[cfg(not(unix))]
        {
            let _ = command;
            Err(MshError::ScriptError(
                "--json is only supported on Unix".into(),
            ))
        }
    }

    fn eval_subshell(&mut self, command: &str) -> Result<String, MshError> {
        let (status, output) = self.eval_capture(command)?;
        self.last_status = status;
        Ok(output)
    }

    /// プロセス置換 `<(cmd)` / `>(cmd)` をファイルパスに展開する。
    fn eval_process_subst(&mut self, ps: &ProcessSubstitution) -> Result<String, MshError> {
        use std::sync::atomic::{AtomicU64, Ordering};

        match ps.mode {
            ProcessSubstMode::Input => {
                let output = self.eval_subshell(&ps.body)?;
                static PS_SEQ: AtomicU64 = AtomicU64::new(0);
                let seq = PS_SEQ.fetch_add(1, Ordering::Relaxed);
                let path = env::temp_dir().join(format!("msh-ps-{}-{seq}.out", std::process::id()));
                std::fs::write(&path, output).map_err(MshError::Io)?;
                Ok(path.to_string_lossy().into_owned())
            }
            ProcessSubstMode::Output => Err(MshError::UnsupportedSyntax {
                feature: "process substitution >( )".into(),
                workaround: "use bash -c 'your command' or a named pipe".into(),
            }),
        }
    }

    fn eval_capture(&mut self, command: &str) -> Result<(i32, String), MshError> {
        #[cfg(unix)]
        {
            let (status, out, _err) = self.capture_with_tempfiles(command, false)?;
            Ok((status, out))
        }

        #[cfg(not(unix))]
        {
            let _ = command;
            Err(MshError::ScriptError(
                "command substitution is only supported on Unix".into(),
            ))
        }
    }

    /// stdout（および `want_stderr` 時は stderr）を**一時ファイル**へリダイレクトして
    /// コマンドを実行し、終了後に内容を読み戻す。
    ///
    /// パイプではなくファイルを使うのは、大出力（>64KB のパイプバッファ）で
    /// 子プロセスがブロックし読み手が実行後に読むためデッドロックする問題を避けるため。
    #[cfg(unix)]
    fn capture_with_tempfiles(
        &mut self,
        command: &str,
        want_stderr: bool,
    ) -> Result<(i32, String, String), MshError> {
        use nix::unistd::{close, dup, dup2};
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::sync::atomic::{AtomicU64, Ordering};

        // ネストしたキャプチャでもファイル名が衝突しないよう一意 ID を採番。
        static CAPTURE_SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = CAPTURE_SEQ.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = env::temp_dir();
        let out_path = dir.join(format!("msh-cap-{pid}-{seq}.out"));
        let err_path = dir.join(format!("msh-cap-{pid}-{seq}.err"));

        let open = |path: &Path| -> Result<std::fs::File, MshError> {
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
                .map_err(MshError::Io)
        };

        let out_file = open(&out_path)?;
        let stdout = io::stdout();
        let saved_out = dup(stdout.as_raw_fd()).map_err(|e| MshError::Io(e.into()))?;
        dup2(out_file.as_raw_fd(), stdout.as_raw_fd()).map_err(|e| MshError::Io(e.into()))?;

        let (err_file, saved_err) = if want_stderr {
            let f = open(&err_path)?;
            let stderr = io::stderr();
            let saved = dup(stderr.as_raw_fd()).map_err(|e| MshError::Io(e.into()))?;
            dup2(f.as_raw_fd(), stderr.as_raw_fd()).map_err(|e| MshError::Io(e.into()))?;
            (Some(f), Some(saved))
        } else {
            (None, None)
        };

        let result = self.eval_line_inner(command);

        // 自前のバッファ済み出力を fd へ確実に送ってから復元する。
        let _ = io::stdout().flush();
        let _ = io::stderr().flush();

        dup2(saved_out, stdout.as_raw_fd()).map_err(|e| MshError::Io(e.into()))?;
        let _ = close(saved_out);
        if let Some(saved) = saved_err {
            dup2(saved, io::stderr().as_raw_fd()).map_err(|e| MshError::Io(e.into()))?;
            let _ = close(saved);
        }
        drop(out_file);
        drop(err_file);

        let out = std::fs::read_to_string(&out_path).unwrap_or_default();
        let err = if want_stderr {
            std::fs::read_to_string(&err_path).unwrap_or_default()
        } else {
            String::new()
        };
        let _ = std::fs::remove_file(&out_path);
        if want_stderr {
            let _ = std::fs::remove_file(&err_path);
        }

        match result {
            Ok(BuiltinAction::Continue) => Ok((self.last_status, out, err)),
            Ok(BuiltinAction::Exit(code)) => Ok((code, out, err)),
            Err(e) => Err(e),
        }
    }

    fn expand_pipeline(&mut self, parsed: &ParsedLine) -> Result<ParsedLine, MshError> {
        let mut parsed = parsed.clone();
        for stage in &mut parsed.pipeline {
            for arg in &mut stage.argv {
                if arg.literal {
                    continue;
                }
                arg.value = self.expand_substitutions(&arg.value)?;
            }
            for redirect in &mut stage.redirects {
                if !redirect.heredoc {
                    redirect.path = self.expand_substitutions(&redirect.path)?;
                }
            }
        }

        let pending = RefCell::new(Vec::new());
        let ctx = ExpandContext {
            last_status: self.last_status,
            shell_vars: self.current_scope(),
            arrays: self.current_arrays(),
            assoc: self.current_assoc(),
            nounset: self.nounset,
            pending_assigns: Some(&pending),
        };
        let parsed = exec::expand_pipeline_with(&parsed, &ctx)?;
        self.apply_pending_assigns(pending.into_inner());
        Ok(parsed)
    }

    fn apply_pending_assigns(&mut self, pending: Vec<(String, String)>) {
        for (key, value) in pending {
            self.set_var(&key, value);
        }
    }

    fn expand_word_list(&mut self, word: &str) -> Result<Vec<String>, MshError> {
        let expanded = self.expand_substitutions(word)?;
        let pending = RefCell::new(Vec::new());
        let ctx = ExpandContext {
            last_status: self.last_status,
            shell_vars: self.current_scope(),
            arrays: self.current_arrays(),
            assoc: self.current_assoc(),
            nounset: self.nounset,
            pending_assigns: Some(&pending),
        };
        let words = expand::expand_word_with(&expanded, &ctx)?;
        self.apply_pending_assigns(pending.into_inner());
        Ok(words)
    }

    fn expand_words(&mut self, words: &[String]) -> Result<Vec<String>, MshError> {
        let mut expanded = Vec::new();
        for word in words {
            expanded.extend(self.expand_word_list(word)?);
        }
        Ok(expanded)
    }

    fn expand_substitutions(&mut self, input: &str) -> Result<String, MshError> {
        let mut current = input.to_string();
        for _ in 0..64 {
            if let Some(ps) = find_process_substitution(&current) {
                let path = self.eval_process_subst(&ps)?;
                current.replace_range(ps.range, &path);
                continue;
            }
            if let Some(subst) = find_command_substitution(&current) {
                let replacement = self.eval_subshell(&subst.body)?;
                // POSIX: コマンド置換は末尾の改行をすべて除去する。
                let replacement = replacement.trim_end_matches('\n');
                current.replace_range(subst.range, replacement);
            } else {
                return Ok(current);
            }
        }
        Err(MshError::ScriptError(
            "too many nested command substitutions".into(),
        ))
    }

    fn expand_word_single(&mut self, word: &str) -> Result<String, MshError> {
        Ok(self.expand_word_list(word)?.join(" "))
    }

    fn run_set(&mut self, args: &[String]) -> Result<(), MshError> {
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            match arg.as_str() {
                "-e" => self.errexit = true,
                "+e" => self.errexit = false,
                "-u" => self.nounset = true,
                "+u" => self.nounset = false,
                "-x" | "+x" | "--" => {}
                "-o" | "+o" => {
                    let enable = arg == "-o";
                    if let Some(opt) = args.get(i + 1) {
                        match opt.as_str() {
                            "errexit" => self.errexit = enable,
                            "nounset" => self.nounset = enable,
                            "pipefail" => {}
                            _ => {}
                        }
                        i += 1;
                    }
                }
                other if other.starts_with('-') || other.starts_with('+') => {}
                _ => {}
            }
            i += 1;
        }
        Ok(())
    }

    fn run_declare(&mut self, args: &[String]) -> Result<(), MshError> {
        let mut assoc = false;
        let mut indexed = false;
        let mut names: Vec<&String> = Vec::new();
        for arg in args {
            if arg.len() > 1 && arg.starts_with('-') {
                for ch in arg[1..].chars() {
                    match ch {
                        'A' => assoc = true,
                        'a' => indexed = true,
                        // -g/-x/-r/-i/-l/-u などは型宣言として無視（値の格納のみ行う）。
                        _ => {}
                    }
                }
            } else {
                names.push(arg);
            }
        }

        for name in names {
            if let Some((base, key, value)) = parse_element_assignment(name) {
                let key = self.expand_word_single(&key)?;
                let value = self.expand_word_single(&value)?;
                if assoc {
                    self.set_assoc_element(&base, key, value);
                } else {
                    self.set_indexed_element(&base, &key, value)?;
                }
            } else if let Some((key, value)) = name.split_once('=') {
                let value = self.expand_word_single(value)?;
                self.set_var(key, value);
            } else if assoc {
                self.declare_assoc(name);
            } else if indexed {
                if let Some(scope) = self.arrays.last_mut() {
                    scope.entry(name.to_string()).or_default();
                }
            }
        }
        Ok(())
    }

    fn run_local(&mut self, args: Vec<String>) -> Result<(), MshError> {
        for arg in args {
            let Some((key, value)) = arg.split_once('=') else {
                return Err(MshError::ScriptError(format!(
                    "local: expected NAME=VALUE, got '{arg}'"
                )));
            };
            self.set_var(key, value.to_string());
        }
        Ok(())
    }

    fn set_var(&mut self, key: &str, value: String) {
        if let Some(scope) = self.shell_vars.last_mut() {
            scope.insert(key.to_string(), value);
        }
    }

    fn set_array(&mut self, key: &str, values: Vec<String>) {
        if let Some(scope) = self.arrays.last_mut() {
            scope.insert(key.to_string(), values);
        }
    }

    fn current_scope(&self) -> &HashMap<String, String> {
        self.shell_vars.last().expect("scope stack empty")
    }

    fn current_arrays(&self) -> &HashMap<String, Vec<String>> {
        self.arrays.last().expect("array scope stack empty")
    }

    fn current_assoc(&self) -> &expand::AssocArrays {
        self.assoc.last().expect("assoc scope stack empty")
    }

    fn set_assoc_element(&mut self, name: &str, key: String, value: String) {
        if let Some(scope) = self.assoc.last_mut() {
            scope
                .entry(name.to_string())
                .or_default()
                .insert(key, value);
        }
    }

    fn declare_assoc(&mut self, name: &str) {
        if let Some(scope) = self.assoc.last_mut() {
            scope.entry(name.to_string()).or_default();
        }
    }

    fn set_indexed_element(
        &mut self,
        name: &str,
        key: &str,
        value: String,
    ) -> Result<(), MshError> {
        let idx = key.parse::<usize>().map_err(|_| {
            MshError::ScriptError(format!("{name}: {key}: array subscript must be an integer"))
        })?;
        if let Some(scope) = self.arrays.last_mut() {
            let vec = scope.entry(name.to_string()).or_default();
            if idx >= vec.len() {
                vec.resize(idx + 1, String::new());
            }
            vec[idx] = value;
        }
        Ok(())
    }

    fn push_scope(&mut self) {
        self.shell_vars.push(HashMap::new());
        self.arrays.push(HashMap::new());
        self.assoc.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        if self.shell_vars.len() > 1 {
            self.shell_vars.pop();
        }
        if self.arrays.len() > 1 {
            self.arrays.pop();
        }
        if self.assoc.len() > 1 {
            self.assoc.pop();
        }
    }

    fn detect_array_assignment(&self, argv: &[Arg]) -> Option<(String, Vec<String>)> {
        if argv.is_empty() {
            return None;
        }
        if argv.len() == 1 {
            return expand::parse_array_assignment(&argv[0].value);
        }
        if argv[0].value.contains("=(") {
            let joined = argv
                .iter()
                .map(|arg| arg.value.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            return expand::parse_array_assignment(&joined);
        }
        None
    }

    fn run_dir_stack_builtin(&mut self, program: &str, args: Vec<String>) -> Result<(), MshError> {
        match program {
            "pushd" => dir_stack::push(&mut self.dir_stack, &args),
            "popd" => dir_stack::pop(&mut self.dir_stack),
            "dirs" => dir_stack::dirs(&self.dir_stack),
            _ => unreachable!(),
        }
    }

    fn run_context_builtin(
        &mut self,
        program: &str,
        args: Vec<String>,
    ) -> Result<BuiltinAction, MshError> {
        match program {
            "exit" => {
                // bash 互換: 引数なしの `exit` は直前コマンドの終了ステータスを使う。
                let code = match args.first() {
                    Some(arg) => arg.parse::<i32>().unwrap_or(1),
                    None => self.last_status,
                };
                Ok(BuiltinAction::Exit(code))
            }
            "alias" => {
                builtins::alias::run(&args, &mut self.aliases)?;
                self.alias_fingerprint = alias_fingerprint(&self.aliases);
                Ok(BuiltinAction::Continue)
            }
            "source" | "." => {
                let Some(path) = args.first() else {
                    return Err(MshError::ParseError("source: filename required".into()));
                };
                for line in builtins::source::read_lines(path)? {
                    match self.eval_line(&line, false)? {
                        BuiltinAction::Continue => {}
                        BuiltinAction::Exit(code) => return Ok(BuiltinAction::Exit(code)),
                    }
                }
                Ok(BuiltinAction::Continue)
            }
            "export" => {
                if args.iter().all(|arg| !arg.contains('=')) {
                    return builtins::run("export", &args);
                }
                for arg in &args {
                    let Some((key, value)) = arg.split_once('=') else {
                        return Err(MshError::InvalidExport(format!(
                            "expected NAME=VALUE, got '{arg}'"
                        )));
                    };
                    let expanded = self.expand_word_single(value)?;
                    env::set_var(key, expanded);
                }
                Ok(BuiltinAction::Continue)
            }
            "help" => Ok(builtins::run("help", &args)?),
            "ai" => {
                self.run_ai(args)?;
                Ok(BuiltinAction::Continue)
            }
            "explain" => {
                self.run_explain(args)?;
                Ok(BuiltinAction::Continue)
            }
            "prompt" => {
                self.run_prompt(args)?;
                Ok(BuiltinAction::Continue)
            }
            _ => builtins::run(program, &args),
        }
    }

    /// AI へ system/user プロンプトを送り応答を返す共通ヘルパ。
    fn ask_ai(&self, system: &str, user: &str) -> Result<String, MshError> {
        let client = crate::ai::AiClient::new(&self.config.ai);
        client.complete(system, user)
    }

    /// `ai <prompt...>` — モデルの応答を表示するだけで、コマンドは実行しない（A-1 安全枠）。
    fn run_ai(&mut self, args: Vec<String>) -> Result<(), MshError> {
        if args.is_empty() {
            return Err(MshError::ScriptError(
                "ai: プロンプトを指定してください（例: ai このディレクトリのRustファイル数を数えるコマンドは?）".into(),
            ));
        }
        let prompt = args.join(" ");
        let system = "You are a concise shell assistant for the msh shell on a Unix system. \
Answer briefly. When asked for a command, show only the command in a single line. \
Do not execute anything; the user reviews and runs commands themselves.";
        let reply = self.ask_ai(system, &prompt)?;
        println!("{reply}");
        Ok(())
    }

    /// `explain [command...]` — 指定コマンド、または直前に実行したコマンドを AI が解説する。
    /// 直前コマンドが非ゼロ終了していた場合は終了コードも添えて「なぜ失敗したか」を尋ねる。
    fn run_explain(&mut self, args: Vec<String>) -> Result<(), MshError> {
        let system = "You are a concise Unix shell tutor. Explain what the given command does, \
flag by flag if helpful. If an exit code is provided and non-zero, explain the most likely \
cause of failure and how to fix it. Be brief and practical.";

        let user = if !args.is_empty() {
            format!("Explain this command:\n{}", args.join(" "))
        } else if !self.last_command.is_empty() {
            if self.last_status != 0 {
                format!(
                    "The previous command failed with exit code {}. Explain why it likely failed and how to fix it:\n{}",
                    self.last_status, self.last_command
                )
            } else {
                format!("Explain this command:\n{}", self.last_command)
            }
        } else {
            return Err(MshError::ScriptError(
                "explain: 解説するコマンドがありません（例: explain tar -xzvf a.tgz）".into(),
            ));
        };

        let reply = self.ask_ai(system, &user)?;
        println!("{reply}");
        Ok(())
    }

    /// `prompt` / `prompt config` — 対話式プロンプト設定。`prompt preview` で現在の見た目を表示。
    fn run_prompt(&mut self, args: Vec<String>) -> Result<(), MshError> {
        match args.first().map(String::as_str) {
            Some("preview") => {
                let line = prompt::render(prompt::RenderContext {
                    last_status: self.last_status,
                    last_duration: self.last_duration,
                    cache: &mut self.prompt_cache,
                    settings: &self.config.prompt,
                    theme: self.config.theme,
                });
                println!("{line}");
            }
            Some("config") | None => {
                let lang = self.config.language;
                crate::prompt_setup::run(&mut self.config, &mut self.prompt_cache, lang)?;
            }
            Some(other) => {
                return Err(MshError::ScriptError(format!(
                    "prompt: unknown subcommand '{other}' (try: prompt config, prompt preview)"
                )));
            }
        }
        Ok(())
    }

    fn suggest_from_comment(&mut self, request: &str) -> Result<(), MshError> {
        let system = "You translate a natural-language request into a single Unix shell command \
for the msh shell. Output ONLY the command on one line, with no explanation, no markdown, \
no code fences. If impossible, output a single echo with a short message.";
        let command = self.ask_ai(system, request)?;
        let command = sanitize_suggested_command(&command);
        if command.is_empty() {
            return Err(MshError::ScriptError(
                "ai: コマンド案を取得できませんでした".into(),
            ));
        }
        eprintln!("\x1b[90m  ↳ 提案（Enter で実行・編集可）\x1b[0m");
        self.pending_prefill = Some(command);
        Ok(())
    }

    fn apply_aliases(&self, parsed: &mut ParsedLine) {
        for stage in &mut parsed.pipeline {
            if stage.argv.is_empty() {
                continue;
            }
            let command = &stage.argv[0].value;
            if let Some(expansion) = self.aliases.get(command) {
                let mut replacement: Vec<Arg> = tokenize_alias(expansion)
                    .into_iter()
                    .map(|value| Arg {
                        value,
                        literal: false,
                    })
                    .collect();
                replacement.extend(stage.argv[1..].iter().cloned());
                stage.argv = replacement;
            }
        }
    }

    fn load_rc(&mut self) -> Result<(), MshError> {
        if let Ok(home) = env::var("HOME") {
            let home_path = PathBuf::from(&home);

            let env_path = home_path.join(".msh_env");
            if env_path.is_file() {
                self.load_file(&env_path)?;
            }

            let msh_config_dir = home_path.join(".config").join("msh");
            let msh_config = msh_config_dir.join("config.toml");
            if !msh_config.is_file() {
                let _ = std::fs::create_dir_all(&msh_config_dir);
            }

            if self.config.load_bashrc {
                let bashrc = home_path.join(".bashrc");
                if bashrc.is_file() {
                    self.load_file(&bashrc)?;
                }
            }

            if self.config.load_zshrc {
                let zshrc = home_path.join(".zshrc");
                if zshrc.is_file() {
                    self.load_file(&zshrc)?;
                }
            }

            let rc_path = home_path.join(".mshrc");
            if rc_path.is_file() {
                self.load_file(&rc_path)?;
            }

            for plugin in config::plugin_paths(&home_path) {
                self.load_file(&plugin)?;
            }
        }

        let local_rc = PathBuf::from(".mshrc");
        if local_rc.is_file() {
            self.load_file(&local_rc)?;
        }

        Ok(())
    }

    fn load_file(&mut self, path: &Path) -> Result<(), MshError> {
        let path = path.to_string_lossy();
        for line in builtins::source::read_lines(&path)? {
            match self.eval_line(&line, false)? {
                BuiltinAction::Continue => {}
                BuiltinAction::Exit(_) => break,
            }
        }
        Ok(())
    }

    fn session_state(&self) -> SessionState {
        SessionState {
            cwd: env::current_dir()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            dir_stack: self.dir_stack.clone(),
        }
    }

    fn save_session(&self) {
        if !self.config.session_restore {
            return;
        }
        let Some(config_dir) = ShellConfig::config_dir() else {
            return;
        };
        let path = session::session_path(&config_dir);
        let _ = session::save(&self.session_state(), &path);
    }

    fn restore_session(&mut self) -> Result<(), MshError> {
        let Some(config_dir) = ShellConfig::config_dir() else {
            return Ok(());
        };
        let path = session::session_path(&config_dir);
        let Some(state) = session::load(&path)? else {
            return Ok(());
        };
        session::restore(&state)?;
        self.dir_stack = state.dir_stack;
        Ok(())
    }

    fn maybe_init_atuin(&mut self) -> Result<(), MshError> {
        if self.config.history_backend != HistoryBackend::Atuin {
            return Ok(());
        }
        let output = std::process::Command::new("atuin")
            .args(["init", "-s", "msh"])
            .output()
            .map_err(|e| MshError::ScriptError(format!("atuin: {e}")))?;
        if !output.status.success() {
            return Err(MshError::ScriptError(
                "atuin init failed — is atuin installed?".into(),
            ));
        }
        let script = String::from_utf8_lossy(&output.stdout);
        for line in script.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            match self.eval_line(trimmed, false)? {
                BuiltinAction::Continue => {}
                BuiltinAction::Exit(_) => break,
            }
        }
        Ok(())
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

fn is_assignment_word(word: &str) -> bool {
    let Some((key, _)) = word.split_once('=') else {
        return false;
    };
    if key.is_empty() {
        return false;
    }
    let mut chars = key.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `name[key]=value` を (name, key, value) に分解する。`name` は識別子、`key` は非空。
fn parse_element_assignment(word: &str) -> Option<(String, String, String)> {
    let open = word.find('[')?;
    let close = word.find(']')?;
    if close <= open + 1 {
        return None;
    }
    let name = &word[..open];
    if !is_identifier(name) {
        return None;
    }
    let key = &word[open + 1..close];
    let value = word[close + 1..].strip_prefix('=')?;
    Some((name.to_string(), key.to_string(), value.to_string()))
}

fn needs_multiline_block(line: &str, kind: &script::BlockKind) -> bool {
    match kind {
        script::BlockKind::Function { .. } => !line.contains('}'),
        script::BlockKind::If => !line.contains("fi"),
        script::BlockKind::For => !line.contains("done"),
        script::BlockKind::While => !line.contains("done"),
        script::BlockKind::Case => !line.contains("esac"),
    }
}

fn open_brace_depth(line: &str) -> usize {
    line.chars().filter(|&c| c == '{').count()
}

fn case_match(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    pattern == value
}

struct CommandSubstitution {
    range: std::ops::Range<usize>,
    body: String,
}

enum ProcessSubstMode {
    Input,
    Output,
}

struct ProcessSubstitution {
    range: std::ops::Range<usize>,
    body: String,
    mode: ProcessSubstMode,
}

fn find_process_substitution(input: &str) -> Option<ProcessSubstitution> {
    let bytes = input.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if (bytes[i] == b'<' || bytes[i] == b'>') && bytes[i + 1] == b'(' {
            let mode = if bytes[i] == b'<' {
                ProcessSubstMode::Input
            } else {
                ProcessSubstMode::Output
            };
            let mut depth = 1;
            let mut j = i + 2;
            while j < bytes.len() {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(ProcessSubstitution {
                                range: i..j + 1,
                                body: input[i + 2..j].to_string(),
                                mode,
                            });
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            return None;
        }
        i += 1;
    }
    None
}

fn find_command_substitution(input: &str) -> Option<CommandSubstitution> {
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            if let Some(end) = input[i + 1..].find('`') {
                let end = i + 1 + end;
                return Some(CommandSubstitution {
                    range: i..end + 1,
                    body: input[i + 1..end].to_string(),
                });
            }
            return None;
        }
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'(' {
            if i + 2 < bytes.len() && bytes[i + 2] == b'(' {
                i += 1;
                continue;
            }
            let mut depth = 1;
            let mut j = i + 2;
            while j < bytes.len() {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(CommandSubstitution {
                                range: i..j + 1,
                                body: input[i + 2..j].to_string(),
                            });
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            return None;
        }
        i += 1;
    }
    None
}

fn tokenize_alias(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let chars = input.chars();
    let mut in_single = false;
    let mut in_double = false;

    for ch in chars {
        if in_single {
            if ch == '\'' {
                in_single = false;
            } else {
                current.push(ch);
            }
            continue;
        }
        if in_double {
            if ch == '"' {
                in_double = false;
            } else {
                current.push(ch);
            }
            continue;
        }
        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

/// AI が返したコマンド案から、コードフェンスや前置きを除いて 1 行に正規化する。
fn sanitize_suggested_command(reply: &str) -> String {
    for raw in reply.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("```") {
            continue;
        }
        // `$ cmd` のようなプロンプト記号を除去。
        let line = line.strip_prefix("$ ").unwrap_or(line);
        return line.to_string();
    }
    String::new()
}

fn alias_fingerprint(aliases: &HashMap<String, String>) -> u64 {
    let mut hasher = DefaultHasher::new();
    aliases.len().hash(&mut hasher);
    for (key, value) in aliases {
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::tokenize_alias;

    #[test]
    fn tokenize_alias_simple() {
        assert_eq!(tokenize_alias("ls -la"), vec!["ls", "-la"]);
    }
}
