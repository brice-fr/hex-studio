<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  /**
   * MapPlot — a map as a family of curves, one per row of the visible slice.
   *
   * Every curve shares one pair of scales, so the rows can be read against
   * each other; scaling each to its own extent would make a flat row look like
   * a varied one. The highlighted row is linked to the grid, so hovering a row
   * there traces it here and the other way round.
   *
   * Props
   *   detail     – ParamDetail from the backend
   *   slice      – subscripts for dimensions 2 and up
   *   highlight  – row index to trace, or null
   *   locked     – true when `highlight` is pinned rather than hovered
   *   decimals   – decimal override, or null for the A2L FORMAT
   *   onHover    – (row: number|null) => void
   *   onLock     – (row: number|null) => void   pin or release a row
   */
  import { sliceIndices, valueExtent } from '$lib/mapGrid.js';
  import { buildPlot, nearestPoint } from '$lib/plot.js';

  let {
    detail    = null,
    slice     = /** @type {number[]} */ ([]),
    highlight = /** @type {number|null} */ (null),
    locked    = false,
    decimals  = null,
    onHover   = (_row) => {},
    onLock    = (_row) => {},
  } = $props();

  const HEIGHT = 150;
  const PAD = { t: 10, r: 10, b: 10, l: 10 };

  let width = $state(0);
  /** The point under the pointer, as `{row, x, y}`. */
  let cursor = $state(/** @type {any} */ (null));

  const dims = $derived(detail?.dims ?? []);
  const rows = $derived(sliceIndices(dims, slice));

  /** X is the first axis's breakpoints, or the column index without one. */
  const xs = $derived.by(() => {
    const pts = detail?.axes?.[0]?.points ?? [];
    return Array.from({ length: dims[0] ?? 0 }, (_, i) => pts[i]?.phys ?? i);
  });

  const plot = $derived.by(() => {
    if (!rows.length || width <= 0) return null;
    const series = rows.map((row) =>
      row.map((index, x) => ({ x: xs[x], y: detail.values[index]?.phys ?? NaN, index })),
    );
    // Scaled to the whole object, not the visible plane, so slices stay
    // comparable as the selector steps through them.
    const e = valueExtent(detail.values);
    return buildPlot(series, {
      width,
      height: HEIGHT,
      pad: PAD,
      yRange: e ? { lo: e.lo, hi: e.hi } : null,
    });
  });

  /** Label for a row, from the second axis where there is one. */
  function rowLabel(y) {
    return detail?.axes?.[1]?.points?.[y]?.display ?? `row ${y}`;
  }

  function fmt(v) {
    if (!Number.isFinite(v)) return '—';
    if (decimals !== null) return v.toFixed(decimals);
    return Number.isInteger(v) ? String(v) : String(Number(v.toFixed(4)));
  }

  /** The point under the pointer, obeying a lock when one is set. */
  function pointAt(e) {
    if (!plot) return null;
    const box = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - box.left;
    const y = e.clientY - box.top;
    // Locked: stay on that curve and read along it. Otherwise match in both
    // axes — every row shares the same x positions, so x alone would always
    // return the same curve however near the pointer got to another.
    return locked
      ? nearestPoint(plot, x, null, highlight)
      : nearestPoint(plot, x, y);
  }

  function move(e) {
    const hit = pointAt(e);
    if (!hit) return;
    cursor = { row: hit.series, ...hit.point };
    if (!locked) onHover(hit.series);
  }

  function click(e) {
    const hit = pointAt(e);
    if (!hit) return;
    // Clicking the pinned curve releases it, so the plot alone is enough to
    // both set and clear the lock.
    onLock(locked && highlight === hit.series ? null : hit.series);
  }

  function keydown(e) {
    if (e.key === 'Escape' && locked) {
      onLock(null);
      e.preventDefault();
    }
  }

  function leave() {
    cursor = null;
    if (!locked) onHover(null);
  }
</script>

<div class="plot-wrap" bind:clientWidth={width}>
  {#if plot}
    <!-- A read-out of the curves, not a second way to edit them: the grid
         above is where values change. Clicking pins a curve so the readout
         follows it alone, which is what makes a crowded family usable. -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <svg
      class="plot"
      width={width}
      height={HEIGHT}
      role="img"
      aria-label="Curves of {detail?.name ?? 'map'}"
      onpointermove={move}
      onpointerleave={leave}
      onclick={click}
      onkeydown={keydown}
    >
      {#if plot.zero !== null}
        <line class="zero" x1={PAD.l} y1={plot.zero} x2={width - PAD.r} y2={plot.zero} />
      {/if}
      {#each plot.series as s}
        <polyline
          class="line"
          class:dim={highlight !== null && highlight !== s.index}
          class:on={highlight === s.index}
          points={s.line}
        />
      {/each}
      {#if highlight !== null && plot.series.some((s) => s.index === highlight)}
        {#each plot.series.filter((s) => s.index === highlight) as s}
          {#each s.points as p}
            <circle class="dot" cx={p.cx} cy={p.cy} r="2.5" />
          {/each}
        {/each}
      {/if}
      {#if cursor}
        <line class="cursor" x1={cursor.cx} y1={PAD.t} x2={cursor.cx} y2={HEIGHT - PAD.b} />
      {/if}
    </svg>

    <div class="cap">
      {#if cursor}
        <span class="c-row">{rowLabel(cursor.row)}</span>
        <span>{fmt(cursor.x)}</span>
        <span class="c-arrow">→</span>
        <span class="c-y">{fmt(cursor.y)}</span>
      {:else}
        <span>{plot.series.length} row{plot.series.length === 1 ? '' : 's'}</span>
        <span class="c-arrow">·</span>
        <span class="c-y">{fmt(plot.yLo)} … {fmt(plot.yHi)}</span>
      {/if}
      {#if locked}
        <button class="lock" onclick={() => onLock(null)} title="Release the pinned row (Escape)">
          pinned: {rowLabel(highlight)} ×
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .plot-wrap { width: 100%; }

  .plot {
    display: block;
    background: var(--c-bg);
    border: 1px solid var(--c-border2);
    border-radius: 5px;
  }

  .line {
    fill: none;
    stroke: var(--c-accent);
    stroke-width: 1.3;
    stroke-linejoin: round;
    stroke-linecap: round;
    opacity: 0.75;
  }

  /* One row traced, the rest kept as context rather than hidden — the shape of
     the surface is the reason to look at this at all. */
  .line.dim { opacity: 0.22; stroke-width: 1; }
  .line.on  { opacity: 1; stroke-width: 2; }

  .dot {
    fill: var(--c-accent);
    stroke: var(--c-bg);
    stroke-width: 1;
  }

  .zero {
    stroke: var(--c-border2);
    stroke-width: 1;
    stroke-dasharray: 2 3;
  }

  .cursor { stroke: var(--c-border2); stroke-width: 1; }

  .cap {
    display: flex;
    gap: 6px;
    align-items: baseline;
    font-size: 10px;
    font-family: 'Cascadia Code', 'SF Mono', monospace;
    color: var(--c-muted);
    padding: 3px 2px 0;
    white-space: nowrap;
    overflow: hidden;
  }

  .lock {
    margin-left: auto;
    background: none;
    border: none;
    padding: 0 2px;
    font: inherit;
    color: var(--c-accent);
    cursor: pointer;
    white-space: nowrap;
  }

  .lock:hover { text-decoration: underline; }

  .plot { cursor: crosshair; }

  .c-row   { color: var(--c-addr); }
  .c-arrow { color: var(--c-dim); }
  .c-y     { color: var(--c-text2); }
</style>
