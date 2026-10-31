//! `git diff --numstat -z` 的模型与解析。
//!
//! `-z` 模式下记录格式（NUL 分隔，路径不做转义）：
//! - 普通记录：`added\tdeleted\tpath\0`
//! - 重命名/复制：`added\tdeleted\t\0origPath\0path\0`——第三个字段为**空**，
//!   随后先原始路径、后新路径（实测 git 2.54 输出，勿凭记忆改回）；
//! - 二进制文件增删行字段为 `-`，解析为 `None`。

/// 增删行统计。`added`/`deleted` 为 `None` 表示二进制文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumStat {
    pub added: Option<u32>,
    pub deleted: Option<u32>,
    /// 当前路径；重命名时为新路径。
    pub path: String,
    /// 重命名/复制时的原始路径。
    pub orig_path: Option<String>,
}

/// 解析 numstat（`-z`）输出。未知 token 跳过，保证向前兼容。
pub fn parse_numstat(data: &str) -> Vec<NumStat> {
    let mut result = Vec::new();
    let mut tokens = data.split('\0').filter(|t| !t.is_empty()).peekable();

    while let Some(token) = tokens.next() {
        let mut parts = token.splitn(3, '\t');
        let (Some(added), Some(deleted), Some(third)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let (path, orig_path) = if third.is_empty() {
            // 重命名记录：紧随其后的是 origPath 与 path 两个字段
            match (tokens.next(), tokens.next()) {
                (Some(orig), Some(path)) => (path, Some(orig.to_string())),
                _ => continue,
            }
        } else {
            (third, None)
        };
        result.push(NumStat {
            added: parse_count(added),
            deleted: parse_count(deleted),
            path: path.to_string(),
            orig_path,
        });
    }
    result
}

fn parse_count(field: &str) -> Option<u32> {
    if field == "-" {
        None
    } else {
        field.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_counts_binary_and_rename() {
        let stats = parse_numstat("3\t1\tsrc/main.rs\0-\t-\t\0old.png\0new.png\0");
        assert_eq!(stats.len(), 2);

        assert_eq!(stats[0].added, Some(3));
        assert_eq!(stats[0].deleted, Some(1));
        assert_eq!(stats[0].path, "src/main.rs");
        assert_eq!(stats[0].orig_path, None);

        assert_eq!(stats[1].added, None);
        assert_eq!(stats[1].deleted, None);
        assert_eq!(stats[1].path, "new.png");
        assert_eq!(stats[1].orig_path.as_deref(), Some("old.png"));
    }

    #[test]
    fn zero_delta_rename_is_supported() {
        let stats = parse_numstat("0\t0\t\0old.txt\0new.txt\0");
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].added, Some(0));
        assert_eq!(stats[0].deleted, Some(0));
        assert_eq!(stats[0].path, "new.txt");
        assert_eq!(stats[0].orig_path.as_deref(), Some("old.txt"));
    }

    #[test]
    fn paths_with_spaces_and_malformed_tokens() {
        let stats = parse_numstat("1\t2\tdir with space/a.rs\0broken token\0");
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].path, "dir with space/a.rs");
    }

    #[test]
    fn empty_input_yields_nothing() {
        assert!(parse_numstat("").is_empty());
        assert!(parse_numstat("\0\0").is_empty());
    }
}
