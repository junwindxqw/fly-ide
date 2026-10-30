# Fly IDE 技术方案与开发计划

> 版本：v0.2（2026-10-30）
> 变更：Git 提升为最高优先级核心模块，新增 3.6 详细设计（对标 JetBrains）与 Phase 2 逐周计划。
> 目标平台：Windows 10 2004+ / Windows 11，x64
> 产品定位：面向「AI 写代码、人来审阅」场景的轻量级 Windows 桌面 IDE

---

## 0. 结论摘要

**做一款「审阅优先」的轻量 IDE，而不是复刻 JetBrains。**

核心工作流是：

1. AI 在外部修改代码；
2. 你在 Fly IDE 中快速打开项目、读代码、跳转定义/引用；
3. 通过 git changes / diff 审阅改动，按文件或按 hunk 暂存；
4. 做少量修改、运行命令，最后提交。

因此优先级不是「功能大而全」，而是：

- **Git 是一等公民（最高优先级）**：对齐 JetBrains 的 Commit/Changes 工作流——changes 列表、高质量 diff、hunk 级暂存、commit、log、blame、冲突解决。这是本产品的立身之本，不是附赠功能；
- **启动快、内存低、文件操作快、编辑不卡**；
- **文件导航 + 全局搜索 + LSP 跳转/引用/补全 + 诊断**；
- 调试器、远程开发、插件市场、数据库工具等一律后置。

### 推荐技术栈

| 层 | 选择 |
|---|---|
| 桌面壳 | **Tauri 2**（Rust + Windows WebView2） |
| 前端 | **Svelte 5 + TypeScript + Vite** |
| 编辑器 | **CodeMirror 6** |
| LSP | **`@codemirror/lsp-client`（官方）** + Rust 管理语言服务器进程 |
| 语法高亮 | CodeMirror 官方 Lezer 语言包（Go/PHP/Python/Rust/JS/TS/HTML/CSS/JSON） |
| Git | **git CLI 优先**（正确性、hooks、签名、worktree/submodule 兼容），热点路径后续可用 gix 优化 |
| 搜索 | Rust：`ignore` + `grep-searcher` + `nucleo` |
| 终端 | xterm.js + portable-pty（ConPTY） |
| 打包 | Tauri NSIS/MSI，按用户安装，自动更新 |

### 关键量化目标（需在 Phase 0 实测校准）

| 指标 | 目标 |
|---|---|
| 冷启动到可交互 | ≤ 1.0s（目标 0.7s） |
| 热启动 | ≤ 0.5s |
| 空闲内存（小项目，无 LSP） | ≤ 180MB |
| 中等项目 + 1 个 LSP | ≤ 350MB（LSP 独立进程，单独展示） |
| 打开 1MB 文件 | ≤ 50ms |
| 打开 10MB 文件 | ≤ 200ms |
| 按键响应 | p95 ≤ 16ms，p99 ≤ 33ms |
| 100k 文件快速打开 | 首次结果 ≤ 50ms，后台索引 ≤ 3s |
| 100k 文件全局搜索 | 冷启动 ≤ 2s，取消 ≤ 100ms |
| 10k 文件 git status | 热刷新 ≤ 300ms |
| 核心安装包 | ≤ 20MB（语言服务器按需下载） |

### 时间预期

- **单人全职 + AI 辅助：约 24–28 周达到 v1.0（目标 24 周）**；
- **第 9 周**可达到「日常看代码 + git 审阅」的 MVP；
- **第 16 周**完成 5 种语言的语言智能；
- 2 名开发并行可压缩到 14–18 周，3 人约 10–14 周（需拆分 UI/编辑器、Rust 后端、LSP/语言包三条线）。

### 已核实的可行性

- 本机已具备 Node 22、Rust 1.98、MSVC Build Tools、WebView2 Runtime、Git 2.54；
- `@codemirror/lang-go`、`lang-rust`、`lang-php`、`lang-python` 等官方语言包可用；
- `@codemirror/lsp-client` 6.3.0 已覆盖补全、hover、诊断、定义/声明/类型定义/实现、引用、重命名、签名帮助、格式化、增量文档同步；
- 代码操作、语义高亮、inlay hints、折叠范围、文档/工作区符号需要在其之上补扩展，工作量可控。

**注意：本机 Go 未安装，PHP 为 7.2（偏旧）。** 开发 Go/PHP 语言服务器时需要安装 Go 工具链和 PHP 8.1+（或使用项目自带运行时）。

---

## 1. 需求分析

### 1.1 目标用户与场景

目标用户是**以 AI 生成代码为主、自己负责审阅和把关的开发者**，主要写 PHP、Go、Python、Rust 和 Web 前端项目。

典型场景：

- **快速读懂陌生/新生成的代码**：打开项目 → 快速打开文件 → 大纲/跳转定义/查找引用 → 全局搜索；
- **审阅 AI 改动**：AI CLI/其他工具在磁盘上改文件 → IDE 检测外部修改 → 查看 git diff → 按 hunk 暂存 → 写 commit；
- **小修小补**：补全、诊断、格式化、重命名、快速修复；
- **运行验证**：在集成终端里跑测试、构建、AI CLI。

### 1.2 功能优先级

#### P0：没有它就不能日常使用

- 打开文件夹 / 最近项目 / 单实例；
- 文件树（懒加载、忽略规则、文件操作）、快速打开（文件名模糊搜索）；
- 多标签编辑器、CodeMirror 6、语法高亮、查找替换、多光标、折叠、括号匹配、注释切换；
- 文件读写：编码检测、BOM/换行符保留、二进制识别、只读文件、原子保存；
- 外部修改检测（AI CLI 改文件后自动刷新/冲突提示）；
- 全局搜索（ripgrep 行为，流式结果、取消、include/exclude）；
- **Git 全流程（核心）**：status、文件树状态标记、changes 面板、diff（统一/并排/词级）、按文件与 hunk 暂存/丢弃、revert、commit（hooks 输出）、分支切换、提交列表、文件历史、blame、冲突检测与基础解决；
- 命令面板、快捷键、主题、设置、会话恢复、热退出（未保存缓冲区恢复）；
- 性能预算与启动/内存基准。

#### P1：v1.0 必须具备

- LSP：补全、hover、签名帮助、诊断、定义/声明/实现、引用、重命名、格式化；
- 5 种语言包：Go、Python、Rust、PHP、TS/JS + HTML/CSS/JSON；
- 大纲（document symbols）、Problems 面板、状态栏诊断计数；
- 代码操作/快速修复、折叠范围、didSave；
- 集成终端（PowerShell / cmd / Git Bash / WSL）；
- 分屏编辑器、面包屑、全局替换、`.editorconfig`；
- git fetch/pull/push（进度 + Git Credential Manager）、stash/shelve、提交详情与提交图、外部改动快照；
- 中文/英文界面、基本无障碍、IME/DPI 测试；
- 自动更新、安装包、代码签名。

#### P2：v1.x 迭代

- 语义高亮（semantic tokens）、inlay hints、工作区符号、代码镜头；
- git 交互式 rebase、cherry-pick、三路合并 UI、行级暂存、PR/GitHub 审阅集成；
- 本地历史（local history）、AI 改动审阅时间线；
- Vue/Svelte/Angular 语言包、ESLint/Biome/Tailwind 集成；
- VS Code 主题导入、minimap、sticky scroll；
- 调试器（DAP）、测试浏览器、任务系统；
- 多根工作区、多窗口、ARM64；
- 远程开发、插件 API、内置 AI 对话/行内编辑。

#### v1 明确不做

- 完整重构套件（extract/move/inline 等）；
- 数据库、Docker、部署、Kubernetes 工具；
- 插件市场；
- 远程开发；
- 内置 AI 聊天/补全（先把外部 AI CLI + 终端 + diff 审阅做好）；
- macOS/Linux 支持。

#### 与 JetBrains 的能力对应关系

借鉴 JetBrains 的产品思路，但不复刻其重量级架构：

| JetBrains 能力 | Fly IDE v1 做法 | 阶段 |
|---|---|---|
| Project 视图 / Recent Files / Search Everywhere | 文件树、最近项目、快速打开（文件名 + 命令 + 符号） | P0/P1 |
| 语法高亮 / 语义高亮 | CodeMirror Lezer 语法高亮；语义高亮由 LSP semantic tokens 补充 | P0/P2 |
| 代码补全 / 参数提示 / 文档弹窗 | LSP completion、signatureHelp、hover | P1 |
| 跳转定义 / 查找引用 / 重命名 / 快速修复 | LSP definition/references/rename/codeAction | P1 |
| Inspections / Problems | LSP diagnostics + Problems 面板 | P1 |
| Git Changes / Commit / Log / Blame / 冲突解决 | git CLI + `@codemirror/merge` + Source Control 面板 | **P0（核心模块）** | 对齐 JetBrains Commit 工具窗、F7/Shift+F7 diff 导航、Ctrl+Alt+Z 还原，详见 3.6 |
| Local History | 保存/外部修改快照 + diff/恢复（轻量实现） | P2 |
| Terminal / Run | xterm.js + ConPTY；运行配置后置 | P1/P2 |
| 全量索引 | **不做**：文件索引只做路径模糊搜索，语义交给 LSP 服务器 | — |
| 调试器 / 数据库 / 远程开发 / 插件市场 | 非 v1 目标 | P3+ |

### 1.3 非功能要求（硬约束）

1. **启动**：冷启动 ≤1s，热启动 ≤0.5s；首屏先出骨架，重组件懒加载。
2. **内存**：空闲 ≤180MB；LSP 服务器独立进程、可关闭；不建立全项目符号索引。
3. **文件操作**：打开 10MB 文件 ≤200ms；快速打开 100k 文件 ≤50ms 出结果；全局搜索流式返回。
4. **编辑**：按键 p95 ≤16ms；大文件降级而不是卡死。
5. **稳定性**：崩溃自动恢复未保存内容；外部修改不丢数据；LSP 崩溃可重启。
6. **安全**：工作区信任模型；LSP 服务器下载校验；默认无遥测；Markdown/HTML 消毒。
7. **Windows 兼容**：长路径、Unicode、CRLF、DPI、只读/锁定文件、Defender、WebView2 缺失兜底。

### 1.4 关键假设

- 单人/小团队开发，AI 辅助；
- 用户机器为 Windows 10/11 x64，WebView2 可安装；
- 用户项目已有 Git、语言工具链（Go/PHP/Python/Node/Rust），IDE 负责检测和引导，不负责安装完整工具链；
- 语言服务器按需下载/检测，不把 JetBrains 级别的语言引擎打包进客户端；
- 先做单根工作区、单窗口，降低复杂度。

---

## 2. 技术选型

### 2.1 评估维度

| 维度 | 权重 | 说明 |
|---|---|---|
| 启动速度 | 高 | 不能出现 JetBrains 式几十秒启动 |
| 内存占用 | 高 | 空闲/多标签/大项目都要可控 |
| 编辑器质量 | 高 | 大文件、IME、多光标、diff、流畅度 |
| 语言智能 | 高 | LSP 客户端成熟度、语言服务器生态 |
| Git 体验 | 高 | diff/stage/commit 的可靠性与性能 |
| 开发效率 | 中高 | 小团队能否在数月内做出可用产品 |
| Windows 适配 | 高 | WebView2、长路径、DPI、终端 |
| 风险 | 中 | 依赖成熟度、许可、后续可维护性 |

### 2.2 候选方案对比

| 方案 | 启动 | 内存 | 编辑器/生态 | 开发效率 | 结论 |
|---|---|---|---|---|---|
| **Tauri 2 + Svelte + CodeMirror 6** | 0.5–1.0s | 100–250MB | CM6 轻量、LSP 官方客户端、语言包齐全 | 高 | **推荐** |
| Electron + Monaco | 1–2s | 400–800MB | 最成熟（VS Code 同源） | 高 | 内存/启动不达标 |
| 纯 Rust 原生（GPUI/egui/iced） | 0.2–0.6s | 80–250MB | 需自研编辑器/IME/无障碍，周期长 | 低 | 风险过高，暂不采用 |
| Qt/C++ + KSyntaxHighlighting | 0.4–1s | 150–350MB | 编辑器成熟但 UI/多光标/diff 需大量自研 | 中低 | 许可与效率不占优 |
| C# WPF + AvalonEdit | 0.5–1.2s | 150–350MB | Windows 原生、编辑器成熟 | 中 | 备选，但 Web 生态/跨语言扩展弱 |
| Fork VS Code / Zed / Lapce | 取决于底座 | 中高 | 功能多但定制/差异化困难 | 中 | 不采用，产品控制力差 |

### 2.3 最终选择与理由

**Tauri 2 + Rust 后端 + Svelte 5 前端 + CodeMirror 6。**

理由：

1. **Tauri 不打包 Chromium**，使用系统 WebView2，安装包和内存显著低于 Electron；
2. **Rust 后端**适合文件系统、搜索、git、进程管理、PTY 等重活，性能和内存可控；
3. **CodeMirror 6**比 Monaco 轻，模块化强，适合按需裁剪；官方语言包覆盖目标语言；
4. **`@codemirror/lsp-client` 是官方 LSP 客户端**，补全/跳转/诊断/重命名/格式化直接可用，避免从零实现 LSP；
5. **语言智能交给成熟的 LSP 服务器**（gopls、rust-analyzer、pyright/basedpyright、phpactor/Intelephense、typescript-language-server），不在 IDE 内重复造语言引擎；
6. **不建立全局符号索引**，避免 JetBrains 式的索引时间和内存开销；文件索引只做路径模糊搜索，语义交给 LSP；
7. Git 使用 **git CLI**，兼容 hooks、签名、worktree、submodule、稀疏检出，避免 libgit2/gix 覆盖不全带来的边界问题。

### 2.4 为什么不是 Electron / 纯原生 / JetBrains 架构

- **Electron**：Chromium + Node 常驻，启动和内存与「轻量」目标冲突；即使 UI 做得再好，基础开销也下不来。
- **纯原生 Rust**：理论性能最好，但文本编辑器是最难做好的组件之一（IME、选区、多光标、无障碍、大文件、diff），从零做需要 1–2 年；GPUI 的 Windows 成熟度也需要验证。
- **JetBrains 架构**：语言引擎、索引、插件体系极强，但 JVM + 全量索引导致重；本项目不需要复刻，直接用 LSP 借用各语言社区的能力。

### 2.5 关键组件清单

| 模块 | 选型 | 说明 |
|---|---|---|
| 桌面壳 | Tauri 2.x | 单窗口、原生菜单、单实例、更新器 |
| 前端框架 | Svelte 5 + TS | 运行时小，细粒度响应式 |
| 构建 | Vite + pnpm | 代码分割、快速 HMR |
| 编辑器 | CodeMirror 6 | `@codemirror/state/view/language/autocomplete/lint/search/merge` |
| 语法 | `@codemirror/lang-*` | Go/PHP/Python/Rust/JS/TS/HTML/CSS/JSON |
| LSP 客户端 | `@codemirror/lsp-client` | 官方；缺失能力自行补扩展 |
| LSP 进程管理 | Rust `tokio::process` | Content-Length 帧、stdio、日志、重启 |
| Git | git CLI | `status --porcelain=v2 -z`、`diff`、`show`、`apply`、`commit` |
| 搜索 | `ignore` + `grep-searcher` + `nucleo` | 尊重 .gitignore、快速模糊匹配 |
| 文件监听 | `notify` | 防抖、合并、焦点时兜底扫描 |
| 编码 | `encoding_rs` + `chardetng` | UTF-8/UTF-16/GB18030 等 |
| 终端 | portable-pty + xterm.js | ConPTY，P1 |
| 设置 | JSONC + serde 校验 | 全局/工作区两级 |
| 日志/追踪 | `tracing` + 文件轮转 | 启动/性能标记 |
| 打包 | Tauri NSIS/MSI | 按用户安装、自动更新、签名 |

### 2.6 技术决策的验证点（Phase 0 必须做）

1. Tauri 基线冷启动/内存：hello world + 空工作区；
2. CodeMirror 打开 1MB/10MB/50MB 文件、1 万行持续输入的帧时间；
3. LSP 链路：mock server → Tauri IPC → CM6 LSP 客户端，测补全/定义/诊断的往返延迟；
4. LSP 跳转到未打开文件：自定义 Workspace 的 `requestFile` / `displayFile`；
5. git diff 渲染 + hunk `git apply --cached`，重点测 CRLF；
6. 100k 文件遍历、快速打开、全文搜索；
7. 结论写入 ADR；若指标严重不达标，先定位是 WebView2 基础开销还是实现问题，再决定是否调整范围/方案。

**Go/No-Go 门槛（Phase 0 结束）**

- Tauri 空壳冷启动 ≤1.2s，空闲内存 ≤120MB；
- 10MB 文件打开 ≤300ms（release）；
- LSP 往返开销 p95 ≤20ms；
- 10k 行编辑 p95 ≤33ms；
- 任意一项严重超标（如空闲内存 >300MB、冷启动 >1.5s）时暂停开发，做专项优化或重新评估桌面壳。

---

## 3. 架构设计

### 3.1 总体结构

```text
┌──────────────────────────────────────────────────────────────┐
│ Tauri 主进程 (Rust)                                          │
│  ├─ 窗口/菜单/单实例/更新器                                  │
│  ├─ WorkspaceService  (打开项目、会话、文件树)              │
│  ├─ FileService       (读写、编码、原子保存、大文件)        │
│  ├─ WatcherService    (notify + 防抖 + 自写过滤)            │
│  ├─ SearchService     (文件索引、nucleo、ripgrep)           │
│  ├─ GitService        (git CLI 封装、状态/差异/提交)        │
│  ├─ LspHost           (语言服务器进程、stdio 帧、日志)      │
│  └─ TerminalHost      (ConPTY，P1)                          │
└──────────────────────────────┬───────────────────────────────┘
                               │ Tauri commands + Channels
                               │ (typed JSON / raw bytes / batched events)
┌──────────────────────────────▼───────────────────────────────┐
│ WebView2: Svelte 5 UI                                        │
│  ├─ Workbench (布局、标签、面板、命令面板、设置)            │
│  ├─ Editor (CodeMirror 6、文档模型、diff)                   │
│  ├─ LSP Bridge (Transport + Workspace + 缺失能力扩展)       │
│  ├─ Git UI / Search UI / Problems / Outline / Terminal      │
│  └─ 主题、快捷键、i18n、通知                                │
└──────────────────────────────┬───────────────────────────────┘
                               │ JSON-RPC message strings
┌──────────────────────────────▼───────────────────────────────┐
│ 语言服务器进程（独立，按需启动）                             │
│  gopls / rust-analyzer / pyright / phpactor / tsserver ...    │
└──────────────────────────────────────────────────────────────┘
```

### 3.2 Rust 后端模块

建议采用 Cargo workspace，领域逻辑与 Tauri 解耦：

```text
crates/
  fly-core/       错误类型、配置模型、路径/URI 工具、事件定义、性能标记
  fly-fs/         文件读取/写入、编码检测、行尾、原子保存、大文件分级
  fly-workspace/  打开项目、文件树懒加载、最近项目、会话、文件索引
  fly-watch/      notify 封装、防抖/合并、自写事件过滤
  fly-search/     ignore 遍历、grep 搜索、nucleo 模糊匹配、取消/流式
  fly-git/        git CLI 执行器、porcelain 解析、diff/log/blame/patch
  fly-lsp-host/   子进程生命周期、Content-Length framing、stderr 日志
  fly-terminal/   ConPTY 会话（P1）
apps/desktop/src-tauri/  Tauri 命令、状态注入、窗口/菜单/更新器
```

原则：

- 每个 crate 都能独立测试，不依赖 Tauri；
- 文件/搜索/git 等阻塞任务走 `tokio::task::spawn_blocking`，避免阻塞 async 运行时；
- 所有长任务支持取消（`CancellationToken`/`AtomicBool`）；
- 错误统一为可序列化结构，前端只展示可操作的信息；
- 大结果集分页/分批推送，禁止一次性把 100 万条结果塞进 IPC。

### 3.3 前端模块

```text
apps/desktop/src/lib/
  app/          启动、布局、命令注册、快捷键、窗口状态、通知
  workspace/    项目树、最近项目、会话、文件操作
  editor/       CodeMirror 集成、DocumentService、主题、diff、编辑命令
  lsp/          Transport、Workspace、语言包、特性扩展、服务器状态
  git/          status/changes/diff/stage/commit/log/blame UI
  search/       快速打开、全局搜索、替换
  panels/       Problems、Outline、Output、Terminal
  settings/     设置模型、JSONC 编辑器、表单
  ui/           按钮、菜单、对话框、树、虚拟列表、图标
  i18n/         zh-CN / en-US
```

关键原则：

- **命令系统先行**：所有动作注册为 command（id、标题、快捷键、when 条件），命令面板/菜单/快捷键/上下文菜单共用；
- **状态分层**：工作区状态、编辑器状态、面板状态相互隔离；避免一个大 store 触发全 UI 重绘；
- **DocumentService 统一文档生命周期**：打开/保存/外部变更/关闭/热退出；
- **CM6 的 `EditorState` 与视图分离**：不可见标签可释放视图，保留轻量状态/光标；
- **懒加载**：编辑器、git、终端、每种语言包都按需 `import()`。

### 3.4 文档模型与数据流

#### 打开文件

```text
用户点击文件
  → Rust FileService 读取字节
  → 检测编码/BOM/EOL/二进制/大小
  → 解码为字符串 + 元数据
  → 前端创建 DocumentService 文档
  → CM6 创建 EditorState/EditorView（按需）
  → LSP Workspace didOpen（如果该语言有服务器）
```

#### 编辑与保存

```text
CM6 编辑
  → DocumentService.version++ / dirty=true
  → LSP 增量 didChange（合并/防抖，请求前强制 sync）
  → Ctrl+S
  → CM6 文本 + 编码/EOL 元数据交给 Rust
  → 临时文件写入 + rename 原子替换
  → 更新 mtime/hash/版本，清 dirty
  → Watcher 识别为自写事件并忽略
```

#### 外部修改（AI CLI 场景）

```text
Watcher 检测到文件变化
  → 计算/比较磁盘 hash
  → 未打开：刷新文件树/git 状态
  → 已打开且未修改：保留光标/滚动，自动重载 + Toast
  → 已打开且有未保存修改：进入冲突状态，提供磁盘版/缓冲区版 diff
```

#### LSP 补全/跳转

```text
CM6 触发补全/跳转命令
  → LSPTransport.send(JSON-RPC)
  → Tauri invoke（队列保序）
  → Rust LspHost 写 stdin
  → 语言服务器处理
  → stdout Content-Length 帧
  → Rust 读取完整消息 → Channel 推送到前端
  → LSPClient 解析 → CM6 扩展渲染/打开文件
```

### 3.5 LSP 集成设计

#### 3.5.1 客户端

使用 `@codemirror/lsp-client`：

- 已支持：initialize/initialized、增量 didChange、didOpen/didClose、补全、hover、签名帮助、诊断、定义/声明/类型定义/实现、引用、重命名、格式化；
- 提供 `client.request()` / `client.notification()`，可自行补缺失能力；
- 提供 `WorkspaceMapping` / `withMapping`，处理请求期间文档变化；
- 提供 `Workspace` 抽象，支持自定义「未打开文件按需加载」。

#### 3.5.2 传输层

```text
@codemirror/lsp-client  Transport
  → TS LspTransport：
      - 发送队列（保证顺序）
      - 微批处理（didChange / 高频消息）
      - 拦截 server→client 请求并应答
      - 记录每类消息延迟/大小
  → Tauri invoke / Channel
  → Rust LspHost：
      - spawn 子进程，stdin/stdout 管道
      - 解析 Content-Length 帧（支持拆包/粘包/大消息）
      - stderr 环形缓冲 + 日志面板
      - 退出码/崩溃通知
  → 语言服务器
```

**必须处理的 server→client 请求**（官方客户端目前统一回 `MethodNotFound`，需在 Transport 层拦截）：

- `workspace/configuration`：按语言/工作区设置返回配置；
- `client/registerCapability` / `unregisterCapability`：返回成功，并记录文件监听等能力；
- `window/workDoneProgress/create`：返回成功，进度显示在状态栏；
- `workspace/applyEdit`：把 WorkspaceEdit 应用到打开的文档/磁盘；
- `window/showDocument`：打开并定位文件；
- `workspace/workspaceFolders`：返回当前根目录；
- `window/showMessageRequest`：显示可操作通知；
- `workspace/semanticTokens/refresh`、`inlayHint/refresh`、`diagnostic/refresh`：使缓存失效并重新请求；
- `telemetry/event`：忽略。

#### 3.5.3 文档工作区

实现自定义 `Workspace`：

- 跟踪所有打开的文档（包括非活动标签）；
- `requestFile(uri)`：文件未打开时，通过 Rust 读取内容，构造 `WorkspaceFile`（`getView()` 返回 null）；
- `displayFile(uri)`：打开标签、加载编辑器、返回 `EditorView`，用于跳转定义/引用；
- `syncFiles()`：把各文档累计的 CM6 `ChangeSet` 同步给服务器；
- `closeFile()`：发送 didClose，释放文档；
- 同一文件多视图 v1 不支持，避免文档同步复杂度。

#### 3.5.4 能力补齐

| 能力 | 实现方式 | 阶段 |
|---|---|---|
| 补全/hover/签名/诊断/定义/引用/重命名/格式化 | 官方客户端直接提供 | P0 |
| didSave | `client.notification("textDocument/didSave")` | P0 |
| 大纲 Outline | `textDocument/documentSymbol` + 自定义面板 | P1 |
| 代码操作/快速修复 | `textDocument/codeAction` + Alt+Enter 菜单 + WorkspaceEdit | P1 |
| 折叠范围 | `textDocument/foldingRange` + CM6 `foldService` | P1 |
| 工作区符号 | `workspace/symbol` + 快速打开 `#` 模式 | P2 |
| 语义高亮 | `textDocument/semanticTokens/full[/delta]` + CM6 装饰 | P2 |
| Inlay hints | `textDocument/inlayHint` + CM6 装饰 | P2 |
| 代码镜头 | `textDocument/codeLens` | P3 |
| 调用/类型层级 | callHierarchy/typeHierarchy | P3 |
| 拉取诊断 | `textDocument/diagnostic` | P2（多数服务器支持推送） |

#### 3.5.5 语言服务器管理

语言包清单示例（`resources/language-packs/go.json`）：

```json
{
  "id": "go",
  "name": "Go",
  "extensions": [".go"],
  "languageId": "go",
  "grammar": "go",
  "server": {
    "command": "gopls",
    "args": ["-mode=stdio"],
    "detect": ["gopls", "%GOPATH%/bin/gopls.exe"],
    "install": { "method": "prebuilt", "source": "official" }
  },
  "settings": {
    "gopls": { "ui.semanticTokens": true, "ui.diagnostic.staticcheck": true }
  }
}
```

管理策略：

- **优先项目自带版本**：`node_modules/.bin`、`vendor/bin`、`.venv`、`rustup`、`go env`；
- 其次系统 PATH，最后提供下载安装（HTTPS + 校验和 + 版本固定）；
- 下载到 `%LOCALAPPDATA%\FlyIDE\servers\<id>\<version>`，无需管理员权限；
- 每个（工作区，语言）一个服务器进程；按需启动，空闲 30 分钟可关闭；
- 崩溃最多自动重启 3 次（指数退避），之后提示用户看日志；
- 状态栏显示服务器状态/内存；Output 面板显示 stderr；
- **轻量模式**：可一键禁用某语言 LSP，只保留语法高亮/搜索；
- **工作区信任**：未信任工作区不自动下载/启动服务器，不应用工作区设置。

#### 3.5.6 五种语言的具体方案

| 语言 | 语法高亮 | 语言服务器 | 运行时/依赖 | 备注 |
|---|---|---|---|---|
| Go | `@codemirror/lang-go` | **gopls**（官方） | Go 工具链 | 需 `go` 在 PATH；支持 build tags/env 配置 |
| Python | `@codemirror/lang-python` | **basedpyright / pyright** | Node（语言包内下载） | 自动检测 `.venv`/conda；ruff 作为可选 lint/format |
| Rust | `@codemirror/lang-rust` | **rust-analyzer** | rustup/toolchain | 内存占用较高；按需启动、可关闭检查 |
| PHP | `@codemirror/lang-php` | **phpactor** 默认；Intelephense 可选 | PHP 8.1+ | 优先 `vendor/bin/phpactor`；Intelephense 为商业软件，不随安装包分发 |
| Web | `lang-javascript`（TS/TSX）、`lang-html`、`lang-css`、`lang-json` | **typescript-language-server / vtsls** + VS Code 提取的 HTML/CSS/JSON server | Node | 优先使用项目 `node_modules/typescript` 版本；Vue/Svelte 作为后续语言包 |

说明：

- 语言智能的质量主要由服务器决定；IDE 的职责是**发现、安装、配置、转发、展示**；
- Node 运行时按需下载到语言包目录，避免核心安装包变大；离线用户可使用离线完整包；
- rust-analyzer 在大型 Rust 项目可能占用数百 MB，必须单独展示进程内存并允许关闭；
- PHP 的 Intelephense 免费版功能有限，高级功能需要用户自行购买许可证，IDE 只做集成，不捆绑；
- Web 首版覆盖 TS/JS/TSX + HTML/CSS/JSON（React 可直接使用）；如果主力项目是 Vue，可把 Volar 提前到 Phase 3（约增加 1 周），否则放到 v1.1。

### 3.6 Git 设计（核心模块，对标 JetBrains）

> Git 是本项目的第一优先级功能：用户习惯用 JetBrains 的方式审阅代码和提交，我们必须把 changes / diff / commit / log / blame / 冲突做到「日常主力工具」水准。进度紧张时优先保 Git，LSP 语言包和终端顺延。

#### 3.6.1 技术路线：git CLI 优先

- 正确处理 hooks、GPG 签名、worktree、submodule、sparse checkout、`.gitattributes` 过滤器、文件锁；
- 用户机器通常已安装 Git；IDE 不打包 Git，避免 GPL 分发和体积问题；
- 读操作（status/diff/log/blame）性能足够，进程短生命周期，内存不常驻；
- 后续如果 status 在超大仓库成为瓶颈，可只把 status/diff 热点换成 `gix`，写操作仍走 CLI。

要求 Git ≥ 2.30；未安装时显示引导，Git 功能降级但不阻塞编辑器。同一仓库的 git 命令串行执行，避免 `index.lock` 竞争；遇到 `index.lock` 做有限重试并明确提示。

#### 3.6.2 功能清单（对标 JetBrains）

| 能力 | JetBrains 对应 | 优先级 | 要点 |
|---|---|---|---|
| Commit/Changes 工具窗 | Commit tool window（Alt+9） | P0 | 未暂存/暂存/未跟踪/冲突分组，文件增删统计，commit 消息编辑，Commit / Commit & Push |
| Diff 查看器 | Show Diff（Ctrl+D） | P0 | 统一/并排切换、词级高亮、忽略空白、折叠未改区域、上下文行、F7/Shift+F7 跳转、只读、两侧语法高亮 |
| Hunk 级暂存/取消/丢弃 | Stage Hunk / Rollback | P0 | 生成标准 patch 后 `git apply --cached`；失败展示 git stderr |
| 文件级操作 | 右键菜单 | P0 | stage/unstage/discard/reveal/open diff/复制路径 |
| Revert | Ctrl+Alt+Z | P0 | 丢弃工作区改动，二次确认 |
| Commit | Commit | P0 | 消息模板、amend、sign-off、`--no-verify` 开关、hooks 输出面板、提交后自动刷新 |
| 分支 | Git Branches | P0 | 切换/创建/删除、脏工作区保护、状态栏显示当前分支与领先/落后 |
| 提交列表 | Git Log | P0 | 分支过滤、作者/日期/消息搜索、两提交 diff、提交详情 |
| 文件历史 | Show History | P0 | 当前文件提交历史，`--follow` 可选 |
| Blame | Annotate with Git Blame | P0 | 行内提交信息开关、悬停显示作者/时间/message |
| 冲突解决 | Resolve Conflicts | P0 | 检测 unmerged 条目，Accept Yours/Theirs/Both，标记 resolved |
| Fetch/Pull/Push | Update Project / Push | P1 | 进度展示，凭据交给 Git Credential Manager，不保存密码 |
| Stash / Shelve | Stash / Shelve | P1/P2 | 列表、apply/pop/drop、shelve 改动 |
| 提交图 | Log Graph | P2 | 分支拓扑可视化 |
| 交互式 rebase / cherry-pick | Rebase / Cherry-pick | P2 | 冲突时提供中止/继续入口 |
| 忽略/未跟踪文件开关 | — | P1 | 显示 ignored、untracked 上限配置 |
| Submodule / Worktree | — | P1 展示 | 状态展示与基本操作 |
| 本地历史 / AI 改动时间线 | Local History | P2 | 保存与外部修改快照，支撑「AI 改了什么」审阅 |
| 复制 diff / 打开 AI CLI 终端 | — | P2 | 工作流增强：把当前 diff 复制给外部 AI，或在终端唤起 AI CLI |

#### 3.6.3 交互与快捷键（JetBrains 习惯）

- 默认提供 **JetBrains keymap 预设**：`Alt+9` Git 工具窗、`Ctrl+K` 提交、`Ctrl+D` 显示 diff、`F7`/`Shift+F7` 上一处/下一处改动、`Ctrl+Alt+Z` 还原、`Alt+←/→` 导航前进后退；同时提供 VS Code 预设（`Ctrl+Shift+G` 等）；
- Changes 树：方向键导航、`Space` 预览 diff、`Enter` 打开、多选批量操作；
- diff 工具栏：并排/统一切换、忽略空白、折叠/展开未改、上一处/下一处、Stage Hunk、Revert、在编辑器中打开、复制 diff；
- **Review 模式**（为审 AI 改动设计）：标记「已看」文件（持久化到会话），`F7` 在改动间跳转，可逐文件过一遍再提交；
- 文件树状态：颜色 + 字母（M/A/D/R/U/C），悬停显示状态与增删统计；
- 状态栏：当前分支、领先/落后、改动文件数、LSP/服务器状态。

#### 3.6.4 性能与缓存

- 刷新触发：保存后、watcher 防抖后、窗口重新聚焦、Git 面板打开；**不在每次按键时刷新**；
- 监听 `.git/index`、`.git/HEAD`、`.git/refs`，不监听 `.git/objects`；
- status 解析在 Rust 完成，前端只接收结构化结果；changes 列表用 `--numstat` 拿统计，不为列表渲染整份 diff；
- diff 只对用户真正打开的文件计算；大 diff 降级为 patch 文本只读模式；
- 大仓库可切换 `--untracked-files=normal`，未跟踪文件上限可配置（默认 5000）；
- 解析 `-z` 输出，避免路径空格/中文/换行问题；
- status 结果按仓库缓存，watcher 事件命中 `.git` 才失效。

#### 3.6.5 Hunk 与行级暂存

- 由 Rust 生成标准 unified patch（保留原文件换行符与编码）；
- `git apply --cached` / `git apply --cached -R`，失败时展示 stderr，并提供「改为文件级暂存」的回退；
- 行级暂存（P1/P2）：对选中行生成零上下文 patch（`-U0`）后应用，失败回退到整个 hunk；
- Windows CRLF、末尾无换行、重命名、二进制文件必须有回归测试。

#### 3.6.6 正确性边界与测试门禁

必须覆盖的边界：CRLF/LF/混合换行、无末尾换行、UTF-8 与 GBK 文件名/内容、重命名检测、可执行位变更、符号链接、大小写不敏感文件系统、submodule 指针变更、worktree、二进制文件、`.gitattributes` 过滤器（LFS 给提示）、patch 应用冲突。

测试要求：

- fixture 仓库矩阵：空仓库、单提交、CRLF 仓库、重命名仓库、submodule、二进制、10k 文件大仓库；
- hunk stage/unstage **往返测试**：apply 后 status 与预期完全一致；
- hooks 输出、amend、sign-off、`--no-verify` 行为测试；
- Git 版本兼容：2.30 / 2.40 / 最新；
- 无 git 环境、`index.lock` 占用、只读仓库的降级测试；
- dogfooding 门禁：Phase 2 起，Fly IDE 自身的开发提交必须通过 Fly IDE 的 Git 工具完成。

### 3.7 搜索与快速打开

#### 快速打开（Ctrl+P）

- Rust 后台遍历项目，生成文件路径索引（尊重 `.gitignore`，默认排除 `node_modules`、`vendor`、`target`、`.git` 等）；
- 路径索引常驻内存（100k 文件约几 MB），磁盘持久化以加速二次启动；
- watcher 增量更新索引；
- `nucleo` 做模糊匹配，返回前 50 条；
- 支持 `>` 命令、`@` 文件内符号、`:` 行号（P1/P2）。

#### 全局搜索

- `ignore` 遍历 + `grep-searcher`，尊重忽略规则；
- 支持大小写、整词、正则、include/exclude；
- 流式分批推送（例如每批 200 条），支持取消；
- 结果按文件分组，显示上下文行；点击跳转；
- 默认结果上限 10k，超出提示「缩小范围」；
- 替换：v1.1 先做单文件替换，再做全局替换预览。

### 3.8 文件读写与大文件

- 读取：Rust 读取字节 → BOM/编码检测（UTF-8 优先，必要时 `chardetng`）→ 解码；
- 写入：临时文件同目录写入 → flush/fsync（可配置）→ rename 替换；保留权限/属性；
- 行尾：检测首行，保留原文件风格；状态栏可切换；
- 编码：状态栏显示；支持重新按编码打开、另存为编码；
- 二进制：NUL 检测，提示用系统程序打开；
- 只读/被锁定：明确提示，重试/另存为；
- 长路径：统一加 `\\?\` 前缀，使用 `dunce` 规范化展示路径；
- 符号链接/ junction：避免遍历环，文件树标记；
- 大小分级：

| 大小 | 行为 |
|---|---|
| < 2MB | 全功能（LSP、语法、折叠、诊断） |
| 2–10MB | 语法高亮，关闭语义高亮/inlay hints/code lens |
| 10–50MB | 纯文本只读，关闭 LSP，文件内查找可用 |
| > 50MB | 提示，默认不加载全文；提供只读预览/外部打开 |

### 3.9 编辑器与 Diff

#### 编辑器

- CodeMirror 6 基础扩展：行号、当前行、选区匹配、括号匹配/自动闭合、缩进、折叠、历史、多光标、矩形选择、拖放；
- 查找替换：CM6 search 或自定义面板（支持正则、大小写、整词、选区）；
- 命令：保存/全部保存、关闭标签、重新打开、切换注释、缩进/反缩进、移动/复制行、转到行、格式化、重命名；
- 键盘映射预设：VS Code / JetBrains / Sublime；`when` 上下文条件；
- IME：不拦截组合输入，中文输入法专项测试；
- 主题：CSS 变量 + CM6 `HighlightStyle`；暗色/亮色/跟随系统；
- 大文件降级策略见 3.8。

#### Diff

- `@codemirror/merge`：统一/并排两种视图；
- 语法高亮两侧；
- 词级差异、忽略空白、折叠未修改区域、上一处/下一处；
- 只读 diff，防止误编辑；「在编辑器中打开」按钮；
- hunk 操作按钮接入 GitService；
- 大 diff 降级为 patch 文本，避免前端卡顿。

### 3.10 性能设计

#### 启动

1. Tauri 窗口先显示骨架（背景色与主题一致，避免白屏）；
2. 前端只加载 shell（命令系统、布局、主题、会话元数据）；
3. 只恢复活动标签的内容，其他标签按需加载；
4. 编辑器、Git、终端、语言包全部懒加载；
5. 启动阶段不扫描全项目、不启动 LSP、不跑 git status；
6. Rust 启动标记 + 前端 `performance.mark` 写入启动日志；
7. 单实例：第二次启动只发送「打开路径」消息，不新开进程。

#### 内存

- 单窗口/单 WebView；
- 干净标签释放 CM6 EditorState/View，切换时从磁盘重载（保留光标/滚动）；
- 脏标签和热退出缓冲区保留；
- 搜索/文件树结果限量、虚拟列表；
- 不建立全局符号索引；
- LSP 按需启动、空闲关闭、独立进程、可单独查看内存；
- Rust 后端不长期保存打开文档的副本；
- 生产包关闭 DevTools、禁用不必要 WebView 特性；
- 定期做内存泄漏回归（开关标签、打开/关闭项目、LSP 重启）。

#### 响应

- 用户输入路径不做网络/磁盘同步 IO；
- 所有 IPC 异步，长任务可取消；
- LSP 消息批量/防抖；补全请求可取消（`$/cancelRequest`）；
- 文件树/搜索结果分批渲染；
- 避免主线程长任务（>50ms）——启动日志/性能面板监控；
- 大文件/大 diff 降级。

### 3.11 安全与隐私

- **工作区信任**：首次打开询问；未信任时禁用自动下载/启动 LSP、不应用工作区设置；
- **CSP**：`default-src 'self'`；不加载远程页面；Markdown/HTML 用消毒器；
- **LSP 输出不可信**：hover/补全文档中的 HTML 必须消毒；不执行服务器返回的 JavaScript；
- **下载校验**：语言服务器/运行时 HTTPS + SHA-256 + 固定版本；支持离线包；
- **路径校验**：文件操作限制在用户明确打开的工作区，除非通过系统对话框选择；
- **Tauri 权限最小化**：capabilities 只开放必要命令；
- **凭据**：Git 使用系统 Credential Manager，IDE 不保存密码/token；
- **遥测**：默认关闭；崩溃报告需显式同意；日志可能含路径，上传前提示；
- **更新**：更新包签名验证；回滚保留上一版安装包。

### 3.12 Windows 专项

- 目标：Windows 10 2004（build 19041）+ / Windows 11，x64；ARM64 后置；
- WebView2：检测 Evergreen Runtime；安装包带 bootstrapper；提供离线完整包选项；
- 长路径：manifest `longPathAware` + `\\?\`；
- 高 DPI：100%/150%/200% 测试；多显示器拖动；
- 深色标题栏：DWM API；
- 文件锁/只读/Defender 扫描：保存失败重试与清晰提示；
- 换行：CRLF 保留；git patch 测试；
- 单实例 + Explorer「打开方式」/文件夹右键（P2）；
- 终端：PowerShell 7 / Windows PowerShell / cmd / Git Bash / WSL 检测；
- 安装：按用户安装到 `%LOCALAPPDATA%\Programs\FlyIDE`，无需管理员权限；
- 签名：尽早采购代码签名证书，避免 SmartScreen 阻断发布；
- **开发期 devUrl 必须写 `http://127.0.0.1:1420` 而不是 `localhost`**：Windows 的 `localhost` 会同时解析到 `::1`，若机器上其他项目的 dev server 占用了 `[::1]:1420`，WebView2 会静默加载到别人的页面（Phase 0 已踩坑，表现为前端标记不出现、页面内容不是本项目）；
- **Windows 上 `npm`/`pnpm` 的依赖安装脚本默认被拦截**：pnpm 11 起 `package.json` 的 `pnpm` 字段已失效，需在 `pnpm-workspace.yaml` 用 `allowBuilds` 批准（如 esbuild）。

### 3.13 仓库结构

```text
fly_ide/
├─ apps/
│  └─ desktop/
│     ├─ src/                    # Svelte 前端
│     │  ├─ lib/app/
│     │  ├─ lib/editor/
│     │  ├─ lib/lsp/
│     │  ├─ lib/git/
│     │  ├─ lib/search/
│     │  ├─ lib/workspace/
│     │  ├─ lib/panels/
│     │  ├─ lib/settings/
│     │  ├─ lib/ui/
│     │  └─ lib/i18n/
│     ├─ src-tauri/              # Tauri 命令/窗口/菜单/更新器
│     ├─ resources/language-packs/
│     └─ vite.config.ts
├─ crates/
│  ├─ fly-core/
│  ├─ fly-fs/
│  ├─ fly-workspace/
│  ├─ fly-watch/
│  ├─ fly-search/
│  ├─ fly-git/
│  ├─ fly-lsp-host/
│  └─ fly-terminal/
├─ packages/
│  ├─ shared-types/              # ts-rs 生成的 IPC 类型
│  └─ test-fixtures/
├─ docs/
│  ├─ DEVELOPMENT_PLAN.md
│  └─ adr/
├─ scripts/
│  ├─ bench/
│  └─ release/
├─ Cargo.toml
├─ pnpm-workspace.yaml
└─ package.json
```

### 3.14 设置与配置

优先级（高 → 低）：

1. 工作区 `.flyide/settings.json`（需信任）；
2. 用户 `%APPDATA%\FlyIDE\settings.json`；
3. 内置默认值；
4. `.editorconfig` 对编辑器选项做补充（不覆盖显式设置）。

设置项示例：

- 编辑器：字体、字号、行高、Tab、自动保存、formatOnSave、trimWhitespace；
- 文件：编码、EOL、排除规则、大文件阈值；
- git：路径、自动刷新、签名、hook 输出；
- 语言：服务器路径/args/env、Python 解释器、Go env、PHP 二进制；
- 界面：主题、图标、语言、快捷键预设；
- 隐私：遥测、更新通道。

快捷键使用独立 `keybindings.json`，支持 `when` 条件（编辑器聚焦、语言、git 面板等）。

---

## 4. 开发计划

### 4.1 阶段总览

| 阶段 | 周期 | 目标 | 里程碑 |
|---|---|---|---|
| Phase 0 | 第 1 周 | 技术验证 + 脚手架 | M0：指标基线，Go/No-Go |
| Phase 1 | 第 2–5 周 | 代码浏览/编辑器内核 | M1：可日常看代码/编辑 v0.1 |
| Phase 2 | 第 6–9 周 | **Git 审阅工作流（核心，最高优先级）** | M2：日常 AI 改动审阅 v0.2（MVP） |
| Phase 3 | 第 10–16 周 | LSP + 5 种语言 | M3：语言智能 v0.5 |
| Phase 4 | 第 17–20 周 | 生产力与体验 | M4：功能完整 v0.9 |
| Phase 5 | 第 21–24 周 | 性能、稳定、发布 | M5：v1.0 |

**优先级说明：Git（Phase 2）是产品立身之本。** 若进度紧张，按「Phase 1 → Phase 2 → Phase 5 → Phase 3 → Phase 4」的顺序保交付，即先把编辑器内核和 Git 做到可日常使用并保证稳定发布，LSP 语言包和终端顺延。

### 4.2 Phase 0：技术验证与脚手架（第 1 周）

**任务**

1. 初始化 Tauri 2 + Svelte 5 + TS + Vite + pnpm；Cargo workspace；
2. 建立 CI（Windows runner）：lint、typecheck、cargo test、构建；
3. Tauri 命令 + typed IPC（ts-rs 生成类型）；Channel 流式事件；
4. 启动/内存基准脚本（PowerShell + 应用内启动标记）；
5. Spike A：Tauri 空壳冷/热启动、空闲内存、WebView2 子进程统计；
6. Spike B：CodeMirror 打开 1MB/10MB/50MB 文件、1 万行输入帧时间；
7. Spike C：mock LSP + `@codemirror/lsp-client` + Tauri IPC，跑通补全/hover/诊断；
8. Spike D：`git status`、diff 渲染、hunk `git apply --cached`，测 CRLF；
9. Spike E：100k 文件遍历、nucleo 快速打开、grep 搜索；
10. 输出 ADR 与基准报告，校准预算。

**产出**：可运行空壳、基准报告、ADR-0001~0005。

**验收**：达到 2.6 的 Go/No-Go 门槛；所有关键依赖在本机可构建。

### 4.3 Phase 1：代码浏览与编辑器内核（第 2–5 周）

**目标**：即使没有 LSP/Git，也能作为「启动极快的代码查看器」日常使用。

**任务**

- 应用外壳：活动栏、侧边栏、编辑器区、底部面板、状态栏、标签页；
- 命令系统、命令面板、快捷键引擎、菜单、通知、对话框；
- 打开文件夹、最近项目、单实例、窗口状态恢复；
- 文件树：懒加载、排序、忽略标记、新建/重命名/删除（回收站）/在资源管理器显示；
- 文件服务：编码/BOM/EOL/二进制/只读/长路径/原子保存；
- 编辑器：CM6 集成、语法高亮、查找替换、多光标、折叠、注释、缩进、保存；
- 标签管理：脏标记、预览标签、关闭/恢复、未保存提示；
- 快速打开（文件路径索引 + nucleo）；
- 全局搜索（流式、取消、include/exclude）；
- 设置、主题、快捷键预设、会话恢复、热退出；
- 外部修改检测/自动重载/冲突提示；
- 性能：懒加载、编辑器状态 LRU、启动标记。

**验收（M1）**

- 冷启动 ≤1.0s（release，开发机）；
- 打开 10MB 文件 ≤200ms；1 万行编辑 p95 ≤16ms；
- 100k 文件快速打开首次结果 ≤50ms；
- 100k 文件搜索 ≤2s，可取消；
- 外部修改 500ms 内可见；不丢未保存内容；
- 连续开关 50 个标签无明显内存增长。

### 4.4 Phase 2：Git 审阅工作流（第 6–9 周，最高优先级）

**目标**：把「AI 改完 → 看 diff → 暂存 → 提交」做到 JetBrains 级体验；达到可日常使用的 MVP。**本阶段不允许为赶进度牺牲正确性。**

**第 6 周：状态与 diff 基础**

- Git CLI 封装：仓库发现、同仓库串行、错误与 `index.lock` 重试；
- status 解析（`--porcelain=v2 -z`）→ 结构化模型（staged/unstaged/untracked/renamed/conflicted）；
- Source Control 面板骨架 + 文件树状态标记 + 状态栏分支信息；
- diff 数据层：取旧/新内容、`--numstat` 统计、编码与换行处理；
- diff 查看器：统一/并排、词级高亮、折叠未改、上一处/下一处、只读、两侧语法高亮。

**第 7 周：暂存与提交**

- 文件级 stage/unstage/discard/revert、批量操作、右键菜单；
- hunk 级 stage/unstage/discard：patch 生成与应用、失败可见、文件级回退；
- commit 面板：消息、amend、sign-off、`--no-verify`、hooks 输出、提交后刷新；
- JetBrains keymap 预设接入（Alt+9 / Ctrl+K / Ctrl+D / F7 / Ctrl+Alt+Z）。

**第 8 周：分支、日志、历史、blame**

- 分支切换/创建/删除、脏工作区保护；
- 提交列表（过滤、搜索、两提交 diff）、文件历史；
- blame 行内注释开关与悬停详情。

**第 9 周：冲突、AI 改动审阅、性能收尾**

- 冲突检测与 Accept Yours/Theirs/Both、标记 resolved；
- 外部修改快照 + 「AI 改动的文件」时间线 + Review 模式（已看标记）；
- 复制 diff、在终端唤起 AI CLI（工作流增强）；
- 性能优化：status 缓存、防抖、聚焦重扫、大仓库 untracked 上限。

**验收（M2）**

- JetBrains keymap 下 Alt+9 / Ctrl+D / F7 / Ctrl+Alt+Z 全流程可走通；
- 10k 文件 status 热刷新 ≤300ms；打开 diff ≤300ms（中等文件）；大 diff 不卡死；
- hunk 暂存/取消往返正确（CRLF / 无末尾换行 / 重命名 / 二进制场景全覆盖）；
- commit 触发 hooks 并显示输出；amend/sign-off 行为符合 git 语义；
- log/blame 在 10k 提交仓库可用（blame 单文件 ≤500ms）；
- AI CLI 外部修改后 1s 内出现在 changes 列表；Review 模式可逐文件审阅；
- 无 git 环境时编辑器不受影响；`index.lock` 被占用时有明确提示；
- 连续 200 次 stage/refresh 无内存泄漏；
- **dogfooding：从本周起，Fly IDE 自身的提交通过 Fly IDE 的 Git 工具完成。**

**M2 结束即可作为个人日常主力版本使用（看代码 + git 审阅提交）。**

### 4.5 Phase 3：语言智能（第 10–16 周）

**目标**：5 种语言达到「可用的 JetBrains 风格」基础体验：补全、跳转、引用、诊断、重命名、格式化、大纲。

**第 10–11 周：LSP 基础设施**

- Rust LspHost：进程/帧/日志/退出/重启/取消；
- TS LspTransport：队列、批处理、server request 拦截、延迟统计；
- 自定义 Workspace：打开文档跟踪、未打开文件按需加载、displayFile；
- 语言包清单、检测、下载、校验、安装、版本固定；
- 设置注入、日志面板、状态栏、信任模型；
- 用 mock server + gopls/TS server 做集成测试。

**第 12–13 周：Go + Web**

- gopls：补全、定义、引用、诊断、重命名、格式化、大纲；
- typescript-language-server/vtsls：TS/JS/TSX；使用项目 TypeScript 版本；
- HTML/CSS/JSON server；
- 快速打开 `@` 符号、Outline、Problems 面板。

**第 14 周：Python + Rust**

- basedpyright/pyright：解释器/venv 检测、诊断、补全、跳转；
- rust-analyzer：项目检测、按需启动、内存展示、轻量模式；
- ruff（可选）。

**第 15 周：PHP**

- phpactor 检测/配置（项目 `vendor/bin` 优先）；
- Intelephense 可选集成（用户自备许可证）；
- PHP 版本/PHP 二进制设置；
- 补全/定义/引用/诊断/格式化/大纲。

**第 16 周：特性补齐与打磨**

- 代码操作/快速修复（Alt+Enter）；
- 折叠范围、didSave、签名帮助、hover 链接；
- 服务器崩溃重启、超时、取消、错误提示；
- LSP 性能/内存回归；
- 5 语言 fixture 项目测试。

**验收（M3）**

- 5 种语言补全/定义/引用/诊断/重命名/格式化可用；
- 跳转到未打开文件正确打开并定位；
- LSP 请求 UI 不阻塞；IPC 开销 p95 ≤5ms；
- 服务器崩溃自动恢复；日志可见；
- 无 LSP 时内存 ≤180MB；rust-analyzer 内存单独可见并可关闭；
- 项目自带 TS/PHP 版本优先生效。

### 4.6 Phase 4：生产力与体验（第 17–20 周）

**任务**

- 集成终端：xterm.js + ConPTY，PowerShell/cmd/Git Bash/WSL，链接/搜索/复制粘贴；
- 分屏编辑器（两个组）、标签拖拽；
- 面包屑、大纲联动、Problems 过滤；
- 全局替换（预览 + 单文件应用）；
- `.editorconfig`、formatOnSave、自动保存选项；
- 本地历史（保存/外部修改快照，diff/恢复）；
- git log/blame、分支列表、基础冲突解决；
- 中文/英文 i18n；
- 无障碍、IME、DPI、高对比度基础测试；
- 欢迎页、语言环境检测/安装向导；
- 复制 diff、打开 AI CLI 终端等工作流增强。

**验收（M4）**

- 终端在 PowerShell/Git Bash/WSL 下正常；
- 分屏/标签切换无卡顿；
- 全局替换预览正确，可撤销；
- 本地历史可恢复误改；
- 核心流程键盘可完成；
- 性能预算无回归。

### 4.7 Phase 5：稳定、性能与发布（第 21–24 周）

**任务**

- 性能专项：启动剖析、内存泄漏、超大仓库/大文件、LSP 空闲回收；
- Windows 边界：WebView2 缺失、长路径、Unicode、只读/锁定、网络盘、Defender、DPI；
- 崩溃处理：panic 日志、WebView 错误、会话恢复、热退出；
- 安装包：NSIS/MSI、按用户安装、图标、文件关联/右键（可选）；
- 代码签名；Tauri updater + 签名清单；
- 文档：安装、语言配置、快捷键、FAQ、故障排查；
- 2 周 dogfooding（用 Fly IDE 开发 Fly IDE 的前端/文档）；
- Beta 发布、收集反馈、修复 P0/P1。

**验收（M5 / v1.0）**

- 所有性能预算达标；
- 崩溃率/无响应显著低于目标（crash-free >99.5%）；
- 安装 → 打开项目 → 配好 5 语言 → 审阅/提交全流程可用；
- 自动更新可用；
- 安装包签名，无 SmartScreen 严重阻断；
- 无 P0 数据丢失/安全问题。

### 4.8 工作量粗估

| 工作流 | 人日（含测试） |
|---|---|
| 脚手架、CI、IPC、性能基线 | 10 |
| 应用外壳/命令/快捷键/设置/主题/i18n | 18 |
| 编辑器/文档模型/大文件/会话 | 22 |
| 文件树/文件服务/监听 | 15 |
| 搜索/快速打开 | 10 |
| Git（status/diff/stage/commit/log/blame/冲突） | 25 |
| LSP 基础设施 + 缺失特性 | 25 |
| 5 种语言包与集成测试 | 18 |
| 终端/分屏/本地历史/杂项 | 15 |
| 性能/稳定/打包/签名/文档 | 20 |
| 合计 | **约 178 人日** |

单人全职、AI 辅助按 1.4–1.5 倍效率折算约 24–28 周；2 人并行约 14–18 周；3 人约 10–14 周。关键路径是「编辑器文档模型 → LSP Workspace/Transport → 语言包」，Git 线可独立并行。

### 4.9 测试与质量门禁

- **Rust**：单元测试（编码/原子保存/framing/porcelain 解析/patch）、集成测试（临时 git 仓库）、criterion 基准、fuzz（patch/framing）；
- **TypeScript**：Vitest（文档模型、快捷键、transport、fuzzy）、svelte-check、ESLint/Prettier；
- **E2E**：Playwright 跑 Web UI 流程；Tauri WebDriver 跑原生文件对话框/终端 smoke；
- **LSP**：mock server 协议测试 + 5 种真实服务器 fixture 测试（nightly）；
- **性能**：`scripts/bench/`， nightly 记录启动/内存/打开文件/搜索/status；回归 >10% 报警；
- **Windows 矩阵**：Win10 22H2、Win11 23H2/24H2；100/150/200% DPI；中文 IME；无 Git/无 Node/PHP 环境；长路径；只读文件；大仓库；
- **安全**：CSP、LSP HTML 消毒测试、路径穿越、下载校验、`cargo audit`/`pnpm audit`；
- **发布门禁**：每阶段性能预算 + 无 P0 数据丢失 + 关键流程 E2E 通过。

### 4.10 启动期前 10 个工作日（Phase 0 + Phase 1 开头）

| 天 | 工作 |
|---|---|
| 1 | Tauri/Svelte/TS 脚手架、Cargo workspace、CI、空壳窗口 |
| 2 | 启动/内存基准脚本、骨架首屏、typed IPC、Channel |
| 3 | 文件服务 + 打开文件夹 + 文件树根目录 + CM6 打开文件 |
| 4 | 标签/保存/编码/EOL/大文件分级；10MB 文件基准 |
| 5 | 快速打开文件索引 + nucleo；100k 文件基准 |
| 6 | LSP Transport（mock server）+ 补全/hover 冒烟 |
| 7 | gopls 或 TS server 接入；未打开文件跳转；IPC 延迟测量 |
| 8 | git status + diff（CM6 merge）渲染 |
| 9 | hunk 暂存 + CRLF/无末尾换行测试 |
| 10 | 全局搜索 spike；汇总基准报告；Go/No-Go 评审，调整计划 |

---

## 5. 风险与应对

| 风险 | 影响 | 概率 | 应对 |
|---|---|---|---|
| WebView2 内存/启动高于预期 | 核心卖点受损 | 中 | Phase 0 实测；懒加载/单窗口/释放标签；若基础开销本身超标，再评估原生 UI（成本高，避免中途切换） |
| `@codemirror/lsp-client` 新、缺特性 | 语言功能受阻 | 中 | Phase 0 spike；Transport 层补齐 server request；缺失特性用 `client.request` 自建；必要时 vendor/上游补丁 |
| 语言服务器安装/版本/许可问题 | 开箱即用差 | 高 | 语言包管理器 + 项目自带版本优先 + 离线包；不捆绑 Intelephense；清晰安装向导 |
| rust-analyzer/PHP 服务器内存高 | 低内存目标受影响 | 中 | 独立进程、按需启动、空闲关闭、单独展示内存、轻量模式 |
| Git hunk 暂存/CRLF 边界错误 | 数据风险 | 中 | 大量 fixture 测试；失败不静默；提供文件级回退；hunk 往返测试作为门禁 |
| Git 体验不达 JetBrains 水准 | 核心卖点受损 | 中 | Git 作为最高优先级工作流单独排期；Phase 0 先验证 diff/hunk 链路；Phase 2 起 dogfooding 只用自家 Git 工具提交 |
| 文件监听漏事件/网络盘 | 状态不同步 | 中 | 防抖 + 聚焦重扫 + 手动刷新；网络盘降级轮询 |
| 大仓库/大文件卡顿 | 体验受损 | 中 | 分级降级；虚拟列表；结果上限；超时/取消 |
| 功能范围膨胀 | 延期 | 高 | 严格 P0/P1/P2；v1 不做调试/远程/插件/AI 对话；每阶段验收 |
| Windows 签名/更新 | 发布受阻 | 中 | 第 1 个月启动证书采购；先做 unsigned beta；updater 签名单独测试 |
| 单人开发 bus factor | 项目风险 | 中 | ADR/文档/测试齐全；关键模块解耦；可外包语言包测试 |
| IME/无障碍问题 | 中文用户核心体验 | 中 | 尽早中文输入测试；不拦截 composition；Narrator 基础检查 |
| LSP 输出 XSS | 安全 | 低中 | Markdown/HTML 消毒；CSP；不执行服务器脚本 |
| 工作区恶意配置 | 安全 | 中 | 工作区信任；不自动运行任务/下载服务器；路径校验 |

---

## 6. 后续演进

### v1.1（发布后 4–8 周）

- 代码操作/快速修复完善、语义高亮、inlay hints、工作区符号；
- 本地历史 + AI 改动审阅时间线；
- git push/pull/fetch、提交图、blame 增强、三路冲突；
- Vue/Svelte/Angular 语言包；ESLint/Biome/Tailwind；
- 全局替换、`.editorconfig` 完整支持；
- 主题导入、minimap、sticky scroll。

### v1.5

- 调试器（DAP：Xdebug、Delve、debugpy、rust-analyzer/lldb、Chrome DevTools Protocol）；
- 测试浏览器、任务/运行配置；
- 多根工作区、多窗口；
- ARM64；
- 任务模板/一键运行 AI CLI。

### v2

- 远程开发（SSH/容器）；
- 插件 API（WASM/JS，工作区信任）；
- 内置 AI 对话/行内编辑（可选）；
- GitHub/GitLab PR 审阅集成；
- 可选符号索引（SQLite FTS）与语义搜索；
- 主题/图标生态。

---

## 7. 待确认的产品决策（不阻塞开发）

以下先给默认值，后续可调整：

1. **许可证**：默认私有/商业闭源；依赖全部使用 MIT/Apache/BSD 等宽松许可；不捆绑 GPL 工具（Git 通过外部调用）。
2. **界面语言**：中文优先，同时预留英文 i18n。
3. **语言服务器分发**：默认按需下载 + 检测系统/项目版本；另提供离线完整包。
4. **内置 AI**：v1 不做，先做终端 + 外部 AI CLI + diff 审阅；v2 再评估。
5. **多根/多窗口**：v1 不做。
6. **品牌名**：暂用 Fly IDE（目录 `fly_ide`），发布前可改。
7. **团队/周期**：按单人全职 + AI、24 周估算；若 2–3 人可并行压缩。

---

## 附录 A：本机环境核实（2026-10-30）

| 项 | 状态 |
|---|---|
| Node.js | v22.23.0 |
| npm | 10.9.8 |
| Rust | 1.98.1 |
| Cargo | 1.98.1 |
| Git | 2.54.0.windows.1 |
| Python | 3.14.6 |
| PHP | 7.2.9（偏旧，PHP 语言服务器测试需升级） |
| Go | 未安装（Go 语言包测试需安装） |
| MSVC Build Tools | 已安装（VS 2022 BuildTools） |
| WebView2 Runtime | 已安装（156.0.4314.50） |
| OS | Windows 11 Pro，build 26300 |
| CPU/RAM | Ryzen 5 8600G，6C/12T，15.1GB 可用 |

## 附录 B：已核实的 npm 包（2026-10-30）

| 包 | 版本 |
|---|---|
| `@tauri-apps/api` / `@tauri-apps/cli` | 2.12.4 |
| `svelte` | 5.57.3 |
| `@codemirror/lsp-client` | 6.3.0 |
| `@codemirror/lang-go` | 6.0.1 |
| `@codemirror/lang-rust` | 6.0.2 |
| `@codemirror/lang-php` | 6.0.2 |
| `@codemirror/lang-python` | 6.2.1 |
| `@codemirror/lang-javascript` | 6.2.5 |
| `@codemirror/lang-html` | 6.4.12 |
| `@codemirror/lang-css` | 6.3.1 |
| `@codemirror/lang-json` | 6.0.2 |
| `@codemirror/merge` | 6.12.3 |
| `vscode-jsonrpc`（备选，不首选） | 10.0.0 |

## 附录 C：性能测量方法

- **启动**：Rust 在进程启动、窗口创建、WebView 加载、前端 shell 挂载、编辑器就绪、工作区就绪各打时间戳；前端 `performance.mark` 回传；写入 `%LOCALAPPDATA%\FlyIDE\logs\startup.jsonl`；基准脚本取最后一次运行。
- **内存**：PowerShell 递归汇总 `fly-ide` 与子进程 `msedgewebview2` 的 WorkingSet/PrivateMemory；空闲 10s 后采样 3 次取中位数；LSP 进程单独统计。
- **编辑器**：Playwright/CDP 注入按键序列，采集 `PerformanceObserver` 长任务与帧间隔；CM6 benchmark 模式输出 p50/p95/p99。
- **文件/搜索/git**：Rust criterion + 真实 fixture 仓库；记录冷/热、取消、结果数。
- **回归门禁**： nightly 运行，超过基线 10% 失败并输出对比。

## 附录 D：建议先写的 ADR

- ADR-0001：桌面壳与前端技术栈（Tauri + Svelte + CM6）
- ADR-0002：LSP 传输与 Workspace 设计
- ADR-0003：Git 采用 CLI 优先
- ADR-0004：前端为文档权威源，Rust 不常驻文档副本
- ADR-0005：语言服务器按需下载与信任模型
- ADR-0006：性能预算与测量方法

---

**下一步**：按 Phase 0 开始脚手架与 spike；完成基准报告后再进入 Phase 1。
