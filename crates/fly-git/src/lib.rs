#![forbid(unsafe_code)]

//! Git 集成（核心模块）：仓库发现、status/numstat 解析、版本检测。
//!
//! 技术路线为 git CLI 优先（正确性 > 速度），详见开发计划 3.6 与 ADR-0003：
//! - 解析器为纯函数，字节级单测不依赖 git 可执行文件；
//! - 子进程封装见 [`process`]，失败统一携带命令与 stderr，不静默吞错；
//! - 真实仓库的集成测试见 `tests/git_integration.rs`。

mod error;
mod numstat;
mod process;
mod repo;
mod status;
mod version;

pub use error::GitError;
pub use numstat::{parse_numstat, NumStat};
pub use repo::Repo;
pub use status::{parse_status, FileStatus, RepoStatus, StatusEntry};
pub use version::{detect_version, ensure_min_version, is_version_supported, MIN_GIT_VERSION};

/// 默认未跟踪文件上限，超出后提示用户（对应开发计划 3.6.4）。
pub const DEFAULT_UNTRACKED_LIMIT: usize = 5_000;

/// status 刷新默认防抖（毫秒）。
pub const STATUS_DEBOUNCE_MS: u64 = 300;

#[cfg(test)]
mod tests {
    #[test]
    fn limits_are_sane() {
        const {
            assert!(super::DEFAULT_UNTRACKED_LIMIT >= 100);
            assert!(super::STATUS_DEBOUNCE_MS >= 50);
        }
    }
}
