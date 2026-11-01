//! 提交历史（`git log`）解析。
//!
//! 输出格式：`git log -z --pretty=format:%H%x00%h%x00%an%x00%ad%x00%s%x00%D`
//! （%x00 分隔字段、-z 记录间以 NUL 分隔，避免消息内换行/空格歧义）。
//! 日期用 `--date=iso-strict` 便于前端格式化。

/// 一条提交记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    /// 完整哈希。
    pub hash: String,
    /// 短哈希。
    pub short: String,
    pub author: String,
    /// ISO-8601 日期字符串（git --date=iso-strict 原样输出）。
    pub date: String,
    /// 主题行（可能含中文/空格）。
    pub subject: String,
    /// 装饰（分支/HEAD 标记），无则空。
    pub refs: Vec<String>,
}

/// 解析 log 输出。未知/残缺记录跳过。
pub fn parse_log(data: &str) -> Vec<CommitInfo> {
    let mut commits = Vec::new();
    for record in data.split('\0') {
        if record.trim().is_empty() {
            continue;
        }
        let mut fields = record.split('\u{1}');
        let (Some(hash), Some(short), Some(author), Some(date), Some(subject)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            continue;
        };
        let refs = fields
            .next()
            .map(|refs| {
                refs.split(", ")
                    .filter(|r| !r.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if hash.len() < 7 {
            continue;
        }
        commits.push(CommitInfo {
            hash: hash.to_string(),
            short: short.to_string(),
            author: author.to_string(),
            date: date.to_string(),
            subject: subject.to_string(),
            refs,
        });
    }
    commits
}

/// 构造 log 命令参数（限制条数）。
pub fn log_args(limit: usize) -> Vec<String> {
    vec![
        "log".into(),
        "-z".into(),
        "--date=iso-strict".into(),
        format!("--max-count={limit}"),
        "--pretty=format:%H\u{1}%h\u{1}%an\u{1}%ad\u{1}%s\u{1}%D".into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = concat!(
        "0123456789abcdef0123456789abcdef01234567\u{1}0123456\u{1}fly\u{1}",
        "2026-11-01T10:00:00+08:00\u{1}feat: 初始提交\u{1}HEAD -> main, origin/main\u{1}",
        "\0",
        "1111111111111111111111111111111111111111\u{1}1111111\u{1}other\u{1}",
        "2026-10-30T09:00:00+08:00\u{1}fix: 修复\u{1}\u{1}",
        "\0"
    );

    #[test]
    fn parses_multiple_commits_with_refs() {
        let commits = parse_log(SAMPLE);
        assert_eq!(commits.len(), 2);
        let first = &commits[0];
        assert_eq!(first.hash.len(), 40);
        assert_eq!(first.short, "0123456");
        assert_eq!(first.author, "fly");
        assert_eq!(first.date, "2026-11-01T10:00:00+08:00");
        assert_eq!(first.subject, "feat: 初始提交");
        assert_eq!(first.refs, vec!["HEAD -> main", "origin/main"]);

        let second = &commits[1];
        assert_eq!(second.subject, "fix: 修复");
        assert!(second.refs.is_empty());
    }

    #[test]
    fn empty_and_malformed_records_are_skipped() {
        assert!(parse_log("").is_empty());
        assert!(parse_log("\0\0").is_empty());
        // 字段不足的记录跳过
        assert!(parse_log("only-two\u{1}fields\0").is_empty());
    }

    #[test]
    fn log_args_contain_limit_and_format() {
        let args = log_args(50);
        assert!(args.iter().any(|a| a == "--max-count=50"));
        assert!(args.iter().any(|a| a.contains("%H")));
    }
}
