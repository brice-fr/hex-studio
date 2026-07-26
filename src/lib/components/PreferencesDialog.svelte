<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  let {
    open        = false,
    fontSize    = 13,
    bytesPerRow = 16,
    theme       = 'system',
    showMeasurements = false,
    showColAddress = false,
    showColType    = true,
    showColRaw     = false,
    onFontSize    = (_n) => {},
    onBytesPerRow = (_n) => {},
    onTheme       = (_t) => {},
    onShowMeasurements = (_v) => {},
    onShowColAddress   = (_v) => {},
    onShowColType      = (_v) => {},
    onShowColRaw       = (_v) => {},
    onClose       = () => {},
  } = $props();

  function handleKey(e) {
    if (e.key === 'Escape') onClose();
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="backdrop"
    role="dialog"
    aria-modal="true"
    aria-label="Preferences"
    tabindex="-1"
    onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    onkeydown={handleKey}
  >
    <div class="card">
      <div class="modal-header">
        <span class="modal-title">Preferences</span>
        <button
          class="close-btn"
          onpointerdown={(e) => e.stopPropagation()}
          onclick={onClose}
          aria-label="Close preferences"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2"
               stroke-linecap="round" xmlns="http://www.w3.org/2000/svg">
            <line x1="18" y1="6" x2="6" y2="18"/>
            <line x1="6"  y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>

      <div class="section">
        <div class="section-title">Appearance</div>

        <div class="pref-row">
          <span class="pref-label">Theme</span>
          <div class="segmented">
            {#each ['system', 'dark', 'light'] as t}
              <button
                class="seg-btn"
                class:active={theme === t}
                onclick={() => onTheme(t)}
              >{t.charAt(0).toUpperCase() + t.slice(1)}</button>
            {/each}
          </div>
        </div>

        <div class="pref-row">
          <span class="pref-label">Font size</span>
          <div class="range-row">
            <input
              type="range"
              min="10"
              max="20"
              step="1"
              value={fontSize}
              oninput={(e) => onFontSize(parseInt(e.target.value))}
            />
            <span class="range-readout">{fontSize} px</span>
          </div>
        </div>
      </div>

      <div class="section">
        <div class="section-title">Hex View</div>

        <div class="pref-row">
          <span class="pref-label">Bytes per row</span>
          <div class="segmented">
            {#each [8, 16, 32] as n}
              <button
                class="seg-btn"
                class:active={bytesPerRow === n}
                onclick={() => onBytesPerRow(n)}
              >{n}</button>
            {/each}
          </div>
        </div>
      </div>

      <div class="section">
        <div class="section-title">Data View</div>

        <div class="pref-row">
          <span class="pref-label">
            Show MEASUREMENT objects
            <span class="pref-hint">RAM signals, usually absent from a flash image</span>
          </span>
          <div class="segmented">
            <button class="seg-btn" class:active={!showMeasurements}
                    onclick={() => onShowMeasurements(false)}>Off</button>
            <button class="seg-btn" class:active={showMeasurements}
                    onclick={() => onShowMeasurements(true)}>On</button>
          </div>
        </div>

        <div class="pref-row col-row">
          <span class="pref-label">
            Table columns
            <span class="pref-hint">Hidden columns stay visible in the parameter pane</span>
          </span>
          <div class="checks">
            {#each [
              { label: 'Address', on: showColAddress, set: onShowColAddress },
              { label: 'Type',    on: showColType,    set: onShowColType },
              { label: 'Raw',     on: showColRaw,     set: onShowColRaw },
            ] as col}
              <label class="check">
                <input
                  type="checkbox"
                  checked={col.on}
                  onchange={(e) => col.set(e.currentTarget.checked)}
                />
                <span>{col.label}</span>
              </label>
            {/each}
          </div>
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn-done" onclick={onClose}>Done</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
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
    width: 380px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.6);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px 12px;
    border-bottom: 1px solid var(--c-border);
  }

  .modal-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--c-text);
    letter-spacing: 0.01em;
  }

  .close-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    background: transparent;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    color: var(--c-muted);
    padding: 0;
    transition: background 0.1s, color 0.1s;
  }

  .close-btn:hover { background: var(--c-hover); color: var(--c-text); }
  .close-btn svg   { width: 14px; height: 14px; }

  .section {
    padding: 12px 16px 4px;
  }

  .section-title {
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--c-muted);
    margin-bottom: 10px;
    user-select: none;
  }

  .pref-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
  }

  .pref-label {
    font-size: 13px;
    color: var(--c-text2);
    flex-shrink: 0;
    user-select: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .pref-hint {
    font-size: 10.5px;
    color: var(--c-dim);
    line-height: 1.35;
    max-width: 190px;
  }

  /* Independent toggles, so checkboxes rather than three On/Off pairs. */
  .col-row { align-items: flex-start; }

  .checks {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding-top: 1px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    color: var(--c-text2);
    cursor: pointer;
    user-select: none;
  }

  .check input {
    accent-color: var(--c-accent);
    cursor: pointer;
    margin: 0;
  }

  .segmented {
    display: flex;
    border-radius: 5px;
    overflow: hidden;
    border: 1px solid var(--c-border);
  }

  .seg-btn {
    padding: 4px 12px;
    font-size: 12px;
    background: var(--c-raised);
    color: var(--c-text2);
    border: none;
    border-right: 1px solid var(--c-border);
    cursor: pointer;
    font-family: inherit;
    transition: background 0.1s, color 0.1s;
  }

  .seg-btn:last-child {
    border-right: none;
  }

  .seg-btn:hover:not(.active) {
    background: var(--c-hover);
  }

  .seg-btn.active {
    background: var(--c-accent-b);
    color: #fff;
  }

  .range-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .range-row input[type="range"] {
    width: 130px;
    accent-color: var(--c-accent);
    cursor: pointer;
  }

  .range-readout {
    font-size: 12px;
    color: var(--c-muted);
    min-width: 36px;
    text-align: right;
    font-family: 'Cascadia Code', 'SF Mono', 'Fira Code', monospace;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    padding: 12px 16px 14px;
    border-top: 1px solid var(--c-border);
  }

  .btn-done {
    padding: 5px 22px;
    font-size: 13px;
    background: var(--c-accent-b);
    color: #fff;
    border: none;
    border-radius: 5px;
    cursor: pointer;
    font-family: inherit;
    transition: background 0.1s;
  }

  .btn-done:hover { background: var(--c-accent-h); }
</style>
