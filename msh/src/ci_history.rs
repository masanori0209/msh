//! rustyline 履歴の大小文字無視検索（regex 依存なし）。
use rustyline::history::{DefaultHistory, History, SearchDirection, SearchResult};
use rustyline::Result;
use std::path::Path;

/// `DefaultHistory` をラップし、`search` / `starts_with` のみ ASCII 大小無視に差し替える。
pub struct CaseInsensitiveHistory {
    inner: DefaultHistory,
}

impl CaseInsensitiveHistory {
    pub fn with_config(config: rustyline::Config) -> Self {
        Self {
            inner: DefaultHistory::with_config(config),
        }
    }
}

impl History for CaseInsensitiveHistory {
    fn get(&self, index: usize, dir: SearchDirection) -> Result<Option<SearchResult<'_>>> {
        self.inner.get(index, dir)
    }

    fn add(&mut self, line: &str) -> Result<bool> {
        self.inner.add(line)
    }

    fn add_owned(&mut self, line: String) -> Result<bool> {
        self.inner.add_owned(line)
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn set_max_len(&mut self, len: usize) -> Result<()> {
        self.inner.set_max_len(len)
    }

    fn ignore_dups(&mut self, yes: bool) -> Result<()> {
        self.inner.ignore_dups(yes)
    }

    fn ignore_space(&mut self, yes: bool) {
        self.inner.ignore_space(yes)
    }

    fn save(&mut self, path: &Path) -> Result<()> {
        self.inner.save(path)
    }

    fn append(&mut self, path: &Path) -> Result<()> {
        self.inner.append(path)
    }

    fn load(&mut self, path: &Path) -> Result<()> {
        self.inner.load(path)
    }

    fn clear(&mut self) -> Result<()> {
        self.inner.clear()
    }

    fn search(
        &self,
        term: &str,
        start: usize,
        dir: SearchDirection,
    ) -> Result<Option<SearchResult<'_>>> {
        let test = |entry: &str| ascii_find_insensitive(entry, term);
        Ok(search_match(self, term, start, dir, test))
    }

    fn starts_with(
        &self,
        term: &str,
        start: usize,
        dir: SearchDirection,
    ) -> Result<Option<SearchResult<'_>>> {
        let test = |entry: &str| ascii_starts_with_insensitive(entry, term);
        Ok(search_match(self, term, start, dir, test))
    }
}

fn search_match<'h, F>(
    history: &'h CaseInsensitiveHistory,
    term: &str,
    start: usize,
    dir: SearchDirection,
    test: F,
) -> Option<SearchResult<'h>>
where
    F: Fn(&str) -> Option<usize>,
{
    let len = history.len();
    if term.is_empty() || start >= len {
        return None;
    }
    match dir {
        SearchDirection::Reverse => {
            for offset in 0..=start {
                let idx = start - offset;
                if let Some(result) = history.get(idx, SearchDirection::Reverse).ok().flatten() {
                    if let Some(pos) = test(&result.entry) {
                        return Some(SearchResult {
                            idx,
                            entry: result.entry,
                            pos,
                        });
                    }
                }
            }
            None
        }
        SearchDirection::Forward => {
            for idx in start..len {
                if let Some(result) = history.get(idx, SearchDirection::Forward).ok().flatten() {
                    if let Some(pos) = test(&result.entry) {
                        return Some(SearchResult {
                            idx,
                            entry: result.entry,
                            pos,
                        });
                    }
                }
            }
            None
        }
    }
}

/// 部分文字列検索（ASCII 大小文字無視）。ヒット位置（バイトオフセット）を返す。
pub fn ascii_find_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let needle_bytes = needle.as_bytes();
    if needle_bytes.iter().any(|b| !b.is_ascii()) {
        return ascii_find_insensitive_unicode(haystack, needle);
    }
    let hay = haystack.as_bytes();
    if needle_bytes.len() > hay.len() {
        return None;
    }
    for i in 0..=hay.len() - needle_bytes.len() {
        if hay[i..i + needle_bytes.len()]
            .iter()
            .zip(needle_bytes.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            return Some(i);
        }
    }
    None
}

fn ascii_find_insensitive_unicode(haystack: &str, needle: &str) -> Option<usize> {
    let needle_len = needle.chars().count();
    for (byte_idx, _) in haystack.char_indices() {
        let tail = &haystack[byte_idx..];
        if tail
            .chars()
            .zip(needle.chars())
            .take(needle_len)
            .all(|(a, b)| a.eq_ignore_ascii_case(&b))
            && tail.chars().take(needle_len).count() == needle_len
        {
            return Some(byte_idx);
        }
    }
    None
}

/// 先頭一致（ASCII 大小文字無視）。マッチ終端位置（バイト長）を返す。
pub fn ascii_starts_with_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let mut hi = haystack.chars();
    for n in needle.chars() {
        match hi.next() {
            Some(h) if h.eq_ignore_ascii_case(&n) => {}
            _ => return None,
        }
    }
    Some(needle.len())
}

/// プレビューヒント用: `line` で始まる最新履歴エントリを逆順に探す。
pub fn find_history_prefix(history: &dyn History, line: &str) -> Option<String> {
    if line.is_empty() {
        return None;
    }
    let len = history.len();
    (0..len).rev().find_map(|idx| {
        let result = history.get(idx, SearchDirection::Reverse).ok().flatten()?;
        if result.entry.len() > line.len()
            && ascii_starts_with_insensitive(&result.entry, line).is_some()
        {
            Some(result.entry.into_owned())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyline::Config;

    fn test_config() -> Config {
        Config::builder().max_history_size(100).unwrap().build()
    }

    #[test]
    fn case_insensitive_search_finds_substring() {
        let mut history = CaseInsensitiveHistory::with_config(test_config());
        history.add("git STATUS").unwrap();
        history.add("cargo build").unwrap();
        let hit = history
            .search("status", 1, SearchDirection::Reverse)
            .unwrap()
            .unwrap();
        assert_eq!(hit.entry, "git STATUS");
    }

    #[test]
    fn case_insensitive_starts_with() {
        assert_eq!(ascii_starts_with_insensitive("Git Status", "git"), Some(3));
        assert!(ascii_starts_with_insensitive("cargo", "git").is_none());
    }

    #[test]
    fn ascii_find_insensitive_locates_substring() {
        assert_eq!(ascii_find_insensitive("foo BAR baz", "bar"), Some(4));
    }
}
