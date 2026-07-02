<script>
  let { state } = $props();
  let input = $derived(state?.xlc_input ?? "");
  let output = $derived(state?.xlc_output ?? []);
  let search = $derived(state?.search_pattern);
</script>

<div class="xlc">
  <div class="output">
    {#each output.slice(-10) as line}
      <div class="out-line">{line}</div>
    {/each}
  </div>
  <div class="input-line">
    <span class="prompt">{search ? "/" : ":"}</span>
    <span>{input}</span>
  </div>
</div>

<style>
  .xlc { border-top: 1px solid #24283b; background: #1a1b26; max-height: 120px; overflow-y: auto; }
  .out-line { padding: 2px 12px; font-size: 12px; color: #a9b1d6; }
  .input-line { padding: 4px 12px; display: flex; gap: 4px; font-size: 14px; }
  .prompt { color: #7aa2f7; }
</style>
