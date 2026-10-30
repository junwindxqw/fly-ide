import "./app.css";
import { invoke } from "@tauri-apps/api/core";
import { mount } from "svelte";
import App from "./App.svelte";

const errors: string[] = [];
(window as unknown as { __errors: string[] }).__errors = errors;
window.addEventListener("error", (event) => {
  errors.push(`error: ${event.message}`);
});
window.addEventListener("unhandledrejection", (event) => {
  errors.push(`unhandledrejection: ${String(event.reason)}`);
});

// 启动标记：证明前端模块已执行（Phase 0 排障用）
void invoke("mark", { name: "ui.boot" }).catch((err) => {
  errors.push(`mark(ui.boot) 失败: ${String(err)}`);
});

const target = document.getElementById("app");
if (!target) {
  throw new Error("找不到 #app 挂载点");
}

let app: ReturnType<typeof mount>;
try {
  app = mount(App, { target });
} catch (err) {
  errors.push(`mount 失败: ${String(err)}`);
  target.innerHTML = `<pre style="color:#ff8888;padding:16px;white-space:pre-wrap">挂载失败：${String(err)}</pre>`;
}

export default app;
