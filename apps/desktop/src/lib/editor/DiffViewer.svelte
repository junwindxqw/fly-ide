<script lang="ts">
  import { EditorState } from "@codemirror/state";
  import { EditorView } from "@codemirror/view";
  import { unifiedMergeView } from "@codemirror/merge";

  let {
    oldText = "",
    newText = "",
    binary = false,
  }: { oldText?: string; newText?: string; binary?: boolean } = $props();

  let host = $state<HTMLDivElement | null>(null);
  let view: EditorView | null = null;

  const theme = EditorView.theme({
    "&": { height: "100%", fontSize: "12px" },
    ".cm-scroller": {
      overflow: "auto",
      fontFamily: "'Cascadia Code', Consolas, monospace",
      lineHeight: "1.5",
    },
    ".cm-content": { padding: "4px 0" },
  });

  // props 变化（切换文件/源）时销毁重建；effect 清理函数负责销毁旧视图
  $effect(() => {
    const modified = newText;
    const original = oldText;
    if (!host || binary) {
      return;
    }
    view = new EditorView({
      state: EditorState.create({
        doc: modified,
        extensions: [
          EditorView.editable.of(false),
          EditorState.readOnly.of(true),
          unifiedMergeView({
            original,
            mergeControls: false,
            highlightChanges: true,
            gutter: true,
          }),
          theme,
        ],
      }),
      parent: host,
    });
    return () => {
      view?.destroy();
      view = null;
    };
  });
</script>

{#if binary}
  <div class="diff-binary">二进制文件，暂不支持 diff 视图（Phase 1 提供大小/哈希对比）</div>
{:else}
  <div class="diff-host" bind:this={host}></div>
{/if}
