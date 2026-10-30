# Fly IDE

轻量级 Windows 桌面 IDE，面向 **「AI 写代码、人来审阅」** 的工作流。

- 平台：Windows 10 2004+ / Windows 11（x64）
- 定位：启动快、内存低、文件操作快、编辑流畅；Git 审阅体验对齐 JetBrains
- 语言：PHP、Go、Python、Rust、Web（TS/JS/HTML/CSS/JSON），语言智能基于 LSP

## 文档

- [技术方案与开发计划](docs/DEVELOPMENT_PLAN.md) —— 需求分析、技术选型（Tauri 2 + Rust + Svelte 5 + CodeMirror 6）、架构设计、24 周分阶段计划
- 架构决策记录（ADR）：`docs/adr/`（开发启动后补充）

## 当前状态

规划完成，Phase 0（技术验证 + 脚手架）进行中。

## 开发

```bash
pnpm install          # 安装依赖
pnpm dev              # 启动开发版（Tauri dev）
pnpm check            # 前端类型检查（svelte-check）
pnpm build:web        # 仅构建前端
pnpm check:rust       # cargo check --workspace --all-targets
pnpm test             # cargo test --workspace
pnpm clippy           # cargo clippy（-D warnings）
```

仓库结构见 [docs/DEVELOPMENT_PLAN.md](docs/DEVELOPMENT_PLAN.md) 3.13；性能基准脚本见 [scripts/bench](scripts/bench/README.md)；Phase 0 基线结果见 [docs/benchmarks/phase0-baseline.md](docs/benchmarks/phase0-baseline.md)。

## 许可证

暂未确定（开发中）。依赖组件均为 MIT / Apache-2.0 / BSD 等宽松许可。
