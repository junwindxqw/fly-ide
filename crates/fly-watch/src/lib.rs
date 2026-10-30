//! 文件监听封装：防抖、事件合并、自写事件过滤。
//!
//! Phase 0：先固定默认防抖窗口（对应开发计划 3.8 外部修改检测）。

/// 文件事件默认防抖窗口（毫秒）。
pub const DEFAULT_DEBOUNCE_MS: u64 = 150;

/// 判断是否应忽略该路径的监听事件（`.git/objects` 等噪声源）。
pub fn is_noise_path(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "objects" || s == "node_modules" || s == "target"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn noise_paths_are_filtered() {
        assert!(is_noise_path(Path::new("repo/.git/objects/ab/cdef")));
        assert!(is_noise_path(Path::new("repo/node_modules/x")));
        assert!(!is_noise_path(Path::new("repo/src/main.rs")));
        assert!(!is_noise_path(Path::new("repo/.git/index")));
    }
}
