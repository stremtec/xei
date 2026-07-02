<script>
  let { state } = $props();
  let rows = $derived(state?.terminal_rows ?? []);
  let mode = $derived(state?.mode ?? "Normal");
</script>

{#if rows.length > 0}
  <div class="term" class:active={mode === "Terminal"}>
    <div class="term-header">TERMINAL {mode === "Terminal" ? "[Esc to exit]" : ""}</div>
    <div class="term-body">
      {#each rows as row}
        <div class="term-row">
          {#each row as cell}
            <span>{cell.ch}</span>
          {/each}
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .term { height:100%; display:flex; flex-direction:column; background:#0f0f1a; }
  .term.active { border-left:2px solid #7aa2f7; }
  .term-header { padding:2px 8px; font-size:11px; background:#24283b; color:#565f89; }
  .term-body { flex:1; overflow-y:auto; padding:4px; font-size:12px; line-height:1.3; }
  .term-row { white-space:pre; }
</style>
