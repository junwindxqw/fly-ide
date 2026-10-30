# ADR-0006：性能预算与测量方法

- 状态：已接受（2026-10-30）
- 关联：[开发计划附录 C](../DEVELOPMENT_PLAN.md)

## 背景

「轻量、快」是产品的立身之本，必须可测量、可回归，而不是主观感受。

## 决策

预算（开发机：Ryzen 5 8600G / 16GB / Win11，Phase 0 实测后校准）：

| 指标 | 目标 |
|---|---|
| 冷启动到可交互 | ≤ 1.0s |
| 热启动 | ≤ 0.5s |
| 空闲内存（小项目，无 LSP） | ≤ 180MB |
| 打开 1MB / 10MB 文件 | ≤ 50ms / ≤ 200ms |
| 按键响应 | p95 ≤ 16ms |
| 10k 文件 git status（热刷新） | ≤ 300ms |
| 100k 文件快速打开 | 首次结果 ≤ 50ms |
| 核心安装包 | ≤ 20MB |

测量方法：

- **启动**：Rust `fly_core::trace::mark()` 在进程启动、窗口创建、WebView 就绪、UI 挂载等节点写 `%LOCALAPPDATA%\FlyIDE\logs\startup.jsonl`；前端通过 `mark` 命令补记 UI 节点；
- **内存**：`scripts/bench/snapshot.ps1` 递归汇总 `fly-ide` 与子进程 `msedgewebview2` 的 WorkingSet/PrivateMemory；
- **编辑器**：Playwright/CDP 注入按键，采集长任务与帧间隔；
- **Rust 微基准**：criterion；真实 fixture 仓库记录冷/热。

门禁：

- 每个阶段验收必须包含对应预算项；
- nightly 跑基准，回归 >10% 报警；
- 不在 CI 跑基准（机器差异大）。

## 后果

- 需要在关键路径埋点，埋点失败必须静默（不能影响启动）；
- 预算未达标时优先砍范围/降级策略，而不是牺牲正确性。
