<script lang="ts">
  import { onMount } from "svelte";
  import { getSettings, updateSettings, type Settings } from "../settings";

  let {
    onClose,
  }: {
    onClose?: () => void;
  } = $props();

  let settings = $state<Settings | null>(null);
  let saving = $state(false);
  let error = $state("");
  let toast = $state("");

  const themes = [
    { value: "dark", label: "深色" },
    { value: "light", label: "浅色" },
    { value: "system", label: "跟随系统" },
  ] as const;

  onMount(() => {
    void load();
  });

  async function load() {
    error = "";
    try {
      settings = await getSettings();
    } catch (err) {
      error = String(err);
    }
  }

  async function save() {
    if (!settings) {
      return;
    }
    saving = true;
    error = "";
    try {
      settings = await updateSettings(settings);
      toast = "已保存";
      setTimeout(() => (toast = ""), 2000);
    } catch (err) {
      error = String(err);
    } finally {
      saving = false;
    }
  }
</script>

<div class="settings-panel">
  {#if error}
    <p class="git-error">{error}</p>
  {/if}
  {#if !settings}
    <div class="git-empty">加载中…</div>
  {:else}
    <section class="settings-group">
      <h3>外观</h3>
      <label class="settings-row">
        <span>主题</span>
        <select bind:value={settings.theme}>
          {#each themes as t (t.value)}
            <option value={t.value}>{t.label}</option>
          {/each}
        </select>
      </label>
      <label class="settings-row">
        <span>界面语言</span>
        <select bind:value={settings.language}>
          <option value="zh-CN">中文</option>
          <option value="en-US">English</option>
        </select>
      </label>
    </section>

    <section class="settings-group">
      <h3>编辑器</h3>
      <label class="settings-row">
        <span>字体</span>
        <input bind:value={settings.editorFont} placeholder="Cascadia Code, Consolas, monospace" />
      </label>
      <label class="settings-row">
        <span>字号</span>
        <input type="number" min="8" max="32" bind:value={settings.editorFontSize} />
      </label>
      <label class="settings-row">
        <span>Tab 宽度</span>
        <input type="number" min="1" max="16" bind:value={settings.tabSize} />
      </label>
      <label class="settings-row settings-check">
        <input type="checkbox" bind:checked={settings.formatOnSave} />
        <span>保存时格式化</span>
      </label>
    </section>

    <div class="settings-actions">
      <button type="button" class="settings-save" disabled={saving} onclick={() => void save()}>
        {saving ? "保存中…" : "保存"}
      </button>
      {#if toast}
        <span class="settings-toast">{toast}</span>
      {/if}
    </div>
  {/if}
</div>
