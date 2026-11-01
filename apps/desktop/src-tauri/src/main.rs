// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 内置 mock 语言服务器模式（LSP 链路 spike / 冒烟用）：
    // `fly-ide.exe --lsp-echo` 以 stdio 协议运行，不启动 GUI。
    // 选用自 spawn 是因为测试沙箱会静默拦截 node.exe 子进程，
    // 而自进程零外部依赖，真实环境同样可用。
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|arg| arg.as_str()) == Some("--lsp-echo") {
        std::process::exit(fly_ide_lib::run_lsp_echo());
    }
    fly_ide_lib::run()
}
