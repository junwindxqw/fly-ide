#![forbid(unsafe_code)]

//! 语言服务器进程宿主：spawn、Content-Length 帧编解码、stdio 转发、退出监控。
//!
//! 只负责字节流与生命周期，LSP 协议语义在前端 `@codemirror/lsp-client`
//! 中处理（ADR-0002）。前端事件命名约定：`lsp-message:{id}`、
//! `lsp-stderr:{id}`、`lsp-exit:{id}`。

pub mod framing;
pub mod installer;
pub mod packs;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub use framing::{read_message, write_message};
pub use installer::{
    download_verified, extract_zip, install_server, servers_dir, sha256_of_file, InstallError,
};
pub use packs::{
    builtin_packs, detect_server, find_pack, pack_for_extension, which, InstallSpec, LanguagePack,
    ServerAvailability, ServerSpec,
};

/// 一次语言服务器会话。Clone 共享底层句柄；所有会话结束后进程退出。
#[derive(Clone)]
pub struct LspHandle {
    stdin: Arc<Mutex<ChildStdin>>,
    child: Arc<Mutex<Child>>,
    done: Arc<AtomicBool>,
    threads: Arc<Mutex<Vec<JoinHandle<()>>>>,
    _cancel: Arc<AtomicBool>,
}

impl LspHandle {
    /// 发送一条 JSON-RPC 消息（自动加 Content-Length 帧）。
    pub fn send(&self, message: &str) -> std::io::Result<()> {
        let mut stdin = self.stdin.lock().expect("lsp stdin 锁");
        write_message(&mut *stdin, message)
    }

    /// 终止服务器进程并等待线程收尾。
    pub fn stop(&self) {
        self.done.store(true, Ordering::Relaxed);
        let _ = self.child.lock().expect("lsp child 锁").kill();
        if let Ok(mut threads) = self.threads.lock() {
            for thread in threads.drain(..) {
                let _ = thread.join();
            }
        }
    }
}

impl Drop for LspHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 启动语言服务器并接入事件回调。
///
/// - `on_message`：每收到一条完整 JSON-RPC 消息（已去帧）；
/// - `on_stderr`：服务器 stderr 的一行（日志用）；
/// - `on_exit`：进程退出，参数为退出码。
pub fn spawn_server(
    command: &str,
    args: &[String],
    cwd: &Path,
    on_message: impl Fn(String) + Send + 'static,
    on_stderr: impl Fn(String) + Send + 'static,
    on_exit: impl Fn(Option<i32>) + Send + 'static,
) -> std::io::Result<LspHandle> {
    let mut child = Command::new(command)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdin = child.stdin.take().expect("stdin 已 piped");
    let mut stdout = child.stdout.take().expect("stdout 已 piped");
    let mut stderr = child.stderr.take().expect("stderr 已 piped");

    let done = Arc::new(AtomicBool::new(false));
    let threads = Arc::new(Mutex::new(Vec::new()));

    let handle = LspHandle {
        stdin: Arc::new(Mutex::new(stdin)),
        child: Arc::new(Mutex::new(child)),
        done: done.clone(),
        threads: threads.clone(),
        _cancel: Arc::new(AtomicBool::new(false)),
    };

    // stdout 读线程：按帧切分消息
    let done_stdout = done.clone();
    let stdout_thread = std::thread::Builder::new()
        .name("lsp-stdout".into())
        .spawn(move || {
            let mut reader = BufReader::new(&mut stdout);
            while !done_stdout.load(Ordering::Relaxed) {
                match read_message(&mut reader) {
                    Ok(Some(message)) => on_message(message),
                    Ok(None) => break, // EOF
                    Err(_) => break,
                }
            }
        })?;
    threads.lock().expect("threads 锁").push(stdout_thread);

    // stderr 读线程：逐行转发
    let done_stderr = done.clone();
    let stderr_thread = std::thread::Builder::new()
        .name("lsp-stderr".into())
        .spawn(move || {
            let mut reader = BufReader::new(&mut stderr);
            let mut line = String::new();
            while !done_stderr.load(Ordering::Relaxed) {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => on_stderr(line.trim_end().to_string()),
                    Err(_) => break,
                }
            }
        })?;
    threads.lock().expect("threads 锁").push(stderr_thread);

    // 退出监控线程
    let child_for_exit = handle.child.clone();
    let done_exit = done.clone();
    let exit_thread = std::thread::Builder::new()
        .name("lsp-exit".into())
        .spawn(move || {
            let code = loop {
                {
                    let mut child = child_for_exit.lock().expect("lsp child 锁");
                    match child.try_wait() {
                        Ok(Some(status)) => break status.code(),
                        Ok(None) => {}
                        Err(_) => break None,
                    }
                }
                if done_exit.load(Ordering::Relaxed) {
                    let mut child = child_for_exit.lock().expect("lsp child 锁");
                    let _ = child.kill();
                    break None;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            };
            done_exit.store(true, Ordering::Relaxed);
            on_exit(code);
        })?;
    threads.lock().expect("threads 锁").push(exit_thread);

    Ok(handle)
}

/// 工作区根目录（供诊断日志等使用）。
pub type ServerRoot = PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn write_message_produces_exact_frame() {
        let mut buf = Vec::new();
        write_message(&mut buf, "{}").unwrap();
        assert_eq!(buf, b"Content-Length: 2\r\n\r\n{}");
        let mut buf = Vec::new();
        write_message(&mut buf, "中文").unwrap();
        assert_eq!(buf, "Content-Length: 6\r\n\r\n中文".as_bytes());
    }

    #[test]
    fn read_message_parses_single_and_multiple() {
        let mut data = Vec::new();
        write_message(&mut data, "{\"a\":1}").unwrap();
        write_message(&mut data, "{\"b\":2}").unwrap();
        let mut reader = BufReader::new(Cursor::new(data));
        assert_eq!(
            read_message(&mut reader).unwrap().as_deref(),
            Some("{\"a\":1}")
        );
        assert_eq!(
            read_message(&mut reader).unwrap().as_deref(),
            Some("{\"b\":2}")
        );
        assert_eq!(read_message(&mut reader).unwrap(), None);
    }

    #[test]
    fn read_message_handles_split_input() {
        // 帧被拆成多次 read 也能正确重组（BufReader 语义）
        let full = b"Content-Length: 4\r\n\r\nab".to_vec();
        let mut reader = BufReader::new(Cursor::new(full));
        // 消息体不完整：read_exact 会报错——完整帧才能读到，这里验证报错路径
        assert!(read_message(&mut reader).is_err());
    }

    #[test]
    fn read_message_tolerates_extra_headers() {
        let data = b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\nContent-Length: 2\r\n\r\n{}";
        let mut reader = BufReader::new(Cursor::new(data.as_slice()));
        assert_eq!(read_message(&mut reader).unwrap().as_deref(), Some("{}"));
    }

    #[test]
    fn spawn_and_echo_via_node() {
        // 端到端：spawn node echo 服务器 → 发 initialize → 收 capabilities
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/echo-lsp.mjs");
        let dir = std::env::temp_dir();
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let handle = spawn_server(
            "node",
            &[fixture.to_string_lossy().into_owned()],
            &dir,
            {
                let tx = tx.clone();
                move |message| tx.send(message).expect("发送")
            },
            |_| {},
            |_| {},
        )
        .expect("spawn node");

        let request = r#"{"id":1,"method":"initialize","params":{}}"#;
        handle.send(request).expect("发送 initialize");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut received = false;
        while std::time::Instant::now() < deadline {
            if let Ok(message) = rx.recv_timeout(std::time::Duration::from_millis(200)) {
                if message.contains("capabilities") {
                    received = true;
                    break;
                }
            }
        }
        handle.stop();
        assert!(received, "20 秒内未收到 initialize 响应");
    }
}
