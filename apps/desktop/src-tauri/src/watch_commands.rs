//! 文件监听命令：工作区递归监听，防抖合批后以 `fs-changed` 事件广播。
//!
//! 一次只保留一个会话；切换工作区时旧会话自动停止。
//! `tauri::State<'_>` 的借用不能进入 `'static` 闭包，通过 `AppHandle` 取 state。

use std::sync::Mutex;

use serde::Serialize;
use tauri::{Emitter, Manager};

use fly_watch::WatchSession;

use crate::commands::run_blocking;

pub struct WatchState(Mutex<Option<WatchSession>>);

impl WatchState {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsChangedPayload {
    pub paths: Vec<String>,
}

#[tauri::command]
pub async fn watch_start(app: tauri::AppHandle, root: String) -> Result<(), String> {
    run_blocking(move || {
        let state = app.state::<WatchState>();
        let mut guard = state.0.lock().expect("watch state 锁");
        // 先停旧会话
        if let Some(session) = guard.take() {
            session.stop();
        }
        let app_handle = app.clone();
        let session = fly_watch::watch_workspace(std::path::Path::new(&root), 300, move |paths| {
            let _ = app_handle.emit("fs-changed", FsChangedPayload { paths });
        })
        .map_err(|err| format!("启动文件监听失败：{err}"))?;
        *guard = Some(session);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn watch_stop(app: tauri::AppHandle) -> Result<(), String> {
    run_blocking(move || {
        let state = app.state::<WatchState>();
        if let Some(session) = state.0.lock().expect("watch state 锁").take() {
            session.stop();
        }
        Ok(())
    })
    .await
}
