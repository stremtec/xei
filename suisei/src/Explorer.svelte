<script>
  let { state } = $props();
  let entries = $derived(state?.explorer_entries ?? []);
  let selected = $derived(state?.explorer_selected ?? 0);
  let cwd = $derived(state?.explorer_cwd ?? "");
</script>

<div class="explorer">
  <div class="cwd">📂 {cwd.split("/").pop() || cwd}</div>
  {#each entries as entry, i}
    <div class="entry" class:selected={i === selected}>
      <span class="icon">{entry.is_dir ? "📁" : "📄"}</span>
      {entry.name}
    </div>
  {/each}
</div>

<style>
  .explorer {
    width: 200px; height: 100%; overflow-y: auto;
    background: #1a1b26; border-right: 1px solid #24283b;
    font-size: 13px;
  }
  .cwd { padding: 4px 8px; color: #7aa2f7; font-weight: bold; }
  .entry { padding: 2px 8px; cursor: pointer; display: flex; gap: 4px; }
  .entry.selected { background: rgba(255,255,255,0.07); }
  .icon { width: 20px; text-align: center; }
</style>
