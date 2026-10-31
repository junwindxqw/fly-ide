#![forbid(unsafe_code)]

//! 项目搜索：ignore 遍历、grep 匹配、nucleo 模糊快速打开。
//!
//! Phase 0：先固定结果上限与分批大小（对应开发计划 3.7）。

/// 单次搜索默认结果上限，超出后提示用户缩小范围。
pub const DEFAULT_RESULT_LIMIT: usize = 10_000;

/// 推送给前端的默认批次大小。
pub const DEFAULT_BATCH_SIZE: usize = 200;

/// 快速打开默认返回条数。
pub const QUICK_OPEN_LIMIT: usize = 50;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_are_sane() {
        const {
            assert!(DEFAULT_RESULT_LIMIT > DEFAULT_BATCH_SIZE);
            assert!(QUICK_OPEN_LIMIT <= DEFAULT_BATCH_SIZE);
        }
    }
}
