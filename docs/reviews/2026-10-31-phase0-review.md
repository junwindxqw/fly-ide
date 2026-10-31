# Code Review 记录：Phase 0 脚手架

- 日期：2026-10-31
- 范围：全部 Rust crates、Tauri 壳、前端骨架、CI、基准脚本、文档
- 方法：三轮独立审查——①正确性/逻辑；②安全/性能/Windows 专项；③文档与代码一致性

## 统计

| 严重度 | 数量 | 处理 |
|---|---|---|
| P0（错误行为/数据风险） | 5 | 全部修复 |
| P1（安全/正确性隐患） | 7 | 全部修复 |
| P2（一致性/健壮性） | 8 | 全部修复 |

## 问题清单

### P0

| # | 问题 | 位置 | 修复 |
|---|---|---|---|
| 1 | `is_noise_path` 把任何名为 `objects` 的目录都当噪声过滤（如 `src/objects/x.rs`），应只匹配 `.git/objects` | `fly-watch` | 改为检查「前一段是 `.git` 且当前段是 `objects`」，并补正反测试 |
| 2 | 单元测试 `trace_marks_do_not_panic` 把 `test.mark` 写进了用户真实日志目录（污染 `%LOCALAPPDATA%\FlyIDE\logs\startup.jsonl`，实测已发生） | `fly-core` | 新增 `FLY_IDE_LOG_DIR` 环境变量覆盖日志目录，测试重定向到临时目录并断言落盘 |
| 3 | `snapshot.ps1` 默认可执行路径指向 `apps\desktop\src-tauri\target\...`，但 workspace 统一 target 在仓库根 `target\`，默认参数必错 | `scripts/bench` | 默认改为 `target\release\fly-ide.exe`，README 同步修正 |
| 19 | porcelain v2 `2`（重命名）记录的 X 与相似度是**合并的一个字段**（如 `R100`），共 9 个字段；解析器按记忆中的规格写成 10 字段（X、score 分开），路径取不到——**集成测试发现** | `fly-git/src/status.rs` | 改为 `splitn(9)` 取第 8 段；单测样例改为真实格式 |
| 20 | numstat `-z` 的重命名记录是 `added\tdel\t\0旧路径\0新路径\0`（第三字段为空、旧路径在前），与「path 后跟 origPath」的假设相反——**集成测试发现** | `fly-git/src/numstat.rs` | 重写解析：第三字段为空 → 取后续两段为 orig/path；单测覆盖 0/0 重命名与二进制 |

### P1（安全/正确性）

| # | 问题 | 位置 | 修复 |
|---|---|---|---|
| 4 | CSP 为 `null`，与开发计划 3.11「CSP `default-src 'self'`」承诺相悖，XSS 防线缺失 | `tauri.conf.json` | 生产 `csp` 与开发 `devCsp` 双配置，IPC/HMR 白名单化；release+dev 双冒烟验证 |
| 5 | `tauri-plugin-opener` 未被前端使用却保留在依赖/能力清单，平白扩大 WebView 攻击面 | `src-tauri`、`package.json` | 移除插件（Cargo/初始化/capability/npm 依赖四处同步清理） |
| 6 | `mark` 命令接受任意长度字符串直接写日志文件，可被用来无限追加膨胀磁盘 | `src-tauri/src/lib.rs` | 按字符截断至 100 |
| 7 | 所有 Rust crate 未声明 `#![forbid(unsafe_code)]`，与「安全默认」的项目定位不符 | 7 个纯 Rust crate | 纯逻辑 crate 全部加上（`src-tauri` 因宏展开暂不加，已记录） |
| 8 | CI 缺 `cargo fmt --check` 与 `cargo clippy -D warnings` 门禁，计划 4.9 承诺落空 | `.github/workflows/ci.yml` | 补两个门禁步骤（本地先跑通） |
| 9 | CI 无 `permissions: contents: read`（GitHub 默认 token 权限过大）与无超时保护 | `ci.yml` | 补 least-privilege 声明与 `timeout-minutes: 30` |
| 10 | `cargo` 命令未用 `--locked`，`Cargo.lock` 已入库但 CI 不校验锁文件一致性 | `ci.yml` | check/clippy/test 均加 `--locked` |

### P2（一致性/健壮性）

| # | 问题 | 位置 | 修复 |
|---|---|---|---|
| 11 | README 说内存取「中位数」，脚本实际算的是平均值 | `snapshot.ps1` | 实现真正的中位数计算 |
| 12 | 基准脚本与开发版同时运行会失败（两实例共用 WebView2 用户数据目录），脚本不设防 | `snapshot.ps1` | 启动前检测已有 `fly-ide` 进程则中止并提示 |
| 13 | 基准脚本结束时只杀直接子进程，`msedgewebview2` 的孙进程可能残留 | `snapshot.ps1` | 递归杀进程树 |
| 14 | `rust-cache` 配置了两个 workspace 根（`.` 与 `src-tauri`），后者是前者成员，冗余缓存 | `ci.yml` | 简化为默认全仓库缓存 |
| 15 | `.npmrc` 与 `pnpm-workspace.yaml` 重复承载同一设置（pnpm 11 已迁移设置位置） | 根目录 | 删除 `.npmrc`，统一到 `pnpm-workspace.yaml` |
| 16 | setup 中 `if let Some(window) = ... { let _ = window; }` 拿到窗口却不用，语义含糊 | `src-tauri/src/lib.rs` | 改为 `is_some()` 判断 |
| 17 | 缺 `.editorconfig`，多语言混合仓库无缩进/换行约定 | 根目录 | 新增（Rust 4 空格、TS/Svelte/JSON 2 空格、LF、UTF-8） |
| 18 | 缺少 Git 领域的任何真实实现——计划中最高优先级模块在 Phase 0 结束时仍是空壳常量 | `fly-git` | 本轮补齐纵切片（见下） |

## 随本轮新增（非 review 问题）

- **Git 纵切片**（对应开发计划 4.4 第 6 周任务）：`fly-git` 实现 `Repo::discover`、`git status --porcelain=v2 -z --branch` 解析（纯函数 + 集成测试）、`git diff --numstat -z` 解析、`git --version` 检测与最低版本门禁；Tauri 命令 `git_status`/`git_numstat`/`git_version`（`spawn_blocking` 不阻塞 UI）；前端 Git 面板（分组列表 + 增删行统计 + 重命名显示）与状态栏分支显示。

## 结论

20/20 已修复（#19、#20 由真实 git 集成测试在验证阶段抓出——正是「解析器不写集成测试就会带着错误假设上线」的实证）。2 项待后续阶段（`src-tauri` 的 unsafe 审查随功能落地持续；CSP 的长期有效性由 CI 冒烟保障）。修复后全量验证：cargo fmt/clippy/test（28 个测试，含真实 git 集成）、svelte-check、vite build、release 生产模式 CDP 冒烟（Git 面板对本仓库自身 dogfood：分支/分组/增删行统计正确、CSP 下 IPC 正常）、基准复测全部通过。
