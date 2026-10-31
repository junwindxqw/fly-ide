// 编辑器工作流冒烟测试（CDP 驱动 release 生产版）：
// 文件树打开文件 → 编辑 → 脏标记 → Ctrl+S 保存 → 磁盘验证 → 关闭标签。
// 前置：以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 启动 fly-ide.exe。
// 用法：node scripts/dev/smoke-editor.mjs <工作区根目录>
// 注意：沙箱环境每 git/fs 命令 ~3s，全部断言用轮询（90s 上限）。

import { setTimeout as sleep } from "node:timers/promises";
import fs from "node:fs";
import path from "node:path";

const repoRoot = process.argv[2];
if (!repoRoot) {
  console.error("usage: node scripts/dev/smoke-editor.mjs <workspace-root>");
  process.exit(2);
}

const scratchPath = path.join(repoRoot, "scratch-editor.txt");
const scratchName = "scratch-editor.txt";

// 12 行内容，末尾换行
fs.writeFileSync(scratchPath, Array.from({ length: 12 }, (_, i) => `line${i + 1}`).join("\n") + "\n");

const targets = await fetch("http://127.0.0.1:9222/json/list").then((r) => r.json());
const page = targets.find((t) => t.type === "page");
if (!page) {
  console.error("FAIL: no page target (is fly-ide running with remote debugging?)");
  process.exit(2);
}

const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});

let nextId = 0;
const pending = new Map();
ws.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
};

function evaluate(expression) {
  const id = ++nextId;
  return new Promise((resolve) => {
    pending.set(id, (message) => resolve(message.result?.result?.value));
    ws.send(JSON.stringify({ id, method: "Runtime.evaluate", params: { expression, returnByValue: true, awaitPromise: true } }));
  });
}

const evalJson = async (expression) => JSON.parse(await evaluate(`JSON.stringify(${expression})`));

async function waitFor(name, getterExpr, { timeoutMs = 90000, intervalMs = 800 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let last;
  for (;;) {
    last = await evalJson(getterExpr);
    if (last && last.ok) {
      return last;
    }
    if (Date.now() > deadline) {
      throw new Error(`等待 ${name} 超时，最后状态: ${JSON.stringify(last)}`);
    }
    await sleep(intervalMs);
  }
}

let passed = 0;
let failed = 0;
async function step(name, fn) {
  try {
    await fn();
    passed += 1;
    console.log(`PASS  ${name}`);
  } catch (error) {
    failed += 1;
    console.error(`FAIL  ${name}: ${error.message}`);
  }
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

const uiStateExpr = `(() => ({
  tabs: document.querySelectorAll('.tab').length,
  activeTab: (document.querySelector('.tab-active .tab-name') || {}).textContent || null,
  dirtyDot: (() => { const dot = document.querySelector('.tab-active .tab-dirty'); return dot ? dot.textContent.trim() : null; })(),
  editor: !!document.querySelector('.editor-host .cm-editor'),
  editorText: (() => { const cm = document.querySelector('.editor-host .cm-content'); return cm ? cm.textContent.slice(0, 40) : null; })(),
  status: [...document.querySelectorAll('.statusbar span')].map((s) => s.textContent),
  errors: window.__errors || [],
}))()`;

await step("前端已挂载且无错误", async () => {
  const state = await evalJson(`({ ok: !!document.querySelector('.workbench'), errors: window.__errors || [] })`);
  assert(state.ok, "workbench 未渲染");
  assert(state.errors.length === 0, `前端错误: ${JSON.stringify(state.errors)}`);
});

await step("文件树加载工作区", async () => {
  await evaluate(`(() => {
    const inp = document.querySelector('.sidebar input.git-input');
    inp.value = ${JSON.stringify(repoRoot)};
    inp.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitFor("文件树行", `({ ok: document.querySelectorAll('.tree-row').length > 0 })`);
});

await step("点击 scratch 文件在编辑器打开", async () => {
  await evaluate(`(() => {
    const row = [...document.querySelectorAll('.tree-name')].find((b) => b.textContent.trim() === ${JSON.stringify(scratchName)});
    if (!row) throw new Error('文件树中找不到 ${scratchName}');
    row.click();
  })()`);
  const state = await waitFor("CM 编辑器", `({ ok: !!document.querySelector('.editor-host .cm-editor') })`);
  assert(state.ok, "编辑器未渲染");
});

await step("编辑文本出现脏标记", async () => {
  await evaluate(`(() => {
    const content = document.querySelector('.editor-host .cm-content');
    content.focus();
    // 光标移到文档开头（Home 键）：execCommand 插入到当前光标
    document.execCommand('insertText', false, 'HELLO ');
  })()`);
  const state = await waitFor("脏标记", `({ ok: (document.querySelector('.tab-active .tab-dirty') || {}).textContent === '●' })`);
  assert(state.ok, "脏标记未出现");
});

await step("Ctrl+S 保存落盘", async () => {
  await evaluate(`(() => {
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true }));
  })()`);
  // 轮询磁盘内容（本地 fs 直读，不走应用 IPC）
  const deadline = Date.now() + 90000;
  for (;;) {
    const content = fs.readFileSync(scratchPath, "utf8");
    if (content.startsWith("HELLO line1")) {
      break;
    }
    if (Date.now() > deadline) {
      throw new Error(`保存未落盘，当前开头: ${JSON.stringify(content.slice(0, 30))}`);
    }
    await sleep(800);
  }
});

await step("状态栏显示光标/编码/行尾", async () => {
  const state = await evalJson(uiStateExpr);
  const status = state.status.join("|");
  assert(status.includes("UTF-8"), `状态栏缺编码: ${status}`);
  assert(status.includes("LF"), `状态栏缺行尾: ${status}`);
  assert(/行 1，列 7/.test(status), `状态栏光标异常: ${status}`);
});

await step("关闭标签回到空状态", async () => {
  await evaluate(`(() => {
    const close = document.querySelector('.tab-active .tab-close');
    close.click();
  })()`);
  const state = await waitFor("标签关闭", `({ ok: document.querySelectorAll('.tab').length === 0 })`);
  assert(state.ok, "标签未关闭");
});

ws.close();
fs.rmSync(scratchPath, { force: true });
console.log(`\n结果: ${passed} 通过, ${failed} 失败（scratch 文件已清理）`);
process.exit(failed === 0 ? 0 : 1);
