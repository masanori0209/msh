use crate::ci_history::{find_history_prefix, CaseInsensitiveHistory};
use crate::complete::MshCompleter;
use crate::config::Language;
use crate::highlight;
use crate::hints;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::{Hint, Hinter};
use rustyline::validate::{
    MatchingBracketValidator, ValidationContext, ValidationResult, Validator,
};
use rustyline::{ColorMode, CompletionType, Config, Context, Editor, Helper};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

const MAX_HISTORY_SIZE: usize = 1000;
const MAX_DIR_COMMANDS: usize = 64;

pub struct HistoryPreviewHint {
    completion: String,
    display: String,
}

impl Hint for HistoryPreviewHint {
    fn display(&self) -> &str {
        &self.display
    }

    fn completion(&self) -> Option<&str> {
        if self.completion.is_empty() {
            None
        } else {
            Some(&self.completion)
        }
    }
}

/// ディレクトリ文脈を考慮した autosuggestion。
/// 「このディレクトリで実際に打ったコマンド」を最優先で提案し、
/// 無ければグローバル履歴の前方一致（大小無視）にフォールバックする。
struct SmartHinter {
    dir_history: RefCell<HashMap<String, VecDeque<String>>>,
}

impl SmartHinter {
    fn new() -> Self {
        Self {
            dir_history: RefCell::new(HashMap::new()),
        }
    }

    fn current_dir() -> String {
        env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// 受理した行を現在ディレクトリの履歴へ記録する（重複は最新へ繰り上げ）。
    fn record(&self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let dir = Self::current_dir();
        let mut map = self.dir_history.borrow_mut();
        let entries = map.entry(dir).or_default();
        if let Some(pos) = entries.iter().position(|c| c == line) {
            entries.remove(pos);
        }
        entries.push_back(line.to_string());
        while entries.len() > MAX_DIR_COMMANDS {
            entries.pop_front();
        }
    }

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<HistoryPreviewHint> {
        if line.is_empty() || pos < line.len() {
            return None;
        }

        // 1) 現在ディレクトリで実際に使ったコマンドを最優先（新しい順）。
        let dir = Self::current_dir();
        let dir_match = {
            let map = self.dir_history.borrow();
            map.get(&dir)
                .and_then(|entries| pick_dir_suggestion(line, entries))
        };

        // 2) 無ければグローバル履歴の前方一致（大小無視）。
        let entry = match dir_match {
            Some(entry) => entry,
            None => find_history_prefix(ctx.history(), line)?,
        };

        let suffix = entry.get(pos..).unwrap_or("").to_owned();
        if !suffix.is_empty() {
            return Some(HistoryPreviewHint {
                completion: suffix.clone(),
                display: format!("{suffix}  ↳ {entry}"),
            });
        }

        Some(HistoryPreviewHint {
            completion: String::new(),
            display: format!("↳ {entry}"),
        })
    }
}

/// ディレクトリ履歴から、行を接頭辞に持つ最も新しいコマンドを選ぶ。
fn pick_dir_suggestion(line: &str, entries: &VecDeque<String>) -> Option<String> {
    entries
        .iter()
        .rev()
        .find(|cmd| cmd.len() > line.len() && cmd.starts_with(line))
        .cloned()
}

pub struct MshHelper {
    highlighter: MshHighlighter,
    completer: MshCompleter,
    hinter: SmartHinter,
    validator: MatchingBracketValidator,
}

struct MshHighlighter;

impl Highlighter for MshHighlighter {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Owned(highlight::highlight_line(line))
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _kind: CmdKind) -> bool {
        true
    }
}

impl Completer for MshHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        self.completer.complete(line, pos, ctx)
    }
}

impl Hinter for MshHelper {
    type Hint = HistoryPreviewHint;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<HistoryPreviewHint> {
        self.hinter.hint(line, pos, ctx)
    }
}

impl Highlighter for MshHelper {
    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        self.highlighter.highlight(line, pos)
    }

    fn highlight_char(&self, _line: &str, _pos: usize, kind: CmdKind) -> bool {
        self.highlighter.highlight_char(_line, _pos, kind)
    }
}

impl Validator for MshHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        self.validator.validate(ctx)
    }

    fn validate_while_typing(&self) -> bool {
        self.validator.validate_while_typing()
    }
}

impl Helper for MshHelper {}

pub struct LineEditor {
    editor: Editor<MshHelper, CaseInsensitiveHistory>,
}

pub fn is_interactive_input() -> bool {
    io::stdin().is_terminal()
}

impl LineEditor {
    pub fn new(aliases: &HashMap<String, String>, fuzzy: bool) -> io::Result<Self> {
        let config = Config::builder()
            .color_mode(ColorMode::Enabled)
            .completion_type(CompletionType::List)
            .max_history_size(MAX_HISTORY_SIZE)
            .map_err(io::Error::other)?
            .history_ignore_dups(true)
            .map_err(io::Error::other)?
            .build();

        let history = CaseInsensitiveHistory::with_config(config);
        let mut editor = Editor::with_history(config, history).map_err(io::Error::other)?;
        editor.set_helper(Some(MshHelper {
            highlighter: MshHighlighter,
            completer: MshCompleter::new(aliases, fuzzy),
            hinter: SmartHinter::new(),
            validator: MatchingBracketValidator::new(),
        }));

        if let Ok(home) = env::var("HOME") {
            let path = PathBuf::from(home).join(".msh_history");
            let _ = editor.load_history(&path);
        }

        Ok(Self { editor })
    }

    pub fn refresh(&mut self, aliases: &HashMap<String, String>) {
        if let Some(helper) = self.editor.helper_mut() {
            helper.completer.refresh(aliases);
        }
    }

    pub fn read_line(&mut self, prompt: &str) -> io::Result<Option<String>> {
        let result = self.editor.readline(prompt);
        self.read_result(result)
    }

    /// 入力欄にあらかじめテキストを挿入して読み取る（NL→コマンド提案の編集用）。
    pub fn read_line_with_initial(
        &mut self,
        prompt: &str,
        initial: &str,
    ) -> io::Result<Option<String>> {
        let result = self.editor.readline_with_initial(prompt, (initial, ""));
        self.read_result(result)
    }

    fn read_result(&mut self, result: rustyline::Result<String>) -> io::Result<Option<String>> {
        match result {
            Ok(line) => {
                if !line.trim().is_empty() {
                    let _ = self.editor.add_history_entry(line.as_str());
                    if let Some(helper) = self.editor.helper() {
                        helper.hinter.record(&line);
                    }
                }
                Ok(Some(line))
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                Ok(None)
            }
            Err(ReadlineError::Eof) => Ok(None),
            Err(err) => Err(io::Error::other(err)),
        }
    }

    pub fn save_history(&mut self) {
        if let Ok(home) = env::var("HOME") {
            let path = PathBuf::from(home).join(".msh_history");
            let _ = self.editor.save_history(&path);
        }
    }
}

pub fn read_plain_line(prompt: &str) -> io::Result<Option<String>> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut line = String::new();
    let bytes = io::stdin().read_line(&mut line)?;
    if bytes == 0 {
        return Ok(None);
    }
    Ok(Some(line))
}

pub fn report_error(err: &crate::error::MshError, language: Language) {
    eprintln!("{}", hints::format_error(err, language));
}

#[cfg(test)]
mod tests {
    use super::{is_interactive_input, pick_dir_suggestion};
    use std::collections::VecDeque;

    #[test]
    fn stdin_is_not_terminal_in_tests() {
        assert!(!is_interactive_input());
    }

    #[test]
    fn dir_suggestion_prefers_most_recent_prefix_match() {
        let entries: VecDeque<String> = ["cargo build", "cargo test", "git status"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // 新しい順（末尾が最新）に走査するため "cargo test" が先に一致する。
        assert_eq!(
            pick_dir_suggestion("cargo ", &entries).as_deref(),
            Some("cargo test")
        );
        assert_eq!(
            pick_dir_suggestion("git", &entries).as_deref(),
            Some("git status")
        );
    }

    #[test]
    fn dir_suggestion_ignores_exact_and_nonmatching() {
        let entries: VecDeque<String> = ["cargo build"].iter().map(|s| s.to_string()).collect();
        // 完全一致（補完すべき接尾辞なし）は提案しない。
        assert_eq!(pick_dir_suggestion("cargo build", &entries), None);
        assert_eq!(pick_dir_suggestion("npm", &entries), None);
    }
}
