//! git 子进程封装：统一执行入口、错误携带 stderr、版本检测。

use std::io::{ErrorKind, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::GitError;

/// 在仓库目录执行 git 子命令，返回原始 stdout 字节。
///
/// 失败时错误信息携带完整命令与 stderr，绝不静默吞错（对应开发计划 3.6.5）。
pub fn run_git(repo_dir: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
    let (stdout, _) = run_git_with_stdin(repo_dir, args, None)?;
    Ok(stdout)
}

/// git 子进程原始输出。
pub struct GitOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// [`run_git_with_stdin`] 的底层版本：不因非零退出码报错，
/// 由调用方决定如何呈现（如 `git commit` 的提示输出在 stdout）。
pub fn run_git_capture(
    repo_dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
) -> Result<GitOutput, GitError> {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo_dir).args(args);
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| match err.kind() {
            ErrorKind::NotFound => GitError::GitNotFound,
            _ => GitError::Io(err),
        })?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin
            .write_all(input)
            .and_then(|_| stdin.flush())
            .map_err(GitError::Io)?;
    }
    let output = child.wait_with_output().map_err(GitError::Io)?;
    Ok(GitOutput {
        success: output.status.success(),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

/// [`run_git`] 的带 stdin 版本：`input` 写入子进程标准输入后关闭（EOF）。
///
/// 用于 `git commit -F -`、`git apply -` 等从 stdin 读补丁/消息的命令。
/// 返回合并前的 stdout 与 stderr。非零退出码转换为
/// [`GitError::CommandFailed`]（stderr 字段携带两个流的合并文本）。
pub fn run_git_with_stdin(
    repo_dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
) -> Result<(Vec<u8>, Vec<u8>), GitError> {
    let output = run_git_capture(repo_dir, args, input)?;
    if !output.success {
        let mut text = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if text.is_empty() {
            text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
        return Err(GitError::CommandFailed {
            cmd: format!("git {}", args.join(" ")),
            stderr: text,
        });
    }
    Ok((output.stdout, output.stderr))
}

/// 解析 `git --version` 输出为 (major, minor, patch)。
///
/// Windows 版输出形如 `git version 2.54.0.windows.1`，只取前三段数字。
pub fn parse_version(output: &str) -> Option<(u32, u32, u32)> {
    let token = output.split_whitespace().nth(2)?;
    let mut parts = token.split(['.', '-']);
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// 检测本机 git 版本；未安装时返回 `None`。
pub fn detect_version() -> Option<(u32, u32, u32)> {
    let output = Command::new("git").arg("--version").output().ok()?;
    parse_version(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn parses_windows_style_version() {
        assert_eq!(
            parse_version("git version 2.54.0.windows.1"),
            Some((2, 54, 0))
        );
        assert_eq!(parse_version("git version 2.30.0"), Some((2, 30, 0)));
        assert_eq!(parse_version("git version 1.8.3.1"), Some((1, 8, 3)));
    }

    #[test]
    fn rejects_malformed_version_output() {
        assert_eq!(parse_version("garbage"), None);
        assert_eq!(parse_version("git version"), None);
        assert_eq!(parse_version(""), None);
    }
}
