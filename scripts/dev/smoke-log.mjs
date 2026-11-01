// Git 历史/blame 冒烟测试（CDP 驱动 release 生产版）：
// 加载仓库 → 切历史视图 → 提交列表 → 点击显示详情 → 打开文件开 blame 装饰。
// 前置：以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" 启动 fly-ide.exe。
// 用法：node scripts/dev/smoke-log.mjs <仓库根目录>
// 建议传独立测试工作区（fly-test-ws），勿用 fly_ide 开发仓库。
// 注意：工作区需有至少一次提交；脚本会额外创建两个测试提交并在结束后
// 通过 git reset --hard <原HEAD> 恢复。

import { setTimeout as sleep } from "node:timers/promises";
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";

const repoRoot = process.argv[2];
if (!repoRoot) {
  console.error("usage: node scripts/dev/smoke-log.mjs <repo-root>");
  process.exit(2);
}

function git(args) {
  return execFileSync("git", ["-C", repoRoot, ...args], { encoding: "utf8" });
}

const originalHead = git(["rev-parse", "HEAD"]).trim();
const smokeFile = "log-smoke.txt";

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

// 两个测试提交
fs.writeFileSync(path.join(repoRoot, smokeFile), "history v1\n");
git(["add", smokeFile]);
git(["commit", "-m", "log-smoke: 提交一"]);
fs.writeFileSync(path.join(repoRoot, smokeFile), "history v2\n");
git(["add", smokeFile]);
git(["commit", "-m", "log-smoke: 提交二"]);
const newHead = git(["rev-parse", "HEAD"]).trim();

await step("前端已挂载且无错误", async () => {
  const state = await evalJson(`({ ok: !!document.querySelector('.workbench'), errors: window.__errors || [] })`);
  assert(state.ok, "workbench 未渲染");
  assert(state.errors.length === 0, `前端错误: ${JSON.stringify(state.errors)}`);
});

await step("加载仓库并切历史视图", async () => {
  for (let attempt = 1; attempt <= 2; attempt++) {
    try {
      await evaluate(`(() => {
        const gitTab = [...document.querySelectorAll('.activitybar button')].find((b) => b.textContent.trim() === 'Git');
        gitTab.click();
        const inp = document.querySelector('.git-toolbar input.git-input');
        inp.value = ${JSON.stringify(repoRoot)};
        inp.dispatchEvent(new Event('input', { bubbles: true }));
        document.querySelector('.git-toolbar .git-load').click();
      })()`);
      await waitFor("git meta", `({ ok: !!document.querySelector('.git-meta') })`);
      break;
    } catch (error) {
      if (attempt === 2) throw error;
      console.log(`      WARN 加载失败，重试`);
      await sleep(3000);
    }
  }
  await evaluate(`(() => {
    const tab = [...document.querySelectorAll('.git-view-tab')].find((b) => b.textContent.trim() === '历史');
    if (tab) { tab.click(); }
  })()`);
  await waitFor("历史列表", `({ ok: document.querySelectorAll('.log-row').length > 0 })`);
});

await step("提交列表包含测试提交（新在前）", async () => {
  const state = await evalJson(`(() => ({
    subjects: [...document.querySelectorAll('.log-subject')].map((n) => n.textContent.trim()),
  }))()`);
  const idxOne = state.subjects.indexOf("log-smoke: 提交一");
  const idxTwo = state.subjects.indexOf("log-smoke: 提交二");
  assert(idxOne >= 0, "未找到提交一");
  assert(idxTwo >= 0, "未找到提交二");
  assert(idxTwo < idxOne, "新提交应在前");
});

await step("点击提交显示变更摘要", async () => {
  await evaluate(`(() => {
    const row = [...document.querySelectorAll('.log-row')].find((r) => r.textContent.includes('log-smoke: 提交一'));
    row.click();
  })()`);
  const state = await waitFor(
    "提交详情",
    `({ ok: ((document.querySelector('.commit-output') || {}).textContent || '').includes('log-smoke.txt') })`,
    { timeoutMs: 30000 },
  );
  assert(state.ok, "提交详情未包含文件名");
});

await step("打开文件开 blame 装饰", async () => {
  await evaluate(`(() => {
    const explorerTab = [...document.querySelectorAll('.activitybar button')].find((b) => b.textContent.trim() === '文件');
    explorerTab.click();
    const inp = document.querySelector('.sidebar input.git-input');
    inp.value = ${JSON.stringify(repoRoot)};
    inp.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitFor("文件树行", `({ ok: document.querySelectorAll('.tree-row').length > 0 })`);
  await evaluate(`(() => {
    const row = [...document.querySelectorAll('.tree-name')].find((b) => b.textContent.trim().endsWith(${JSON.stringify(smokeFile)}));
    if (!row) throw new Error('找不到 ${smokeFile}');
    row.click();
  })()`);
  const editorReady = await waitFor("编辑器", `({ ok: !!document.querySelector('.editor-host .cm-editor') })`);
  assert(editorReady.ok, "编辑器未渲染");
  await evaluate(`(() => {
    const btn = [...document.querySelectorAll('.blame-toggle')].find((b) => b.textContent.includes('blame'));
    btn.click();
  })()`);
  const blameOn = await waitFor(
    "blame 装饰",
    `({ ok: document.querySelectorAll('.blame-widget').length > 0 })`,
    { timeoutMs: 30000 },
  );
  assert(blameOn.ok, "blame 装饰未出现");
  const author = git(["config", "user.name"]).trim();
  const text = await evalJson(`JSON.stringify((document.querySelector('.blame-widget') || {}).textContent || '')`);
  assert(text.includes(author), `blame 应含作者 ${author}: ${text}`);
});

// 恢复仓库到冒烟前状态
git(["reset", "--hard", originalHead]);
git(["clean", "-fd"]);
ws.close();
console.log(`\n结果: ${passed} 通过, ${failed} 失败（仓库已恢复到 ${originalHead.slice(0, 7)}）`);
process.exit(failed === 0 ? 0 : 1);
