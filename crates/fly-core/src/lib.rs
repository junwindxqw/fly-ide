//! Fly IDE 核心领域模型与通用工具。
//!
//! 该 crate 不依赖 Tauri，可在其他前端（CLI、测试）中复用。

pub const APP_NAME: &str = "Fly IDE";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 启动/性能标记。
///
/// 每个关键节点调用 [`trace::mark`]，以 JSON Lines 追加写入
/// `%LOCALAPPDATA%\FlyIDE\logs\startup.jsonl`，供 Phase 0 基准脚本读取。
/// 写入失败静默忽略——性能标记不能影响启动本身。
pub mod trace {
    use std::fs::{create_dir_all, OpenOptions};
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use std::time::Instant;

    static START: OnceLock<Instant> = OnceLock::new();

    /// 记录进程起点。重复调用无副作用。
    pub fn init() {
        START.get_or_init(Instant::now);
    }

    /// 写入一条启动标记，例如 `rust.main.enter`。
    pub fn mark(name: &str) {
        let start = START.get_or_init(Instant::now);
        let Some(dir) = log_dir() else { return };
        let line = format!(
            "{{\"mark\":{name:?},\"ms\":{:.3}}}\n",
            start.elapsed().as_secs_f64() * 1000.0
        );
        // 日志目录/文件写入失败不影响运行，直接忽略
        let _ = create_dir_all(&dir);
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(dir.join("startup.jsonl"))
        {
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn log_dir() -> Option<PathBuf> {
        std::env::var_os("LOCALAPPDATA").map(|base| {
            PathBuf::from(base)
                .join(crate::APP_NAME.replace(' ', ""))
                .join("logs")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_identity() {
        assert_eq!(APP_NAME, "Fly IDE");
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn trace_marks_do_not_panic() {
        trace::init();
        trace::mark("test.mark");
    }
}
