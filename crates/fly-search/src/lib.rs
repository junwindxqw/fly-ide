#![forbid(unsafe_code)]

//! 项目搜索：文件索引与模糊快速打开、内容搜索（ripgrep 系 crate）。
//!
//! 对应开发计划 3.7：
//! - 文件索引尊重 .gitignore 与默认排除规则，路径规范化为 `/` 分隔的
//!   相对路径，便于前端拼接展示与打开；
//! - 模糊匹配用 nucleo（与 Helix 同源）；
//! - 内容搜索基于 grep-searcher/grep-regex，尊重忽略规则，支持取消与
//!   结果上限，命中上限立即停止遍历。

pub mod content;
pub mod files;

/// 单次内容搜索默认结果上限，超出后提示用户缩小范围。
pub const DEFAULT_RESULT_LIMIT: usize = 2_000;

/// 推送给前端的默认批次大小。
pub const DEFAULT_BATCH_SIZE: usize = 200;

/// 快速打开默认返回条数。
pub const QUICK_OPEN_LIMIT: usize = 50;

pub use content::{search_content, ContentMatch, SearchOptions, SearchSummary};
pub use files::{build_file_index, fuzzy_search, FileMatch};

#[cfg(test)]
mod tests {
    #[test]
    fn limits_are_sane() {
        const {
            assert!(super::DEFAULT_RESULT_LIMIT >= 100);
            assert!(super::QUICK_OPEN_LIMIT <= 1000);
        }
    }
}
