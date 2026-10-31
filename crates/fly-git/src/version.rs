//! git 版本检测与最低版本门禁。

use crate::error::GitError;
use crate::process;

/// 支持的最低 Git 版本（`git switch`、`--porcelain=v2` 等特性）。
pub const MIN_GIT_VERSION: (u32, u32, u32) = (2, 30, 0);

/// 判断 Git 版本是否满足最低要求。
pub fn is_version_supported(version: (u32, u32, u32)) -> bool {
    version >= MIN_GIT_VERSION
}

/// 检测本机 git 版本；未安装时返回 `None`。
pub fn detect_version() -> Option<(u32, u32, u32)> {
    process::detect_version()
}

/// 检测 git 并校验最低版本，成功返回 "major.minor.patch" 字符串。
pub fn ensure_min_version() -> Result<String, GitError> {
    let Some(found) = detect_version() else {
        return Err(GitError::GitNotFound);
    };
    if !is_version_supported(found) {
        return Err(GitError::VersionTooOld {
            found: format_version(found),
            min: format_version(MIN_GIT_VERSION),
        });
    }
    Ok(format_version(found))
}

fn format_version(version: (u32, u32, u32)) -> String {
    format!("{}.{}.{}", version.0, version.1, version.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_gate() {
        assert!(is_version_supported((2, 30, 0)));
        assert!(is_version_supported((2, 54, 0)));
        assert!(!is_version_supported((2, 29, 9)));
        assert!(!is_version_supported((1, 99, 0)));
    }

    #[test]
    fn formats_version() {
        assert_eq!(format_version((2, 54, 0)), "2.54.0");
    }
}
