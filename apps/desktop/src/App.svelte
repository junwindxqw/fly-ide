<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import {
    loadGitNumstat,
    loadGitStatus,
    loadGitVersion,
    type GitStatus,
    type StatusEntry,
  } from "./lib/git";

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

  // ---- Git 面板状态（Phase 2 第 6 周纵切片） ----
  let repoPath = $state("");
  let gitStatus = $state<GitStatus | null>(null);
  let gitError = $state("");
  let gitLoading = $state(false);
  let gitVersion = $state("");
  let numStaged = $state<{ added: number | null; deleted: number | null; path: string }[]>([]);
  let numWorktree = $state<{ added: number | null; deleted: number | null; path: string }[]>([]);

  const stagedEntries = $derived(
    gitStatus?.entries.filter((entry) => !entry.untracked && !entry.unmerged && entry.x !== ".") ?? [],
  );
  const unstagedEntries = $derived(
    gitStatus?.entries.filter((entry) => !entry.untracked && !entry.unmerged && entry.y !== ".") ?? [],
  );
  const untrackedEntries = $derived(gitStatus?.entries.filter((entry) => entry.untracked) ?? []);
  const unmergedEntries = $derived(gitStatus?.entries.filter((entry) => entry.unmerged) ?? []);

  const gitGroups = $derived([
    { title: "暂存区", entries: stagedEntries, staged: true },
    { title: "工作区", entries: unstagedEntries, staged: false },
    { title: "未跟踪", entries: untrackedEntries, staged: false },
    { title: "冲突", entries: unmergedEntries, staged: false },
  ]);

  function entryLabel(entry: StatusEntry): string {
    return entry.origPath ? `${entry.origPath} → ${entry.path}` : entry.path;
  }

  function numstatFor(path: string, staged: boolean): { added: number | null; deleted: number | null } | undefined {
    return (staged ? numStaged : numWorktree).find((stat) => stat.path === path);
  }

  function numstatAdded(path: string, staged: boolean): string {
    const added = numstatFor(path, staged)?.added;
    return added === null || added === undefined ? "" : `+${added}`;
  }

  function numstatDeleted(path: string, staged: boolean): string {
    const deleted = numstatFor(path, staged)?.deleted;
    return deleted === null || deleted === undefined ? "" : `-${deleted}`;
  }

  async function refreshGit() {
    const root = repoPath.trim();
    if (!root) {
      gitError = "请输入 git 仓库根目录";
      return;
    }
    gitLoading = true;
    gitError = "";
    try {
      const status = await loadGitStatus(root);
      gitStatus = status;
      numWorktree = await loadGitNumstat(root, false);
      // 空仓库（无提交）没有 HEAD，--cached 会报错，直接置空
      numStaged = status.oid ? await loadGitNumstat(root, true) : [];
      if (!gitVersion) {
        gitVersion = await loadGitVersion().catch(() => "");
      }
    } catch (err) {
      gitStatus = null;
      gitError = String(err);
    } finally {
      gitLoading = false;
    }
  }

  function panelTitle(id: PanelId): string {
    return panels.find((panel) => panel.id === id)?.title ?? "";
  }

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
    {#if activePanel === "explorer"}
      <div class="panel-body">打开文件夹后在此显示文件树（Phase 1 实现）。</div>
    {:else if activePanel === "search"}
      <div class="panel-body">全局搜索面板（Phase 1 实现，Rust ripgrep）。</div>
    {:else if activePanel === "git"}
      <div class="git-toolbar">
        <input
          class="git-input"
          bind:value={repoPath}
          placeholder="仓库根目录，如 D:\myProject\fly_ide"
          aria-label="仓库根目录"
        />
        <button type="button" class="git-load" onclick={refreshGit} disabled={gitLoading}>
          {gitLoading ? "加载中…" : "加载"}
        </button>
      </div>
      {#if gitError}
        <p class="git-error">{gitError}</p>
      {:else if gitStatus}
        <p class="git-meta">
          <span class="git-branch">{gitStatus.branch ?? "HEAD (detached)"}</span>
          {#if gitStatus.ahead !== null || gitStatus.behind !== null}
            <span>↑{gitStatus.ahead ?? 0} ↓{gitStatus.behind ?? 0}</span>
          {/if}
          {#if gitVersion}<span class="git-ver">git {gitVersion}</span>{/if}
        </p>
        {#each gitGroups as group (group.title)}
          <div class="git-group">
            <div class="git-group-title">{group.title}（{group.entries.length}）</div>
            {#if group.entries.length === 0}
              <div class="git-empty">无</div>
            {:else}
              {#each group.entries as entry (entry.path)}
                <div class="git-row" title={entryLabel(entry)}>
                  <span class="git-xy">{entry.x}{entry.y}</span>
                  <span class="git-path">{entryLabel(entry)}</span>
                  <span class="git-numstat">
                    <span class="git-add">{numstatAdded(entry.path, group.staged)}</span>
                    <span class="git-del">{numstatDeleted(entry.path, group.staged)}</span>
                  </span>
                </div>
              {/each}
            {/if}
          </div>
        {/each}
      {:else}
        <div class="panel-body">输入 git 仓库根目录后加载状态。</div>
      {/if}
    {:else}
      <div class="panel-body">设置面板（Phase 1 实现）。</div>
    {/if}
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
    <span>分支: {gitStatus?.branch ?? "—"}</span>
    <span>问题: 0</span>
    <span class="spacer"></span>
    <span>行 1，列 1</span>
    <span>UTF-8</span>
    <span>LF</span>
  </footer>
</div>
