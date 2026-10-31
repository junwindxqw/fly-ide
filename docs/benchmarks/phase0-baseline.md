# Phase 0 基准报告（脚手架基线）

- 日期：2026-10-30（首次），2026-10-31（code review + Git 纵切片后复测）
- 机器：Windows 11 Pro build 26300，AMD Ryzen 5 8600G（6C/12T），15.1GB 可用内存
- 构建：`cargo build --release -p fly-ide --features tauri/custom-protocol`（生产模式，内嵌前端资源）
- 前端：`pnpm build:web`（Vite 8 + Svelte 5），复测产物 JS 54.54 kB / gzip 21.68 kB，CSS 4.37 kB（含 Git 面板）
- 测量工具：`scripts/bench/snapshot.ps1`（启动标记 + 进程树内存采样，中位数）
- 场景：空壳（无项目、无编辑器、无 LSP），只读测量，未操作窗口

## 冷启动标记（2026-10-31 复测）

| 节点 | 耗时（距进程启动） |
|---|---|
| `rust.main.enter` | 0.01 ms |
| `tauri.setup.begin` | ~385 ms |
| `tauri.window.ready` | ~385 ms |
| `tauri.setup.end` | ~385 ms |
| `ui.boot` | ~465 ms |
| `ui.mounted` | **~466 ms** |

> 注意：构建后的**首次**启动约 1.0s（Defender 首次扫描新二进制的一次性开销），第二次起稳定在 ~0.47s。基准口径以第二次及以后为准。

## 内存（进程树，稳态中位数）

| 指标 | 数值 |
|---|---|
| `fly-ide` 主进程 WorkingSet | ~4 MB |
| WebView2 子进程（6 个）合计 | ~105 MB |
| **进程树 WorkingSet 合计** | **~109 MB** |
| 进程树 Private 合计 | ~34 MB |

## 与预算对比（复测口径）

| 指标 | 预算 | 实测 | 结论 |
|---|---|---|---|
| 冷启动到可交互 | ≤ 1.0s | ~0.47s | 达标 |
| 空闲内存（小项目，无 LSP） | ≤ 180MB | ~109MB | 达标 |
| 初始 JS 体积 | ≤ 300KB gzip | 21.7KB gzip | 达标 |

## 排障记录（已写入开发计划 3.12）

1. **devUrl 必须用 `127.0.0.1`**：本机 `localhost` 同时解析 `::1`，其他项目的 dev server 占用 `[::1]:1420` 时，WebView2 会静默加载别人的页面（表现为前端标记不出现）。
2. **`cargo build --release` 不等于生产模式**：Tauri 以 `tauri/custom-protocol` feature 区分开发/生产；裸 release 构建仍走 devUrl，页面白屏。生产模式构建需 `--features tauri/custom-protocol`（`pnpm tauri build` 会自动处理）。
3. **同一 app identifier 的两个实例不能同时运行**：debug 与 release 共用 WebView2 用户数据目录，第二个实例创建 webview 失败（HRESULT `0x8007139F`）。基准测试前需先停掉开发版。
4. **pnpm 11 起 `package.json` 的 `pnpm` 字段失效**：批准依赖安装脚本需写在 `pnpm-workspace.yaml` 的 `allowBuilds`。
5. **Windows PowerShell 5.1 按 ANSI 读取脚本**：`.ps1` 必须纯 ASCII，中文注释会导致解析错误。

## 下一步测量（随功能落地逐阶段复测）

- Phase 1：打开 1MB/10MB 文件耗时、1 万行编辑帧时间、100k 文件快速打开、全局搜索、外部修改检测延迟、20 标签内存；
- Phase 2：10k 文件 git status、diff 打开、hunk 往返、连续 200 次 stage/refresh 内存；
- Phase 3：LSP 请求 UI 阻塞时间、5 语言 fixture 项目内存。
