<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  import ParamDetail from './ParamDetail.svelte';

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
    onSelect    = (_name) => {},
    onEditValue = (_name, _phys) => {},
    onEditText  = (_name, _text) => {},
    onGoto      = (_addr) => {},
  } = $props();

  const ROW_H = 22;

  let query    = $state('');
  let category = $state('all');
  let scrollEl = $state(/** @type {HTMLDivElement|null} */ (null));
  let scrollTop = $state(0);
  let viewportH = $state(400);

  const counts = $derived.by(() => {
    const c = { all: rows.length, scalar: 0, curve: 0, unsupported: 0, absent: 0 };
    for (const r of rows) {
      if (r.category === 'scalar')           c.scalar++;
      else if (r.category === 'curve')       c.curve++;
      else if (r.category === 'unsupported') c.unsupported++;
      if (r.presence === 'absent')           c.absent++;
    }
    return c;
  });

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return rows.filter((r) => {
      if (category === 'absent') {
        if (r.presence !== 'absent') return false;
      } else if (category !== 'all' && r.category !== category) {
        return false;
      }
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

  function hex32(n) { return n.toString(16).padStart(8, '0').toUpperCase(); }
  function pct(v)   { return `${v.toFixed(1)} %`; }
  function num(v)   { return v.toLocaleString(); }

  function handleKey(e, name) {
    if (e.key === 'Enter' || e.key === ' ') { onSelect(name); e.preventDefault(); }
  }
</script>

<div class="data-view" style="--fs: {fontSize}px">

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

  <div class="dv-body">
    <!-- ── Category sidebar ── -->
    <aside class="dv-side">
      <div class="side-title">Categories</div>
      {#each [
        { id: 'all',         label: 'All',         n: counts.all },
        { id: 'scalar',      label: 'Scalars',     n: counts.scalar },
        { id: 'curve',       label: '1D curves',   n: counts.curve },
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
    <div class="dv-main">
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
      </div>

      <div class="dv-head">
        <span>Name</span>
        <span>Address</span>
        <span>Type</span>
        <span class="r">Raw</span>
        <span class="r">Physical</span>
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
                  class:muted={row.presence === 'absent'}
                  role="button"
                  tabindex="0"
                  onclick={() => onSelect(row.name)}
                  onkeydown={(e) => handleKey(e, row.name)}
                  title={row.description || row.name}
                >
                  <span class="c-name">
                    {#if row.category === 'curve'}<span class="glyph" aria-hidden="true">∿</span>{/if}
                    {#if row.category === 'unsupported'}<span class="glyph warn" aria-hidden="true">!</span>{/if}
                    {row.name}
                  </span>
                  <span class="c-addr">{hex32(row.address)}</span>
                  <span class="c-type">{row.datatype}</span>
                  <span class="c-raw r">{row.raw_hex ?? '—'}</span>
                  <span class="c-phys r">
                    {row.display}{#if row.unit}<span class="u"> {row.unit}</span>{/if}
                  </span>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    </div>

    <!-- ── Detail pane ── -->
    <aside class="dv-detail">
      <ParamDetail
        row={selectedRow}
        {detail}
        onEditValue={(phys) => selectedRow && onEditValue(selectedRow.name, phys)}
        onEditText={(text) => selectedRow && onEditText(selectedRow.name, text)}
        {onGoto}
      />
    </aside>
  </div>
</div>

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

  /* ── Table ── */
  .dv-head, .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 84px 68px 92px 124px;
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

  .c-name {
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--c-text);
  }

  .glyph { color: var(--c-muted); margin-right: 3px; }
  .glyph.warn { color: var(--c-diff-changed); }

  .c-addr { color: var(--c-addr); font-size: 11.5px; }
  .c-type { color: var(--c-muted); font-size: 10.5px; overflow: hidden; text-overflow: ellipsis; }
  .c-raw  { color: var(--c-text2); font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; }
  .c-phys { color: var(--c-text); overflow: hidden; text-overflow: ellipsis; }
  .u      { color: var(--c-muted); font-size: 10.5px; }

  .r { text-align: right; }

  .msg {
    color: var(--c-dim);
    text-align: center;
    margin-top: 2rem;
    font-family: 'Inter', sans-serif;
    font-size: 12px;
  }

  /* ── Detail pane ── */
  .dv-detail {
    width: 250px;
    flex-shrink: 0;
    border-left: 1px solid var(--c-hover);
    overflow: hidden;
  }
</style>
