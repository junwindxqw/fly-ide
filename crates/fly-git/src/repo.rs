//! 仓库发现与 git 命令封装。

use std::path::{Path, PathBuf};

use crate::error::GitError;
use crate::numstat::{parse_numstat, NumStat};
use crate::process::run_git;
use crate::status::{parse_status, RepoStatus};

/// 一个已发现的 git 仓库（工作区根目录）。
#[derive(Debug, Clone)]
pub struct Repo {
    root: PathBuf,
}

impl Repo {
    /// 仓库工作区根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 从指定目录向上查找包含 `.git`（仓库目录或 worktree 指针文件）的根目录。
    pub fn discover(start: &Path) -> Option<Repo> {
        let mut dir = Some(start);
        while let Some(current) = dir {
            if current.join(".git").exists() {
                return Some(Repo {
                    root: current.to_path_buf(),
                });
            }
            dir = current.parent();
        }
        None
    }

    /// `git status --porcelain=v2 -z --branch --untracked-files=all`。
    pub fn status(&self) -> Result<RepoStatus, GitError> {
        let out = run_git(
            &self.root,
            &[
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--untracked-files=all",
            ],
        )?;
        Ok(parse_status(&String::from_utf8_lossy(&out)))
    }

    /// 增删行统计：`staged=true` 为暂存区 vs HEAD，否则为工作区 vs 暂存区。
    pub fn diff_numstat(&self, staged: bool) -> Result<Vec<NumStat>, GitError> {
        let mut args = vec!["diff", "--numstat", "-z", "-M"];
        if staged {
            args.push("--cached");
        }
        let out = run_git(&self.root, &args)?;
        Ok(parse_numstat(&String::from_utf8_lossy(&out)))
    }
}
