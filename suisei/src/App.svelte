<script>
  import { invoke } from "@tauri-apps/api/core";
  import Editor from "./Editor.svelte";
  import StatusBar from "./StatusBar.svelte";
  import TabBar from "./TabBar.svelte";
  import Explorer from "./Explorer.svelte";
  import XlcPanel from "./XlcPanel.svelte";

  let state = $state(null);
  let error = $state("");

  async function refresh() {
    try {
      state = await invoke("get_state");
    } catch (e) {
      error = e.toString();
    }
  }

  async function onKey(e) {
    if (e.metaKey || e.ctrlKey || e.altKey) {
      e.preventDefault();
      try {
        await invoke("handle_key", {
          key: e.key,
          ctrl: e.ctrlKey,
          alt: e.altKey,
          shift: e.shiftKey,
          meta: e.metaKey,
        });
        await refresh();
      } catch (err) {
        error = err.toString();
      }
      return;
    }
    if (e.key.length === 1 || ["Enter", "Backspace", "Tab", "Escape", "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(e.key)) {
      e.preventDefault();
      try {
        await invoke("handle_key", {
          key: e.key,
          ctrl: false, alt: false, shift: false, meta: false,
        });
        await refresh();
      } catch (err) {
        error = err.toString();
      }
    }
  }

  $effect(() => { refresh(); });
</script>

<svelte:window onkeydown={onKey} />

<div class="app">
  <TabBar {state} />
  <div class="main">
    {#if state?.explorer_open}
      <Explorer {state} />
    {/if}
    <div class="editor-wrap" tabindex="0">
      <Editor {state} />
    </div>
  </div>
  {#if state?.xlc_open}
    <XlcPanel {state} />
  {/if}
  <StatusBar {state} {error} />
</div>

<style>
  .app { display: flex; flex-direction: column; height: 100vh; background: #1a1b26; color: #c0caf5; font-size: 14px; }
  .main { display: flex; flex: 1; overflow: hidden; }
  .editor-wrap { flex: 1; overflow-y: auto; outline: none; padding: 4px 8px; font-family: 'SF Mono', 'Fira Code', monospace; line-height: 1.5; white-space: pre; }
  .editor-wrap:focus { background: #1a1b26; }
</style>
