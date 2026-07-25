<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  let {
    open        = false,
    prefillStart = 0,
    prefillEnd   = 0,
    onSelect    = (_opts) => {},
    onClose     = () => {},
  } = $props();

  let startHex = $state('');
  let endHex   = $state('');

  function parseAddr(s) {
    const v = parseInt(s.replace(/^0x/i, '').trim(), 16);
    return isFinite(v) && v >= 0 ? v : null;
  }

  const lo = $derived(parseAddr(startHex));
  const hi = $derived(parseAddr(endHex));
  const rangeValid = $derived(lo !== null && hi !== null && hi >= lo);

  $effect(() => {
    if (open) {
      startHex = '0x' + prefillStart.toString(16).toUpperCase().padStart(8, '0');
      endHex   = '0x' + prefillEnd.toString(16).toUpperCase().padStart(8, '0');
    }
  });

  function doSelect() {
    if (!rangeValid) return;
    onSelect({ start: lo, end: hi });
  }

  function handleKey(e) {
    if (e.key === 'Escape') { onClose(); return; }
    if (e.key === 'Enter' && rangeValid) doSelect();
  }

  function handleBackdrop(e) {
    if (e.target === e.currentTarget) onClose();
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="backdrop" role="dialog" aria-modal="true" aria-label="Select Range"
       onclick={handleBackdrop} onkeydown={handleKey}>
    <div class="card">
      <h2 class="title">Select Range</h2>
      <p class="subtitle">Define a precise address range to select</p>

      <div class="two-col">
        <div class="field-col">
          <label class="mini-label" for="sr-start">Start address</label>
          <input id="sr-start" type="text" class="hex-input"
            class:invalid={startHex.trim() !== '' && lo === null}
            bind:value={startHex} placeholder="0x00000000"
            spellcheck="false" autocomplete="off" />
        </div>
        <div class="field-col">
          <label class="mini-label" for="sr-end">End address</label>
          <input id="sr-end" type="text" class="hex-input"
            class:invalid={endHex.trim() !== '' && (hi === null || (lo !== null && hi < lo))}
            bind:value={endHex} placeholder="0x0000FFFF"
            spellcheck="false" autocomplete="off" />
        </div>
      </div>

      {#if rangeValid}
        <p class="info">{(hi - lo + 1).toLocaleString()} byte{(hi - lo + 1) !== 1 ? 's' : ''} selected</p>
      {:else}
        <p class="info placeholder">Enter a valid start ≤ end address</p>
      {/if}

      <div class="actions">
        <button class="btn-cancel" onclick={onClose}>Cancel</button>
        <button class="btn-ok" onclick={doSelect} disabled={!rangeValid}>Select</button>
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
    width: 360px;
    box-shadow: 0 20px 60px rgba(0,0,0,0.6);
    display: flex;
    flex-direction: column;
    gap: 8px;
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
    margin: 0 0 4px;
  }

  .two-col {
    display: flex;
    gap: 10px;
  }

  .field-col {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .mini-label {
    font-size: 11px;
    color: var(--c-muted);
  }

  .hex-input {
    background: var(--c-bg);
    border: 1px solid var(--c-border2);
    border-radius: 4px;
    color: var(--c-text);
    font-family: 'Cascadia Code', 'SF Mono', monospace;
    font-size: 12px;
    padding: 4px 8px;
    outline: none;
    transition: border-color 0.15s;
    width: 100%;
    box-sizing: border-box;
  }

  .hex-input:focus { border-color: var(--c-accent-b); }
  .hex-input.invalid { border-color: var(--c-err, #e05555); }

  .info {
    font-size: 12px;
    color: var(--c-accent-b);
    margin: 0;
  }

  .info.placeholder {
    color: var(--c-muted);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  .btn-cancel, .btn-ok {
    padding: 5px 16px;
    font-size: 13px;
    border-radius: 5px;
    cursor: pointer;
    font-family: inherit;
    transition: background 0.1s;
  }

  .btn-cancel {
    background: transparent;
    border: 1px solid var(--c-dim);
    color: var(--c-muted);
  }
  .btn-cancel:hover { background: var(--c-hover); color: var(--c-text); }

  .btn-ok {
    background: var(--c-accent-b);
    border: 1px solid transparent;
    color: #fff;
  }
  .btn-ok:hover:not(:disabled) { background: var(--c-accent-h); }
  .btn-ok:disabled { opacity: 0.45; cursor: not-allowed; }
</style>
