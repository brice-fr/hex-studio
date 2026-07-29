<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  /**
   * MapEditor — the full-size grid for a map, cuboid or cube.
   *
   * An in-app overlay rather than a second OS window, deliberately: the grid
   * needs the live, unsaved records and its edits have to join the same undo
   * stack. A separate window would have to ship the image across, send every
   * edit back, and re-sync on undo — three ways to drift out of step for a
   * layout benefit an overlay already gives.
   *
   * Props
   *   open         – whether the overlay is showing
   *   row          – the selected ParamRow
   *   detail       – ParamDetail from the backend
   *   decimals     – decimal override, or null for the A2L FORMAT
   *   onEditPoint  – (target, index, phys) => void
   *   onNavigate   – (name) => void
   *   onClose      – () => void
   */
  import MapGrid from './MapGrid.svelte';
  import MapPlot from './MapPlot.svelte';
  import MapSurface from './MapSurface.svelte';

  let {
    open        = false,
    row         = null,
    detail      = null,
    decimals    = null,
    onEditPoint = (_target, _index, _phys) => {},
    onNavigate  = (_name) => {},
    onClose     = () => {},
  } = $props();

  let shaded = $state(true);
  /** Which view sits under the grid: `curves`, `surface`, or neither. The two
   *  answer the same question differently, and the card is tall enough as it
   *  is, so they take turns rather than stacking. */
  let below = $state(/** @type {'curves'|'surface'|'none'} */ ('curves'));

  /** The row traced in the plot and marked in the grid. One piece of state
   *  drives both, so they can never disagree about which row that is.
   *
   *  A pinned row outranks the hovered one and survives the pointer leaving:
   *  with a dozen curves stacked together, reading one of them means being
   *  able to stop chasing it. */
  let hovered = $state(/** @type {number|null} */ (null));
  let pinned  = $state(/** @type {number|null} */ (null));
  const traced = $derived(pinned ?? hovered);

  // A different object, or a different slice, is a different set of rows.
  $effect(() => { void detail; void slice; pinned = null; });

  /** Subscripts for every dimension beyond the second. */
  let slice = $state(/** @type {number[]} */ ([]));

  const dims  = $derived(detail?.dims ?? []);
  /** Dimensions the grid cannot show at once, which the selectors pin. */
  const extra = $derived(dims.slice(2));

  // A different object, or a different shape, invalidates the pinned slice.
  $effect(() => {
    const n = extra.length;
    if (slice.length !== n) slice = Array(n).fill(0);
  });

  /** Breakpoint label for one of the pinned dimensions, or its index. */
  function head(d, i) {
    return detail?.axes?.[d]?.points?.[i]?.display ?? String(i);
  }

  function handleKey(e) {
    // Escape closes, unless a cell editor is open and wants it first.
    if (e.key === 'Escape' && !(e.target instanceof HTMLInputElement)) {
      onClose();
      e.preventDefault();
    }
  }

  function handleBackdrop(e) {
    if (e.target === e.currentTarget) onClose();
  }
</script>

<svelte:window onkeydown={(e) => open && handleKey(e)} />

{#if open && detail}
  <!-- Clicking the backdrop is a convenience; Escape is the keyboard route and
       is handled on the window, so the dialog itself needs no key handler. -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="backdrop" role="dialog" aria-modal="true" tabindex="-1"
       aria-label="Edit {row?.name}" onclick={handleBackdrop}>
    <div class="card">
      <div class="head">
        <div class="titles">
          <h2 class="title">{row?.name}</h2>
          {#if row?.description}<p class="sub">{row.description}</p>{/if}
        </div>
        <div class="tools">
          <label class="toggle" title="Colour cells by value">
            <input type="checkbox" bind:checked={shaded}>
            Shade
          </label>
          <div class="seg" role="group" aria-label="Plot below the grid">
            {#each [
              { id: 'curves',  label: 'Curves',  hint: 'Plot each row as a curve' },
              { id: 'surface', label: '3D',      hint: 'Draw the slice as a surface' },
              { id: 'none',    label: 'None',    hint: 'Grid only' },
            ] as opt}
              <button
                class="seg-btn"
                class:on={below === opt.id}
                title={opt.hint}
                onclick={() => (below = opt.id)}
              >{opt.label}</button>
            {/each}
          </div>
          <button class="btn-close" onclick={onClose} aria-label="Close">×</button>
        </div>
      </div>

      <div class="meta">
        <span class="m"><span class="m-l">Shape</span>{dims.join(' × ')}</span>
        <span class="m"><span class="m-l">Values</span>{detail.values.length}</span>
        {#if detail.value_unit}
          <span class="m"><span class="m-l">Unit</span>{detail.value_unit}</span>
        {/if}
        {#if !detail.values_editable}
          <span class="m ro">read-only</span>
        {/if}
      </div>

      {#if extra.length}
        <!-- Beyond two dimensions the grid shows one plane at a time; these
             pin the rest. -->
        <div class="slices">
          {#each extra as n, k}
            {@const d = k + 2}
            <label class="slice">
              <span class="s-l">{'ZW5'[k] ?? `D${d}`}</span>
              <select
                value={slice[k] ?? 0}
                onchange={(e) => {
                  const next = [...slice];
                  next[k] = Number(e.currentTarget.value);
                  slice = next;
                }}
              >
                {#each Array(n) as _, i}
                  <option value={i}>{head(d, i)}</option>
                {/each}
              </select>
              <span class="s-n">of {n}</span>
            </label>
          {/each}
        </div>
      {/if}

      <div class="body">
        <MapGrid
          {detail} {decimals} {shaded} {slice} {onEditPoint} {onNavigate}
          highlight={traced}
          locked={pinned !== null}
          onHoverRow={(y) => (hovered = y)}
          onLockRow={(y) => (pinned = y)}
        />
      </div>

      {#if below === 'curves'}
        <MapPlot
          {detail} {slice} {decimals}
          highlight={traced}
          locked={pinned !== null}
          onHover={(y) => (hovered = y)}
          onLock={(y) => (pinned = y)}
        />
      {:else if below === 'surface'}
        <MapSurface {detail} {slice} {decimals} highlight={traced} onHover={(y) => (hovered = y)} />
      {/if}
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
    z-index: 110;
    backdrop-filter: blur(2px);
  }

  .card {
    background: var(--c-raised);
    border: 1px solid var(--c-border2);
    border-radius: 10px;
    padding: 18px 20px 16px;
    /* Fit the grid in both directions, capped at the viewport. A fixed card is
       either cramped for a 16x16 map or mostly empty around a 2x3 one. The
       minimum keeps the header and slice selectors from wrapping. */
    width: max-content;
    min-width: min(460px, calc(100vw - 48px));
    max-width: calc(100vw - 48px);
    height: auto;
    max-height: calc(100vh - 48px);
    box-shadow: 0 20px 60px rgba(0,0,0,0.6);
    display: flex;
    flex-direction: column;
    gap: 10px;
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
  }

  .head {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .titles { flex: 1; min-width: 0; }

  .title {
    font-size: 13px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0;
    font-family: 'Cascadia Code', 'SF Mono', monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sub {
    font-size: 11px;
    color: var(--c-muted);
    margin: 2px 0 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tools {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--c-muted);
    cursor: pointer;
  }

  /* Matches the hex/data switch in the toolbar, so a three-way choice looks
     like the one the app already has. */
  .seg {
    display: flex;
    border: 1px solid var(--c-border2);
    border-radius: 4px;
    overflow: hidden;
  }

  .seg-btn {
    background: none;
    border: none;
    color: var(--c-muted);
    font-family: inherit;
    font-size: 11px;
    padding: 2px 8px;
    cursor: pointer;
  }

  .seg-btn:hover:not(.on) { background: var(--c-hover); color: var(--c-text); }
  .seg-btn.on { background: var(--c-accent); color: #fff; }

  .btn-close {
    background: none;
    border: none;
    color: var(--c-muted);
    font-size: 20px;
    line-height: 1;
    padding: 0 4px;
    cursor: pointer;
  }

  .btn-close:hover { color: var(--c-text); }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    font-size: 11px;
    color: var(--c-text2);
    font-family: 'Cascadia Code', 'SF Mono', monospace;
  }

  .m-l {
    color: var(--c-dim);
    margin-right: 5px;
    text-transform: uppercase;
    font-size: 9px;
    letter-spacing: 0.4px;
  }

  .m.ro { color: var(--c-diff-changed); }

  .slices {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .slice {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--c-muted);
  }

  .s-l {
    color: var(--c-addr);
    font-family: 'Cascadia Code', 'SF Mono', monospace;
  }

  .s-n { color: var(--c-dim); font-size: 10px; }

  .slice select {
    background: var(--c-bg);
    color: var(--c-text);
    border: 1px solid var(--c-border2);
    border-radius: 4px;
    font-family: 'Cascadia Code', 'SF Mono', monospace;
    font-size: 11px;
    padding: 2px 4px;
  }

  /* The grid takes the rest of the card and scrolls inside it, so the header
     and slice selectors stay put however large the object is. */
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    /* The card cannot be narrower than its header, so a small grid would
       otherwise sit against the left edge with the rest of the row empty. */
    justify-content: center;
    /* Wide enough to be worth opening, but never taller than the card. */
    max-height: calc(100vh - 210px);
  }

  .body :global(.grid-wrap) {
    flex: 0 1 auto;
    min-height: 0;
  }
</style>
