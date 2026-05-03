<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  /**
   * ImportMergeDialog — import an external IHex/SREC/binary file and merge
   * its data into the current file.
   *
   * Props
   *   open      – controls visibility
   *   onMerge   – called with { importedRecords: Array, mode: 'overwrite'|'fill-empty' }
   *   onClose   – called when the dialog should be dismissed
   */
  import { parseFile } from '$lib/api.js';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';

  let {
    open    = false,
    onMerge = (_opts) => {},
    onClose = () => {},
  } = $props();

  let selectedPath    = $state('');
  let importedRecords = $state(/** @type {Array|null} */ (null));
  let mode            = $state(/** @type {'overwrite'|'fill-empty'} */ ('overwrite'));
  let loading         = $state(false);
  let loadError       = $state('');

  // Derived segment summary from imported records
  const segments = $derived.by(() => {
    if (!importedRecords) return [];
    const dataRecs = importedRecords.filter(r =>
      (r.record_type === 'Data' || r.record_type === 'S1' || r.record_type === 'S2' || r.record_type === 'S3')
      && r.data.length > 0
    ).sort((a, b) => a.address - b.address);

    // Merge contiguous into segments
    const segs = [];
    for (const rec of dataRecs) {
      if (segs.length === 0) {
        segs.push({ start: rec.address, end: rec.address + rec.data.length - 1, bytes: rec.data.length });
      } else {
        const last = segs[segs.length - 1];
        if (rec.address === last.end + 1) {
          last.end = rec.address + rec.data.length - 1;
          last.bytes += rec.data.length;
        } else {
          segs.push({ start: rec.address, end: rec.address + rec.data.length - 1, bytes: rec.data.length });
        }
      }
    }
    return segs;
  });

  const totalBytes = $derived(segments.reduce((s, r) => s + r.bytes, 0));

  // Reset state each time dialog opens
  $effect(() => {
    if (open) {
      selectedPath    = '';
      importedRecords = null;
      mode            = 'overwrite';
      loading         = false;
      loadError       = '';
    }
  });

  async function pickFile() {
    const path = await openDialog({
      multiple: false,
      filters: [
        { name: 'Firmware files', extensions: ['hex', 'ihex', 'srec', 'mot', 's19', 's28', 's37', 'bin'] },
        { name: 'All files', extensions: ['*'] },
      ],
    });
    if (!path) return;
    await loadFile(path);
  }

  async function loadFile(path) {
    loading      = true;
    loadError    = '';
    importedRecords = null;
    selectedPath = path;
    try {
      const recs = await parseFile(path);
      importedRecords = recs;
    } catch (e) {
      loadError = String(e);
    } finally {
      loading = false;
    }
  }

  function doMerge() {
    if (!importedRecords) return;
    onMerge({ importedRecords, mode });
  }

  function handleBackdrop(e) {
    if (e.target === e.currentTarget) onClose();
  }

  function handleKey(e) {
    if (e.key === 'Escape') { onClose(); return; }
    if (e.key === 'Enter' && importedRecords && !loading) doMerge();
  }

  function hex32(n) { return '0x' + n.toString(16).toUpperCase().padStart(8, '0'); }
  function fileName(p) { return p.split(/[\\/]/).at(-1) ?? p; }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="backdrop" role="dialog" aria-modal="true" aria-label="Import from File"
       onclick={handleBackdrop} onkeydown={handleKey}>
    <div class="card">
      <h2 class="title">Import from File</h2>
      <p class="subtitle">Merge data from an IHex, S-record, or binary file into the current file</p>

      <!-- File picker -->
      <div class="pick-row">
        <span class="pick-path" class:placeholder={!selectedPath}>
          {selectedPath ? fileName(selectedPath) : 'No file selected'}
        </span>
        <button class="btn-pick" onclick={pickFile} disabled={loading}>Browse…</button>
      </div>

      <!-- Loading / error -->
      {#if loading}
        <p class="status-msg">Loading…</p>
      {:else if loadError}
        <p class="error-msg">{loadError}</p>
      {:else if importedRecords !== null}
        <!-- Segment summary -->
        <div class="summary-box">
          <div class="summary-header">
            {segments.length} segment{segments.length === 1 ? '' : 's'} &nbsp;·&nbsp;
            {totalBytes.toLocaleString()} bytes
          </div>
          <div class="seg-list">
            {#each segments as seg}
              <div class="seg-row">
                <span class="seg-addr">{hex32(seg.start)} – {hex32(seg.end)}</span>
                <span class="seg-size">{seg.bytes.toLocaleString()} B</span>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Write mode -->
      <div class="field-row">
        <span class="field-label">Write mode</span>
        <div class="radio-group">
          <label class="radio-opt" class:selected={mode === 'overwrite'}>
            <input type="radio" name="im-mode" value="overwrite" bind:group={mode}>
            Overwrite existing
          </label>
          <label class="radio-opt" class:selected={mode === 'fill-empty'}>
            <input type="radio" name="im-mode" value="fill-empty" bind:group={mode}>
            Fill empty only
          </label>
        </div>
      </div>

      <div class="actions">
        <button class="btn-cancel" onclick={onClose}>Cancel</button>
        <button class="btn-ok" onclick={doMerge} disabled={!importedRecords || loading}>
          Merge
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
    width: 400px;
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

  .pick-row {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--c-bg);
    border: 1px solid var(--c-border2);
    border-radius: 5px;
    padding: 4px 4px 4px 10px;
  }

  .pick-path {
    flex: 1;
    font-size: 12px;
    color: var(--c-text);
    font-family: 'Cascadia Code', 'SF Mono', monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pick-path.placeholder {
    color: var(--c-dim);
  }

  .btn-pick {
    flex-shrink: 0;
    background: var(--c-hover);
    border: 1px solid var(--c-border2);
    border-radius: 4px;
    color: var(--c-text);
    font-size: 12px;
    padding: 3px 10px;
    cursor: pointer;
    font-family: inherit;
    transition: background 0.1s;
  }

  .btn-pick:hover:not(:disabled) { background: var(--c-border2); }
  .btn-pick:disabled { opacity: 0.45; cursor: not-allowed; }

  .status-msg {
    font-size: 12px;
    color: var(--c-muted);
    margin: 0;
  }

  .error-msg {
    font-size: 11px;
    color: var(--c-err, #e05555);
    margin: 0;
  }

  .summary-box {
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 5px;
    overflow: hidden;
  }

  .summary-header {
    font-size: 11px;
    color: var(--c-muted);
    padding: 5px 10px;
    border-bottom: 1px solid var(--c-border);
    background: var(--c-surface);
  }

  .seg-list {
    max-height: 120px;
    overflow-y: auto;
    scrollbar-width: thin;
  }

  .seg-row {
    display: flex;
    justify-content: space-between;
    padding: 3px 10px;
    font-size: 11px;
    font-family: 'Cascadia Code', 'SF Mono', monospace;
  }

  .seg-row:hover { background: var(--c-hover); }

  .seg-addr { color: var(--c-addr, #9cdcfe); }
  .seg-size { color: var(--c-muted); }

  .field-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .field-label {
    font-size: 13px;
    color: var(--c-text);
    flex-shrink: 0;
    min-width: 80px;
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

  .radio-opt input[type="radio"] { accent-color: var(--c-accent-b); }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 6px;
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
