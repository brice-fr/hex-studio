<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  /**
   * MoveDialog — move (cut + paste) a byte selection to a new address.
   *
   * Props
   *   open         – controls visibility
   *   sourceMin    – inclusive start address of the selection
   *   sourceMax    – inclusive end address of the selection
   *   onMove       – called with { targetAddr: number, mode: 'overwrite'|'fill-empty' }
   *   onClose      – called when the dialog should be dismissed
   */

  let {
    open      = false,
    sourceMin = 0,
    sourceMax = 0,
    onMove    = (_opts) => {},
    onClose   = () => {},
  } = $props();

  const selCount = $derived(sourceMax - sourceMin + 1);

  let targetHex  = $state('');
  let mode       = $state(/** @type {'overwrite'|'fill-empty'} */ ('overwrite'));
  let inputError = $state('');

  const targetAddr = $derived.by(() => {
    const stripped = targetHex.replace(/^0x/i, '').trim();
    if (stripped === '') return null;
    const v = parseInt(stripped, 16);
    return isFinite(v) && v >= 0 ? v : null;
  });

  const canConfirm = $derived(targetAddr !== null && targetHex.trim() !== '');

  // Reset state each time the dialog opens
  $effect(() => {
    if (open) {
      targetHex  = '';
      mode       = 'overwrite';
      inputError = '';
    }
  });

  function doMove() {
    if (!canConfirm) { inputError = 'Enter a valid hex address.'; return; }
    onMove({ targetAddr, mode });
  }

  function handleKey(e) {
    if (e.key === 'Escape') { onClose(); return; }
    if (e.key === 'Enter' && canConfirm) doMove();
  }

  function handleBackdrop(e) {
    if (e.target === e.currentTarget) onClose();
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="backdrop" role="dialog" aria-modal="true" aria-label="Move Selection"
       onclick={handleBackdrop} onkeydown={handleKey}>
    <div class="card">
      <h2 class="title">Move Selection</h2>
      <p class="subtitle">
        0x{sourceMin.toString(16).toUpperCase().padStart(8,'0')} –
        0x{sourceMax.toString(16).toUpperCase().padStart(8,'0')}
        &nbsp;·&nbsp; {selCount.toLocaleString()} byte{selCount === 1 ? '' : 's'}
      </p>

      <div class="field-row">
        <label class="field-label" for="move-target">Target address</label>
        <input
          id="move-target"
          type="text"
          class="hex-input"
          class:invalid={targetHex.trim() !== '' && targetAddr === null}
          bind:value={targetHex}
          placeholder="0x00001000"
          spellcheck="false"
          autocomplete="off"
        />
      </div>

      <p class="hint-text">
        The source bytes will be deleted and written at the target address.
      </p>

      <div class="field-row top-gap">
        <span class="field-label">Write mode</span>
        <div class="radio-group">
          <label class="radio-opt" class:selected={mode === 'overwrite'}>
            <input type="radio" name="move-mode" value="overwrite" bind:group={mode}>
            Overwrite existing
          </label>
          <label class="radio-opt" class:selected={mode === 'fill-empty'}>
            <input type="radio" name="move-mode" value="fill-empty" bind:group={mode}>
            Fill empty only
          </label>
        </div>
      </div>

      {#if inputError}
        <p class="error-msg">{inputError}</p>
      {/if}

      <div class="actions">
        <button class="btn-cancel" onclick={onClose}>Cancel</button>
        <button class="btn-ok" onclick={doMove} disabled={!canConfirm}>Move</button>
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
    width: 340px;
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
    font-size: 11px;
    color: var(--c-muted);
    margin: 0 0 4px;
    font-family: 'Cascadia Code', 'SF Mono', monospace;
  }

  .hint-text {
    font-size: 11px;
    color: var(--c-muted);
    margin: 0;
    line-height: 1.4;
  }

  .field-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .field-row.top-gap { margin-top: 4px; }

  .field-label {
    font-size: 13px;
    color: var(--c-text);
    flex-shrink: 0;
    min-width: 100px;
  }

  .hex-input {
    flex: 1;
    background: var(--c-bg);
    border: 1px solid var(--c-border2);
    border-radius: 4px;
    color: var(--c-text);
    font-family: 'Cascadia Code', 'SF Mono', monospace;
    font-size: 13px;
    padding: 4px 8px;
    outline: none;
    transition: border-color 0.15s;
  }

  .hex-input:focus { border-color: var(--c-accent-b); }
  .hex-input.invalid { border-color: var(--c-err, #e05555); }

  .radio-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .radio-opt {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--c-muted);
    cursor: pointer;
    padding: 3px 8px;
    border-radius: 4px;
    transition: background 0.1s;
  }

  .radio-opt.selected {
    color: var(--c-text);
    background: var(--c-hover);
  }

  .radio-opt input[type="radio"] { accent-color: var(--c-accent-b); }

  .error-msg {
    font-size: 11px;
    color: var(--c-err, #e05555);
    margin: 0;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
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
