const RESET: &str = "\x1b[0m";
const BOLD_BLUE: &str = "\x1b[1;34m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const MAGENTA: &str = "\x1b[35m";
const CYAN: &str = "\x1b[36m";

pub fn highlight_line(line: &str) -> String {
    let mut output = String::with_capacity(line.len() + 32);
    let mut chars = line.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut word_index = 0;
    let mut current = String::new();
    let mut after_pipe = false;

    while let Some(ch) = chars.next() {
        if in_single {
            current.push(ch);
            if ch == '\'' {
                in_single = false;
                flush_word(
                    &mut output,
                    &current,
                    word_index,
                    after_pipe,
                    true,
                    in_single,
                    in_double,
                );
                current.clear();
            }
            continue;
        }

        if in_double {
            current.push(ch);
            if ch == '"' {
                in_double = false;
                flush_word(
                    &mut output,
                    &current,
                    word_index,
                    after_pipe,
                    true,
                    in_single,
                    in_double,
                );
                current.clear();
            }
            continue;
        }

        match ch {
            '\'' => {
                flush_plain(&mut output, &current);
                current.clear();
                in_single = true;
                current.push(ch);
            }
            '"' => {
                flush_plain(&mut output, &current);
                current.clear();
                in_double = true;
                current.push(ch);
            }
            '|' | '>' | '<' | '&' => {
                flush_word(
                    &mut output,
                    &current,
                    word_index,
                    after_pipe,
                    false,
                    in_single,
                    in_double,
                );
                current.clear();
                if ch == '|' {
                    word_index = 0;
                    after_pipe = true;
                }
                output.push_str(YELLOW);
                if ch == '2' {
                    output.push('2');
                }
                output.push(ch);
                output.push_str(RESET);
            }
            '2' if chars.peek() == Some(&'>') => {
                flush_word(
                    &mut output,
                    &current,
                    word_index,
                    after_pipe,
                    false,
                    in_single,
                    in_double,
                );
                current.clear();
                chars.next();
                output.push_str(YELLOW);
                output.push_str("2>");
                output.push_str(RESET);
            }
            c if c.is_whitespace() => {
                flush_word(
                    &mut output,
                    &current,
                    word_index,
                    after_pipe,
                    false,
                    in_single,
                    in_double,
                );
                current.clear();
                word_index += 1;
                after_pipe = false;
                output.push(c);
            }
            c => current.push(c),
        }
    }

    flush_word(
        &mut output,
        &current,
        word_index,
        after_pipe,
        in_single || in_double,
        in_single,
        in_double,
    );
    output
}

fn flush_plain(output: &mut String, word: &str) {
    if !word.is_empty() {
        output.push_str(word);
    }
}

fn flush_word(
    output: &mut String,
    word: &str,
    word_index: usize,
    after_pipe: bool,
    quoted: bool,
    in_single: bool,
    in_double: bool,
) {
    if word.is_empty() {
        return;
    }

    let color = if quoted || in_single || in_double {
        GREEN
    } else if word_index == 0 || after_pipe {
        if word.starts_with('-') {
            MAGENTA
        } else {
            BOLD_BLUE
        }
    } else if word.contains('/') || word.starts_with('.') || word.starts_with('~') {
        CYAN
    } else if word.starts_with('-') {
        MAGENTA
    } else {
        RESET
    };

    if color == RESET {
        output.push_str(word);
    } else {
        output.push_str(color);
        output.push_str(word);
        output.push_str(RESET);
    }
}

#[cfg(test)]
mod tests {
    use super::highlight_line;

    #[test]
    fn highlight_contains_ansi() {
        let highlighted = highlight_line("echo hello | wc");
        assert!(highlighted.contains("\x1b["));
    }
}
