//! Tauri 命令层：把 `crates/` 的领域模型映射为前端 DTO。
//!
//! 原则（对应开发计划 3.2/3.10）：阻塞操作一律 `spawn_blocking`，
//! 错误统一转为可展示字符串，不在命令层做业务逻辑。

use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::task::spawn_blocking;

use fly_git::{FilePatch, NumStat, Repo, RepoStatus, StatusEntry};

pub(crate) async fn run_blocking<T, F>(task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    match spawn_blocking(task).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(message)) => Err(message),
        // JoinError：任务 panic 或运行时关闭
        Err(err) => Err(err.to_string()),
    }
}

fn repo_of(root: &str) -> Result<Repo, String> {
    Repo::discover(Path::new(root)).ok_or_else(|| format!("目录不是 git 仓库：{root}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEntryDto {
    pub x: char,
    pub y: char,
    pub path: String,
    pub orig_path: Option<String>,
    pub untracked: bool,
    pub unmerged: bool,
}

impl From<&StatusEntry> for StatusEntryDto {
    fn from(entry: &StatusEntry) -> Self {
        Self {
            x: entry.index.code(),
            y: entry.worktree.code(),
            path: entry.path.clone(),
            orig_path: entry.orig_path.clone(),
            untracked: entry.untracked,
            unmerged: entry.unmerged,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatusDto {
    pub branch: Option<String>,
    pub oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub entries: Vec<StatusEntryDto>,
}

impl From<&RepoStatus> for RepoStatusDto {
    fn from(status: &RepoStatus) -> Self {
        Self {
            branch: status.branch.clone(),
            oid: status.oid.clone(),
            upstream: status.upstream.clone(),
            ahead: status.ahead,
            behind: status.behind,
            entries: status.entries.iter().map(StatusEntryDto::from).collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumStatDto {
    pub added: Option<u32>,
    pub deleted: Option<u32>,
    pub path: String,
    pub orig_path: Option<String>,
}

impl From<&NumStat> for NumStatDto {
    fn from(stat: &NumStat) -> Self {
        Self {
            added: stat.added,
            deleted: stat.deleted,
            path: stat.path.clone(),
            orig_path: stat.orig_path.clone(),
        }
    }
}

/// 检测 git 并校验最低版本，返回 "major.minor.patch"。
#[tauri::command]
pub async fn git_version() -> Result<String, String> {
    run_blocking(|| fly_git::ensure_min_version().map_err(|err| err.to_string())).await
}

/// 读取仓库状态（分支 + 变更条目）。
#[tauri::command]
pub async fn git_status(root: String) -> Result<RepoStatusDto, String> {
    run_blocking(move || {
        let repo = repo_of(&root)?;
        let status = repo.status().map_err(|err| err.to_string())?;
        Ok(RepoStatusDto::from(&status))
    })
    .await
}

/// 增删行统计。`staged=true` 为暂存区 vs HEAD。
#[tauri::command]
pub async fn git_numstat(root: String, staged: bool) -> Result<Vec<NumStatDto>, String> {
    run_blocking(move || {
        let repo = repo_of(&root)?;
        let stats = repo.diff_numstat(staged).map_err(|err| err.to_string())?;
        Ok(stats.iter().map(NumStatDto::from).collect())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HunkDto {
    pub index: usize,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub section: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiffDto {
    pub path: String,
    pub staged: bool,
    pub binary: bool,
    pub old_text: String,
    pub new_text: String,
    pub hunks: Vec<HunkDto>,
}

/// hunk 级操作方向。
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HunkAction {
    /// 工作区 → 暂存区
    Stage,
    /// 暂存区 → 工作区
    Unstage,
    /// 丢弃工作区改动（回滚到 HEAD）
    Discard,
}

/// 读取单文件的 diff 数据：old/new 文本（渲染用）+ hunk 列表（操作用）。
///
/// - `staged=false`（工作区 diff）：old = 暂存区内容，new = 工作区内容；
/// - `staged=true`（暂存区 diff）：old = HEAD 内容，new = 暂存区内容；
/// - 未跟踪文件 old 为空；已删除文件 new 为空。
///
/// 实现：3 次 git spawn（index 探测/读取 + diff）加 1 次工作区读盘。
/// 每次优先保正确性；真实环境 spawn ~10-50ms，若成为热点再合并。
#[tauri::command]
pub async fn git_diff_file(
    root: String,
    path: String,
    staged: bool,
) -> Result<FileDiffDto, String> {
    run_blocking(move || {
        let repo = repo_of(&root)?;
        let (old_text, new_text) = if staged {
            let old = repo.show("HEAD", &path).map_err(|err| err.to_string())?;
            let new = repo.index_text(&path).map_err(|err| err.to_string())?;
            (old, new)
        } else {
            let old = repo.index_text(&path).map_err(|err| err.to_string())?;
            let new = repo.worktree_text(&path).map_err(|err| err.to_string())?;
            (old, new)
        };
        let old_text = old_text.unwrap_or_default();
        let new_text = new_text.unwrap_or_default();
        let binary = old_text.contains('\0') || new_text.contains('\0');

        let hunks = repo
            .file_patch(&path, staged)
            .map_err(|err| err.to_string())?
            .map(|FilePatch { hunks, .. }| {
                hunks
                    .into_iter()
                    .enumerate()
                    .map(|(index, hunk)| HunkDto {
                        index,
                        old_start: hunk.old_start,
                        old_lines: hunk.old_lines,
                        new_start: hunk.new_start,
                        new_lines: hunk.new_lines,
                        section: hunk.section,
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(FileDiffDto {
            path,
            staged,
            binary,
            old_text,
            new_text,
            hunks,
        })
    })
    .await
}

/// 文件级暂存。
#[tauri::command]
pub async fn git_stage(root: String, paths: Vec<String>) -> Result<(), String> {
    run_blocking(move || {
        let repo = repo_of(&root)?;
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        repo.stage_paths(&refs).map_err(|err| err.to_string())
    })
    .await
}

/// 文件级取消暂存。
#[tauri::command]
pub async fn git_unstage(root: String, paths: Vec<String>) -> Result<(), String> {
    run_blocking(move || {
        let repo = repo_of(&root)?;
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        repo.unstage_paths(&refs).map_err(|err| err.to_string())
    })
    .await
}

/// 文件级丢弃（Rollback 到 HEAD；新文件取消暂存并删除）。不可恢复。
#[tauri::command]
pub async fn git_discard(root: String, paths: Vec<String>) -> Result<(), String> {
    run_blocking(move || {
        let repo = repo_of(&root)?;
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        repo.discard_paths(&refs).map_err(|err| err.to_string())
    })
    .await
}

/// hunk 级操作：`hunks` 为 [`git_diff_file`] 返回的 hunk 索引子集。
#[tauri::command]
pub async fn git_apply_hunks(
    root: String,
    path: String,
    hunks: Vec<usize>,
    action: HunkAction,
) -> Result<(), String> {
    run_blocking(move || {
        if hunks.is_empty() {
            return Err("未选择任何 hunk".to_string());
        }
        let repo = repo_of(&root)?;
        let result = match action {
            HunkAction::Stage => repo.stage_hunks(&path, &hunks),
            HunkAction::Unstage => repo.unstage_hunks(&path, &hunks),
            HunkAction::Discard => repo.discard_hunks(&path, &hunks),
        };
        result.map_err(|err| err.to_string())
    })
    .await
}

/// 提交选项。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitOptionsDto {
    pub amend: bool,
    pub signoff: bool,
    pub no_verify: bool,
}

/// 提交暂存区，返回命令输出（提交摘要 + hooks 输出）。
/// 失败（hooks 拒绝、无暂存变更等）时 Err 携带可展示文本。
#[tauri::command]
pub async fn git_commit(
    root: String,
    message: String,
    options: CommitOptionsDto,
) -> Result<String, String> {
    run_blocking(move || {
        let repo = repo_of(&root)?;
        repo.commit(
            &message,
            &fly_git::CommitOptions {
                amend: options.amend,
                signoff: options.signoff,
                no_verify: options.no_verify,
            },
        )
        .map_err(|err| err.to_string())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfoDto {
    pub hash: String,
    pub short: String,
    pub author: String,
    pub date: String,
    pub subject: String,
    pub refs: Vec<String>,
}

/// 提交历史（默认 50 条）。
#[tauri::command]
pub async fn git_log(root: String, limit: Option<usize>) -> Result<Vec<CommitInfoDto>, String> {
    run_blocking(move || {
        let repo = repo_of(&root)?;
        let log = repo
            .log(limit.unwrap_or(50))
            .map_err(|err| err.to_string())?;
        Ok(log
            .into_iter()
            .map(|c| CommitInfoDto {
                hash: c.hash,
                short: c.short,
                author: c.author,
                date: c.date,
                subject: c.subject,
                refs: c.refs,
            })
            .collect())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameLineDto {
    pub hash: String,
    pub author: String,
    pub time: i64,
    pub summary: String,
    pub line: u32,
}

/// 单文件行内 blame。
#[tauri::command]
pub async fn git_blame(root: String, path: String) -> Result<Vec<BlameLineDto>, String> {
    run_blocking(move || {
        let repo = repo_of(&root)?;
        let blame = repo.blame(&path).map_err(|err| err.to_string())?;
        Ok(blame
            .into_iter()
            .map(|b| BlameLineDto {
                hash: b.hash,
                author: b.author,
                time: b.time,
                summary: b.summary,
                line: b.line,
            })
            .collect())
    })
    .await
}

/// 提交的变更摘要（历史详情展示）。
#[tauri::command]
pub async fn git_show_commit(root: String, hash: String) -> Result<String, String> {
    run_blocking(move || {
        let repo = repo_of(&root)?;
        repo.show_commit_stat(&hash).map_err(|err| err.to_string())
    })
    .await
}

/// 网络操作（fetch/pull/push），凭据由 Git Credential Manager 处理。
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkAction {
    Fetch,
    Pull,
    Push,
}

#[tauri::command]
pub async fn git_network(root: String, action: NetworkAction) -> Result<String, String> {
    use fly_git::Repo;
    run_blocking(move || {
        let repo =
            Repo::discover(Path::new(&root)).ok_or_else(|| format!("目录不是 git 仓库：{root}"))?;
        let result = match action {
            NetworkAction::Fetch => repo.fetch(),
            NetworkAction::Pull => repo.pull(),
            NetworkAction::Push => repo.push(),
        };
        result.map_err(|err| err.to_string())
    })
    .await
}

/// 分支跟踪信息（当前分支、上游、领先/落后）。
#[tauri::command]
pub async fn git_branch_info(root: String) -> Result<BranchInfoDto, String> {
    use fly_git::Repo;
    run_blocking(move || {
        let repo =
            Repo::discover(Path::new(&root)).ok_or_else(|| format!("目录不是 git 仓库：{root}"))?;
        let info = repo.branch_info().map_err(|err| err.to_string())?;
        Ok(BranchInfoDto {
            branch: info.branch,
            upstream: info.upstream,
            ahead: info.ahead,
            behind: info.behind,
            has_remote: info.has_remote,
        })
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfoDto {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub has_remote: bool,
}
