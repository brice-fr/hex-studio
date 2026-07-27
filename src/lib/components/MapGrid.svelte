<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  /**
   * MapGrid — a 2D slice of a multi-dimensional parameter.
   *
   * One renderer serves both places the grid appears: `compact` gives the
   * read-only, truncated preview in the detail pane, and the full form gives
   * the editable grid in the map editor. Keeping them the same component is
   * what stops the preview and the editor drifting apart on which cell is
   * which.
   *
   * Props
   *   detail       – ParamDetail from the backend
   *   decimals     – decimal override, or null for the A2L FORMAT
   *   compact      – read-only and truncated, for the detail pane
   *   shaded       – colour cells on a value ramp
   *   slice        – subscripts for dimensions 2 and up
   *   onEditPoint  – (target, index, phys) => void
   *   onNavigate   – (name) => void   select another parameter
   */
  import { sliceIndices, valueExtent, shadeOf, axisAccess } from '$lib/mapGrid.js';

  let {
    detail      = null,
    decimals    = null,
    compact     = false,
    shaded      = true,
    slice       = /** @type {number[]} */ ([]),
    onEditPoint = (_target, _index, _phys) => {},
    onNavigate  = (_name) => {},
  } = $props();

  /** How much of the grid the detail pane shows before eliding. */
  const MAX_C = 6;
  const MAX_R = 6;

  const dims   = $derived(detail?.dims ?? []);
  const values = $derived(detail?.values ?? []);
  const axes   = $derived(detail?.axes ?? []);

  const extent = $derived(valueExtent(values));
  const rows   = $derived(sliceIndices(dims, slice));

  /** Breakpoint labels for one dimension, falling back to the index when the
   *  object has no axis there — a value block still needs column headings. */
  function heads(d, n) {
    const pts = axes[d]?.points ?? [];
    return Array.from({ length: n }, (_, i) => pts[i]?.display ?? String(i));
  }

  const xHeads = $derived(heads(0, dims[0] ?? 0));
  const yHeads = $derived(heads(1, dims[1] ?? 1));

  const xAccess = $derived(axisAccess(axes[0]));
  const yAccess = $derived(axisAccess(axes[1]));

  // Elision only applies to the preview; the editor scrolls instead.
  const shownC = $derived(compact ? Math.min(MAX_C, dims[0] ?? 0) : (dims[0] ?? 0));
  const shownR = $derived(compact ? Math.min(MAX_R, rows.length) : rows.length);
  const moreC  = $derived((dims[0] ?? 0) - shownC);
  const moreR  = $derived(rows.length - shownR);

  function fmt(pt) {
    if (!pt) return '—';
    if (decimals === null) return pt.display;
    if (typeof pt.phys !== 'number' || !Number.isFinite(pt.phys)) return pt.display;
    if (Number.isInteger(pt.phys) && !String(pt.display).includes('.')) return pt.display;
    return pt.phys.toFixed(decimals);
  }

  // ── Editing ───────────────────────────────────────────────────────────────
  /** The cell being edited, as `"value:12"` or `"axis:0:3"`. */
  let editing = $state(/** @type {string|null} */ (null));
  let draft   = $state('');
  let invalid = $state(false);

  const valuesEditable = $derived(!compact && !!detail?.values_editable);

  function begin(key, current) {
    editing = key;
    draft   = String(current ?? '');
    invalid = false;
  }

  function cancel() {
    editing = null;
    invalid = false;
  }

  /** Commit `draft` to `target`, refusing anything that is not a number. */
  function commit(target, index) {
    const t = draft.trim();
    const v = Number(t);
    if (t === '' || Number.isNaN(v)) {
      invalid = true;
      return;
    }
    editing = null;
    invalid = false;
    onEditPoint(target, index, v);
  }

  function key(e, target, index) {
    if (e.key === 'Enter')  { commit(target, index); e.preventDefault(); }
    if (e.key === 'Escape') { cancel(); e.preventDefault(); }
  }
</script>

{#if rows.length}
  <div class="grid-wrap" class:compact>
    <table class="grid">
      <thead>
        <tr>
          <th class="corner" title={dims.join(' × ')}>
            {#if !compact}<span class="corner-dim">{dims.join('×')}</span>{/if}
          </th>
          {#each Array(shownC) as _, x}
            <th
              class="xh"
              class:ro={!xAccess.editable}
              title={xAccess.why || `X ${x}`}
            >
              {#if editing === `axis:0:${x}`}
                <!-- svelte-ignore a11y_autofocus -->
                <input
                  class="cell-input"
                  class:invalid
                  bind:value={draft}
                  onkeydown={(e) => key(e, { axis: 0 }, x)}
                  onblur={() => commit({ axis: 0 }, x)}
                  aria-label="X breakpoint {x}"
                  spellcheck="false"
                  autofocus
                />
              {:else if xAccess.editable && !compact}
                <button class="hbtn" onclick={() => begin(`axis:0:${x}`, axes[0]?.points[x]?.phys)}
                        title="Edit X breakpoint {x}">{xHeads[x]}</button>
              {:else}
                {xHeads[x]}
              {/if}
            </th>
          {/each}
          {#if moreC > 0}<th class="more">+{moreC}</th>{/if}
        </tr>
      </thead>
      <tbody>
        {#each rows.slice(0, shownR) as row, y}
          <tr>
            <th class="yh" class:ro={!yAccess.editable} title={yAccess.why || `Y ${y}`}>
              {#if editing === `axis:1:${y}`}
                <!-- svelte-ignore a11y_autofocus -->
                <input
                  class="cell-input"
                  class:invalid
                  bind:value={draft}
                  onkeydown={(e) => key(e, { axis: 1 }, y)}
                  onblur={() => commit({ axis: 1 }, y)}
                  aria-label="Y breakpoint {y}"
                  spellcheck="false"
                  autofocus
                />
              {:else if yAccess.editable && !compact}
                <button class="hbtn" onclick={() => begin(`axis:1:${y}`, axes[1]?.points[y]?.phys)}
                        title="Edit Y breakpoint {y}">{yHeads[y]}</button>
              {:else}
                {yHeads[y]}
              {/if}
            </th>

            {#each row.slice(0, shownC) as index}
              {@const pt = values[index]}
              <td
                class="cell"
                class:on={editing === `value:${index}`}
                style={shaded && extent ? `--t: ${shadeOf(pt?.phys, extent).toFixed(3)}` : ''}
              >
                {#if editing === `value:${index}`}
                  <!-- svelte-ignore a11y_autofocus -->
                  <input
                    class="cell-input"
                    class:invalid
                    bind:value={draft}
                    onkeydown={(e) => key(e, 'value', index)}
                    onblur={() => commit('value', index)}
                    aria-label="Value at index {index}"
                    spellcheck="false"
                    autofocus
                  />
                {:else if valuesEditable}
                  <button class="cbtn" onclick={() => begin(`value:${index}`, pt?.phys)}
                          title="Edit">{fmt(pt)}</button>
                {:else}
                  <span class="v">{fmt(pt)}</span>
                {/if}
              </td>
            {/each}
            {#if moreC > 0}<td class="more">…</td>{/if}
          </tr>
        {/each}
        {#if moreR > 0}
          <tr class="more-row">
            <th class="yh more">+{moreR}</th>
            {#each Array(shownC) as _}<td class="more">…</td>{/each}
            {#if moreC > 0}<td class="more"></td>{/if}
          </tr>
        {/if}
      </tbody>
    </table>
  </div>

  {#if !compact}
    <!-- Both headers are read-only for different reasons, and saying which is
         the difference between "you cannot" and "not here". -->
    <div class="legend">
      {#each [{ a: xAccess, n: 'X' }, { a: yAccess, n: 'Y' }] as e}
        {#if e.a.why}
          <span class="leg">
            <span class="leg-ax">{e.n}</span>
            {#if e.a.reference}
              stored in
              <button class="link" onclick={() => onNavigate(e.a.reference)}
                      title="Show {e.a.reference}">{e.a.reference}</button>
            {:else}
              {e.a.note}
            {/if}
          </span>
        {/if}
      {/each}
    </div>
  {/if}
{/if}

<style>
  .grid-wrap {
    overflow: auto;
    scrollbar-width: thin;
    border: 1px solid var(--c-border2);
    border-radius: 5px;
    background: var(--c-bg);
  }

  /* The preview shrinks to its table: a border box wider than the grid reads
     as missing columns rather than as a small map. */
  .grid-wrap.compact {
    overflow: hidden;
    width: max-content;
    max-width: 100%;
  }

  .grid {
    border-collapse: separate;
    border-spacing: 0;
    font-family: 'Cascadia Code', 'SF Mono', monospace;
    font-size: 11px;
    white-space: nowrap;
  }

  th, td {
    border-right: 1px solid var(--c-border2);
    border-bottom: 1px solid var(--c-border2);
    padding: 0;
    text-align: right;
  }

  th:last-child, td:last-child { border-right: none; }
  tr:last-child th, tr:last-child td { border-bottom: none; }

  /* Headers stay put so a large grid keeps its bearings while scrolling. */
  thead th {
    position: sticky;
    top: 0;
    z-index: 2;
    background: var(--c-surface);
    color: var(--c-addr);
    font-weight: 500;
  }

  .yh {
    position: sticky;
    left: 0;
    z-index: 1;
    background: var(--c-surface);
    color: var(--c-addr);
    font-weight: 500;
  }

  .corner {
    position: sticky;
    left: 0;
    z-index: 3;
    background: var(--c-surface);
  }

  .corner-dim {
    display: block;
    padding: 2px 6px;
    font-size: 9px;
    color: var(--c-dim);
  }

  /* A read-only header is dimmed; the legend below says which kind it is. */
  .xh.ro, .yh.ro { color: var(--c-muted); }

  .cell {
    position: relative;
    color: var(--c-text);
  }

  /* Value shading as its own layer, so the ramp never bleeds into the text
     and no colour function is needed to keep both themes readable. */
  .cell::before {
    content: '';
    position: absolute;
    inset: 0;
    background: var(--c-accent);
    opacity: var(--t, 0);
    pointer-events: none;
  }

  .cell.on::before { opacity: 0; }

  .v, .cbtn, .hbtn {
    position: relative;
    display: block;
    width: 100%;
    padding: 2px 6px;
    text-align: right;
    font: inherit;
    color: inherit;
    background: none;
    border: none;
  }

  .cbtn, .hbtn { cursor: text; }
  .cbtn:hover, .hbtn:hover { background: var(--c-hover); }

  .cell-input {
    display: block;
    width: 100%;
    min-width: 52px;
    box-sizing: border-box;
    padding: 1px 5px;
    font: inherit;
    text-align: right;
    color: var(--c-text);
    background: var(--c-bg);
    border: 1px solid var(--c-accent);
    border-radius: 3px;
    outline: none;
  }

  .cell-input.invalid { border-color: var(--c-err, #e05555); }

  .more {
    padding: 2px 6px;
    color: var(--c-dim);
    font-size: 10px;
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    font-size: 10px;
    color: var(--c-muted);
    padding: 4px 2px 0;
  }

  .leg-ax {
    color: var(--c-addr);
    font-family: 'Cascadia Code', 'SF Mono', monospace;
  }

  .link {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--c-accent);
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .link:hover { color: var(--c-accent-h, var(--c-accent)); }
</style>
