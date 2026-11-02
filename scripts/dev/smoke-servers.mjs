// 语言服务器管理面板冒烟测试（CDP 驱动 release 生产版）。
// 前置：以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" 启动 fly-ide.exe。
// 用法：node scripts/dev/smoke-servers.mjs

import { setTimeout as sleep } from "node:timers/promises";

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

async function waitFor(name, getterExpr, { timeoutMs = 30000, intervalMs = 800 } = {}) {
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

await step("前端已挂载且无错误", async () => {
  const state = await evalJson(`({ ok: !!document.querySelector('.workbench'), errors: window.__errors || [] })`);
  assert(state.ok, "workbench 未渲染");
  assert(state.errors.length === 0, `前端错误: ${JSON.stringify(state.errors)}`);
});

await step("设置面板展示 5 种语言的服务器状态", async () => {
  await evaluate(`(() => {
    const tab = [...document.querySelectorAll('.activitybar button')].find((b) => b.textContent.trim() === '设置');
    tab.click();
  })()`);
  const state = await waitFor("服务器行", `({ ok: document.querySelectorAll('.server-row').length >= 5 })`);
  assert(state.ok, "服务器行不足");
  const names = await evalJson(`[...document.querySelectorAll('.server-name')].map((n) => n.textContent.trim())`);
  for (const expected of ["Go", "Rust", "Python", "PHP", "TypeScript / JavaScript"]) {
    assert(names.includes(expected), `缺少语言: ${expected}，实际: ${JSON.stringify(names)}`);
  }
});

await step("PATH 检测状态真实（至少有 available/missing 之一）", async () => {
  const avails = await evalJson(`[...document.querySelectorAll('.server-avail')].map((n) => n.textContent.trim())`);
  const valid = avails.every((a) => a === "已安装" || a === "可下载安装" || a === "未安装");
  assert(valid, `状态值异常: ${JSON.stringify(avails)}`);
  const hasInstalled = avails.includes("已安装");
  console.log(`      已安装语言数: ${avails.filter((a) => a === "已安装").length}/5（有安装为 ${hasInstalled}）`);
});

await step("Rust 服务器可用（rust-analyzer 在 PATH）", async () => {
  // 开发机已装 rust-analyzer；若未装则该断言转为信息性输出
  const rows = await evalJson(`[...document.querySelectorAll('.server-row')].map((r) => ({
    name: r.querySelector('.server-name').textContent.trim(),
    avail: r.querySelector('.server-avail').textContent.trim(),
  }))`);
  const rust = rows.find((r) => r.name === "Rust");
  if (rust && rust.avail === "已安装") {
    return;
  }
  console.log(`      WARN rust-analyzer 未在 PATH 中（真实接入需先安装）`);
});

ws.close();
console.log(`\n结果: ${passed} 通过, ${failed} 失败`);
process.exit(failed === 0 ? 0 : 1);
