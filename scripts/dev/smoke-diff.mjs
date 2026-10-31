// Git diff 工作流冒烟测试（CDP 驱动 release 生产版）。
// 前置：以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 启动 fly-ide.exe，
// 并预先在工作区制造一个 README.md 的两处远距离修改；测试结束时通过 UI 的“丢弃”恢复，
// 仓库应回到干净状态。
// 用法：node scripts/dev/smoke-diff.mjs <仓库根目录>
// 注意：在受限/沙箱环境下每个 git 子进程 spawn 可能慢至 ~3s（实测 ZCode Bash 沙箱），
// 因此所有断言用轮询（60s 上限），真实机器上几百 ms 即通过。

import { setTimeout as sleep } from "node:timers/promises";

const repoRoot = process.argv[2];
if (!repoRoot) {
  console.error("usage: node scripts/dev/smoke-diff.mjs <repo-root>");
  process.exit(2);
}

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
  return new Promise((resolve, reject) => {
    pending.set(id, (message) => {
      if (message.error) {
        reject(new Error(message.error.message));
      } else {
        resolve(message.result?.result?.value);
      }
    });
    ws.send(JSON.stringify({ id, method: "Runtime.evaluate", params: { expression, returnByValue: true } }));
  });
}

const evalJson = async (expression) => JSON.parse(await evaluate(`JSON.stringify(${expression})`));

const sleepMs = (ms) => sleep(ms);

/** 轮询直到 getter 返回真值；超时抛错并附带最后一次快照。
 *  默认 60s：沙箱环境下单次 git spawn ~3s，一次 diff 操作链可达 ~15s。 */
async function waitFor(name, getterExpr, { timeoutMs = 60000, intervalMs = 500 } = {}) {
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
    await sleepMs(intervalMs);
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

const rowWithXy = (expected) => `(() => {
  const row = [...document.querySelectorAll('button.git-row')].find((b) => b.textContent.includes('README.md'));
  const xy = row ? row.querySelector('.git-xy').textContent : null;
  return { ok: xy === ${JSON.stringify(expected)}, xy };
})()`;

// ---- 步骤 ----

await step("前端已挂载且无错误", async () => {
  const state = await evalJson(
    `({ ok: !!document.querySelector('.workbench'), errors: window.__errors || [] })`,
  );
  assert(state.ok, "workbench 未渲染");
  assert(state.errors.length === 0, `前端错误: ${JSON.stringify(state.errors)}`);
});

await step("加载仓库状态", async () => {
  // 默认面板是文件树，先切到 Git 面板
  await evaluate(`(() => {
    const gitTab = [...document.querySelectorAll('.activitybar button')].find((b) => b.textContent.trim() === 'Git');
    if (!gitTab) throw new Error('找不到 Git 面板按钮');
    gitTab.click();
  })()`);
  await evaluate(`(() => {
    const inp = document.querySelector('.git-toolbar input.git-input');
    inp.value = ${JSON.stringify(repoRoot)};
    inp.dispatchEvent(new Event('input'));
    document.querySelector('.git-toolbar .git-load').click();
  })()`);
  await waitFor("git meta", `({ ok: !!document.querySelector('.git-meta') })`);
});

await step("预热：首次 git_diff_file（沙箱冷启动可能极慢）", async () => {
  // 首次调用偶发被环境拒绝（空错误对象），重试吸收；失败仅告警不阻断
  let last = null;
  for (let attempt = 1; attempt <= 3; attempt++) {
    last = await evaluate(
      `window.__TAURI_INTERNALS__.invoke('git_diff_file', { root: ${JSON.stringify(repoRoot)}, path: 'README.md', staged: false }).then(r => JSON.stringify({ ok: true, hunks: r.hunks.length })).catch(e => JSON.stringify({ ok: false, err: String(e) }))`,
    );
    if (typeof last === "string" && last.includes('"ok":true')) {
      console.log("      预热完成");
      return;
    }
    await sleepMs(2000);
  }
  console.warn(`      WARN 预热未完成（后续步骤自带 90s 轮询，可吸收冷启动），最后返回: ${JSON.stringify(last)}`);
});

await step("点击 README.md 打开 diff 视图", async () => {
  await evaluate(`(() => {
    const row = [...document.querySelectorAll('button.git-row')].find((b) => b.textContent.includes('README.md'));
    if (!row) throw new Error('README.md 不在工作区改动列表');
    row.click();
  })()`);
  const state = await waitFor(
    "CM6 diff 视图",
    `({
      ok: !!document.querySelector('.diff-host .cm-editor'),
      error: (document.querySelector('.diff-error') || {}).textContent || null,
      chips: document.querySelectorAll('.diff-hunk-chip').length,
      hostHTMLLength: (document.querySelector('.diff-host') || {}).innerHTML?.length ?? -1,
    })`,
  );
  assert(state.chips >= 1, "hunk 列表为空");
});

await step("hunk 暂存后文件变为 MM", async () => {
  await evaluate(`document.querySelector('.diff-hunk-chip .diff-btn').click()`);
  const row = await waitFor("README xy=MM", rowWithXy("MM"));
  assert(row.xy === "MM", `期望 MM，实际 ${row.xy}`);
});

await step("切换暂存区视图并取消暂存，回到 .M", async () => {
  await evaluate(`(() => {
    const chip = [...document.querySelectorAll('.diff-chip')].find((b) => b.textContent.trim() === '暂存区');
    if (!chip) throw new Error('找不到暂存区切换按钮');
    chip.click();
  })()`);
  // 等暂存区视图真正渲染出取消暂存按钮（加载期间 chips 为空，不能盲点）
  await waitFor(
    "暂存区视图渲染",
    `({ ok: [...document.querySelectorAll('.diff-hunk-chip .diff-btn')].some((b) => b.textContent.trim() === '取消暂存') })`,
  );
  // 沙箱下偶发点击不触发/命令极慢：点击后轮询 UI 状态（DOM 轮询，不发起新 invoke），最多重试 3 次
  for (let attempt = 1; attempt <= 3; attempt++) {
    await evaluate(`(() => {
      const btn = [...document.querySelectorAll('.diff-hunk-chip .diff-btn')].find((b) => b.textContent.trim() === '取消暂存');
      if (!btn) throw new Error('取消暂存按钮消失');
      btn.click();
    })()`);
    const settled = await waitFor(
      `取消暂存生效(第${attempt}次)`,
      `(() => {
        const row = [...document.querySelectorAll('button.git-row')].find((b) => b.textContent.includes('README.md'));
        const xy = row ? row.querySelector('.git-xy').textContent : null;
        return {
          ok: xy === '.M',
          xy,
          err: (document.querySelector('.diff-error') || {}).textContent || null,
        };
      })()`,
      { timeoutMs: 90000, intervalMs: 1500 },
    ).catch(() => null);
    if (settled && settled.ok) {
      break;
    }
    if (attempt === 3) {
      throw new Error(`取消暂存三次未生效，最后状态: ${JSON.stringify(settled)}`);
    }
    await sleepMs(3000);
  }
  const row = await waitFor("README xy=.M", rowWithXy(".M"));
  assert(row.xy === ".M", `期望 .M，实际 ${row.xy}`);
});

await step("两段式确认丢弃后仓库恢复干净", async () => {
  await evaluate(`(() => {
    const btn = [...document.querySelectorAll('.diff-file-actions .diff-danger')].find((b) => b.textContent.startsWith('丢弃'));
    if (!btn) throw new Error('找不到丢弃按钮');
    btn.click();
  })()`);
  await waitFor("确认态", `({ ok: [...document.querySelectorAll('.diff-file-actions .diff-danger')].some((b) => b.textContent.startsWith('确认')) })`);
  await evaluate(`(() => {
    const btn = [...document.querySelectorAll('.diff-file-actions .diff-danger')].find((b) => b.textContent.startsWith('确认'));
    btn.click();
  })()`);
  const state = await waitFor("丢弃完成", `({
    ok: !document.querySelector('.diff-host') && ![...document.querySelectorAll('button.git-row')].some((b) => b.textContent.includes('README.md')),
    errors: window.__errors || [],
  })`, { timeoutMs: 90000 });
  assert(state.errors.length === 0, `前端错误: ${JSON.stringify(state.errors)}`);
});

ws.close();
console.log(`\n结果: ${passed} 通过, ${failed} 失败`);
process.exit(failed === 0 ? 0 : 1);
