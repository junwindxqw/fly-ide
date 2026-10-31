#![forbid(unsafe_code)]

//! 语言服务器进程宿主：spawn、Content-Length 帧、stdio 转发、日志与重启。
//!
//! 只负责字节流与生命周期，LSP 协议语义在前端 `@codemirror/lsp-client` 中处理。
//!
//! Phase 0：先固定重启策略常量（对应开发计划 3.5.5）。

/// 崩溃后最大自动重启次数，超过则提示用户查看日志。
pub const MAX_RESTARTS: u32 = 3;

/// 语言服务器默认空闲关闭时间（分钟），0 表示不自动关闭。
pub const IDLE_SHUTDOWN_MINUTES: u64 = 30;

/// Content-Length 帧头最大长度，超过视为异常帧。
pub const MAX_HEADER_BYTES: usize = 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_policy_is_bounded() {
        const {
            assert!(MAX_RESTARTS >= 1);
            assert!(MAX_HEADER_BYTES >= 256);
        }
    }
}
