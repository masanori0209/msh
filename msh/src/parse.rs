use crate::error::{MshError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdin,
    Stdout,
    Stderr,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    Truncate,
    Append,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg {
    pub value: String,
    pub literal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    pub stream: Stream,
    pub mode: OpenMode,
    pub path: String,
    pub heredoc: bool,
}

impl Redirect {
    pub fn file(stream: Stream, mode: OpenMode, path: String) -> Self {
        Self {
            stream,
            mode,
            path,
            heredoc: false,
        }
    }

    pub fn heredoc(delimiter: String) -> Self {
        Self {
            stream: Stream::Stdin,
            mode: OpenMode::Truncate,
            path: format!("__MSH_HD__:{delimiter}"),
            heredoc: true,
        }
    }

    pub fn heredoc_delimiter(&self) -> Option<&str> {
        self.path.strip_prefix("__MSH_HD__:")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub argv: Vec<Arg>,
    pub redirects: Vec<Redirect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLine {
    pub pipeline: Vec<CommandSpec>,
    pub background: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainOp {
    Semicolon,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptSegment {
    pub pipeline: ParsedLine,
    pub op: Option<ChainOp>,
    /// チェイン分割後の原文（`;` / `do`/`done` 等を保持）
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedScript {
    pub segments: Vec<ScriptSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    LiteralWord(String),
    Pipe,
    Background,
    RedirectStdin,
    RedirectHeredoc,
    RedirectStdout(OpenMode),
    RedirectStderr(OpenMode),
    RedirectBoth(OpenMode),
    Semicolon,
    And,
    Or,
}

pub fn parse_script(line: &str) -> Result<ParsedScript> {
    let tokens = lex(line.trim())?;
    if tokens.is_empty() {
        return Ok(ParsedScript { segments: vec![] });
    }

    let chain_segments = split_chain(&tokens);
    let mut segments = Vec::with_capacity(chain_segments.len());

    for (segment_tokens, op) in chain_segments {
        let source = detokenize(&segment_tokens);
        let pipeline = if segment_is_compound(&segment_tokens) {
            ParsedLine {
                pipeline: vec![],
                background: false,
            }
        } else {
            parse_pipeline_tokens(segment_tokens)?
        };
        segments.push(ScriptSegment {
            pipeline,
            op,
            source,
        });
    }

    Ok(ParsedScript { segments })
}

pub fn parse_line(line: &str) -> Result<ParsedLine> {
    let script = parse_script(line)?;
    if script.segments.len() > 1 {
        return Err(MshError::ParseError(
            "multiple statements require eval_script; use parse_script instead".into(),
        ));
    }

    Ok(script
        .segments
        .into_iter()
        .next()
        .map(|segment| segment.pipeline)
        .unwrap_or(ParsedLine {
            pipeline: vec![],
            background: false,
        }))
}

fn split_chain(tokens: &[Token]) -> Vec<(Vec<Token>, Option<ChainOp>)> {
    let mut segments = Vec::new();
    let mut current = Vec::new();
    let mut compound = CompoundDepth::default();

    for token in tokens {
        match token {
            Token::Semicolon if compound.allows_split() => {
                segments.push((std::mem::take(&mut current), Some(ChainOp::Semicolon)));
            }
            Token::Semicolon => {
                current.push(token.clone());
            }
            Token::And if compound.allows_split() => {
                segments.push((std::mem::take(&mut current), Some(ChainOp::And)));
            }
            Token::And => {
                current.push(token.clone());
            }
            Token::Or if compound.allows_split() => {
                segments.push((std::mem::take(&mut current), Some(ChainOp::Or)));
            }
            Token::Or => {
                current.push(token.clone());
            }
            other => {
                if let Some(word) = token_word(other) {
                    compound.observe_word(word);
                }
                current.push(other.clone());
            }
        }
    }

    if !current.is_empty() {
        segments.push((current, None));
    }

    segments
}

#[derive(Default)]
struct CompoundDepth {
    r#loop: u32,
    if_depth: u32,
    case_depth: u32,
}

impl CompoundDepth {
    fn allows_split(&self) -> bool {
        self.r#loop == 0 && self.if_depth == 0 && self.case_depth == 0
    }

    fn observe_word(&mut self, word: &str) {
        match word {
            "while" | "for" | "until" => self.r#loop += 1,
            "done" => self.r#loop = self.r#loop.saturating_sub(1),
            "if" => self.if_depth += 1,
            "fi" => self.if_depth = self.if_depth.saturating_sub(1),
            "case" => self.case_depth += 1,
            "esac" => self.case_depth = self.case_depth.saturating_sub(1),
            _ => {}
        }
    }
}

fn token_word(token: &Token) -> Option<&str> {
    match token {
        Token::Word(word) | Token::LiteralWord(word) => Some(word.as_str()),
        _ => None,
    }
}

/// トークン列をスクリプト断片文字列に戻す（複合文の `;` / `do`/`done` 保持用）。
fn segment_is_compound(tokens: &[Token]) -> bool {
    matches!(
        tokens.first(),
        Some(Token::Word(w)) if matches!(w.as_str(), "while" | "for" | "until" | "if" | "case")
    )
}

fn detokenize(tokens: &[Token]) -> String {
    let mut out = String::new();
    for (i, token) in tokens.iter().enumerate() {
        if i > 0 {
            let prev = &tokens[i - 1];
            let need_space = !matches!(
                token,
                Token::Semicolon | Token::And | Token::Or | Token::Pipe | Token::Background
            ) && !matches!(
                prev,
                Token::Semicolon | Token::And | Token::Or | Token::Pipe | Token::Background
            );
            if need_space {
                out.push(' ');
            }
        }
        match token {
            Token::Word(w) | Token::LiteralWord(w) => out.push_str(w),
            Token::Pipe => out.push('|'),
            Token::Background => out.push('&'),
            Token::Semicolon => {
                out.push(';');
                out.push(' ');
            }
            Token::And => out.push_str("&& "),
            Token::Or => out.push_str("|| "),
            Token::RedirectStdin => out.push('<'),
            Token::RedirectHeredoc => out.push_str("<<"),
            Token::RedirectStdout(OpenMode::Truncate) => out.push('>'),
            Token::RedirectStdout(OpenMode::Append) => out.push_str(">>"),
            Token::RedirectStderr(OpenMode::Truncate) => out.push_str("2>"),
            Token::RedirectStderr(OpenMode::Append) => out.push_str("2>>"),
            Token::RedirectBoth(OpenMode::Truncate) => out.push_str(">&"),
            Token::RedirectBoth(OpenMode::Append) => out.push_str(">>&"),
        }
    }
    out.trim().to_string()
}

fn parse_pipeline_tokens(tokens: Vec<Token>) -> Result<ParsedLine> {
    let mut tokens = tokens;
    let background = matches!(tokens.last(), Some(Token::Background));
    if background {
        tokens.pop();
    }

    if tokens.is_empty() {
        return Ok(ParsedLine {
            pipeline: vec![],
            background: false,
        });
    }

    let pipe_segments = split_pipeline(&tokens);
    let pipeline = pipe_segments
        .into_iter()
        .map(parse_command)
        .collect::<Result<Vec<_>>>()?;

    Ok(ParsedLine {
        pipeline,
        background,
    })
}

fn split_pipeline(tokens: &[Token]) -> Vec<Vec<Token>> {
    let mut segments = Vec::new();
    let mut current = Vec::new();

    for token in tokens {
        if *token == Token::Pipe {
            segments.push(std::mem::take(&mut current));
        } else {
            current.push(token.clone());
        }
    }

    if !current.is_empty() {
        segments.push(current);
    }

    segments
}

fn parse_command(tokens: Vec<Token>) -> Result<CommandSpec> {
    let mut argv = Vec::new();
    let mut redirects = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i] {
            Token::Word(word) => argv.push(Arg {
                value: word.clone(),
                literal: false,
            }),
            Token::LiteralWord(word) => argv.push(Arg {
                value: word.clone(),
                literal: true,
            }),
            Token::RedirectStdin => {
                let path = next_word(&tokens, i + 1)?;
                redirects.push(Redirect::file(Stream::Stdin, OpenMode::Truncate, path));
                i += 1;
            }
            Token::RedirectHeredoc => {
                let delimiter = next_word(&tokens, i + 1)?;
                redirects.push(Redirect::heredoc(delimiter));
                i += 1;
            }
            Token::RedirectStdout(mode) => {
                let path = next_word(&tokens, i + 1)?;
                redirects.push(Redirect::file(Stream::Stdout, *mode, path));
                i += 1;
            }
            Token::RedirectStderr(mode) => {
                let path = next_word(&tokens, i + 1)?;
                redirects.push(Redirect::file(Stream::Stderr, *mode, path));
                i += 1;
            }
            Token::RedirectBoth(mode) => {
                let path = next_word(&tokens, i + 1)?;
                redirects.push(Redirect::file(Stream::Both, *mode, path));
                i += 1;
            }
            Token::Pipe | Token::Background | Token::Semicolon | Token::And | Token::Or => {
                return Err(MshError::ParseError("unexpected token in command".into()));
            }
        }
        i += 1;
    }

    Ok(CommandSpec { argv, redirects })
}

fn next_word(tokens: &[Token], index: usize) -> Result<String> {
    match tokens.get(index) {
        Some(Token::Word(word)) | Some(Token::LiteralWord(word)) => Ok(word.clone()),
        _ => Err(MshError::ParseError("expected path after redirect".into())),
    }
}

fn lex(input: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut word_quoted = false;
    let mut brace_depth: usize = 0;

    while let Some(ch) = chars.next() {
        if in_single {
            if ch == '\'' {
                in_single = false;
                flush_literal_word(&mut tokens, &mut current, &mut word_quoted);
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
            '\'' => {
                in_single = true;
                word_quoted = true;
            }
            '"' => {
                in_double = true;
                word_quoted = true;
            }
            '{' => {
                brace_depth += 1;
                current.push(ch);
            }
            '}' => {
                brace_depth = brace_depth.saturating_sub(1);
                current.push(ch);
            }
            '|' if chars.peek() == Some(&'|') => {
                flush_word(&mut tokens, &mut current, &mut word_quoted);
                chars.next();
                tokens.push(Token::Or);
            }
            '|' => {
                flush_word(&mut tokens, &mut current, &mut word_quoted);
                tokens.push(Token::Pipe);
            }
            '&' if chars.peek() == Some(&'&') => {
                flush_word(&mut tokens, &mut current, &mut word_quoted);
                chars.next();
                tokens.push(Token::And);
            }
            '&' if chars.peek() == Some(&'>') => {
                flush_word(&mut tokens, &mut current, &mut word_quoted);
                chars.next();
                let mode = read_redirect_append(&mut chars);
                tokens.push(Token::RedirectBoth(mode));
            }
            '&' if chars.clone().all(|c| c.is_whitespace()) => {
                flush_word(&mut tokens, &mut current, &mut word_quoted);
                tokens.push(Token::Background);
                break;
            }
            '&' => current.push(ch),
            ';' if brace_depth == 0 => {
                flush_word(&mut tokens, &mut current, &mut word_quoted);
                tokens.push(Token::Semicolon);
            }
            ';' => current.push(ch),
            '2' if chars.peek() == Some(&'>') => {
                flush_word(&mut tokens, &mut current, &mut word_quoted);
                chars.next();
                let mode = read_redirect_append(&mut chars);
                tokens.push(Token::RedirectStderr(mode));
            }
            '>' if chars.peek() == Some(&'>') => {
                flush_word(&mut tokens, &mut current, &mut word_quoted);
                chars.next();
                tokens.push(Token::RedirectStdout(OpenMode::Append));
            }
            '>' => {
                flush_word(&mut tokens, &mut current, &mut word_quoted);
                tokens.push(Token::RedirectStdout(OpenMode::Truncate));
            }
            '<' if chars.peek() == Some(&'<') => {
                flush_word(&mut tokens, &mut current, &mut word_quoted);
                chars.next();
                if chars.peek() == Some(&'-') {
                    chars.next();
                }
                tokens.push(Token::RedirectHeredoc);
            }
            '<' => {
                flush_word(&mut tokens, &mut current, &mut word_quoted);
                tokens.push(Token::RedirectStdin);
            }
            c if c.is_whitespace() => flush_word(&mut tokens, &mut current, &mut word_quoted),
            '$' if chars.peek() == Some(&'(') => {
                current.push('$');
                current.push('(');
                chars.next();
                read_dollar_paren_body(&mut chars, &mut current)?;
                flush_word(&mut tokens, &mut current, &mut word_quoted);
            }
            '$' if chars.peek() == Some(&'{') => {
                // `${...}` はパラメータ展開。内部の空白や `;` で語分割しないよう一括取り込み。
                current.push('$');
                current.push('{');
                chars.next();
                read_braced_param(&mut chars, &mut current)?;
            }
            '`' => {
                current.push(ch);
                read_backtick_word(&mut chars, &mut current)?;
                flush_word(&mut tokens, &mut current, &mut word_quoted);
            }
            c => current.push(c),
        }
    }

    flush_word(&mut tokens, &mut current, &mut word_quoted);
    Ok(tokens)
}

fn read_dollar_paren_body<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
    current: &mut String,
) -> Result<()> {
    let mut depth = 1;
    for ch in chars.by_ref() {
        current.push(ch);
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
            }
            _ => {}
        }
    }
    Err(MshError::ParseError("unclosed command substitution".into()))
}

fn read_braced_param<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
    current: &mut String,
) -> Result<()> {
    let mut depth = 1;
    for ch in chars.by_ref() {
        current.push(ch);
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
            }
            _ => {}
        }
    }
    Err(MshError::ParseError(
        "unclosed parameter expansion ${ }".into(),
    ))
}

fn read_backtick_word<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
    current: &mut String,
) -> Result<()> {
    for ch in chars.by_ref() {
        current.push(ch);
        if ch == '`' {
            return Ok(());
        }
    }
    Err(MshError::ParseError(
        "unclosed backtick substitution".into(),
    ))
}

fn flush_literal_word(tokens: &mut Vec<Token>, current: &mut String, quoted: &mut bool) {
    if !current.is_empty() || *quoted {
        tokens.push(Token::LiteralWord(std::mem::take(current)));
    }
    *quoted = false;
}

fn flush_word(tokens: &mut Vec<Token>, current: &mut String, quoted: &mut bool) {
    if !current.is_empty() || *quoted {
        tokens.push(Token::Word(std::mem::take(current)));
    }
    *quoted = false;
}

fn read_redirect_append<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) -> OpenMode {
    if chars.peek() == Some(&'>') {
        chars.next();
        OpenMode::Append
    } else {
        OpenMode::Truncate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pipeline() {
        let parsed = parse_line("echo hello | wc -c").unwrap();
        assert_eq!(parsed.pipeline.len(), 2);
        assert_eq!(parsed.pipeline[0].argv[0].value, "echo");
        assert_eq!(parsed.pipeline[0].argv[1].value, "hello");
        assert_eq!(parsed.pipeline[1].argv[0].value, "wc");
        assert_eq!(parsed.pipeline[1].argv[1].value, "-c");
    }

    #[test]
    fn parse_redirect_stdout() {
        let parsed = parse_line("echo hello > /tmp/out.txt").unwrap();
        assert_eq!(parsed.pipeline.len(), 1);
        assert_eq!(parsed.pipeline[0].redirects[0].stream, Stream::Stdout);
        assert_eq!(parsed.pipeline[0].redirects[0].path, "/tmp/out.txt");
    }

    #[test]
    fn parse_background() {
        let parsed = parse_line("sleep 1 &").unwrap();
        assert!(parsed.background);
    }

    #[test]
    fn parse_and_chain() {
        let script = parse_script("echo ok && echo done").unwrap();
        assert_eq!(script.segments.len(), 2);
        assert_eq!(script.segments[0].op, Some(ChainOp::And));
        assert_eq!(
            script.segments[0].pipeline.pipeline[0].argv[0].value,
            "echo"
        );
        assert_eq!(script.segments[0].pipeline.pipeline[0].argv[1].value, "ok");
        assert_eq!(
            script.segments[1].pipeline.pipeline[0].argv[0].value,
            "echo"
        );
        assert_eq!(
            script.segments[1].pipeline.pipeline[0].argv[1].value,
            "done"
        );
    }

    #[test]
    fn parse_or_chain() {
        let script = parse_script("false || echo fallback").unwrap();
        assert_eq!(script.segments.len(), 2);
        assert_eq!(script.segments[0].op, Some(ChainOp::Or));
    }

    #[test]
    fn parse_semicolon_chain() {
        let script = parse_script("echo a; echo b").unwrap();
        assert_eq!(script.segments.len(), 2);
        assert_eq!(script.segments[0].op, Some(ChainOp::Semicolon));
    }

    #[test]
    fn parse_alias_assignment_with_embedded_single_quotes() {
        let script = parse_script("alias ll='echo aliased'").unwrap();
        assert_eq!(script.segments.len(), 1);
        let argv = &script.segments[0].pipeline.pipeline[0].argv;
        assert_eq!(argv.len(), 2);
        assert_eq!(argv[0].value, "alias");
        assert_eq!(argv[1].value, "ll=echo aliased");
    }

    #[test]
    fn parse_semicolon_inside_while_not_split() {
        let script = parse_script("i=0; while [ $i -lt 1 ]; do echo w; i=1; done").unwrap();
        assert_eq!(script.segments.len(), 2);
        assert_eq!(script.segments[0].op, Some(ChainOp::Semicolon));
    }

    #[test]
    fn parse_pipeline_with_and() {
        let script = parse_script("echo hello | wc -c && echo done").unwrap();
        assert_eq!(script.segments.len(), 2);
        assert_eq!(script.segments[0].pipeline.pipeline.len(), 2);
    }
}
