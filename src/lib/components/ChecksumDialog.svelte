<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  /**
   * ChecksumDialog — compute a checksum over an address range and insert it.
   *
   * Props
   *   open       – controls visibility
   *   records    – current records array (for live preview)
   *   prefillMin – suggested start address (from selection)
   *   prefillMax – suggested end address (from selection)
   *   onInsert   – called with { lo, hi, algo, targetAddr, width, le }
   *   onClose    – called when the dialog should be dismissed
   */
  import { computeChecksum, getByteAt, numberToBytes } from '$lib/editOps.js';

  let {
    open           = false,
    records        = [],
    prefillMin     = 0,
    prefillMax     = 0,
    prefillTarget  = /** @type {number|null} */ (null),
    onInsert       = (_opts) => {},
    onClose        = () => {},
  } = $props();

  let startHex  = $state('');
  let endHex    = $state('');
  let targetHex = $state('');
  let algo      = $state(/** @type {'xor'|'sum8'|'crc16'|'crc32'} */ ('crc32'));
  let width     = $state(4);
  let le        = $state(true);

  function parseAddr(s) {
    const v = parseInt(s.replace(/^0x/i, '').trim(), 16);
    return isFinite(v) && v >= 0 ? v : null;
  }

  const lo         = $derived(parseAddr(startHex));
  const hi         = $derived(parseAddr(endHex));
  const targetAddr = $derived(parseAddr(targetHex));

  const rangeValid  = $derived(lo !== null && hi !== null && hi >= lo);
  const targetValid = $derived(targetAddr !== null);

  const algoMaxWidth   = $derived(algo === 'xor' || algo === 'sum8' ? 1 : algo === 'crc16' ? 2 : 4);
  const effectiveWidth = $derived(Math.min(width, algoMaxWidth));

  // Live preview is skipped for very large address spans to keep the UI responsive.
  // CRC over zero-filled gaps is O(span), so spans beyond this limit would stall.
  const PREVIEW_LIMIT = 16 * 1024 * 1024; // 16 MB
  const spanTooLarge  = $derived(rangeValid && (hi - lo + 1) > PREVIEW_LIMIT);

  const checksumVal = $derived.by(() => {
    if (!rangeValid || spanTooLarge || records.length === 0) return null;
    return computeChecksum(records, lo, hi, algo);
  });

  const checksumBytes = $derived.by(() => {
    if (!rangeValid)    return '—';
    if (spanTooLarge)   return '(span > 16 MB)';
    if (checksumVal === null) return '—';
    return numberToBytes(checksumVal, effectiveWidth, le)
      .map(b => b.toString(16).toUpperCase().padStart(2, '0'))
      .join(' ');
  });

  const checksumFull = $derived.by(() => {
    if (!rangeValid || spanTooLarge || checksumVal === null) return null;
    return '0x' + checksumVal.toString(16).toUpperCase().padStart(effectiveWidth * 2, '0');
  });

  const actualContent = $derived.by(() => {
    if (!targetValid) return null;
    const parts = [];
    for (let i = 0; i < effectiveWidth; i++) {
      const b = getByteAt(records, targetAddr + i);
      parts.push(b !== null ? b.toString(16).toUpperCase().padStart(2, '0') : '--');
    }
    return parts.join(' ');
  });

  const canInsert = $derived(rangeValid && targetValid);

  // Reset + pre-fill when dialog opens
  $effect(() => {
    if (open) {
      startHex  = '0x' + prefillMin.toString(16).toUpperCase().padStart(8, '0');
      endHex    = '0x' + prefillMax.toString(16).toUpperCase().padStart(8, '0');
      targetHex = prefillTarget !== null
        ? '0x' + prefillTarget.toString(16).toUpperCase().padStart(8, '0')
        : '';
      algo  = 'crc32';
      width = 4;
      le    = true;
    }
  });

  // Clamp width to algorithm max when algo changes
  $effect(() => {
    if (width > algoMaxWidth) width = algoMaxWidth;
  });

  function doInsert() {
    if (!canInsert) return;
    onInsert({ lo, hi, algo, targetAddr, width: Math.min(width, algoMaxWidth), le });
  }

  function handleKey(e) {
    if (e.key === 'Escape') { onClose(); return; }
    if (e.key === 'Enter' && canInsert) doInsert();
  }

  function handleBackdrop(e) {
    if (e.target === e.currentTarget) onClose();
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="backdrop" role="dialog" aria-modal="true" aria-label="Insert Checksum"
       onclick={handleBackdrop} onkeydown={handleKey}>
    <div class="card">
      <h2 class="title">Insert Checksum</h2>
      <p class="subtitle">Compute a checksum over an address range and write it to memory</p>

      <!-- Source range -->
      <div class="section-label">Source range</div>
      <div class="two-col">
        <div class="field-col">
          <label class="mini-label" for="cs-start">Start address</label>
          <input id="cs-start" type="text" class="hex-input"
            class:invalid={startHex.trim() !== '' && lo === null}
            bind:value={startHex} placeholder="0x00000000"
            spellcheck="false" autocomplete="off" />
        </div>
        <div class="field-col">
          <label class="mini-label" for="cs-end">End address</label>
          <input id="cs-end" type="text" class="hex-input"
            class:invalid={endHex.trim() !== '' && (hi === null || (lo !== null && hi < lo))}
            bind:value={endHex} placeholder="0x0000FFFF"
            spellcheck="false" autocomplete="off" />
        </div>
      </div>

      <!-- Algorithm -->
      <div class="section-label">Algorithm</div>
      <div class="algo-group">
        {#each [
          { v: 'xor',   label: 'XOR',        maxW: 1 },
          { v: 'sum8',  label: 'Sum 8-bit',   maxW: 1 },
          { v: 'crc16', label: 'CRC-16/CCITT',maxW: 2 },
          { v: 'crc32', label: 'CRC-32',      maxW: 4 },
        ] as a}
          <label class="algo-opt" class:selected={algo === a.v}>
            <input type="radio" name="cs-algo" value={a.v} bind:group={algo}>
            {a.label}
          </label>
        {/each}
      </div>

      <!-- Width + Endianness -->
      <div class="row-pair">
        <div class="field-inline">
          <span class="field-label">Width (bytes)</span>
          <div class="width-group">
            {#each [1, 2, 4] as w}
              <label class="width-opt"
                     class:selected={width === w}
                     class:disabled={w > algoMaxWidth}>
                <input type="radio" name="cs-width" value={w} bind:group={width}
                       disabled={w > algoMaxWidth}>
                {w}
              </label>
            {/each}
          </div>
        </div>
        <div class="field-inline">
          <span class="field-label">Byte order</span>
          <div class="endian-group">
            <label class="endian-opt" class:selected={le}>
              <input type="radio" name="cs-endian" value={true} bind:group={le}>LE</label>
            <label class="endian-opt" class:selected={!le}>
              <input type="radio" name="cs-endian" value={false} bind:group={le}>BE</label>
          </div>
        </div>
      </div>

      <!-- Target address -->
      <div class="section-label">Target address</div>
      <input type="text" class="hex-input full"
        class:invalid={targetHex.trim() !== '' && targetAddr === null}
        bind:value={targetHex} placeholder="0x00010000 — write checksum here"
        spellcheck="false" autocomplete="off" />

      <!-- Actual content at target address -->
      <div class="preview-row">
        <span class="preview-label">Actual content</span>
        <span class="preview-val actual" class:valid={targetValid}>{actualContent ?? '—'}</span>
        {#if targetValid}
          <span class="preview-note">{effectiveWidth} byte{effectiveWidth > 1 ? 's' : ''} at 0x{targetAddr.toString(16).toUpperCase().padStart(8, '0')}</span>
        {/if}
      </div>

      <!-- Live preview -->
      <div class="preview-row">
        <span class="preview-label">Preview</span>
        <span class="preview-val" class:valid={rangeValid}>{checksumBytes ?? '—'}</span>
        {#if rangeValid && !spanTooLarge && checksumFull !== null}
          <span class="preview-note">
            {checksumFull} ({algo.toUpperCase()}, {effectiveWidth} byte{effectiveWidth > 1 ? 's' : ''}, {le ? 'LE' : 'BE'})
          </span>
        {/if}
      </div>

      <div class="actions">
        <button class="btn-cancel" onclick={onClose}>Cancel</button>
        <button class="btn-ok" onclick={doInsert} disabled={!canInsert}>Insert</button>
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
    width: 400px;
    box-shadow: 0 20px 60px rgba(0,0,0,0.6);
    display: flex;
    flex-direction: column;
    gap: 6px;
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

  .section-label {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--c-muted);
    margin-top: 6px;
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
  .hex-input.full { width: 100%; }

  .algo-group {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .algo-opt {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--c-muted);
    cursor: pointer;
    padding: 3px 10px;
    border: 1px solid var(--c-border2);
    border-radius: 4px;
    transition: background 0.1s, color 0.1s;
  }

  .algo-opt.selected {
    background: var(--c-accent-b);
    border-color: var(--c-accent-b);
    color: #fff;
  }

  .algo-opt input[type="radio"] { display: none; }

  .row-pair {
    display: flex;
    gap: 16px;
    align-items: center;
  }

  .field-inline {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .field-label {
    font-size: 12px;
    color: var(--c-text);
    white-space: nowrap;
  }

  .width-group, .endian-group {
    display: flex;
    gap: 4px;
  }

  .width-opt, .endian-opt {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 28px;
    height: 24px;
    padding: 0 6px;
    border: 1px solid var(--c-dim);
    border-radius: 4px;
    font-size: 12px;
    color: var(--c-muted);
    cursor: pointer;
    transition: background 0.1s, color 0.1s;
  }

  .width-opt input[type="radio"],
  .endian-opt input[type="radio"] { display: none; }

  .width-opt.selected, .endian-opt.selected {
    background: var(--c-accent-b);
    border-color: var(--c-accent-b);
    color: #fff;
  }

  .width-opt.disabled {
    opacity: 0.3;
    cursor: not-allowed;
    pointer-events: none;
  }

  .preview-row {
    display: grid;
    grid-template-columns: 90px 1fr;
    column-gap: 8px;
    row-gap: 2px;
    align-items: baseline;
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 5px;
    padding: 6px 10px;
    margin-top: 4px;
  }

  .preview-label {
    font-size: 11px;
    color: var(--c-muted);
  }

  .preview-val {
    font-family: 'Cascadia Code', 'SF Mono', monospace;
    font-size: 13px;
    color: var(--c-dim);
    font-weight: 600;
  }

  .preview-val.valid {
    color: var(--c-accent-b);
  }

  .preview-val.actual.valid {
    color: var(--c-text);
  }

  .preview-note {
    grid-column: 2;
    font-size: 11px;
    color: var(--c-muted);
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
