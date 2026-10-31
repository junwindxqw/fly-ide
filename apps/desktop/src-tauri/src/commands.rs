//! Tauri 命令层：把 `crates/` 的领域模型映射为前端 DTO。
//!
//! 原则（对应开发计划 3.2/3.10）：阻塞操作一律 `spawn_blocking`，
//! 错误统一转为可展示字符串，不在命令层做业务逻辑。

use std::path::Path;

use serde::Serialize;
use tokio::task::spawn_blocking;

use fly_git::{NumStat, Repo, RepoStatus, StatusEntry};

async fn run_blocking<T, F>(task: F) -> Result<T, String>
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
