// 提交工作流冒烟测试（CDP 驱动 release 生产版）：暂存 → 提交 → amend → hooks 拒绝 → no-verify。
// 前置：以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 启动 fly-ide.exe。
// 用法：node scripts/dev/smoke-commit.mjs [repo-root]
// 未提供 repo-root 时在系统临时目录创建一个专用仓库（原生 Windows 路径，
// 不能用 Git Bash 的 /tmp——应用无法解析 MSYS 虚拟路径）。

import { setTimeout as sleep } from "node:timers/promises";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";

let repoRoot = process.argv[2];
if (!repoRoot) {
  repoRoot = fs.mkdtempSync(path.join(os.tmpdir(), "fly-smoke-commit-"));
  console.log(`临时仓库: ${repoRoot}`);
}
const cleanup = !process.argv[2];

function git(args) {
  return execFileSync("git", ["-C", repoRoot, ...args], { encoding: "utf8" });
}

const scratchName = "smoke-commit.txt";
const scratchPath = path.join(repoRoot, scratchName);

// 初始化临时仓库（初始提交）
git(["init", "-q", "-b", "main"]);
git(["config", "user.name", "fly"]);
git(["config", "user.email", "fly@example.com"]);
git(["config", "core.autocrlf", "false"]);
fs.writeFileSync(path.join(repoRoot, "README.md"), "readme\n");
git(["add", "-A"]);
git(["commit", "-qm", "init"]);

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

function uiExpr(extra) {
  return `(() => ({
    stagedRows: [...document.querySelectorAll('.git-row')].map((r) => r.textContent.trim().slice(0, 40)),
    commitValue: (document.querySelector('.commit-message') || {}).value ?? null,
    commitBtnDisabled: (document.querySelector('.commit-btn') || {}).disabled ?? null,
    output: (document.querySelector('.commit-output') || {}).textContent || null,
    meta: !!document.querySelector('.git-meta'),
    toast: (document.querySelector('.toast') || {}).textContent || null,
    errors: window.__errors || [],
    ...(${extra}),
  }))()`;
}

async function loadRepo() {
  await evaluate(`(() => {
    const gitTab = [...document.querySelectorAll('.activitybar button')].find((b) => b.textContent.trim() === 'Git');
    gitTab.click();
    return 'ok';
  })()`);
  await waitFor("git 输入框", `({ ok: !!document.querySelector('.git-toolbar input.git-input') })`);
  await evaluate(`(() => {
    const inp = document.querySelector('.git-toolbar input.git-input');
    inp.value = ${JSON.stringify(repoRoot)};
    inp.dispatchEvent(new Event('input', { bubbles: true }));
    document.querySelector('.git-toolbar .git-load').click();
  })()`);
  await waitFor(
    "git meta 或错误",
    `(() => ({
      ok: !!document.querySelector('.git-meta'),
      err: (document.querySelector('.git-error') || {}).textContent || null,
      toast: (document.querySelector('.toast') || {}).textContent || null,
    }))()`,
  );
}

async function typeMessage(message) {
  await evaluate(`(() => {
    const ta = document.querySelector('.commit-message');
    ta.value = ${JSON.stringify(message)};
    ta.dispatchEvent(new Event('input', { bubbles: true }));
  })()`);
}

async function setCheckbox(label, checked) {
  await evaluate(`(() => {
    const label = [...document.querySelectorAll('.commit-options label')].find((l) => l.textContent.trim().startsWith(${JSON.stringify(label)}));
    const box = label.querySelector('input');
    if (box.checked !== ${checked}) { box.click(); }
    return box.checked;
  })()`);
}

async function clickCommit() {
  // Svelte 的 disabled 属性在微任务 flush 时才落到 DOM；
  // 同步栈内"输入→点击"会点在仍禁用的按钮上（click 对 disabled 无效）
  await waitFor(
    "提交按钮可用",
    `({ ok: !!document.querySelector('.commit-btn') && document.querySelector('.commit-btn').disabled === false })`,
  );
  await evaluate(`(() => { document.querySelector('.commit-btn').click(); })()`);
}

await step("前端已挂载且无错误", async () => {
  const state = await evalJson(`({ ok: !!document.querySelector('.workbench'), errors: window.__errors || [] })`);
  assert(state.ok, "workbench 未渲染");
  assert(state.errors.length === 0, `前端错误: ${JSON.stringify(state.errors)}`);
});

await step("加载临时仓库", loadRepo);

await step("暂存新文件后 UI 自动出现（watcher）", async () => {
  fs.writeFileSync(scratchPath, "commit me\n");
  git(["add", scratchName]);
  await waitFor("staged 行", `({ ok: [...document.querySelectorAll('.git-row')].some((r) => r.textContent.includes(${JSON.stringify(scratchName)})) })`);
});

await step("填写消息并提交，状态清空且 HEAD 正确", async () => {
  await typeMessage("smoke: 提交测试");
  const ready = await evalJson(uiExpr(`{ ready: document.querySelector('.commit-btn').disabled === false }`));
  assert(ready.ready, "提交按钮应可用");
  await clickCommit();
  await waitFor("staged 清空", `({ ok: ![...document.querySelectorAll('.git-row')].some((r) => r.textContent.includes(${JSON.stringify(scratchName)})) })`);
  const subject = git(["log", "-1", "--pretty=%s"]).trim();
  assert(subject === "smoke: 提交测试", `HEAD 消息错误: ${subject}`);
});

await step("草稿随提交清空并持久化逻辑生效", async () => {
  const state = await evalJson(uiExpr(`{}`));
  assert(state.commitValue === "", `提交后消息应清空: ${state.commitValue}`);
});

await step("amend 修补提交", async () => {
  fs.writeFileSync(scratchPath, "commit me v2\n");
  git(["add", scratchName]);
  await waitFor("staged 行", `({ ok: [...document.querySelectorAll('.git-row')].some((r) => r.textContent.includes(${JSON.stringify(scratchName)})) })`);
  await setCheckbox("amend", true);
  await typeMessage("smoke: amended 提交");
  await clickCommit();
  await waitFor(
    "staged 清空",
    `(() => ({
      ok: ![...document.querySelectorAll('.git-row')].some((r) => r.textContent.includes(${JSON.stringify(scratchName)})),
      output: (document.querySelector('.commit-output') || {}).textContent || null,
      err: (document.querySelector('.git-error') || {}).textContent || null,
    }))()`,
  );
  const subject = git(["log", "-1", "--pretty=%s"]).trim();
  assert(subject === "smoke: amended 提交", `amend 后消息错误: ${subject}`);
  const count = git(["rev-list", "--count", "HEAD"]).trim();
  assert(count === "2", `amend 不应产生新提交，实际 ${count}`);
  await setCheckbox("amend", false);
});

await step("pre-commit hook 拒绝时输出可见，no-verify 可跳过", async () => {
  const hookPath = path.join(repoRoot, ".git", "hooks", "pre-commit");
  fs.writeFileSync(hookPath, "#!/bin/sh\necho 'hook says NO'\nexit 1\n");
  fs.writeFileSync(scratchPath, "commit me v3\n");
  git(["add", scratchName]);
  await waitFor("staged 行", `({ ok: [...document.querySelectorAll('.git-row')].some((r) => r.textContent.includes(${JSON.stringify(scratchName)})) })`);
  await typeMessage("smoke: blocked by hook");
  await clickCommit();
  const blocked = await waitFor("hook 输出", `({ ok: ((document.querySelector('.commit-output') || {}).textContent || '').includes('hook says NO') })`);
  assert(blocked.ok, "hooks 拒绝输出未展示");
  const count = git(["rev-list", "--count", "HEAD"]).trim();
  assert(count === "2", "被拒绝的提交不应产生新 HEAD");

  await setCheckbox("no-verify", true);
  await typeMessage("smoke: bypass hook");
  await clickCommit();
  await waitFor("staged 清空", `({ ok: ![...document.querySelectorAll('.git-row')].some((r) => r.textContent.includes(${JSON.stringify(scratchName)})) })`);
  const subject = git(["log", "-1", "--pretty=%s"]).trim();
  assert(subject === "smoke: bypass hook", `no-verify 提交失败: ${subject}`);
  fs.rmSync(hookPath, { force: true });
  await setCheckbox("no-verify", false);
});

await step("无消息时提交按钮禁用", async () => {
  await typeMessage("");
  const state = await evalJson(uiExpr(`{ disabled: document.querySelector('.commit-btn').disabled }`));
  assert(state.disabled === true, "空消息应禁用提交按钮");
});

ws.close();
fs.rmSync(scratchPath, { force: true });
if (cleanup && failed === 0) {
  fs.rmSync(repoRoot, { recursive: true, force: true });
} else if (cleanup) {
  console.log(`保留失败现场: ${repoRoot}`);
}
console.log(`\n结果: ${passed} 通过, ${failed} 失败`);
process.exit(failed === 0 ? 0 : 1);
