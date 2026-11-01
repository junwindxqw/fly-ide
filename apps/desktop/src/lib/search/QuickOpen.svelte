<script lang="ts">
  import { searchFiles, type FileMatch } from "../search";

  let {
    root,
    onOpenFile,
    onClose,
  }: {
    root: string;
    onOpenFile: (path: string) => void;
    onClose: () => void;
  } = $props();

  let inputEl = $state<HTMLInputElement | null>(null);
  let query = $state("");
  let results = $state<FileMatch[]>([]);
  let selected = $state(0);
  let loading = $state(false);
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    inputEl?.focus();
  });

  async function run() {
    if (!root) {
      return;
    }
    loading = true;
    try {
      results = await searchFiles(root, query, 50);
      selected = 0;
    } catch (err) {
      results = [];
      console.error("快速打开搜索失败", err);
    } finally {
      loading = false;
    }
  }

  function onInput() {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => void run(), 120);
  }

  function open(index: number) {
    const match = results[index];
    if (!match) {
      return;
    }
    onOpenFile(`${root.replace(/[\\/]+$/, "")}/${match.path}`);
    onClose();
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      selected = Math.min(selected + 1, results.length - 1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      selected = Math.max(selected - 1, 0);
    } else if (event.key === "Enter") {
      event.preventDefault();
      open(selected);
    } else if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }
</script>

<div
  class="quickopen-overlay"
  role="presentation"
  onclick={(event) => {
    if (event.target === event.currentTarget) {
      onClose();
    }
  }}
>
  <div class="quickopen" role="dialog" aria-label="快速打开">
    <input
      bind:this={inputEl}
      bind:value={query}
      class="quickopen-input"
      placeholder="输入文件名模糊搜索，↑↓ 选择，Enter 打开，Esc 关闭"
      aria-label="快速打开搜索"
      oninput={onInput}
      onkeydown={onKeydown}
    />
    <div class="quickopen-list">
      {#if loading}
        <div class="quickopen-hint">搜索中…</div>
      {:else if results.length === 0}
        <div class="quickopen-hint">无匹配文件</div>
      {:else}
        {#each results as match, index (match.path)}
          <!-- svelte-ignore a11y_mouse_events_have_key_events, a11y_no_noninteractive_element_interactions -->
          <!-- 鼠标悬停跟随选中是快速打开的刻意交互（同 VS Code），键盘导航由 ↑↓ 提供 -->
          <button
            type="button"
            class="quickopen-item"
            class:quickopen-item-active={index === selected}
            onmouseover={() => (selected = index)}
            onclick={() => open(index)}
          >
            {match.path}
          </button>
        {/each}
      {/if}
    </div>
  </div>
</div>
