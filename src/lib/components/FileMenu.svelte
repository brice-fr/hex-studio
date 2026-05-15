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
    onFill       = () => {}, onMove       = () => {},
    onChecksum   = () => {}, onImportMerge = () => {},
    canUndo      = false,    canRedo       = false,
    hasSelection = false,
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

  @media (prefers-color-scheme: light) {
    .toolbar  { background: #f3f3f3; border-bottom-color: #ddd; }
    .icon-btn { color: #424242; }
    .icon-btn:hover:not(:disabled) { background: #e0e0e0; color: #1e1e1e; }
    .divider  { background: #ddd; }
  }
</style>
