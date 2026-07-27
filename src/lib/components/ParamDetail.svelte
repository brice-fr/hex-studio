<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  /**
   * ParamDetail — right-hand pane of the data view.
   *
   * Scalars get an editor; 1D objects get a read-only axis→value table.
   *
   * Props
   *   row          – the selected ParamRow, or null
   *   detail       – ParamDetail from the backend for 1D objects, or null
   *   decimals     – decimal override for fractional values, or null for the
   *                  A2L FORMAT default
   *   onEditValue  – (phys: number) => void
   *   onEditText   – (text: string) => void
   *   onGoto       – (addr: number) => void   jump to the address in the hex view
   *   onNavigate   – (name: string) => void   select another parameter
   *   onEditPoint  – (target: 'value'|'axis', index: number, phys: number) => void
   */
  import MapGrid from './MapGrid.svelte';
  import { buildPlot, nearestPoint } from '$lib/plot.js';

  let {
    row         = null,
    detail      = null,
    decimals    = null,
    onEditValue = (_phys) => {},
    onEditText  = (_text) => {},
    onGoto      = (_addr) => {},
    onNavigate  = (_name) => {},
    onEditPoint = (_target, _index, _phys) => {},
    onOpenMap   = (_name) => {},
  } = $props();

  /** Which point cell is being edited, as `"target:index"`. */
  let editingCell = $state(/** @type {string|null} */ (null));
  let cellDraft   = $state('');
  let cellError   = $state('');

  function beginCell(target, index, current) {
    if (target === 'axis' ? !detail?.axis_editable : !detail?.values_editable) return;
    editingCell = `${target}:${index}`;
    cellDraft = String(current);
    cellError = '';
  }

  function cancelCell() {
    editingCell = null;
    cellError = '';
  }

  function commitCell(target, index) {
    const v = Number(cellDraft.trim());
    if (cellDraft.trim() === '' || Number.isNaN(v)) {
      cellError = 'number';
      return;
    }
    editingCell = null;
    cellError = '';
    onEditPoint(target, index, v);
  }

  function cellKey(e, target, index) {
    if (e.key === 'Enter')  { commitCell(target, index); e.preventDefault(); }
    if (e.key === 'Escape') { cancelCell(); e.preventDefault(); }
  }

  /** How each AXIS_DESCR attribute stores its breakpoints, in plain words. */
  const AXIS_KIND_LABEL = {
    STD_AXIS: 'Standard — stored with this curve',
    COM_AXIS: 'Common — shared axis object',
    RES_AXIS: 'Rescale — shared axis object',
    CURVE_AXIS: 'Curve — another curve’s values',
    FIX_AXIS: 'Fixed — computed, not stored',
  };

  /** Re-render a server-formatted number at the chosen precision. */
  function atPrecision(value, fallback) {
    if (decimals === null || value === null || value === undefined) return fallback;
    // Whole numbers stay whole unless the default rendering already had a
    // fractional part, matching the table's behaviour.
    if (Number.isInteger(value) && !String(fallback).includes('.')) return fallback;
    return value.toFixed(decimals);
  }

  let draft = $state('');
  let error = $state('');

  /**
   * What the editor should start from. An ASCII field uses its decoded text,
   * not `display`, which reads "(empty)" for an all-NUL array.
   */
  function initialDraft(r) {
    if (!r) return '';
    if (r.category === 'ascii') return r.text_value ?? '';
    if (r.phys_num !== null && r.phys_num !== undefined) return String(r.phys_num);
    return r.display ?? '';
  }

  // Re-seed the editor whenever a different parameter is selected, or the
  // displayed value changes underneath us after an edit or an undo.
  $effect(() => {
    const key = row ? `${row.name}:${row.display}` : '';
    void key;
    draft = initialDraft(row);
    error = '';
  });

  const isEnum   = $derived(!!row?.enum_options);
  const isText    = $derived(row?.category === 'ascii');
  const isVirtual = $derived(row?.category === 'virtual');
  /** Two or more dimensions: a map, cuboid or cube. */
  const isMap = $derived(row?.category === 'map');
  /** `4 x 5`, the shape as a person would say it. */
  const shape = $derived((detail?.dims ?? row?.dims ?? []).join(' x '));
  /** Characters left before the field is full. */
  const textLeft = $derived(
    isText && row?.text_max_len != null ? row.text_max_len - draft.length : 0
  );
  /**
   * A slider needs a numeric scalar, a real range, and a step the field can
   * actually store.
   *
   * Deliberately no cap on the number of increments. A ULONG spanning three
   * million counts is exactly the case a slider helps with — the track gives
   * coarse reach across the range, the text field gives precision, and arrow
   * keys move exactly one step. Capping the count only denied a slider to the
   * widest integer parameters, which are the ones most tedious to type.
   *
   * The pathological range needs no guard here: a span beyond about 1.8e308
   * overflows to infinity, so the backend reports no step and this returns
   * false on the `Number.isFinite` check below.
   */
  const showSlider = $derived.by(() => {
    if (!row?.editable || isEnum || isText) return false;
    const lo = row.lower_limit, hi = row.upper_limit, step = row.phys_step;
    if (![lo, hi, step].every((n) => typeof n === 'number' && Number.isFinite(n))) return false;
    return hi > lo && step > 0;
  });

  /** Slider position, clamped — a stored value may sit outside the limits. */
  const sliderValue = $derived.by(() => {
    if (!row) return 0;
    const v = Number(draft);
    if (!Number.isFinite(v)) return row.lower_limit;
    return Math.min(row.upper_limit, Math.max(row.lower_limit, v));
  });

  const outLimit = $derived.by(() => {
    if (!row || row.phys_num === null || row.phys_num === undefined) return false;
    return row.phys_num < row.lower_limit || row.phys_num > row.upper_limit;
  });

  function hex32(n) { return '0x' + n.toString(16).padStart(8, '0').toUpperCase(); }

  function commit() {
    if (!row?.editable) return;
    if (isEnum) { onEditText(draft); return; }

    if (isText) {
      // The same rules the backend enforces, checked here so the user gets the
      // message before a round-trip.
      if (draft.length > row.text_max_len) {
        error = `At most ${row.text_max_len} characters`;
        return;
      }
      // eslint-disable-next-line no-control-regex
      if (!/^[\x20-\x7E]*$/.test(draft)) {
        error = 'Only printable ASCII';
        return;
      }
      error = '';
      onEditText(draft);
      return;
    }

    const v = Number(draft.trim());
    if (draft.trim() === '' || Number.isNaN(v)) {
      error = 'Enter a number';
      return;
    }
    if (v < row.lower_limit || v > row.upper_limit) {
      error = `Outside ${row.lower_limit} … ${row.upper_limit}`;
      return;
    }
    error = '';
    onEditValue(v);
  }

  function handleKey(e) {
    if (e.key === 'Enter')  { commit(); e.preventDefault(); }
    if (e.key === 'Escape') {
      draft = initialDraft(row);
      error = '';
    }
  }

  // ── Curve plot ────────────────────────────────────────────────────────────
  /** Measured so the plot can be laid out in real pixels; stretching a fixed
   *  viewBox to fit would turn the point dots into ellipses. */
  let plotW = $state(0);
  const PLOT_H = 92;
  const PAD = { t: 10, r: 8, b: 10, l: 8 };

  /** Which point the pointer is over, or null. */
  let hoverPoint = $state(/** @type {number|null} */ (null));

  /**
   * The value a cell would take right now, preferring an in-progress edit.
   *
   * Reading the draft rather than the committed value is what makes the plot
   * follow the keystrokes: the backend only hears about the change on commit,
   * which would otherwise leave the curve a step behind what the user typed.
   */
  function liveValue(target, i, fallback) {
    if (editingCell === `${target}:${i}`) {
      const t = cellDraft.trim();
      const v = Number(t);
      if (t !== '' && !Number.isNaN(v)) return v;
    }
    return fallback;
  }

  /**
   * Screen geometry for the plot, or null when there is nothing to draw.
   *
   * X is the axis breakpoint where the object has one and the point index
   * otherwise, so a value block still plots as the series it is.
   */
  const plot = $derived.by(() => {
    if (!points.length || plotW <= 0) return null;
    const data = points
      .map((p, i) => ({
        i,
        x: p.axis ? liveValue('axis', i, p.axis.phys) : i,
        y: p.value ? liveValue('value', i, p.value.phys) : NaN,
      }));
    return buildPlot([data], { width: plotW, height: PLOT_H, pad: PAD });
  });

  /** The single series, which is all a 1D object has. */
  const screen = $derived(plot?.series[0]?.points ?? []);

  /** Nearest plotted point to a pointer position, for hover and click. */
  function pointAt(e) {
    const box = e.currentTarget.getBoundingClientRect();
    return nearestPoint(plot, e.clientX - box.left)?.point ?? null;
  }

  /** Compact number for the plot caption, honouring the decimals override. */
  function fmtNum(v) {
    if (!Number.isFinite(v)) return '—';
    if (decimals !== null) return v.toFixed(decimals);
    return Number.isInteger(v) ? String(v) : String(Number(v.toFixed(4)));
  }


  /**
   * Pair axis breakpoints with function values for the curve table.
   *
   * Only meaningful in one dimension: a map holds one breakpoint list per axis
   * and a grid of values, which this row-by-row pairing cannot represent.
   */
  const points = $derived.by(() => {
    if (!detail || isMap) return [];
    const n = Math.max(detail.axis.length, detail.values.length);
    return Array.from({ length: n }, (_, i) => ({
      i,
      axis:  detail.axis[i]   ?? null,
      value: detail.values[i] ?? null,
    }));
  });
</script>

<div class="detail-panel">
  <div class="panel-header">
    <span class="header-title">Parameter</span>
  </div>

  {#if !row}
    <p class="empty">Select a parameter</p>
  {:else}
    <div class="detail-body">
      <div class="name" title={row.name}>{row.name}</div>
      {#if row.description}
        <div class="desc">{row.description}</div>
      {/if}

      <!-- ── Editor / value ── -->
      {#if row.editable}
        <div class="edit-row">
          {#if isEnum}
            <select bind:value={draft} onchange={commit} aria-label="Value">
              {#each row.enum_options as opt}
                <option value={opt}>{opt}</option>
              {/each}
            </select>
          {:else if isText}
            <input
              type="text"
              class="text-input"
              bind:value={draft}
              onkeydown={handleKey}
              maxlength={row.text_max_len}
              class:invalid={!!error}
              aria-label="String value"
              spellcheck="false"
              autocomplete="off"
            />
          {:else}
            <input
              type="text"
              bind:value={draft}
              onkeydown={handleKey}
              class:invalid={!!error}
              aria-label="Physical value"
              spellcheck="false"
            />
            {#if row.unit}<span class="unit">{row.unit}</span>{/if}
          {/if}
        </div>

        {#if showSlider}
          <!-- Dragging updates the field live but only writes on release, so a
               sweep leaves one undo entry rather than one per pixel. -->
          <input
            type="range"
            class="slider"
            min={row.lower_limit}
            max={row.upper_limit}
            step={row.phys_step}
            value={sliderValue}
            aria-label="{row.name} value"
            oninput={(e) => { draft = e.currentTarget.value; error = ''; }}
            onchange={commit}
          />
          <div class="slider-ends">
            <span>{row.lower_limit}</span>
            <span>{row.upper_limit}</span>
          </div>
        {/if}

        {#if error}
          <div class="err">{error}</div>
        {:else if isText}
          <div class="counter" class:full={textLeft === 0}>
            {draft.length} / {row.text_max_len} chars
            {#if row.text_capacity > row.text_max_len}
              <span class="counter-note">· 1 byte reserved for the terminator</span>
            {/if}
          </div>
          <button class="apply" onclick={commit}>Apply</button>
        {:else if !isEnum}
          <button class="apply" onclick={commit}>Apply</button>
        {/if}
      {:else}
        <div class="value-static" class:out={outLimit}>
          <span class="v">{atPrecision(row.phys_num, row.display)}</span>
          {#if row.unit}<span class="unit">{row.unit}</span>{/if}
        </div>
        {#if isVirtual}
          <!-- The value above is computed, so show what produced it and let
               the reader follow each input. -->
          <div class="formula">{row.formula ?? 'computed'}</div>
          {#if row.depends_on?.length}
            <div class="deps">
              <span class="deps-label">Reads</span>
              {#each row.depends_on as dep}
                <button class="link ref" onclick={() => onNavigate(dep)}
                        title="Show {dep}">{dep}</button>
              {/each}
            </div>
          {/if}
        {/if}
        {#if row.note}
          <div class="note">{row.note}</div>
        {/if}
      {/if}

      <!-- ── Facts ── -->
      <table class="facts">
        <tbody>
          {#if isVirtual}
            <!-- No storage, so an address, a size and a presence would all be
                 fictions. The A2L declares 0x0 purely to satisfy the syntax. -->
            <tr><td>Storage</td><td class="v virt">not stored</td></tr>
          {:else}
            <tr>
              <td>Address</td>
              <td class="v">
                <button class="link" onclick={() => onGoto(row.address)}
                        title="Show in hex view">{hex32(row.address)}</button>
              </td>
            </tr>
            <tr><td>Size</td><td class="v">{row.byte_size} B</td></tr>
          {/if}
          <tr><td>Type</td><td class="v">{row.datatype}</td></tr>
          {#if row.text_capacity}
            <tr>
              <td>Capacity</td>
              <td class="v">{row.text_max_len} of {row.text_capacity} chars</td>
            </tr>
          {/if}
          {#if row.raw_hex}
            <tr><td>Raw</td><td class="v raw">{row.raw_hex}</td></tr>
          {/if}
          <tr>
            <td>Limits</td>
            <td class="v">{row.lower_limit} … {row.upper_limit}</td>
          </tr>
          <tr><td>Conversion</td><td class="v conv" title={row.conversion}>{row.conversion_type}</td></tr>
          {#if detail?.axis_kind}
            <tr>
              <td>Axis</td>
              <td class="v axis-kind" title={AXIS_KIND_LABEL[detail.axis_kind] ?? ''}>
                {detail.axis_kind}
              </td>
            </tr>
            {#if detail.axis_ref}
              <tr>
                <td>Axis data</td>
                <td class="v">
                  <button
                    class="link ref"
                    onclick={() => onNavigate(detail.axis_ref)}
                    title="Show {detail.axis_ref}"
                  >{detail.axis_ref}</button>
                </td>
              </tr>
            {/if}
          {/if}
          {#if isMap && shape}
            <tr><td>Shape</td><td class="v shape">{shape}</td></tr>
          {/if}
          {#if row.point_count !== null && row.point_count !== undefined}
            <tr><td>{isMap ? 'Values' : 'Points'}</td><td class="v">{row.point_count}</td></tr>
          {/if}
          {#if !isVirtual}
            <tr>
              <td>In image</td>
              <td class="v">
                <span class="pres {row.presence}">{row.presence}</span>
              </td>
            </tr>
          {/if}
        </tbody>
      </table>

      <!-- ── Axes of a map, cuboid or cube ── -->
      {#if isMap && detail?.axes?.length}
        <div class="sub-header">Axes</div>
        <div class="points-wrap">
          <table class="points">
            <thead>
              <tr><th class="i">#</th><th>Kind</th><th class="r">Points</th></tr>
            </thead>
            <tbody>
              {#each detail.axes as ax, d}
                <tr>
                  <td class="i">{'XYZ45'[d] ?? d + 1}</td>
                  <td class="ax" title={AXIS_KIND_LABEL[ax.kind] ?? ''}>
                    {ax.kind}
                    {#if ax.reference}
                      <button class="link ref" onclick={() => onNavigate(ax.reference)}
                              title="Show {ax.reference}">{ax.reference}</button>
                    {/if}
                  </td>
                  <td class="r">{ax.points.length}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        <div class="sub-header">
          Values
          {#if detail?.value_unit}<span class="units">{detail.value_unit}</span>{/if}
        </div>
        <!-- A preview only: the pane is too narrow for a real grid, so this
             elides and the editor opens full size. -->
        <MapGrid {detail} {decimals} compact />
        <button class="open-map" onclick={() => onOpenMap(row.name)}>
          {detail?.values_editable ? 'Edit values…' : 'View values…'}
        </button>
      {/if}

      <!-- ── 1D points ── -->
      {#if points.length > 0}
        <div class="sub-header">
          Points
          {#if detail?.axis_unit || detail?.value_unit}
            <span class="units">
              {detail.axis_unit || '—'} → {detail.value_unit || '—'}
            </span>
          {/if}
        </div>
        <!-- Shape of the curve at a glance. Follows the draft while a cell is
             being typed into, so an edit is visible before it is committed. -->
        <div class="plot-wrap" bind:clientWidth={plotW}>
          {#if plot}
            <!-- The plot is a second view of the table below, not a second way
                 in: hovering only reads out a point, and clicking is a shortcut
                 to the same cell editor the table exposes to the keyboard. -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <svg
              class="plot"
              width={plotW}
              height={PLOT_H}
              role="img"
              aria-label="Plot of {row.name}"
              onpointermove={(e) => (hoverPoint = pointAt(e)?.i ?? null)}
              onpointerleave={() => (hoverPoint = null)}
              onclick={(e) => {
                const p = pointAt(e);
                if (p && detail?.values_editable) beginCell('value', p.i, points[p.i].value.phys);
              }}
            >
              {#if plot.zero !== null}
                <line class="zero" x1={PAD.l} y1={plot.zero} x2={plotW - PAD.r} y2={plot.zero} />
              {/if}
              <polyline class="line" points={plot.series[0].line} />
              {#each screen as p}
                <circle
                  class="dot"
                  class:on={hoverPoint === p.i}
                  class:editing={editingCell === `value:${p.i}` || editingCell === `axis:${p.i}`}
                  cx={p.cx}
                  cy={p.cy}
                  r={hoverPoint === p.i ? 3.5 : 2}
                />
              {/each}
              {#if hoverPoint !== null}
                {@const h = screen.find((p) => p.i === hoverPoint)}
                {#if h}
                  <line class="cursor" x1={h.cx} y1={PAD.t} x2={h.cx} y2={PLOT_H - PAD.b} />
                {/if}
              {/if}
            </svg>
            <div class="plot-cap">
              {#if hoverPoint !== null}
                {@const h = screen.find((p) => p.i === hoverPoint)}
                {#if h}
                  <span class="cap-i">#{h.i}</span>
                  <span>{fmtNum(h.x)}</span>
                  <span class="cap-arrow">→</span>
                  <span class="cap-y">{fmtNum(h.y)}</span>
                {/if}
              {:else}
                <span>{fmtNum(plot.xLo)} … {fmtNum(plot.xHi)}</span>
                <span class="cap-arrow">→</span>
                <span class="cap-y">{fmtNum(plot.yLo)} … {fmtNum(plot.yHi)}</span>
              {/if}
            </div>
          {/if}
        </div>

        <div class="points-wrap">
          <table class="points">
            <thead>
              <tr><th class="i">#</th><th>Axis</th><th class="r">Value</th></tr>
            </thead>
            <tbody>
              {#each points as p}
                <tr>
                  <td class="i">{p.i}</td>
                  {#each [
                    { target: 'axis',  pt: p.axis,  cls: 'ax', can: detail?.axis_editable },
                    { target: 'value', pt: p.value, cls: 'r',  can: detail?.values_editable },
                  ] as col}
                    <td class={col.cls}>
                      {#if !col.pt}
                        —
                      {:else if editingCell === `${col.target}:${p.i}`}
                        <!-- svelte-ignore a11y_autofocus -->
                        <input
                          class="cell-input"
                          class:invalid={!!cellError}
                          bind:value={cellDraft}
                          onkeydown={(e) => cellKey(e, col.target, p.i)}
                          onblur={() => commitCell(col.target, p.i)}
                          aria-label="{col.target} at point {p.i}"
                          spellcheck="false"
                          autofocus
                        />
                      {:else if col.can}
                        <button
                          class="cell"
                          onclick={() => beginCell(col.target, p.i, col.pt.phys)}
                          title="Edit"
                        >{atPrecision(col.pt.phys, col.pt.display)}</button>
                      {:else}
                        {atPrecision(col.pt.phys, col.pt.display)}
                      {/if}
                    </td>
                  {/each}
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .detail-panel {
    height: 100%;
    display: flex;
    flex-direction: column;
    font-family: 'Cascadia Code', 'SF Mono', 'Fira Code', 'Courier New', monospace;
    font-size: 12px;
    background: var(--c-bg);
    color: var(--c-text);
    overflow: hidden;
  }

  .panel-header {
    display: flex;
    align-items: center;
    padding: 5px 6px 5px 10px;
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--c-muted);
    background: var(--c-surface);
    border-bottom: 1px solid var(--c-hover);
    flex-shrink: 0;
    user-select: none;
  }

  .header-title { flex: 1; }

  .empty {
    color: var(--c-dim);
    text-align: center;
    margin-top: 2rem;
    font-family: 'Inter', sans-serif;
    font-size: 12px;
  }

  .detail-body {
    flex: 1;
    overflow-y: auto;
    padding: 10px;
    min-height: 0;
  }

  .name {
    font-size: 12px;
    color: var(--c-accent-t);
    word-break: break-all;
    line-height: 1.35;
  }

  .desc {
    font-family: 'Inter', sans-serif;
    font-size: 11px;
    color: var(--c-muted);
    margin-top: 3px;
    line-height: 1.45;
  }

  /* ── Editor ── */
  .edit-row {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 10px;
  }

  input, select {
    flex: 1;
    min-width: 0;
    background: var(--c-surface);
    border: 1px solid var(--c-border2);
    border-radius: 4px;
    color: var(--c-text);
    font-family: inherit;
    font-size: 12.5px;
    padding: 4px 6px;
    text-align: right;
  }

  select { text-align: left; }

  /* Strings run long — 41 characters here — so they get a smaller face than
     the numeric field and read left-to-right like text. */
  .text-input {
    font-size: 11.5px;
    text-align: left;
    letter-spacing: 0.01em;
  }

  .counter {
    margin-top: 5px;
    font-size: 10.5px;
    color: var(--c-dim);
    font-variant-numeric: tabular-nums;
  }

  .counter.full { color: var(--c-diff-changed); }

  .counter-note { color: var(--c-dim); opacity: 0.8; }

  input:focus, select:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  input.invalid { border-color: var(--c-err); }

  .unit { color: var(--c-muted); font-size: 11px; flex-shrink: 0; }

  /* ── Range slider ── */
  .slider {
    width: 100%;
    margin: 8px 0 0;
    padding: 0;
    accent-color: var(--c-accent);
    cursor: pointer;
    background: transparent;
    border: none;
    /* The shared input rule right-aligns text; irrelevant here but the border
       and padding are not, so they are cleared explicitly. */
  }

  .slider-ends {
    display: flex;
    justify-content: space-between;
    margin-top: 1px;
    font-size: 10px;
    color: var(--c-dim);
    font-variant-numeric: tabular-nums;
  }

  .apply {
    margin-top: 6px;
    width: 100%;
    background: var(--c-accent-b);
    color: #fff;
    border: none;
    border-radius: 4px;
    padding: 4px 0;
    font-family: 'Inter', sans-serif;
    font-size: 11.5px;
    cursor: pointer;
  }

  .apply:hover { background: var(--c-accent-h); }

  .err {
    margin-top: 5px;
    color: var(--c-err);
    font-size: 11px;
    font-family: 'Inter', sans-serif;
  }

  .value-static {
    margin-top: 10px;
    font-size: 15px;
    color: var(--c-text);
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
  }

  .value-static .v { word-break: break-all; }

  .value-static.out { color: var(--c-diff-changed); }

  .note {
    margin-top: 5px;
    color: var(--c-muted);
    font-family: 'Inter', sans-serif;
    font-size: 11px;
    line-height: 1.45;
  }

  /* ── Facts ── */
  .facts {
    width: 100%;
    margin-top: 12px;
    border-collapse: collapse;
  }

  .facts td {
    padding: 2px 0;
    font-size: 11.5px;
    vertical-align: top;
  }

  .facts td:first-child {
    color: var(--c-muted);
    white-space: nowrap;
    padding-right: 8px;
  }

  .facts .v { text-align: right; word-break: break-all; }
  .facts .raw  { color: var(--c-accent-t); }
  .facts .conv { color: var(--c-text2); }

  .link {
    background: none;
    border: none;
    padding: 0;
    color: var(--c-addr);
    font-family: inherit;
    font-size: 11.5px;
    cursor: pointer;
    text-decoration: underline;
    text-decoration-style: dotted;
  }

  .link:hover { color: var(--c-accent-t); }

  /* Object names run long, so the reference wraps rather than overflowing. */
  .ref {
    text-align: right;
    white-space: normal;
    word-break: break-all;
    line-height: 1.35;
  }

  .axis-kind { color: var(--c-text2); font-size: 11px; }
  .virt { color: var(--c-diff-ref-only); font-size: 11px; }

  /* ── Computed parameter ── */
  .formula {
    margin-top: 10px;
    padding: 6px 8px;
    background: var(--c-surface);
    border-left: 2px solid var(--c-diff-ref-only);
    border-radius: 0;
    font-size: 12px;
    color: var(--c-text);
    word-break: break-word;
    line-height: 1.4;
  }

  .deps {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    margin-top: 7px;
  }

  .deps-label {
    font-family: 'Inter', sans-serif;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--c-muted);
  }

  .deps .ref { text-align: left; }

  .pres { font-size: 10.5px; }
  .pres.full    { color: var(--c-diff-cmp-only); }
  .pres.partial { color: var(--c-diff-changed); }
  .pres.absent  { color: var(--c-dim); }
  .pres.unknown { color: var(--c-muted); font-style: italic; }

  /* ── Points table ── */
  .sub-header {
    display: flex;
    align-items: baseline;
    gap: 6px;
    margin: 14px 0 4px;
    font-family: 'Inter', sans-serif;
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--c-muted);
    border-top: 1px solid var(--c-hover);
    padding-top: 10px;
  }

  .units {
    text-transform: none;
    letter-spacing: 0;
    font-weight: 400;
    color: var(--c-dim);
  }

  .shape {
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.5px;
  }

  .open-map {
    align-self: flex-start;
    margin-top: 7px;
    background: var(--c-hover);
    border: 1px solid var(--c-border2);
    border-radius: 5px;
    color: var(--c-text);
    font-family: inherit;
    font-size: 12px;
    padding: 4px 12px;
    cursor: pointer;
  }

  .open-map:hover { background: var(--c-border2); }

  .plot-wrap {
    margin: 4px 0 2px;
  }

  .plot {
    display: block;
    background: var(--c-bg);
    border: 1px solid var(--c-border2);
    border-radius: 5px;
    cursor: crosshair;
    touch-action: none;
  }

  .plot .line {
    fill: none;
    stroke: var(--c-accent);
    stroke-width: 1.5;
    stroke-linejoin: round;
    stroke-linecap: round;
  }

  .plot .dot {
    fill: var(--c-accent);
    stroke: var(--c-bg);
    stroke-width: 1;
  }

  .plot .dot.on      { fill: var(--c-accent-h, var(--c-accent)); }
  .plot .dot.editing { fill: var(--c-diff-changed); }

  /* A zero crossing is worth seeing; a full grid would be noise at this size. */
  .plot .zero {
    stroke: var(--c-border2);
    stroke-width: 1;
    stroke-dasharray: 2 3;
  }

  .plot .cursor {
    stroke: var(--c-border2);
    stroke-width: 1;
  }

  .plot-cap {
    display: flex;
    gap: 5px;
    align-items: baseline;
    font-size: 10px;
    font-family: 'Cascadia Code', 'SF Mono', monospace;
    color: var(--c-muted);
    padding: 3px 2px 0;
    white-space: nowrap;
    overflow: hidden;
  }

  .cap-i     { color: var(--c-dim); }
  .cap-arrow { color: var(--c-dim); }
  .cap-y     { color: var(--c-text2); }

  .points-wrap { overflow-x: auto; }

  .points {
    width: 100%;
    border-collapse: collapse;
  }

  .points th {
    color: var(--c-dim);
    font-weight: 400;
    font-size: 10.5px;
    text-align: left;
    padding: 2px 4px;
    border-bottom: 1px solid var(--c-hover);
    position: sticky;
    top: 0;
    background: var(--c-bg);
  }

  .points td {
    padding: 2px 4px;
    font-size: 11.5px;
    border-bottom: 1px solid var(--c-ec1);
    white-space: nowrap;
  }

  /* An editable cell reads as plain text until hovered, so the table stays
     scannable rather than looking like a form. */
  .cell {
    background: none;
    border: none;
    padding: 0;
    margin: 0;
    color: inherit;
    font: inherit;
    cursor: text;
    width: 100%;
    text-align: inherit;
    border-radius: 2px;
  }

  .cell:hover      { background: var(--c-hover); }
  .cell:focus-visible {
    outline: 1px solid var(--c-accent);
    outline-offset: 0;
  }

  .cell-input {
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
    background: var(--c-bg);
    border: 1px solid var(--c-accent);
    border-radius: 2px;
    color: var(--c-text);
    font: inherit;
    padding: 0 2px;
    text-align: inherit;
  }

  .cell-input:focus { outline: none; }
  .cell-input.invalid { border-color: var(--c-err); }

  .points .i  { color: var(--c-dim); width: 22px; }
  .points .ax { color: var(--c-addr); }
  .points .r  { text-align: right; color: var(--c-text); }
  .points th.r { text-align: right; }
</style>
