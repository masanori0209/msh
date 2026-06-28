use crate::error::{MshError, Result};

#[derive(Debug, Clone)]
pub struct PendingHeredoc {
    pub delimiter: String,
    pub strip_tabs: bool,
    pub literal: bool,
    pub command_line: String,
    pub body: Vec<String>,
}

pub type HeredocBodies = Vec<(String, String)>;
pub type HeredocContinueResult = Option<(String, HeredocBodies)>;

#[derive(Debug)]
pub enum PrepareResult {
    Ready {
        input: String,
        bodies: HeredocBodies,
    },
    NeedMore(PendingHeredoc),
    Unchanged,
}

pub fn prepare(input: &str) -> Result<PrepareResult> {
    if !input.contains("<<") {
        return Ok(PrepareResult::Unchanged);
    }

    let lines: Vec<&str> = input.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let Some(spec) = parse_start(line)? else {
            continue;
        };

        let mut body = Vec::new();
        for body_line in lines.iter().skip(index + 1) {
            if *body_line == spec.delimiter {
                let body_text = format_body(&body, spec.strip_tabs, spec.literal);
                let mut rebuilt = lines[..index].to_vec();
                rebuilt.push(spec.command_line.as_str());
                rebuilt.extend(lines.iter().skip(index + 1 + body.len() + 1));
                let bodies = vec![(spec.delimiter.clone(), body_text)];
                return Ok(PrepareResult::Ready {
                    input: rebuilt.join("\n"),
                    bodies,
                });
            }
            body.push(body_line.to_string());
        }

        return Ok(PrepareResult::NeedMore(PendingHeredoc {
            delimiter: spec.delimiter,
            strip_tabs: spec.strip_tabs,
            literal: spec.literal,
            command_line: spec.command_line,
            body,
        }));
    }

    Ok(PrepareResult::Unchanged)
}

pub fn continue_pending(pending: &mut PendingHeredoc, line: &str) -> Result<HeredocContinueResult> {
    if line == pending.delimiter {
        let body_text = format_body(&pending.body, pending.strip_tabs, pending.literal);
        return Ok(Some((
            pending.command_line.clone(),
            vec![(pending.delimiter.clone(), body_text)],
        )));
    }
    pending.body.push(line.to_string());
    Ok(None)
}

struct HeredocSpec {
    delimiter: String,
    strip_tabs: bool,
    literal: bool,
    command_line: String,
}

fn parse_start(line: &str) -> Result<Option<HeredocSpec>> {
    let Some(pos) = line.find("<<") else {
        return Ok(None);
    };
    let mut rest = &line[pos + 2..];
    let strip_tabs = rest.starts_with('-');
    if strip_tabs {
        rest = &rest[1..];
    }
    rest = rest.trim_start();
    let (delimiter, _quoted) = read_delimiter(rest)?;
    Ok(Some(HeredocSpec {
        delimiter,
        strip_tabs,
        literal: rest.starts_with('\''),
        command_line: line.to_string(),
    }))
}

fn read_delimiter(input: &str) -> Result<(String, bool)> {
    if let Some(stripped) = input.strip_prefix('\'') {
        let Some(end) = stripped.find('\'') else {
            return Err(MshError::ParseError("unclosed heredoc delimiter".into()));
        };
        return Ok((stripped[..end].to_string(), true));
    }
    if let Some(stripped) = input.strip_prefix('"') {
        let Some(end) = stripped.find('"') else {
            return Err(MshError::ParseError("unclosed heredoc delimiter".into()));
        };
        return Ok((stripped[..end].to_string(), false));
    }
    let end = input
        .find(|c: char| c.is_whitespace())
        .unwrap_or(input.len());
    Ok((input[..end].to_string(), false))
}

fn format_body(lines: &[impl AsRef<str>], strip_tabs: bool, _literal: bool) -> String {
    let mut out = String::new();
    for line in lines {
        let line = line.as_ref();
        let line = if strip_tabs {
            line.strip_prefix('\t').unwrap_or(line)
        } else {
            line
        };
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_heredoc() {
        let input = "cat <<EOF\nhello\nEOF";
        match prepare(input).unwrap() {
            PrepareResult::Ready { bodies, .. } => {
                assert_eq!(bodies[0].0, "EOF");
                assert!(bodies[0].1.contains("hello"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
