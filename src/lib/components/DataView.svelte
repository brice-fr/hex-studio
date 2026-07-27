<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  import { tick } from 'svelte';
  import ParamDetail from './ParamDetail.svelte';
  import MapEditor from './MapEditor.svelte';

  /**
   * DataView — physical-value view of the image, driven by a loaded A2L.
   *
   * Replaces HexViewer in the viewer area when the user switches to data mode.
   * The rows are decoded in Rust; this component only filters and presents them.
   *
   * Props
   *   rows        – ParamRow[] from a2lList
   *   stats       – CoverageStats from a2lStats, or null
   *   detail      – ParamDetail for the selected 1D object, or null
   *   selected    – name of the selected row, or null
   *   loading     – decode in flight
   *   fontSize    – shared display preference
   *   showAddress – column visibility; the detail pane always shows the value
   *   showType    – …regardless of these, so nothing is ever unreachable
   *   showRaw     – …
   *   onSelect    – (name: string) => void
   *   onEditValue – (name: string, phys: number) => void
   *   onEditText  – (name: string, text: string) => void
   *   onGoto      – (addr: number) => void
   */
  let {
    rows        = [],
    stats       = null,
    detail      = null,
    selected    = null,
    loading     = false,
    fontSize    = 13,
    showAddress = false,
    showType    = true,
    showRaw     = false,
    onSelect    = (_name) => {},
    onEditValue = (_name, _phys) => {},
    onEditText  = (_name, _text) => {},
    onGoto      = (_addr) => {},
    onEditPoint = (_name, _target, _index, _phys) => {},
  } = $props();

  const ROW_H = 22;

  // Every category carries a marker, so names start at a common offset instead
  // of scalars sitting one glyph further left than everything else.
  const CATEGORY_GLYPH = {
    scalar: '·',   // middle dot — the quiet, ordinary case
    curve: '∿',    // sine wave
    map: '▦',      // a grid, for two or more dimensions
    ascii: '"',    // quote mark for a character array
    virtual: 'ƒ',  // computed by a formula
    unsupported: '!',
  };
  /** A shared axis is graduations, not a curve. */
  const AXIS_GLYPH = '≡';

  // ── Resizable columns ─────────────────────────────────────────────────────
  // Name, Address, Type and Raw carry explicit widths; Physical takes the
  // remainder.
  //
  // Address, Type and Raw are reference data rather than the focus of this
  // view, so their floors are roughly their own header label width — narrow
  // enough to get out of the way, but never so narrow that a heading spills
  // into its neighbour. Name keeps a workable floor since it is the column
  // being read.
  const COL_MIN     = { name: 190, address: 56, type: 32, raw: 30 };
  const PHYS_MIN    = 96;
  const COL_DEFAULT = { name: 360, address: 80, type: 60, raw: 88 };
  const COLS_LS_KEY = 'hex-studio.dataViewCols';

  /** Fixed-width columns currently shown; Physical is always last and flexes. */
  const visibleCols = $derived([
    { key: 'name', label: 'Name', right: false },
    ...(showAddress ? [{ key: 'address', label: 'Address', right: false }] : []),
    ...(showType    ? [{ key: 'type',    label: 'Type',    right: false }] : []),
    ...(showRaw     ? [{ key: 'raw',     label: 'Raw',     right: true  }] : []),
  ]);

  /** Grid gaps (one per boundary) plus the row's horizontal padding. */
  const gridChrome = $derived(visibleCols.length * 8 + 20);

  function loadCols() {
    try {
      const stored = JSON.parse(localStorage.getItem(COLS_LS_KEY) ?? 'null');
      if (stored) {
        return Object.fromEntries(
          Object.keys(COL_DEFAULT).map((k) => [
            k,
            Math.max(COL_MIN[k], Number(stored[k]) || COL_DEFAULT[k]),
          ]),
        );
      }
    } catch {
      // Malformed entry: fall through to defaults rather than fail to render.
    }
    return { ...COL_DEFAULT };
  }

  let colW  = $state(loadCols());
  let mainW = $state(0);

  // ── Resizable parameter pane ──────────────────────────────────────────────
  const DETAIL_MIN     = 200;
  const DETAIL_MAX     = 620;
  const DETAIL_DEFAULT = 250;
  const DETAIL_LS_KEY  = 'hex-studio.dataViewDetailW';

  function loadDetailW() {
    const n = Number(localStorage.getItem(DETAIL_LS_KEY));
    return Number.isFinite(n) && n >= DETAIL_MIN && n <= DETAIL_MAX ? n : DETAIL_DEFAULT;
  }

  let detailW = $state(loadDetailW());
  let bodyW   = $state(0);

  function setDetailW(px) {
    // Never let the pane squeeze the table below a usable width.
    const ceiling = bodyW ? Math.min(DETAIL_MAX, bodyW - COL_MIN.name - PHYS_MIN - 160) : DETAIL_MAX;
    detailW = Math.round(Math.min(Math.max(px, DETAIL_MIN), Math.max(DETAIL_MIN, ceiling)));
  }

  function persistDetailW() {
    try {
      localStorage.setItem(DETAIL_LS_KEY, String(detailW));
    } catch {
      // Non-fatal — the width just won't survive a restart.
    }
  }

  let paneDrag = $state(/** @type {{startX: number, startW: number}|null} */ (null));

  function startPaneResize(e) {
    e.preventDefault();
    paneDrag = { startX: e.clientX, startW: detailW };
    e.currentTarget.setPointerCapture(e.pointerId);
  }

  function movePaneResize(e) {
    if (!paneDrag) return;
    // The pane is on the right, so dragging left widens it.
    setDetailW(paneDrag.startW - (e.clientX - paneDrag.startX));
  }

  function endPaneResize(e) {
    if (!paneDrag) return;
    paneDrag = null;
    e.currentTarget.releasePointerCapture?.(e.pointerId);
    persistDetailW();
  }

  function paneKeyResize(e) {
    const step = e.shiftKey ? 40 : 12;
    if (e.key === 'ArrowLeft')       setDetailW(detailW + step);
    else if (e.key === 'ArrowRight') setDetailW(detailW - step);
    else return;
    e.preventDefault();
    persistDetailW();
  }

  const gridTemplate = $derived(
    visibleCols.map((c) => `${colW[c.key]}px`).join(' ') + ` minmax(${PHYS_MIN}px, 1fr)`
  );

  /** Largest width `key` may take before Physical would drop below its floor. */
  function maxWidth(key) {
    if (!mainW) return Infinity;
    const others = visibleCols
      .filter((c) => c.key !== key)
      .reduce((sum, c) => sum + colW[c.key], 0);
    return Math.max(COL_MIN[key], mainW - others - PHYS_MIN - gridChrome);
  }

  function setWidth(key, px) {
    colW = { ...colW, [key]: Math.min(maxWidth(key), Math.max(COL_MIN[key], Math.round(px))) };
  }

  function persistCols() {
    try {
      localStorage.setItem(COLS_LS_KEY, JSON.stringify(colW));
    } catch {
      // Storage full or disabled — widths simply won't survive a restart.
    }
  }

  // Must be $state: the template reads it to highlight the grip being dragged.
  let drag = $state(/** @type {{key: string, startX: number, startW: number}|null} */ (null));

  function startResize(e, key) {
    e.preventDefault();
    e.stopPropagation();
    drag = { key, startX: e.clientX, startW: colW[key] };
    e.currentTarget.setPointerCapture(e.pointerId);
  }

  function moveResize(e) {
    if (!drag) return;
    setWidth(drag.key, drag.startW + (e.clientX - drag.startX));
  }

  function endResize(e) {
    if (!drag) return;
    drag = null;
    e.currentTarget.releasePointerCapture?.(e.pointerId);
    persistCols();
  }

  /** Keyboard resizing, so the grips are not mouse-only. */
  function keyResize(e, key) {
    const step = e.shiftKey ? 32 : 8;
    if (e.key === 'ArrowLeft')       setWidth(key, colW[key] - step);
    else if (e.key === 'ArrowRight') setWidth(key, colW[key] + step);
    else return;
    e.preventDefault();
    persistCols();
  }

  function resetWidth(key) {
    setWidth(key, COL_DEFAULT[key]);
    persistCols();
  }

  // Narrowing the window reclaims space from Name — the most elastic column —
  // rather than letting the row overflow and clip the Physical value.
  $effect(() => {
    if (!mainW) return;
    const others = visibleCols
      .filter((c) => c.key !== 'name')
      .reduce((sum, c) => sum + colW[c.key], 0);
    const maxName = mainW - others - PHYS_MIN - gridChrome;
    if (colW.name > maxName && maxName >= COL_MIN.name) {
      colW = { ...colW, name: Math.round(maxName) };
    }
  });

  // ── Decimal precision ─────────────────────────────────────────────────────
  // `null` means follow each parameter's A2L FORMAT, which is the default and
  // the only setting that respects what the description actually declares.
  // A number overrides it for every non-integer value; whole numbers such as a
  // UBYTE count are left alone so the table doesn't fill with "20.000".
  const DEC_LS_KEY = 'hex-studio.dataViewDecimals';
  const DEC_MAX    = 6;

  function loadDecimals() {
    const raw = localStorage.getItem(DEC_LS_KEY);
    if (raw === null || raw === 'auto') return null;
    const n = Number(raw);
    return Number.isInteger(n) && n >= 0 && n <= DEC_MAX ? n : null;
  }

  let decimals = $state(/** @type {number|null} */ (loadDecimals()));

  function setDecimals(next) {
    decimals = next;
    try {
      localStorage.setItem(DEC_LS_KEY, next === null ? 'auto' : String(next));
    } catch {
      // Non-fatal: the setting just won't survive a restart.
    }
  }

  /** Steps auto → 0 → 1 … → DEC_MAX, and back down to auto. */
  function stepDecimals(delta) {
    if (decimals === null) { if (delta > 0) setDecimals(0); return; }
    const next = decimals + delta;
    setDecimals(next < 0 ? null : Math.min(DEC_MAX, next));
  }

  /**
   * True when a row's value is real-valued rather than a whole count.
   * A declared decimal in the A2L format counts, so a scaled value that lands
   * exactly on 4.0 is still treated as a float.
   */
  function isFractional(row) {
    if (row.phys_num !== null && row.phys_num !== undefined) {
      return row.display.includes('.') || !Number.isInteger(row.phys_num);
    }
    if (row.phys_min !== null && row.phys_min !== undefined) {
      return row.display.includes('.')
          || !Number.isInteger(row.phys_min)
          || !Number.isInteger(row.phys_max);
    }
    return false;
  }

  /** The Physical column text, honouring any decimal override. */
  function physText(row) {
    if (decimals === null || !isFractional(row)) return row.display;
    if (row.phys_num !== null && row.phys_num !== undefined) {
      return row.phys_num.toFixed(decimals);
    }
    if (row.phys_min !== null && row.phys_min !== undefined) {
      return `${row.phys_min.toFixed(decimals)} … ${row.phys_max.toFixed(decimals)}`;
    }
    return row.display;
  }

  let query    = $state('');
  let category = $state('all');

  /** The full-size map grid. Closes on a new selection: it is bound to one
   *  object, and leaving it open over a different one would be a lie. */
  let mapOpen = $state(false);
  $effect(() => { void selected; mapOpen = false; });
  let scrollEl = $state(/** @type {HTMLDivElement|null} */ (null));
  let scrollTop = $state(0);
  let viewportH = $state(400);

  /** A computed parameter was never meant to be in the image. */
  const isVirtual = (r) => r.category === 'virtual';
  /**
   * Declared somewhere real, but those bytes are not in this image.
   *
   * Excludes both computed parameters, which were never stored, and objects
   * whose extent could not be resolved, where nothing is known either way.
   */
  const isMissing = (r) => r.presence === 'absent' && !isVirtual(r);

  /** A shared AXIS_PTS object rather than a curve of its own. */
  const isAxis = (r) => r.kind === 'axis_pts';

  /**
   * What each sidebar entry selects.
   *
   * `category` describes the shape of the data and `kind` says which A2L block
   * it came from, so a shared axis can be told apart from a curve without
   * either concept having to absorb the other.
   *
   * Counts and filtering both run off this one map, so a sidebar number can
   * never disagree with the list it opens.
   */
  const CATEGORY_MATCH = {
    all:         () => true,
    scalar:      (r) => r.category === 'scalar',
    curve:       (r) => r.category === 'curve' && !isAxis(r),
    axis:        isAxis,
    map:         (r) => r.category === 'map',
    ascii:       (r) => r.category === 'ascii',
    virtual:     (r) => r.category === 'virtual',
    unsupported: (r) => r.category === 'unsupported',
    absent:      (r) => isMissing(r),
  };

  const counts = $derived.by(() => {
    const c = Object.fromEntries(Object.keys(CATEGORY_MATCH).map((k) => [k, 0]));
    for (const r of rows) {
      for (const id in CATEGORY_MATCH) {
        if (CATEGORY_MATCH[id](r)) c[id]++;
      }
    }
    return c;
  });

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return rows.filter((r) => {
      const match = CATEGORY_MATCH[category] ?? CATEGORY_MATCH.all;
      if (!match(r)) return false;
      if (!q) return true;
      return r.name.toLowerCase().includes(q)
          || r.description.toLowerCase().includes(q);
    });
  });

  // Windowed rendering — A2L files routinely describe thousands of parameters.
  const window_ = $derived.by(() => {
    const start = Math.max(0, Math.floor(scrollTop / ROW_H) - 6);
    const count = Math.ceil(viewportH / ROW_H) + 12;
    return { start, items: filtered.slice(start, start + count) };
  });

  const selectedRow = $derived(rows.find((r) => r.name === selected) ?? null);

  // Keep the selected row reachable when the filter changes under it.
  $effect(() => {
    const n = filtered.length;
    void n;
    if (scrollEl && scrollTop > n * ROW_H) {
      scrollEl.scrollTop = 0;
      scrollTop = 0;
    }
  });

  function hex32(n) { return '0x' + n.toString(16).padStart(8, '0').toUpperCase(); }
  function pct(v)   { return `${v.toFixed(1)} %`; }
  function num(v)   { return v.toLocaleString(); }

  function handleKey(e, name) {
    if (e.key === 'Enter' || e.key === ' ') { onSelect(name); e.preventDefault(); }
  }

  /**
   * Jump to another parameter, e.g. the axis a curve refers to.
   *
   * The target may be filtered out of view — a shared AXIS_PTS object is not in
   * the "1D curves" category the user was probably browsing — so the filters are
   * cleared first, then the row is centred.
   */
  async function navigateTo(name) {
    query = '';
    category = 'all';
    onSelect(name);
    await tick();
    const idx = filtered.findIndex((r) => r.name === name);
    if (idx < 0 || !scrollEl) return;
    const top = Math.max(0, idx * ROW_H - viewportH / 2 + ROW_H / 2);
    scrollEl.scrollTop = top;
    scrollTop = top;
  }
</script>

<div class="data-view" style="--fs: {fontSize}px; --gt: {gridTemplate}">

  <!-- ── Coverage statistics ── -->
  {#if stats}
    <div class="stats">
      <div class="stat">
        <span class="s-label">In image</span>
        <span class="s-val ok">{stats.present_full} / {stats.total_objects}</span>
      </div>
      <div class="stat">
        <span class="s-label">Partial</span>
        <span class="s-val warn">{stats.present_partial}</span>
      </div>
      <div class="stat">
        <span class="s-label">Absent</span>
        <span class="s-val dim">{stats.absent}</span>
      </div>
      <!-- Counted apart from the three above: a computed parameter is never
           stored, so it is neither present nor missing. -->
      <div class="stat" title="Computed parameters — never stored, so not counted as present or absent">
        <span class="s-label">Virtual</span>
        <span class="s-val virt">{stats.virtuals}</span>
      </div>
      <div class="stat">
        <span class="s-label">Described</span>
        <span class="s-val">{num(stats.described_present_bytes)} B</span>
      </div>
      <div class="stat">
        <span class="s-label">Undescribed</span>
        <span class="s-val">{num(stats.undescribed_bytes)} B</span>
      </div>
      <div class="stat">
        <span class="s-label">Coverage</span>
        <span class="s-val accent">{pct(stats.coverage_pct)}</span>
      </div>
    </div>
  {/if}

  <div class="dv-body" bind:clientWidth={bodyW}>
    <!-- ── Category sidebar ── -->
    <aside class="dv-side">
      <div class="side-title">Categories</div>
      {#each [
        { id: 'all',         label: 'All',         n: counts.all },
        { id: 'scalar',      label: 'Scalars',     n: counts.scalar },
        // Ordered by dimension: a scalar, then the breakpoints a curve is
        // indexed by, then the curves themselves, then the grids.
        { id: 'axis',        label: 'Axes',        n: counts.axis },
        { id: 'curve',       label: '1D curves',   n: counts.curve },
        { id: 'map',         label: 'Maps & cubes', n: counts.map },
        { id: 'ascii',       label: 'Strings',     n: counts.ascii },
        { id: 'virtual',     label: 'Virtual',     n: counts.virtual },
        { id: 'unsupported', label: 'Unsupported', n: counts.unsupported },
        { id: 'absent',      label: 'Not in image', n: counts.absent },
      ] as cat}
        <button
          class="cat"
          class:active={category === cat.id}
          onclick={() => (category = cat.id)}
        >
          <span class="c-label">{cat.label}</span>
          <span class="c-n">{cat.n}</span>
        </button>
      {/each}
    </aside>

    <!-- ── Parameter table ── -->
    <div class="dv-main" bind:clientWidth={mainW}>
      <div class="dv-filter">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <circle cx="11" cy="11" r="7"/><line x1="16.5" y1="16.5" x2="22" y2="22"/>
        </svg>
        <input
          type="text"
          bind:value={query}
          placeholder="Filter parameters…"
          spellcheck="false"
          aria-label="Filter parameters"
        />
        {#if query}
          <button class="clear" onclick={() => (query = '')} aria-label="Clear filter">×</button>
        {/if}
        <span class="shown">{filtered.length}</span>

        <!-- Decimal precision for fractional values -->
        <div class="dec" role="group" aria-label="Decimal places">
          <span class="dec-label">dec</span>
          <button
            class="dec-btn"
            onclick={() => stepDecimals(-1)}
            disabled={decimals === null}
            title="Fewer decimals"
            aria-label="Fewer decimals"
          >−</button>
          <span
            class="dec-val"
            title={decimals === null
              ? 'Following each parameter’s A2L FORMAT'
              : `Showing ${decimals} decimal${decimals === 1 ? '' : 's'} for fractional values`}
          >{decimals === null ? 'auto' : decimals}</span>
          <button
            class="dec-btn"
            onclick={() => stepDecimals(1)}
            disabled={decimals === DEC_MAX}
            title="More decimals"
            aria-label="More decimals"
          >+</button>
        </div>
      </div>

      <div class="dv-head">
        {#each visibleCols as col (col.key)}
          <span class="h-cell" class:r={col.right}>
            <span class="h-label">{col.label}</span>
            <!-- A separator that is focusable is a widget per ARIA (a window
                 splitter), so the tabindex is correct here despite the lint. -->
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
            <span
              class="grip"
              class:active={drag?.key === col.key}
              role="separator"
              aria-orientation="vertical"
              aria-label="Resize {col.label} column"
              tabindex="0"
              onpointerdown={(e) => startResize(e, col.key)}
              onpointermove={moveResize}
              onpointerup={endResize}
              onpointercancel={endResize}
              onkeydown={(e) => keyResize(e, col.key)}
              ondblclick={() => resetWidth(col.key)}
              title="Drag to resize · double-click to reset"
            ></span>
          </span>
        {/each}
        <span class="h-cell r"><span class="h-label">Physical</span></span>
      </div>

      <div
        class="dv-scroll"
        bind:this={scrollEl}
        bind:clientHeight={viewportH}
        onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
      >
        {#if loading}
          <p class="msg">Decoding…</p>
        {:else if rows.length === 0}
          <p class="msg">No parameters — load an A2L file</p>
        {:else if filtered.length === 0}
          <p class="msg">Nothing matches this filter</p>
        {:else}
          <div class="spacer" style="height: {filtered.length * ROW_H}px">
            <div class="rows" style="transform: translateY({window_.start * ROW_H}px)">
              {#each window_.items as row (row.name)}
                <!-- svelte-ignore a11y_interactive_supports_focus -->
                <div
                  class="row"
                  class:sel={row.name === selected}
                  class:muted={isMissing(row)}
                  role="button"
                  tabindex="0"
                  onclick={() => onSelect(row.name)}
                  onkeydown={(e) => handleKey(e, row.name)}
                  title={row.description || row.name}
                >
                  <span class="c-name"><span
                      class="glyph {isAxis(row) ? 'axis' : row.category}"
                      aria-hidden="true"
                    >{isAxis(row) ? AXIS_GLYPH : (CATEGORY_GLYPH[row.category] ?? '·')}</span>{row.name}</span>
                  <!-- A computed parameter's declared address is a placeholder,
                       so showing 0x00000000 would only mislead. -->
                  {#if showAddress}<span class="c-addr">{isVirtual(row) ? '—' : hex32(row.address)}</span>{/if}
                  {#if showType}<span class="c-type">{row.datatype}</span>{/if}
                  {#if showRaw}<span class="c-raw r">{row.raw_hex ?? '—'}</span>{/if}
                  <span class="c-phys r">
                    <span class="v">{physText(row)}</span>{#if row.unit}<span class="u">{row.unit}</span>{/if}
                  </span>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    </div>

    <!-- ── Parameter pane, with a splitter on its leading edge ── -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="v-split"
      class:active={!!paneDrag}
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize parameter pane"
      tabindex="0"
      onpointerdown={startPaneResize}
      onpointermove={movePaneResize}
      onpointerup={endPaneResize}
      onpointercancel={endPaneResize}
      onkeydown={paneKeyResize}
      ondblclick={() => { setDetailW(DETAIL_DEFAULT); persistDetailW(); }}
      title="Drag to resize · double-click to reset"
    ></div>

    <aside class="dv-detail" style="width: {detailW}px">
      <ParamDetail
        row={selectedRow}
        {detail}
        {decimals}
        onEditValue={(phys) => selectedRow && onEditValue(selectedRow.name, phys)}
        onEditText={(text) => selectedRow && onEditText(selectedRow.name, text)}
        {onGoto}
        onNavigate={navigateTo}
        onEditPoint={(target, index, phys) =>
          selectedRow && onEditPoint(selectedRow.name, target, index, phys)}
        onOpenMap={() => (mapOpen = true)}
      />
    </aside>
  </div>
</div>

<MapEditor
  open={mapOpen}
  row={selectedRow}
  {detail}
  {decimals}
  onNavigate={navigateTo}
  onEditPoint={(target, index, phys) =>
    selectedRow && onEditPoint(selectedRow.name, target, index, phys)}
  onClose={() => (mapOpen = false)}
/>

<style>
  .data-view {
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--c-bg);
    color: var(--c-text);
    font-family: 'Cascadia Code', 'SF Mono', 'Fira Code', 'Courier New', monospace;
  }

  /* ── Statistics strip ── */
  .stats {
    display: flex;
    flex-wrap: wrap;
    flex-shrink: 0;
    border-bottom: 1px solid var(--c-hover);
    background: var(--c-surface);
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px 14px;
    border-right: 1px solid var(--c-hover);
    min-width: 92px;
  }

  .s-label {
    font-family: 'Inter', sans-serif;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--c-dim);
  }

  .s-val { font-size: 13px; color: var(--c-text); }
  .s-val.ok     { color: var(--c-diff-cmp-only); }
  .s-val.warn   { color: var(--c-diff-changed); }
  .s-val.dim    { color: var(--c-muted); }
  .s-val.virt   { color: var(--c-diff-ref-only); }
  .s-val.accent { color: var(--c-accent-t); }

  /* ── Body: sidebar | table | detail ── */
  .dv-body {
    flex: 1;
    display: flex;
    min-height: 0;
    overflow: hidden;
  }

  .dv-side {
    width: 150px;
    flex-shrink: 0;
    border-right: 1px solid var(--c-hover);
    padding: 6px 4px;
    overflow-y: auto;
    background: var(--c-bg);
  }

  .side-title {
    font-family: 'Inter', sans-serif;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--c-muted);
    padding: 2px 8px 6px;
  }

  .cat {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    width: 100%;
    background: none;
    border: none;
    border-radius: 4px;
    padding: 4px 8px;
    color: var(--c-text2);
    font-family: 'Inter', sans-serif;
    font-size: 11.5px;
    cursor: pointer;
    text-align: left;
  }

  .cat:hover { background: var(--c-ec1); }

  .cat.active {
    background: var(--c-accent-bg);
    color: var(--c-accent-t);
  }

  .c-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .c-n     { color: var(--c-dim); font-size: 10.5px; flex-shrink: 0; }
  .cat.active .c-n { color: var(--c-accent-t); }

  .dv-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* ── Filter bar ── */
  .dv-filter {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border-bottom: 1px solid var(--c-hover);
    flex-shrink: 0;
  }

  .dv-filter svg { width: 13px; height: 13px; color: var(--c-dim); flex-shrink: 0; }

  .dv-filter input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    color: var(--c-text);
    font-family: inherit;
    font-size: 12px;
    padding: 2px 0;
  }

  .dv-filter input:focus { outline: none; }
  .dv-filter input::placeholder { color: var(--c-dim); }

  .clear {
    background: none;
    border: none;
    color: var(--c-dim);
    cursor: pointer;
    font-size: 15px;
    line-height: 1;
    padding: 0 4px;
  }

  .clear:hover { color: var(--c-text2); }

  .shown {
    color: var(--c-dim);
    font-size: 10.5px;
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }

  /* ── Decimal stepper ── */
  .dec {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
    margin-left: 4px;
    padding-left: 8px;
    border-left: 1px solid var(--c-hover);
  }

  .dec-label {
    font-family: 'Inter', sans-serif;
    font-size: 9.5px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--c-dim);
    margin-right: 2px;
  }

  .dec-btn {
    width: 17px;
    height: 17px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 1px solid var(--c-border2);
    border-radius: 3px;
    color: var(--c-text2);
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
    padding: 0;
  }

  .dec-btn:hover:not(:disabled) {
    background: var(--c-hover);
    color: var(--c-text);
    border-color: var(--c-accent);
  }

  .dec-btn:disabled { opacity: 0.3; cursor: default; }

  .dec-val {
    min-width: 26px;
    text-align: center;
    font-size: 10.5px;
    color: var(--c-accent-t);
    font-variant-numeric: tabular-nums;
  }

  /* ── Table ── */
  .dv-head, .row {
    display: grid;
    grid-template-columns: var(--gt);
    gap: 8px;
    padding: 0 10px;
    align-items: center;
  }

  .dv-head {
    height: 22px;
    flex-shrink: 0;
    font-family: 'Inter', sans-serif;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--c-dim);
    background: var(--c-surface);
    border-bottom: 1px solid var(--c-hover);
    user-select: none;
  }

  /* Header cells clip their own label, so a narrow column can never let one
     bleed into the next heading. */
  .h-cell {
    position: relative;
    min-width: 0;
    height: 100%;
    display: flex;
    align-items: center;
  }

  .h-cell.r { justify-content: flex-end; }

  .h-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Sits over the gap between two columns so the hit area is comfortable
     without shifting the layout. */
  .grip {
    position: absolute;
    top: 0;
    right: -8px;
    width: 9px;
    height: 100%;
    cursor: col-resize;
    z-index: 1;
  }

  .grip::after {
    content: '';
    position: absolute;
    top: 4px;
    bottom: 4px;
    left: 4px;
    width: 1px;
    background: var(--c-border2);
    transition: background 0.1s;
  }

  .grip:hover::after,
  .grip:focus-visible::after,
  .grip.active::after {
    top: 0;
    bottom: 0;
    background: var(--c-accent);
  }

  .grip:focus-visible { outline: none; }

  .dv-scroll {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    min-height: 0;
    position: relative;
  }

  .spacer { position: relative; }
  .rows   { position: absolute; top: 0; left: 0; right: 0; }

  .row {
    height: 22px;
    font-size: var(--fs);
    cursor: pointer;
    border-bottom: 1px solid var(--c-ec1);
    white-space: nowrap;
  }

  .row:hover { background: var(--c-ec1); }

  .row.sel { background: var(--c-accent-bg); }
  .row.sel .c-name, .row.sel .c-phys { color: var(--c-accent-t); }

  .row.muted { opacity: 0.45; }

  /* A touch smaller than the physical value, which is what the eye should land
     on. Tracks the font-size preference rather than pinning a literal size. */
  .c-name {
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--c-text);
    font-size: calc(var(--fs) - 1px);
  }

  /* Fixed width so the markers form a column and every name lines up. */
  .glyph {
    display: inline-block;
    width: 9px;
    margin-right: 4px;
    text-align: center;
    color: var(--c-muted);
  }

  .glyph.scalar      { color: var(--c-dim); }
  .glyph.axis        { color: var(--c-addr); }
  .glyph.map         { color: var(--c-accent, #4a9eff); }
  .glyph.virtual     { color: var(--c-diff-ref-only); }
  .glyph.unsupported { color: var(--c-diff-changed); }

  /* Address, Type and Raw are supporting detail: same small size so none of
     them competes with the name and physical value. All three clip rather than
     spill, so they stay legible at their narrowest. */
  .c-addr, .c-type, .c-raw {
    font-size: 10.5px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .c-addr { color: var(--c-addr); }
  .c-type { color: var(--c-muted); }
  .c-raw  { color: var(--c-text2); }
  /* Value and unit are separate spans so the gap is a real margin rather than
     a collapsible text space. */
  .c-phys {
    color: var(--c-text);
    overflow: hidden;
    display: flex;
    align-items: baseline;
    justify-content: flex-end;
    gap: 7px;
    min-width: 0;
  }

  .c-phys .v {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .u {
    color: var(--c-muted);
    font-size: 10.5px;
    flex-shrink: 0;
  }

  .r { text-align: right; }

  .msg {
    color: var(--c-dim);
    text-align: center;
    margin-top: 2rem;
    font-family: 'Inter', sans-serif;
    font-size: 12px;
  }

  /* ── Parameter pane ── */
  .dv-detail {
    flex-shrink: 0;
    overflow: hidden;
  }

  /* Splitter sits where the pane's border used to be and draws it, so the
     divider and the hit area are the same line. */
  .v-split {
    width: 5px;
    flex-shrink: 0;
    cursor: col-resize;
    background: var(--c-hover);
    position: relative;
    transition: background 0.1s;
  }

  .v-split:hover,
  .v-split:focus-visible,
  .v-split.active { background: var(--c-accent); }

  .v-split:focus-visible { outline: none; }
</style>
