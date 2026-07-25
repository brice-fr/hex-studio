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
   *   onEditValue  – (phys: number) => void
   *   onEditText   – (text: string) => void
   *   onGoto       – (addr: number) => void   jump to the address in the hex view
   */
  let {
    row         = null,
    detail      = null,
    onEditValue = (_phys) => {},
    onEditText  = (_text) => {},
    onGoto      = (_addr) => {},
  } = $props();

  let draft = $state('');
  let error = $state('');

  // Re-seed the editor whenever a different parameter is selected, or the
  // displayed value changes underneath us after an edit or an undo.
  $effect(() => {
    const key = row ? `${row.name}:${row.display}` : '';
    void key;
    draft = row?.phys_num !== null && row?.phys_num !== undefined
      ? String(row.phys_num)
      : (row?.display ?? '');
    error = '';
  });

  const isEnum   = $derived(!!row?.enum_options);
  const outLimit = $derived.by(() => {
    if (!row || row.phys_num === null || row.phys_num === undefined) return false;
    return row.phys_num < row.lower_limit || row.phys_num > row.upper_limit;
  });

  function hex32(n) { return '0x' + n.toString(16).padStart(8, '0').toUpperCase(); }

  function commit() {
    if (!row?.editable) return;
    if (isEnum) { onEditText(draft); return; }
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
      draft = row?.phys_num != null ? String(row.phys_num) : (row?.display ?? '');
      error = '';
    }
  }

  /** Pair axis breakpoints with function values for the curve table. */
  const points = $derived.by(() => {
    if (!detail) return [];
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
        {#if error}
          <div class="err">{error}</div>
        {:else if !isEnum}
          <button class="apply" onclick={commit}>Apply</button>
        {/if}
      {:else}
        <div class="value-static" class:out={outLimit}>
          {row.display}{#if row.unit}<span class="unit"> {row.unit}</span>{/if}
        </div>
        {#if row.note}
          <div class="note">{row.note}</div>
        {/if}
      {/if}

      <!-- ── Facts ── -->
      <table class="facts">
        <tbody>
          <tr>
            <td>Address</td>
            <td class="v">
              <button class="link" onclick={() => onGoto(row.address)}
                      title="Show in hex view">{hex32(row.address)}</button>
            </td>
          </tr>
          <tr><td>Type</td><td class="v">{row.datatype}</td></tr>
          <tr><td>Size</td><td class="v">{row.byte_size} B</td></tr>
          {#if row.raw_hex}
            <tr><td>Raw</td><td class="v raw">{row.raw_hex}</td></tr>
          {/if}
          <tr>
            <td>Limits</td>
            <td class="v">{row.lower_limit} … {row.upper_limit}</td>
          </tr>
          <tr><td>Conversion</td><td class="v conv" title={row.conversion}>{row.conversion_type}</td></tr>
          {#if row.point_count !== null && row.point_count !== undefined}
            <tr><td>Points</td><td class="v">{row.point_count}</td></tr>
          {/if}
          <tr>
            <td>In image</td>
            <td class="v">
              <span class="pres {row.presence}">{row.presence}</span>
            </td>
          </tr>
        </tbody>
      </table>

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
        <div class="points-wrap">
          <table class="points">
            <thead>
              <tr><th class="i">#</th><th>Axis</th><th class="r">Value</th></tr>
            </thead>
            <tbody>
              {#each points as p}
                <tr>
                  <td class="i">{p.i}</td>
                  <td class="ax">{p.axis ? p.axis.display : '—'}</td>
                  <td class="r">{p.value ? p.value.display : '—'}</td>
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

  input:focus, select:focus {
    outline: none;
    border-color: var(--c-accent);
  }

  input.invalid { border-color: var(--c-err); }

  .unit { color: var(--c-muted); font-size: 11px; flex-shrink: 0; }

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
  }

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

  .pres { font-size: 10.5px; }
  .pres.full    { color: var(--c-diff-cmp-only); }
  .pres.partial { color: var(--c-diff-changed); }
  .pres.absent  { color: var(--c-dim); }

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

  .points .i  { color: var(--c-dim); width: 22px; }
  .points .ax { color: var(--c-addr); }
  .points .r  { text-align: right; color: var(--c-text); }
  .points th.r { text-align: right; }
</style>
