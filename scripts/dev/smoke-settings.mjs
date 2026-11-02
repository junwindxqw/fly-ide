// 设置界面冒烟测试（CDP 驱动 release 生产版）：
// 打开设置面板 → 表单渲染 → 修改主题/字号 → 保存 → 重新加载验证持久化。
// 前置：以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222" 启动 fly-ide.exe。
// 用法：node scripts/dev/smoke-settings.mjs

import { setTimeout as sleep } from "node:timers/promises";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const settingsFile = path.join(os.homedir(), "AppData", "Roaming", "FlyIDE", "settings.json");
const backup = fs.existsSync(settingsFile) ? fs.readFileSync(settingsFile, "utf8") : null;

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

await step("设置面板渲染（表单控件）", async () => {
  await evaluate(`(() => {
    const tab = [...document.querySelectorAll('.activitybar button')].find((b) => b.textContent.trim() === '设置');
    tab.click();
  })()`);
  const state = await waitFor("设置表单", `({ ok: document.querySelectorAll('.settings-row').length >= 4 })`);
  assert(state.ok, "设置表单未渲染");
});

await step("修改主题并保存，磁盘文件更新", async () => {
  await evaluate(`(() => {
    const select = document.querySelector('.settings-row select');
    select.value = 'light';
    select.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await sleep(500);
  await evaluate(`(() => {
    document.querySelector('.settings-save').click();
  })()`);
  const deadline = Date.now() + 20000;
  for (;;) {
    if (fs.existsSync(settingsFile)) {
      const text = fs.readFileSync(settingsFile, "utf8");
      if (text.includes('"theme": "light"')) {
        break;
      }
    }
    if (Date.now() > deadline) {
      throw new Error("设置文件未包含 light 主题");
    }
    await sleep(600);
  }
});

await step("重新加载设置验证持久化", async () => {
  // 切走再切回触发重新加载
  await evaluate(`(() => {
    const fileTab = [...document.querySelectorAll('.activitybar button')].find((b) => b.textContent.trim() === '文件');
    fileTab.click();
  })()`);
  await sleep(600);
  await evaluate(`(() => {
    const settingsTab = [...document.querySelectorAll('.activitybar button')].find((b) => b.textContent.trim() === '设置');
    settingsTab.click();
  })()`);
  const state = await waitFor(
    "主题回显",
    `(() => {
      const select = document.querySelector('.settings-row select');
      return { ok: select && select.value === 'light', value: select ? select.value : null };
    })()`,
  );
  assert(state.ok, `主题未持久化: ${JSON.stringify(state)}`);
});

// 恢复原设置
if (backup !== null) {
  fs.writeFileSync(settingsFile, backup);
} else {
  fs.rmSync(settingsFile, { force: true });
}
ws.close();
console.log(`\n结果: ${passed} 通过, ${failed} 失败（设置文件已恢复）`);
process.exit(failed === 0 ? 0 : 1);
