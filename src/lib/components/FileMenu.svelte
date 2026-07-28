<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  let {
    // ── File + navigation ────────────────────────────────────────────────────
    onOpen     = () => {}, onSave     = () => {}, onExport  = () => {},
    onFind     = () => {}, onGoto     = () => {}, onCompare = () => {},
    onSettings = () => {},
    loading = false, saving = false, hasFile = false,
    // ── Edit operations ───────────────────────────────────────────────────────
    onUndo       = () => {}, onRedo       = () => {},
    onFill        = () => {}, onMove        = () => {},
    onChecksum    = () => {}, onImportMerge = () => {},
    onSelectRange = () => {},
    canUndo       = false,    canRedo       = false,
    hasSelection  = false,
    // ── A2L data view ─────────────────────────────────────────────────────────
    viewMode    = 'hex',            // 'hex' | 'data'
    onViewMode  = (_mode) => {},
    a2lName     = '',               // basename of the loaded A2L, '' when none
    a2lLoading  = false,
    onLoadA2l   = () => {},
    onUnloadA2l = () => {},
    // ── CDFX calibration data ─────────────────────────────────────────────────
    cdfxReady    = false,           // an A2L *and* an image are both loaded
    cdfxBusy     = false,
    onImportCdfx = () => {},
    onExportCdfx = () => {},
  } = $props();
</script>

<div class="toolbar">

  <!-- ── Open ── -->
  <button class="icon-btn" onclick={() => onOpen()} disabled={loading || saving}
          title="Open file… (⌘O)" aria-label="Open file">
    {#if loading}
      <svg class="spin" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
           stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
      </svg>
    {:else}
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
           stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
        <polyline points="16 13 12 9 8 13"/>
        <line x1="12" y1="9" x2="12" y2="17"/>
      </svg>
    {/if}
  </button>

  <!-- ── Save as ── -->
  <button class="icon-btn" onclick={() => onSave()} disabled={loading || saving || !hasFile}
          title="Save as… (⌘⇧S)" aria-label="Save file as">
    {#if saving}
      <svg class="spin" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
           stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
      </svg>
    {:else}
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
           stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/>
        <polyline points="17 21 17 13 7 13 7 21"/>
        <polyline points="7 3 7 8 15 8"/>
      </svg>
    {/if}
  </button>

  <!-- ── Export as HTML ── -->
  <button class="icon-btn" onclick={() => onExport()} disabled={!hasFile}
          title="Export as HTML report…" aria-label="Export as HTML report">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
      <polyline points="14 2 14 8 20 8"/>
      <line x1="12" y1="12" x2="12" y2="18"/>
      <polyline points="9 15 12 18 15 15"/>
    </svg>
  </button>

  <div class="divider"></div>

  <!-- ── Find ── -->
  <button class="icon-btn" onclick={() => onFind()} disabled={!hasFile}
          title="Find… (⌘F)" aria-label="Find">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="11" cy="11" r="7"/>
      <line x1="16.5" y1="16.5" x2="22" y2="22"/>
    </svg>
  </button>

  <!-- ── Go to address ── -->
  <button class="icon-btn" onclick={() => onGoto()} disabled={!hasFile}
          title="Go to address… (⌘G)" aria-label="Go to address">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <line x1="12" y1="3" x2="12" y2="15"/>
      <polyline points="7 10 12 15 17 10"/>
      <line x1="3" y1="20" x2="21" y2="20"/>
    </svg>
  </button>

  <div class="divider"></div>

  <!-- ── Compare ── -->
  <button class="icon-btn" onclick={() => onCompare()} disabled={!hasFile}
          title="Compare with…" aria-label="Compare with another file">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <rect x="3" y="4" width="18" height="16" rx="2"/>
      <line x1="12" y1="4" x2="12" y2="20"/>
      <line x1="5.5" y1="9"  x2="10" y2="9"/>
      <line x1="5.5" y1="13" x2="10" y2="13"/>
      <line x1="14" y1="9"  x2="18.5" y2="9"/>
      <line x1="14" y1="13" x2="18.5" y2="13"/>
    </svg>
  </button>

  <div class="divider"></div>

  <!-- ── View mode ── data half stays disabled until an A2L is loaded -->
  <div class="seg" role="group" aria-label="View mode">
    <button
      class="seg-btn"
      class:on={viewMode === 'hex'}
      onclick={() => onViewMode('hex')}
      title="Hex and ASCII view"
    >hex</button>
    <button
      class="seg-btn"
      class:on={viewMode === 'data'}
      onclick={() => onViewMode('data')}
      disabled={!a2lName}
      title={a2lName ? 'Physical data view' : 'Load an A2L file to enable the data view'}
    >data</button>
  </div>

  <!-- ── A2L slot ── drop target when empty, chip when loaded -->
  {#if a2lName}
    <div class="a2l-chip" title={a2lName}>
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
           stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
        <polyline points="14 2 14 8 20 8"/>
        <polyline points="9 15 11 17 15 13"/>
      </svg>
      <span class="a2l-name">{a2lName}</span>
      <button class="a2l-x" onclick={() => onUnloadA2l()}
              title="Unload A2L" aria-label="Unload A2L file">×</button>
    </div>
  {:else}
    <button class="a2l-drop" onclick={() => onLoadA2l()} disabled={a2lLoading}
            title="Load associated A2L to enable data view — or drop an .a2l file anywhere">
      {#if a2lLoading}
        <svg class="spin" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
        </svg>
        <span>Loading…</span>
      {:else}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M12 3v12"/>
          <polyline points="8 11 12 15 16 11"/>
          <path d="M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2"/>
        </svg>
        <span>Drop A2L</span>
      {/if}
    </button>
  {/if}

  <!-- ── CDFX import / export ── both need an A2L to name the parameters and
       an image to hold them, so neither is offered until both are present -->
  <button class="icon-btn" onclick={() => onImportCdfx()} disabled={!cdfxReady || cdfxBusy}
          title={cdfxReady ? 'Import calibration data (CDFX)…' : 'Load a hex file and an A2L to import calibration data'}
          aria-label="Import calibration data">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M4 4h7l2 2h7a1 1 0 0 1 1 1v3"/>
      <path d="M3 7v12a1 1 0 0 0 1 1h9"/>
      <path d="M18 13v8"/>
      <polyline points="15 18 18 21 21 18"/>
    </svg>
  </button>

  <button class="icon-btn" onclick={() => onExportCdfx()} disabled={!cdfxReady || cdfxBusy}
          title={cdfxReady ? 'Export calibration data (CDFX)…' : 'Load a hex file and an A2L to export calibration data'}
          aria-label="Export calibration data">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M4 4h7l2 2h7a1 1 0 0 1 1 1v3"/>
      <path d="M3 7v12a1 1 0 0 0 1 1h9"/>
      <path d="M18 21v-8"/>
      <polyline points="15 16 18 13 21 16"/>
    </svg>
  </button>

  <div class="divider"></div>

  <!-- ── Undo ── enabled when there is history to undo -->
  <button class="icon-btn" onclick={() => onUndo()} disabled={!canUndo}
          title="Undo (⌘Z)" aria-label="Undo">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M9 14 4 9l5-5"/>
      <path d="M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5v0a5.5 5.5 0 0 1-5.5 5.5H11"/>
    </svg>
  </button>

  <!-- ── Redo ── enabled when there is history to redo -->
  <button class="icon-btn" onclick={() => onRedo()} disabled={!canRedo}
          title="Redo (⌘⇧Z)" aria-label="Redo">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="m15 14 5-5-5-5"/>
      <path d="M20 9H9.5A5.5 5.5 0 0 0 4 14.5v0A5.5 5.5 0 0 0 9.5 20H13"/>
    </svg>
  </button>

  <div class="divider"></div>

  <!-- ── Select range ── enabled when a file is loaded -->
  <button class="icon-btn" onclick={() => onSelectRange()} disabled={!hasFile}
          title="Select range… (⌘⇧A)" aria-label="Select address range">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <!-- Selection bracket left -->
      <path d="M7 4H4v16h3"/>
      <!-- Selection bracket right -->
      <path d="M17 4h3v16h-3"/>
      <!-- Horizontal range line -->
      <line x1="7" y1="12" x2="17" y2="12"/>
      <!-- Small tick marks -->
      <line x1="7"  y1="9" x2="7"  y2="15"/>
      <line x1="17" y1="9" x2="17" y2="15"/>
    </svg>
  </button>

  <!-- ── Fill selection ── enabled when bytes are selected -->
  <button class="icon-btn" onclick={() => onFill()} disabled={!hasSelection}
          title="Fill selection…" aria-label="Fill selection">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <!-- Paint bucket -->
      <path d="m19 11-8-8-8.5 8.5a5.5 5.5 0 0 0 7.78 7.78L19 11z"/>
      <path d="m5 2 5 5"/>
      <path d="M2 13h15"/>
      <!-- Drop -->
      <path d="M22 20a2 2 0 1 1-4 0c0-1.6 1.7-2.4 2-4 .3 1.6 2 2.4 2 4z"/>
    </svg>
  </button>

  <!-- ── Move selection ── enabled when bytes are selected -->
  <button class="icon-btn" onclick={() => onMove()} disabled={!hasSelection}
          title="Move selection…" aria-label="Move selection to new address">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <!-- Horizontal move arrows -->
      <polyline points="14 8 18 12 14 16"/>
      <polyline points="10 8 6 12 10 16"/>
      <line x1="6" y1="12" x2="18" y2="12"/>
      <!-- baseline divider hint -->
      <line x1="4" y1="4" x2="4" y2="20" stroke-dasharray="2 2"/>
      <line x1="20" y1="4" x2="20" y2="20" stroke-dasharray="2 2"/>
    </svg>
  </button>

  <!-- ── Insert checksum ── enabled when a file is loaded -->
  <button class="icon-btn" onclick={() => onChecksum()} disabled={!hasFile}
          title="Insert checksum…" aria-label="Insert checksum">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <!-- Hash / CRC symbol -->
      <line x1="4"  y1="9"  x2="20" y2="9"/>
      <line x1="4"  y1="15" x2="20" y2="15"/>
      <line x1="10" y1="3"  x2="8"  y2="21"/>
      <line x1="16" y1="3"  x2="14" y2="21"/>
    </svg>
  </button>

  <!-- ── Import / merge from file ── enabled when a file is loaded -->
  <button class="icon-btn" onclick={() => onImportMerge()} disabled={!hasFile}
          title="Import and merge from file…" aria-label="Import and merge from file">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <!-- Document outline -->
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
      <polyline points="14 2 14 8 20 8"/>
      <!-- Two arrows merging into doc body -->
      <polyline points="9 13 12 16 15 13"/>
      <line x1="9"  y1="10" x2="9"  y2="16"/>
      <line x1="15" y1="10" x2="15" y2="16"/>
    </svg>
  </button>

  <!-- Auto-spacer pushes Settings to the far right -->
  <div class="divider" style="margin-left: auto;"></div>

  <!-- ── Settings ── always enabled -->
  <button class="icon-btn" onclick={() => onSettings()} title="Settings…" aria-label="Settings">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="3"/>
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
    </svg>
  </button>

</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    background: #2d2d2d;
    border-bottom: 1px solid #3c3c3c;
    height: 36px;
  }

  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    background: transparent;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    color: #cccccc;
    padding: 0;
    transition: background 0.1s, color 0.1s;
    flex-shrink: 0;
  }

  .icon-btn:hover:not(:disabled) {
    background: #3c3c3c;
    color: #ffffff;
  }

  .icon-btn:active:not(:disabled) {
    background: #4a4a4a;
  }

  .icon-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .icon-btn svg {
    width: 18px;
    height: 18px;
  }

  .spin {
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .divider {
    width: 1px;
    height: 18px;
    background: #3c3c3c;
    margin: 0 4px;
    flex-shrink: 0;
  }

  /* ── View-mode segmented control ── */
  .seg {
    display: flex;
    border: 1px solid #4a4a4a;
    border-radius: 4px;
    overflow: hidden;
    flex-shrink: 0;
  }

  .seg-btn {
    background: transparent;
    border: none;
    color: #cccccc;
    font-family: 'Cascadia Code', 'SF Mono', 'Courier New', monospace;
    font-size: 11px;
    padding: 3px 10px;
    cursor: pointer;
    transition: background 0.1s, color 0.1s;
  }

  .seg-btn:hover:not(:disabled):not(.on) { background: #3c3c3c; color: #fff; }

  .seg-btn.on {
    background: #0e639c;
    color: #ffffff;
  }

  .seg-btn:disabled { opacity: 0.35; cursor: not-allowed; }

  /* ── A2L slot ── */
  .a2l-drop, .a2l-chip {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 22px;
    padding: 0 8px;
    border-radius: 4px;
    font-family: 'Inter', -apple-system, sans-serif;
    font-size: 11px;
    flex-shrink: 0;
    max-width: 190px;
  }

  .a2l-drop {
    background: transparent;
    border: 1px dashed #4a4a4a;
    color: #999;
    cursor: pointer;
  }

  .a2l-drop:hover:not(:disabled) { border-color: #007acc; color: #ccc; }
  .a2l-drop:disabled { opacity: 0.5; cursor: default; }

  .a2l-chip {
    background: rgba(32, 208, 194, 0.13);
    border: 1px solid rgba(32, 208, 194, 0.35);
    color: #20d0c2;
  }

  .a2l-drop svg, .a2l-chip svg { width: 13px; height: 13px; flex-shrink: 0; }

  .a2l-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: 'Cascadia Code', 'SF Mono', monospace;
    font-size: 10.5px;
  }

  .a2l-x {
    background: none;
    border: none;
    color: inherit;
    opacity: 0.7;
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    padding: 0 0 0 2px;
    flex-shrink: 0;
  }

  .a2l-x:hover { opacity: 1; }

  /* The toolbar paints its own colours rather than reading the theme
     variables, so it has to resolve the theme the same way the app shell
     does: an explicit choice wins, and the OS preference applies only when
     none was made. Keying on the media query alone left the toolbar light
     over a dark app whenever Preferences disagreed with the system.

     The root part must be :global — :root is not in this component's
     markup, so Svelte's scoping pass drops the whole rule as unreachable
     and the light theme silently loses its colours. */
  @media (prefers-color-scheme: light) {
  :global(:root:not([data-theme="dark"])) .toolbar  { background: #f3f3f3; border-bottom-color: #ddd; }
  :global(:root:not([data-theme="dark"])) .icon-btn { color: #424242; }
  :global(:root:not([data-theme="dark"])) .icon-btn:hover:not(:disabled) { background: #e0e0e0; color: #1e1e1e; }
  :global(:root:not([data-theme="dark"])) .divider  { background: #ddd; }
  :global(:root:not([data-theme="dark"])) .seg      { border-color: #c4c4c4; }
  :global(:root:not([data-theme="dark"])) .seg-btn  { color: #424242; }
  :global(:root:not([data-theme="dark"])) .seg-btn:hover:not(:disabled):not(.on) { background: #e0e0e0; color: #1e1e1e; }
  :global(:root:not([data-theme="dark"])) .seg-btn.on { background: #0070c1; color: #fff; }
  :global(:root:not([data-theme="dark"])) .a2l-drop { border-color: #c4c4c4; color: #777; }
  :global(:root:not([data-theme="dark"])) .a2l-drop:hover:not(:disabled) { border-color: #0070c1; color: #333; }
  :global(:root:not([data-theme="dark"])) .a2l-chip {
      background: rgba(0, 128, 120, 0.10);
      border-color: rgba(0, 128, 120, 0.35);
      color: #00706a;
    }
  }

  :global(:root[data-theme="light"]) .toolbar  { background: #f3f3f3; border-bottom-color: #ddd; }
  :global(:root[data-theme="light"]) .icon-btn { color: #424242; }
  :global(:root[data-theme="light"]) .icon-btn:hover:not(:disabled) { background: #e0e0e0; color: #1e1e1e; }
  :global(:root[data-theme="light"]) .divider  { background: #ddd; }
  :global(:root[data-theme="light"]) .seg      { border-color: #c4c4c4; }
  :global(:root[data-theme="light"]) .seg-btn  { color: #424242; }
  :global(:root[data-theme="light"]) .seg-btn:hover:not(:disabled):not(.on) { background: #e0e0e0; color: #1e1e1e; }
  :global(:root[data-theme="light"]) .seg-btn.on { background: #0070c1; color: #fff; }
  :global(:root[data-theme="light"]) .a2l-drop { border-color: #c4c4c4; color: #777; }
  :global(:root[data-theme="light"]) .a2l-drop:hover:not(:disabled) { border-color: #0070c1; color: #333; }
  :global(:root[data-theme="light"]) .a2l-chip {
      background: rgba(0, 128, 120, 0.10);
      border-color: rgba(0, 128, 120, 0.35);
      color: #00706a;
    }

</style>
