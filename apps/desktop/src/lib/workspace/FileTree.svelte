<script lang="ts">
  import { fsListDir, type DirEntry } from "../fs";
  import { untrack } from "svelte";

  let {
    root,
    activePath = "",
    version = 0,
    onOpenFile,
  }: {
    root: string;
    activePath?: string;
    /** 递增以重新拉取根目录与已展开目录（外部文件变更后刷新） */
    version?: number;
    onOpenFile: (path: string) => void;
  } = $props();

  let expanded = $state<Record<string, boolean>>({});
  let children = $state<Record<string, DirEntry[]>>({});
  let loadError = $state("");

  function join(parent: string, name: string): string {
    return `${parent.replace(/[\\/]+$/, "")}/${name}`;
  }

  async function ensureChildren(path: string) {
    if (children[path]) {
      return;
    }
    try {
      const list = await fsListDir(path);
      children = { ...children, [path]: list };
    } catch (err) {
      loadError = String(err);
    }
  }

  async function refreshLoaded() {
    // 重新拉取根目录与已展开目录（保留展开状态）
    const targets = [root, ...Object.keys(expanded).filter((path) => expanded[path])];
    try {
      const updates: Record<string, DirEntry[]> = { ...children };
      for (const path of targets) {
        updates[path] = await fsListDir(path);
      }
      children = updates;
      loadError = "";
    } catch (err) {
      loadError = String(err);
    }
  }

  $effect(() => {
    const dir = root;
    if (!dir) {
      return;
    }
    expanded = {};
    children = {};
    loadError = "";
    // children 的读取会建立依赖导致循环刷新，这里只关心 root
    untrack(() => void ensureChildren(dir));
  });

  $effect(() => {
    const dir = root;
    const v = version;
    if (!dir || v === 0) {
      return;
    }
    untrack(() => void refreshLoaded());
  });

  function toggle(path: string) {
    expanded = { ...expanded, [path]: !expanded[path] };
    if (expanded[path]) {
      void ensureChildren(path);
    }
  }
</script>

{#snippet rows(entries: DirEntry[], parent: string, depth: number)}
  {#each entries as entry (entry.name)}
    {@const fullPath = join(parent, entry.name)}
    <div class="tree-row" class:tree-active={fullPath === activePath} style={`padding-left: ${8 + depth * 14}px`}>
      {#if entry.isDir}
        <button type="button" class="tree-name" onclick={() => toggle(fullPath)}>
          <span class="tree-caret">{expanded[fullPath] ? "▾" : "▸"}</span>
          <span class:tree-ignored={entry.ignored}>{entry.name}</span>
        </button>
      {:else}
        <button type="button" class="tree-name" onclick={() => onOpenFile(fullPath)}>
          <span class="tree-caret"></span>
          <span class:tree-ignored={entry.ignored}>{entry.name}</span>
        </button>
      {/if}
    </div>
    {#if entry.isDir && expanded[fullPath]}
      {@render rows(children[fullPath] ?? [], fullPath, depth + 1)}
    {/if}
  {/each}
{/snippet}

{#if loadError}
  <p class="git-error">{loadError}</p>
{:else if root}
  {@render rows(children[root] ?? [], root, 0)}
{:else}
  <div class="panel-body">打开文件夹后在此显示文件树。</div>
{/if}
