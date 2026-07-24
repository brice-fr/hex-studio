<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  /**
   * AboutDialog — modal showing app icon, name, version and copyright.
   *
   * Props
   *   open     – controls visibility
   *   onClose  – called when the user dismisses the dialog
   */
  let { open = false, onClose = () => {} } = $props();

  const version   = import.meta.env.VITE_APP_VERSION ?? '0.1.0';
  const copyright = `© ${new Date().getFullYear()} Brice LECOLE`;

  function handleBackdrop(e) { if (e.target === e.currentTarget) onClose(); }
  function handleKey(e)      { if (e.key === 'Escape' || e.key === 'Enter') onClose(); }

  let okBtn = $state(null);
  $effect(() => { if (open) setTimeout(() => okBtn?.focus(), 40); });
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="backdrop"
    role="dialog"
    aria-modal="true"
    aria-label="About Hex Studio"
    tabindex="-1"
    onclick={handleBackdrop}
    onkeydown={handleKey}
  >
    <div class="card">
      <!-- Icon -->
      <img class="app-icon" src="/icon.png" alt="Hex Studio icon" draggable="false" />

      <!-- Name -->
      <h1 class="app-name">Hex Studio</h1>

      <!-- Version -->
      <p class="app-version">Version {version}</p>

      <!-- Divider -->
      <hr class="divider" />

      <!-- Copyright -->
      <p class="app-copyright">{copyright}</p>

      <!-- OK button -->
      <button class="btn-ok" bind:this={okBtn} onclick={onClose}>OK</button>
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
    backdrop-filter: blur(3px);
  }

  .card {
    background: var(--c-raised);
    border: 1px solid var(--c-border2);
    border-radius: 12px;
    padding: 32px 36px 24px;
    width: 280px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.65);
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  }

  .app-icon {
    width: 80px;
    height: 80px;
    border-radius: 18px;
    margin-bottom: 8px;
    user-select: none;
  }

  .app-name {
    font-size: 18px;
    font-weight: 700;
    color: var(--c-text);
    letter-spacing: -0.01em;
    margin: 0;
  }

  .app-version {
    font-size: 12.5px;
    color: var(--c-muted);
    margin: 0;
  }

  .divider {
    width: 100%;
    border: none;
    border-top: 1px solid var(--c-border);
    margin: 8px 0 4px;
  }

  .app-copyright {
    font-size: 11.5px;
    color: var(--c-dim);
    text-align: center;
    margin: 0;
    line-height: 1.5;
  }

  .btn-ok {
    margin-top: 16px;
    padding: 6px 32px;
    background: var(--c-accent-b);
    color: #fff;
    border: none;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    transition: background 0.1s;
  }

  .btn-ok:hover  { background: var(--c-accent-h); }
  .btn-ok:active { background: #0a5080; }
</style>
