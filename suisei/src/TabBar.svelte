<script>
  let { state } = $props();
  let tabs = $derived(state?.tabs ?? []);
  let current = $derived(state?.current_tab ?? 0);
</script>

<div class="tabs">
  {#each tabs as tab, i}
    <div class="tab" class:active={i === current}>
      {tab.filename}{tab.modified ? " +" : ""}
    </div>
  {/each}
</div>

<style>
  .tabs { display: flex; height: 24px; background: #1a1b26; }
  .tab {
    padding: 2px 12px; font-size: 12px;
    background: #1a1b26; color: #565f89;
  }
  .tab.active { background: #24283b; color: #c0caf5; }
</style>
