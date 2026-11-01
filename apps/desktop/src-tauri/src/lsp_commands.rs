//! LSP 命令：语言服务器进程生命周期与消息桥接。
//!
//! 协议语义在前端（@codemirror/lsp-client，ADR-0002），本层只做
//! 进程管理与字节搬运。事件命名：`lsp-message:{id}`（服务器 → 前端）、
//! `lsp-stderr:{id}`、`lsp-exit:{id}`。
//!
//! 会话分两种：
//! - **进程会话**：spawn 外部语言服务器（真实环境使用）；
//! - **进程内 mock 会话**（`command == "@mock"`）：不 spawn 子进程，
//!   消息在本进程内直接交给 mock 处理器再以事件回环——开发计划 3.5.2
//!   传输桥的冒烟能力。本测试沙箱会静默拦截 node/自身副本子进程，
//!   因此冒烟走 mock 会话；进程会话已由 fly-lsp-host 的 node 集成
//!   测试覆盖，真实服务器接入时在真实环境验证。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;
use tauri::{Emitter, Manager};

use crate::commands::run_blocking;
use fly_lsp_host::LspHandle;

pub struct LspState(Mutex<HashMap<String, LspSession>>);

pub enum LspSession {
    Process(LspHandle),
    Mock,
}

impl LspState {
    pub fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LspExitPayload {
    pub code: Option<i32>,
}

/// mock 处理器：输入一条客户端消息，输出零或多条服务器消息。
///
/// 与 `run_lsp_echo`（独立二进制模式）保持同一协议行为。
fn mock_handle_message(message: &str, mut emit: impl FnMut(String)) {
    let parsed: Value = match serde_json::from_str(message) {
        Ok(value) => value,
        Err(_) => return,
    };
    let method = parsed.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = parsed.get("id").cloned();
    match method {
        "initialize" => {
            if let Some(id) = id {
                let response = serde_json::json!({
                    "id": id,
                    "result": {
                        "capabilities": {
                            "positionEncoding": "utf-16",
                            "textDocumentSync": { "openClose": true, "change": 1 },
                            "completionProvider": { "triggerCharacters": ["."] },
                            "hoverProvider": true,
                            "documentSymbolProvider": true,
                        },
                        "serverInfo": { "name": "fly-ide-mock-lsp" },
                    },
                });
                emit(response.to_string());
            }
        }
        "initialized" => {}
        "textDocument/didOpen" => {
            let uri = parsed
                .pointer("/params/textDocument/uri")
                .cloned()
                .unwrap_or(Value::Null);
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
                        "source": "fly-ide-mock-lsp",
                    }],
                },
            });
            emit(diagnostics.to_string());
        }
        "textDocument/documentSymbol" => {
            if let Some(id) = id {
                let response = serde_json::json!({
                    "id": id,
                    "result": [
                        {
                            "name": "main",
                            "kind": 12,
                            "range": {
                                "start": { "line": 0, "character": 0 },
                                "end": { "line": 10, "character": 0 },
                            },
                            "selectionRange": {
                                "start": { "line": 0, "character": 4 },
                                "end": { "line": 0, "character": 8 },
                            },
                            "detail": "fn",
                        },
                        {
                            "name": "MOCK_SYMBOL_CONST",
                            "kind": 13,
                            "range": {
                                "start": { "line": 3, "character": 0 },
                                "end": { "line": 3, "character": 20 },
                            },
                            "selectionRange": {
                                "start": { "line": 3, "character": 6 },
                                "end": { "line": 3, "character": 23 },
                            },
                            "detail": "const",
                        },
                    ],
                });
                emit(response.to_string());
            }
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
                emit(response.to_string());
            }
        }
        "textDocument/hover" => {
            if let Some(id) = id {
                let response = serde_json::json!({
                    "id": id,
                    "result": { "contents": { "kind": "markdown", "value": "**mock** hover" } },
                });
                emit(response.to_string());
            }
        }
        "shutdown" => {
            if let Some(id) = id {
                let response = serde_json::json!({ "id": id, "result": null });
                emit(response.to_string());
            }
        }
        "exit" => {}
        other => {
            if let Some(id) = id {
                let response = serde_json::json!({
                    "id": id,
                    "error": { "code": -32601, "message": format!("MethodNotFound: {other}") },
                });
                emit(response.to_string());
            }
        }
    }
}

#[tauri::command]
pub async fn lsp_start(
    app: tauri::AppHandle,
    id: String,
    root: String,
    command: String,
    args: Vec<String>,
) -> Result<(), String> {
    run_blocking(move || {
        let state = app.state::<LspState>();
        let mut sessions = state.0.lock().expect("lsp state 锁");
        // 同 id 重复启动：先停旧会话
        if let Some(old) = sessions.remove(&id) {
            match old {
                LspSession::Process(handle) => handle.stop(),
                LspSession::Mock => {}
            }
        }
        if command == "@mock" {
            // 进程内 mock 会话：不 spawn 子进程
            sessions.insert(id, LspSession::Mock);
            return Ok(());
        }
        let resolved_command = if command == "@self" {
            std::env::current_exe()
                .map_err(|err| format!("解析自身可执行文件失败：{err}"))?
                .to_string_lossy()
                .into_owned()
        } else {
            command
        };
        let app_handle_message = app.clone();
        let app_handle_stderr = app.clone();
        let app_handle_exit = app.clone();
        let id_message = id.clone();
        let id_stderr = id.clone();
        let id_exit = id.clone();
        let handle = fly_lsp_host::spawn_server(
            &resolved_command,
            &args,
            Path::new(&root),
            move |message| {
                let _ = app_handle_message.emit(&format!("lsp-message:{id_message}"), message);
            },
            move |line| {
                let _ = app_handle_stderr.emit(&format!("lsp-stderr:{id_stderr}"), line);
            },
            move |code| {
                let _ =
                    app_handle_exit.emit(&format!("lsp-exit:{id_exit}"), LspExitPayload { code });
            },
        )
        .map_err(|err| format!("启动语言服务器失败：{err}"))?;
        sessions.insert(id, LspSession::Process(handle));
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn lsp_send(app: tauri::AppHandle, id: String, message: String) -> Result<(), String> {
    run_blocking(move || {
        let state = app.state::<LspState>();
        let app_handle = app.clone();
        let mut sessions = state.0.lock().expect("lsp state 锁");
        match sessions.get_mut(&id) {
            Some(LspSession::Process(handle)) => handle
                .send(&message)
                .map_err(|err| format!("发送 LSP 消息失败：{err}")),
            Some(LspSession::Mock) => {
                mock_handle_message(&message, |response| {
                    let _ = app_handle.emit(&format!("lsp-message:{id}"), response);
                });
                Ok(())
            }
            None => Err(format!("语言服务器会话不存在：{id}")),
        }
    })
    .await
}

#[tauri::command]
pub async fn lsp_stop(app: tauri::AppHandle, id: String) -> Result<(), String> {
    run_blocking(move || {
        let state = app.state::<LspState>();
        if let Some(session) = state.0.lock().expect("lsp state 锁").remove(&id) {
            match session {
                LspSession::Process(handle) => handle.stop(),
                LspSession::Mock => {}
            }
        }
        Ok(())
    })
    .await
}
