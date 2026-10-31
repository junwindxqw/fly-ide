//! `git status --porcelain=v2 -z` 的模型与解析。
//!
//! 解析是纯字节级函数，不依赖子进程；`-z` 模式下所有记录以 NUL 分隔、
//! 路径不做转义（可含空格/中文），重命名记录的原始路径是紧随其后的下一个
//! NUL 字段。对未知记录跳过，保证 git 新版本向前兼容。

/// porcelain v2 状态码对应的文件状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Unmodified,
    Added,
    Copied,
    Deleted,
    Modified,
    Renamed,
    TypeChanged,
    Unmerged,
    Untracked,
    Ignored,
    Unknown,
}

impl FileStatus {
    /// 还原为 porcelain 状态码字符。
    pub const fn code(self) -> char {
        match self {
            Self::Unmodified => '.',
            Self::Added => 'A',
            Self::Copied => 'C',
            Self::Deleted => 'D',
            Self::Modified => 'M',
            Self::Renamed => 'R',
            Self::TypeChanged => 'T',
            Self::Unmerged => 'U',
            Self::Untracked => '?',
            Self::Ignored => '!',
            Self::Unknown => 'X',
        }
    }
}

impl From<char> for FileStatus {
    fn from(code: char) -> Self {
        match code {
            '.' => Self::Unmodified,
            'A' => Self::Added,
            'C' => Self::Copied,
            'D' => Self::Deleted,
            'M' => Self::Modified,
            'R' => Self::Renamed,
            'T' => Self::TypeChanged,
            'U' => Self::Unmerged,
            '?' => Self::Untracked,
            '!' => Self::Ignored,
            _ => Self::Unknown,
        }
    }
}

/// 单个文件的变更状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    /// 暂存区状态（X）。
    pub index: FileStatus,
    /// 工作区状态（Y）。
    pub worktree: FileStatus,
    /// 当前路径；重命名时为新路径。
    pub path: String,
    /// 重命名/复制时的原始路径。
    pub orig_path: Option<String>,
    pub untracked: bool,
    pub unmerged: bool,
}

/// 一次 `git status --branch` 的完整结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoStatus {
    /// 当前分支名；detached HEAD 时为 `None`。
    pub branch: Option<String>,
    /// HEAD 提交；空仓库（无提交）时为 `None`。
    pub oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub entries: Vec<StatusEntry>,
}

/// 解析 porcelain v2（`-z`，NUL 分隔）输出。
pub fn parse_status(data: &str) -> RepoStatus {
    let mut status = RepoStatus::default();
    let mut tokens = data.split('\0').filter(|t| !t.is_empty());

    while let Some(token) = tokens.next() {
        if let Some(header) = token.strip_prefix("# branch.") {
            parse_header(header, &mut status);
            continue;
        }
        if let Some(rest) = token.strip_prefix("1 ") {
            // 1 XY sub mH mI mW hH hI path
            let mut parts = rest.splitn(8, ' ');
            if let (Some(xy), Some(path)) = (parts.next(), parts.nth(6)) {
                status
                    .entries
                    .push(entry_from_xy(xy, path, None, false, false));
            }
        } else if let Some(rest) = token.strip_prefix("2 ") {
            // 2 XY sub mH mI mW hH hI Xscore path\0origPath
            // 注意 X 与相似度合为一个字段（如 "R100"），共 9 个字段
            let mut parts = rest.splitn(9, ' ');
            if let (Some(xy), Some(path)) = (parts.next(), parts.nth(7)) {
                let orig = tokens.next().unwrap_or_default().to_string();
                status
                    .entries
                    .push(entry_from_xy(xy, path, Some(orig), false, false));
            }
        } else if let Some(rest) = token.strip_prefix("u ") {
            // u XY sub m1 m2 m3 mW h1 h2 h3 path
            let mut parts = rest.splitn(10, ' ');
            if let (Some(xy), Some(path)) = (parts.next(), parts.nth(8)) {
                status
                    .entries
                    .push(entry_from_xy(xy, path, None, false, true));
            }
        } else if let Some(path) = token.strip_prefix("? ") {
            status.entries.push(StatusEntry {
                index: FileStatus::Untracked,
                worktree: FileStatus::Untracked,
                path: path.to_string(),
                orig_path: None,
                untracked: true,
                unmerged: false,
            });
        }
        // 其余记录（"! " ignored、其他 "# " 头）跳过
    }
    status
}

fn parse_header(header: &str, status: &mut RepoStatus) {
    match header.split_once(' ') {
        Some(("oid", v)) if v != "(initial)" => status.oid = Some(v.to_string()),
        Some(("head", v)) if v != "(detached)" => status.branch = Some(v.to_string()),
        Some(("upstream", v)) => status.upstream = Some(v.to_string()),
        Some(("ab", v)) => {
            // 形如 "+2 -1"
            for part in v.split_whitespace() {
                let (sign, digits) = part.split_at(part.len().min(1));
                match (sign, digits.parse::<u32>()) {
                    ("+", Ok(n)) => status.ahead = Some(n),
                    ("-", Ok(n)) => status.behind = Some(n),
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn entry_from_xy(
    xy: &str,
    path: &str,
    orig: Option<String>,
    untracked: bool,
    unmerged: bool,
) -> StatusEntry {
    let mut chars = xy.chars();
    let x = chars.next().unwrap_or('.');
    let y = chars.next().unwrap_or('.');
    StatusEntry {
        index: FileStatus::from(x),
        worktree: FileStatus::from(y),
        path: path.to_string(),
        orig_path: orig,
        untracked,
        unmerged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = concat!(
        "# branch.oid 0123456789abcdef\0",
        "# branch.head main\0",
        "# branch.upstream origin/main\0",
        "# branch.ab +2 -1\0",
        "1 M. N... 000000 100644 100644 AAA BBB src/main.rs\0",
        "1 .M N... 000000 100644 100644 AAA BBB lib with space.rs\0",
        "2 R. N... 100644 100644 100644 AAA BBB R100 new name.txt\0old name.txt\0",
        "? untracked dir/file with space.rs\0",
        "u UU N... 111111 222222 333333 444444 555555 666666 777777 conflict file.rs\0",
    );

    #[test]
    fn parses_branch_headers() {
        let status = parse_status(SAMPLE);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.oid.as_deref(), Some("0123456789abcdef"));
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!(status.ahead, Some(2));
        assert_eq!(status.behind, Some(1));
    }

    #[test]
    fn parses_all_entry_kinds_with_spaces_in_paths() {
        let status = parse_status(SAMPLE);
        assert_eq!(status.entries.len(), 5);

        let staged = &status.entries[0];
        assert_eq!(staged.index, FileStatus::Modified);
        assert_eq!(staged.worktree, FileStatus::Unmodified);
        assert_eq!(staged.path, "src/main.rs");

        // -z 模式路径不做转义，空格原样保留
        assert_eq!(status.entries[1].path, "lib with space.rs");
        assert_eq!(status.entries[1].worktree, FileStatus::Modified);

        let renamed = &status.entries[2];
        assert_eq!(renamed.index, FileStatus::Renamed);
        assert_eq!(renamed.path, "new name.txt");
        assert_eq!(renamed.orig_path.as_deref(), Some("old name.txt"));

        let untracked = &status.entries[3];
        assert!(untracked.untracked);
        assert_eq!(untracked.index, FileStatus::Untracked);
        assert_eq!(untracked.path, "untracked dir/file with space.rs");

        let unmerged = &status.entries[4];
        assert!(unmerged.unmerged);
        assert_eq!(unmerged.index, FileStatus::Unmerged);
        assert_eq!(unmerged.path, "conflict file.rs");
    }

    #[test]
    fn detached_head_and_initial_repo() {
        let status = parse_status("# branch.oid (initial)\0# branch.head (detached)\0");
        assert!(status.branch.is_none());
        assert!(status.oid.is_none());
        assert!(status.entries.is_empty());
    }

    #[test]
    fn unknown_records_are_skipped() {
        let status = parse_status("# stash 0\0? kept.rs\0Z weird record\0");
        assert_eq!(status.entries.len(), 1);
        assert_eq!(status.entries[0].path, "kept.rs");
    }

    #[test]
    fn status_code_roundtrip() {
        for code in ['.', 'A', 'C', 'D', 'M', 'R', 'T', 'U', '?', '!'] {
            assert_eq!(FileStatus::from(code).code(), code);
        }
        assert_eq!(FileStatus::from('Z'), FileStatus::Unknown);
        assert_eq!(FileStatus::from('Z').code(), 'X');
    }
}
