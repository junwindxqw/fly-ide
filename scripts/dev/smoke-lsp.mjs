// LSP 链路冒烟测试（CDP 驱动 release 生产版）：
// 打开文件 → mock LSP 自动启动（进程内会话）→ didOpen → 诊断装饰 → 补全弹窗。
// 前置：以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" 启动 fly-ide.exe。
// 用法：node scripts/dev/smoke-lsp.mjs <工作区根目录>
// 约定（用户要求）：被打开的工作区必须是独立测试工作区（如 D:/myProject/fly-test-ws），
// 禁止对 fly_ide 开发仓库本身冒烟。
// LSP mock 会话为进程内会话（@mock）：不写日志文件，断言全部基于 UI 证据。

import { setTimeout as sleep } from "node:timers/promises";
import fs from "node:fs";
import path from "node:path";

const repoRoot = process.argv[2];
if (!repoRoot) {
  console.error("usage: node scripts/dev/smoke-lsp.mjs <workspace-root>（请传独立测试工作区）");
  process.exit(2);
}

const sampleName = "index.js";
const samplePath = path.join(repoRoot, "frontend-demo", sampleName);
const initialContent = fs.readFileSync(samplePath, "utf8");
fs.writeFileSync(samplePath, "// LSP 冒烟样例\nconst value = 1;\n// sample line\n");

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

const uiExpr = `(() => ({
  editor: !!document.querySelector('.editor-host .cm-editor'),
  lintRange: document.querySelectorAll('.cm-lintRange').length,
  completion: [...document.querySelectorAll('.cm-completionLabel')].map((n) => n.textContent.trim()),
  lspStatus: [...document.querySelectorAll('.statusbar span')].map((s) => s.textContent).join('|'),
  toast: (document.querySelector('.toast') || {}).textContent || null,
  errors: window.__errors || [],
}))()`;

await step("前端已挂载且无错误", async () => {
  const state = await evalJson(`({ ok: !!document.querySelector('.workbench'), errors: window.__errors || [] })`);
  assert(state.ok, "workbench 未渲染");
  assert(state.errors.length === 0, `前端错误: ${JSON.stringify(state.errors)}`);
});

await step("文件树加载并打开 frontend-demo/index.js", async () => {
  await evaluate(`(() => {
    const inp = document.querySelector('.sidebar input.git-input');
    inp.value = ${JSON.stringify(repoRoot)};
    inp.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitFor("文件树行", `({ ok: document.querySelectorAll('.tree-row').length > 0 })`);
  // 展开 frontend-demo 目录后再点击样例文件
  // 注意：.tree-name 的 textContent 带 caret 前缀（▸），必须用 endsWith 匹配
  await evaluate(`(() => {
    const dir = [...document.querySelectorAll('.tree-name')].find((b) => b.textContent.trim().endsWith('frontend-demo'));
    if (!dir) throw new Error('找不到 frontend-demo 目录');
    dir.click();
  })()`);
  await waitFor("index.js 树行", `({ ok: [...document.querySelectorAll('.tree-name')].some((b) => b.textContent.trim().endsWith('index.js')) })`);
  await evaluate(`(() => {
    const row = [...document.querySelectorAll('.tree-name')].find((b) => b.textContent.trim().endsWith('index.js'));
    row.click();
  })()`);
  const state = await waitFor("编辑器", `({ ok: !!document.querySelector('.editor-host .cm-editor') })`);
  assert(state.ok, "编辑器未渲染");
});

await step("LSP 会话建立（initialize 往返成功）", async () => {
  const state = await waitFor("LSP 状态", `({ ok: [...document.querySelectorAll('.statusbar span')].some((s) => s.textContent === 'LSP: mock') })`);
  assert(state.ok, "状态栏无 LSP 标记");
});

await step("didOpen 后服务器诊断渲染为编辑器装饰", async () => {
  // 诊断由服务器在收到 didOpen 后推送——出现即证明完整链路通；
  // @codemirror/lint 的行内标记类为 .cm-lintRange（tooltip 才是 .cm-diagnostic）
  const state = await waitFor(
    "诊断装饰",
    `({ ok: document.querySelectorAll('.cm-lintRange').length > 0 })`,
    { timeoutMs: 60000 },
  );
  assert(state.ok, "诊断装饰未出现");
});

await step("输入触发 mock 补全弹窗", async () => {
  await evaluate(`(() => {
    const content = document.querySelector('.editor-host .cm-content');
    content.focus();
    const sel = window.getSelection();
    if (sel) { sel.selectAllChildren(content); sel.collapseToEnd(); }
    document.execCommand('insertText', false, '\\nx.');
  })()`);
  const state = await waitFor(
    "补全弹窗",
    `(() => {
      const items = [...document.querySelectorAll('.cm-completionLabel')].map((n) => n.textContent.trim());
      return { ok: items.some((t) => t.startsWith('MOCK_COMPLETION')), items };
    })()`,
    { timeoutMs: 60000 },
  );
  assert(state.ok, `补全弹窗无 mock 项: ${JSON.stringify(state.items)}`);
});

const finalUi = await evalJson(uiExpr);
ws.close();
// 恢复样例文件初始内容（测试工作区是常驻资产，不可破坏）
fs.writeFileSync(samplePath, initialContent);
console.log(`\n结果: ${passed} 通过, ${failed} 失败`);
console.log(`最终 UI: ${JSON.stringify(finalUi)}`);
process.exit(failed === 0 ? 0 : 1);
