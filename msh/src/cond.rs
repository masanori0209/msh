//! `[[ ... ]]` 条件式の最小評価器。
//!
//! bash の `[[ ]]` 構文のうち、移行で頻出する単項・二項演算をサポートする。
//! glob パターンマッチや `-a`/`-o` の論理結合は未対応（必要時は `if cmd; then` で代替）。

use std::path::Path;

/// `[[` と `]]` の間のトークン列を評価し、終了ステータス（0=真, 1=偽, 2=構文エラー）を返す。
pub fn eval(tokens: &[String]) -> i32 {
    // 先頭の否定 `!`
    if let Some(first) = tokens.first() {
        if first == "!" {
            return match eval(&tokens[1..]) {
                0 => 1,
                1 => 0,
                other => other,
            };
        }
    }

    match tokens.len() {
        0 => 1,
        1 => bool_to_status(!tokens[0].is_empty()),
        2 => eval_unary(&tokens[0], &tokens[1]),
        3 => eval_binary(&tokens[0], &tokens[1], &tokens[2]),
        _ => 2,
    }
}

fn eval_unary(op: &str, operand: &str) -> i32 {
    let result = match op {
        "-z" => operand.is_empty(),
        "-n" => !operand.is_empty(),
        "-e" => Path::new(operand).exists(),
        "-f" => Path::new(operand).is_file(),
        "-d" => Path::new(operand).is_dir(),
        "-r" | "-w" | "-x" => Path::new(operand).exists(),
        "-s" => Path::new(operand)
            .metadata()
            .map(|m| m.len() > 0)
            .unwrap_or(false),
        _ => return 2,
    };
    bool_to_status(result)
}

fn eval_binary(lhs: &str, op: &str, rhs: &str) -> i32 {
    let result = match op {
        "=" | "==" => lhs == rhs,
        "!=" => lhs != rhs,
        "<" => lhs < rhs,
        ">" => lhs > rhs,
        "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" => {
            let (Ok(a), Ok(b)) = (lhs.parse::<i64>(), rhs.parse::<i64>()) else {
                return 2;
            };
            match op {
                "-eq" => a == b,
                "-ne" => a != b,
                "-lt" => a < b,
                "-le" => a <= b,
                "-gt" => a > b,
                "-ge" => a >= b,
                _ => unreachable!(),
            }
        }
        _ => return 2,
    };
    bool_to_status(result)
}

fn bool_to_status(b: bool) -> i32 {
    if b {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn string_equality() {
        assert_eq!(eval(&s(&["abc", "==", "abc"])), 0);
        assert_eq!(eval(&s(&["abc", "==", "xyz"])), 1);
        assert_eq!(eval(&s(&["a", "!=", "b"])), 0);
    }

    #[test]
    fn emptiness() {
        assert_eq!(eval(&s(&["-z", ""])), 0);
        assert_eq!(eval(&s(&["-n", "x"])), 0);
        assert_eq!(eval(&s(&["-z", "x"])), 1);
    }

    #[test]
    fn numeric_comparison() {
        assert_eq!(eval(&s(&["3", "-lt", "5"])), 0);
        assert_eq!(eval(&s(&["5", "-le", "5"])), 0);
        assert_eq!(eval(&s(&["5", "-gt", "9"])), 1);
    }

    #[test]
    fn negation() {
        assert_eq!(eval(&s(&["!", "-z", "x"])), 0);
        assert_eq!(eval(&s(&["!", "abc", "==", "abc"])), 1);
    }

    #[test]
    fn existing_directory() {
        assert_eq!(eval(&s(&["-d", "/tmp"])), 0);
        assert_eq!(eval(&s(&["-d", "/no/such/path/xyz"])), 1);
    }

    #[test]
    fn single_token_truthiness() {
        assert_eq!(eval(&s(&["nonempty"])), 0);
        assert_eq!(eval(&s(&[""])), 1);
    }
}
