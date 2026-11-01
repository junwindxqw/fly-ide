//! Fly IDE 桌面端：Tauri 命令与窗口装配。
//!
//! 该 crate 只做「壳」：命令注册、窗口生命周期、与前端 IPC。
//! 领域逻辑放在 `crates/` 下的各 crate 中，便于独立测试。

mod commands;
mod fs_commands;
mod lsp_commands;
mod search_commands;
mod watch_commands;

use fly_core::trace;
use tauri::Manager;

/// IPC 冒烟测试命令。
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {name}! You've been greeted from Rust!")
}

/// 返回应用标识信息（名称 + 版本）。
#[tauri::command]
fn app_info() -> String {
    format!("{} v{}", fly_core::APP_NAME, fly_core::VERSION)
}

/// 写入一条启动/性能标记（见 `fly_core::trace`）。
#[tauri::command]
fn mark(name: &str) {
    // 名称来自 WebView，截断防日志文件被撑爆
    let name: String = name.chars().take(100).collect();
    trace::mark(&name);
}

pub fn run() {
    trace::init();
    trace::mark("rust.main.enter");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(search_commands::SearchState::new())
        .manage(watch_commands::WatchState::new())
        .manage(lsp_commands::LspState::new())
        .invoke_handler(tauri::generate_handler![
            greet,
            app_info,
            mark,
            commands::git_version,
            commands::git_status,
            commands::git_numstat,
            commands::git_diff_file,
            commands::git_stage,
            commands::git_unstage,
            commands::git_discard,
            commands::git_apply_hunks,
            commands::git_commit,
            commands::git_log,
            commands::git_blame,
            commands::git_show_commit,
            fs_commands::fs_read_text,
            fs_commands::fs_write_text,
            fs_commands::fs_list_dir,
            fs_commands::fs_is_dir,
            search_commands::search_files,
            search_commands::search_content,
            search_commands::search_cancel,
            watch_commands::watch_start,
            watch_commands::watch_stop,
            lsp_commands::lsp_start,
            lsp_commands::lsp_send,
            lsp_commands::lsp_stop
        ])
        .setup(|app| {
            trace::mark("tauri.setup.begin");
            if app.get_webview_window("main").is_some() {
                trace::mark("tauri.window.ready");
            }
            trace::mark("tauri.setup.end");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Fly IDE");
}

/// 内置 mock 语言服务器（`fly-ide.exe --lsp-echo`）。
///
/// 协议与 `crates/fly-lsp-host/tests/fixtures/echo-lsp.mjs` 一致：
/// initialize/didOpen(回诊断)/completion/hover/shutdown/exit，
/// 每条消息记录到 cwd 的 `fly-mock-lsp.log` 供冒烟断言链路。
pub fn run_lsp_echo() -> i32 {
    use std::io::{BufReader, Write};

    fn log_line(entry: &str) {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("fly-mock-lsp.log")
            .and_then(|mut file| writeln!(file, "{entry}"));
    }

    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    log_line("{\"started\":true}");

    loop {
        let message = match fly_lsp_host::read_message(&mut reader) {
            Ok(Some(message)) => {
                log_line(&format!("{{\"got_bytes\":{}}}", message.len()));
                message
            }
            Ok(None) => {
                log_line("{\"stdin\":\"EOF\"}");
                return 0; // EOF
            }
            Err(err) => {
                log_line(&format!(
                    "{{\"stdin_error\":{}}}",
                    serde_json::json!(err.to_string())
                ));
                return 1;
            }
        };
        let parsed: serde_json::Value = match serde_json::from_str(&message) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let entry = serde_json::json!({
            "method": parsed.get("method").cloned().unwrap_or(serde_json::json!("*")),
            "id": parsed.get("id").cloned().unwrap_or(serde_json::json!(null)),
        });
        log_line(&entry.to_string());

        let method = parsed.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let id = parsed.get("id").cloned();
        match method {
            "initialize" => {
                let result = serde_json::json!({
                    "capabilities": {
                        "positionEncoding": "utf-16",
                        "textDocumentSync": { "openClose": true, "change": 1 },
                        "completionProvider": { "triggerCharacters": ["."] },
                        "hoverProvider": true,
                    },
                    "serverInfo": { "name": "fly-ide-lsp-echo" },
                });
                if let Some(id) = id {
                    let response = serde_json::json!({ "id": id, "result": result });
                    let _ = fly_lsp_host::write_message(&mut out, &response.to_string());
                }
            }
            "initialized" => {}
            "textDocument/didOpen" => {
                let uri = parsed
                    .pointer("/params/textDocument/uri")
                    .cloned()
                    .unwrap_or(serde_json::json!(null));
                let diagnostics = serde_json::json!({
                    "method": "textDocument/publishDiagnostics",
                    "params": {
                        "uri": uri,
                        "diagnostics": [{
                            "range": {
                                "start": { "line": 0, "character": 0 },
                                "end": { "line": 0, "character": 5 },
                            },
                            "message": "mock diagnostic from server",
                            "severity": 1,
                            "source": "fly-ide-lsp-echo",
                        }],
                    },
                });
                let _ = fly_lsp_host::write_message(&mut out, &diagnostics.to_string());
            }
            "textDocument/completion" => {
                if let Some(id) = id {
                    let response = serde_json::json!({
                        "id": id,
                        "result": [
                            { "label": "MOCK_COMPLETION_ALPHA", "kind": 2 },
                            { "label": "MOCK_COMPLETION_BETA", "kind": 2 },
                        ],
                    });
                    let _ = fly_lsp_host::write_message(&mut out, &response.to_string());
                }
            }
            "textDocument/hover" => {
                if let Some(id) = id {
                    let response = serde_json::json!({
                        "id": id,
                        "result": { "contents": { "kind": "markdown", "value": "**mock** hover" } },
                    });
                    let _ = fly_lsp_host::write_message(&mut out, &response.to_string());
                }
            }
            "shutdown" => {
                if let Some(id) = id {
                    let response = serde_json::json!({ "id": id, "result": null });
                    let _ = fly_lsp_host::write_message(&mut out, &response.to_string());
                }
            }
            "exit" => return 0,
            other => {
                // 未实现的请求回 MethodNotFound，避免客户端挂起
                if let Some(id) = id {
                    let response = serde_json::json!({
                        "id": id,
                        "error": { "code": -32601, "message": format!("MethodNotFound: {other}") },
                    });
                    let _ = fly_lsp_host::write_message(&mut out, &response.to_string());
                }
            }
        }
    }
}
