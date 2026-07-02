<script>
  let { state, error } = $props();
  let mode = $derived(state?.mode ?? "Normal");
  let file = $derived(state?.filename ?? "[no name]");
  let lang = $derived(state?.filename?.split(".").pop() ?? "txt");
  let diags = $derived(state?.lsp_diagnostics ?? 0);
</script>

<div class="status">
  <div class="left">
    <span class="mode">{mode}</span>
    <span>{file}</span>
  </div>
  <div class="right">
    {#if diags > 0}<span class="diag">{diags} issues</span>{/if}
    <span>{lang}</span>
    <span>{state?.cursor_row + 1}:{state?.cursor_col + 1}</span>
  </div>
  {#if error}<div class="error">{error}</div>{/if}
</div>

<style>
  .status {
    display: flex; justify-content: space-between; align-items: center;
    height: 24px; padding: 0 12px;
    background: #24283b; color: #a9b1d6; font-size: 12px;
  }
  .left, .right { display: flex; gap: 16px; }
  .mode { color: #9ece6a; font-weight: bold; }
  .diag { color: #f7768e; }
  .error { color: #f7768e; position: fixed; bottom: 28px; }
</style>
