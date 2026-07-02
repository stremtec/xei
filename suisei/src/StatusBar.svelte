<script>
  let { state } = $props();
  let mode = $derived(state?.mode ?? "Normal");
  let pending = $derived(state?.pending_key);
  let file = $derived(state?.filename?.split("/").pop() || "[no name]");
  let lang = $derived(state?.filename?.split(".").pop() ?? "txt");
  let diags = $derived(state?.lsp_diagnostics ?? 0);
  let modified = $derived(state?.modified ?? false);

  let display = $derived(
    pending ? pending.toUpperCase() :
    mode === "Normal" ? "NORMAL" :
    mode === "Insert" ? "INSERT" :
    mode === "Visual" ? "VISUAL" :
    mode === "VisualLine" ? "V-LINE" :
    mode === "Explorer" ? "EXPLORER" :
    mode === "Terminal" ? "TERMINAL" :
    mode === "XlcInput" ? "COMMAND" : mode
  );
  let modeColor = $derived(
    mode === "Insert" ? "#9ece6a" :
    mode === "Visual" || mode === "VisualLine" ? "#e0af68" :
    mode === "Explorer" || mode === "Terminal" ? "#7dcfff" :
    "#7aa2f7"
  );
</script>

<div class="status">
  <div class="left">
    <span class="mode" style="color:{modeColor};">{display}</span>
    <span>{file}{modified ? " +" : ""}</span>
    {#if state?.message}
      <span class="msg">{state.message}</span>
    {/if}
  </div>
  <div class="right">
    {#if diags > 0}<span class="diag">{diags} issues</span>{/if}
    <span>{lang}</span>
    <span>{state?.cursor_row + 1}:{state?.cursor_col + 1}</span>
  </div>
</div>

<style>
  .status { display:flex; justify-content:space-between; align-items:center; height:24px; padding:0 12px; background:#24283b; color:#a9b1d6; font-size:12px; flex-shrink:0; }
  .left, .right { display:flex; gap:12px; align-items:center; }
  .mode { font-weight:bold; }
  .diag { color:#f7768e; }
  .msg { color:#e0af68; }
</style>
