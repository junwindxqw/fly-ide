# @fly-ide/desktop

Fly IDE 桌面端：Tauri 2 壳 + Svelte 5 前端。

- 前端源码：`src/`（纯 Svelte + Vite，未使用 SvelteKit）
- Tauri 命令与窗口装配：`src-tauri/`（领域逻辑在仓库根 `crates/`）
- 语言包清单：`src-tauri/resources/language-packs/`（Phase 3 接入）

## 开发

```bash
pnpm dev        # tauri dev（自动启动 Vite dev server，端口 1420）
pnpm build      # tauri build（打包安装包）
pnpm check      # svelte-check
pnpm build:web  # 仅构建前端到 dist/
```

注意：

- `src-tauri/tauri.conf.json` 的 `devUrl` 使用 `http://127.0.0.1:1420`（不要改成 `localhost`，原因见根目录开发计划 3.12）；
- 生产模式二进制需要 `cargo build --release -p fly-ide --features tauri/custom-protocol`，否则仍走 devUrl。
