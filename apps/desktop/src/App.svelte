<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import DiffViewer from "./lib/editor/DiffViewer.svelte";
  import {
    applyHunks,
    discardFiles,
    loadFileDiff,
    loadGitNumstat,
    loadGitStatus,
    loadGitVersion,
    stageFiles,
    unstageFiles,
    type FileDiff,
    type GitStatus,
    type HunkAction,
    type NumStat,
    type StatusEntry,
  } from "./lib/git";

  type PanelId = "explorer" | "search" | "git" | "settings";

  const panels: { id: PanelId; label: string; title: string }[] = [
    { id: "explorer", label: "文件", title: "资源管理器" },
    { id: "search", label: "搜索", title: "全局搜索" },
    { id: "git", label: "Git", title: "源代码管理" },
    { id: "settings", label: "设置", title: "设置" },
  ];

  let activePanel = $state<PanelId>("git");
  let greetMsg = $state("");
  let appInfo = $state("读取中…");
  let marks = $state<string[]>([]);

  // ---- Git 面板状态（Phase 2 纵切片） ----
  let repoPath = $state("");
  let gitStatus = $state<GitStatus | null>(null);
  let gitError = $state("");
  let gitLoading = $state(false);
  let gitVersion = $state("");
  let numStaged = $state<NumStat[]>([]);
  let numWorktree = $state<NumStat[]>([]);

  // ---- Diff 视图状态 ----
  let selectedPath = $state("");
  let selectedStaged = $state(false);
  let fileDiff = $state<FileDiff | null>(null);
  let diffLoading = $state(false);
  let diffError = $state("");
  let pendingConfirm = $state("");

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

  const selectedEntry = $derived(
    gitStatus?.entries.find((entry) => entry.path === selectedPath) ?? null,
  );
  const canViewStaged = $derived(
    selectedEntry !== null && selectedEntry.x !== "." && !selectedEntry.untracked,
  );
  const canViewWorktree = $derived(
    selectedEntry !== null && (selectedEntry.y !== "." || selectedEntry.untracked),
  );

  function numstatFor(path: string, staged: boolean): NumStat | undefined {
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

  function defaultStagedFor(entry: StatusEntry | undefined): boolean {
    // 有暂存差异且无工作区差异 → 默认看暂存区 diff；否则默认工作区 diff
    return entry !== undefined && entry.x !== "." && entry.y === "." && !entry.untracked;
  }

  async function loadDiff(path: string, staged: boolean) {
    selectedPath = path;
    selectedStaged = staged;
    diffLoading = true;
    diffError = "";
    try {
      fileDiff = await loadFileDiff(repoPath.trim(), path, staged);
    } catch (err) {
      fileDiff = null;
      diffError = String(err);
    } finally {
      diffLoading = false;
    }
  }

  async function openDiff(path: string) {
    await loadDiff(path, defaultStagedFor(gitStatus?.entries.find((entry) => entry.path === path)));
  }

  function closeDiff() {
    fileDiff = null;
    selectedPath = "";
    diffError = "";
    pendingConfirm = "";
  }

  /// 操作后刷新状态；选中文件若已无变更则关闭 diff
  async function afterOp() {
    await refreshGit();
    if (!selectedPath) {
      return;
    }
    const stillDirty = gitStatus?.entries.some((entry) => entry.path === selectedPath);
    if (stillDirty) {
      await loadDiff(selectedPath, selectedStaged);
    } else {
      closeDiff();
    }
  }

  async function opFiles(op: "stage" | "unstage" | "discard") {
    diffError = "";
    try {
      const root = repoPath.trim();
      if (op === "stage") {
        await stageFiles(root, [selectedPath]);
      } else if (op === "unstage") {
        await unstageFiles(root, [selectedPath]);
      } else {
        await discardFiles(root, [selectedPath]);
      }
      await afterOp();
    } catch (err) {
      diffError = String(err);
    }
  }

  async function opHunks(action: HunkAction, index: number) {
    diffError = "";
    try {
      await applyHunks(repoPath.trim(), selectedPath, [index], action);
      await afterOp();
    } catch (err) {
      diffError = String(err);
    }
  }

  /// 丢弃操作的两段式确认：第一次点击进入待确认态，3 秒内再点执行
  function confirmThen(key: string, run: () => Promise<void>) {
    if (pendingConfirm === key) {
      pendingConfirm = "";
      void run();
    } else {
      pendingConfirm = key;
      setTimeout(() => {
        if (pendingConfirm === key) {
          pendingConfirm = "";
        }
      }, 3000);
    }
  }

  function hunkLabel(hunk: FileDiff["hunks"][number]): string {
    const section = hunk.section ? ` ${hunk.section}` : "";
    return `@@ -${hunk.oldStart},${hunk.oldLines} +${hunk.newStart},${hunk.newLines} @@${section}`;
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
    <span class="app-tag">Phase 0 · diff 视图</span>
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
                <button
                  type="button"
                  class="git-row"
                  class:git-row-active={entry.path === selectedPath}
                  title={entry.origPath ? `${entry.origPath} → ${entry.path}` : entry.path}
                  onclick={() => openDiff(entry.path)}
                >
                  <span class="git-xy">{entry.x}{entry.y}</span>
                  <span class="git-path">{entry.origPath ? `${entry.origPath} → ${entry.path}` : entry.path}</span>
                  <span class="git-numstat">
                    <span class="git-add">{numstatAdded(entry.path, group.staged)}</span>
                    <span class="git-del">{numstatDeleted(entry.path, group.staged)}</span>
                  </span>
                </button>
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
    {#if diffError}
      <p class="git-error diff-error">{diffError}</p>
    {/if}
    {#if fileDiff}
      <div class="diff-header">
        <span class="diff-path" title={fileDiff.path}>{fileDiff.path}</span>
        <span class="diff-source">
          <button
            type="button"
            class="diff-chip"
            class:diff-chip-on={!fileDiff.staged}
            disabled={!canViewWorktree}
            onclick={() => loadDiff(selectedPath, false)}
          >工作区</button>
          <button
            type="button"
            class="diff-chip"
            class:diff-chip-on={fileDiff.staged}
            disabled={!canViewStaged}
            onclick={() => loadDiff(selectedPath, true)}
          >暂存区</button>
        </span>
        <span class="diff-file-actions">
          {#if canViewWorktree}
            <button type="button" class="diff-btn" onclick={() => opFiles("stage")}>暂存全部</button>
          {/if}
          {#if canViewStaged}
            <button type="button" class="diff-btn" onclick={() => opFiles("unstage")}>取消暂存</button>
          {/if}
          {#if canViewWorktree}
            <button
              type="button"
              class="diff-btn diff-danger"
              onclick={() => confirmThen(`f:${selectedPath}`, () => opFiles("discard"))}
            >{pendingConfirm === `f:${selectedPath}` ? "确认丢弃？" : "丢弃"}</button>
          {/if}
          <button type="button" class="diff-btn" title="关闭 diff" onclick={closeDiff}>✕</button>
        </span>
      </div>
      <div class="diff-hunks">
        {#if fileDiff.binary}
          <span class="diff-hunk-note">二进制文件</span>
        {:else if fileDiff.hunks.length === 0}
          <span class="diff-hunk-note">无 hunk（{fileDiff.staged ? "暂存区" : "工作区"}无差异）</span>
        {:else}
          {#each fileDiff.hunks as hunk (hunk.index)}
            <span class="diff-hunk-chip">
              <span class="mono">{hunkLabel(hunk)}</span>
              {#if fileDiff.staged}
                <button type="button" class="diff-btn" onclick={() => opHunks("unstage", hunk.index)}>
                  取消暂存
                </button>
              {:else}
                <button type="button" class="diff-btn" onclick={() => opHunks("stage", hunk.index)}>
                  暂存
                </button>
                <button
                  type="button"
                  class="diff-btn diff-danger"
                  onclick={() => confirmThen(`h:${hunk.index}`, () => opHunks("discard", hunk.index))}
                >{pendingConfirm === `h:${hunk.index}` ? "确认？" : "丢弃"}</button>
              {/if}
            </span>
          {/each}
        {/if}
      </div>
      {#if diffError}
        <p class="git-error diff-error">{diffError}</p>
      {/if}
      <div class="diff-body">
        {#if diffLoading}
          <div class="diff-binary">加载中…</div>
        {:else}
          <DiffViewer
            oldText={fileDiff.oldText}
            newText={fileDiff.newText}
            binary={fileDiff.binary}
          />
        {/if}
      </div>
    {:else if diffLoading && selectedPath}
      <div class="welcome"><div class="diff-binary">加载 diff 中…</div></div>
    {:else}
      <div class="welcome">
        <h1>Fly IDE</h1>
        <p class="sub">轻量级 Windows IDE：AI 写代码，人来审阅。在 Git 面板点击文件查看 diff。</p>

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
    {/if}
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
