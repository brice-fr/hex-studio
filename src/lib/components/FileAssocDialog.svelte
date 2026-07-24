<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  import { getFileAssociations, applyFileAssociations } from '$lib/api.js';

  let {
    open = false,
    onClose = () => {},
  } = $props();

  let entries = $state([]);
  let pending = $state({});
  let loading = $state(false);
  let applying = $state(false);
  let statusMsg = $state('');
  let isMac = $state(false);

  $effect(() => {
    if (open) {
      isMac = /Mac/i.test(navigator.platform);
      loadAssociations();
    } else {
      statusMsg = '';
    }
  });

  async function loadAssociations() {
    loading = true;
    statusMsg = '';
    try {
      const result = await getFileAssociations();
      entries = result;
      const map = {};
      for (const e of result) {
        map[e.ext] = e.associated;
      }
      pending = map;
    } catch (err) {
      statusMsg = String(err);
    } finally {
      loading = false;
    }
  }

  async function handleApply() {
    applying = true;
    statusMsg = '';
    try {
      const changes = [];
      for (const e of entries) {
        if (pending[e.ext] !== e.associated) {
          changes.push([e.ext, pending[e.ext]]);
        }
      }
      await applyFileAssociations(changes);
      await loadAssociations();
      statusMsg = 'Applied successfully.';
    } catch (err) {
      statusMsg = String(err);
    } finally {
      applying = false;
    }
  }

  function handleBackdrop(e) {
    if (e.target === e.currentTarget) onClose();
  }

  function handleKeydown(e) {
    if (e.key === 'Escape') onClose();
  }

  const hasChanges = $derived(
    entries.some(e => pending[e.ext] !== e.associated)
  );

  // Group entries by group field
  const grouped = $derived((() => {
    const map = new Map();
    for (const e of entries) {
      if (!map.has(e.group)) map.set(e.group, []);
      map.get(e.group).push(e);
    }
    return map;
  })());
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={handleBackdrop}>
    <div class="modal" role="dialog" aria-modal="true" aria-label="File Associations">
      <div class="modal-header">
        <span class="modal-title">File Associations</span>
        <button class="close-btn" onclick={onClose} aria-label="Close">×</button>
      </div>

      <div class="modal-body">
        {#if loading}
          <div class="spinner-row">
            <span class="spinner"></span>
            <span class="spinner-text">Checking current associations…</span>
          </div>
        {:else}
          <p class="description">Select which file formats should open with Hex Studio.</p>

          {#if isMac}
            <div class="macos-note">
              On macOS, deassociating extensions is not supported. You can only associate formats.
            </div>
          {/if}

          <div class="entry-list">
            {#each [...grouped] as [group, groupEntries]}
              <div class="group-header">{group}</div>
              {#each groupEntries as entry}
                {@const isDisabled = isMac && !entry.can_deassociate && pending[entry.ext]}
                <label class="entry-row" class:disabled={isDisabled}>
                  <input
                    type="checkbox"
                    checked={pending[entry.ext]}
                    disabled={isDisabled}
                    onchange={(e) => { pending = { ...pending, [entry.ext]: e.currentTarget.checked }; }}
                  />
                  <span class="ext">.{entry.ext}</span>
                  <span class="entry-desc">{entry.description}</span>
                  {#if entry.associated}
                    <span class="badge badge-yes">associated</span>
                  {:else}
                    <span class="badge badge-no">not associated</span>
                  {/if}
                </label>
              {/each}
            {/each}
          </div>

          {#if statusMsg}
            <div class="status-msg" class:status-ok={statusMsg.startsWith('Applied')} class:status-err={!statusMsg.startsWith('Applied')}>
              {statusMsg}
            </div>
          {/if}
        {/if}
      </div>

      <div class="modal-footer">
        <button class="btn btn-cancel" onclick={onClose}>Cancel</button>
        <button
          class="btn btn-primary"
          disabled={applying || !hasChanges || loading}
          onclick={handleApply}
        >
          {#if applying}
            <span class="spinner spinner-sm"></span>
            Applying…
          {:else}
            Apply
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 300;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .modal {
    background: var(--c-surface);
    border: 1px solid var(--c-border);
    border-radius: 8px;
    width: 460px;
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 64px);
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px 12px;
    border-bottom: 1px solid var(--c-border);
    flex-shrink: 0;
  }

  .modal-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--c-text);
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--c-muted);
    font-size: 20px;
    line-height: 1;
    cursor: pointer;
    padding: 0 4px;
    border-radius: 4px;
  }

  .close-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .modal-body {
    padding: 14px 16px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }

  .description {
    font-size: 13px;
    color: var(--c-text2);
    margin-bottom: 12px;
  }

  .macos-note {
    background: rgba(217, 119, 6, 0.15);
    color: #b45309;
    border: 1px solid rgba(217, 119, 6, 0.3);
    border-radius: 5px;
    padding: 8px 12px;
    font-size: 12px;
    margin-bottom: 12px;
  }

  .spinner-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 20px 0;
    color: var(--c-muted);
    font-size: 13px;
  }

  .spinner {
    display: inline-block;
    width: 16px;
    height: 16px;
    border: 2px solid var(--c-border2);
    border-top-color: var(--c-accent);
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
    flex-shrink: 0;
  }

  .spinner-sm {
    width: 12px;
    height: 12px;
  }

  .spinner-text {
    font-size: 13px;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .entry-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .group-header {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--c-muted);
    padding: 10px 0 4px;
  }

  .entry-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 6px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 13px;
    color: var(--c-text);
  }

  .entry-row:hover {
    background: var(--c-hover);
  }

  .entry-row.disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .entry-row input[type="checkbox"] {
    flex-shrink: 0;
    accent-color: var(--c-accent);
  }

  .ext {
    font-family: 'Consolas', 'Menlo', monospace;
    font-size: 12px;
    color: var(--c-accent-t);
    min-width: 44px;
    flex-shrink: 0;
  }

  .entry-desc {
    flex: 1;
    color: var(--c-text2);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .badge {
    flex-shrink: 0;
    font-size: 10px;
    font-weight: 600;
    padding: 2px 7px;
    border-radius: 10px;
    letter-spacing: 0.02em;
  }

  .badge-yes {
    background: rgba(34, 197, 94, 0.18);
    color: #16a34a;
    border: 1px solid rgba(34, 197, 94, 0.3);
  }

  .badge-no {
    background: var(--c-raised);
    color: var(--c-muted);
    border: 1px solid var(--c-border);
  }

  .status-msg {
    margin-top: 12px;
    padding: 8px 12px;
    border-radius: 5px;
    font-size: 12px;
  }

  .status-ok {
    background: rgba(34, 197, 94, 0.12);
    color: #16a34a;
    border: 1px solid rgba(34, 197, 94, 0.25);
  }

  .status-err {
    background: var(--c-err-bg);
    color: var(--c-err);
    border: 1px solid rgba(244, 113, 113, 0.3);
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--c-border);
    flex-shrink: 0;
  }

  .btn {
    padding: 6px 16px;
    font-size: 13px;
    font-weight: 500;
    border-radius: 5px;
    border: 1px solid transparent;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .btn-cancel {
    background: var(--c-raised);
    color: var(--c-text);
    border-color: var(--c-border2);
  }

  .btn-cancel:hover {
    background: var(--c-hover);
  }

  .btn-primary {
    background: var(--c-accent);
    color: #fff;
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--c-accent-h);
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
