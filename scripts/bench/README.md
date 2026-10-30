# Fly IDE 性能基准脚本（Phase 0）

对应开发计划「附录 C：性能测量方法」。均为只读测量，不修改系统设置。

## 用法

```powershell
# 启动 + 内存 + 启动标记 一并测量（默认用 debug 构建）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench\snapshot.ps1

# 指定 release 构建
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\bench\snapshot.ps1 -Exe target\release\fly-ide.exe
```

**重要**：生产模式的 release 二进制必须带 `custom-protocol` feature 构建，否则仍走 devUrl、白屏：

```bash
cargo build --release -p fly-ide --features tauri/custom-protocol
```

测试前请先停掉 `pnpm dev` 启动的开发版（同一 app identifier 的两个实例会争抢 WebView2 用户数据目录）。

## 输出

- 冷启动标记（来自 `%LOCALAPPDATA%\FlyIDE\logs\startup.jsonl`，各节点毫秒数）
- 进程树内存：`fly-ide` + 子进程 `msedgewebview2` 的 WorkingSet / PrivateMemory 中位数

## 说明

- 启动前会先删除旧的 `startup.jsonl`，保证读到的是本次运行；
- 采样期间请勿操作窗口，空闲 10 秒后取中位数；
- CI 不跑基准（机器差异大），由开发机 nightly 手工/定时运行并记录结果。
