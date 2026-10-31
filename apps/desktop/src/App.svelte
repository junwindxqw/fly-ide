<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import DiffViewer from "./lib/editor/DiffViewer.svelte";
  import EditorPane from "./lib/editor/EditorPane.svelte";
  import { cursorInfo, createDoc, isDirty, type DocEntry } from "./lib/editor/doc";
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
  import { fsReadText, fsWriteText, pickFolder } from "./lib/fs";
  import QuickOpen from "./lib/search/QuickOpen.svelte";
  import SearchPanel from "./lib/search/SearchPanel.svelte";
  import { watchStart, watchStop, type FsChangedPayload } from "./lib/search";
  import FileTree from "./lib/workspace/FileTree.svelte";

  type PanelId = "explorer" | "search" | "git" | "settings";

  const panels: { id: PanelId; label: string; title: string }[] = [
    { id: "explorer", label: "文件", title: "资源管理器" },
    { id: "search", label: "搜索", title: "全局搜索" },
    { id: "git", label: "Git", title: "源代码管理" },
    { id: "settings", label: "设置", title: "设置" },
  ];

  let activePanel = $state<PanelId>("explorer");
  let marks = $state<string[]>([]);

  // ---- 工作区（文件树与 Git 面板共享的已提交路径） ----
  let workspacePath = $state("");
  let gitPathDraft = $state("");
  let toast = $state("");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  // ---- Git 面板状态（Phase 2 纵切片） ----
  let gitStatus = $state<GitStatus | null>(null);
  let gitError = $state("");
  let gitLoading = $state(false);
  let gitVersion = $state("");
  let numStaged = $state<NumStat[]>([]);
  let numWorktree = $state<NumStat[]>([]);

  // ---- Diff 视图状态 ----
  let diffPath = $state("");
  let selectedStaged = $state(false);
  let fileDiff = $state<FileDiff | null>(null);
  let diffLoading = $state(false);
  let diffError = $state("");
  let pendingConfirm = $state("");

  // ---- 编辑器文档状态 ----
  let docs = $state<DocEntry[]>([]);
  let activePath = $state("");
  let cursor = $state({ line: 1, col: 1 });

  // ---- 快速打开 / 文件监听 ----
  let quickOpenOpen = $state(false);
  let treeVersion = $state(0);
  let gitRefreshTimer: ReturnType<typeof setTimeout> | undefined;
  let unlistenFsChanged: (() => void) | null = null;

  const activeDoc = $derived(docs.find((doc) => doc.path === activePath) ?? null);

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

  const diffOpen = $derived(fileDiff !== null);
  const selectedEntry = $derived(
    gitStatus?.entries.find((entry) => entry.path === diffPath) ?? null,
  );
  const canViewStaged = $derived(
    selectedEntry !== null && selectedEntry.x !== "." && !selectedEntry.untracked,
  );
  const canViewWorktree = $derived(
    selectedEntry !== null && (selectedEntry.y !== "." || selectedEntry.untracked),
  );

  function showToast(message: string) {
    toast = message;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toast = "";
    }, 4000);
  }

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
    const root = gitPathDraft.trim();
    if (!root) {
      gitError = "请输入 git 仓库根目录";
      return;
    }
    workspacePath = root;
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
    diffPath = path;
    selectedStaged = staged;
    diffLoading = true;
    diffError = "";
    try {
      fileDiff = await loadFileDiff(workspacePath, path, staged);
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
    diffPath = "";
    diffError = "";
    pendingConfirm = "";
  }

  /// 操作后刷新状态；diff 文件若已无变更则关闭 diff
  async function afterOp() {
    if (gitPathDraft.trim()) {
      await refreshGit();
    }
    if (!diffPath) {
      return;
    }
    const stillDirty = gitStatus?.entries.some((entry) => entry.path === diffPath);
    if (stillDirty) {
      await loadDiff(diffPath, selectedStaged);
    } else {
      closeDiff();
    }
  }

  async function opFiles(op: "stage" | "unstage" | "discard") {
    diffError = "";
    try {
      if (op === "stage") {
        await stageFiles(workspacePath, [diffPath]);
      } else if (op === "unstage") {
        await unstageFiles(workspacePath, [diffPath]);
      } else {
        await discardFiles(workspacePath, [diffPath]);
      }
      await afterOp();
    } catch (err) {
      diffError = String(err);
    }
  }

  async function opHunks(action: HunkAction, index: number) {
    diffError = "";
    try {
      await applyHunks(workspacePath, diffPath, [index], action);
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

  // ---- 编辑器文档 ----

  /** 稳定回调：CM6 updateListener 在打开文档时被捕获进 state 扩展，
   *  必须保证身份稳定；$state 读取走信号，总是拿到最新值。 */
  function handleDocChanged(path: string, state: import("@codemirror/state").EditorState) {
    docs = docs.map((doc) => (doc.path === path ? { ...doc, state } : doc));
    if (path === activePath) {
      cursor = cursorInfo(state);
    }
  }

  async function openTextFile(path: string, targetLine?: number) {
    const existing = docs.find((doc) => doc.path === path);
    if (existing) {
      activateDoc(path, targetLine);
      return;
    }
    try {
      const file = await fsReadText(path);
      if (file.binary) {
        showToast("二进制文件，暂不支持查看");
        return;
      }
      docs = [...docs, createDoc(path, file, handleDocChanged, targetLine)];
      activateDoc(path);
    } catch (err) {
      showToast(String(err));
    }
  }

  function activateDoc(path: string, targetLine?: number) {
    activePath = path;
    fileDiff = null;
    diffPath = "";
    if (targetLine === undefined) {
      const doc = docs.find((entry) => entry.path === path);
      if (doc) {
        cursor = cursorInfo(doc.state);
      }
      return;
    }
    // 带行号打开：重建 state 使光标落到目标行
    const doc = docs.find((entry) => entry.path === path);
    if (!doc) {
      return;
    }
    const file = {
      text: doc.state.doc.toString(),
      encoding: doc.encoding,
      bom: doc.bom,
      eol: doc.eol,
      size: 0,
      tier: doc.tier,
      binary: false,
    };
    docs = docs.map((entry) =>
      entry.path === path ? createDoc(path, file, handleDocChanged, targetLine) : entry,
    );
    const updated = docs.find((entry) => entry.path === path);
    if (updated) {
      cursor = cursorInfo(updated.state);
    }
  }

  function closeDoc(path: string) {
    const index = docs.findIndex((doc) => doc.path === path);
    if (index < 0) {
      return;
    }
    docs = docs.filter((doc) => doc.path !== path);
    if (activePath === path) {
      const next = docs[index] ?? docs[index - 1];
      activePath = next?.path ?? "";
      const doc = next ? cursorInfo(next.state) : null;
      if (doc) {
        cursor = doc;
      }
    }
  }

  async function saveDoc(path: string) {
    const doc = docs.find((entry) => entry.path === path);
    if (!doc) {
      return;
    }
    if (doc.readOnly) {
      showToast("只读文件（大文件降级），不保存");
      return;
    }
    const text = doc.state.doc.toString();
    try {
      await fsWriteText(path, text, doc.eol, doc.encoding, doc.bom);
      docs = docs.map((entry) =>
        entry.path === path ? { ...entry, originalText: text } : entry,
      );
      showToast(`已保存 ${doc.name}`);
    } catch (err) {
      showToast(`保存失败：${String(err)}`);
    }
  }

  /// 外部文件变更：重载干净的打开文档、防抖刷新 git 状态与文件树
  function handleFsChanged(paths: string[]) {
    if (paths.length === 0) {
      return;
    }
    void (async () => {
      let notified = false;
      for (const doc of docs) {
        if (!paths.includes(doc.path)) {
          continue;
        }
        try {
          const file = await fsReadText(doc.path);
          if (file.binary) {
            continue;
          }
          const current = doc.state.doc.toString();
          if (file.text === current) {
            continue; // 自己刚保存的内容，跳过以保留撤销历史
          }
          if (isDirty(doc)) {
            if (!notified) {
              notified = true;
              showToast("有文件已被外部修改，未覆盖未保存的编辑");
            }
            continue;
          }
          const line = cursorInfo(doc.state).line;
          docs = docs.map((entry) =>
            entry.path === doc.path ? createDoc(doc.path, file, handleDocChanged, line) : entry,
          );
        } catch {
          // 文件可能已被删除，文件树刷新即可
        }
      }
    })();
    if (gitStatus) {
      clearTimeout(gitRefreshTimer);
      gitRefreshTimer = setTimeout(() => void refreshGit(), 1200);
    }
    treeVersion += 1;
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

  onMount(() => {
    void mark("ui.mounted");
    void listen<FsChangedPayload>("fs-changed", (event) => {
      handleFsChanged(event.payload.paths ?? []);
    }).then((unlisten) => {
      unlistenFsChanged = unlisten;
    });
    const onKeyDown = (event: KeyboardEvent) => {
      const ctrl = event.ctrlKey || event.metaKey;
      if (ctrl && event.key.toLowerCase() === "p") {
        event.preventDefault();
        quickOpenOpen = !quickOpenOpen;
        return;
      }
      if (ctrl && event.key.toLowerCase() === "s") {
        event.preventDefault();
        if (activePath) {
          void saveDoc(activePath);
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      unlistenFsChanged?.();
    };
  });

  // 工作区变化时启停文件监听
  $effect(() => {
    const root = workspacePath.trim();
    if (!root) {
      return;
    }
    void watchStart(root).catch((err) => showToast(String(err)));
    return () => {
      void watchStop().catch(() => {});
    };
  });
</script>

<div class="workbench">
  <header class="titlebar">
    <span class="app-name">Fly IDE</span>
    <span class="app-tag">Phase 1 · 编辑器</span>
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
      <div class="git-toolbar">
        <input
          class="git-input"
          value={workspacePath}
          placeholder="工作区文件夹路径"
          aria-label="工作区路径"
          onchange={(event) => (workspacePath = event.currentTarget.value.trim())}
        />
        <button
          type="button"
          class="git-load"
          title="选择文件夹"
          onclick={async () => {
            const dir = await pickFolder();
            if (dir) {
              workspacePath = dir;
            }
          }}
        >…</button>
      </div>
      <FileTree root={workspacePath} activePath={activePath} version={treeVersion} onOpenFile={openTextFile} />
    {:else if activePanel === "search"}
      <SearchPanel root={workspacePath} onOpenFile={openTextFile} />
    {:else if activePanel === "git"}
      <div class="git-toolbar">
        <input
          class="git-input"
          bind:value={gitPathDraft}
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
                  class:git-row-active={entry.path === diffPath}
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
      <div class="panel-body">设置面板（后续实现）。</div>
    {/if}
  </aside>

  <main class="editor-area">
    {#if diffError}
      <p class="git-error diff-error">{diffError}</p>
    {/if}
    {#if diffOpen}
      <div class="diff-header">
        <span class="diff-path" title={fileDiff?.path}>{fileDiff?.path}</span>
        <span class="diff-source">
          <button
            type="button"
            class="diff-chip"
            class:diff-chip-on={!fileDiff?.staged}
            disabled={!canViewWorktree}
            onclick={() => loadDiff(diffPath, false)}
          >工作区</button>
          <button
            type="button"
            class="diff-chip"
            class:diff-chip-on={fileDiff?.staged}
            disabled={!canViewStaged}
            onclick={() => loadDiff(diffPath, true)}
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
              onclick={() => confirmThen(`f:${diffPath}`, () => opFiles("discard"))}
            >{pendingConfirm === `f:${diffPath}` ? "确认丢弃？" : "丢弃"}</button>
          {/if}
          <button type="button" class="diff-btn" title="关闭 diff" onclick={closeDiff}>✕</button>
        </span>
      </div>
      <div class="diff-hunks">
        {#if fileDiff?.binary}
          <span class="diff-hunk-note">二进制文件</span>
        {:else if (fileDiff?.hunks.length ?? 0) === 0}
          <span class="diff-hunk-note">无 hunk（{fileDiff?.staged ? "暂存区" : "工作区"}无差异）</span>
        {:else}
          {#each fileDiff?.hunks ?? [] as hunk (hunk.index)}
            <span class="diff-hunk-chip">
              <span class="mono">{hunkLabel(hunk)}</span>
              {#if fileDiff?.staged}
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
      <div class="diff-body">
        {#if diffLoading}
          <div class="diff-binary">加载中…</div>
        {:else}
          <DiffViewer
            oldText={fileDiff?.oldText ?? ""}
            newText={fileDiff?.newText ?? ""}
            binary={fileDiff?.binary ?? false}
          />
        {/if}
      </div>
    {:else if docs.length > 0}
      <div class="tabbar" role="tablist">
        {#each docs as doc (doc.path)}
          <div
            class="tab"
            class:tab-active={doc.path === activePath}
            role="tab"
            aria-selected={doc.path === activePath}
            tabindex="0"
            onclick={() => activateDoc(doc.path)}
            onkeydown={(event) => {
              if (event.key === "Enter") {
                activateDoc(doc.path);
              }
            }}
          >
            <span class="tab-dirty" class:tab-clean={!isDirty(doc)}>{isDirty(doc) ? "●" : ""}</span>
            <span class="tab-name" title={doc.path}>{doc.name}{doc.readOnly ? "（只读）" : ""}</span>
            <button
              type="button"
              class="tab-close"
              title="关闭"
              onclick={(event) => {
                event.stopPropagation();
                closeDoc(doc.path);
              }}
            >✕</button>
          </div>
        {/each}
      </div>
      <div class="editor-pane">
        {#if activeDoc}
          <EditorPane editorState={activeDoc.state} />
        {:else}
          <div class="welcome"><div class="diff-binary">无打开的文件</div></div>
        {/if}
      </div>
    {:else}
      <div class="welcome">
        <h1>Fly IDE</h1>
        <p class="sub">轻量级 Windows IDE：AI 写代码，人来审阅。</p>
        <section class="card">
          <h2>开始</h2>
          <ul class="marks">
            <li>文件面板：选择文件夹浏览，点击文件打开编辑；Ctrl+S 保存</li>
            <li>Ctrl+P：快速打开文件（模糊搜索）</li>
            <li>搜索面板：全局内容搜索（ripgrep），点击结果跳转到行</li>
            <li>Git 面板：加载仓库后点击改动文件查看 diff，可按 hunk 暂存</li>
          </ul>
        </section>
      </div>
    {/if}
  </main>

  <footer class="statusbar">
    <span>分支: {gitStatus?.branch ?? "—"}</span>
    <span>问题: 0</span>
    <span class="spacer"></span>
    {#if activeDoc}
      <span>行 {cursor.line}，列 {cursor.col}</span>
      <span>{activeDoc.encoding}{activeDoc.bom ? " BOM" : ""}</span>
      <span>{activeDoc.eol}</span>
    {:else}
      <span>行 1，列 1</span>
      <span>UTF-8</span>
      <span>LF</span>
    {/if}
  </footer>

  {#if toast}
    <div class="toast">{toast}</div>
  {/if}

  {#if quickOpenOpen}
    <QuickOpen
      root={workspacePath}
      onOpenFile={openTextFile}
      onClose={() => (quickOpenOpen = false)}
    />
  {/if}
</div>
