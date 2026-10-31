//! git 子进程封装：统一执行入口、错误携带 stderr、版本检测。

use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use crate::error::GitError;

/// 在仓库目录执行 git 子命令，返回原始 stdout 字节。
///
/// 失败时错误信息携带完整命令与 stderr，绝不静默吞错（对应开发计划 3.6.5）。
pub fn run_git(repo_dir: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_dir)
        .args(args)
        .output()
        .map_err(|err| match err.kind() {
            ErrorKind::NotFound => GitError::GitNotFound,
            _ => GitError::Io(err),
        })?;
    if !output.status.success() {
        return Err(GitError::CommandFailed {
            cmd: format!("git {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(output.stdout)
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
