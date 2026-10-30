# ADR-0001：桌面壳与前端技术栈（Tauri 2 + Svelte 5 + CodeMirror 6）

- 状态：已接受（2026-10-30）
- 关联：[开发计划 2.3](../DEVELOPMENT_PLAN.md)

## 背景

产品核心约束是「启动快、内存低、文件操作快、编辑流畅」，且必须支持 5 类语言的语言智能与高质量 git diff。候选方案：Electron + Monaco、Tauri + CodeMirror、纯 Rust 原生 UI、Qt/C++、C# WPF。

## 决策

- 桌面壳：**Tauri 2**（Rust + Windows WebView2），单窗口、按用户安装；
- 前端：**Svelte 5 + TypeScript + Vite**，无 UI 组件库，样式用 CSS 变量主题；
- 编辑器：**CodeMirror 6**，语法高亮用官方 Lezer 语言包，diff 用 `@codemirror/merge`；
- 领域逻辑全部放在 Rust crates（`crates/`），`src-tauri` 只做命令与窗口装配。

## 理由

- Electron 的 Chromium + Node 常驻与「低内存」目标冲突（400MB+）；
- 纯原生 Rust 需自研编辑器/IME/无障碍，周期以年计，风险不可接受；
- CM6 比 Monaco 轻且模块化，官方语言包覆盖 Go/PHP/Python/Rust/Web；
- Rust crates 与 Tauri 解耦，未来若换 UI 层（含原生）可复用文件/搜索/Git 能力。

## 后果

- 接受 WebView2 依赖：Win10/11 通常已安装，安装包需带 Evergreen bootstrapper 与离线方案；
- 内存目标定为 IDE 本体 150–250MB（语言服务器另算），不是 50MB 级原生编辑器；
- 若 Phase 0 实测冷启动 >1.5s 或空闲内存 >300MB，需专项优化后再评估是否调整桌面壳。
