# ADR-0003：Git 采用 CLI 优先

- 状态：已接受（2026-10-30）
- 关联：[开发计划 3.6](../DEVELOPMENT_PLAN.md)

## 背景

Git 是本产品的核心模块，用户习惯 JetBrains 的 changes/diff/commit 工作流。实现方式候选：`git2`（libgit2）、`gix`（纯 Rust）、shell 调用 git CLI。

## 决策

- **写操作一律走 git CLI**：stage/unstage/commit/apply/branch switch/fetch/push；
- **读操作也先走 CLI**（`status --porcelain=v2 -z`、`diff`、`show`、`log`、`blame`），Rust 负责解析为结构化模型；
- 要求 Git ≥ 2.30；未安装时 Git 功能降级，不阻塞编辑器；
- 同一仓库的 git 命令串行执行，`index.lock` 竞争做有限重试并提示；
- 若超大仓库 status 成为瓶颈，仅把该热点换成 `gix`，写操作保持 CLI。

## 理由

- CLI 正确处理 hooks、GPG 签名、worktree、submodule、sparse checkout、`.gitattributes` 过滤器、文件锁——这些正是审阅工作流的高频边界；
- 用户机器通常已有 Git，不捆绑可避免 GPL 分发与体积问题；
- 进程短生命周期，内存不常驻；
- libgit2/gix 在这些边界上的覆盖与行为差异会直接变成数据风险。

## 后果

- 依赖外部 git：需要在 UI 中检测版本并给安装引导；
- 每次操作有进程启动开销（10–50ms），通过防抖/缓存/串行控制缓解；
- hunk 暂存依赖 patch 生成与 `git apply --cached`，CRLF/无末尾换行/重命名必须有回归测试（往返测试作为门禁）。
