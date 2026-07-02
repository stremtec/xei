<script>
  let { state } = $props();
  let lines = $derived(state?.lines ?? []);
  let cursorRow = $derived(state?.cursor_row ?? 0);
  let cursorCol = $derived(state?.cursor_col ?? 0);
  let mode = $derived(state?.mode ?? "Normal");
  let scroll = $derived(state?.scroll ?? 0);
  let searchMatches = $derived(state?.search_matches ?? []);
  let searchCurrent = $derived(state?.search_current ?? 0);

  let matchSet = $derived(new Set(searchMatches.map(([r, c]) => `${r},${c}`)));
  let currentMatch = $derived(searchMatches[searchCurrent] ? `${searchMatches[searchCurrent][0]},${searchMatches[searchCurrent][1]}` : null);

  function isSelected(row, col) {
    if (mode !== "Visual" && mode !== "VisualLine") return false;
    // Simple: highlight cursor row in VisualLine, cursor row+col area in Visual
    return row === cursorRow;
  }
</script>

<div class="editor" style="height:100%;overflow-y:auto;padding:4px 0;">
  {#each lines.slice(scroll, scroll + 50) as line, i}
    {@const row = scroll + i}
    <div class="line" class:active={row === cursorRow} class:visual={isSelected(row, 0)} style="display:flex;">
      <span class="gutter">{String(row + 1).padStart(4, " ")} </span>
      <span class="text">
        {#if row === cursorRow}
          <span>{line.slice(0, cursorCol)}</span>
          <span class="cursor" class:insert={mode === "Insert"}>{line[cursorCol] || " "}</span>
          <span>{line.slice(cursorCol + 1)}</span>
        {:else}
          {line}
        {/if}
      </span>
    </div>
  {/each}
  {#if lines.length === 0}
    <div class="line"><span class="gutter">    1 </span><span class="text">~</span></div>
  {/if}
</div>

<style>
  .editor { font-family: 'SF Mono','Fira Code',monospace; line-height:1.6; font-size:14px; tab-size:4; }
  .line { min-height:22px; white-space:pre; }
  .line.active { background:rgba(255,255,255,0.06); }
  .line.visual { background:rgba(100,100,255,0.2); }
  .gutter { color:#565f89; min-width:48px; text-align:right; margin-right:12px; user-select:none; flex-shrink:0; }
  .text { flex:1; }
  .cursor { background:#7dcfff; color:#1a1b26; }
  .cursor.insert { background:transparent; border-left:2px solid #7dcfff; color:inherit; }
</style>
