<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  /**
   * DiffViewer — side-by-side hex diff of two firmware files.
   *
   * Layout per data row:
   *   [ref hex bytes]   ADDRESS   [cmp hex bytes]
   *
   * Identical aligned sections are collapsed into a single gap marker row.
   *   • Double-click a gap marker  → expand it (show individual identical rows)
   *   • Double-click a same row    → collapse it back to a gap marker
   *
   * Cell status colours:
   *   same      → default text
   *   changed   → --c-diff-changed  (yellow)
   *   ref-only  → --c-diff-ref-only (blue)  — cmp side shows "··"
   *   cmp-only  → --c-diff-cmp-only (green) — ref side shows "··"
   *   empty     → dim "··" on both sides
   */

  import { writeTextFile } from '$lib/api.js';

  let {
    refPath     = '',
    cmpPath     = '',
    refRecords  = [],
    cmpRecords  = [],
    bytesPerRow = 16,
  } = $props();

  const ITEM_HEIGHT = 22;  // px — data rows
  const GAP_HEIGHT  = 30;  // px — gap-marker rows
  const BUFFER      = 8;   // extra rows rendered above/below viewport

  // ── Step 1: build raw rows (no collapsing) ──────────────────────────────────

  function buildRawRows(refRecs, cmpRecs, bpr) {
    const isData = (t) => t === 'Data' || t === 'S1' || t === 'S2' || t === 'S3';

    // Find address bounds with a fast loop (avoid spread/Math.min stack overflow)
    let minAddr = Infinity, maxAddr = -Infinity;
    for (const rec of refRecs) {
      if (!isData(rec.record_type) || rec.data.length === 0) continue;
      if (rec.address < minAddr) minAddr = rec.address;
      const end = rec.address + rec.data.length - 1;
      if (end > maxAddr) maxAddr = end;
    }
    for (const rec of cmpRecs) {
      if (!isData(rec.record_type) || rec.data.length === 0) continue;
      if (rec.address < minAddr) minAddr = rec.address;
      const end = rec.address + rec.data.length - 1;
      if (end > maxAddr) maxAddr = end;
    }
    if (minAddr === Infinity) return [];

    const span = maxAddr - minAddr + 1;

    // For compact address spaces use typed arrays: Uint8Array population is a
    // native memcpy (set/fill), and indexed access is ~5× faster than Map.get.
    // Fall back to Map for very sparse / huge address ranges (>8 MB).
    const USE_TYPED = span <= 8 * 1024 * 1024;

    let refHas, cmpHas, refBuf, cmpBuf;  // typed-array path
    let refMap, cmpMap;                   // Map fallback path

    if (USE_TYPED) {
      refHas = new Uint8Array(span);
      cmpHas = new Uint8Array(span);
      refBuf = new Uint8Array(span);
      cmpBuf = new Uint8Array(span);
      for (const rec of refRecs) {
        if (!isData(rec.record_type)) continue;
        const off = rec.address - minAddr;
        refBuf.set(rec.data, off);
        refHas.fill(1, off, off + rec.data.length);
      }
      for (const rec of cmpRecs) {
        if (!isData(rec.record_type)) continue;
        const off = rec.address - minAddr;
        cmpBuf.set(rec.data, off);
        cmpHas.fill(1, off, off + rec.data.length);
      }
    } else {
      refMap = new Map();
      cmpMap = new Map();
      for (const rec of refRecs) {
        if (!isData(rec.record_type)) continue;
        for (let i = 0; i < rec.data.length; i++) refMap.set(rec.address + i, rec.data[i]);
      }
      for (const rec of cmpRecs) {
        if (!isData(rec.record_type)) continue;
        for (let i = 0; i < rec.data.length; i++) cmpMap.set(rec.address + i, rec.data[i]);
      }
    }

    const rowStart = Math.floor(minAddr / bpr) * bpr;
    const rowEnd   = Math.ceil((maxAddr + 1) / bpr) * bpr;

    const rows = [];
    let emptyRunStart = null;

    for (let addr = rowStart; addr < rowEnd; addr += bpr) {
      // ── hasData check ──────────────────────────────────────────────────────
      let hasData = false;
      if (USE_TYPED) {
        for (let i = 0; i < bpr; i++) {
          const off = addr + i - minAddr;
          if (off >= 0 && off < span && (refHas[off] || cmpHas[off])) { hasData = true; break; }
        }
      } else {
        for (let i = 0; i < bpr; i++) {
          if (refMap.has(addr + i) || cmpMap.has(addr + i)) { hasData = true; break; }
        }
      }

      if (!hasData) {
        if (emptyRunStart === null) emptyRunStart = addr;
        continue;
      }
      if (emptyRunStart !== null) {
        rows.push({ type: 'addr-gap', addr: emptyRunStart, toAddr: addr });
        emptyRunStart = null;
      }

      // ── build cells ────────────────────────────────────────────────────────
      const cells = [];
      let allSame = true, nonEmptyCount = 0;

      for (let i = 0; i < bpr; i++) {
        const a = addr + i;
        let rb, cb;
        if (USE_TYPED) {
          const off = a - minAddr;
          const inRange = off >= 0 && off < span;
          rb = (inRange && refHas[off]) ? refBuf[off] : undefined;
          cb = (inRange && cmpHas[off]) ? cmpBuf[off] : undefined;
        } else {
          rb = refMap.get(a);
          cb = cmpMap.get(a);
        }

        let status;
        if      (rb === undefined && cb === undefined)              status = 'empty';
        else if (rb !== undefined && cb !== undefined && rb === cb) status = 'same';
        else if (rb !== undefined && cb !== undefined)              status = 'changed';
        else if (rb !== undefined)                                  status = 'ref-only';
        else                                                        status = 'cmp-only';

        if (status !== 'empty')                    nonEmptyCount++;
        if (status !== 'same' && status !== 'empty') allSame = false;
        cells.push({ rb, cb, status });
      }

      rows.push({ type: 'row', addr, cells, allSame, nonEmptyCount });
    }
    return rows;
  }

  // ── Step 2: build display items — collapse same runs or expand them ─────────

  function buildDisplayItems(rawRows, expanded, bpr) {
    const result = [];
    let gapStart = null, gapBytes = 0, gapRows = [], gapLastAddr = 0;

    for (const row of rawRows) {
      if (row.type === 'addr-gap') {
        if (gapStart !== null) {
          if (expanded.includes(gapStart)) {
            for (const r of gapRows) result.push({ ...r, gapAddr: gapStart });
          } else {
            result.push({ type: 'gap', addr: gapStart, lastAddr: gapLastAddr + bpr - 1, count: gapBytes });
          }
          gapStart = null; gapBytes = 0; gapRows = []; gapLastAddr = 0;
        }
        result.push(row);
        continue;
      }
      if (row.allSame) {
        if (gapStart === null) gapStart = row.addr;
        gapBytes += row.nonEmptyCount;
        gapLastAddr = row.addr;
        gapRows.push(row);
      } else {
        if (gapStart !== null) {
          if (expanded.includes(gapStart)) {
            for (const r of gapRows) result.push({ ...r, gapAddr: gapStart });
          } else {
            result.push({ type: 'gap', addr: gapStart, lastAddr: gapLastAddr + bpr - 1, count: gapBytes });
          }
          gapStart = null; gapBytes = 0; gapRows = []; gapLastAddr = 0;
        }
        result.push(row);
      }
    }
    if (gapStart !== null) {
      if (expanded.includes(gapStart)) {
        for (const r of gapRows) result.push({ ...r, gapAddr: gapStart });
      } else {
        result.push({ type: 'gap', addr: gapStart, lastAddr: gapLastAddr + bpr - 1, count: gapBytes });
      }
    }
    return result;
  }

  // ── Expand/collapse state ───────────────────────────────────────────────────

  let expandedGaps = $state(/** @type {number[]} */ ([]));

  function expandGap(addr) {
    if (!expandedGaps.includes(addr)) expandedGaps.push(addr);
  }
  function collapseGap(addr) {
    const idx = expandedGaps.indexOf(addr);
    if (idx !== -1) expandedGaps.splice(idx, 1);
  }

  // ── Derived state ───────────────────────────────────────────────────────────

  // rawRows is computed off the main thread tick so the loading spinner in the
  // parent can finish rendering before the (potentially heavy) computation runs.
  let rawRows    = $state(/** @type {any[]} */ ([]));
  let computing  = $state(false);

  $effect(() => {
    const ref = refRecords;
    const cmp = cmpRecords;
    const bpr = bytesPerRow;
    computing = true;
    // Double-rAF: first rAF fires before the browser paints, schedules the
    // second, then the browser paints the "Processing…" spinner, then the
    // second rAF fires and the heavy computation runs.
    let raf1, raf2;
    raf1 = requestAnimationFrame(() => {
      raf2 = requestAnimationFrame(() => {
        rawRows   = buildRawRows(ref, cmp, bpr);
        computing = false;
      });
    });
    return () => { cancelAnimationFrame(raf1); cancelAnimationFrame(raf2); };
  });
  const changedRows = $derived(rawRows.filter(r => r.type === 'row' && !r.allSame).length);

  // Per-gap row counts — used for stats and expand/collapse all
  const gapInfo = $derived.by(() => {
    const gaps = [];
    let gapStart = null, gapCount = 0;
    for (const row of rawRows) {
      if (row.type === 'addr-gap') {
        if (gapStart !== null) { gaps.push({ addr: gapStart, count: gapCount }); gapStart = null; gapCount = 0; }
        continue;
      }
      if (row.allSame) {
        if (gapStart === null) gapStart = row.addr;
        gapCount++;
      } else {
        if (gapStart !== null) { gaps.push({ addr: gapStart, count: gapCount }); gapStart = null; gapCount = 0; }
      }
    }
    if (gapStart !== null) gaps.push({ addr: gapStart, count: gapCount });
    return gaps;
  });

  const collapsedRows = $derived(
    gapInfo.filter(g => !expandedGaps.includes(g.addr)).reduce((s, g) => s + g.count, 0)
  );
  const shownSameRows = $derived(
    gapInfo.filter(g =>  expandedGaps.includes(g.addr)).reduce((s, g) => s + g.count, 0)
  );

  function expandAll() {
    for (const g of gapInfo) {
      if (!expandedGaps.includes(g.addr)) expandedGaps.push(g.addr);
    }
  }
  function collapseAll() {
    expandedGaps.length = 0;
  }

  // ── HTML export ─────────────────────────────────────────────────────────────

  let showExportDialog = $state(false);
  let exportShowPaths  = $state(false);
  let exportSections   = $state(/** @type {'collapsed'|'expanded'|'as-shown'} */ ('as-shown'));
  let exportError      = $state('');

  // CSS embedded in the exported HTML report (dark default, light via media query)
  const REPORT_CSS = `:root{--cb:#1e1e1e;--cs:#252526;--ch:#3c3c3c;--cbr:#3a3a3a;--ct:#e0e0e0;--cm:#888;--cd:#555;--ca:#569cd6;--cgb:#1a1a1a;--cgt:#555;--ce1:rgba(255,255,255,.03);--chb:#1e1e1e;--cr:#6bc5f8;--crb:rgba(107,197,248,.12);--cg:#6cde8d;--cgr:rgba(108,222,141,.12);--cdf:#ffcb3d}
@media(prefers-color-scheme:light){:root{--cb:#fff;--cs:#f8f8f8;--ch:#e8e8e8;--cbr:#e0e0e0;--ct:#1e1e1e;--cm:#666;--cd:#999;--ca:#0451a5;--cgb:#f0f0f0;--cgt:#aaa;--ce1:rgba(0,0,0,.03);--chb:#f3f3f3;--cr:#0451a5;--crb:rgba(4,81,165,.10);--cg:#057a43;--cgr:rgba(5,122,67,.10);--cdf:#9a6200}}
*,*::before,*::after{box-sizing:border-box;margin:0;padding:0}
body{background:var(--cb);color:var(--ct);font-family:'Cascadia Code','SF Mono','Fira Code',Consolas,monospace;font-size:12px;-webkit-font-smoothing:antialiased}
.report{display:flex;flex-direction:column;min-height:100vh}
.diff-header{display:flex;align-items:center;background:var(--chb);border-bottom:1px solid var(--cbr);padding:0 12px;height:36px;flex-shrink:0}
.side-label{flex:1;font-size:11.5px;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;padding:0 4px}
.ref-label{color:var(--cr);text-align:right}.cmp-label{color:var(--cg);text-align:left}
.addr-col-label-hdr{flex-shrink:0;width:calc(90px + 4px)}
.file-paths{display:flex;align-items:baseline;padding:3px 12px;background:var(--cs);border-bottom:1px solid var(--cbr);font-family:'Inter',-apple-system,sans-serif;font-size:10px;gap:0}
.fp-ref{flex:1;color:var(--cr);text-align:right;padding-right:4px;word-break:break-all}
.fp-cmp{flex:1;color:var(--cg);text-align:left;padding-left:4px;word-break:break-all}
.col-header{display:flex;align-items:stretch;justify-content:center;padding:0 12px;background:var(--cs);border-bottom:2px solid var(--ch);height:22px;color:var(--cd)}
.col-idx{color:var(--cd)}.col-addr-label{color:var(--ca)}
.v-sep{width:2px;flex-shrink:0;background:var(--ch);align-self:stretch}
.addr-col{flex-shrink:0;width:90px;display:flex;align-items:center;justify-content:center;color:var(--ca);font-size:11px;letter-spacing:.04em}
.data-row{display:flex;align-items:stretch;justify-content:center;padding:0 12px}
.data-row.row-alt{background:var(--ce1)}
.hex-side{flex-shrink:0;display:flex;align-items:center}
.ref-side{padding-right:8px}.cmp-side{padding-left:8px}
.half-gap{width:8px;flex-shrink:0}
.byte-cell{display:flex;align-items:center;justify-content:center;width:3ch;flex-shrink:0;border-radius:2px;color:var(--ct)}
.byte-cell.ec-0{background:var(--ce1)}
.ref-side .byte-cell.changed,.ref-side .byte-cell.ref-only,.ref-side .byte-cell.cmp-only{color:var(--cr);background:var(--crb)}
.cmp-side .byte-cell.changed,.cmp-side .byte-cell.cmp-only,.cmp-side .byte-cell.ref-only{color:var(--cg);background:var(--cgr)}
.byte-cell.empty{color:var(--cd)}
.addr-gap-row{display:flex;align-items:center;padding:0 14px;gap:10px;height:30px;background:var(--cgb);font-family:'Inter',-apple-system,sans-serif}
.addr-gap-text{color:var(--cgt);font-size:11px}
.gap-row{display:flex;align-items:center;padding:0 14px;gap:10px;height:30px;font-family:'Inter',-apple-system,sans-serif}
.gap-line{flex:1;height:1px;background:var(--cbr)}
.gap-text{flex-shrink:0;font-size:11px;color:var(--cd);letter-spacing:.03em;white-space:nowrap}
.report-stats{padding:12px 16px;border-top:1px solid var(--cbr);background:var(--cs);font-family:'Inter',-apple-system,sans-serif;font-size:11.5px}
.stats-title{font-weight:600;color:var(--ct);margin-bottom:8px;font-size:12px}
.stat-row{display:flex;align-items:baseline;gap:12px;padding:2px 0}
.stat-label{color:var(--cm);width:140px;flex-shrink:0}
.stat-val{color:var(--ct);font-family:'Cascadia Code','SF Mono',monospace}
.stat-val.diff{color:var(--cdf)}
.legend{display:flex;align-items:center;gap:16px;padding:5px 14px;border-top:1px solid var(--cbr);background:var(--cs);font-size:10.5px;color:var(--cm);font-family:'Inter',-apple-system,sans-serif}
.leg-item{display:flex;align-items:center;gap:5px}
.leg-swatch{display:inline-block;width:10px;height:10px;border-radius:2px}
.ref-swatch{background:var(--cr)}.cmp-swatch{background:var(--cg)}
.leg-generated{margin-left:auto;color:var(--cd);font-style:italic}`;

  function generateHtml({ showPaths, sections }) {
    const bpr = bytesPerRow;
    const esc = s => String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
    const h8  = n => n.toString(16).padStart(8,'0').toUpperCase();
    const h2  = n => n.toString(16).padStart(2,'0').toUpperCase();

    // Effective expansion state
    let effExp;
    if      (sections === 'expanded')  effExp = gapInfo.map(g => g.addr);
    else if (sections === 'collapsed') effExp = [];
    else                               effExp = [...expandedGaps];

    const items = buildDisplayItems(rawRows, effExp, bpr);

    // Statistics — count over all raw rows (not just display items)
    let identicalBytes = 0, differentBytes = 0;
    for (const row of rawRows) {
      if (row.type !== 'row') continue;
      for (const cell of row.cells) {
        if      (cell.status === 'same')  identicalBytes++;
        else if (cell.status !== 'empty') differentBytes++;
      }
    }
    const comparedBytes = identicalBytes + differentBytes;
    const diffRowCount  = rawRows.filter(r => r.type === 'row' && !r.allSame).length;

    // Column header cells (identical for both sides)
    const hdrCells = Array.from({length: bpr}, (_, i) =>
      (i === bpr/2 ? '<span class="half-gap"></span>' : '') +
      `<span class="byte-cell ec-${i%2} col-idx">${h2(i)}</span>`
    ).join('');

    // Per-item HTML
    const itemHtml = item => {
      if (item.type === 'addr-gap') {
        const size = item.toAddr - item.addr;
        return `<div class="addr-gap-row"><span class="addr-gap-text">gap: 0x${size.toString(16).toUpperCase()} bytes (${size.toLocaleString()}) \u00b7 ${h8(item.addr)} \u2013 ${h8(item.toAddr-1)}</span></div>`;
      }
      if (item.type === 'gap') {
        return `<div class="gap-row"><span class="gap-line"></span><span class="gap-text">${h8(item.addr)} \u2013 ${h8(item.lastAddr)} \u00b7 ${item.count.toLocaleString()} identical byte${item.count === 1 ? '' : 's'}</span><span class="gap-line"></span></div>`;
      }
      const alt  = Math.floor(item.addr / bpr) % 2 === 1;
      const side = (getVal) => item.cells.map((cell, ci) =>
        (ci === bpr/2 ? '<span class="half-gap"></span>' : '') +
        `<span class="byte-cell ${cell.status} ec-${ci%2}">${getVal(cell)}</span>`
      ).join('');
      return `<div class="data-row${alt ? ' row-alt' : ''}">` +
        `<div class="hex-side ref-side">${side(c => c.rb !== undefined ? h2(c.rb) : '\u00b7\u00b7')}</div>` +
        `<span class="v-sep"></span>` +
        `<div class="addr-col">${h8(item.addr)}</div>` +
        `<span class="v-sep"></span>` +
        `<div class="hex-side cmp-side">${side(c => c.cb !== undefined ? h2(c.cb) : '\u00b7\u00b7')}</div>` +
        `</div>`;
    };

    const refName = lastName(refPath);
    const cmpName = lastName(cmpPath);

    return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>Diff Report \u2014 ${esc(refName)} \u2194 ${esc(cmpName)}</title>
<style>${REPORT_CSS}</style>
</head>
<body><div class="report">
<div class="diff-header">
  <div class="side-label ref-label">${esc(refName)}</div>
  <div class="addr-col-label-hdr"></div>
  <div class="side-label cmp-label">${esc(cmpName)}</div>
</div>
${showPaths ? `<div class="file-paths">
  <div class="fp-ref">${esc(refPath)}</div>
  <div class="addr-col-label-hdr"></div>
  <div class="fp-cmp">${esc(cmpPath)}</div>
</div>` : ''}
<div class="col-header">
  <div class="hex-side ref-side">${hdrCells}</div>
  <span class="v-sep"></span>
  <div class="addr-col col-addr-label">Address</div>
  <span class="v-sep"></span>
  <div class="hex-side cmp-side">${hdrCells}</div>
</div>
<div class="data-section">${items.map(itemHtml).join('\n')}</div>
<div class="report-stats">
  <div class="stats-title">Difference statistics</div>
  <div class="stat-row"><span class="stat-label">Compared bytes</span><span class="stat-val">${comparedBytes.toLocaleString()}</span></div>
  <div class="stat-row"><span class="stat-label">Identical bytes</span><span class="stat-val">${identicalBytes.toLocaleString()}</span></div>
  <div class="stat-row"><span class="stat-label">Different bytes</span><span class="stat-val diff">${differentBytes.toLocaleString()}</span></div>
</div>
<div class="legend">
  <span class="leg-item"><span class="leg-swatch ref-swatch"></span>Reference file</span>
  <span class="leg-item"><span class="leg-swatch cmp-swatch"></span>Compare file</span>
  <span class="leg-generated">Difference report generated by Hex Studio</span>
</div>
</div></body></html>`;
  }

  async function doExport() {
    exportError = '';
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const base    = lastName(refPath).replace(/\.[^.]+$/, '');
      const base2   = lastName(cmpPath).replace(/\.[^.]+$/, '');
      const path = await save({
        defaultPath: `diff-${base}-${base2}.html`,
        filters: [{ name: 'HTML Report', extensions: ['html'] }],
      });
      if (!path) return; // user cancelled
      const html = generateHtml({ showPaths: exportShowPaths, sections: exportSections });
      await writeTextFile(path, html);
      showExportDialog = false;
    } catch (e) {
      exportError = String(e);
    }
  }

  const items = $derived.by(() => buildDisplayItems(rawRows, expandedGaps, bytesPerRow));

  // Per-item heights → prefix-sum array for virtual scroll
  const offsets = $derived.by(() => {
    const arr = new Float64Array(items.length + 1);
    for (let i = 0; i < items.length; i++) {
      arr[i + 1] = arr[i] + (items[i].type === 'gap' || items[i].type === 'addr-gap' ? GAP_HEIGHT : ITEM_HEIGHT);
    }
    return arr;
  });
  const totalHeight = $derived(offsets[items.length]);

  function firstVisible(top) {
    let lo = 0, hi = items.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (offsets[mid + 1] <= top) lo = mid + 1; else hi = mid;
    }
    return lo;
  }

  let scrollTop  = $state(0);
  let containerH = $state(600);

  const COL_HEADER_H = 22;
  const visStart = $derived(Math.max(0, firstVisible(scrollTop) - BUFFER));
  const visEnd   = $derived(Math.min(items.length, firstVisible(scrollTop + containerH - COL_HEADER_H) + BUFFER));
  const visItems = $derived(items.slice(visStart, visEnd));
  const padTop   = $derived(offsets[visStart]);
  const padBot   = $derived(totalHeight - offsets[visEnd]);

  // ── Helpers ─────────────────────────────────────────────────────────────────

  function hex8(n) { return n.toString(16).padStart(8, '0').toUpperCase(); }
  function hex2(n) { return n.toString(16).padStart(2, '0').toUpperCase(); }
  function lastName(path) { return path ? path.split(/[\\/]/).at(-1) : '—'; }

  function itemKey(item) {
    if (item.type === 'gap')      return `gap-${item.addr}`;
    if (item.type === 'addr-gap') return `addr-gap-${item.addr}`;
    if (item.gapAddr !== undefined) return `same-${item.addr}`;
    return `row-${item.addr}`;
  }
</script>

<div class="diff-shell">

  <!-- File names header -->
  <div class="diff-header">
    <div class="side-label ref-label" title={refPath}>{lastName(refPath)}</div>
    <div class="addr-col-label"></div>
    <div class="side-label cmp-label" title={cmpPath}>{lastName(cmpPath)}</div>
  </div>

  {#if computing}
    <div class="computing-screen">
      <svg class="spin" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"
           stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M21 12a9 9 0 1 1-6.219-8.56"/>
      </svg>
      <span>Processing…</span>
    </div>
  {:else}

  <!-- Virtual scroll container — col-header is the first child so it shares
       the same content width as data rows (scrollbar-aware), then sticks to top. -->
  <div
    class="diff-scroll"
    bind:clientHeight={containerH}
    onscroll={(e) => { scrollTop = e.currentTarget.scrollTop; }}
  >

    <!-- Column index header (sticky) -->
    <div class="col-header">
      <div class="hex-side ref-side">
        {#each { length: bytesPerRow } as _, i}
          {#if i === bytesPerRow / 2}<span class="half-gap"></span>{/if}
          <span class="byte-cell ec-{i % 2} col-idx">{(i).toString(16).padStart(2, '0').toUpperCase()}</span>
        {/each}
      </div>
      <span class="v-sep"></span>
      <div class="addr-col col-addr-label">Address</div>
      <span class="v-sep"></span>
      <div class="hex-side cmp-side">
        {#each { length: bytesPerRow } as _, i}
          {#if i === bytesPerRow / 2}<span class="half-gap"></span>{/if}
          <span class="byte-cell ec-{i % 2} col-idx">{(i).toString(16).padStart(2, '0').toUpperCase()}</span>
        {/each}
      </div>
    </div>

    <div class="scroll-space" style="height:{totalHeight}px; position:relative;">
      <div style="position:absolute; top:{padTop}px; left:0; right:0;">

        {#each visItems as item (itemKey(item))}

          {#if item.type === 'addr-gap'}
            <div class="addr-gap-row" style="height:{GAP_HEIGHT}px">
              <span class="gap-line"></span>
              <span class="gap-text addr-gap-text">
                gap: 0x{(item.toAddr - item.addr).toString(16).toUpperCase()} bytes ({(item.toAddr - item.addr).toLocaleString()}) · {hex8(item.addr)} – {hex8(item.toAddr - 1)}
              </span>
              <span class="gap-line"></span>
            </div>

          {:else if item.type === 'gap'}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="gap-row"
              style="height:{GAP_HEIGHT}px"
              title="Double-click to expand"
              ondblclick={() => expandGap(item.addr)}
            >
              <span class="gap-line"></span>
              <span class="gap-text">
                {hex8(item.addr)} – {hex8(item.lastAddr)}
                &nbsp;·&nbsp;
                {item.count.toLocaleString()} identical byte{item.count === 1 ? '' : 's'}
              </span>
              <span class="gap-line"></span>
            </div>

          {:else if item.gapAddr !== undefined}
            <!-- Expanded same row — double-click collapses back -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="data-row same-row"
              class:row-alt={Math.floor(item.addr / bytesPerRow) % 2 === 1}
              style="height:{ITEM_HEIGHT}px"
              title="Double-click to collapse"
              ondblclick={() => collapseGap(item.gapAddr)}
            >
              <div class="hex-side ref-side">
                {#each item.cells as cell, ci}
                  {#if ci === bytesPerRow / 2}<span class="half-gap"></span>{/if}
                  <span class="byte-cell {cell.status} ec-{ci % 2}">{cell.rb !== undefined ? hex2(cell.rb) : '··'}</span>
                {/each}
              </div>
              <span class="v-sep"></span>
              <div class="addr-col">{hex8(item.addr)}</div>
              <span class="v-sep"></span>
              <div class="hex-side cmp-side">
                {#each item.cells as cell, ci}
                  {#if ci === bytesPerRow / 2}<span class="half-gap"></span>{/if}
                  <span class="byte-cell {cell.status} ec-{ci % 2}">{cell.cb !== undefined ? hex2(cell.cb) : '··'}</span>
                {/each}
              </div>
            </div>

          {:else}
            <!-- Diff row -->
            <div
              class="data-row"
              class:row-alt={Math.floor(item.addr / bytesPerRow) % 2 === 1}
              style="height:{ITEM_HEIGHT}px"
            >
              <div class="hex-side ref-side">
                {#each item.cells as cell, ci}
                  {#if ci === bytesPerRow / 2}<span class="half-gap"></span>{/if}
                  <span class="byte-cell {cell.status} ec-{ci % 2}">{cell.rb !== undefined ? hex2(cell.rb) : '··'}</span>
                {/each}
              </div>
              <span class="v-sep"></span>
              <div class="addr-col">{hex8(item.addr)}</div>
              <span class="v-sep"></span>
              <div class="hex-side cmp-side">
                {#each item.cells as cell, ci}
                  {#if ci === bytesPerRow / 2}<span class="half-gap"></span>{/if}
                  <span class="byte-cell {cell.status} ec-{ci % 2}">{cell.cb !== undefined ? hex2(cell.cb) : '··'}</span>
                {/each}
              </div>
            </div>
          {/if}

        {/each}

      </div>
    </div>
  </div>

  <!-- Stats bar -->
  <div class="stats-bar">
    {#if rawRows.length === 0 || changedRows === 0}
      <span>Files are identical — no differences found.</span>
    {:else}
      <span class="stat changed">{changedRows} difference row{changedRows === 1 ? '' : 's'}</span>
      {#if collapsedRows > 0}
        <span class="stat-sep">·</span>
        <span class="stat same">{collapsedRows.toLocaleString()} identical row{collapsedRows === 1 ? '' : 's'} collapsed</span>
      {/if}
      {#if shownSameRows > 0}
        <span class="stat-sep">·</span>
        <span class="stat same">{shownSameRows.toLocaleString()} identical row{shownSameRows === 1 ? '' : 's'} shown</span>
      {/if}
    {/if}
    <span class="stat-actions">
      {#if collapsedRows > 0}
        <!-- svelte-ignore a11y_invalid_attribute -->
        <a href="#" class="stat-link" onclick={(e) => { e.preventDefault(); expandAll(); }}>expand all</a>
      {/if}
      {#if collapsedRows > 0 && shownSameRows > 0}
        <span class="stat-sep">–</span>
      {/if}
      {#if shownSameRows > 0}
        <!-- svelte-ignore a11y_invalid_attribute -->
        <a href="#" class="stat-link" onclick={(e) => { e.preventDefault(); collapseAll(); }}>collapse all</a>
      {/if}
    </span>
  </div>

  <!-- Legend -->
  <div class="legend">
    <span class="leg-item"><span class="leg-swatch ref-only"></span>Reference file</span>
    <span class="leg-item"><span class="leg-swatch cmp-only"></span>Compare file</span>
    <!-- svelte-ignore a11y_invalid_attribute -->
    <a href="#" class="export-link" onclick={(e) => { e.preventDefault(); exportError = ''; showExportDialog = true; }}>Export as HTML…</a>
  </div>

  <!-- Export dialog -->
  {#if showExportDialog}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="exp-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label="Export as HTML"
      onclick={(e) => { if (e.target === e.currentTarget) showExportDialog = false; }}
      onkeydown={(e) => { if (e.key === 'Escape') showExportDialog = false; if (e.key === 'Enter') doExport(); }}
    >
      <div class="exp-card">
        <h2 class="exp-title">Export as HTML</h2>
        <p class="exp-subtitle">Configure the report output</p>

        <label class="exp-checkbox-row">
          <input type="checkbox" bind:checked={exportShowPaths}>
          Show full file paths in report
        </label>

        <div class="exp-field-label">Identical sections</div>
        <div class="exp-radio-group">
          <label class="exp-radio"><input type="radio" bind:group={exportSections} value="as-shown"> As on screen</label>
          <label class="exp-radio"><input type="radio" bind:group={exportSections} value="collapsed"> All collapsed</label>
          <label class="exp-radio"><input type="radio" bind:group={exportSections} value="expanded"> All expanded</label>
        </div>

        {#if exportError}
          <p class="exp-error">{exportError}</p>
        {/if}

        <div class="exp-actions">
          <button class="btn-cancel" onclick={() => showExportDialog = false}>Cancel</button>
          <button class="btn-ok" onclick={doExport}>Export…</button>
        </div>
      </div>
    </div>
  {/if}

  {/if}<!-- end computing -->

</div>

<style>
  /* ── Processing spinner ────────────────────────────────────── */
  .computing-screen {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    color: var(--c-muted);
    font-size: 13px;
  }
  .spin {
    width: 28px;
    height: 28px;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  /* ── Shell ─────────────────────────────────────────────────── */
  .diff-shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--c-bg);
    color: var(--c-text);
    overflow: hidden;
    font-family: 'Cascadia Code', 'SF Mono', 'Fira Code', 'Consolas', monospace;
    font-size: 12px;
  }

  /* ── Header ────────────────────────────────────────────────── */
  .diff-header {
    display: flex;
    align-items: center;
    background: var(--c-header-bg);
    border-bottom: 1px solid var(--c-border);
    padding: 0 12px;
    height: 36px;
    flex-shrink: 0;
  }

  .side-label {
    flex: 1;
    font-size: 11.5px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    padding: 0 4px;
  }

  .ref-label { color: var(--c-diff-ref-only); text-align: right; }
  .cmp-label { color: var(--c-diff-cmp-only); text-align: left;  }

  /* spacer that holds the width of the address column in the file-names header */
  .addr-col-label {
    flex-shrink: 0;
    width: calc(90px + 4px); /* addr-col + 2 × v-sep */
  }

  /* ── Column index header ───────────────────────────────────── */
  .col-header {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: stretch;
    justify-content: center;
    padding: 0 12px;
    background: var(--c-surface);
    border-bottom: 2px solid var(--c-hover);
    height: 22px;
    color: var(--c-dim);
  }

  .col-idx   { color: var(--c-dim); }
  .col-addr-label { color: var(--c-addr); }

  /* ── Stats bar ─────────────────────────────────────────────── */
  .stats-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 14px;
    background: var(--c-surface);
    border-top: 1px solid var(--c-border);
    font-size: 11px;
    flex-shrink: 0;
    color: var(--c-muted);
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
  }

  .stat.changed { color: var(--c-diff-changed); }
  .stat.same    { color: var(--c-muted); }
  .stat-sep     { color: var(--c-dim); }

  .stat-actions { margin-left: auto; display: flex; align-items: center; gap: 6px; }

  .stat-link {
    color: var(--c-accent-t);
    text-decoration: none;
    cursor: pointer;
  }
  .stat-link:hover { text-decoration: underline; }

  /* ── Scroll area ───────────────────────────────────────────── */
  .diff-scroll {
    flex: 1;
    overflow-y: scroll;
    overflow-x: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--c-scrollbar-thumb) var(--c-scrollbar-track);
  }

  .diff-scroll::-webkit-scrollbar       { width: 8px; height: 8px; }
  .diff-scroll::-webkit-scrollbar-track { background: var(--c-scrollbar-track); }
  .diff-scroll::-webkit-scrollbar-thumb { background: var(--c-scrollbar-thumb); border-radius: 4px; }

  /* ── Address gap row (no data in either file) ─────────────── */
  .addr-gap-row {
    display: flex;
    align-items: center;
    padding: 0 14px;
    gap: 10px;
    background: var(--c-gap-bg);
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
    user-select: none;
  }

  .addr-gap-text {
    color: var(--c-gap-text) !important;
  }

  /* ── Gap row ───────────────────────────────────────────────── */
  .gap-row {
    display: flex;
    align-items: center;
    padding: 0 14px;
    gap: 10px;
    cursor: pointer;
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
    user-select: none;
  }

  .gap-row:hover .gap-text { color: var(--c-text2); }
  .gap-row:hover .gap-line { background: var(--c-border2); }

  .gap-line {
    flex: 1;
    height: 1px;
    background: var(--c-border);
    transition: background 0.1s;
  }

  .gap-text {
    flex-shrink: 0;
    font-size: 11px;
    color: var(--c-dim);
    letter-spacing: 0.03em;
    white-space: nowrap;
    transition: color 0.1s;
  }

  /* ── Vertical separators (between hex sides and address column) */
  .v-sep {
    width: 2px;
    flex-shrink: 0;
    background: var(--c-hover);
    align-self: stretch;
  }

  /* ── Data row ──────────────────────────────────────────────── */
  .data-row {
    display: flex;
    align-items: stretch;
    justify-content: center;
    padding: 0 12px;
  }

  .data-row.row-alt { background: var(--c-ec1); }

  .data-row:hover { background: var(--c-hover); }

  /* Flatten per-cell column tint on hover */
  .data-row:hover .byte-cell.ec-0 { background: transparent; }

  /* Expanded same rows — pointer cursor */
  .same-row { cursor: pointer; }

  /* ── Hex sides ─────────────────────────────────────────────── */
  /* flex-shrink:0 gives each side a fixed intrinsic width (content-driven),
     exactly like HexViewer's col-hex-area.  Combined with justify-content:center
     on the row, every byte column lands at the same x-position in both the
     sticky col-header and every data row, regardless of scroll-container width. */
  .hex-side {
    flex-shrink: 0;
    display: flex;
    align-items: center;
  }

  .ref-side { padding-right: 8px; }
  .cmp-side { padding-left:  8px; }

  .half-gap {
    width: 8px;
    flex-shrink: 0;
  }

  /* ── Byte cells ────────────────────────────────────────────── */
  .byte-cell {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 3ch;
    flex-shrink: 0;
    border-radius: 2px;
    color: var(--c-text);
  }

  /* Alternating column tint — only visible on same/empty cells (diff rules override) */
  .byte-cell.ec-0 { background: var(--c-ec1); }

  /* ref side: always blue for any differing position */
  .ref-side .byte-cell.changed,
  .ref-side .byte-cell.ref-only,
  .ref-side .byte-cell.cmp-only { color: var(--c-diff-ref-only); background: var(--c-diff-ref-only-bg); }

  /* cmp side: always green for any differing position */
  .cmp-side .byte-cell.changed,
  .cmp-side .byte-cell.cmp-only,
  .cmp-side .byte-cell.ref-only { color: var(--c-diff-cmp-only); background: var(--c-diff-cmp-only-bg); }

  .byte-cell.empty { color: var(--c-dim); }

  /* ── Address column ────────────────────────────────────────── */
  .addr-col {
    flex-shrink: 0;
    width: 90px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--c-addr);
    font-size: 11px;
    letter-spacing: 0.04em;
  }

  /* ── Legend ────────────────────────────────────────────────── */
  .legend {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 5px 14px;
    border-top: 1px solid var(--c-border);
    background: var(--c-surface);
    font-size: 10.5px;
    color: var(--c-muted);
    flex-shrink: 0;
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
  }

  .leg-item {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .leg-swatch {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 2px;
  }

  .leg-swatch.ref-only { background: var(--c-diff-ref-only); }
  .leg-swatch.cmp-only { background: var(--c-diff-cmp-only); }

  .export-link {
    margin-left: auto;
    color: var(--c-accent-t);
    font-size: 10.5px;
    text-decoration: none;
    cursor: pointer;
    white-space: nowrap;
  }
  .export-link:hover { text-decoration: underline; }

  /* ── Export dialog ─────────────────────────────────────────── */
  .exp-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    backdrop-filter: blur(2px);
  }

  .exp-card {
    background: var(--c-raised);
    border: 1px solid var(--c-border2);
    border-radius: 10px;
    padding: 24px 28px 20px;
    width: 320px;
    box-shadow: 0 20px 60px rgba(0,0,0,0.6);
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
  }

  .exp-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0;
  }

  .exp-subtitle {
    font-size: 12px;
    color: var(--c-muted);
    margin: 0 0 8px;
  }

  .exp-checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--c-text);
    cursor: pointer;
    padding: 4px 0;
  }

  .exp-field-label {
    font-size: 11px;
    color: var(--c-muted);
    letter-spacing: 0.02em;
    margin-top: 8px;
    text-transform: uppercase;
  }

  .exp-radio-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 4px 0 8px;
  }

  .exp-radio {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--c-text);
    cursor: pointer;
    padding: 2px 0;
  }

  .exp-error {
    font-size: 11px;
    color: var(--c-err);
  }

  .exp-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }

  .btn-cancel, .btn-ok {
    padding: 5px 16px;
    font-size: 13px;
    border-radius: 5px;
    cursor: pointer;
    font-family: inherit;
    transition: background 0.1s, color 0.1s;
  }

  .btn-cancel {
    background: transparent;
    border: 1px solid var(--c-dim);
    color: var(--c-muted);
  }
  .btn-cancel:hover { background: var(--c-hover); color: var(--c-text); }

  .btn-ok {
    background: var(--c-accent-b);
    border: 1px solid transparent;
    color: #fff;
  }
  .btn-ok:hover { background: var(--c-accent-h); }
</style>
