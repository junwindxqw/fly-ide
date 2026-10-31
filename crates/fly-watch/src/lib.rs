#![forbid(unsafe_code)]

//! 文件监听封装：防抖、事件合并、自写事件过滤。
//!
//! Phase 0：先固定默认防抖窗口（对应开发计划 3.8 外部修改检测）。

/// 文件事件默认防抖窗口（毫秒）。
pub const DEFAULT_DEBOUNCE_MS: u64 = 150;

/// 判断是否应忽略该路径的监听事件（噪声源：`.git/objects`、`node_modules`、`target`）。
///
/// 注意 `objects` 只在 `.git/objects` 下才是噪声——业务代码里同样可能有
/// 叫 `objects` 的目录，不能一刀切。
pub fn is_noise_path(path: &std::path::Path) -> bool {
    use std::ffi::OsStr;

    let mut prev: Option<&OsStr> = None;
    for component in path.components() {
        let name = component.as_os_str();
        if name == OsStr::new("node_modules") || name == OsStr::new("target") {
            return true;
        }
        if name == OsStr::new("objects") && prev == Some(OsStr::new(".git")) {
            return true;
        }
        prev = Some(name);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn git_objects_are_noise_but_plain_objects_are_not() {
        assert!(is_noise_path(Path::new("repo/.git/objects/ab/cdef")));
        assert!(!is_noise_path(Path::new("repo/src/objects/holder.rs")));
        assert!(!is_noise_path(Path::new("repo/.git/index")));
        assert!(!is_noise_path(Path::new("objects/README")));
    }

    #[test]
    fn dependency_dirs_are_noise() {
        assert!(is_noise_path(Path::new("repo/node_modules/pkg/index.js")));
        assert!(is_noise_path(Path::new("repo/target/debug/fly-ide.exe")));
        assert!(!is_noise_path(Path::new("repo/src/main.rs")));
    }
}
