#![forbid(unsafe_code)]

//! 工作区打开、文件树懒加载、最近项目、会话、文件索引。

pub mod settings;

use std::fs;
use std::path::Path;

pub use settings::{Settings, SettingsStore};

/// 遍历/搜索时默认排除的目录名。
pub const DEFAULT_EXCLUDES: &[&str] =
    &[".git", "node_modules", "vendor", "target", "dist", ".venv"];

/// 判断目录名是否应被默认排除。
pub fn is_default_excluded(name: &str) -> bool {
    DEFAULT_EXCLUDES.contains(&name)
}

/// 文件树单个条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    /// 命中默认排除规则（前端以弱化样式展示，不隐藏）。
    pub ignored: bool,
}

/// 列出目录的一层内容：目录在前，各自按名称不区分大小写排序。
///
/// 懒加载契约：调用方逐目录请求，本函数不做递归（避免大仓库与
/// 符号链接环的遍历成本）。符号链接/junction 按其目标类型展示。
pub fn list_dir(path: &Path) -> std::io::Result<Vec<DirEntry>> {
    let mut dirs: Vec<DirEntry> = Vec::new();
    let mut files: Vec<DirEntry> = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        // metadata 跟随符号链接/junction，使目录快捷方式按目录展示
        let is_dir = entry.metadata()?.is_dir();
        let name = entry.file_name().to_string_lossy().into_owned();
        let ignored = is_default_excluded(&name);
        let entry = DirEntry {
            name,
            is_dir,
            ignored,
        };
        if is_dir {
            dirs.push(entry);
        } else {
            files.push(entry);
        }
    }
    let by_name = |a: &DirEntry, b: &DirEntry| a.name.to_lowercase().cmp(&b.name.to_lowercase());
    dirs.sort_unstable_by(by_name);
    files.sort_unstable_by(by_name);
    dirs.extend(files);
    Ok(dirs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fly-ws-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("创建临时目录");
        dir
    }

    #[test]
    fn excludes_common_heavy_dirs() {
        assert!(is_default_excluded("node_modules"));
        assert!(is_default_excluded(".git"));
        assert!(!is_default_excluded("src"));
    }

    #[test]
    fn list_dir_sorts_dirs_first_case_insensitive() {
        let root = temp_dir();
        fs::create_dir_all(root.join("zebra")).unwrap();
        fs::create_dir_all(root.join("node_modules")).unwrap();
        fs::write(root.join("Apple.txt"), "x").unwrap();
        fs::write(root.join("banana.txt"), "x").unwrap();

        let entries = list_dir(&root).expect("列出目录");
        let names: Vec<(&str, bool, bool)> = entries
            .iter()
            .map(|e| (e.name.as_str(), e.is_dir, e.ignored))
            .collect();
        assert_eq!(
            names,
            vec![
                ("node_modules", true, true),
                ("zebra", true, false),
                ("Apple.txt", false, false),
                ("banana.txt", false, false),
            ]
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn list_dir_missing_dir_is_io_error() {
        let root = temp_dir().join("不存在");
        assert!(list_dir(&root).is_err());
    }
}
