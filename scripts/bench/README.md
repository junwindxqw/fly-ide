# Fly IDE 性能基准脚本（Phase 0）

对应开发计划「附录 C：性能测量方法」。均为只读测量，不修改系统设置。

## 用法

```powershell
# 构建 release 基准二进制（生产模式必须带 custom-protocol feature）
pnpm bench:build

# 启动 + 内存 + 启动标记 一并测量
pnpm bench
# 等价于：
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench\snapshot.ps1 -Exe target\release\fly-ide.exe
```

## 行为说明

- **冲突保护**：检测到已运行的 `fly-ide` 实例会直接中止——同一 app identifier 的两个实例共用 WebView2 用户数据目录，第二个会创建 webview 失败；
- 启动前删除旧的 `startup.jsonl`，并等到出现 `tauri.setup.end` 标记再读取，保证启动链完整；
- 内存采样默认 10 秒，输出各样本与**中位数**（脚本内的 `Get-Median`）；
- 结束时递归结束整个进程树（含 WebView2 孙进程）。

## 输出

- 冷启动标记（各节点距进程启动的毫秒数）
- 进程树内存：`fly-ide` + 子进程 `msedgewebview2` 的 WorkingSet / PrivateMemory 中位数

## 说明

- CI 不跑基准（机器差异大），由开发机手工/定时运行并记录到 `docs/benchmarks/`；
- PowerShell 5.1 按 ANSI 读取脚本，`scripts/` 下所有 `.ps1` 必须保持纯 ASCII。
