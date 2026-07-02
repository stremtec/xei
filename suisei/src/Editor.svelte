<script>
  let { state } = $props();

  let lines = $derived(state?.text?.split("\n") ?? []);
  let cursorRow = $derived(state?.cursor_row ?? 0);
  let cursorCol = $derived(state?.cursor_col ?? 0);
  let mode = $derived(state?.mode ?? "Normal");
</script>

<div class="editor">
  {#each lines as line, i}
    <div class="line" class:active={i === cursorRow}>
      <span class="gutter">{String(i + 1).padStart(4, " ")} </span>
      {#if i === cursorRow}
        <span>{line.slice(0, cursorCol)}</span>
        <span class="cursor" class:insert={mode === "Insert"}>{line[cursorCol] || " "}</span>
        <span>{line.slice(cursorCol + 1)}</span>
      {:else}
        <span>{line}</span>
      {/if}
    </div>
  {/each}
</div>

<style>
  .editor { font-family: 'SF Mono', 'Fira Code', monospace; line-height: 1.5; }
  .line { display: flex; }
  .line.active { background: rgba(255,255,255,0.05); }
  .gutter { color: #565f89; min-width: 3em; text-align: right; margin-right: 8px; user-select: none; }
  .cursor { background: #7dcfff; color: #1a1b26; }
  .cursor.insert { background: transparent; border-left: 2px solid #7dcfff; color: inherit; }
</style>
