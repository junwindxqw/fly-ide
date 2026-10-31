// 快速打开 / 全局搜索 / 文件监听 冒烟测试（CDP 驱动 release 生产版）。
// 前置：以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 启动 fly-ide.exe。
// 用法：node scripts/dev/smoke-search.mjs <工作区根目录>
// 注意：沙箱环境每命令 ~3s，全部断言轮询（90s 上限）。

import { setTimeout as sleep } from "node:timers/promises";
import fs from "node:fs";
import path from "node:path";

const repoRoot = process.argv[2];
if (!repoRoot) {
  console.error("usage: node scripts/dev/smoke-search.mjs <workspace-root>");
  process.exit(2);
}

const scratchPath = path.join(repoRoot, "scratch-search.txt");
const scratchName = "scratch-search.txt";
const scratchBody = ["line1", "NEEDLE alpha", "line3", "NEEDLE beta", "line5"].join("\n") + "\n";
fs.writeFileSync(scratchPath, scratchBody);

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

async function loadWorkspace() {
  await evaluate(`(() => {
    const explorerTab = [...document.querySelectorAll('.activitybar button')].find((b) => b.textContent.trim() === '文件');
    explorerTab.click();
    const inp = document.querySelector('.sidebar input.git-input');
    inp.value = ${JSON.stringify(repoRoot)};
    inp.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitFor("文件树行", `({ ok: document.querySelectorAll('.tree-row').length > 0 })`);
}

const uiExpr = `(() => ({
  quickOpen: !!document.querySelector('.quickopen'),
  quickOpenItems: [...document.querySelectorAll('.quickopen-item')].map((b) => b.textContent.trim()),
  editorOpen: !!document.querySelector('.editor-host .cm-editor'),
  editorText: (() => { const cm = document.querySelector('.editor-host .cm-content'); return cm ? cm.textContent : null; })(),
  activeTab: (document.querySelector('.tab-active .tab-name') || {}).textContent || null,
  status: [...document.querySelectorAll('.statusbar span')].map((s) => s.textContent),
  searchRows: document.querySelectorAll('.search-row').length,
  gitRows: [...document.querySelectorAll('.git-row')].map((r) => r.textContent.trim().slice(0, 40)),
  errors: window.__errors || [],
}))()`;

await step("前端已挂载且无错误", async () => {
  const state = await evalJson(`({ ok: !!document.querySelector('.workbench'), errors: window.__errors || [] })`);
  assert(state.ok, "workbench 未渲染");
  assert(state.errors.length === 0, `前端错误: ${JSON.stringify(state.errors)}`);
});

await step("文件树加载工作区", loadWorkspace);

await step("Ctrl+P 打开快速打开并搜索到 scratch 文件", async () => {
  await evaluate(`(() => {
    // 冒烟可能残留上一次的弹窗状态：先读状态再条件切换
    if (!document.querySelector('.quickopen')) {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'p', ctrlKey: true, bubbles: true }));
    }
    return 'ok';
  })()`);
  await waitFor("quickopen 出现", `({ ok: !!document.querySelector('.quickopen-input') })`);
  await evaluate(`(() => {
    const inp = document.querySelector('.quickopen-input');
    inp.value = 'scratch';
    inp.dispatchEvent(new Event('input', { bubbles: true }));
  })()`);
  const state = await waitFor(
    "quickopen 结果",
    `({ ok: [...document.querySelectorAll('.quickopen-item')].some((b) => b.textContent.includes(${JSON.stringify(scratchName)})) })`,
  );
  assert(state.ok, "结果中无 scratch-search.txt");
});

await step("Enter 打开文件到编辑器", async () => {
  await evaluate(`(() => {
    const inp = document.querySelector('.quickopen-input');
    inp.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  })()`);
  const state = await waitFor("编辑器打开", `({ ok: !!document.querySelector('.editor-host .cm-editor') })`);
  assert(state.ok, "编辑器未渲染");
  const tab = (await evalJson(uiExpr)).activeTab;
  assert(tab === scratchName, `活动标签错误: ${tab}`);
});

await step("搜索面板找到 NEEDLE 并跳转到行", async () => {
  await evaluate(`(() => {
    const searchTab = [...document.querySelectorAll('.activitybar button')].find((b) => b.textContent.trim() === '搜索');
    if (!searchTab) throw new Error('找不到搜索面板按钮');
    searchTab.click();
  })()`);
  await waitFor("搜索输入框", `({ ok: !!document.querySelector('.search-panel input.git-input') })`);
  await evaluate(`(() => {
    const inp = document.querySelector('.search-panel input.git-input');
    inp.value = 'NEEDLE alpha';
    inp.dispatchEvent(new Event('input', { bubbles: true }));
    inp.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  })()`);
  await waitFor("搜索结果", `({ ok: document.querySelectorAll('.search-row').length >= 1 })`);
  await evaluate(`(() => {
    document.querySelector('.search-row').click();
  })()`);
  const state = await waitFor("跳转到行", `(() => {
    const status = [...document.querySelectorAll('.statusbar span')].map((s) => s.textContent).join('|');
    return { ok: status.includes('行 2') && !!document.querySelector('.editor-host .cm-editor'), status };
  })()`);
  assert(state.ok, `状态栏未显示行 2: ${state.status}`);
});

await step("外部修改自动重载干净文档", async () => {
  fs.appendFileSync(scratchPath, "EXTERNAL-CHANGE\n");
  const state = await waitFor(
    "编辑器重载",
    `({ ok: (document.querySelector('.editor-host .cm-content') || {}).textContent?.includes('EXTERNAL-CHANGE') })`,
    { timeoutMs: 90000, intervalMs: 1000 },
  );
  assert(state.ok, "编辑器未自动重载外部修改");
});

await step("外部修改触发 git 状态自动刷新", async () => {
  // 在 git 面板加载仓库后，外部改动应自动出现在 changes 列表
  await evaluate(`(() => {
    const gitTab = [...document.querySelectorAll('.activitybar button')].find((b) => b.textContent.trim() === 'Git');
    if (!gitTab) throw new Error('找不到 Git 面板按钮');
    gitTab.click();
    return 'ok';
  })()`);
  await waitFor("git 输入框", `({ ok: !!document.querySelector('.git-toolbar input.git-input') })`);
  await evaluate(`(() => {
    const inp = document.querySelector('.git-toolbar input.git-input');
    inp.value = ${JSON.stringify(repoRoot)};
    inp.dispatchEvent(new Event('input', { bubbles: true }));
    document.querySelector('.git-toolbar .git-load').click();
    return 'ok';
  })()`);
  await waitFor(
    "git meta",
    `(() => ({
      ok: !!document.querySelector('.git-meta'),
      panel: (document.querySelector('.panel-title') || {}).textContent || null,
      err: (document.querySelector('.git-error') || {}).textContent || null,
      toast: (document.querySelector('.toast') || {}).textContent || null,
    }))()`,
    { timeoutMs: 90000, intervalMs: 1500 },
  );
  // 外部再改一次，触发 watcher → git 自动刷新
  fs.appendFileSync(scratchPath, "EXTERNAL-CHANGE-2\n");
  await waitFor(
    "git 刷新出现 scratch",
    `({ ok: [...document.querySelectorAll('.git-row')].some((r) => r.textContent.includes(${JSON.stringify(scratchName)})) })`,
    { timeoutMs: 90000, intervalMs: 1500 },
  );
});

ws.close();
fs.rmSync(scratchPath, { force: true });
console.log(`\n结果: ${passed} 通过, ${failed} 失败（scratch 文件已清理）`);
process.exit(failed === 0 ? 0 : 1);
