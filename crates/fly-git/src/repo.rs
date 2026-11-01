//! 仓库发现与 git 命令封装。

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::GitError;
use crate::numstat::{parse_numstat, NumStat};
use crate::patch::{build_patch, parse_patch, FilePatch};
use crate::process::{run_git, run_git_with_stdin};
use crate::status::{parse_status, RepoStatus};

/// 提交选项（对应开发计划 3.6.2 Commit 行）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CommitOptions {
    pub amend: bool,
    pub signoff: bool,
    pub no_verify: bool,
}

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

    /// 读取 `<rev>:<path>` 的内容。rev 形如 `HEAD`、`:path`（index 用空 rev，见
    /// [`Repo::index_text`]）。对象不存在时返回 `None`（用 `cat-file -e` 先探测，
    /// 避免 `git show` 的 stderr 文案随 locale 变化）。
    pub fn show(&self, rev: &str, path: &str) -> Result<Option<String>, GitError> {
        let spec = format!("{rev}:{path}");
        let exists = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["cat-file", "-e", &spec])
            .output()
            .map_err(|err| match err.kind() {
                std::io::ErrorKind::NotFound => GitError::GitNotFound,
                _ => GitError::Io(err),
            })?
            .status
            .success();
        if !exists {
            return Ok(None);
        }
        let out = run_git(&self.root, &["show", &spec])?;
        Ok(Some(String::from_utf8_lossy(&out).into_owned()))
    }

    /// 读取暂存区（index）中的文件内容；文件不在 index 时返回 `None`。
    pub fn index_text(&self, path: &str) -> Result<Option<String>, GitError> {
        self.show("", path)
    }

    /// 读取工作区文件文本（UTF-8 lossy；编码检测在 Phase 1 的文件服务实现）。
    /// 文件不存在时返回 `None`。
    pub fn worktree_text(&self, path: &str) -> Result<Option<String>, GitError> {
        match std::fs::read(self.root.join(path)) {
            Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(GitError::Io(err)),
        }
    }

    /// 单文件 unified patch（含 hunk），无差异时为空字符串。
    pub fn diff_patch(&self, path: &str, staged: bool) -> Result<String, GitError> {
        let mut args = vec!["diff", "-M", "--no-ext-diff", "--no-color"];
        if staged {
            args.push("--cached");
        }
        args.push("--");
        args.push(path);
        let out = run_git(&self.root, &args)?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// 当前文件的 patch 解析结果（无差异时为 `None`）。
    pub fn file_patch(&self, path: &str, staged: bool) -> Result<Option<FilePatch>, GitError> {
        let patch = self.diff_patch(path, staged)?;
        if patch.trim().is_empty() {
            return Ok(None);
        }
        Ok(parse_patch(&patch).into_iter().next())
    }

    /// 通过 stdin 给 `git apply` 喂 patch。
    ///
    /// - `cached=true`：只改暂存区；
    /// - `reverse=true`：反向应用（取消暂存/丢弃 hunk）。
    pub fn apply_patch(&self, patch: &str, cached: bool, reverse: bool) -> Result<(), GitError> {
        let mut args = vec!["apply", "--whitespace=nowarn"];
        if cached {
            args.push("--cached");
        }
        if reverse {
            args.push("-R");
        }
        args.push("-");
        run_git_with_stdin(&self.root, &args, Some(patch.as_bytes()))?;
        Ok(())
    }

    /// 提交暂存区（`git commit -F -`，消息经 stdin 传入）。
    ///
    /// 返回合并的命令输出（提交摘要 + hooks 输出），供前端展示。
    /// GitError::CommandFailed 的 stderr 同样携带 hooks 输出——
    /// pre-commit 拒绝时调用方应把错误文本展示给用户而非静默吞掉。
    pub fn commit(&self, message: &str, opts: &CommitOptions) -> Result<String, GitError> {
        let mut args = vec!["commit", "-F", "-"];
        if opts.amend {
            args.push("--amend");
        }
        if opts.signoff {
            args.push("-s");
        }
        if opts.no_verify {
            args.push("--no-verify");
        }
        match run_git_with_stdin(&self.root, &args, Some(message.as_bytes())) {
            Ok((stdout, stderr)) => {
                let mut out = String::from_utf8_lossy(&stdout).into_owned();
                let err_text = String::from_utf8_lossy(&stderr).into_owned();
                if !out.is_empty() && !err_text.is_empty() {
                    out.push('\n');
                }
                out.push_str(&err_text);
                Ok(out)
            }
            Err(GitError::CommandFailed { cmd, stderr }) => {
                Err(GitError::CommandFailed { cmd, stderr })
            }
            Err(err) => Err(err),
        }
    }

    /// 暂存指定 hunk（工作区 → 暂存区）。索引从 0 开始，指向
    /// [`Repo::file_patch`]（staged=false）解析出的 hunk 顺序。
    pub fn stage_hunks(&self, path: &str, hunk_indices: &[usize]) -> Result<(), GitError> {
        let file = self
            .file_patch(path, false)?
            .ok_or_else(|| GitError::CommandFailed {
                cmd: format!("stage-hunks {path}"),
                stderr: "文件当前没有工作区差异".to_string(),
            })?;
        let patch = build_patch(&file, hunk_indices);
        self.apply_patch(&patch, true, false)
    }

    /// 取消暂存指定 hunk（暂存区 → 工作区）。
    pub fn unstage_hunks(&self, path: &str, hunk_indices: &[usize]) -> Result<(), GitError> {
        let file = self
            .file_patch(path, true)?
            .ok_or_else(|| GitError::CommandFailed {
                cmd: format!("unstage-hunks {path}"),
                stderr: "文件当前没有暂存差异".to_string(),
            })?;
        let patch = build_patch(&file, hunk_indices);
        self.apply_patch(&patch, true, true)
    }

    /// 丢弃工作区指定 hunk（不可恢复，调用方负责确认）。
    pub fn discard_hunks(&self, path: &str, hunk_indices: &[usize]) -> Result<(), GitError> {
        let file = self
            .file_patch(path, false)?
            .ok_or_else(|| GitError::CommandFailed {
                cmd: format!("discard-hunks {path}"),
                stderr: "文件当前没有工作区差异".to_string(),
            })?;
        let patch = build_patch(&file, hunk_indices);
        self.apply_patch(&patch, false, true)
    }

    /// 文件级暂存（`git add --`）。
    pub fn stage_paths(&self, paths: &[&str]) -> Result<(), GitError> {
        let mut args = vec!["add", "--"];
        args.extend(paths.iter().copied());
        run_git(&self.root, &args)?;
        Ok(())
    }

    /// 文件级取消暂存（`git restore --staged --`）。
    pub fn unstage_paths(&self, paths: &[&str]) -> Result<(), GitError> {
        let mut args = vec!["restore", "--staged", "--"];
        args.extend(paths.iter().copied());
        run_git(&self.root, &args)?;
        Ok(())
    }

    /// 文件级丢弃（Rollback 语义，对齐 JetBrains）：
    /// - HEAD 中存在的文件：工作区与暂存区一并回滚到 HEAD；
    /// - HEAD 中不存在的新文件：取消暂存并从磁盘删除。
    ///
    /// 不可恢复，调用方负责确认。
    pub fn discard_paths(&self, paths: &[&str]) -> Result<(), GitError> {
        for path in paths {
            if self.show("HEAD", path)?.is_some() {
                run_git(
                    &self.root,
                    &[
                        "restore",
                        "--source=HEAD",
                        "--staged",
                        "--worktree",
                        "--",
                        path,
                    ],
                )?;
            } else {
                self.unstage_paths(&[path])?;
                let file = self.root.join(path);
                if file.is_file() {
                    std::fs::remove_file(file).map_err(GitError::Io)?;
                }
            }
        }
        Ok(())
    }

    /// 提交历史（默认当前分支，限制条数）。
    pub fn log(&self, limit: usize) -> Result<Vec<crate::log::CommitInfo>, GitError> {
        let args = crate::log::log_args(limit);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = run_git(&self.root, &args)?;
        Ok(crate::log::parse_log(&String::from_utf8_lossy(&out)))
    }

    /// 单文件的行内 blame（`git blame --line-porcelain -w`）。
    pub fn blame(&self, path: &str) -> Result<Vec<crate::blame::BlameLine>, GitError> {
        let args = crate::blame::blame_args(path);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = run_git(&self.root, &args)?;
        Ok(crate::blame::parse_blame(&String::from_utf8_lossy(&out)))
    }

    /// 提交的变更摘要（`git show --stat --oneline`，供历史详情展示）。
    pub fn show_commit_stat(&self, hash: &str) -> Result<String, GitError> {
        let out = run_git(
            &self.root,
            &[
                "show",
                "--stat",
                "--oneline",
                "--no-color",
                "--no-ext-diff",
                hash,
            ],
        )?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// fetch（`git fetch --all`，凭据由 Git Credential Manager 处理）。
    pub fn fetch(&self) -> Result<String, GitError> {
        let out = run_git(&self.root, &["fetch", "--all"])?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// pull（`git pull`，凭据由 Git Credential Manager 处理）。
    pub fn pull(&self) -> Result<String, GitError> {
        let out = run_git(&self.root, &["pull"])?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    /// push（`git push`，凭据由 Git Credential Manager 处理）。
    pub fn push(&self) -> Result<String, GitError> {
        let out = run_git(&self.root, &["push"])?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }
}
