//! Git 集成：status/diff/log/blame 解析、patch 生成与应用。
//!
//! 技术路线为 git CLI 优先（正确性 > 速度），详见开发计划 3.6。
//! 本 crate 是产品的**核心模块**：changes / diff / hunk 暂存 / commit。
//!
//! Phase 0：先落地版本要求判断与 hunk 暂存的必要常量。

/// 支持的最低 Git 版本（`git switch`、`--porcelain=v2` 等特性）。
pub const MIN_GIT_VERSION: (u32, u32, u32) = (2, 30, 0);

/// 默认未跟踪文件上限，超出后提示用户（对应开发计划 3.6.4）。
pub const DEFAULT_UNTRACKED_LIMIT: usize = 5_000;

/// status 刷新默认防抖（毫秒）。
pub const STATUS_DEBOUNCE_MS: u64 = 300;

/// 判断 Git 版本是否满足最低要求。
pub fn is_version_supported(version: (u32, u32, u32)) -> bool {
    version >= MIN_GIT_VERSION
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
}
