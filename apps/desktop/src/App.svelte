<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import DiffViewer from "./lib/editor/DiffViewer.svelte";
  import EditorPane from "./lib/editor/EditorPane.svelte";
  import { cursorInfo, createDoc, isDirty, type DocEntry } from "./lib/editor/doc";
  import { loadSyntaxFor } from "./lib/editor/syntax";
  import {
    applyHunks,
    commitFiles,
    discardFiles,
    gitNetwork,
    loadBranchInfo,
    loadFileDiff,
    loadGitBlame,
    loadGitLog,
    loadGitNumstat,
    loadGitStatus,
    loadGitVersion,
    stageFiles,
    unstageFiles,
    type BlameLine,
    type BranchInfo,
    type CommitInfo,
    type FileDiff,
    type GitStatus,
    type HunkAction,
    type NumStat,
    type StatusEntry,
  } from "./lib/git";
  import { blameExtension, blameMapOf } from "./lib/editor/blame";
  import {
    allDiagnostics,
    getOutline,
    requestOutline,
    type DiagnosticEntry,
  } from "./lib/lsp/diagnostics.svelte";
  import { fsReadText, fsWriteText, pickFolder } from "./lib/fs";
  import {
    ensureMockLsp,
    languageIdForPath,
    lspExtensionsFor,
    pathToFileUri,
    type MockLspSession,
  } from "./lib/lsp/lsp";
  import QuickOpen from "./lib/search/QuickOpen.svelte";
  import SearchPanel from "./lib/search/SearchPanel.svelte";
  import SettingsPanel from "./lib/settings/SettingsPanel.svelte";
  import { watchStart, watchStop, type FsChangedPayload } from "./lib/search";
  import FileTree from "./lib/workspace/FileTree.svelte";

  type PanelId = "explorer" | "search" | "git" | "outline" | "settings";

  const panels: { id: PanelId; label: string; title: string }[] = [
    { id: "explorer", label: "文件", title: "资源管理器" },
    { id: "search", label: "搜索", title: "全局搜索" },
    { id: "git", label: "Git", title: "源代码管理" },
    { id: "outline", label: "大纲", title: "大纲" },
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

  // ---- 提交框状态 ----
  let commitMessage = $state("");
  let commitAmend = $state(false);
  let commitSignoff = $state(false);
  let commitNoVerify = $state(false);
  let commitOutput = $state("");
  let committing = $state(false);

  // ---- 历史视图 / blame ----
  let gitView = $state<"changes" | "history">("changes");
  let gitLog = $state<CommitInfo[]>([]);
  let logLoading = $state(false);
  let logError = $state("");
  let blameOn = $state(false);
  let blameLoading = $state(false);

  // ---- 远程操作 ----
  let branchInfo = $state<BranchInfo | null>(null);
  let netRunning = $state(false);
  let netOutput = $state("");

  // ---- Problems / Outline 面板 ----
  let bottomPanelOpen = $state(false);
  let diagnosticsVersion = $state(0);
  let outlineVersion = $state(0);

  /** 诊断按 URI 分组（含计数）。 */
  const problemsGroups = $derived.by(() => {
    void diagnosticsVersion; // 诊断变化时重算
    return allDiagnostics().filter((group) => group.entries.length > 0);
  });

  const problemsCount = $derived.by(() => {
    void diagnosticsVersion;
    return problemsGroups.reduce((sum, group) => sum + group.entries.length, 0);
  });

  /** 分支跟踪展示（↑ahead ↓behind）。 */
  const trackingDisplay = $derived.by(() => {
    const info = branchInfo;
    if (!info || (info.ahead === null && info.behind === null)) {
      return null;
    }
    return `↑${info.ahead ?? 0} ↓${info.behind ?? 0}`;
  });

  /** 从 file URI 还原磁盘路径并打开定位。 */
  async function openProblemFile(uri: string, line: number) {
    let path = decodeURIComponent(uri.replace(/^file:\/\//, ""));
    if (/^[A-Za-z]:/.test(path)) {
      path = path.replace(/\//g, "\\");
    }
    await openTextFile(path, line);
  }

  // ---- 编辑器文档状态 ----
  let docs = $state<DocEntry[]>([]);
  let activePath = $state("");
  let cursor = $state({ line: 1, col: 1 });

  // ---- 快速打开 / 文件监听 / LSP ----
  let quickOpenOpen = $state(false);
  let treeVersion = $state(0);
  let gitRefreshTimer: ReturnType<typeof setTimeout> | undefined;
  let unlistenFsChanged: (() => void) | null = null;
  let lspSession = $state<MockLspSession | null>(null);
  let lspStarting = false;

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

  const canCommit = $derived(
    !committing &&
      commitMessage.trim().length > 0 &&
      (commitAmend || stagedEntries.length > 0),
  );

  // 提交消息草稿按仓库持久化（localStorage）
  $effect(() => {
    const repo = workspacePath;
    commitMessage = localStorage.getItem(`flyide:commit-draft:${repo}`) ?? "";
    commitOutput = "";
  });
  $effect(() => {
    const repo = workspacePath;
    const message = commitMessage;
    if (repo) {
      localStorage.setItem(`flyide:commit-draft:${repo}`, message);
    }
  });

  async function doCommit() {
    const message = commitMessage.trim();
    if (!message) {
      return;
    }
    committing = true;
    commitOutput = "";
    try {
      const output = await commitFiles(workspacePath, message, {
        amend: commitAmend,
        signoff: commitSignoff,
        noVerify: commitNoVerify,
      });
      commitOutput = output;
      commitMessage = "";
      showToast(commitAmend ? "已修补提交" : "提交成功");
      await refreshGit();
    } catch (err) {
      // hooks 拒绝等失败原因完整展示给用户
      commitOutput = String(err);
      await refreshGit().catch(() => {});
    } finally {
      committing = false;
    }
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
      // 分支跟踪信息（无远端时字段为 null，不影响主流程）
      branchInfo = await loadBranchInfo(root).catch(() => null);
    } catch (err) {
      gitStatus = null;
      gitError = String(err);
    } finally {
      gitLoading = false;
    }
  }

  /** fetch/pull/push（输出展示，凭据由 Git Credential Manager 处理）。 */
  async function runNetwork(action: "fetch" | "pull" | "push") {
    netRunning = true;
    netOutput = "";
    try {
      netOutput = await gitNetwork(workspacePath, action);
      showToast(`${action} 完成`);
      await refreshGit();
    } catch (err) {
      netOutput = String(err);
    } finally {
      netRunning = false;
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

  /** 为路径生成 LSP 扩展（会话未就绪时为空）。 */
  function lspExtsFor(path: string): import("@codemirror/state").Extension[] {
    return lspSession ? lspExtensionsFor(lspSession, path, languageIdForPath(path)) : [];
  }

  // ---- 历史视图 / blame ----
  async function loadHistory() {
    if (!workspacePath) {
      return;
    }
    logLoading = true;
    logError = "";
    try {
      gitLog = await loadGitLog(workspacePath, 50);
    } catch (err) {
      logError = String(err);
    } finally {
      logLoading = false;
    }
  }

  $effect(() => {
    if (gitView === "history" && gitLog.length === 0 && !logLoading && gitStatus) {
      void loadHistory();
    }
  });

  async function showCommitDetail(hash: string) {
    commitOutput = "";
    try {
      const { showCommit } = await import("./lib/git");
      commitOutput = await showCommit(workspacePath, hash);
    } catch (err) {
      commitOutput = String(err);
    }
  }

  /** blame 开关：加载当前文档的 blame 并重建 state 附加装饰；关闭时重建移除。 */
  async function toggleBlame() {
    if (!activeDoc) {
      showToast("先打开文件再启用 blame");
      return;
    }
    if (blameOn) {
      blameOn = false;
      rebuildActiveDoc();
      return;
    }
    blameLoading = true;
    try {
      const lines: BlameLine[] = await loadGitBlame(workspacePath, activePath);
      const exts = blameExtension(blameMapOf(lines));
      rebuildActiveDoc(exts);
      blameOn = true;
    } catch (err) {
      showToast(`blame 加载失败：${String(err)}`);
    } finally {
      blameLoading = false;
    }
  }

  /** 用当前文本重建活动文档的 state（LSP/语法/blame 扩展按需附加）。 */
  function rebuildActiveDoc(extraBlame: import("@codemirror/state").Extension[] = []) {
    const doc = docs.find((entry) => entry.path === activePath);
    if (!doc) {
      return;
    }
    const line = cursorInfo(doc.state).line;
    docs = docs.map((entry) =>
      entry.path === activePath
        ? createDoc(
            entry.path,
            {
              text: entry.state.doc.toString(),
              encoding: entry.encoding,
              bom: entry.bom,
              eol: entry.eol,
              size: 0,
              tier: entry.tier,
              binary: false,
            },
            handleDocChanged,
            line,
            lspExtsFor(entry.path),
            [],
            extraBlame,
          )
        : entry,
    );
  }

  /** mock LSP 会话就绪后，为所有已打开文档附加插件（保留当前文本）。 */
  async function ensureLspAndAttach() {
    if (lspSession || lspStarting || !workspacePath) {
      return;
    }
    lspStarting = true;
    try {
      const session = await ensureMockLsp(workspacePath);
      if (!session) {
        return;
      }
      lspSession = session;
      docs = docs.map((doc) => {
        const file = {
          text: doc.state.doc.toString(),
          encoding: doc.encoding,
          bom: doc.bom,
          eol: doc.eol,
          size: 0,
          tier: doc.tier,
          binary: false,
        };
        const line = cursorInfo(doc.state).line;
        return createDoc(doc.path, file, handleDocChanged, line, lspExtsFor(doc.path));
      });
      // 为已打开文档请求大纲
      const { pathToFileUri: toUri } = await import("./lib/lsp/lsp");
      for (const doc of docs) {
        void requestOutline(session.client, toUri(doc.path)).catch(() => {});
      }
    } catch (err) {
      showToast(`LSP 启动失败：${String(err)}`);
    } finally {
      lspStarting = false;
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
      // 语法包按需加载（失败降级为纯文本）
      const syntax = await loadSyntaxFor(path).catch(() => []);
      docs = [...docs, createDoc(path, file, handleDocChanged, targetLine, lspExtsFor(path), syntax)];
      activateDoc(path);
      // 先等会话就绪（内部会为所有已打开文档请求大纲，含当前文件）
      await ensureLspAndAttach();
      if (lspSession) {
        const { pathToFileUri: toUri } = await import("./lib/lsp/lsp");
        void requestOutline(lspSession.client, toUri(path)).catch(() => {});
      }
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
    diagnosticsVersion += 1; // 重新计算问题计数
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
        <div class="git-tabs">
          <button
            type="button"
            class="git-view-tab"
            class:git-view-tab-on={gitView === "changes"}
            onclick={() => (gitView = "changes")}
          >更改</button>
          <button
            type="button"
            class="git-view-tab"
            class:git-view-tab-on={gitView === "history"}
            onclick={() => (gitView = "history")}
          >历史</button>
        </div>
        {#if gitView === "changes"}
          <div class="net-toolbar">
            <button type="button" class="net-btn" disabled={netRunning || !branchInfo?.hasRemote} onclick={() => void runNetwork("fetch")}>
              {netRunning ? "…" : "fetch"}
            </button>
            <button type="button" class="net-btn" disabled={netRunning || !branchInfo?.hasRemote} onclick={() => void runNetwork("pull")}>
              {netRunning ? "…" : "pull"}
            </button>
            <button type="button" class="net-btn" disabled={netRunning || !branchInfo?.hasRemote} onclick={() => void runNetwork("push")}>
              {netRunning ? "…" : "push"}
            </button>
          </div>
          {#if netOutput}
            <pre class="net-output">{netOutput}</pre>
          {/if}
        {/if}
        {#if gitView === "history"}
          <div class="history-list">
            {#if logLoading}
              <div class="git-empty">加载中…</div>
            {:else if logError}
              <p class="git-error">{logError}</p>
            {:else if gitLog.length === 0}
              <div class="git-empty">暂无提交</div>
            {:else}
              {#each gitLog as commit (commit.hash)}
                <button
                  type="button"
                  class="log-row"
                  title={`${commit.hash}\n${commit.author} · ${commit.date.slice(0, 10)}`}
                  onclick={() => showCommitDetail(commit.hash)}
                >
                  <span class="log-short">{commit.short}</span>
                  <span class="log-subject" title={commit.subject}>{commit.subject}</span>
                  {#if commit.refs.length > 0}
                    <span class="log-refs">{commit.refs.join(", ")}</span>
                  {/if}
                  <span class="log-meta">{commit.author} · {commit.date.slice(0, 10)}</span>
                </button>
              {/each}
              {#if commitOutput}
                <pre class="commit-output">{commitOutput}</pre>
              {/if}
            {/if}
          </div>
        {:else}
          <p class="git-meta">
          <span class="git-branch">{gitStatus.branch ?? "HEAD (detached)"}</span>
          {#if trackingDisplay}
            <span title={`上游: ${branchInfo?.upstream ?? "无"}`}>{trackingDisplay}</span>
          {:else if gitStatus.ahead !== null || gitStatus.behind !== null}
            <span>↑{gitStatus.ahead ?? 0} ↓{gitStatus.behind ?? 0}</span>
          {/if}
          {#if branchInfo?.hasRemote}
            <span class="git-remote" title="已配置远程仓库">●</span>
          {/if}
          {#if gitVersion}<span class="git-ver">git {gitVersion}</span>{/if}
        </p>
        <div class="commit-box">
          <textarea
            class="commit-message"
            bind:value={commitMessage}
            placeholder="提交消息（Ctrl+Enter 提交）"
            rows="3"
            aria-label="提交消息"
            onkeydown={(event) => {
              if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
                event.preventDefault();
                if (canCommit) {
                  void doCommit();
                }
              }
            }}
          ></textarea>
          <div class="commit-options">
            <label><input type="checkbox" bind:checked={commitAmend} /> amend</label>
            <label><input type="checkbox" bind:checked={commitSignoff} /> sign-off</label>
            <label><input type="checkbox" bind:checked={commitNoVerify} /> no-verify</label>
            <span class="commit-spacer"></span>
            <button type="button" class="commit-btn" disabled={!canCommit} onclick={() => void doCommit()}>
              {committing ? "提交中…" : commitAmend ? "修补提交" : "提交"}
            </button>
          </div>
          {#if commitOutput}
            <pre class="commit-output">{commitOutput}</pre>
          {/if}
        </div>
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
        {/if}
      {:else}
        <div class="panel-body">输入 git 仓库根目录后加载状态。</div>
      {/if}
    {:else if activePanel === "outline"}
      <div class="outline-panel">
        {#if !lspSession}
          <div class="git-empty">大纲需要 LSP 会话（打开文件后自动建立）</div>
        {:else if activeDoc}
          {@const symbols = getOutline(pathToFileUri(activeDoc.path))}
          {#if symbols.length === 0}
            <div class="git-empty">当前文件无符号</div>
          {:else}
            {#each symbols as sym, index (index)}
              <button
                type="button"
                class="outline-row"
                title={`${sym.name} · 行 ${sym.line + 1}`}
                onclick={() => openTextFile(activePath, sym.line + 1)}
              >
                <span class="outline-kind">{sym.kind}</span>
                <span class="outline-name">{sym.name}</span>
                <span class="outline-line">{sym.line + 1}</span>
              </button>
            {/each}
          {/if}
        {:else}
          <div class="git-empty">当前文件无符号</div>
        {/if}
      </div>
    {:else if activePanel === "settings"}
      <SettingsPanel />
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
    <span>分支: {gitStatus?.branch ?? "—"}{trackingDisplay ? ` ${trackingDisplay}` : ""}</span>
    <button
      type="button"
      class="problems-toggle"
      class:problems-toggle-on={bottomPanelOpen}
      title="问题面板（诊断）"
      onclick={() => (bottomPanelOpen = !bottomPanelOpen)}
    >问题: {problemsCount}</button>
    <span class="spacer"></span>
    {#if activeDoc}
      <span>行 {cursor.line}，列 {cursor.col}</span>
      <span>{activeDoc.encoding}{activeDoc.bom ? " BOM" : ""}</span>
      <span>{activeDoc.eol}</span>
      <span>LSP: {lspSession ? "mock" : "—"}</span>
      <button
        type="button"
        class="blame-toggle"
        class:blame-toggle-on={blameOn}
        disabled={blameLoading}
        title="行内 blame（当前文件）"
        onclick={() => void toggleBlame()}
      >{blameLoading ? "blame…" : blameOn ? "blame ✓" : "blame"}</button>
    {:else}
      <span>行 1，列 1</span>
      <span>UTF-8</span>
      <span>LF</span>
    {/if}
  </footer>

  {#if bottomPanelOpen}
    <section class="problems-panel" aria-label="问题面板">
      {#if problemsGroups.length === 0}
        <div class="problems-empty">没有诊断问题</div>
      {:else}
        {#each problemsGroups as group (group.uri)}
          <div class="problems-group">
            <div class="problems-file" title={group.uri}>
              {decodeURIComponent(group.uri.replace(/^file:\/\//, "")).replace(/\//g, "\\")}（{group.entries.length}）
            </div>
            {#each group.entries as entry, index (index)}
              <button
                type="button"
                class="problems-row"
                title={entry.message}
                onclick={() => openProblemFile(group.uri, entry.line + 1)}
              >
                <span class="problems-sev" class:problems-sev-error={entry.severity === 1}>
                  {entry.severity === 1 ? "✗" : entry.severity === 2 ? "⚠" : "ℹ"}
                </span>
                <span class="problems-loc">{entry.line + 1}:{entry.col + 1}</span>
                <span class="problems-msg">{entry.message}</span>
              </button>
            {/each}
          </div>
        {/each}
      {/if}
    </section>
  {/if}

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
