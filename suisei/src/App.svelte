<script>
  import { invoke } from "@tauri-apps/api/core";
  import Explorer from "./Explorer.svelte";
  import Editor from "./Editor.svelte";
  import Terminal from "./Terminal.svelte";
  import XlcPanel from "./XlcPanel.svelte";
  import StatusBar from "./StatusBar.svelte";
  import TabBar from "./TabBar.svelte";

  let state = $state(null);
  let explorerWidth = $state(200);
  let termWidth = $state(0);
  let dragging = $state(null);

  async function refresh() {
    try { state = await invoke("get_state"); } catch(e) {}
  }

  async function sendKey(e) {
    e.preventDefault();
    try {
      await invoke("handle_key", {
        key: e.key, ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, meta: e.metaKey,
      });
      await refresh();
    } catch(e) {}
  }

  function onMouseDownResize(panel, e) {
    e.preventDefault();
    dragging = { panel, startX: e.clientX, startW: panel === "explorer" ? explorerWidth : termWidth };
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }
  function onMouseMove(e) {
    if (!dragging) return;
    let dw = e.clientX - dragging.startX;
    let newW = (dragging.startW + dw).clamp(100, 400);
    if (dragging.panel === "explorer") explorerWidth = newW;
    else termWidth = newW;
  }
  function onMouseUp() {
    dragging = null;
    document.removeEventListener("mousemove", onMouseMove);
    document.removeEventListener("mouseup", onMouseUp);
  }

  $effect(() => {
    refresh();
    let timer = setInterval(refresh, 500);
    return () => clearInterval(timer);
  });
</script>

<svelte:window onkeydown={sendKey} />

<div class="app" style="font-family:'SF Mono','Fira Code',monospace;font-size:14px;">
  {#if state?.tabs}
    <TabBar {state} />
  {/if}

  <div class="main">
    {#if state?.explorer_open}
      <div class="panel" style="width:{explorerWidth}px;min-width:100px;">
        <Explorer {state} />
      </div>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="resize-handle" onmousedown={(e) => onMouseDownResize("explorer", e)}></div>
    {/if}

    <div class="editor-area" style="flex:1;overflow:hidden;">
      <Editor {state} />
    </div>

    {#if state?.terminal_open}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="resize-handle" onmousedown={(e) => onMouseDownResize("term", e)}></div>
      <div class="panel" style="width:{termWidth || 300}px;min-width:150px;">
        <Terminal {state} />
      </div>
    {/if}
  </div>

  {#if state?.xlc_open}
    <XlcPanel {state} />
  {/if}
  {#if state}
    <StatusBar {state} />
  {/if}
</div>

<style>
  :global(body) { margin:0; padding:0; background:#1a1b26; color:#c0caf5; overflow:hidden; }
  .app { display:flex; flex-direction:column; height:100vh; background:#1a1b26; color:#c0caf5; }
  .main { display:flex; flex:1; overflow:hidden; }
  .panel { overflow-y:auto; overflow-x:hidden; }
  .editor-area { display:flex; flex-direction:column; overflow:hidden; }
  .resize-handle {
    width:4px; cursor:col-resize; background:#24283b;
    flex-shrink:0; transition: background 0.15s;
  }
  .resize-handle:hover { background:#7aa2f7; }
</style>
