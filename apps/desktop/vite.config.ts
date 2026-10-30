import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const nodeProcess = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process;
const host = nodeProcess?.env?.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],

  // Vite 选项：仅在 `tauri dev` / `tauri build` 时生效
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: {
      // 忽略 src-tauri，避免 Rust 改动触发前端刷新
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    target: "chrome110",
    minify: "esbuild",
    sourcemap: false,
  },
});
