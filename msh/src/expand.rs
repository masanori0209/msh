use crate::error::{MshError, Result};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// 連想配列（`declare -A`）。キー順を安定させるため `BTreeMap` を用いる。
pub type AssocArrays = HashMap<String, BTreeMap<String, String>>;

pub struct ExpandContext<'a> {
    pub last_status: i32,
    pub shell_vars: &'a HashMap<String, String>,
    pub arrays: &'a HashMap<String, Vec<String>>,
    pub assoc: &'a AssocArrays,
    pub nounset: bool,
}

pub fn expand_word(word: &str) -> Result<Vec<String>> {
    let empty_vars = HashMap::new();
    let empty_arrays: HashMap<String, Vec<String>> = HashMap::new();
    let empty_assoc: AssocArrays = HashMap::new();
    let ctx = ExpandContext {
        last_status: 0,
        shell_vars: &empty_vars,
        arrays: &empty_arrays,
        assoc: &empty_assoc,
        nounset: false,
    };
    expand_word_with(word, &ctx)
}

pub fn expand_word_with(word: &str, ctx: &ExpandContext<'_>) -> Result<Vec<String>> {
    let expanded = expand_all(word, ctx)?;
    expand_glob(&expanded)
}

pub fn expand_vars(input: &str) -> String {
    expand_vars_with(input, 0, &HashMap::new(), &HashMap::new())
}

pub fn expand_vars_with(
    input: &str,
    last_status: i32,
    shell_vars: &HashMap<String, String>,
    arrays: &HashMap<String, Vec<String>>,
) -> String {
    let empty_assoc: AssocArrays = HashMap::new();
    let ctx = ExpandContext {
        last_status,
        shell_vars,
        arrays,
        assoc: &empty_assoc,
        nounset: false,
    };
    expand_all(input, &ctx).unwrap_or_else(|_| input.to_string())
}

pub fn expand_all(input: &str, ctx: &ExpandContext<'_>) -> Result<String> {
    let mut output = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' {
            if chars.peek() == Some(&'(') {
                chars.next();
                if chars.peek() == Some(&'(') {
                    chars.next();
                    let body = read_arith_body(&mut chars)?;
                    let value = eval_arith(&body, ctx)?;
                    output.push_str(&value.to_string());
                    continue;
                }
                return Err(MshError::ParseError(
                    "command substitution must be expanded before variable expansion".into(),
                ));
            }

            match chars.peek() {
                Some('?') => {
                    chars.next();
                    output.push_str(&ctx.last_status.to_string());
                }
                Some('{') => {
                    chars.next();
                    let reference = read_braced_reference(&mut chars);
                    output.push_str(&expand_reference(&reference, ctx)?);
                }
                Some(c) if is_var_start(*c) => {
                    let name = read_var_name(&mut chars, false);
                    output.push_str(&lookup_var(&name, ctx)?);
                }
                _ => output.push('$'),
            }
            continue;
        }

        output.push(ch);
    }

    Ok(output)
}

fn read_braced_reference<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) -> String {
    let mut reference = String::new();
    while let Some(ch) = chars.peek() {
        if *ch == '}' {
            chars.next();
            break;
        }
        reference.push(chars.next().unwrap());
    }
    reference
}

fn expand_reference(reference: &str, ctx: &ExpandContext<'_>) -> Result<String> {
    // ${!name[@]} / ${!name[*]} -> 連想配列のキー / インデックス配列の添字一覧。
    // ${!var} -> 間接参照（var の値を名前とする変数の値）。
    if let Some(stripped) = reference.strip_prefix('!') {
        if let Some((name, raw_index)) = split_array_ref(stripped) {
            let index = expand_all(raw_index, ctx)?;
            if index == "@" || index == "*" {
                if let Some(map) = ctx.assoc.get(name) {
                    return Ok(map.keys().cloned().collect::<Vec<_>>().join(" "));
                }
                if let Some(values) = ctx.arrays.get(name) {
                    return Ok((0..values.len())
                        .map(|i| i.to_string())
                        .collect::<Vec<_>>()
                        .join(" "));
                }
                return Ok(String::new());
            }
        }
        let pointed = resolve_value_opt(stripped, ctx).unwrap_or_default();
        return Ok(resolve_value_opt(&pointed, ctx).unwrap_or_default());
    }

    // ${#name[@]} -> 要素数, ${#name[key]} -> 要素の文字数, ${#var} -> 文字長。
    if let Some(stripped) = reference.strip_prefix('#') {
        if let Some((name, raw_index)) = split_array_ref(stripped) {
            let index = expand_all(raw_index, ctx)?;
            if let Some(map) = ctx.assoc.get(name) {
                return Ok(match index.as_str() {
                    "@" | "*" => map.len().to_string(),
                    key => map
                        .get(key)
                        .map(|v| v.chars().count().to_string())
                        .unwrap_or_else(|| "0".into()),
                });
            }
            if index == "@" || index == "*" {
                return Ok(ctx
                    .arrays
                    .get(name)
                    .map(|v| v.len().to_string())
                    .unwrap_or_else(|| "0".into()));
            }
            if let Ok(idx) = index.parse::<usize>() {
                return Ok(ctx
                    .arrays
                    .get(name)
                    .and_then(|v| v.get(idx))
                    .map(|v| v.chars().count().to_string())
                    .unwrap_or_else(|| "0".into()));
            }
            return Ok("0".into());
        }
        let value = resolve_value_opt(stripped, ctx).unwrap_or_default();
        return Ok(value.chars().count().to_string());
    }

    // 名前部（添字付きを含む）と後続の演算子部に分割。
    let (name, op) = split_name_and_op(reference);
    if !op.is_empty() {
        return apply_param_op(name, op, ctx);
    }

    expand_plain_reference(reference, ctx)
}

/// 演算子なしの素の参照（スカラ / `name[index]`）を展開する。
fn expand_plain_reference(reference: &str, ctx: &ExpandContext<'_>) -> Result<String> {
    if let Some((name, raw_index)) = split_array_ref(reference) {
        let index = expand_all(raw_index, ctx)?;
        if let Some(map) = ctx.assoc.get(name) {
            return Ok(match index.as_str() {
                "@" | "*" => map.values().cloned().collect::<Vec<_>>().join(" "),
                key => map.get(key).cloned().unwrap_or_default(),
            });
        }
        if let Some(values) = ctx.arrays.get(name) {
            return Ok(match index.as_str() {
                "@" | "*" => values.join(" "),
                idx => idx
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| values.get(i).cloned())
                    .unwrap_or_default(),
            });
        }
        return Ok(String::new());
    }

    lookup_var(reference, ctx)
}

/// 参照を Option で解決する（未設定なら None）。スカラ・配列要素の両方に対応。
fn resolve_value_opt(reference: &str, ctx: &ExpandContext<'_>) -> Option<String> {
    if let Some((name, raw_index)) = split_array_ref(reference) {
        let index = expand_all(raw_index, ctx).ok()?;
        if let Some(map) = ctx.assoc.get(name) {
            return match index.as_str() {
                "@" | "*" => Some(map.values().cloned().collect::<Vec<_>>().join(" ")),
                key => map.get(key).cloned(),
            };
        }
        if let Some(values) = ctx.arrays.get(name) {
            return match index.as_str() {
                "@" | "*" => Some(values.join(" ")),
                idx => idx
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| values.get(i).cloned()),
            };
        }
        return None;
    }
    if let Some(value) = ctx.shell_vars.get(reference) {
        return Some(value.clone());
    }
    env::var(reference).ok()
}

/// `${parameter<op>word}` の名前部と演算子部を分割する。
/// 名前は識別子の連続、または `name[index]`。残りが演算子＋オペランド。
fn split_name_and_op(reference: &str) -> (&str, &str) {
    let bytes = reference.as_bytes();
    let mut i = 0;
    while i < bytes.len() && is_var_continue(bytes[i] as char) {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'[' {
        if let Some(rel) = reference[i..].find(']') {
            i += rel + 1;
        }
    }
    (&reference[..i], &reference[i..])
}

/// パラメータ展開の演算子を適用する。
fn apply_param_op(name: &str, op: &str, ctx: &ExpandContext<'_>) -> Result<String> {
    let current = resolve_value_opt(name, ctx);

    // `:` 付きは「未設定または空」、`:` なしは「未設定」のみを対象とする。
    if let Some(after) = op.strip_prefix(':') {
        match after.chars().next() {
            Some('-') => return default_value(&current, true, &after[1..], ctx),
            Some('=') => return default_value(&current, true, &after[1..], ctx),
            Some('?') => return error_if_unset(name, &current, true, &after[1..], ctx),
            Some('+') => return alternate_value(&current, true, &after[1..], ctx),
            _ => return substring(&current.unwrap_or_default(), after, ctx),
        }
    }

    let first = op.as_bytes()[0];
    let rest = &op[1..];
    match first {
        b'-' => default_value(&current, false, rest, ctx),
        b'=' => default_value(&current, false, rest, ctx),
        b'?' => error_if_unset(name, &current, false, rest, ctx),
        b'+' => alternate_value(&current, false, rest, ctx),
        b'#' => {
            let longest = op.starts_with("##");
            let pat = expand_all(if longest { &op[2..] } else { rest }, ctx)?;
            Ok(remove_prefix(&current.unwrap_or_default(), &pat, longest))
        }
        b'%' => {
            let longest = op.starts_with("%%");
            let pat = expand_all(if longest { &op[2..] } else { rest }, ctx)?;
            Ok(remove_suffix(&current.unwrap_or_default(), &pat, longest))
        }
        b'/' => replace(&current.unwrap_or_default(), op, ctx),
        b'^' => Ok(change_case(
            &current.unwrap_or_default(),
            op.starts_with("^^"),
            true,
        )),
        b',' => Ok(change_case(
            &current.unwrap_or_default(),
            op.starts_with(",,"),
            false,
        )),
        _ => Ok(current.unwrap_or_default()),
    }
}

fn is_active(current: &Option<String>, colon: bool) -> bool {
    match current {
        Some(v) => !colon || !v.is_empty(),
        None => false,
    }
}

fn default_value(
    current: &Option<String>,
    colon: bool,
    word: &str,
    ctx: &ExpandContext<'_>,
) -> Result<String> {
    if is_active(current, colon) {
        Ok(current.clone().unwrap_or_default())
    } else {
        expand_all(word, ctx)
    }
}

fn alternate_value(
    current: &Option<String>,
    colon: bool,
    word: &str,
    ctx: &ExpandContext<'_>,
) -> Result<String> {
    if is_active(current, colon) {
        expand_all(word, ctx)
    } else {
        Ok(String::new())
    }
}

fn error_if_unset(
    name: &str,
    current: &Option<String>,
    colon: bool,
    word: &str,
    ctx: &ExpandContext<'_>,
) -> Result<String> {
    if is_active(current, colon) {
        return Ok(current.clone().unwrap_or_default());
    }
    let msg = expand_all(word, ctx)?;
    let msg = if msg.is_empty() {
        "parameter null or not set".to_string()
    } else {
        msg
    };
    Err(MshError::ScriptError(format!("{name}: {msg}")))
}

fn substring(value: &str, spec: &str, ctx: &ExpandContext<'_>) -> Result<String> {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len() as i64;
    let (offset_part, length_part) = match spec.split_once(':') {
        Some((o, l)) => (o, Some(l)),
        None => (spec, None),
    };
    let offset = eval_arith(offset_part.trim(), ctx).unwrap_or(0);
    let start = if offset < 0 {
        (len + offset).max(0)
    } else {
        offset.min(len)
    } as usize;

    let end = match length_part {
        None => chars.len(),
        Some(l) => {
            let length = eval_arith(l.trim(), ctx).unwrap_or(0);
            if length < 0 {
                ((len + length).max(start as i64)) as usize
            } else {
                (start + length as usize).min(chars.len())
            }
        }
    };
    Ok(chars[start..end.max(start)].iter().collect())
}

fn remove_prefix(value: &str, pattern: &str, longest: bool) -> String {
    let chars: Vec<char> = value.chars().collect();
    let n = chars.len();
    let range: Vec<usize> = if longest {
        (0..=n).rev().collect()
    } else {
        (0..=n).collect()
    };
    for end in range {
        let candidate: String = chars[..end].iter().collect();
        if glob_match(pattern, &candidate) {
            return chars[end..].iter().collect();
        }
    }
    value.to_string()
}

fn remove_suffix(value: &str, pattern: &str, longest: bool) -> String {
    let chars: Vec<char> = value.chars().collect();
    let n = chars.len();
    let range: Vec<usize> = if longest {
        (0..=n).collect()
    } else {
        (0..=n).rev().collect()
    };
    for start in range {
        let candidate: String = chars[start..].iter().collect();
        if glob_match(pattern, &candidate) {
            return chars[..start].iter().collect();
        }
    }
    value.to_string()
}

fn replace(value: &str, op: &str, ctx: &ExpandContext<'_>) -> Result<String> {
    let all = op.starts_with("//");
    let body = if all { &op[2..] } else { &op[1..] };
    let (raw_pat, raw_rep) = match body.split_once('/') {
        Some((p, r)) => (p, r),
        None => (body, ""),
    };
    let pattern = expand_all(raw_pat, ctx)?;
    let replacement = expand_all(raw_rep, ctx)?;
    if pattern.is_empty() {
        return Ok(value.to_string());
    }
    if all {
        Ok(value.replace(&pattern, &replacement))
    } else {
        Ok(value.replacen(&pattern, &replacement, 1))
    }
}

fn change_case(value: &str, whole: bool, upper: bool) -> String {
    if whole {
        if upper {
            value.to_uppercase()
        } else {
            value.to_lowercase()
        }
    } else {
        let mut chars = value.chars();
        match chars.next() {
            Some(first) => {
                let head: String = if upper {
                    first.to_uppercase().collect()
                } else {
                    first.to_lowercase().collect()
                };
                format!("{head}{}", chars.as_str())
            }
            None => String::new(),
        }
    }
}

/// `name[index]` を (name, 添字文字列) に分解する。添字は呼び出し側で展開する。
fn split_array_ref(reference: &str) -> Option<(&str, &str)> {
    let open = reference.find('[')?;
    let close = reference.rfind(']')?;
    if close <= open {
        return None;
    }
    let name = &reference[..open];
    if name.is_empty() {
        return None;
    }
    Some((name, reference[open + 1..close].trim()))
}

fn lookup_var(name: &str, ctx: &ExpandContext<'_>) -> Result<String> {
    if let Some(value) = ctx.shell_vars.get(name) {
        return Ok(value.clone());
    }
    if let Ok(value) = env::var(name) {
        return Ok(value);
    }
    // シェル変数・環境変数で上書きされていない場合のみ動的特殊変数を返す。
    if name == "RANDOM" {
        return Ok((pseudo_random() % 32768).to_string());
    }
    if ctx.nounset {
        return Err(MshError::ScriptError(format!("{name}: unbound variable")));
    }
    Ok(String::new())
}

/// 依存を増やさない簡易 PRNG（`$RANDOM` 用）。xorshift をスレッドローカル状態で進める。
fn pseudo_random() -> u64 {
    use std::cell::Cell;
    use std::time::{SystemTime, UNIX_EPOCH};
    thread_local! {
        static STATE: Cell<u64> = const { Cell::new(0) };
    }
    STATE.with(|state| {
        let mut x = state.get();
        if x == 0 {
            x = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9e3779b97f4a7c15)
                | 1;
        }
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        state.set(x);
        x
    })
}

/// `$((` の後ろから対応する `))` までの本体を読み取る（ネストした括弧を考慮）。
fn read_arith_body<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) -> Result<String> {
    let mut body = String::new();
    let mut depth = 0i32;
    while let Some(&c) = chars.peek() {
        match c {
            '(' => {
                depth += 1;
                body.push(c);
                chars.next();
            }
            ')' if depth == 0 => {
                chars.next();
                if chars.peek() == Some(&')') {
                    chars.next();
                    return Ok(body);
                }
                return Err(MshError::ParseError(
                    "malformed arithmetic expansion $(( ))".into(),
                ));
            }
            ')' => {
                depth -= 1;
                body.push(c);
                chars.next();
            }
            _ => {
                body.push(c);
                chars.next();
            }
        }
    }
    Err(MshError::ParseError(
        "unclosed arithmetic expansion $(( ))".into(),
    ))
}

#[derive(Debug, PartialEq)]
enum ArithToken {
    Num(i64),
    Op(char),
    LParen,
    RParen,
}

/// 算術式を整数評価する。変数は ctx から解決し、未定義・非数値は 0 とみなす。
fn eval_arith(expr: &str, ctx: &ExpandContext<'_>) -> Result<i64> {
    let tokens = tokenize_arith(expr, ctx)?;
    let mut pos = 0;
    let value = parse_arith_expr(&tokens, &mut pos)?;
    if pos != tokens.len() {
        return Err(MshError::ScriptError(format!(
            "invalid arithmetic expression: {expr}"
        )));
    }
    Ok(value)
}

fn tokenize_arith(expr: &str, ctx: &ExpandContext<'_>) -> Result<Vec<ArithToken>> {
    let mut tokens = Vec::new();
    let mut chars = expr.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' => {
                chars.next();
            }
            '0'..='9' => {
                let mut num = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() {
                        num.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(ArithToken::Num(num.parse().unwrap_or(0)));
            }
            '$' | 'a'..='z' | 'A'..='Z' | '_' => {
                if c == '$' {
                    chars.next();
                }
                let mut name = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_alphanumeric() || d == '_' {
                        name.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let raw = lookup_var(&name, ctx).unwrap_or_default();
                tokens.push(ArithToken::Num(raw.trim().parse().unwrap_or(0)));
            }
            '+' | '-' | '*' | '/' | '%' => {
                tokens.push(ArithToken::Op(c));
                chars.next();
            }
            '(' => {
                tokens.push(ArithToken::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(ArithToken::RParen);
                chars.next();
            }
            _ => {
                return Err(MshError::ScriptError(format!(
                    "invalid character in arithmetic expression: {c}"
                )));
            }
        }
    }
    Ok(tokens)
}

fn parse_arith_expr(tokens: &[ArithToken], pos: &mut usize) -> Result<i64> {
    let mut value = parse_arith_term(tokens, pos)?;
    while let Some(ArithToken::Op(op @ ('+' | '-'))) = tokens.get(*pos) {
        let op = *op;
        *pos += 1;
        let rhs = parse_arith_term(tokens, pos)?;
        value = if op == '+' { value + rhs } else { value - rhs };
    }
    Ok(value)
}

fn parse_arith_term(tokens: &[ArithToken], pos: &mut usize) -> Result<i64> {
    let mut value = parse_arith_factor(tokens, pos)?;
    while let Some(ArithToken::Op(op @ ('*' | '/' | '%'))) = tokens.get(*pos) {
        let op = *op;
        *pos += 1;
        let rhs = parse_arith_factor(tokens, pos)?;
        value = match op {
            '*' => value * rhs,
            '/' => {
                if rhs == 0 {
                    return Err(MshError::ScriptError("arithmetic: division by zero".into()));
                }
                value / rhs
            }
            '%' => {
                if rhs == 0 {
                    return Err(MshError::ScriptError("arithmetic: division by zero".into()));
                }
                value % rhs
            }
            _ => unreachable!(),
        };
    }
    Ok(value)
}

fn parse_arith_factor(tokens: &[ArithToken], pos: &mut usize) -> Result<i64> {
    match tokens.get(*pos) {
        Some(ArithToken::Num(n)) => {
            *pos += 1;
            Ok(*n)
        }
        Some(ArithToken::Op('-')) => {
            *pos += 1;
            Ok(-parse_arith_factor(tokens, pos)?)
        }
        Some(ArithToken::Op('+')) => {
            *pos += 1;
            parse_arith_factor(tokens, pos)
        }
        Some(ArithToken::LParen) => {
            *pos += 1;
            let value = parse_arith_expr(tokens, pos)?;
            if tokens.get(*pos) != Some(&ArithToken::RParen) {
                return Err(MshError::ScriptError(
                    "arithmetic: expected closing paren".into(),
                ));
            }
            *pos += 1;
            Ok(value)
        }
        _ => Err(MshError::ScriptError(
            "arithmetic: unexpected end of expression".into(),
        )),
    }
}

fn read_var_name<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
    braced: bool,
) -> String {
    let mut name = String::new();

    while let Some(ch) = chars.peek() {
        if braced && *ch == '}' {
            chars.next();
            break;
        }
        if !braced && !is_var_continue(*ch) {
            break;
        }
        name.push(chars.next().unwrap());
    }

    name
}

fn is_var_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_var_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn expand_glob(pattern: &str) -> Result<Vec<String>> {
    if !pattern.contains(['*', '?', '[']) {
        return Ok(vec![pattern.to_string()]);
    }

    let path = Path::new(pattern);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_pattern = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| pattern.to_string());

    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(_) => return Ok(vec![pattern.to_string()]),
    };

    let mut matches = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if glob_match(&file_pattern, &name) {
            matches.push(entry.path().to_string_lossy().into_owned());
        }
    }

    if matches.is_empty() {
        Ok(vec![pattern.to_string()])
    } else {
        matches.sort();
        Ok(matches)
    }
}

fn glob_match(pattern: &str, text: &str) -> bool {
    glob_match_bytes(pattern.as_bytes(), text.as_bytes())
}

fn glob_match_bytes(pattern: &[u8], text: &[u8]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }

    if pattern[0] == b'*' {
        return glob_match_bytes(&pattern[1..], text)
            || (!text.is_empty() && glob_match_bytes(pattern, &text[1..]));
    }

    if text.is_empty() {
        return false;
    }

    if pattern[0] == b'?' || pattern[0] == text[0] {
        return glob_match_bytes(&pattern[1..], &text[1..]);
    }

    false
}

pub fn parse_array_assignment(word: &str) -> Option<(String, Vec<String>)> {
    let open = word.find("=(")?;
    let name = word[..open].trim();
    if name.is_empty() {
        return None;
    }
    let close = word.rfind(')')?;
    if close <= open + 1 {
        return None;
    }
    let inner = word[open + 2..close].trim();
    if inner.is_empty() {
        return Some((name.to_string(), Vec::new()));
    }
    Some((name.to_string(), tokenize_array_values(inner)))
}

fn tokenize_array_values(input: &str) -> Vec<String> {
    let mut values = Vec::new();
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
            '\'' => in_single = true,
            '"' => in_double = true,
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    values.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }

    if !current.is_empty() {
        values.push(current);
    }

    values
}

pub fn resolve_command_path(command: &str) -> Option<PathBuf> {
    if command.contains('/') {
        let path = PathBuf::from(command);
        return path.is_file().then_some(path);
    }

    let path_var = env::var("PATH").unwrap_or_default();
    for dir in path_var.split(':').filter(|d| !d.is_empty()) {
        let candidate = PathBuf::from(dir).join(command);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_simple_var() {
        env::set_var("MSH_TEST_VAR", "hello");
        assert_eq!(expand_vars("$MSH_TEST_VAR"), "hello");
        assert_eq!(expand_vars("${MSH_TEST_VAR}"), "hello");
    }

    #[test]
    fn expand_last_status() {
        assert_eq!(
            expand_vars_with("$?", 42, &Default::default(), &Default::default()),
            "42"
        );
    }

    #[test]
    fn parse_array_assignment_values() {
        let (name, values) = parse_array_assignment("arr=(a b c)").unwrap();
        assert_eq!(name, "arr");
        assert_eq!(values, vec!["a", "b", "c"]);
    }

    #[test]
    fn arithmetic_eval_basic() {
        let ctx = ExpandContext {
            last_status: 0,
            shell_vars: &HashMap::new(),
            arrays: &HashMap::new(),
            assoc: &HashMap::new(),
            nounset: false,
        };
        assert_eq!(eval_arith("1 + 2 * 3", &ctx).unwrap(), 7);
        assert_eq!(eval_arith("(1 + 2) * 3", &ctx).unwrap(), 9);
        assert_eq!(eval_arith("10 % 3", &ctx).unwrap(), 1);
        assert_eq!(eval_arith("-5 + 2", &ctx).unwrap(), -3);
    }

    #[test]
    fn arithmetic_eval_with_var() {
        let mut vars = HashMap::new();
        vars.insert("i".to_string(), "5".to_string());
        let ctx = ExpandContext {
            last_status: 0,
            shell_vars: &vars,
            arrays: &HashMap::new(),
            assoc: &HashMap::new(),
            nounset: false,
        };
        assert_eq!(eval_arith("i + 1", &ctx).unwrap(), 6);
        assert_eq!(eval_arith("$i * 2", &ctx).unwrap(), 10);
    }

    #[test]
    fn expand_array_index() {
        let mut arrays = HashMap::new();
        arrays.insert("arr".into(), vec!["x".into(), "y".into()]);
        let ctx = ExpandContext {
            last_status: 0,
            shell_vars: &HashMap::new(),
            arrays: &arrays,
            assoc: &HashMap::new(),
            nounset: false,
        };
        assert_eq!(expand_all("${arr[1]}", &ctx).unwrap(), "y");
        assert_eq!(expand_all("${arr[@]}", &ctx).unwrap(), "x y");
        assert_eq!(expand_all("${#arr[@]}", &ctx).unwrap(), "2");
        assert_eq!(expand_all("${!arr[@]}", &ctx).unwrap(), "0 1");
    }

    #[test]
    fn expand_associative_array() {
        let mut assoc: AssocArrays = HashMap::new();
        let mut m = BTreeMap::new();
        m.insert("name".to_string(), "msh".to_string());
        m.insert("lang".to_string(), "rust".to_string());
        assoc.insert("cfg".to_string(), m);
        let ctx = ExpandContext {
            last_status: 0,
            shell_vars: &HashMap::new(),
            arrays: &HashMap::new(),
            assoc: &assoc,
            nounset: false,
        };
        assert_eq!(expand_all("${cfg[name]}", &ctx).unwrap(), "msh");
        assert_eq!(expand_all("${cfg[missing]}", &ctx).unwrap(), "");
        assert_eq!(expand_all("${#cfg[@]}", &ctx).unwrap(), "2");
        // BTreeMap によりキー順は安定（lang < name）。
        assert_eq!(expand_all("${!cfg[@]}", &ctx).unwrap(), "lang name");
        assert_eq!(expand_all("${cfg[@]}", &ctx).unwrap(), "rust msh");
    }

    fn ctx_with<'a>(vars: &'a HashMap<String, String>) -> ExpandContext<'a> {
        ExpandContext {
            last_status: 0,
            shell_vars: vars,
            arrays: Box::leak(Box::new(HashMap::new())),
            assoc: Box::leak(Box::new(AssocArrays::new())),
            nounset: false,
        }
    }

    #[test]
    fn param_default_and_alternate() {
        let mut vars = HashMap::new();
        vars.insert("set".into(), "v".into());
        vars.insert("empty".into(), "".into());
        let ctx = ctx_with(&vars);
        assert_eq!(expand_all("${unset:-d}", &ctx).unwrap(), "d");
        assert_eq!(expand_all("${set:-d}", &ctx).unwrap(), "v");
        assert_eq!(expand_all("${empty:-d}", &ctx).unwrap(), "d");
        assert_eq!(expand_all("${empty-d}", &ctx).unwrap(), "");
        assert_eq!(expand_all("${set:+alt}", &ctx).unwrap(), "alt");
        assert_eq!(expand_all("${unset:+alt}", &ctx).unwrap(), "");
    }

    #[test]
    fn param_error_when_unset() {
        let vars = HashMap::new();
        let ctx = ctx_with(&vars);
        assert!(expand_all("${missing:?required}", &ctx).is_err());
    }

    #[test]
    fn param_prefix_suffix_removal() {
        let mut vars = HashMap::new();
        vars.insert("file".into(), "archive.tar.gz".into());
        vars.insert("path".into(), "/usr/local/bin".into());
        let ctx = ctx_with(&vars);
        assert_eq!(expand_all("${file%.gz}", &ctx).unwrap(), "archive.tar");
        assert_eq!(expand_all("${file%.*}", &ctx).unwrap(), "archive.tar");
        assert_eq!(expand_all("${file%%.*}", &ctx).unwrap(), "archive");
        assert_eq!(expand_all("${path#*/}", &ctx).unwrap(), "usr/local/bin");
        assert_eq!(expand_all("${path##*/}", &ctx).unwrap(), "bin");
    }

    #[test]
    fn param_replace() {
        let mut vars = HashMap::new();
        vars.insert("p".into(), "a:b:c".into());
        let ctx = ctx_with(&vars);
        assert_eq!(expand_all("${p/:/ }", &ctx).unwrap(), "a b:c");
        assert_eq!(expand_all("${p//:/ }", &ctx).unwrap(), "a b c");
    }

    #[test]
    fn param_case_and_substring() {
        let mut vars = HashMap::new();
        vars.insert("s".into(), "hello".into());
        let ctx = ctx_with(&vars);
        assert_eq!(expand_all("${s^^}", &ctx).unwrap(), "HELLO");
        assert_eq!(expand_all("${s^}", &ctx).unwrap(), "Hello");
        assert_eq!(expand_all("${s:1:3}", &ctx).unwrap(), "ell");
        assert_eq!(expand_all("${s: -2}", &ctx).unwrap(), "lo");
        assert_eq!(expand_all("${#s}", &ctx).unwrap(), "5");
    }

    #[test]
    fn param_indirection() {
        let mut vars = HashMap::new();
        vars.insert("ptr".into(), "target".into());
        vars.insert("target".into(), "value".into());
        let ctx = ctx_with(&vars);
        assert_eq!(expand_all("${!ptr}", &ctx).unwrap(), "value");
    }

    #[test]
    fn random_is_in_range() {
        let vars = HashMap::new();
        let ctx = ctx_with(&vars);
        for _ in 0..50 {
            let n: i64 = expand_all("$RANDOM", &ctx).unwrap().parse().unwrap();
            assert!((0..32768).contains(&n));
        }
    }

    #[test]
    fn glob_star() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let pattern = format!("{manifest_dir}/Cargo.*");
        let matches = expand_glob(&pattern).unwrap();
        assert!(matches.iter().any(|m| m.contains("Cargo.toml")));
    }
}
