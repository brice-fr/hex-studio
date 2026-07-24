// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

/**
 * hexHtmlExport.js — pure utility for generating self-contained HTML reports
 * from the main hex-viewer data.  No Tauri/Svelte dependencies.
 */

const isDataRec = t => t === 'Data' || t === 'S1' || t === 'S2' || t === 'S3';

/**
 * Build rows from records — identical logic to HexViewer's buildRows().
 * Returns an array of { type:'gap', gapBytes, fromAddr, toAddr }
 *                   or { type:'data', address, bytes:(number|null)[] }.
 * @param {Array}  records
 * @param {number} bpr     bytes per row
 */
export function buildHexRows(records, bpr) {
  const dataRecs = records
    .filter(r => isDataRec(r.record_type) && r.data.length > 0)
    .sort((a, b) => a.address - b.address);

  // Merge contiguous records into segments
  const segments = [];
  for (const rec of dataRecs) {
    if (!segments.length) {
      segments.push({ address: rec.address, data: [...rec.data] });
    } else {
      const last = segments[segments.length - 1];
      if (rec.address === last.address + last.data.length) {
        last.data.push(...rec.data);
      } else {
        segments.push({ address: rec.address, data: [...rec.data] });
      }
    }
  }

  const rows = [];
  for (let si = 0; si < segments.length; si++) {
    const seg = segments[si];

    // Gap row between segments
    if (si > 0) {
      const prev = segments[si - 1];
      rows.push({
        type:     'gap',
        gapBytes: seg.address - (prev.address + prev.data.length),
        fromAddr: prev.address + prev.data.length,
        toAddr:   seg.address,
      });
    }

    // Data rows — pad start to bpr boundary with null (leading blanks)
    const alignedStart   = Math.floor(seg.address / bpr) * bpr;
    const leadingBlanks  = seg.address - alignedStart;
    const padded         = [...Array(leadingBlanks).fill(null), ...seg.data];

    for (let i = 0; i < padded.length; i += bpr) {
      rows.push({
        type:    'data',
        address: alignedStart + i,
        bytes:   padded.slice(i, i + bpr),  // may be shorter than bpr on last row
      });
    }
  }

  return rows;
}

// ── Embedded CSS ─────────────────────────────────────────────────────────────
// Abbreviated variable names keep the inline <style> compact.
// Dark-mode defaults; light via prefers-color-scheme.
const HEX_REPORT_CSS = `:root{--cb:#1e1e1e;--cs:#252526;--ch:#3c3c3c;--cbr:#3a3a3a;--ct:#e0e0e0;--cm:#888;--cd:#555;--ca:#569cd6;--ce1:rgba(255,255,255,.03);--chb:#1e1e1e;--cnt:#3a3a3a;--casc:#9cdcfe;--cnp:#454545;--cgb:#1a1a1a;--cgt:#555}
@media(prefers-color-scheme:light){:root{--cb:#fff;--cs:#f8f8f8;--ch:#e8e8e8;--cbr:#e0e0e0;--ct:#1e1e1e;--cm:#666;--cd:#999;--ca:#0451a5;--ce1:rgba(0,0,0,.03);--chb:#f3f3f3;--cnt:#e8e8e8;--casc:#0070c1;--cnp:#d4d4d4;--cgb:#f0f0f0;--cgt:#aaa}}
*,*::before,*::after{box-sizing:border-box;margin:0;padding:0}
body{background:var(--cb);color:var(--ct);font-family:'Cascadia Code','SF Mono','Fira Code',Consolas,monospace;font-size:12px;-webkit-font-smoothing:antialiased}
.report{display:flex;flex-direction:column;min-height:100vh}
.file-header{display:flex;align-items:center;gap:10px;padding:0 12px;height:36px;background:var(--chb);border-bottom:1px solid var(--cbr);flex-shrink:0}
.file-name{font-size:13px;font-weight:600;color:var(--ct);font-family:'Inter',-apple-system,sans-serif}
.file-format{font-size:11px;color:var(--cm);background:var(--ch);border-radius:3px;padding:2px 6px;font-family:'Inter',-apple-system,sans-serif}
.file-path-row{padding:3px 12px;background:var(--cs);border-bottom:1px solid var(--cbr);font-size:10.5px;color:var(--cm);font-family:'Inter',-apple-system,sans-serif;word-break:break-all}
.col-header{display:flex;align-items:stretch;height:22px;background:var(--cs);border-bottom:2px solid var(--ch);flex-shrink:0}
.col-addr{display:flex;align-items:center;flex-shrink:0;width:calc(8ch + 22px);padding:0 10px 0 12px;color:var(--ca);font-size:11px;letter-spacing:.04em}
.v-sep{width:2px;flex-shrink:0;background:var(--ch);align-self:stretch}
.col-hex-area{display:flex;align-items:stretch;padding:0 8px;flex-shrink:0}
.col-ascii-area{display:flex;align-items:stretch;padding:0 8px}
.col-ascii-label{align-items:center;color:var(--cm);font-size:11px}
.hb{display:flex;align-items:center;justify-content:center;width:3ch;flex-shrink:0;color:var(--ct)}
.hb.ec-0{background:var(--ce1)}
.hb.blank{color:var(--cnt)}
.hb.pad{color:transparent}
.col-idx{color:var(--cd)}
.mid-gap{width:8px;flex-shrink:0}
.ac{display:flex;align-items:center;justify-content:center;width:1ch;padding:0 2px;flex-shrink:0;color:var(--casc)}
.ac.np{color:var(--cnp)}
.ac.blank{color:var(--cnt)}
.hex-row{display:flex;align-items:stretch;height:22px}
.hex-row.row-alt .hb.ec-0{background:transparent}
.hex-row.row-alt{background:var(--ce1)}
.gap-row{display:flex;align-items:center;padding:0 12px;gap:8px;height:30px;background:var(--cgb);font-family:'Inter',-apple-system,sans-serif}
.gap-line{flex:1;border-top:1px dashed var(--cbr)}
.gap-label{color:var(--cgt);font-size:11px;white-space:nowrap;flex-shrink:0}
.report-stats{padding:12px 16px;border-top:1px solid var(--cbr);background:var(--cs);font-family:'Inter',-apple-system,sans-serif;font-size:11.5px}
.stats-title{font-weight:600;color:var(--ct);margin-bottom:8px;font-size:12px}
.stat-row{display:flex;align-items:baseline;gap:12px;padding:2px 0}
.stat-label{color:var(--cm);width:140px;flex-shrink:0}
.stat-val{color:var(--ct);font-family:'Cascadia Code','SF Mono',monospace}
.legend{display:flex;align-items:center;padding:5px 14px;border-top:1px solid var(--cbr);background:var(--cs);font-family:'Inter',-apple-system,sans-serif;font-size:10.5px}
.leg-generated{color:var(--cd);font-style:italic}`;

// ── HTML generator ────────────────────────────────────────────────────────────

/**
 * Generate a self-contained HTML diff report.
 * @param {{ records: Array, bytesPerRow: number, currentFile: string,
 *           showAscii: boolean, showPath: boolean, currentFormat: string }} opts
 * @returns {string}  Complete HTML document.
 */
export function generateHexHtml({ records, bytesPerRow, currentFile, showAscii, showPath, currentFormat }) {
  const bpr  = bytesPerRow;
  const rows = buildHexRows(records, bpr);

  // ── Statistics ──────────────────────────────────────────────────────────────
  let totalBytes = 0, minAddr = Infinity, maxAddr = -Infinity;
  for (const rec of records) {
    if (!isDataRec(rec.record_type) || !rec.data.length) continue;
    totalBytes += rec.data.length;
    if (rec.address < minAddr) minAddr = rec.address;
    const end = rec.address + rec.data.length - 1;
    if (end > maxAddr) maxAddr = end;
  }
  const segCount = rows.filter(r => r.type === 'gap').length + (rows.length > 0 ? 1 : 0);

  // ── Helpers ─────────────────────────────────────────────────────────────────
  const h8      = n => n.toString(16).padStart(8, '0').toUpperCase();
  const h2      = n => n.toString(16).padStart(2, '0').toUpperCase();
  const esc     = s => String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
  const isPrint = b => b >= 32 && b <= 126;
  const fileName = currentFile ? currentFile.split('/').at(-1) : 'Unknown';

  // ── Column header ───────────────────────────────────────────────────────────
  const hdrBytes = Array.from({ length: bpr }, (_, i) =>
    (i > 0 && i % 8 === 0 ? '<span class="mid-gap"></span>' : '') +
    `<span class="hb ec-${i % 2} col-idx">${h2(i)}</span>`
  ).join('');

  // ── Per-row HTML ────────────────────────────────────────────────────────────
  const rowHtml = row => {
    if (row.type === 'gap') {
      const hx = row.gapBytes.toString(16).toUpperCase();
      return `<div class="gap-row">` +
        `<span class="gap-line"></span>` +
        `<span class="gap-label">gap: 0x${hx} bytes (${row.gapBytes.toLocaleString()}) \u00b7 ${h8(row.fromAddr)} \u2013 ${h8(row.toAddr - 1)}</span>` +
        `<span class="gap-line"></span></div>`;
    }

    const alt      = Math.floor(row.address / bpr) % 2 === 1;
    const hexCells = Array.from({ length: bpr }, (_, i) => {
      const b    = row.bytes[i];
      const half = (i > 0 && i % 8 === 0) ? '<span class="mid-gap"></span>' : '';
      if (b === undefined) return `${half}<span class="hb ec-${i%2} pad">\u00b7\u00b7</span>`;
      if (b === null)      return `${half}<span class="hb ec-${i%2} blank">\u00b7\u00b7</span>`;
      return `${half}<span class="hb ec-${i%2}">${h2(b)}</span>`;
    }).join('');

    const ascCells = showAscii ? Array.from({ length: bpr }, (_, i) => {
      const b = row.bytes[i];
      if (b === undefined || b === null) return `<span class="ac blank">\u00b7</span>`;
      if (isPrint(b)) return `<span class="ac">${esc(String.fromCharCode(b))}</span>`;
      return `<span class="ac np">\u00b7</span>`;
    }).join('') : '';

    return `<div class="hex-row${alt ? ' row-alt' : ''}">` +
      `<span class="col-addr">${h8(row.address)}</span>` +
      `<span class="v-sep"></span>` +
      `<span class="col-hex-area">${hexCells}</span>` +
      (showAscii
        ? `<span class="v-sep"></span><span class="col-ascii-area">${ascCells}</span>`
        : '') +
      `</div>`;
  };

  // ── Assemble document ───────────────────────────────────────────────────────
  const addrRange = minAddr === Infinity
    ? '\u2014'
    : `0x${h8(minAddr)} \u2013 0x${h8(maxAddr)}`;

  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>Hex Report \u2014 ${esc(fileName)}</title>
<style>${HEX_REPORT_CSS}</style>
</head>
<body><div class="report">
<div class="file-header">
  <span class="file-name">${esc(fileName)}</span>
  ${currentFormat ? `<span class="file-format">${esc(currentFormat.toUpperCase())}</span>` : ''}
</div>
${showPath && currentFile ? `<div class="file-path-row">${esc(currentFile)}</div>` : ''}
<div class="col-header">
  <span class="col-addr">Address</span>
  <span class="v-sep"></span>
  <span class="col-hex-area">${hdrBytes}</span>
  ${showAscii ? `<span class="v-sep"></span><span class="col-ascii-area col-ascii-label">ASCII</span>` : ''}
</div>
<div class="data-section">${rows.map(rowHtml).join('\n')}</div>
<div class="report-stats">
  <div class="stats-title">File statistics</div>
  <div class="stat-row"><span class="stat-label">Total bytes</span><span class="stat-val">${totalBytes.toLocaleString()}</span></div>
  <div class="stat-row"><span class="stat-label">Address range</span><span class="stat-val">${addrRange}</span></div>
  <div class="stat-row"><span class="stat-label">Segments</span><span class="stat-val">${segCount}</span></div>
</div>
<div class="legend">
  <span class="leg-generated">Hex export generated by Hex Studio</span>
</div>
</div></body></html>`;
}
