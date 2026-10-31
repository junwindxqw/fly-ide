//! 文件路径索引与模糊匹配（快速打开）。

use std::path::Path;

use ignore::WalkBuilder;
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// 构建文件路径索引：相对路径（`/` 分隔）、排序、尊重 .gitignore 与默认排除。
pub fn build_file_index(root: &Path) -> std::io::Result<Vec<String>> {
    let mut files = Vec::new();
    let mut builder = WalkBuilder::new(root);
    configure_walker(&mut builder);
    for entry in builder.build() {
        let entry = entry.map_err(|err| std::io::Error::other(err.to_string()))?;
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
        files.push(rel.to_string_lossy().replace('\\', "/"));
    }
    files.sort_unstable();
    Ok(files)
}

/// 共用的遍历器配置：尊重 .gitignore、跳过 .git 与默认排除目录。
///
/// `filter_entry` 返回 `&mut`，因此调用方自建 builder 后交给本函数配置。
pub fn configure_walker(builder: &mut WalkBuilder) -> &mut WalkBuilder {
    builder.filter_entry(|entry| {
        let name = entry.file_name().to_string_lossy();
        name != ".git" && !fly_workspace::is_default_excluded(&name)
    })
}

/// 模糊匹配一条路径的得分。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMatch {
    pub path: String,
    pub score: u32,
}

/// 对索引执行模糊匹配，按得分降序返回前 `limit` 条；空查询返回索引前缀。
pub fn fuzzy_search(files: &[String], query: &str, limit: usize) -> Vec<FileMatch> {
    if query.trim().is_empty() {
        return files
            .iter()
            .take(limit)
            .map(|path| FileMatch {
                path: path.clone(),
                score: 0,
            })
            .collect();
    }
    let pattern = Pattern::new(
        query,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let mut char_buf = Vec::new();
    let mut scored: Vec<FileMatch> = files
        .iter()
        .filter_map(|path| {
            let haystack = Utf32Str::new(path, &mut char_buf);
            pattern
                .score(haystack, &mut matcher)
                .map(|score| FileMatch {
                    path: path.clone(),
                    score,
                })
        })
        .collect();
    scored.sort_unstable_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    scored.truncate(limit);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fly-search-files-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("创建临时目录");
        dir
    }

    #[test]
    fn index_respects_gitignore_and_excludes() {
        let root = temp_dir();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::create_dir_all(root.join(".git/objects")).unwrap();
        fs::create_dir_all(root.join("generated")).unwrap();
        fs::write(root.join("src/main.rs"), "x").unwrap();
        fs::write(root.join("node_modules/pkg/index.js"), "x").unwrap();
        fs::write(root.join(".git/objects/ab"), "x").unwrap();
        fs::write(root.join("generated/out.txt"), "x").unwrap();
        fs::write(root.join(".gitignore"), "generated/\n").unwrap();

        let index = build_file_index(&root).expect("构建索引");
        assert_eq!(index, vec!["src/main.rs"], ".gitignore/排除规则应生效");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn fuzzy_search_ranks_path_segments() {
        let files: Vec<String> = vec![
            "docs/readme.md".into(),
            "src/app/main.rs".into(),
            "src/application/service.rs".into(),
            "zzz/unrelated.txt".into(),
        ];
        let matches = fuzzy_search(&files, "app", 10);
        assert!(!matches.is_empty());
        // "app" 完整出现在 app/main.rs，应排在子串匹配之前
        assert_eq!(matches[0].path, "src/app/main.rs");
        assert!(matches
            .iter()
            .any(|m| m.path == "src/application/service.rs"));
        assert!(!matches.iter().any(|m| m.path == "zzz/unrelated.txt"));
    }

    #[test]
    fn empty_query_returns_prefix_of_index() {
        let files: Vec<String> = (0..10).map(|i| format!("f{i}.txt")).collect();
        let matches = fuzzy_search(&files, "  ", 3);
        assert_eq!(matches.len(), 3);
    }
}
