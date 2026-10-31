#![forbid(unsafe_code)]

//! 文件读写、编码/行尾处理、原子保存、大文件分级。

pub mod text;

pub use text::{
    detect_eol, is_binary, normalize_eol, read_text_file, write_text_file, Eol, TextEncoding,
    TextError, TextFile,
};

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

impl SizeTier {
    /// 按字节数返回文件分级。
    pub fn from_size(bytes: u64) -> Self {
        size_tier(bytes)
    }

    /// DTO/前端使用的标识串。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Reduced => "reduced",
            Self::ReadOnlyPlain => "readOnlyPlain",
            Self::TooLarge => "tooLarge",
        }
    }
}

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

    #[test]
    fn from_size_matches_free_function() {
        assert_eq!(SizeTier::from_size(1), size_tier(1));
        assert_eq!(SizeTier::from_size(READ_ONLY_LIMIT), SizeTier::TooLarge);
    }

    #[test]
    fn tier_strings_for_dto() {
        assert_eq!(SizeTier::Full.as_str(), "full");
        assert_eq!(SizeTier::ReadOnlyPlain.as_str(), "readOnlyPlain");
    }
}
