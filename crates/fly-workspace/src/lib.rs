#![forbid(unsafe_code)]

//! 工作区打开、文件树懒加载、最近项目、会话、文件索引。
//!
//! Phase 0：先固定默认排除规则（对应开发计划 3.7）。

/// 遍历/搜索时默认排除的目录名。
pub const DEFAULT_EXCLUDES: &[&str] =
    &[".git", "node_modules", "vendor", "target", "dist", ".venv"];

/// 判断目录名是否应被默认排除。
pub fn is_default_excluded(name: &str) -> bool {
    DEFAULT_EXCLUDES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_common_heavy_dirs() {
        assert!(is_default_excluded("node_modules"));
        assert!(is_default_excluded(".git"));
        assert!(!is_default_excluded("src"));
    }
}
