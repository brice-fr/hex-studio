<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  /**
   * FillDialog — fill a byte selection with a repeated pattern or random data.
   *
   * Props
   *   open      – controls visibility
   *   selMin    – inclusive start address of the selection
   *   selMax    – inclusive end address of the selection
   *   onFill    – called with { pattern: Uint8Array|null, randomize: boolean, mode: 'overwrite'|'fill-empty' }
   *   onClose   – called when the dialog should be dismissed
   */
  import { parseHexPattern } from '$lib/editOps.js';

  let {
    open     = false,
    selMin   = 0,
    selMax   = 0,
    onFill   = (_opts) => {},
    onClose  = () => {},
  } = $props();

  const selCount = $derived(selMax - selMin + 1);

  let patternHex = $state('FF');
  let randomize  = $state(false);
  let mode       = $state(/** @type {'overwrite'|'fill-empty'} */ ('overwrite'));

  const pattern = $derived(randomize ? new Uint8Array([0xFF]) : parseHexPattern(patternHex));
  const patternValid = $derived(randomize || pattern !== null);
  const truncated    = $derived(!randomize && pattern !== null && selCount % pattern.length !== 0);

  // Reset state each time the dialog opens
  $effect(() => {
    if (open) {
      patternHex = 'FF';
      randomize  = false;
      mode       = 'overwrite';
    }
  });

  function doFill() {
    if (!patternValid) return;
    onFill({ pattern: randomize ? null : pattern, randomize, mode });
  }

  function handleKey(e) {
    if (e.key === 'Escape') { onClose(); return; }
    if (e.key === 'Enter' && patternValid) doFill();
  }

  function handleBackdrop(e) {
    if (e.target === e.currentTarget) onClose();
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="backdrop" role="dialog" aria-modal="true" aria-label="Fill Selection"
       onclick={handleBackdrop} onkeydown={handleKey}>
    <div class="card">
      <h2 class="title">Fill Selection</h2>
      <p class="subtitle">
        0x{selMin.toString(16).toUpperCase().padStart(8,'0')} –
        0x{selMax.toString(16).toUpperCase().padStart(8,'0')}
        &nbsp;·&nbsp; {selCount.toLocaleString()} byte{selCount === 1 ? '' : 's'}
      </p>

      <!-- Pattern input -->
      <div class="field-row" class:disabled={randomize}>
        <label class="field-label" for="fill-pattern">Pattern (hex)</label>
        <input
          id="fill-pattern"
          type="text"
          class="hex-input"
          class:invalid={!randomize && !patternValid}
          bind:value={patternHex}
          disabled={randomize}
          placeholder="FF or DE AD BE EF"
          spellcheck="false"
          autocomplete="off"
        />
      </div>

      {#if truncated}
        <p class="warn-msg">
          ⚠ Pattern length ({pattern.length}) does not divide evenly into {selCount} bytes — last repeat will be truncated.
        </p>
      {/if}

      <label class="checkbox-row">
        <input type="checkbox" bind:checked={randomize}>
        Randomize (ignore pattern)
      </label>

      <div class="field-row top-gap">
        <span class="field-label">Write mode</span>
        <div class="radio-group">
          <label class="radio-opt" class:selected={mode === 'overwrite'}>
            <input type="radio" name="fill-mode" value="overwrite" bind:group={mode}>
            Overwrite existing
          </label>
          <label class="radio-opt" class:selected={mode === 'fill-empty'}>
            <input type="radio" name="fill-mode" value="fill-empty" bind:group={mode}>
            Fill empty only
          </label>
        </div>
      </div>

      <div class="actions">
        <button class="btn-cancel" onclick={onClose}>Cancel</button>
        <button class="btn-ok" onclick={doFill} disabled={!patternValid}>Fill</button>
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

  .field-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .field-row.top-gap {
    margin-top: 4px;
  }

  .field-row.disabled {
    opacity: 0.4;
    pointer-events: none;
  }

  .field-label {
    font-size: 13px;
    color: var(--c-text);
    flex-shrink: 0;
    min-width: 90px;
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

  .hex-input:focus {
    border-color: var(--c-accent-b);
  }

  .hex-input.invalid {
    border-color: var(--c-err, #e05555);
  }

  .hex-input:disabled {
    opacity: 0.4;
  }

  .warn-msg {
    font-size: 11px;
    color: #d09030;
    margin: 0;
    line-height: 1.4;
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--c-text);
    cursor: pointer;
  }

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

  .radio-opt input[type="radio"] {
    accent-color: var(--c-accent-b);
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
