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
   */
  let {
    row         = null,
    detail      = null,
    decimals    = null,
    onEditValue = (_phys) => {},
    onEditText  = (_text) => {},
    onGoto      = (_addr) => {},
    onNavigate  = (_name) => {},
  } = $props();

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
        {#if isVirtual}
          <!-- Computed from other parameters, so there is no stored value to
               show — the formula and its inputs are the useful content. -->
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
        {:else}
          <div class="value-static" class:out={outLimit}>
            <span class="v">{atPrecision(row.phys_num, row.display)}</span>
            {#if row.unit}<span class="unit">{row.unit}</span>{/if}
          </div>
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
          {#if row.point_count !== null && row.point_count !== undefined}
            <tr><td>Points</td><td class="v">{row.point_count}</td></tr>
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
                  <td class="ax">{p.axis ? atPrecision(p.axis.phys, p.axis.display) : '—'}</td>
                  <td class="r">{p.value ? atPrecision(p.value.phys, p.value.display) : '—'}</td>
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
