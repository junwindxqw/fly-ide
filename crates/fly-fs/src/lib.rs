//! 文件读写、编码/行尾处理、原子保存、大文件分级。
//!
//! Phase 0：先落地大文件分级策略（对应开发计划 3.8），Phase 1 实现完整读写。

/// 文件大小分级：决定编辑器启用哪些特性，避免大文件卡死。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeTier {
    /// < 2MB：全功能（LSP、语法高亮、折叠、诊断）
    Full,
    /// 2–10MB：语法高亮，关闭语义高亮 / inlay hints / code lens
    Reduced,
    /// 10–50MB：纯文本只读，关闭 LSP
    ReadOnlyPlain,
    /// > 50MB：默认不加载全文，提示用户
    TooLarge,
}

pub const FULL_FEATURE_LIMIT: u64 = 2 * 1024 * 1024;
pub const REDUCED_LIMIT: u64 = 10 * 1024 * 1024;
pub const READ_ONLY_LIMIT: u64 = 50 * 1024 * 1024;

/// 按字节数返回文件分级。
pub fn size_tier(bytes: u64) -> SizeTier {
    match bytes {
        b if b < FULL_FEATURE_LIMIT => SizeTier::Full,
        b if b < REDUCED_LIMIT => SizeTier::Reduced,
        b if b < READ_ONLY_LIMIT => SizeTier::ReadOnlyPlain,
        _ => SizeTier::TooLarge,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_follow_limits() {
        assert_eq!(size_tier(0), SizeTier::Full);
        assert_eq!(size_tier(FULL_FEATURE_LIMIT - 1), SizeTier::Full);
        assert_eq!(size_tier(FULL_FEATURE_LIMIT), SizeTier::Reduced);
        assert_eq!(size_tier(REDUCED_LIMIT - 1), SizeTier::Reduced);
        assert_eq!(size_tier(REDUCED_LIMIT), SizeTier::ReadOnlyPlain);
        assert_eq!(size_tier(READ_ONLY_LIMIT - 1), SizeTier::ReadOnlyPlain);
        assert_eq!(size_tier(READ_ONLY_LIMIT), SizeTier::TooLarge);
        assert_eq!(size_tier(u64::MAX), SizeTier::TooLarge);
    }
}
