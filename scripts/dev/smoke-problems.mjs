// Problems/Outline 冒烟测试（CDP 驱动 release 生产版）：
// 打开文件 → LSP 会话 → 诊断出现（Problems 面板 + 状态栏计数）→ Outline 符号。
// 前置：以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" 启动 fly-ide.exe。
// 用法：node scripts/dev/smoke-problems.mjs <工作区根目录>
// 约定：被打开的工作区必须是独立测试工作区（fly-test-ws），勿用 fly_ide 开发仓库。

import { setTimeout as sleep } from "node:timers/promises";
import fs from "node:fs";
import path from "node:path";

const repoRoot = process.argv[2];
if (!repoRoot) {
  console.error("usage: node scripts/dev/smoke-problems.mjs <workspace-root>");
  process.exit(2);
}

const sampleName = "index.js";
const samplePath = path.join(repoRoot, "frontend-demo", sampleName);
const initialContent = fs.readFileSync(samplePath, "utf8");
fs.writeFileSync(samplePath, "// Problems 冒烟样例\nconst value = 1;\n// sample line\n");

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

async function waitFor(name, getterExpr, { timeoutMs = 90000, intervalMs = 1000 } = {}) {
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

/** 点击 dirName 目录并等待 childSuffix 出现（树 DOM 会被 watcher 刷新重建，带点击重试）。 */
async function expandDirAndAwait(dirName, childSuffix) {
  for (let attempt = 1; attempt <= 4; attempt++) {
    await evaluate(`(() => {
      const rows = [...document.querySelectorAll('.tree-name')];
      const idx = rows.findIndex((b) => b.textContent.trim().endsWith(${JSON.stringify(dirName)}));
      if (idx < 0) throw new Error('找不到目录 ${dirName}');
      rows[idx].click();
    })()`);
    const appeared = await waitFor(
      `${dirName} 下出现 ${childSuffix}`,
      `(() => {
        const rows = [...document.querySelectorAll('.tree-name')].map((b) => b.textContent.trim());
        const idx = rows.findIndex((r) => r.endsWith(${JSON.stringify(dirName)}));
        if (idx < 0) throw new Error('目录行消失');
        return { ok: rows.slice(idx + 1).some((r) => r.endsWith(${JSON.stringify(childSuffix)})) };
      })()`,
      { timeoutMs: 20000, intervalMs: 800 },
    ).catch(() => null);
    if (appeared && appeared.ok) return;
    if (attempt === 4) throw new Error(`展开 ${dirName} 失败`);
    await sleep(2000);
  }
}

async function openFile(fileName) {
  for (let attempt = 1; attempt <= 4; attempt++) {
    await evaluate(`(() => {
      const row = [...document.querySelectorAll('.tree-name')].find((b) => b.textContent.trim().endsWith(${JSON.stringify(fileName)}));
      if (!row) throw new Error('找不到 ${fileName}');
      row.click();
    })()`);
    const switched = await waitFor(
      `活动标签 ${fileName}`,
      `({ ok: (document.querySelector('.tab-active .tab-name') || {}).textContent?.endsWith(${JSON.stringify(fileName)}) })`,
      { timeoutMs: 15000, intervalMs: 600 },
    ).catch(() => null);
    if (switched && switched.ok) return;
    if (attempt === 4) throw new Error(`打开 ${fileName} 失败`);
    await sleep(2000);
  }
}

await step("前端已挂载且无错误", async () => {
  const state = await evalJson(`({ ok: !!document.querySelector('.workbench'), errors: window.__errors || [] })`);
  assert(state.ok, "workbench 未渲染");
  assert(state.errors.length === 0, `前端错误: ${JSON.stringify(state.errors)}`);
});

await step("文件树加载并打开 index.js", async () => {
  await evaluate(`(() => {
    const inp = document.querySelector('.sidebar input.git-input');
    inp.value = ${JSON.stringify(repoRoot)};
    inp.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitFor("文件树行", `({ ok: document.querySelectorAll('.tree-row').length > 0 })`);
  await expandDirAndAwait("frontend-demo", "index.js");
  await openFile("index.js");
});

await step("LSP 会话建立", async () => {
  const state = await waitFor("LSP 状态", `({ ok: [...document.querySelectorAll('.statusbar span')].some((s) => s.textContent === 'LSP: mock') })`);
  assert(state.ok, "状态栏无 LSP 标记");
});

await step("诊断写入状态（状态栏问题计数）", async () => {
  const state = await waitFor(
    "问题计数",
    `({ ok: [...document.querySelectorAll('.problems-toggle')].some((b) => /问题: [1-9]/.test(b.textContent)) })`,
    { timeoutMs: 60000 },
  );
  assert(state.ok, "状态栏问题计数未出现");
});

await step("Problems 面板展示诊断", async () => {
  await evaluate(`(() => {
    const btn = document.querySelector('.problems-toggle');
    btn.click();
  })()`);
  const state = await waitFor(
    "诊断行",
    `({ ok: document.querySelectorAll('.problems-row').length > 0 })`,
    { timeoutMs: 30000 },
  );
  assert(state.ok, "Problems 面板无诊断行");
  const text = await evalJson(`JSON.stringify((document.querySelector('.problems-msg') || {}).textContent || '')`);
  assert(text.includes("mock diagnostic"), `诊断内容异常: ${text}`);
});

await step("Outline 面板展示符号", async () => {
  await evaluate(`(() => {
    const tab = [...document.querySelectorAll('.activitybar button')].find((b) => b.textContent.trim() === '大纲');
    if (!tab) throw new Error('找不到大纲按钮');
    tab.click();
  })()`);
  const state = await waitFor(
    "大纲行",
    `({ ok: document.querySelectorAll('.outline-row').length >= 2 })`,
    { timeoutMs: 30000 },
  );
  assert(state.ok, "Outline 无符号行");
  const names = await evalJson(`[...document.querySelectorAll('.outline-name')].map((n) => n.textContent.trim())`);
  assert(
    Array.isArray(names) &&
      names.includes("main") &&
      names.includes("MOCK_SYMBOL_CONST"),
    `符号名异常: ${JSON.stringify(names)}`,
  );
});

ws.close();
// 恢复样例文件初始内容（测试工作区是常驻资产）
fs.writeFileSync(samplePath, initialContent);
console.log(`\n结果: ${passed} 通过, ${failed} 失败（样例文件已恢复）`);
process.exit(failed === 0 ? 0 : 1);
