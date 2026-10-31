#![forbid(unsafe_code)]

//! 文件监听封装：notify 递归监听 + 防抖合批 + 噪声过滤。
//!
//! 对应开发计划 3.8「外部修改检测」：AI CLI 在磁盘上改文件后，
//! 事件经过 `debounce_ms` 静默窗口合并成一批相对路径推给调用方。
//! 噪声源（`.git/objects`、`node_modules`、`target`、IDE 自身临时文件）
//! 在 [`is_noise_path`] 过滤。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use notify::Watcher;

/// 判断是否应忽略该路径的监听事件（噪声源：`.git/objects`、`node_modules`、
/// `target`、IDE 自身的 `.flyide-*.tmp` 临时文件）。
pub fn is_noise_path(path: &Path) -> bool {
    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    if let Some(name) = &file_name {
        if name.starts_with(".flyide-") && name.ends_with(".tmp") {
            return true;
        }
    }
    let mut prev: Option<std::ffi::OsString> = None;
    for component in path.components() {
        let name = component.as_os_str();
        if name == "node_modules" || name == "target" {
            return true;
        }
        if name == "objects" && prev.as_deref() == Some(std::ffi::OsStr::new(".git")) {
            return true;
        }
        prev = Some(name.to_os_string());
    }
    false
}

/// 一次监听会话；drop 或 [`WatchSession::stop`] 结束后台线程。
pub struct WatchSession {
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl WatchSession {
    /// 停止监听并等待线程退出。
    pub fn stop(mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for WatchSession {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// 递归监听 `root`，把静默 `debounce_ms` 后的一批变更路径推给 `on_batch`。
///
/// 路径为绝对路径字符串。回调在后台线程执行，应尽快返回（重活交给调用方）。
pub fn watch_workspace(
    root: &Path,
    debounce_ms: u64,
    on_batch: impl Fn(Vec<String>) + Send + 'static,
) -> notify::Result<WatchSession> {
    let root: PathBuf = root.to_path_buf();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_thread = cancel.clone();
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();

    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(&root, notify::RecursiveMode::Recursive)?;

    let thread = std::thread::Builder::new()
        .name("fly-watch".into())
        .spawn(move || {
            // watcher 必须活到线程结束
            let _watcher = watcher;
            let mut pending: Vec<String> = Vec::new();
            let mut last_event = Instant::now();
            loop {
                if cancel_thread.load(Ordering::Relaxed) {
                    return;
                }
                match rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(Ok(event)) => {
                        for path in event.paths {
                            if is_noise_path(&path) {
                                continue;
                            }
                            // 与其余 API 一致，统一输出正斜杠路径（前端 doc.path 亦为正斜杠）
                            pending.push(path.to_string_lossy().replace('\\', "/"));
                        }
                        last_event = Instant::now();
                    }
                    Ok(Err(_)) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if !pending.is_empty()
                            && last_event.elapsed() >= Duration::from_millis(debounce_ms)
                        {
                            pending.sort_unstable();
                            pending.dedup();
                            on_batch(std::mem::take(&mut pending));
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
        })?;

    Ok(WatchSession {
        cancel,
        thread: Some(thread),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::AtomicU32;
    use std::sync::{Arc, Mutex};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fly-watch-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("创建临时目录");
        dir
    }

    #[test]
    fn noise_paths_are_filtered() {
        assert!(is_noise_path(Path::new("repo/.git/objects/ab/cdef")));
        // 名为 objects 的普通目录不是噪声（只有 .git/objects 才是）
        assert!(!is_noise_path(Path::new("repo/src/objects/holder.rs")));
        assert!(is_noise_path(Path::new("repo/node_modules/pkg/index.js")));
        assert!(is_noise_path(Path::new("repo/target/debug/x.exe")));
        assert!(is_noise_path(Path::new("repo/.flyide-f.txt-123-456.tmp")));
        assert!(!is_noise_path(Path::new("repo/src/main.rs")));
        assert!(!is_noise_path(Path::new("repo/.git/index")));
        assert!(!is_noise_path(Path::new("repo/.git/HEAD")));
    }

    #[test]
    fn watch_reports_changes_after_debounce() {
        let root = temp_dir();
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_thread = seen.clone();
        let session = watch_workspace(&root, 80, move |paths| {
            seen_thread.lock().expect("锁").extend(paths);
        })
        .expect("启动监听");

        // 触发一批事件：新建文件 + 修改文件
        fs::write(root.join("a.txt"), "hello\n").expect("写入");
        fs::write(root.join("b.txt"), "world\n").expect("写入");
        // 噪声不应出现
        fs::create_dir_all(root.join(".git/objects")).expect("创建");
        fs::write(root.join(".git/objects/xyz"), "noise\n").expect("写入");

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = seen.lock().expect("锁").clone();
            let has_a = snapshot.iter().any(|p| p.ends_with("a.txt"));
            let has_b = snapshot.iter().any(|p| p.ends_with("b.txt"));
            let has_noise = snapshot.iter().any(|p| p.contains("objects"));
            assert!(!has_noise, "噪声路径不应出现在批次中: {snapshot:?}");
            if has_a && has_b {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "10 秒内未收到变更批次: {snapshot:?}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        session.stop();
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stop_stops_the_session() {
        let root = temp_dir();
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_thread = seen.clone();
        let session = watch_workspace(&root, 50, move |paths| {
            seen_thread.lock().expect("锁").extend(paths);
        })
        .expect("启动监听");
        session.stop();
        // stop 后写文件，1 秒内不应有事件
        fs::write(root.join("after-stop.txt"), "x\n").expect("写入");
        std::thread::sleep(Duration::from_millis(1000));
        assert!(seen.lock().expect("锁").is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
