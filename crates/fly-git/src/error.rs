use std::io;

/// Git 操作错误。展示给前端前统一转为字符串。
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("未找到 git 可执行文件，请安装 Git ≥ 2.30 并加入 PATH")]
    GitNotFound,
    #[error("git 版本过旧：{found}，要求 ≥ {min}")]
    VersionTooOld { found: String, min: String },
    #[error("git 命令执行失败（{cmd}）：{stderr}")]
    CommandFailed { cmd: String, stderr: String },
    #[error("IO 错误：{0}")]
    Io(#[from] io::Error),
}
