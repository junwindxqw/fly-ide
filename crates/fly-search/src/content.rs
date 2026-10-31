//! 内容搜索：ripgrep 系 crate + 并行遍历，支持取消与结果上限。
//!
//! 命中上限或收到取消信号时遍历立即停止（WalkState::Quit + sink 中止）。
//! 单文件错误（权限、编码）跳过不中断整体搜索。

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{sinks::UTF8, BinaryDetection, SearcherBuilder};
use ignore::{WalkBuilder, WalkParallel, WalkState};

use crate::files::configure_walker;

/// 内容搜索选项。
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// 默认忽略大小写（`case_sensitive=true` 才区分）。
    pub case_sensitive: bool,
    pub whole_word: bool,
    /// `false` 时按字面量处理（自动转义正则元字符）。
    pub regex: bool,
    pub max_results: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            whole_word: false,
            regex: false,
            max_results: crate::DEFAULT_RESULT_LIMIT,
        }
    }
}

/// 单条匹配：行号 1 起，`text` 截断到 300 字符，匹配位置为字节偏移。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentMatch {
    /// 相对路径（`/` 分隔）。
    pub path: String,
    pub line: u64,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// 搜索汇总。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchSummary {
    pub truncated: bool,
    pub canceled: bool,
    /// 搜索的文件数（不含跳过/出错文件）。
    pub files_searched: usize,
}

/// 搜索错误。
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("非法搜索表达式：{0}")]
    InvalidPattern(String),
}

/// 共享的搜索状态（遍历线程间只读共享）。
struct SearchShared<'a> {
    matcher: &'a grep_regex::RegexMatcher,
    found: &'a AtomicUsize,
    results: &'a Mutex<Vec<ContentMatch>>,
    files_searched: &'a AtomicUsize,
    cancel: &'a AtomicBool,
    max: usize,
    root: &'a Path,
}

/// 每线程访客：持有本线程的 searcher 与待收集结果。
struct SearchVisitor<'a> {
    shared: &'a SearchShared<'a>,
}

impl<'a> ParallelVisitorBuilder<'a> for SearchVisitorBuilder<'a> {
    fn build(&mut self) -> Box<dyn ParallelVisitor + 'a> {
        Box::new(SearchVisitor {
            shared: self.shared,
        })
    }
}

struct SearchVisitorBuilder<'a> {
    shared: &'a SearchShared<'a>,
}

use ignore::{DirEntry, Error as IgnoreError, ParallelVisitor, ParallelVisitorBuilder};

impl ParallelVisitor for SearchVisitor<'_> {
    fn visit(&mut self, entry: Result<DirEntry, IgnoreError>) -> WalkState {
        let shared = self.shared;
        let Ok(entry) = entry else {
            return WalkState::Continue;
        };
        if shared.cancel.load(Ordering::Relaxed)
            || shared.found.load(Ordering::Relaxed) >= shared.max
        {
            return WalkState::Quit;
        }
        let Some(file_type) = entry.file_type() else {
            return WalkState::Continue;
        };
        if !file_type.is_file() {
            return WalkState::Continue;
        }
        let path = entry.path();
        // 跳过超大文件（>1MB），避免二进制/日志拖慢搜索
        if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) > 1_000_000 {
            return WalkState::Continue;
        }
        let mut collected: Vec<ContentMatch> = Vec::new();
        let rel = path
            .strip_prefix(shared.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let mut sink = UTF8(|line, line_text| {
            if shared.cancel.load(Ordering::Relaxed) {
                return Err(std::io::Error::other("canceled"));
            }
            if shared.found.load(Ordering::Relaxed) >= shared.max {
                return Err(std::io::Error::other("canceled"));
            }
            let span = shared
                .matcher
                .find(line_text.as_bytes())
                .map_err(|err| std::io::Error::other(err.to_string()))?;
            if let Some(m) = span {
                shared.found.fetch_add(1, Ordering::Relaxed);
                let text: String = line_text
                    .trim_end_matches(['\r', '\n'])
                    .chars()
                    .take(300)
                    .collect();
                collected.push(ContentMatch {
                    path: rel.clone(),
                    line,
                    text,
                    start: m.start(),
                    end: m.end(),
                });
            }
            Ok(true)
        });
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(b'\x00'))
            .line_number(true)
            .build();
        if searcher
            .search_path(shared.matcher, path, &mut sink)
            .is_ok()
        {
            shared.files_searched.fetch_add(1, Ordering::Relaxed);
        }
        if !collected.is_empty() {
            shared.results.lock().expect("results 锁").extend(collected);
        }
        if shared.found.load(Ordering::Relaxed) >= shared.max {
            WalkState::Quit
        } else {
            WalkState::Continue
        }
    }
}

/// 执行内容搜索，阻塞直到完成/取消/命中上限。
pub fn search_content(
    root: &Path,
    query: &str,
    opts: &SearchOptions,
    cancel: &AtomicBool,
) -> Result<(Vec<ContentMatch>, SearchSummary), SearchError> {
    let mut summary = SearchSummary::default();
    if query.is_empty() {
        return Ok((Vec::new(), summary));
    }
    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(!opts.case_sensitive)
        .word(opts.whole_word)
        .fixed_strings(!opts.regex)
        .build(query)
        .map_err(|err| SearchError::InvalidPattern(err.to_string()))?;

    let found = AtomicUsize::new(0);
    let results: Mutex<Vec<ContentMatch>> = Mutex::new(Vec::new());
    let files_searched = AtomicUsize::new(0);
    let max = opts.max_results.max(1);

    let shared = SearchShared {
        matcher: &matcher,
        found: &found,
        results: &results,
        files_searched: &files_searched,
        cancel,
        max,
        root,
    };

    let mut walker_builder = WalkBuilder::new(root);
    configure_walker(&mut walker_builder);
    let walker: WalkParallel = walker_builder.build_parallel();
    let mut builder = SearchVisitorBuilder { shared: &shared };
    walker.visit(&mut builder);

    let mut matches = results.into_inner().expect("results 锁");
    matches.sort_unstable_by(|a, b| a.path.cmp(&b.path).then_with(|| a.line.cmp(&b.line)));
    summary.truncated = found.load(Ordering::Relaxed) >= max;
    summary.canceled = cancel.load(Ordering::Relaxed);
    summary.files_searched = files_searched.load(Ordering::Relaxed);
    Ok((matches, summary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::AtomicU32;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fly-search-content-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("创建临时目录");
        dir
    }

    fn find(
        repo: &std::path::Path,
        query: &str,
        opts: &SearchOptions,
    ) -> (Vec<ContentMatch>, SearchSummary) {
        search_content(repo, query, opts, &AtomicBool::new(false)).expect("搜索成功")
    }

    #[test]
    fn finds_matches_and_respects_ignore_rules() {
        let root = temp_dir();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("vendor/lib")).unwrap();
        fs::write(root.join("src/a.rs"), "hello world\nHELLO RUST\n").unwrap();
        fs::write(root.join("vendor/lib/x.rs"), "hello vendored\n").unwrap();
        fs::write(root.join(".gitignore"), "vendor/\n").unwrap();

        let (matches, summary) = find(&root, "hello", &SearchOptions::default());
        assert_eq!(matches.len(), 2, "vendor 应被忽略: {matches:?}");
        assert_eq!(matches[0].path, "src/a.rs");
        assert_eq!(matches[0].line, 1);
        assert_eq!(matches[1].line, 2);
        assert!(!summary.truncated);
        assert!(!summary.canceled);
        assert_eq!(summary.files_searched, 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn case_sensitivity_and_whole_word_options() {
        let root = temp_dir();
        fs::write(root.join("f.txt"), "Apple\napple\nPineapple apple\n").unwrap();

        let (sensitive, _) = find(
            &root,
            "apple",
            &SearchOptions {
                case_sensitive: true,
                ..Default::default()
            },
        );
        assert_eq!(sensitive.len(), 2, "区分大小写应排除 Apple");

        let (word, _) = find(
            &root,
            "apple",
            &SearchOptions {
                whole_word: true,
                ..Default::default()
            },
        );
        assert_eq!(
            word.len(),
            3,
            "整词应排除 Pineapple 行首的子串，但保留行尾独立 apple"
        );

        let (insensitive, _) = find(&root, "apple", &SearchOptions::default());
        assert_eq!(insensitive.len(), 3);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn literal_vs_regex_mode() {
        let root = temp_dir();
        fs::write(root.join("f.txt"), "a.c\nabc\n").unwrap();

        let (literal, _) = find(&root, "a.c", &SearchOptions::default());
        assert_eq!(literal.len(), 1, "字面量模式应只匹配 a.c");

        let (regex, _) = find(
            &root,
            "a.c",
            &SearchOptions {
                regex: true,
                ..Default::default()
            },
        );
        assert_eq!(regex.len(), 2, "正则模式 . 匹配任意字符");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn max_results_truncates_and_reports() {
        let root = temp_dir();
        fs::write(
            root.join("f.txt"),
            (0..50).map(|i| format!("needle {i}\n")).collect::<String>(),
        )
        .unwrap();

        let (matches, summary) = find(
            &root,
            "needle",
            &SearchOptions {
                max_results: 5,
                ..Default::default()
            },
        );
        assert_eq!(matches.len(), 5);
        assert!(summary.truncated);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cancel_flag_stops_search() {
        let root = temp_dir();
        fs::write(
            root.join("f.txt"),
            (0..50).map(|i| format!("needle {i}\n")).collect::<String>(),
        )
        .unwrap();
        let cancel = AtomicBool::new(true);
        let (matches, summary) =
            search_content(&root, "needle", &SearchOptions::default(), &cancel).expect("搜索成功");
        assert!(matches.is_empty());
        assert!(summary.canceled);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_query_returns_empty() {
        let root = temp_dir();
        let (matches, _) = find(&root, "", &SearchOptions::default());
        assert!(matches.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn invalid_regex_returns_error() {
        let root = temp_dir();
        let err = search_content(
            &root,
            "(",
            &SearchOptions {
                regex: true,
                ..Default::default()
            },
            &AtomicBool::new(false),
        )
        .expect_err("非法正则应返回错误");
        assert!(matches!(err, SearchError::InvalidPattern(_)));
        let _ = fs::remove_dir_all(&root);
    }
}
