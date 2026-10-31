<script lang="ts">
  import {
    nextSearchId,
    searchCancel,
    searchContent,
    type ContentMatch,
  } from "../search";

  let {
    root,
    onOpenFile,
  }: {
    root: string;
    onOpenFile: (path: string, line: number) => void;
  } = $props();

  let query = $state("");
  let caseSensitive = $state(false);
  let wholeWord = $state(false);
  let regex = $state(false);
  let loading = $state(false);
  let error = $state("");
  let matches = $state<ContentMatch[]>([]);
  let truncated = $state(false);
  let filesSearched = $state(0);

  let currentSearchId = 0;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  const grouped = $derived.by(() => {
    const groups: { path: string; matches: ContentMatch[] }[] = [];
    for (const match of matches) {
      const last = groups[groups.length - 1];
      if (last && last.path === match.path) {
        last.matches.push(match);
      } else {
        groups.push({ path: match.path, matches: [match] });
      }
    }
    return groups;
  });

  function join(rootPath: string, rel: string): string {
    return `${rootPath.replace(/[\\/]+$/, "")}/${rel}`;
  }

  async function run() {
    const rootPath = root.trim();
    if (!rootPath || !query.trim()) {
      return;
    }
    // 取消上一次仍在跑的搜索
    if (currentSearchId > 0) {
      void searchCancel(currentSearchId).catch(() => {});
    }
    const searchId = nextSearchId();
    currentSearchId = searchId;
    loading = true;
    error = "";
    try {
      const result = await searchContent(
        rootPath,
        query,
        { caseSensitive, wholeWord, regex },
        searchId,
      );
      // 只展示最新一次搜索的结果
      if (searchId === currentSearchId) {
        matches = result.matches;
        truncated = result.truncated;
        filesSearched = result.filesSearched;
      }
    } catch (err) {
      if (searchId === currentSearchId) {
        error = String(err);
        matches = [];
      }
    } finally {
      if (searchId === currentSearchId) {
        loading = false;
      }
    }
  }

  function onInput() {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => void run(), 400);
  }
</script>

<div class="search-panel">
  <div class="git-toolbar">
    <input
      class="git-input"
      bind:value={query}
      placeholder="搜索内容，Enter 执行"
      aria-label="搜索内容"
      oninput={onInput}
      onkeydown={(event) => {
        if (event.key === "Enter") {
          void run();
        }
      }}
    />
  </div>
  <div class="search-options">
    <label><input type="checkbox" bind:checked={caseSensitive} /> Aa</label>
    <label><input type="checkbox" bind:checked={wholeWord} /> 整词</label>
    <label><input type="checkbox" bind:checked={regex} /> .*</label>
  </div>
  {#if error}
    <p class="git-error">{error}</p>
  {:else if loading}
    <div class="search-hint">搜索中…</div>
  {:else if matches.length > 0}
    <div class="search-hint">
      {matches.length} 条结果（{filesSearched} 个文件）{truncated ? "，已达上限，请缩小范围" : ""}
    </div>
    {#each grouped as group (group.path)}
      <div class="search-group">
        <div class="search-group-title" title={group.path}>
          {group.path}（{group.matches.length}）
        </div>
        {#each group.matches as match (match.line)}
          <button
            type="button"
            class="search-row"
            title={match.text}
            onclick={() => onOpenFile(join(root, match.path), match.line)}
          >
            <span class="search-line">{match.line}</span>
            <span class="search-text">{match.text}</span>
          </button>
        {/each}
      </div>
    {/each}
  {:else if matches.length === 0 && filesSearched > 0}
    <div class="search-hint">无匹配</div>
  {:else}
    <div class="panel-body">输入内容后搜索当前工作区（尊重 .gitignore）。</div>
  {/if}
</div>
