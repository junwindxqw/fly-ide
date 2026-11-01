// 语法高亮冒烟测试（CDP 驱动 release 生产版）：
// 打开多语言文件验证 Lezer 高亮生效（以 .txt 纯文本为对照）。
// 前置：以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" 启动 fly-ide.exe。
// 用法：node scripts/dev/smoke-syntax.mjs <工作区根目录>
// 约定：被打开的工作区必须是独立测试工作区（fly-test-ws），勿用 fly_ide 开发仓库。

import { setTimeout as sleep } from "node:timers/promises";
import fs from "node:fs";
import path from "node:path";

const repoRoot = process.argv[2];
if (!repoRoot) {
  console.error("usage: node scripts/dev/smoke-syntax.mjs <workspace-root>");
  process.exit(2);
}

const plainName = "plain-sample.txt";
const plainPath = path.join(repoRoot, plainName);
const plainBody = Array.from({ length: 8 }, (_, i) => `plain line ${i + 1} with some words`).join("\n") + "\n";
fs.writeFileSync(plainPath, plainBody);

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

async function loadTree() {
  await evaluate(`(() => {
    const inp = document.querySelector('.sidebar input.git-input');
    inp.value = ${JSON.stringify(repoRoot)};
    inp.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitFor("文件树行", `({ ok: document.querySelectorAll('.tree-row').length > 0 })`);
}

// 树行按钮 textContent 带 caret 前缀，必须用 endsWith 匹配
async function expandDir(dirName) {
  await evaluate(`(() => {
    const dir = [...document.querySelectorAll('.tree-name')].find((b) => b.textContent.trim().endsWith(${JSON.stringify(dirName)}));
    if (!dir) throw new Error('找不到目录 ${dirName}');
    dir.click();
  })()`);
}

/** 点击 dirName 目录并等待其后代 childSuffix 行出现（按 DOM 顺序定位，避免误匹配同级同名目录）。
 *  树 DOM 会被 watcher 触发的刷新随时重建（点击可能落在死节点上），因此带点击重试。 */
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
    if (appeared && appeared.ok) {
      return;
    }
    if (attempt === 4) {
      throw new Error(`展开 ${dirName} 四次未出现 ${childSuffix}`);
    }
    await sleep(2000);
  }
}

async function openFile(fileName, parentDir) {
  // 树 DOM 会被 watcher 刷新随时重建：先确保目标行存在（必要时展开父目录），点击带重试
  for (let attempt = 1; attempt <= 4; attempt++) {
    const ensure = await evaluate(`(() => {
      const rows = [...document.querySelectorAll('.tree-name')];
      let row = rows.find((b) => b.textContent.trim().endsWith(${JSON.stringify(fileName)}));
      if (!row && ${JSON.stringify(parentDir ?? null)}) {
        const dir = rows.find((b) => b.textContent.trim().endsWith(${JSON.stringify(parentDir)}));
        if (dir) { dir.click(); return 'expanding'; }
      }
      if (!row) return 'no-row';
      row.click();
      return 'clicked';
    })()`);
    if (ensure === 'expanding') {
      await waitFor(
        `${parentDir} 下出现 ${fileName}`,
        `({ ok: [...document.querySelectorAll('.tree-name')].some((b) => b.textContent.trim().endsWith(${JSON.stringify(fileName)})) })`,
        { timeoutMs: 20000, intervalMs: 800 },
      );
    }
    const switched = await waitFor(
      `活动标签 ${fileName}`,
      `({ ok: (document.querySelector('.tab-active .tab-name') || {}).textContent?.endsWith(${JSON.stringify(fileName)}) })`,
      { timeoutMs: 15000, intervalMs: 600 },
    ).catch(() => null);
    if (switched && switched.ok) {
      break;
    }
    if (attempt === 4) {
      throw new Error(`打开 ${fileName} 四次未切换标签`);
    }
    await sleep(2000);
  }
  await sleep(500); // 语法包动态加载
}

await step("前端已挂载且无错误", async () => {
  const state = await evalJson(`({ ok: !!document.querySelector('.workbench'), errors: window.__errors || [] })`);
  assert(state.ok, "workbench 未渲染");
  assert(state.errors.length === 0, `前端错误: ${JSON.stringify(state.errors)}`);
});

await step("文件树加载", loadTree);

let txtLength = 0;
await step("打开纯文本（对照）", async () => {
  // 根层文件由 watcher 刷新后出现
  await waitFor("plain.txt 行", `({ ok: [...document.querySelectorAll('.tree-name')].some((b) => b.textContent.trim().endsWith('${plainName}')) })`);
  await openFile(plainName);
  txtLength = (await contentLength()).length;
  assert(txtLength > 0, "纯文本内容为空");
});

await step("Rust 文件高亮显著强于纯文本", async () => {
  // rust-demo 下有同名 src 目录歧义：锚定 rust-demo 行后点击其 src，等待 main.rs 出现
  for (let attempt = 1; attempt <= 4; attempt++) {
    await evaluate(`(() => {
      const all = [...document.querySelectorAll('.tree-name')];
      const rustIdx = all.findIndex((b) => b.textContent.trim().endsWith('rust-demo'));
      if (rustIdx < 0) throw new Error('找不到 rust-demo');
      const srcIdx = all.slice(rustIdx + 1).findIndex((b) => b.textContent.trim().endsWith('src'));
      if (srcIdx < 0) { all[rustIdx].click(); return; }
      all[rustIdx + 1 + srcIdx].click();
    })()`);
    const appeared = await waitFor(
      "main.rs 行",
      `({ ok: [...document.querySelectorAll('.tree-name')].some((b) => b.textContent.trim().endsWith('main.rs')) })`,
      { timeoutMs: 20000, intervalMs: 800 },
    ).catch(() => null);
    if (appeared && appeared.ok) {
      break;
    }
    if (attempt === 4) {
      throw new Error("展开 rust-demo/src 四次未出现 main.rs");
    }
    await sleep(2000);
  }
  await openFile("main.rs");
  const rsLength = (await contentLength()).length;
  // 8 行的 main.rs tokenize 后每 token 一个 span，innerHTML 应显著大于纯文本
  assert(rsLength > txtLength * 1.5, `Rust 高亮不足：rs=${rsLength} txt=${txtLength}`);
});

await step("多语言文件打开无错误（懒加载各语法包）", async () => {
  for (const [dir, file] of [
    ["golang-demo", "main.go"],
    ["python-demo", "main.py"],
    ["php-demo", "index.php"],
  ]) {
    await expandDirAndAwait(dir, file);
    await openFile(file, dir);
  }
  // frontend-demo 已展开，直接打开
  await openFile("index.js", "frontend-demo");
  const state = await evalJson(`({ ok: (window.__errors || []).length === 0, errors: window.__errors || [] })`);
  assert(state.ok, `前端错误: ${JSON.stringify(state.errors)}`);
});

function contentLength() {
  return evalJson(`({ ok: true, length: (document.querySelector('.editor-host .cm-content') || {}).innerHTML?.length ?? 0 })`);
}

ws.close();
fs.rmSync(plainPath, { force: true });
console.log(`\n结果: ${passed} 通过, ${failed} 失败（plain.txt 已清理）`);
process.exit(failed === 0 ? 0 : 1);
