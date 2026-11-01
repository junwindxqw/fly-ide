//! `git blame --line-porcelain` 解析。
//!
//! porcelain 输出：每行一段 `<40位hash> <原行号> <结果行号> [<次数>]` 头，
//! 后随 `author `、`author-time `、`summary ` 等键值行，内容行以 `\t` 开头。
//! 我们只取每行的作者/时间/主题。

/// 单行的 blame 信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    /// 完整哈希。
    pub hash: String,
    pub author: String,
    /// Unix 时间戳（秒）。
    pub time: i64,
    /// 提交主题。
    pub summary: String,
    /// 结果行号（1 起）。
    pub line: u32,
}

/// 解析 blame porcelain 输出为按结果行号索引的信息列表。
pub fn parse_blame(data: &str) -> Vec<BlameLine> {
    let mut lines: Vec<BlameLine> = Vec::new();
    let mut current: Option<(String, String, i64, String)> = None; // hash, author, time, summary
    let mut current_line_no: u32 = 0;

    for raw in data.lines() {
        if raw.len() >= 40 {
            let head = &raw[..40];
            if head.bytes().all(|b| b.is_ascii_hexdigit()) {
                // 新段头：flush 上一段
                if let Some((hash, author, time, summary)) = current.take() {
                    if current_line_no > 0 {
                        lines.push(BlameLine {
                            hash,
                            author,
                            time,
                            summary,
                            line: current_line_no,
                        });
                    }
                }
                let rest = raw[41..].trim_start();
                // `<原行号> <结果行号> [<合并次数>]`
                if let Some(result_no) = rest.split_whitespace().nth(1) {
                    current_line_no = result_no.parse().unwrap_or(0);
                }
                current = Some((head.to_string(), String::new(), 0, String::new()));
                continue;
            }
        }
        if let Some((_, author, time, summary)) = current.as_mut() {
            if let Some(value) = raw.strip_prefix("author ") {
                *author = value.to_string();
            } else if let Some(value) = raw.strip_prefix("author-time ") {
                *time = value.parse().unwrap_or(0);
            } else if let Some(value) = raw.strip_prefix("summary ") {
                *summary = value.to_string();
            }
        }
    }
    if let Some((hash, author, time, summary)) = current.take() {
        if current_line_no > 0 {
            lines.push(BlameLine {
                hash,
                author,
                time,
                summary,
                line: current_line_no,
            });
        }
    }
    lines
}

/// 构造 blame 命令参数（`-w` 忽略空白，与审阅语义一致）。
pub fn blame_args(path: &str) -> Vec<String> {
    vec![
        "blame".into(),
        "--line-porcelain".into(),
        "-w".into(),
        "--".into(),
        path.into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = concat!(
        "0123456789abcdef0123456789abcdef01234567 1 1 2\n",
        "author fly\n",
        "author-mail <fly@example.com>\n",
        "author-time 1798000000\n",
        "author-tz +0800\n",
        "summary feat: 初始提交\n",
        "\tfn main() {}\n",
        "1111111111111111111111111111111111111111 2 2\n",
        "author other\n",
        "author-mail <other@example.com>\n",
        "author-time 1799000000\n",
        "author-tz +0800\n",
        "summary fix: 第二次\n",
        "\tlet x = 1;\n"
    );

    #[test]
    fn parses_authors_times_summaries() {
        let blame = parse_blame(SAMPLE);
        assert_eq!(blame.len(), 2);
        assert_eq!(blame[0].line, 1);
        assert_eq!(blame[0].author, "fly");
        assert_eq!(blame[0].time, 1798000000);
        assert_eq!(blame[0].summary, "feat: 初始提交");
        assert_eq!(blame[1].line, 2);
        assert_eq!(blame[1].author, "other");
        assert_eq!(blame[1].hash.len(), 40);
    }

    #[test]
    fn empty_input_yields_nothing() {
        assert!(parse_blame("").is_empty());
    }

    #[test]
    fn blame_args_target_path() {
        let args = blame_args("src/main.rs");
        assert!(args.iter().any(|a| a == "--line-porcelain"));
        assert!(args.iter().any(|a| a == "src/main.rs"));
    }
}
