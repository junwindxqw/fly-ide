//! unified diff patch 的解析与重组。
//!
//! 用途：hunk 级暂存/取消暂存/丢弃——从 `git diff` 原始 patch 中按 hunk
//! 切分，选中子集后重组为完整 patch，再交给 `git apply [--cached] [-R]`。
//!
//! 格式（`git diff`，单文件）：
//! ```text
//! diff --git a/f b/f
//! index ..
//! --- a/f
//! +++ b/f
//! @@ -l,s +l,s @@ section
//!  context
//! -removed
//! +added
//! \ No newline at end of file
//! ```
//! 注意 `\ No newline` 行属于当前 hunk，必须一并保留。

/// 单个 hunk：从 `@@` 行到 hunk 末尾的完整文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// 可选的 hunk 尾注（函数/上下文标记）。
    pub section: Option<String>,
    /// 从 `@@` 行开始的完整 hunk 文本（含 `\ No newline` 行，不含末尾换行）。
    pub text: String,
}

/// 单个文件的 patch：文件头（`diff --git` 到 `+++`）+ hunk 列表。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilePatch {
    pub header: String,
    pub hunks: Vec<PatchHunk>,
}

/// 按 `\n` 切行并只去掉行终止符本身——**保留内容行末尾的 `\r`**。
///
/// CRLF 文件的 patch 中，内容行的 `\r` 是内容的一部分；
/// `str::lines()` 会把 `\r\n` 整个吞掉，导致重组的 patch 无法应用。
fn iter_lines(patch: &str) -> impl Iterator<Item = &str> {
    patch
        .split_inclusive('\n')
        .map(|line| line.strip_suffix('\n').unwrap_or(line))
}

/// 解析（可能含多个文件的）unified diff。未知行尽力归位，可用性优先。
pub fn parse_patch(patch: &str) -> Vec<FilePatch> {
    fn is_hunk_body(line: &str) -> bool {
        // git 对空上下文行输出单个空格前缀；历史工具可能输出纯空行
        line.is_empty()
            || line.starts_with(' ')
            || line.starts_with('+')
            || line.starts_with('-')
            || line.starts_with('\\')
    }

    fn flush_hunk(result: &mut [FilePatch], hunk_lines: &mut Vec<&str>) {
        if hunk_lines.is_empty() {
            return;
        }
        if let Some(current) = result.last_mut() {
            if let Ok(hunk) = parse_hunk(hunk_lines[0], &hunk_lines[1..]) {
                current.hunks.push(hunk);
            }
        }
        hunk_lines.clear();
    }

    let mut result: Vec<FilePatch> = Vec::new();
    let mut hunk_lines: Vec<&str> = Vec::new();
    let mut in_hunk = false;

    for line in iter_lines(patch) {
        if line.starts_with("diff --git ") {
            flush_hunk(&mut result, &mut hunk_lines);
            in_hunk = false;
            result.push(FilePatch {
                header: line.to_string(),
                hunks: Vec::new(),
            });
        } else if line.starts_with("@@ ") {
            flush_hunk(&mut result, &mut hunk_lines);
            in_hunk = true;
            hunk_lines.push(line);
        } else if in_hunk {
            if is_hunk_body(line) {
                hunk_lines.push(line);
            } else {
                // 防御：hunk 内出现意外行，结束 hunk 并归入 header
                flush_hunk(&mut result, &mut hunk_lines);
                in_hunk = false;
                if let Some(current) = result.last_mut() {
                    current.header.push('\n');
                    current.header.push_str(line);
                }
            }
        } else if let Some(current) = result.last_mut() {
            current.header.push('\n');
            current.header.push_str(line);
        }
        // 尚无文件头：patch 开头的杂散行，忽略
    }
    flush_hunk(&mut result, &mut hunk_lines);
    result
}

/// 重组 patch：文件头 + 选中的 hunk（按给定索引）。
///
/// hunk 头携带绝对行号，`git apply` 按其定位，因此任意子集可直接应用。
pub fn build_patch(file: &FilePatch, selected: &[usize]) -> String {
    let mut out = String::new();
    out.push_str(file.header.trim_end());
    for &index in selected {
        let Some(hunk) = file.hunks.get(index) else {
            continue;
        };
        out.push('\n');
        out.push_str(&hunk.text);
    }
    out.push('\n');
    out
}

fn parse_hunk(header_line: &str, body: &[&str]) -> Result<PatchHunk, ()> {
    let rest = header_line.strip_prefix("@@ ").ok_or(())?;
    let end = rest.find(" @@").ok_or(())?;
    let spec = &rest[..end];
    let section = rest[end + 3..].trim();
    let mut specs = spec.split_whitespace();

    let old = parse_range(specs.next().ok_or(())?)?;
    let new = parse_range(specs.next().ok_or(())?)?;

    let mut text = header_line.to_string();
    for line in body {
        text.push('\n');
        text.push_str(line);
    }
    Ok(PatchHunk {
        old_start: old.0,
        old_lines: old.1,
        new_start: new.0,
        new_lines: new.1,
        section: if section.is_empty() {
            None
        } else {
            Some(section.to_string())
        },
        text,
    })
}

/// 解析 `-l[,s]` / `+l[,s]`；省略 `,s` 时行数为 1。
fn parse_range(spec: &str) -> Result<(u32, u32), ()> {
    let (sign, rest) = spec.split_at(1);
    if sign != "-" && sign != "+" {
        return Err(());
    }
    let mut parts = rest.splitn(2, ',');
    let start: u32 = parts.next().ok_or(())?.parse().map_err(|_| ())?;
    let lines: u32 = match parts.next() {
        Some(count) => count.parse().map_err(|_| ())?,
        None => 1,
    };
    Ok((start, lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_HUNKS: &str = concat!(
        "diff --git a/f.txt b/f.txt\n",
        "index 1111111..2222222 100644\n",
        "--- a/f.txt\n",
        "+++ b/f.txt\n",
        "@@ -1,4 +1,4 @@ first\n",
        " line1\n",
        "-line2\n",
        "+LINE2\n",
        " line3\n",
        " line4\n",
        "@@ -10,4 +10,4 @@ second\n",
        " line10\n",
        "-line11\n",
        "+LINE11\n",
        "\\ No newline at end of file\n",
    );

    #[test]
    fn parses_header_and_both_hunks() {
        let files = parse_patch(TWO_HUNKS);
        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert!(file.header.contains("diff --git a/f.txt b/f.txt"));
        assert!(file.header.contains("+++ b/f.txt"));
        assert_eq!(file.hunks.len(), 2);

        let first = &file.hunks[0];
        assert_eq!((first.old_start, first.old_lines), (1, 4));
        assert_eq!((first.new_start, first.new_lines), (1, 4));
        assert_eq!(first.section.as_deref(), Some("first"));

        let second = &file.hunks[1];
        assert_eq!((second.old_start, second.old_lines), (10, 4));
        assert_eq!(second.section.as_deref(), Some("second"));
        // `\ No newline` 归属当前 hunk
        assert!(second.text.contains("\\ No newline at end of file"));
    }

    #[test]
    fn rebuild_with_subset_keeps_hunk_text_verbatim() {
        let files = parse_patch(TWO_HUNKS);
        let rebuilt = build_patch(&files[0], &[0]);
        assert!(rebuilt.contains("diff --git a/f.txt b/f.txt"));
        assert!(rebuilt.contains("@@ -1,4 +1,4 @@ first"));
        assert!(rebuilt.contains("+LINE2"));
        assert!(!rebuilt.contains("LINE11"));
        // 重建的 patch 再次解析应得到恰好 1 个 hunk，且文本一致
        let reparsed = parse_patch(&rebuilt);
        assert_eq!(reparsed.len(), 1);
        assert_eq!(reparsed[0].hunks.len(), 1);
        assert_eq!(reparsed[0].hunks[0].text, files[0].hunks[0].text);
    }

    #[test]
    fn single_line_ranges_without_count() {
        let files = parse_patch("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -3 +3 @@\n-a\n+b\n");
        assert_eq!(files[0].hunks.len(), 1);
        let hunk = &files[0].hunks[0];
        assert_eq!((hunk.old_start, hunk.old_lines), (3, 1));
        assert_eq!((hunk.new_start, hunk.new_lines), (3, 1));
        assert_eq!(hunk.section, None);
    }

    #[test]
    fn multi_file_patch_is_split() {
        let two_files =
            format!("{TWO_HUNKS}diff --git a/g b/g\n--- a/g\n+++ b/g\n@@ -1 +1 @@\n-a\n+b\n");
        let files = parse_patch(&two_files);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].hunks.len(), 2);
        assert_eq!(files[1].hunks.len(), 1);
        assert!(files[1].header.contains("a/g"));
    }

    #[test]
    fn invalid_index_selects_nothing() {
        let files = parse_patch(TWO_HUNKS);
        let rebuilt = build_patch(&files[0], &[99]);
        assert!(rebuilt.starts_with(files[0].header.trim_end()));
        assert!(!rebuilt.contains("@@ "), "无效索引不应带出任何 hunk");
    }

    #[test]
    fn empty_range_edge_cases() {
        assert_eq!(parse_range("-0,0"), Ok((0, 0)));
        assert_eq!(parse_range("+7"), Ok((7, 1)));
        assert!(parse_range("x1,2").is_err());
        assert!(parse_range("-a,b").is_err());
    }

    #[test]
    fn crlf_content_lines_survive_rebuild() {
        // CRLF 文件的 patch：内容行以 \r\n 结尾，\r 属于内容
        let patch =
            "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n alpha\r\n-beta\r\n+BETA\r\n";
        let files = parse_patch(patch);
        assert_eq!(files[0].hunks.len(), 1);
        let rebuilt = build_patch(&files[0], &[0]);
        assert!(rebuilt.contains(" alpha\r\n"), "上下文行的 \\r 必须保留");
        assert!(rebuilt.contains("+BETA\r\n"), "新增行的 \\r 必须保留");
    }
}
