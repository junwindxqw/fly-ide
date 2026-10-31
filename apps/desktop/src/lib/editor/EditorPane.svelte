<script lang="ts">
  import { EditorView } from "@codemirror/view";
  import type { EditorState } from "@codemirror/state";
  import { untrack } from "svelte";

  let { editorState }: { editorState: EditorState } = $props();

  let host = $state<HTMLDivElement | null>(null);
  let view: EditorView | null = null;

  // 挂载/卸载只在 host 变化时发生；editorState 变化走下方同步 effect，
  // 避免“每个按键销毁重建视图”导致滚动位置丢失。
  $effect(() => {
    if (!host) {
      return;
    }
    const initial = untrack(() => editorState);
    view = new EditorView({ state: initial, parent: host });
    return () => {
      view?.destroy();
      view = null;
    };
  });

  $effect(() => {
    const next = editorState;
    if (view && view.state !== next) {
      view.setState(next);
      // 切换文档/跳转行后把光标滚到可视区中央
      view.dispatch({
        effects: EditorView.scrollIntoView(next.selection.main.head, { y: "center" }),
      });
    }
  });
</script>

<div class="editor-host" bind:this={host}></div>
