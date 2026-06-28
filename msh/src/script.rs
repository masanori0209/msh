use crate::error::{MshError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Command(String),
    If {
        condition: String,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },
    For {
        var: String,
        items: Vec<String>,
        body: Vec<Stmt>,
    },
    While {
        condition: String,
        body: Vec<Stmt>,
    },
    Case {
        word: String,
        arms: Vec<CaseArm>,
    },
    FunctionDef {
        name: String,
        body: Vec<Stmt>,
    },
    Return {
        code: Option<String>,
    },
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseArm {
    pub patterns: Vec<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    Function { name: String },
    If,
    For,
    While,
    Case,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingBlock {
    pub kind: BlockKind,
    pub lines: Vec<String>,
    pub depth: usize,
}

pub fn detect_block_start(line: &str) -> Option<BlockKind> {
    let trimmed = line.trim();
    if let Some(name) = parse_function_header(trimmed) {
        return Some(BlockKind::Function { name });
    }
    if starts_with_keyword(trimmed, "if") {
        return Some(BlockKind::If);
    }
    if starts_with_keyword(trimmed, "for") {
        return Some(BlockKind::For);
    }
    if starts_with_keyword(trimmed, "while") {
        return Some(BlockKind::While);
    }
    if starts_with_keyword(trimmed, "case") {
        return Some(BlockKind::Case);
    }
    None
}

pub fn parse_inline_or_single(line: &str) -> Result<Vec<Stmt>> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(vec![]);
    }

    if let Some(name) = parse_function_header(trimmed) {
        if function_has_only_body(trimmed) {
            return parse_function_definition(trimmed, &name);
        }
    }
    if starts_with_keyword(trimmed, "if") {
        return parse_if_stmt(trimmed);
    }
    if starts_with_keyword(trimmed, "for") {
        return parse_for_stmt(trimmed);
    }
    if starts_with_keyword(trimmed, "while") {
        return parse_while_stmt(trimmed);
    }
    if starts_with_keyword(trimmed, "case") {
        return parse_case_stmt(trimmed);
    }
    if starts_with_keyword(trimmed, "return") {
        return Ok(vec![parse_return(trimmed)?]);
    }
    if trimmed == "break" {
        return Ok(vec![Stmt::Break]);
    }
    if trimmed == "continue" {
        return Ok(vec![Stmt::Continue]);
    }

    Ok(vec![Stmt::Command(trimmed.to_string())])
}

pub fn continue_block(block: &mut PendingBlock, line: &str) -> Result<Option<Vec<Stmt>>> {
    block.lines.push(line.to_string());
    block.depth += count_open_braces(line);
    block.depth = block.depth.saturating_sub(count_close_braces(line));

    match &block.kind {
        BlockKind::Function { name } => {
            if block.depth == 0 && line.trim().ends_with('}') {
                let body = extract_brace_body(&block.lines.join("\n"))?;
                let stmts = parse_body_lines(&body)?;
                return Ok(Some(vec![Stmt::FunctionDef {
                    name: name.clone(),
                    body: stmts,
                }]));
            }
        }
        BlockKind::If => {
            if line.trim() == "fi" {
                return parse_if_block(&block.lines);
            }
        }
        BlockKind::For => {
            if line.trim() == "done" {
                return parse_for_block(&block.lines);
            }
        }
        BlockKind::While => {
            if line.trim() == "done" {
                return parse_while_block(&block.lines);
            }
        }
        BlockKind::Case => {
            if line.trim() == "esac" {
                return parse_case_block(&block.lines);
            }
        }
    }

    Ok(None)
}

fn starts_with_keyword(input: &str, keyword: &str) -> bool {
    input == keyword
        || input
            .strip_prefix(keyword)
            .is_some_and(|rest| rest.starts_with(' ') || rest.starts_with('\t'))
}

fn parse_function_header(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let open = trimmed.find("()")?;
    let name = trimmed[..open].trim();
    if name.is_empty() || !is_name(name) {
        return None;
    }
    Some(name.to_string())
}

fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn function_has_only_body(line: &str) -> bool {
    let Some(open) = line.find('{') else {
        return false;
    };
    let mut depth = 0;
    for (idx, ch) in line[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return line[open + idx + 1..].trim().is_empty();
                }
            }
            _ => {}
        }
    }
    false
}

fn parse_function_definition(line: &str, name: &str) -> Result<Vec<Stmt>> {
    let body = extract_brace_body(line)?;
    let stmts = parse_body_lines(&body)?;
    Ok(vec![Stmt::FunctionDef {
        name: name.to_string(),
        body: stmts,
    }])
}

fn extract_brace_body(line: &str) -> Result<String> {
    let Some(start) = line.find('{') else {
        return Err(MshError::ScriptError(
            "expected `{` in function body".into(),
        ));
    };
    let Some(end) = line.rfind('}') else {
        return Err(MshError::ScriptError(
            "expected `}` in function body".into(),
        ));
    };
    if end <= start {
        return Err(MshError::ScriptError("invalid function braces".into()));
    }
    Ok(line[start + 1..end].to_string())
}

fn parse_body_lines(body: &str) -> Result<Vec<Stmt>> {
    let mut stmts = Vec::new();
    for part in split_semicolons(body) {
        let part = part.trim();
        if part.is_empty() || part.starts_with('#') {
            continue;
        }
        stmts.extend(parse_inline_or_single(part)?);
    }
    Ok(stmts)
}

fn split_semicolons(input: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for ch in input.chars() {
        if in_single {
            current.push(ch);
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }
        if in_double {
            current.push(ch);
            if ch == '"' {
                in_double = false;
            }
            continue;
        }
        match ch {
            '\'' => {
                in_single = true;
                current.push(ch);
            }
            '"' => {
                in_double = true;
                current.push(ch);
            }
            ';' => {
                parts.push(std::mem::take(&mut current));
            }
            c => current.push(c),
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

fn parse_if_stmt(line: &str) -> Result<Vec<Stmt>> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("if")
        .ok_or_else(|| MshError::ScriptError("invalid if statement".into()))?
        .trim_start();
    let then_pos = find_keyword(rest, "then")
        .ok_or_else(|| MshError::ScriptError("expected `then` in if statement".into()))?;
    let fi_pos = find_keyword(rest, "fi")
        .ok_or_else(|| MshError::ScriptError("expected `fi` in if statement".into()))?;

    let condition = rest[..then_pos].trim().trim_end_matches(';').to_string();
    let middle = rest[then_pos + 4..fi_pos].trim();

    let (then_src, else_src) = if let Some(else_pos) = find_keyword(middle, "else") {
        (
            middle[..else_pos].trim(),
            Some(middle[else_pos + 4..].trim()),
        )
    } else {
        (middle, None)
    };

    Ok(vec![Stmt::If {
        condition,
        then_body: parse_body_lines(then_src)?,
        else_body: else_src.map(parse_body_lines).transpose()?,
    }])
}

fn parse_for_stmt(line: &str) -> Result<Vec<Stmt>> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("for")
        .ok_or_else(|| MshError::ScriptError("invalid for loop".into()))?
        .trim_start();
    let in_pos = find_keyword(rest, "in")
        .ok_or_else(|| MshError::ScriptError("expected `in` in for loop".into()))?;
    let do_pos = find_keyword(rest, "do")
        .ok_or_else(|| MshError::ScriptError("expected `do` in for loop".into()))?;
    let done_pos = find_keyword(rest, "done")
        .ok_or_else(|| MshError::ScriptError("expected `done` in for loop".into()))?;

    let var = rest[..in_pos].trim().to_string();
    let items_src = rest[in_pos + 2..do_pos].trim().trim_end_matches(';').trim();
    let body_src = rest[do_pos + 2..done_pos].trim();

    Ok(vec![Stmt::For {
        var,
        items: tokenize_words(items_src),
        body: parse_body_lines(body_src)?,
    }])
}

fn parse_while_stmt(line: &str) -> Result<Vec<Stmt>> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("while")
        .ok_or_else(|| MshError::ScriptError("invalid while loop".into()))?
        .trim_start();
    let do_pos = find_keyword(rest, "do")
        .ok_or_else(|| MshError::ScriptError("expected `do` in while loop".into()))?;
    let done_pos = find_keyword(rest, "done")
        .ok_or_else(|| MshError::ScriptError("expected `done` in while loop".into()))?;

    let condition = rest[..do_pos].trim().trim_end_matches(';').to_string();
    let body_src = rest[do_pos + 2..done_pos].trim();

    Ok(vec![Stmt::While {
        condition,
        body: parse_body_lines(body_src)?,
    }])
}

fn parse_case_stmt(line: &str) -> Result<Vec<Stmt>> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("case")
        .ok_or_else(|| MshError::ScriptError("invalid case statement".into()))?
        .trim_start();
    let in_pos = find_keyword(rest, "in")
        .ok_or_else(|| MshError::ScriptError("expected `in` in case statement".into()))?;
    let esac_pos = find_keyword(rest, "esac")
        .ok_or_else(|| MshError::ScriptError("expected `esac` in case statement".into()))?;

    let word = rest[..in_pos].trim().to_string();
    let arms_src = rest[in_pos + 2..esac_pos].trim();
    let arms = parse_case_arms(arms_src)?;

    Ok(vec![Stmt::Case { word, arms }])
}

fn parse_case_arms(input: &str) -> Result<Vec<CaseArm>> {
    let mut arms = Vec::new();
    for segment in input.split(";;") {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let Some(pos) = segment.find(')') else {
            continue;
        };
        let patterns = segment[..pos]
            .split('|')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect();
        let body = parse_body_lines(segment[pos + 1..].trim())?;
        arms.push(CaseArm { patterns, body });
    }
    Ok(arms)
}

fn parse_return(line: &str) -> Result<Stmt> {
    let rest = line.trim().strip_prefix("return").unwrap_or("").trim();
    Ok(Stmt::Return {
        code: if rest.is_empty() {
            None
        } else {
            Some(rest.to_string())
        },
    })
}

fn parse_if_block(lines: &[String]) -> Result<Option<Vec<Stmt>>> {
    parse_if_stmt(&lines.join("\n")).map(Some)
}

fn parse_for_block(lines: &[String]) -> Result<Option<Vec<Stmt>>> {
    parse_for_stmt(&lines.join("\n")).map(Some)
}

fn parse_while_block(lines: &[String]) -> Result<Option<Vec<Stmt>>> {
    parse_while_stmt(&lines.join("\n")).map(Some)
}

fn parse_case_block(lines: &[String]) -> Result<Option<Vec<Stmt>>> {
    parse_case_stmt(&lines.join("\n")).map(Some)
}

fn find_keyword(input: &str, keyword: &str) -> Option<usize> {
    let mut search_from = 0;
    while search_from < input.len() {
        let slice = &input[search_from..];
        let Some(rel) = slice.find(keyword) else {
            break;
        };
        let idx = search_from + rel;
        let before_ok =
            idx == 0 || matches!(input.as_bytes()[idx - 1], b' ' | b'\t' | b';' | b'\n');
        let after_idx = idx + keyword.len();
        let after_ok = after_idx >= input.len()
            || matches!(input.as_bytes()[after_idx], b' ' | b'\t' | b';' | b'\n');
        if before_ok && after_ok {
            return Some(idx);
        }
        search_from = idx + 1;
    }
    None
}

fn tokenize_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for ch in input.chars() {
        if in_single {
            current.push(ch);
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }
        if in_double {
            current.push(ch);
            if ch == '"' {
                in_double = false;
            }
            continue;
        }
        match ch {
            '\'' => {
                in_single = true;
                current.push(ch);
            }
            '"' => {
                in_double = true;
                current.push(ch);
            }
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

fn count_open_braces(line: &str) -> usize {
    line.chars().filter(|&c| c == '{').count()
}

fn count_close_braces(line: &str) -> usize {
    line.chars().filter(|&c| c == '}').count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_inline_for() {
        let stmts = parse_inline_or_single("for f in a b; do echo $f; done").unwrap();
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::For { var, items, .. } => {
                assert_eq!(var, "f");
                assert_eq!(items, &vec!["a".to_string(), "b".to_string()]);
            }
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn parse_function() {
        let stmts = parse_inline_or_single("hello() { echo hi; }").unwrap();
        assert!(matches!(stmts[0], Stmt::FunctionDef { .. }));
    }
}
