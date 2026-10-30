# ADR-0002：LSP 客户端选型（官方 `@codemirror/lsp-client` + Rust 进程宿主）

- 状态：已接受（2026-10-30）
- 关联：[开发计划 3.5](../DEVELOPMENT_PLAN.md)

## 背景

语言智能（补全、跳转、引用、诊断、重命名、格式化）必须复用各语言社区成熟的 LSP 服务器（gopls、rust-analyzer、pyright/basedpyright、phpactor/Intelephense、typescript-language-server），不在 IDE 内重复实现语言引擎。

## 决策

- 协议客户端使用官方 **`@codemirror/lsp-client`**（已覆盖补全/hover/签名/诊断/定义/声明/类型定义/实现/引用/重命名/格式化、增量文档同步、Workspace 映射）；
- **Rust 侧只做进程宿主**：spawn、Content-Length 帧解析、stdio 转发、stderr 日志、重启策略；不做 LSP 协议语义；
- 传输层（TS）负责：发送队列保序、微批处理、拦截 server→client 请求（`workspace/configuration`、`client/registerCapability`、`window/workDoneProgress/create`、`workspace/applyEdit`、`window/showDocument`、`workspace/workspaceFolders` 等）；
- 自定义 `Workspace` 实现：跟踪所有打开文档，未打开文件按需读取，`displayFile` 负责打开标签；
- 缺失能力（代码操作、semantic tokens、inlay hints、folding range、document/workspace symbols）通过 `client.request()` 自建 CM6 扩展补齐。

## 理由

- 官方客户端与 CM6 深度集成，避免从零实现 LSP 的进度/取消/位置映射；
- 进程管理放 Rust 可保证生命周期、日志与重启策略可靠；
- 文档文本权威在前端（CM6），与 LSP 的 UTF-16 行/列模型天然匹配，避免双份文档同步。

## 后果

- 官方客户端较新（6.3.0），需在 Phase 0 spike 验证 CRLF、大文件、未打开文件跳转；
- server→client 请求必须自行应答，否则 `workspace/configuration` 等会拿到 MethodNotFound；
- LSP 服务器进程按需启动、可 idle 关闭，内存单独展示（rust-analyzer 可能数百 MB）。
