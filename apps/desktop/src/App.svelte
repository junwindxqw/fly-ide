<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  type PanelId = "explorer" | "search" | "git" | "settings";

  const panels: { id: PanelId; label: string; title: string }[] = [
    { id: "explorer", label: "文件", title: "资源管理器" },
    { id: "search", label: "搜索", title: "全局搜索" },
    { id: "git", label: "Git", title: "源代码管理" },
    { id: "settings", label: "设置", title: "设置" },
  ];

  let activePanel = $state<PanelId>("explorer");
  let greetMsg = $state("");
  let appInfo = $state("读取中…");
  let marks = $state<string[]>([]);

  onMount(() => {
    void mark("ui.mounted");
    invoke<string>("app_info")
      .then((info) => {
        appInfo = info;
      })
      .catch((err) => {
        appInfo = `调用失败：${String(err)}`;
      });
  });

  async function mark(name: string) {
    try {
      await invoke("mark", { name });
      marks = [...marks, name];
    } catch {
      // 启动标记失败不影响使用
    }
  }

  async function greet() {
    greetMsg = await invoke<string>("greet", { name: "Fly IDE" });
  }

  function panelTitle(id: PanelId): string {
    return panels.find((p) => p.id === id)?.title ?? "";
  }
</script>

<div class="workbench">
  <header class="titlebar">
    <span class="app-name">Fly IDE</span>
    <span class="app-tag">Phase 0 · 脚手架</span>
  </header>

  <nav class="activitybar" aria-label="活动栏">
    {#each panels as panel (panel.id)}
      <button
        type="button"
        aria-pressed={activePanel === panel.id}
        title={panel.title}
        onclick={() => (activePanel = panel.id)}
      >
        {panel.label}
      </button>
    {/each}
  </nav>

  <aside class="sidebar">
    <div class="panel-title">{panelTitle(activePanel)}</div>
    <div class="panel-body">
      {#if activePanel === "explorer"}
        打开文件夹后在此显示文件树（Phase 1 实现）。
      {:else if activePanel === "search"}
        全局搜索面板（Phase 1 实现，Rust ripgrep）。
      {:else if activePanel === "git"}
        源代码管理面板（Phase 2 实现，核心模块）。
      {:else}
        设置面板（Phase 1 实现）。
      {/if}
    </div>
  </aside>

  <main class="editor-area">
    <div class="welcome">
      <h1>Fly IDE</h1>
      <p class="sub">轻量级 Windows IDE：AI 写代码，人来审阅。</p>

      <section class="card">
        <h2>IPC 冒烟测试</h2>
        <div class="row">
          <button type="button" class="primary" onclick={greet}>调用 Rust greet</button>
          <span class="mono">{greetMsg || "（未调用）"}</span>
        </div>
        <p class="hint">Rust 侧返回：<span class="mono">{appInfo}</span></p>
      </section>

      <section class="card">
        <h2>启动标记（Phase 0 性能基线）</h2>
        <p class="hint">
          每次关键节点调用 Rust <code>mark()</code>，追加写入
          <code>%LOCALAPPDATA%\FlyIDE\logs\startup.jsonl</code>。
        </p>
        {#if marks.length > 0}
          <ul class="marks">
            {#each marks as m (m)}
              <li>{m}</li>
            {/each}
          </ul>
        {/if}
      </section>
    </div>
  </main>

  <footer class="statusbar">
    <span>分支: —</span>
    <span>问题: 0</span>
    <span class="spacer"></span>
    <span>行 1，列 1</span>
    <span>UTF-8</span>
    <span>LF</span>
  </footer>
</div>
