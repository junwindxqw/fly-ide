//! Fly IDE 桌面端：Tauri 命令与窗口装配。
//!
//! 该 crate 只做「壳」：命令注册、窗口生命周期、与前端 IPC。
//! 领域逻辑放在 `crates/` 下的各 crate 中，便于独立测试。

mod commands;

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
            commands::git_apply_hunks
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
