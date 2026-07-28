<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  /**
   * MapSurface — the visible slice of a map drawn as a 3D surface.
   *
   * Cells are shaded on the same ramp the grid uses, so the two views agree at
   * a glance about where the highs are. Drag to orbit; hovering a cell traces
   * its row in the grid, the same link the curves plot has.
   *
   * Props
   *   detail     – ParamDetail from the backend
   *   slice      – subscripts for dimensions 2 and up
   *   highlight  – row index to mark, or null
   *   decimals   – decimal override, or null for the A2L FORMAT
   *   onHover    – (row: number|null) => void
   */
  import { sliceIndices } from '$lib/mapGrid.js';
  import { buildSurface, cellAt, axisPositions, DEFAULT_VIEW } from '$lib/surface.js';

  let {
    detail    = null,
    slice     = /** @type {number[]} */ ([]),
    highlight = /** @type {number|null} */ (null),
    decimals  = null,
    onHover   = (_row) => {},
  } = $props();

  const HEIGHT = 230;
  const PAD = 14;

  let width = $state(0);
  let yaw   = $state(DEFAULT_VIEW.yaw);
  let pitch = $state(DEFAULT_VIEW.pitch);
  let drag  = $state(/** @type {{x: number, y: number, yaw: number, pitch: number}|null} */ (null));
  let hover = $state(/** @type {{ix: number, iy: number}|null} */ (null));

  const dims = $derived(detail?.dims ?? []);
  const rows = $derived(sliceIndices(dims, slice));

  /** True breakpoints where the axis is numeric, index spacing where it is not. */
  const xAxis = $derived(axisPositions(detail?.axes?.[0]?.points, dims[0] ?? 0));
  const yAxis = $derived(axisPositions(detail?.axes?.[1]?.points, dims[1] ?? 0));

  const surface = $derived.by(() => {
    if (!rows.length || width <= 0) return null;
    const values = rows.flatMap((row) => row.map((i) => detail.values[i]?.phys ?? null));
    return buildSurface(
      { xs: xAxis.positions, ys: yAxis.positions, values, dims },
      { width, height: HEIGHT, pad: PAD, yaw, pitch },
    );
  });

  function fmt(v) {
    if (!Number.isFinite(v)) return '—';
    if (decimals !== null) return v.toFixed(decimals);
    return Number.isInteger(v) ? String(v) : String(Number(v.toFixed(4)));
  }

  /** Label a position on an axis, preferring the breakpoint's own rendering. */
  function label(d, i) {
    return detail?.axes?.[d]?.points?.[i]?.display ?? String(i);
  }

  function down(e) {
    drag = { x: e.clientX, y: e.clientY, yaw, pitch };
    e.currentTarget.setPointerCapture(e.pointerId);
  }

  function move(e) {
    if (drag) {
      yaw = drag.yaw + (e.clientX - drag.x) * 0.01;
      // Stop short of the poles, where the surface collapses to a line and
      // there is nothing left to steer by.
      const next = drag.pitch + (e.clientY - drag.y) * 0.01;
      pitch = Math.max(0.05, Math.min(1.45, next));
      return;
    }
    if (!surface) return;
    const box = e.currentTarget.getBoundingClientRect();
    const c = cellAt(surface, e.clientX - box.left, e.clientY - box.top);
    hover = c;
    onHover(c ? c.iy : null);
  }

  function up(e) {
    drag = null;
    e.currentTarget.releasePointerCapture?.(e.pointerId);
  }

  function leave() {
    if (drag) return;
    hover = null;
    onHover(null);
  }

  function key(e) {
    const step = e.shiftKey ? 0.3 : 0.1;
    if (e.key === 'ArrowLeft')       yaw -= step;
    else if (e.key === 'ArrowRight') yaw += step;
    else if (e.key === 'ArrowUp')    pitch = Math.min(1.45, pitch + step);
    else if (e.key === 'ArrowDown')  pitch = Math.max(0.05, pitch - step);
    else if (e.key === 'Home')       reset();
    else return;
    e.preventDefault();
  }

  function reset() {
    yaw = DEFAULT_VIEW.yaw;
    pitch = DEFAULT_VIEW.pitch;
  }
</script>

<div class="wrap" bind:clientWidth={width}>
  {#if surface}
    <!-- Focusable and keyboard-driven, so it carries an interactive role
         rather than role="img" with a tab stop bolted on. Svelte treats any
         <svg> as non-interactive whatever its role says, hence the ignores;
         the surface really is operable by pointer and by arrow keys, and the
         grid above remains the way to read the same numbers exactly. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <svg
      class="surface"
      class:dragging={!!drag}
      width={width}
      height={HEIGHT}
      role="application"
      aria-roledescription="rotatable surface plot"
      aria-label="Surface of {detail?.name ?? 'map'}. Arrow keys rotate, Home resets."
      tabindex="0"
      onpointerdown={down}
      onpointermove={move}
      onpointerup={up}
      onpointerleave={leave}
      onkeydown={key}
    >
      {#each surface.frame as ring}
        <polygon class="base" points={ring.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' ')} />
      {/each}
      {#each surface.quads as q}
        <polygon
          class="cell"
          class:traced={highlight === q.iy || highlight === q.iy + 1}
          class:on={hover && hover.ix === q.ix && hover.iy === q.iy}
          style="--t: {q.t.toFixed(3)}"
          points={q.points.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' ')}
        />
      {/each}
    </svg>

    <div class="cap">
      {#if hover}
        <span class="c-at">{label(0, hover.ix)}, {label(1, hover.iy)}</span>
        <span class="c-arrow">→</span>
        <span class="c-v">{fmt(detail.values[rows[hover.iy]?.[hover.ix]]?.phys)}</span>
      {:else}
        <span>{fmt(surface.zLo)} … {fmt(surface.zHi)}</span>
        {#if xAxis.even || yAxis.even}
          <span class="c-note">
            · {xAxis.even && yAxis.even ? 'axes' : xAxis.even ? 'X' : 'Y'} spaced evenly, no numeric breakpoints
          </span>
        {/if}
      {/if}
      <button class="reset" onclick={reset} title="Reset the view (Home)">reset</button>
    </div>
  {:else}
    <p class="none">A surface needs at least two points on each axis.</p>
  {/if}
</div>

<style>
  .wrap { width: 100%; }

  .surface {
    display: block;
    background: var(--c-bg);
    border: 1px solid var(--c-border2);
    border-radius: 5px;
    cursor: grab;
    touch-action: none;
  }

  .surface.dragging { cursor: grabbing; }
  .surface:focus { outline: 1px solid var(--c-accent); outline-offset: -1px; }

  /* The same ramp the grid cells use, so the two views agree about the highs.
     Painted as a flat fill plus an opacity layer rather than a colour
     function, which keeps both themes readable without any colour maths. */
  .cell {
    fill: var(--c-accent);
    fill-opacity: var(--t, 0.5);
    stroke: var(--c-border2);
    stroke-width: 0.5;
    stroke-linejoin: round;
  }

  .cell.traced { stroke: var(--c-accent); stroke-width: 1.2; }
  .cell.on     { stroke: var(--c-text); stroke-width: 1.4; }

  .base {
    fill: none;
    stroke: var(--c-border2);
    stroke-width: 1;
    stroke-dasharray: 3 3;
  }

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

  .c-at    { color: var(--c-addr); }
  .c-arrow { color: var(--c-dim); }
  .c-v     { color: var(--c-text2); }
  .c-note  { color: var(--c-dim); }

  .reset {
    margin-left: auto;
    background: none;
    border: none;
    padding: 0 2px;
    font: inherit;
    color: var(--c-accent);
    cursor: pointer;
  }

  .reset:hover { text-decoration: underline; }

  .none {
    font-size: 11px;
    color: var(--c-muted);
    margin: 0;
    padding: 10px 2px;
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
  }
</style>
