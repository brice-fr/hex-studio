<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  /** @type {{ record_type: string; address: number; data: number[] }[]} */
  let {
    records = [], bytesPerRow = 16, fontSize = 13,
    onScrolled = () => {}, onTopAddress = (_addr) => {},
    onByteClick = (_addr) => {}, onSelectionChange = (_sel) => {},
    gotoTarget = null,
    // ── Edit mode ────────────────────────────────────────────────────────────
    editable    = false,
    onEditByte  = (_addr, _byte) => {},
    onDelete    = (_lo, _hi) => {},
    onCut       = (_lo, _hi) => {},
    onFill      = (_lo, _hi) => {},
    onMove      = (_lo, _hi) => {},
    onPaste     = (_addr, _mode) => {},
    clipboardSize = 0,
  } = $props();

  const ROW_HEIGHT = $derived(fontSize + 7);
  const OVERSCAN   = 8;

  function buildRows(records, bytesPerRow) {
    // 1. Filter to data records with data.length > 0
    const dataRecs = records.filter(rec => {
      const isData = rec.record_type === 'Data'
                  || rec.record_type === 'S1'
                  || rec.record_type === 'S2'
                  || rec.record_type === 'S3';
      return isData && rec.data.length > 0;
    });

    // 2. Sort by address ascending
    dataRecs.sort((a, b) => a.address - b.address);

    // 3. Merge contiguous records (gap == 0) into segments
    /** @type {{ address: number; data: number[] }[]} */
    const segments = [];
    for (const rec of dataRecs) {
      if (segments.length === 0) {
        segments.push({ address: rec.address, data: [...rec.data] });
      } else {
        const last = segments[segments.length - 1];
        const gap = rec.address - (last.address + last.data.length);
        if (gap === 0) {
          // Contiguous — merge
          last.data.push(...rec.data);
        } else {
          segments.push({ address: rec.address, data: [...rec.data] });
        }
      }
    }

    // 4. Build rows from segments
    const rows = [];
    let counter = 0;

    for (let si = 0; si < segments.length; si++) {
      const seg = segments[si];

      // 4a. Insert gap row if not the first segment
      if (si > 0) {
        const prevSeg = segments[si - 1];
        const gap = seg.address - (prevSeg.address + prevSeg.data.length);
        rows.push({
          type: 'gap',
          id: `gap_${counter++}`,
          gapBytes: gap,
          fromAddr: prevSeg.address + prevSeg.data.length,
          toAddr: seg.address,
        });
      }

      // 4b. Align start to bytesPerRow boundary
      const alignedStart = Math.floor(seg.address / bytesPerRow) * bytesPerRow;
      const leadingBlanks = seg.address - alignedStart;
      const padded = [...Array(leadingBlanks).fill(null), ...seg.data];

      // 4c. Emit data rows
      for (let i = 0; i < padded.length; i += bytesPerRow) {
        rows.push({
          type: 'data',
          id: `data_${counter++}`,
          address: alignedStart + i,
          bytes: padded.slice(i, i + bytesPerRow),
        });
      }
    }

    return rows;
  }

  const rows = $derived(buildRows(records, bytesPerRow));

  const segmentCount = $derived(rows.filter(r => r.type === 'gap').length + (rows.length > 0 ? 1 : 0));
  const dataRowCount = $derived(rows.filter(r => r.type === 'data').length);
  const totalDataBytes = $derived(
    rows.filter(r => r.type === 'data')
        .reduce((sum, r) => sum + r.bytes.filter(b => b !== null).length, 0)
  );

  let scrollTop    = $state(0);
  let clientHeight = $state(600);

  const totalHeight = $derived(rows.length * ROW_HEIGHT);
  const startIdx    = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN));
  const endIdx      = $derived(Math.min(rows.length, Math.ceil((scrollTop + clientHeight) / ROW_HEIGHT) + OVERSCAN));
  const visibleRows = $derived(rows.slice(startIdx, endIdx));
  const offsetTop   = $derived(startIdx * ROW_HEIGHT);

  let scrollEl         = $state(null);
  let headerEl         = $state(null);
  let suppressScrolled = false;

  // Throttle state — plain vars, not reactive (no re-render needed)
  const SCROLL_THROTTLE_MS = 50;
  let rawScrollTop = 0;   // latest position from every scroll event
  let throttleId   = null;

  function commitScroll() {
    if (scrollTop !== rawScrollTop) scrollTop = rawScrollTop;
    throttleId = null;
  }

  function onScroll(e) {
    rawScrollTop = e.currentTarget.scrollTop;
    // Keep header horizontally in sync with content
    if (headerEl) headerEl.scrollLeft = e.currentTarget.scrollLeft;
    if (!suppressScrolled) onScrolled(); // user-initiated only

    if (throttleId === null) {
      // Leading edge: apply immediately, schedule a trailing catch-up
      scrollTop  = rawScrollTop;
      throttleId = setTimeout(commitScroll, SCROLL_THROTTLE_MS);
    }
    // Intermediate events: rawScrollTop advances; commitScroll will flush it
  }

  // Clean up any pending timer when the component is destroyed
  $effect(() => () => { if (throttleId !== null) clearTimeout(throttleId); });

  // Keep parent's top-address in sync (fires on mount, file load, scroll, and navigation)
  $effect(() => {
    const topIdx = Math.floor(scrollTop / ROW_HEIGHT);
    const topRow = rows.slice(topIdx).find(r => r.type === 'data');
    onTopAddress(topRow?.address ?? 0);
  });

  // Navigate to a target address when gotoTarget changes
  $effect(() => {
    if (!gotoTarget || !scrollEl) return;
    const rowIdx = rows.findIndex(
      r => r.type === 'data' && r.address <= gotoTarget.addr && gotoTarget.addr < r.address + bytesPerRow
    );
    if (rowIdx >= 0) {
      const newTop = rowIdx * ROW_HEIGHT;
      // Cancel any pending throttle so it can't overwrite the navigation jump
      if (throttleId !== null) { clearTimeout(throttleId); throttleId = null; }
      rawScrollTop     = newTop;
      suppressScrolled = true;
      scrollEl.scrollTop = newTop;
      scrollTop          = newTop;
      // Re-enable after the scroll event has fired
      requestAnimationFrame(() => { suppressScrolled = false; });
    }
  });

  function hex8(n)  { return n.toString(16).padStart(2, '0').toUpperCase(); }
  function hex32(n) { return n.toString(16).padStart(8, '0').toUpperCase(); }
  function toAscii(b) { return b !== null && b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : '.'; }
  function isPrint(b) { return b !== null && b >= 0x20 && b < 0x7f; }

  const COLS = $derived(Array.from({ length: bytesPerRow }, (_, i) => i));

  // ── Selection ────────────────────────────────────────────────────────────
  let selAnchor  = $state(/** @type {number|null} */ (null));
  let selFocus   = $state(/** @type {number|null} */ (null));
  let isDragging = false;

  const selMin   = $derived(selAnchor !== null && selFocus !== null ? Math.min(selAnchor, selFocus) : null);
  const selMax   = $derived(selAnchor !== null && selFocus !== null ? Math.max(selAnchor, selFocus) : null);
  const selCount = $derived(selMin !== null ? selMax - selMin + 1 : 0);

  // Notify parent whenever selection changes (focus = last byte cursor touched)
  $effect(() => {
    onSelectionChange(selMin !== null ? { start: selMin, end: selMax, count: selCount, focus: selFocus } : null);
  });

  // Clear selection when file changes
  $effect(() => { void records; selAnchor = null; selFocus = null; });

  function onScrollPointerDown(e) {
    if (e.button !== 0) return;
    const byteEl = document.elementFromPoint(e.clientX, e.clientY)?.closest('[data-addr]');
    if (!byteEl) { selAnchor = null; selFocus = null; return; }
    const addr = parseInt(/** @type {HTMLElement} */ (byteEl).dataset.addr ?? '');
    if (isNaN(addr)) return;
    // Toggle off: re-clicking the same single-byte selection clears it
    if (!e.shiftKey && selAnchor === addr && selFocus === addr) {
      selAnchor = null; selFocus = null; return;
    }
    if (e.shiftKey && selAnchor !== null) {
      selFocus = addr;
    } else {
      selAnchor = addr;
      selFocus  = addr;
    }
    isDragging = true;
    e.currentTarget.setPointerCapture(e.pointerId);
  }

  function onScrollPointerMove(e) {
    if (!isDragging) return;
    const byteEl = document.elementFromPoint(e.clientX, e.clientY)?.closest('[data-addr]');
    if (byteEl) {
      const addr = parseInt(/** @type {HTMLElement} */ (byteEl).dataset.addr ?? '');
      if (!isNaN(addr)) selFocus = addr;
    }
  }

  function onScrollPointerUp() { isDragging = false; }

  /** Clear selection — called from parent via Escape */
  export function clearSelection() { selAnchor = null; selFocus = null; }

  // ── Minimap ──────────────────────────────────────────────────────────────
  const MINIMAP_W     = 16;
  let minimapCanvas     = $state(/** @type {HTMLCanvasElement|null} */ (null));
  let minimapH          = $state(0);
  let minimapMode       = 'none'; // 'none' | 'drag' | 'page'
  let minimapDragOffset = 0;      // clientY offset within the band at drag start
  let minimapScrollTimer = null;
  let minimapScrollIntv  = null;

  function drawMinimap() {
    const canvas = minimapCanvas;
    if (!canvas || minimapH < 2) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    // Resize the canvas here (not via Svelte binding) so the size update and
    // the draw happen in the same synchronous call — avoids a blank frame
    // where Svelte's DOM update clears the canvas before $effect redraws it.
    if (canvas.height !== minimapH) canvas.height = minimapH;
    const W = MINIMAP_W, H = minimapH, total = rows.length;
    if (total === 0) { ctx.clearRect(0, 0, W, H); return; }

    // Per-pixel row fill
    const img = ctx.createImageData(W, H);
    const px  = img.data;
    for (let py = 0; py < H; py++) {
      const ri     = Math.min(total - 1, Math.floor(py / H * total));
      const isData = rows[ri].type === 'data';
      const r = isData ? 74  : 36;
      const g = isData ? 144 : 36;
      const b = isData ? 226 : 40;
      for (let x = 0; x < W; x++) {
        const i = (py * W + x) * 4;
        px[i] = r; px[i+1] = g; px[i+2] = b; px[i+3] = 255;
      }
    }
    ctx.putImageData(img, 0, 0);

    // Viewport band
    const visRows = Math.ceil(clientHeight / ROW_HEIGHT);
    const topRow  = Math.floor(scrollTop / ROW_HEIGHT);
    const bandTop = Math.round(topRow / total * H);
    const bandH   = Math.max(3, Math.round(visRows / total * H));
    ctx.fillStyle   = 'rgba(255,255,255,0.18)';
    ctx.fillRect(0, bandTop, W, bandH);
    ctx.strokeStyle = 'rgba(255,255,255,0.55)';
    ctx.lineWidth   = 1;
    ctx.strokeRect(0.5, bandTop + 0.5, W - 1, bandH - 1);
  }

  // Redraw whenever any dependency changes
  $effect(() => { void rows; void scrollTop; void clientHeight; void minimapH; void minimapCanvas; void ROW_HEIGHT; drawMinimap(); });

  /** Returns the current viewport band geometry in minimap pixels. */
  function minimapBand() {
    const total = rows.length;
    if (!total || !minimapH) return { top: 0, h: minimapH };
    const visRows = Math.ceil(clientHeight / ROW_HEIGHT);
    const topRow  = Math.floor(scrollTop / ROW_HEIGHT);
    const top     = Math.round(topRow / total * minimapH);
    const h       = Math.max(3, Math.round(visRows / total * minimapH));
    return { top, h };
  }

  function onMinimapDown(e) {
    e.currentTarget.setPointerCapture(e.pointerId);
    const rect = e.currentTarget.getBoundingClientRect();
    const y = e.clientY - rect.top;
    const { top, h } = minimapBand();
    if (y >= top && y <= top + h) {
      // Drag the band
      minimapMode = 'drag';
      minimapDragOffset = y - top;
    } else {
      // Page scroll above or below the band
      minimapMode = 'page';
      const dir = y < top ? -1 : 1;
      if (scrollEl) scrollEl.scrollTop += dir * clientHeight;
      minimapScrollTimer = setTimeout(() => {
        minimapScrollIntv = setInterval(() => {
          if (scrollEl) scrollEl.scrollTop += dir * clientHeight;
        }, 80);
      }, 350);
    }
  }

  function onMinimapMove(e) {
    if (minimapMode !== 'drag') return;
    const total = rows.length;
    if (!total || !minimapH) return;
    const { h } = minimapBand();
    const rect = e.currentTarget.getBoundingClientRect();
    const y = e.clientY - rect.top;
    const newTop = Math.max(0, Math.min(minimapH - h, y - minimapDragOffset));
    const range  = minimapH - h;
    const frac   = range > 0 ? newTop / range : 0;
    if (scrollEl) scrollEl.scrollTop = frac * (scrollEl.scrollHeight - scrollEl.clientHeight);
  }

  function onMinimapUp() {
    clearTimeout(minimapScrollTimer);
    clearInterval(minimapScrollIntv);
    minimapScrollTimer = null;
    minimapScrollIntv  = null;
    minimapMode = 'none';
  }

  // ── Scroll buttons ────────────────────────────────────────────────────────
  let scrollBtnInterval = null;

  function startScrollBtn(dir) {
    if (scrollEl) scrollEl.scrollTop += dir * ROW_HEIGHT * 3;
    scrollBtnInterval = setInterval(() => {
      if (scrollEl) scrollEl.scrollTop += dir * ROW_HEIGHT * 3;
    }, 80);
  }

  function stopScrollBtn() {
    if (scrollBtnInterval) { clearInterval(scrollBtnInterval); scrollBtnInterval = null; }
  }

  $effect(() => () => stopScrollBtn());

  // ── Copy context menu ─────────────────────────────────────────────────────
  let ctxMenu = $state(/** @type {{ x: number; y: number; previews: string[] } | null} */ (null));

  /** Extract the selected bytes from records into a Uint8Array. */
  function getSelectedBytes() {
    if (selMin === null || selMax === null) return new Uint8Array(0);
    const len = selMax - selMin + 1;
    const buf = new Uint8Array(len);
    for (const rec of records) {
      const isData = rec.record_type === 'Data' || rec.record_type === 'S1'
                  || rec.record_type === 'S2'   || rec.record_type === 'S3';
      if (!isData || !rec.data.length) continue;
      const rEnd = rec.address + rec.data.length - 1;
      if (rEnd < selMin || rec.address > selMax) continue;
      const from = Math.max(rec.address, selMin);
      const to   = Math.min(rEnd, selMax);
      for (let a = from; a <= to; a++) buf[a - selMin] = rec.data[a - rec.address];
    }
    return buf;
  }

  const ALL_COPY_FORMATS = [
    { label: 'Hex string (spaced)',  fn: (b) => Array.from(b).map(v => v.toString(16).padStart(2,'0').toUpperCase()).join(' ') },
    { label: 'Hex string',           fn: (b) => Array.from(b).map(v => v.toString(16).padStart(2,'0').toUpperCase()).join('') },
    { label: 'C array',              fn: (b) => '{ ' + Array.from(b).map(v => '0x' + v.toString(16).padStart(2,'0').toUpperCase()).join(', ') + ' }' },
    { label: 'Python bytes',         fn: (b) => "b'" + Array.from(b).map(v => '\\x' + v.toString(16).padStart(2,'0')).join('') + "'" },
    { label: 'Base64',               fn: (b) => { let s = ''; for (const v of b) s += String.fromCharCode(v); return btoa(s); } },
    { label: 'ASCII / UTF-8 string', fn: (b) => Array.from(b).map(v => v >= 0x20 && v < 0x7f ? String.fromCharCode(v) : '·').join('') },
    { label: 'UTF-16 LE string',     fn: (b) => new TextDecoder('utf-16le').decode(b), even: true },
    { label: 'UTF-16 BE string',     fn: (b) => new TextDecoder('utf-16be').decode(b), even: true },
  ];

  // Only include UTF-16 entries when the selection has an even byte count
  const activeCopyFormats = $derived(ALL_COPY_FORMATS.filter(f => !f.even || selCount % 2 === 0));

  function onContextMenu(e) {
    const hasSel = selMin !== null;
    const hasClip = editable && clipboardSize > 0;
    if (!hasSel && !hasClip) return;

    const byteEl = document.elementFromPoint(e.clientX, e.clientY)?.closest('[data-addr]');
    if (!byteEl) { ctxMenu = null; return; }
    const addr = parseInt(/** @type {HTMLElement} */ (byteEl).dataset.addr ?? '');
    if (isNaN(addr)) { ctxMenu = null; return; }

    const inSel = hasSel && addr >= selMin && addr <= selMax;
    if (!inSel && !hasClip) { ctxMenu = null; return; }

    e.preventDefault();

    const buf      = inSel ? getSelectedBytes() : new Uint8Array(0);
    const formats  = inSel ? activeCopyFormats : [];
    const previews = formats.map(f => { const s = f.fn(buf); return s.length > 32 ? s.slice(0, 29) + '…' : s; });

    // Estimate menu height for overflow avoidance
    const editRows = editable ? (inSel ? 3 : 0) + (hasClip ? 2 : 0) : 0;
    const copyRows = formats.length;
    const menuW = 310, menuH = (editRows + copyRows) * 36 + 80;
    const x = e.clientX + menuW > window.innerWidth  ? e.clientX - menuW : e.clientX;
    const y = e.clientY + menuH > window.innerHeight ? e.clientY - menuH : e.clientY;
    ctxMenu = { x, y, previews, ctxAddr: addr, inSel };
  }

  async function copyAs(fmt) {
    const buf = getSelectedBytes();
    const { invoke } = await import('@tauri-apps/api/core');
    try { await invoke('copy_plain_text', { text: fmt.fn(buf) }); } catch { /* clipboard unavailable */ }
    ctxMenu = null;
  }

  // ── Edit mode ─────────────────────────────────────────────────────────────
  let editAddr   = $state(/** @type {number|null} */ (null));
  let editNibble = $state(0);  // 0 = no nibble typed yet, 1 = high nibble typed
  let editValue  = $state(0);  // accumulating byte value while editing

  // Cancel edit when records change (file load) or editable turned off
  $effect(() => {
    void records;
    if (!editable) { editAddr = null; editNibble = 0; editValue = 0; }
  });

  function getByteFromRecords(addr) {
    for (const rec of records) {
      const isData = rec.record_type === 'Data' || rec.record_type === 'S1'
                  || rec.record_type === 'S2'   || rec.record_type === 'S3';
      if (!isData || !rec.data.length) continue;
      const off = addr - rec.address;
      if (off >= 0 && off < rec.data.length) return rec.data[off];
    }
    return null;
  }

  function findNextDataAddr(addr) {
    for (const row of rows) {
      if (row.type !== 'data') continue;
      for (let i = 0; i < row.bytes.length; i++) {
        if (row.bytes[i] !== null && row.address + i > addr) {
          return row.address + i;
        }
      }
    }
    return null;
  }

  function enterEdit(addr) {
    if (!editable) return;
    const cur = getByteFromRecords(addr);
    if (cur === null) return; // can't edit null/blank bytes
    editAddr   = addr;
    editNibble = 0;
    editValue  = cur;
    // Single-byte selection to show context
    selAnchor = addr; selFocus = addr;
  }

  function cancelEdit() {
    editAddr = null; editNibble = 0; editValue = 0;
  }

  function confirmEdit(advance = true) {
    if (editAddr === null) return;
    const addr = editAddr;
    const val  = editValue;
    onEditByte(addr, val);
    if (advance) {
      const next = findNextDataAddr(addr);
      if (next !== null) {
        const cur = getByteFromRecords(next);
        editAddr   = next;
        editNibble = 0;
        editValue  = cur ?? 0;
        selAnchor = next; selFocus = next;
      } else {
        cancelEdit();
      }
    } else {
      cancelEdit();
    }
  }

  function onHexKeydown(e) {
    if (editAddr === null) return false;
    if (e.key === 'Escape') { cancelEdit(); e.preventDefault(); return true; }
    if (e.key === 'Enter')  { confirmEdit(false); e.preventDefault(); return true; }
    if (e.key === 'Tab')    { confirmEdit(true);  e.preventDefault(); return true; }
    const hexVal = '0123456789abcdef'.indexOf(e.key.toLowerCase());
    if (hexVal !== -1) {
      e.preventDefault();
      if (editNibble === 0) {
        editValue  = hexVal << 4;
        editNibble = 1;
      } else {
        editValue  = (editValue & 0xF0) | hexVal;
        editNibble = 2;
        confirmEdit(true); // auto-advance after second nibble
      }
      return true;
    }
    return false;
  }
</script>

<div class="hex-viewer" style="font-size:{fontSize}px">
  {#if rows.length === 0}
    <p class="empty-state">No data to display. Open an Intel HEX or S-record file.</p>
  {:else}

    <!-- ── Scroll area + minimap ── -->
    <div class="scroll-and-minimap">

      <!-- ── Left column: header + scrollable content ── -->
      <div class="hex-left">

        <!-- ── Column header (scrolls horizontally in sync with content) ── -->
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div class="hex-header" bind:this={headerEl} onpointerdown={() => { selAnchor = null; selFocus = null; }}>
          <span class="col-addr">Address</span>
          <span class="v-sep"></span>
          <span class="col-hex-area">
            {#each COLS as i}
              {#if i > 0 && i % 8 === 0}<span class="mid-gap"></span>{/if}
              <span class="hb ec-{i % 2}">{i.toString(16).padStart(2, '0').toUpperCase()}</span>
            {/each}
          </span>
          <span class="v-sep"></span>
          <span class="col-ascii-area">
            {#each COLS as i}
              <span class="ac">{(i % 16).toString(16).toUpperCase()}</span>
            {/each}
          </span>
        </div>

        <!-- ── Virtual-scroll container ── -->
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div
          class="hex-scroll"
          role="grid"
          aria-rowcount={rows.length}
          onscroll={onScroll}
          onpointerdown={onScrollPointerDown}
          onpointermove={onScrollPointerMove}
          onpointerup={onScrollPointerUp}
          oncontextmenu={onContextMenu}
          bind:clientHeight
          bind:this={scrollEl}
        >
          <div class="scroll-space" style="height:{totalHeight}px; padding-top:{offsetTop}px;">
            {#each visibleRows as row (row.id)}
              {#if row.type === 'gap'}
                <div class="gap-row" style="height:{ROW_HEIGHT}px;">
                  <div class="gap-line"></div>
                  <span class="gap-label">gap: 0x{row.gapBytes.toString(16).toUpperCase()} bytes ({row.gapBytes}) · {hex32(row.fromAddr)} – {hex32(row.toAddr - 1)}</span>
                  <div class="gap-line"></div>
                </div>
              {:else}
                {@const pad = bytesPerRow - row.bytes.length}
                <div class="hex-row" role="row" style="height:{ROW_HEIGHT}px;">

                  <span class="col-addr">{hex32(row.address)}</span>
                  <span class="v-sep"></span>

                  <span class="col-hex-area">
                    {#each row.bytes as byte, i}
                      {#if i > 0 && i % 8 === 0}<span class="mid-gap"></span>{/if}
                      {#if byte === null}
                        <span class="hb ec-{i % 2} blank">__</span>
                      {:else}
                        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
                        <span
                          class="hb ec-{i % 2} clickable"
                          class:sel={selMin !== null && selMin <= row.address + i && row.address + i <= selMax}
                          class:editing={editAddr === row.address + i}
                          data-addr={row.address + i}
                          onclick={(e) => { e.stopPropagation(); onByteClick(row.address + i); }}
                          ondblclick={(e) => { e.stopPropagation(); enterEdit(row.address + i); }}
                        >{editAddr === row.address + i
                            ? (editNibble === 0 ? hex8(byte) : hex8(editValue))
                            : hex8(byte)}</span>
                      {/if}
                    {/each}
                    {#each { length: pad } as _, i}
                      {@const col = row.bytes.length + i}
                      {#if col > 0 && col % 8 === 0}<span class="mid-gap"></span>{/if}
                      <span class="hb ec-{col % 2} pad"></span>
                    {/each}
                  </span>

                  <span class="v-sep"></span>
                  <span class="col-ascii-area">
                    {#each row.bytes as byte, i}
                      {#if byte === null}
                        <span class="ac blank">·</span>
                      {:else}
                        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
                        <span
                          class="ac clickable"
                          class:sel={selMin !== null && selMin <= row.address + i && row.address + i <= selMax}
                          class:editing={editAddr === row.address + i}
                          class:np={!isPrint(byte) && editAddr !== row.address + i}
                          data-addr={row.address + i}
                          onclick={(e) => { e.stopPropagation(); onByteClick(row.address + i); }}
                          ondblclick={(e) => { e.stopPropagation(); enterEdit(row.address + i); }}
                        >{editAddr === row.address + i
                            ? (editNibble === 0 ? toAscii(byte) : toAscii(editValue))
                            : toAscii(byte)}</span>
                      {/if}
                    {/each}
                  </span>

                </div>
              {/if}
            {/each}
          </div>
        </div>

      </div><!-- /hex-left -->

      <!-- ── Minimap ── -->
      <div class="minimap-wrap">
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <button
          class="scroll-btn"
          onpointerdown={() => startScrollBtn(-1)}
          onpointerup={stopScrollBtn}
          onpointerleave={stopScrollBtn}
          onpointercancel={stopScrollBtn}
        >▲</button>

        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div
          class="minimap-canvas-area"
          bind:clientHeight={minimapH}
          onpointerdown={onMinimapDown}
          onpointermove={onMinimapMove}
          onpointerup={onMinimapUp}
          onpointercancel={onMinimapUp}
        >
          <canvas bind:this={minimapCanvas} width={MINIMAP_W}></canvas>
        </div>

        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <button
          class="scroll-btn"
          onpointerdown={() => startScrollBtn(1)}
          onpointerup={stopScrollBtn}
          onpointerleave={stopScrollBtn}
          onpointercancel={stopScrollBtn}
        >▼</button>
      </div>

    </div><!-- /scroll-and-minimap -->

    <div class="row-count">
      {dataRowCount.toLocaleString()} rows &nbsp;·&nbsp; {totalDataBytes.toLocaleString()} bytes
      {#if segmentCount > 1}&nbsp;·&nbsp; {segmentCount} segments{/if}
    </div>

  {/if}
</div>

<!-- ── Copy / Edit context menu (fixed, portal-like) ── -->
<svelte:window
  onclick={(e) => {
    if (ctxMenu && !/** @type {Element} */ (e.target)?.closest('.ctx-menu')) ctxMenu = null;
    // Click outside hex viewer cancels edit mode
    if (editAddr !== null && !/** @type {Element} */ (e.target)?.closest('.hex-viewer')) cancelEdit();
  }}
  onkeydown={(e) => {
    if (onHexKeydown(e)) return;
    if (e.key === 'Escape') { ctxMenu = null; cancelEdit(); }
  }}
/>

{#if ctxMenu}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="ctx-menu"
    style="left:{ctxMenu.x}px; top:{ctxMenu.y}px;"
    oncontextmenu={(e) => e.preventDefault()}
  >
    {#if editable && ctxMenu.inSel}
      <div class="ctx-header">Edit {selCount} byte{selCount === 1 ? '' : 's'}</div>
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="ctx-item" onclick={() => { onCut(selMin, selMax); ctxMenu = null; }}>
        <span class="ctx-label">Cut</span>
        <span class="ctx-preview">→ clipboard · deletes from address space</span>
      </div>
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="ctx-item ctx-danger" onclick={() => { onDelete(selMin, selMax); ctxMenu = null; }}>
        <span class="ctx-label">Delete</span>
        <span class="ctx-preview">remove from address space</span>
      </div>
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="ctx-item" onclick={() => { onFill(selMin, selMax); ctxMenu = null; }}>
        <span class="ctx-label">Fill…</span>
        <span class="ctx-preview">pattern or random bytes</span>
      </div>
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="ctx-item" onclick={() => { onMove(selMin, selMax); ctxMenu = null; }}>
        <span class="ctx-label">Move…</span>
        <span class="ctx-preview">cut and write to new address</span>
      </div>
    {/if}
    {#if editable && clipboardSize > 0}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="ctx-item" onclick={() => { onPaste(ctxMenu.ctxAddr, 'overwrite'); ctxMenu = null; }}>
        <span class="ctx-label">Paste {clipboardSize} byte{clipboardSize === 1 ? '' : 's'} (overwrite)</span>
        <span class="ctx-preview">at 0x{ctxMenu.ctxAddr.toString(16).toUpperCase().padStart(8,'0')}</span>
      </div>
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="ctx-item" onclick={() => { onPaste(ctxMenu.ctxAddr, 'fill-empty'); ctxMenu = null; }}>
        <span class="ctx-label">Paste {clipboardSize} byte{clipboardSize === 1 ? '' : 's'} (fill empty)</span>
        <span class="ctx-preview">at 0x{ctxMenu.ctxAddr.toString(16).toUpperCase().padStart(8,'0')}</span>
      </div>
    {/if}
    {#if editable && (ctxMenu.inSel || clipboardSize > 0) && ctxMenu.inSel}
      <div class="ctx-sep"></div>
    {/if}
    {#if ctxMenu.inSel}
      <div class="ctx-header">Copy {selCount} byte{selCount === 1 ? '' : 's'} as…</div>
      {#each activeCopyFormats as fmt, i}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="ctx-item" onclick={() => copyAs(fmt)}>
          <span class="ctx-label">{fmt.label}</span>
          <span class="ctx-preview">{ctxMenu.previews[i]}</span>
        </div>
      {/each}
    {/if}
  </div>
{/if}

<style>
  .hex-viewer {
    font-family: 'Cascadia Code', 'SF Mono', 'Fira Code', 'Courier New', Courier, monospace;
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--c-bg);
  }

  /* ── Scroll + minimap wrapper ── */
  .scroll-and-minimap {
    flex: 1;
    display: flex;
    overflow: hidden;
    min-height: 0;
  }

  /* ── Left column: header + scroll ── */
  .hex-left {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
  }

  /* ── Header (scrolls horizontally in sync via JS scrollLeft) ── */
  .hex-header {
    display: flex;
    align-items: stretch;
    height: 22px;
    background: var(--c-surface);
    border-bottom: 2px solid var(--c-hover);
    flex-shrink: 0;
    font-weight: 400;
    letter-spacing: 0;
    color: var(--c-dim);
    overflow: hidden; /* hide header scrollbar; scrollLeft synced via JS */
  }

  /* ── Scroll container ── */
  .hex-scroll {
    flex: 1;
    /* Push element 20px past hex-left's overflow:hidden boundary so the
       vertical scrollbar (if any) is clipped and invisible, while
       padding-right restores the content area to the same width as the
       header — fixing the header-sync-not-all-the-way bug. */
    margin-right: -20px;
    padding-right: 20px;
    box-sizing: border-box;
    overflow-y: scroll;
    overflow-x: auto;
    user-select: none;
    /* Firefox: show only horizontal bar — thumb on track matching theme */
    scrollbar-width: thin;
    scrollbar-color: var(--c-scrollbar-thumb) var(--c-scrollbar-track);
  }
  /* WebKit: style horizontal scrollbar only; vertical is clipped by parent */
  .hex-scroll::-webkit-scrollbar          { height: 8px; }
  .hex-scroll::-webkit-scrollbar-track    { background: var(--c-scrollbar-track); }
  .hex-scroll::-webkit-scrollbar-thumb    { background: var(--c-scrollbar-thumb); border-radius: 4px; }
  .hex-scroll::-webkit-scrollbar-thumb:hover { background: var(--c-scrollbar-thumb); opacity: 0.8; }

  /* ── Minimap ── */
  .minimap-wrap {
    width: 16px;
    flex-shrink: 0;
    background: var(--c-surface);
    border-left: 1px solid var(--c-border);
    display: flex;
    flex-direction: column;
  }

  .minimap-canvas-area {
    flex: 1;
    overflow: hidden;
    cursor: ns-resize;
    user-select: none;
  }

  .minimap-canvas-area canvas {
    display: block;
  }

  .scroll-btn {
    width: 100%;
    height: 16px;
    flex-shrink: 0;
    background: var(--c-surface);
    border: none;
    border-color: var(--c-border);
    color: var(--c-dim);
    font-size: 7px;
    line-height: 1;
    padding: 0;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    user-select: none;
  }

  .scroll-btn:first-child {
    border-bottom: 1px solid var(--c-border);
  }

  .scroll-btn:last-child {
    border-top: 1px solid var(--c-border);
  }

  .scroll-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .scroll-btn:active {
    background: var(--c-accent-b);
    color: #fff;
  }

  .scroll-space {
    box-sizing: border-box;
    min-width: max-content; /* expand to row width so horizontal scroll triggers */
    padding-right: 8px;     /* breathing room so last ASCII char is fully reachable */
  }

  /* ── Row ── */
  .hex-row {
    display: flex;
    align-items: stretch;
  }

  .hex-row:nth-child(odd) {
    background: var(--c-ec1);
  }

  /* On hover: flatten per-cell backgrounds, apply uniform highlight */
  .hex-row:hover {
    background: var(--c-hover);
  }

  .hex-row:hover .hb.ec-0,
  .hex-row:hover .hb.ec-1 {
    background: transparent;
  }

  /* ── Address column ── */
  .col-addr {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    width: calc(8ch + 22px);
    padding: 0 10px 0 12px;
    color: var(--c-addr);
  }

  .v-sep {
    width: 2px;
    flex-shrink: 0;
    background: var(--c-hover);
    align-self: stretch;
  }

  /* ── Hex bytes area ── */
  .col-hex-area {
    display: flex;
    align-items: stretch;
    padding: 0 8px;
    flex-shrink: 0;
  }

  /* Individual hex byte cell — 3ch = "XX " */
  .hb {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 3ch;
    flex-shrink: 0;
    color: var(--c-text);
  }

  /* Alternating column shading */
  .hb.ec-0 {
    background: var(--c-ec1);
  }

  .hb.pad {
    color: transparent;
  }

  .hb.clickable,
  .ac.clickable {
    cursor: pointer;
  }

  .hb.clickable:hover {
    background: var(--c-hover) !important;
    border-radius: 2px;
  }

  .ac.clickable:hover {
    background: var(--c-hover);
    border-radius: 2px;
  }

  /* Mid-row gap between byte 7 and byte 8 */
  .mid-gap {
    width: 8px;
    flex-shrink: 0;
  }

  /* ── ASCII area ── */
  .col-ascii-area {
    display: flex;
    align-items: stretch;
    padding: 0 8px;
  }

  /* Individual ASCII char cell */
  .ac {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 1ch;
    padding: 0 2px;
    flex-shrink: 0;
    color: var(--c-accent-t);
  }

  /* Non-printable dot */
  .ac.np {
    color: var(--c-border2);
  }

  /* ── Selection ── */
  .hb.sel,
  .ac.sel {
    background: var(--c-sel) !important;
    color: var(--c-sel-text) !important;
    border-radius: 2px;
  }

  /* ── Edit mode cursor ── */
  .hb.editing,
  .ac.editing {
    outline: 2px solid var(--c-accent-b) !important;
    outline-offset: -1px;
    background: rgba(74, 144, 226, 0.22) !important;
    color: #fff !important;
    border-radius: 2px;
    position: relative;
    z-index: 1;
  }

  /* Gap separator row */
  .gap-row {
    display: flex;
    align-items: center;
    padding: 0 12px;
    gap: 8px;
    flex-shrink: 0;
    background: var(--c-gap-bg);
  }
  .gap-line {
    flex: 1;
    border-top: 1px dashed var(--c-border);
  }
  .gap-label {
    color: var(--c-gap-text);
    font-size: 11px;
    white-space: nowrap;
    flex-shrink: 0;
    user-select: none;
  }
  /* Blank/gap byte cells (leading nulls at segment start) */
  .hb.blank {
    color: var(--c-null-text);
  }
  .ac.blank {
    color: var(--c-null-text);
  }

  /* ── Row-count footer ── */
  .row-count {
    padding: 3px 12px;
    font-size: 11px;
    color: var(--c-dim);
    border-top: 1px solid var(--c-border);
    flex-shrink: 0;
    user-select: none;
  }

  /* ── Empty state ── */
  .empty-state {
    color: var(--c-dim);
    text-align: center;
    margin-top: 4rem;
  }

  /* ── Copy context menu ── */
  :global(.ctx-menu) {
    position: fixed;
    z-index: 9999;
    min-width: 310px;
    background: var(--c-surface, #252526);
    border: 1px solid var(--c-border, #3c3c3c);
    border-radius: 6px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.45);
    padding: 4px 0;
    font-family: system-ui, -apple-system, sans-serif;
    font-size: 12px;
    user-select: none;
  }

  :global(.ctx-header) {
    padding: 6px 12px 5px;
    font-size: 11px;
    color: var(--c-muted, #858585);
    border-bottom: 1px solid var(--c-border, #3c3c3c);
    margin-bottom: 3px;
  }

  :global(.ctx-item) {
    display: flex;
    flex-direction: column;
    padding: 5px 12px;
    cursor: pointer;
    gap: 1px;
  }

  :global(.ctx-item:hover) {
    background: var(--c-hover, rgba(255,255,255,0.07));
  }

  :global(.ctx-label) {
    color: var(--c-text, #d4d4d4);
    font-size: 12px;
  }

  :global(.ctx-preview) {
    color: var(--c-muted, #858585);
    font-family: 'Cascadia Code', 'SF Mono', 'Fira Code', monospace;
    font-size: 10px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  :global(.ctx-sep) {
    height: 1px;
    background: var(--c-border, #3c3c3c);
    margin: 4px 0;
  }

  :global(.ctx-danger .ctx-label) {
    color: var(--c-err, #e05555);
  }

  :global(.ctx-danger:hover) {
    background: rgba(220, 50, 50, 0.12) !important;
  }

  @media (prefers-color-scheme: light) {
    :global(.ctx-menu)   { background: #fff; border-color: #ddd; box-shadow: 0 8px 24px rgba(0,0,0,0.15); }
    :global(.ctx-header) { color: #888; border-color: #e8e8e8; }
    :global(.ctx-label)  { color: #1e1e1e; }
    :global(.ctx-preview){ color: #888; }
    :global(.ctx-item:hover) { background: #f0f0f0; }
    :global(.ctx-sep)    { background: #e8e8e8; }
    :global(.ctx-danger:hover) { background: rgba(200, 30, 30, 0.08) !important; }
  }
</style>
