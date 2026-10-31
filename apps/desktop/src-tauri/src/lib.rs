//! Fly IDE 桌面端：Tauri 命令与窗口装配。
//!
//! 该 crate 只做「壳」：命令注册、窗口生命周期、与前端 IPC。
//! 领域逻辑放在 `crates/` 下的各 crate 中，便于独立测试。

mod commands;
mod fs_commands;
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
            fs_commands::fs_read_text,
            fs_commands::fs_write_text,
            fs_commands::fs_list_dir,
            fs_commands::fs_is_dir,
            search_commands::search_files,
            search_commands::search_content,
            search_commands::search_cancel,
            watch_commands::watch_start,
            watch_commands::watch_stop
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
