<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  /**
   * CdfxImportDialog — review what importing a CDFX calibration file would
   * change before any of it is written.
   *
   * The backend has already resolved every value against the loaded A2L and
   * the current image, so this is purely presentational: it shows the tally,
   * lists the differences, and hands the whole set back to be applied as one
   * undoable edit.
   *
   * Props
   *   report   – the CdfxImport returned by cdfxPreview, or null when closed
   *   onApply  – called when the user accepts the changes
   *   onClose  – called when the dialog should be dismissed
   */
  let {
    report  = /** @type {any} */ (null),
    onApply = () => {},
    onClose = () => {},
  } = $props();

  /** Which of the three lists is open. Changes are the point, so they lead. */
  let tab = $state(/** @type {'changes'|'skipped'|'missing'} */ ('changes'));

  $effect(() => { if (report) tab = 'changes'; });

  const changes = $derived(report?.changes ?? []);
  const skipped = $derived(report?.skipped ?? []);
  const missing = $derived(report?.not_in_a2l ?? []);

  /** Distinct parameters touched — a curve with twelve edited points is one
   *  parameter, and that is the number a user reasons about. */
  const changedParams = $derived(new Set(changes.map((/** @type {any} */ c) => c.name)).size);

  function hex32(n) {
    return '0x' + n.toString(16).toUpperCase().padStart(8, '0');
  }

  /** `NAME`, or `NAME axis[3]` for one point of a 1D object. */
  function label(c) {
    if (c.index === null || c.index === undefined) return c.name;
    return `${c.name} ${c.target}[${c.index}]`;
  }

  function handleBackdrop(e) {
    if (e.target === e.currentTarget) onClose();
  }

  function handleKey(e) {
    if (e.key === 'Escape') { onClose(); return; }
    if (e.key === 'Enter' && changes.length) onApply();
  }
</script>

{#if report}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="backdrop" role="dialog" aria-modal="true" aria-label="Import calibration data"
       onclick={handleBackdrop} onkeydown={handleKey}>
    <div class="card">
      <h2 class="title">Import Calibration Data</h2>
      <p class="subtitle">{report.file_name}</p>

      <div class="tally">
        <div class="stat">
          <span class="stat-num" class:accent={changes.length > 0}>{changedParams}</span>
          <span class="stat-lbl">to change</span>
        </div>
        <div class="stat">
          <span class="stat-num">{report.unchanged}</span>
          <span class="stat-lbl">already match</span>
        </div>
        <div class="stat">
          <span class="stat-num">{skipped.length}</span>
          <span class="stat-lbl">skipped</span>
        </div>
        <div class="stat">
          <span class="stat-num">{missing.length}</span>
          <span class="stat-lbl">not in A2L</span>
        </div>
      </div>

      <div class="tabs">
        <button class="tab" class:on={tab === 'changes'} onclick={() => (tab = 'changes')}>
          Changes <span class="count">{changes.length}</span>
        </button>
        <button class="tab" class:on={tab === 'skipped'} onclick={() => (tab = 'skipped')}
                disabled={!skipped.length}>
          Skipped <span class="count">{skipped.length}</span>
        </button>
        <button class="tab" class:on={tab === 'missing'} onclick={() => (tab = 'missing')}
                disabled={!missing.length}>
          Unknown <span class="count">{missing.length}</span>
        </button>
      </div>

      <div class="list">
        {#if tab === 'changes'}
          {#if !changes.length}
            <p class="empty">Every value in this file already matches the image.</p>
          {:else}
            {#each changes as c}
              <div class="row">
                <span class="nm" title={label(c)}>{label(c)}</span>
                <span class="was">{c.current}</span>
                <span class="arrow">→</span>
                <span class="now">{c.incoming}</span>
                <span class="addr">{hex32(c.address)}</span>
              </div>
            {/each}
          {/if}
        {:else if tab === 'skipped'}
          {#each skipped as s}
            <div class="row">
              <span class="nm" title={s.name}>{s.name}</span>
              <span class="why">{s.reason}</span>
            </div>
          {/each}
        {:else}
          {#each missing as n}
            <div class="row"><span class="nm" title={n}>{n}</span></div>
          {/each}
        {/if}
      </div>

      <div class="actions">
        <button class="btn-cancel" onclick={onClose}>Cancel</button>
        <button class="btn-ok" onclick={onApply} disabled={!changes.length}>
          {changes.length ? `Apply ${changes.length} change${changes.length === 1 ? '' : 's'}` : 'Nothing to apply'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    backdrop-filter: blur(2px);
  }

  .card {
    background: var(--c-raised);
    border: 1px solid var(--c-border2);
    border-radius: 10px;
    padding: 24px 28px 20px;
    width: 620px;
    max-width: calc(100vw - 48px);
    box-shadow: 0 20px 60px rgba(0,0,0,0.6);
    display: flex;
    flex-direction: column;
    gap: 10px;
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
  }

  .title {
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0;
  }

  .subtitle {
    font-size: 12px;
    color: var(--c-muted);
    margin: 0;
    font-family: 'Cascadia Code', 'SF Mono', monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tally {
    display: flex;
    gap: 8px;
  }

  .stat {
    flex: 1;
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 5px;
    padding: 7px 10px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .stat-num {
    font-size: 17px;
    font-weight: 600;
    color: var(--c-text);
    font-variant-numeric: tabular-nums;
  }

  .stat-num.accent { color: var(--c-accent, #4a9eff); }

  .stat-lbl {
    font-size: 10px;
    color: var(--c-muted);
    text-transform: uppercase;
    letter-spacing: 0.4px;
  }

  .tabs {
    display: flex;
    gap: 2px;
    border-bottom: 1px solid var(--c-border);
  }

  .tab {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--c-muted);
    font-family: inherit;
    font-size: 12px;
    padding: 5px 10px;
    cursor: pointer;
    margin-bottom: -1px;
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .tab:hover:not(:disabled) { color: var(--c-text); }
  .tab:disabled { opacity: 0.4; cursor: default; }

  .tab.on {
    color: var(--c-text);
    border-bottom-color: var(--c-accent, #4a9eff);
  }

  .count {
    font-size: 10px;
    background: var(--c-hover);
    border-radius: 8px;
    padding: 0 5px;
    font-variant-numeric: tabular-nums;
  }

  .list {
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 5px;
    height: 260px;
    overflow-y: auto;
    scrollbar-width: thin;
  }

  .row {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 3px 10px;
    font-size: 11px;
    font-family: 'Cascadia Code', 'SF Mono', monospace;
    white-space: nowrap;
  }

  .row:hover { background: var(--c-hover); }

  .nm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--c-text);
  }

  /* Old and new sit in fixed-width columns so a long list reads as two
     columns of values rather than as ragged prose. */
  .was, .now {
    width: 96px;
    text-align: right;
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .was   { color: var(--c-muted); }
  .now   { color: var(--c-accent, #4a9eff); }
  .arrow { color: var(--c-dim); flex-shrink: 0; }

  .addr {
    width: 82px;
    text-align: right;
    flex-shrink: 0;
    color: var(--c-addr, #9cdcfe);
    opacity: 0.75;
  }

  .why {
    color: var(--c-muted);
    flex-shrink: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .empty {
    font-size: 12px;
    color: var(--c-muted);
    margin: 0;
    padding: 14px;
    text-align: center;
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 2px;
  }

  .btn-cancel, .btn-ok {
    font-family: inherit;
    font-size: 12px;
    padding: 5px 14px;
    border-radius: 5px;
    cursor: pointer;
    border: 1px solid var(--c-border2);
  }

  .btn-cancel {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .btn-cancel:hover { background: var(--c-border2); }

  .btn-ok {
    background: var(--c-accent, #4a9eff);
    border-color: var(--c-accent, #4a9eff);
    color: #fff;
  }

  .btn-ok:hover:not(:disabled) { filter: brightness(1.1); }
  .btn-ok:disabled { opacity: 0.45; cursor: not-allowed; }
</style>
