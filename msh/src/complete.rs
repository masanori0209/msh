use crate::path_cache;
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::Context;
use std::collections::HashMap;

pub struct MshCompleter {
    filename: FilenameCompleter,
    aliases: HashMap<String, String>,
    fuzzy: bool,
}

enum CompletionKind {
    Command,
    Path,
    Argument(String),
}

/// よく使うコマンドの第 1 引数（サブコマンド）候補。`(候補, 説明)`。
fn subcommands(command: &str) -> Option<&'static [(&'static str, &'static str)]> {
    match command {
        "git" => Some(&[
            ("status", "作業ツリーの状態"),
            ("add", "変更をステージ"),
            ("commit", "コミット作成"),
            ("push", "リモートへ反映"),
            ("pull", "リモートを取得+統合"),
            ("fetch", "リモートを取得"),
            ("checkout", "ブランチ切替/復元"),
            ("switch", "ブランチ切替"),
            ("branch", "ブランチ操作"),
            ("merge", "マージ"),
            ("rebase", "リベース"),
            ("log", "履歴表示"),
            ("diff", "差分表示"),
            ("stash", "退避"),
            ("clone", "クローン"),
            ("restore", "ファイル復元"),
            ("reset", "リセット"),
            ("tag", "タグ操作"),
        ]),
        "cargo" => Some(&[
            ("build", "ビルド"),
            ("run", "実行"),
            ("test", "テスト"),
            ("check", "型チェック"),
            ("clippy", "lint"),
            ("fmt", "整形"),
            ("bench", "ベンチ"),
            ("add", "依存追加"),
            ("remove", "依存削除"),
            ("update", "依存更新"),
            ("doc", "ドキュメント生成"),
            ("clean", "成果物削除"),
            ("publish", "公開"),
            ("install", "インストール"),
        ]),
        "docker" => Some(&[
            ("ps", "コンテナ一覧"),
            ("images", "イメージ一覧"),
            ("build", "イメージビルド"),
            ("run", "コンテナ起動"),
            ("exec", "コンテナ内実行"),
            ("logs", "ログ表示"),
            ("pull", "イメージ取得"),
            ("push", "イメージ送信"),
            ("stop", "停止"),
            ("start", "開始"),
            ("rm", "コンテナ削除"),
            ("rmi", "イメージ削除"),
            ("compose", "Compose 操作"),
        ]),
        "npm" => Some(&[
            ("install", "依存インストール"),
            ("run", "スクリプト実行"),
            ("test", "テスト"),
            ("start", "起動"),
            ("build", "ビルド"),
            ("init", "初期化"),
            ("publish", "公開"),
            ("update", "更新"),
        ]),
        _ => None,
    }
}

impl MshCompleter {
    pub fn new(aliases: &HashMap<String, String>, fuzzy: bool) -> Self {
        Self {
            filename: FilenameCompleter::new(),
            aliases: aliases.clone(),
            fuzzy,
        }
    }

    pub fn refresh(&mut self, aliases: &HashMap<String, String>) {
        self.aliases.clone_from(aliases);
    }
}

impl Completer for MshCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let (start, word, kind) = completion_context(line, pos);
        match kind {
            CompletionKind::Command => {
                let matches: Vec<Pair> =
                    path_cache::complete_commands(word, &self.aliases, self.fuzzy)
                        .into_iter()
                        .map(|candidate| Pair {
                            display: candidate.display,
                            replacement: candidate.replacement,
                        })
                        .collect();
                Ok((start, matches))
            }
            CompletionKind::Argument(command) => {
                let matches = self.subcommand_matches(&command, word);
                if matches.is_empty() {
                    self.filename.complete(line, pos, ctx)
                } else {
                    Ok((start, matches))
                }
            }
            CompletionKind::Path => self.filename.complete(line, pos, ctx),
        }
    }
}

impl MshCompleter {
    fn subcommand_matches(&self, command: &str, word: &str) -> Vec<Pair> {
        let Some(entries) = subcommands(command) else {
            return Vec::new();
        };
        let lower = word.to_ascii_lowercase();
        entries
            .iter()
            .filter(|(name, _)| {
                if self.fuzzy {
                    name.to_ascii_lowercase().contains(&lower)
                } else {
                    name.to_ascii_lowercase().starts_with(&lower)
                }
            })
            .map(|(name, desc)| Pair {
                display: format!("{name}  — {desc}"),
                replacement: (*name).to_string(),
            })
            .collect()
    }
}

fn completion_context(line: &str, pos: usize) -> (usize, &str, CompletionKind) {
    let pos = pos.min(line.len());
    let prefix = &line[..pos];
    let segment_start = prefix.rfind('|').map(|index| index + 1).unwrap_or(0);
    let segment = &line[segment_start..pos];

    let word_start = segment
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(index, _)| segment_start + index + 1)
        .unwrap_or(segment_start);

    let word = &line[word_start..pos];
    let trimmed = segment.trim_start();
    let is_command_word = !trimmed.contains(char::is_whitespace);

    let path_like = word.starts_with('-')
        || word.contains('/')
        || word.starts_with('~')
        || word.starts_with('"')
        || word.starts_with('\'');

    let kind = if is_command_word && !path_like {
        CompletionKind::Command
    } else if !path_like && is_first_argument(segment, word_start - segment_start) {
        // 第 1 引数を補完中。既知コマンドならサブコマンド候補を出す。
        match trimmed.split_whitespace().next() {
            Some(cmd) if subcommands(cmd).is_some() => CompletionKind::Argument(cmd.to_string()),
            _ => CompletionKind::Path,
        }
    } else {
        CompletionKind::Path
    };

    (word_start, word, kind)
}

/// 補完中の単語がコマンドの第 1 引数か（前に語が 1 つ＝コマンド名だけか）を判定する。
fn is_first_argument(segment: &str, word_offset_in_segment: usize) -> bool {
    segment[..word_offset_in_segment].split_whitespace().count() == 1
}

#[cfg(test)]
mod tests {
    use super::completion_context;

    #[test]
    fn first_word_is_command() {
        let (_, word, kind) = completion_context("ec", 2);
        assert_eq!(word, "ec");
        assert!(matches!(kind, super::CompletionKind::Command));
    }

    #[test]
    fn git_first_argument_is_subcommand() {
        let line = "git st";
        let (_, word, kind) = completion_context(line, line.len());
        assert_eq!(word, "st");
        match kind {
            super::CompletionKind::Argument(cmd) => assert_eq!(cmd, "git"),
            _ => panic!("expected Argument(git)"),
        }
    }

    #[test]
    fn unknown_command_argument_is_path() {
        let line = "echo fo";
        let (_, _, kind) = completion_context(line, line.len());
        assert!(matches!(kind, super::CompletionKind::Path));
    }

    #[test]
    fn second_argument_is_path() {
        let line = "git commit fo";
        let (_, _, kind) = completion_context(line, line.len());
        assert!(matches!(kind, super::CompletionKind::Path));
    }

    #[test]
    fn subcommand_filtering() {
        let completer = super::MshCompleter::new(&std::collections::HashMap::new(), false);
        let matches = completer.subcommand_matches("git", "c");
        let names: Vec<_> = matches.iter().map(|p| p.replacement.clone()).collect();
        assert!(names.contains(&"commit".to_string()));
        assert!(names.contains(&"checkout".to_string()));
        assert!(names.contains(&"clone".to_string()));
        assert!(!names.contains(&"status".to_string()));
    }
}
