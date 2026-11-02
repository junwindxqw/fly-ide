//! 分支信息（当前分支、上游、领先/落后）。

use serde::Serialize;

/// 当前分支的跟踪状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BranchInfo {
    /// 当前分支名；detached HEAD 为 None。
    pub branch: Option<String>,
    /// 上游（如 origin/main）；无上游为 None。
    pub upstream: Option<String>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    /// 是否有远程仓库。
    pub has_remote: bool,
}

impl BranchInfo {
    /// 构造分支信息（字段来自 `git status --porcelain=v2` 的 branch 头）。
    pub fn new(
        branch: Option<String>,
        upstream: Option<String>,
        ahead: Option<u32>,
        behind: Option<u32>,
        has_remote: bool,
    ) -> Self {
        Self {
            branch,
            upstream,
            ahead,
            behind,
            has_remote,
        }
    }

    /// 状态栏展示用，如 `↑2 ↓1`；无跟踪信息返回 None。
    pub fn tracking_display(&self) -> Option<String> {
        if self.ahead.is_none() && self.behind.is_none() {
            return None;
        }
        Some(format!(
            "↑{} ↓{}",
            self.ahead.unwrap_or(0),
            self.behind.unwrap_or(0)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracking_display_omits_when_no_upstream() {
        let info = BranchInfo::new(Some("main".into()), None, None, None, true);
        assert_eq!(info.tracking_display(), None);
    }

    #[test]
    fn tracking_display_formats_counts() {
        let info = BranchInfo::new(
            Some("main".into()),
            Some("origin/main".into()),
            Some(2),
            Some(1),
            true,
        );
        assert_eq!(info.tracking_display(), Some("↑2 ↓1".into()));
    }

    #[test]
    fn ahead_only_display() {
        let info = BranchInfo::new(
            Some("main".into()),
            Some("origin/main".into()),
            Some(3),
            None,
            true,
        );
        assert_eq!(info.tracking_display(), Some("↑3 ↓0".into()));
    }
}
