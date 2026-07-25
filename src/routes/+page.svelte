<!-- SPDX-License-Identifier: MIT -->
<!-- SPDX-FileCopyrightText: 2026 Brice LECOLE -->

<script>
  import { onMount, onDestroy } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { PhysicalSize } from '@tauri-apps/api/dpi';
  import { Menu, Submenu, MenuItem, CheckMenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu';
  import { open, save, message } from '@tauri-apps/plugin-dialog';
  import { openFile, parseIntelHex, parseSrec, detectFileFormat, saveFile, saveBinary, getStartupFile,
           a2lLoad, a2lUnload, a2lList, a2lDetail, a2lStats, a2lEncodeValue, a2lEncodeText } from '$lib/api.js';
  import { listen } from '@tauri-apps/api/event';
  import {
    cloneRecords, deleteRange, writeBytes, writeBytesEmpty,
    getBytesRange, buildFill, randomBytes as genRandomBytes,
    computeChecksum, numberToBytes, normalize, findChecksumDefaults,
  } from '$lib/editOps.js';
  import FileMenu from '$lib/components/FileMenu.svelte';
  import HexViewer from '$lib/components/HexViewer.svelte';
  import SegmentList   from '$lib/components/SegmentList.svelte';
  import DataInspector from '$lib/components/DataInspector.svelte';
  import SaveFormatDialog from '$lib/components/SaveFormatDialog.svelte';
  import HexExportDialog from '$lib/components/HexExportDialog.svelte';
  import GoToDialog  from '$lib/components/GoToDialog.svelte';
  import FindDialog  from '$lib/components/FindDialog.svelte';
  import AboutDialog from '$lib/components/AboutDialog.svelte';
  import ImportBinaryDialog from '$lib/components/ImportBinaryDialog.svelte';
  import PreferencesDialog from '$lib/components/PreferencesDialog.svelte';
  import FileAssocDialog from '$lib/components/FileAssocDialog.svelte';
  import CompareDialog  from '$lib/components/CompareDialog.svelte';
  import FillDialog     from '$lib/components/FillDialog.svelte';
  import MoveDialog     from '$lib/components/MoveDialog.svelte';
  import ChecksumDialog from '$lib/components/ChecksumDialog.svelte';
  import ImportMergeDialog from '$lib/components/ImportMergeDialog.svelte';
  import SelectRangeDialog from '$lib/components/SelectRangeDialog.svelte';
  import DataView from '$lib/components/DataView.svelte';

  // ── Persistent settings — read synchronously before first render ──────────
  const LS = 'hex-studio.';
  const lsGet = (key, fallback) => { const v = localStorage.getItem(LS + key); return v !== null ? v : fallback; };
  const lsSet = (key, val)      => localStorage.setItem(LS + key, String(val));

  let records       = $state([]);
  let currentFile   = $state('');
  let currentFormat = $state('');
  let status        = $state('');
  let loading       = $state(false);
  let saving           = $state(false);
  let showFormatPicker  = $state(false);
  let showExportHtml    = $state(false);
  let showGoto         = $state(false);
  let showFind         = $state(false);
  let showAbout        = $state(false);
  let isDragging       = $state(false);
  let showImportBinary = $state(false);
  let showCompare      = $state(false);
  let compareFile      = $state('');
  let pendingBinaryPath = $state('');
  let hexTopAddress    = $state(0);        // tracks topmost visible address in HexViewer
  let gotoTarget       = $state(null);     // { addr, seq } — seq ensures reactivity on repeat
  let gotoSeq          = 0;

  // ── Undo / Redo ───────────────────────────────────────────────────────────
  /** @type {{ label: string; records: Array }[]} */
  let undoStack = $state([]);
  /** @type {{ label: string; records: Array }[]} */
  let redoStack = $state([]);
  const MAX_UNDO = 50;

  function pushUndo(label) {
    undoStack = [...undoStack.slice(-(MAX_UNDO - 1)), { label, records: cloneRecords(records) }];
    redoStack = [];
  }

  function undo() {
    if (undoStack.length === 0) return;
    const top = undoStack[undoStack.length - 1];
    redoStack = [...redoStack, { label: top.label, records: cloneRecords(records) }];
    records   = top.records;
    undoStack = undoStack.slice(0, -1);
    status = `Undo: ${top.label}`;
  }

  function redo() {
    if (redoStack.length === 0) return;
    const top = redoStack[redoStack.length - 1];
    undoStack = [...undoStack, { label: top.label, records: cloneRecords(records) }];
    records   = top.records;
    redoStack = redoStack.slice(0, -1);
    status = `Redo: ${top.label}`;
  }

  // Reset undo/redo when a new file is loaded
  function resetUndoHistory() { undoStack = []; redoStack = []; }

  // Modified state — true when undoStack is non-empty (records changed since last load/save)
  const isModified = $derived(undoStack.length > 0);

  $effect(() => {
    if (!currentFile) return;
    const fileName = currentFile.split(/[\\/]/).at(-1);
    getCurrentWindow().setTitle(`Hex Studio — ${fileName}${isModified ? ' ●' : ''}`);
  });

  // ── Binary clipboard (internal, not system clipboard) ────────────────────
  /**
   * @type {{ addr: number; bytes: Uint8Array } | null}
   */
  let binaryClipboard = $state(null);

  // ── Edit dialog visibility ────────────────────────────────────────────────
  let showFill         = $state(false);
  let fillSelMin       = $state(0);
  let fillSelMax       = $state(0);

  let showMove         = $state(false);
  let moveSelMin       = $state(0);
  let moveSelMax       = $state(0);

  let showChecksum      = $state(false);
  let checksumPrefMin   = $state(0);
  let checksumPrefMax   = $state(0);
  let checksumPrefTarget = $state(/** @type {number|null} */ (null));

  let showImportMerge  = $state(false);

  let showSelectRange       = $state(false);
  let selectRangePrefStart  = $state(0);
  let selectRangePrefEnd    = $state(0);
  let rangeTarget           = $state(/** @type {{start:number,end:number,seq:number}|null} */ (null));
  let rangeTargetSeq        = 0;

  // ── A2L data view ─────────────────────────────────────────────────────────
  let viewMode     = $state(/** @type {'hex'|'data'} */ ('hex'));
  let a2lPath      = $state('');
  let a2lSummary   = $state(/** @type {any} */ (null));
  let a2lRows      = $state(/** @type {any[]} */ ([]));
  let a2lStatsData = $state(/** @type {any} */ (null));
  let a2lDetailData = $state(/** @type {any} */ (null));
  let a2lSelected  = $state(/** @type {string|null} */ (null));
  let a2lLoading   = $state(false);
  let a2lDecoding  = $state(false);

  const a2lFileName = $derived(a2lPath ? (a2lPath.split(/[\\/]/).at(-1) ?? '') : '');

  // Remember which A2L was last used with a given hex file so the load dialog
  // can pre-fill it. Deliberately not auto-loaded — the association is a hint,
  // not a fact, and silently decoding against the wrong A2L is worse than
  // asking.
  const a2lMemoryKey = (hexPath) => `a2lFor.${hexPath}`;
  function rememberA2l(hexPath, path) {
    if (hexPath && path) lsSet(a2lMemoryKey(hexPath), path);
  }
  function recallA2l(hexPath) {
    return hexPath ? lsGet(a2lMemoryKey(hexPath), '') : '';
  }

  // ── Display preferences ────────────────────────────────────────────────────
  let fontSize        = $state(parseInt(lsGet('fontSize',    '13')));
  let bytesPerRow     = $state(parseInt(lsGet('bytesPerRow', '16')));
  let theme           = $state(lsGet('theme', 'system'));
  let showPreferences = $state(false);
  let showFileAssoc = $state(false);
  let showMeasurements = $state(lsGet('showMeasurements', 'false') === 'true');

  $effect(() => { lsSet('fontSize',    fontSize); });
  $effect(() => { lsSet('bytesPerRow', bytesPerRow); });
  $effect(() => { lsSet('theme',       theme); });
  $effect(() => { lsSet('showMeasurements', showMeasurements); });

  $effect(() => {
    document.documentElement.setAttribute('data-theme', theme === 'system' ? '' : theme);
  });

  // ── Side-panel visibility — defaults to true on first launch ─────────────
  let showSegmentList   = $state(lsGet('showSegmentList',   'true') === 'true');
  let showDataInspector = $state(lsGet('showDataInspector', 'true') === 'true');

  // Persist panel state immediately whenever it changes
  $effect(() => { lsSet('showSegmentList',   showSegmentList); });
  $effect(() => { lsSet('showDataInspector', showDataInspector); });

  // References to native CheckMenuItems so we can sync their checked state
  let segmentListMenuItem   = null;
  let dataInspectorMenuItem = null;
  let exportHtmlMenuItem    = null;
  let compareMenuItem       = null;
  let a2lUnloadMenuItem     = null;
  let hexViewMenuItem       = null;
  let dataViewMenuItem      = null;

  // Keep native menu checkmarks in sync with state.
  // NOTE: the value must be read into a local variable BEFORE the ?. call —
  // optional chaining short-circuits argument evaluation when the object is
  // null, so Svelte would never track the dependency otherwise.
  $effect(() => { const v = showSegmentList;   segmentListMenuItem?.setChecked(v); });
  $effect(() => { const v = showDataInspector; dataInspectorMenuItem?.setChecked(v); });
  $effect(() => { const v = records.length > 0; exportHtmlMenuItem?.setEnabled(v); });
  $effect(() => { const v = records.length > 0; compareMenuItem?.setEnabled(v); });
  $effect(() => { const v = a2lSummary !== null; a2lUnloadMenuItem?.setEnabled(v); });
  $effect(() => { const v = a2lSummary !== null; dataViewMenuItem?.setEnabled(v); });
  $effect(() => { const v = viewMode === 'hex';  hexViewMenuItem?.setChecked(v); });
  $effect(() => { const v = viewMode === 'data'; dataViewMenuItem?.setChecked(v); });

  // ── Data Inspector address — follows scroll unless pinned by a byte click ─
  let inspectorAddress = $state(0);
  let inspectorPinned  = $state(false);

  $effect(() => { if (!inspectorPinned) inspectorAddress = hexTopAddress; });

  function handleByteClick(addr) {
    inspectorAddress = addr;
    inspectorPinned  = true;
  }

  let hexSelection = $state(/** @type {{start:number,end:number,count:number,focus:number}|null} */ (null));
  function handleSelectionChange(sel) {
    hexSelection = sel;
    if (sel) { inspectorAddress = sel.focus; inspectorPinned = true; }
  }

  // ── Edit operation handlers ───────────────────────────────────────────────

  function handleEditByte(addr, byte) {
    pushUndo('Edit byte');
    records = writeBytes(records, addr, [byte]);
    status = `Edited 0x${addr.toString(16).toUpperCase().padStart(8,'0')} → 0x${byte.toString(16).toUpperCase().padStart(2,'0')}`;
  }

  function handleDelete(lo, hi) {
    pushUndo('Delete');
    records = deleteRange(records, lo, hi);
    const n = hi - lo + 1;
    status = `Deleted ${n.toLocaleString()} byte${n === 1 ? '' : 's'} at 0x${lo.toString(16).toUpperCase().padStart(8,'0')}`;
  }

  function handleCut(lo, hi) {
    pushUndo('Cut');
    binaryClipboard = { addr: lo, bytes: getBytesRange(records, lo, hi) };
    records = deleteRange(records, lo, hi);
    const n = hi - lo + 1;
    status = `Cut ${n.toLocaleString()} byte${n === 1 ? '' : 's'} at 0x${lo.toString(16).toUpperCase().padStart(8,'0')}`;
  }

  function handleFillOpen(lo, hi) {
    fillSelMin = lo;
    fillSelMax = hi;
    showFill = true;
  }

  function handleFillConfirm({ pattern, randomize, mode }) {
    showFill = false;
    const len = fillSelMax - fillSelMin + 1;
    let fillBytes;
    if (randomize) {
      fillBytes = genRandomBytes(len);
    } else {
      const { filled } = buildFill(pattern, len);
      fillBytes = filled;
    }
    pushUndo('Fill');
    records = mode === 'overwrite'
      ? writeBytes(records, fillSelMin, fillBytes)
      : writeBytesEmpty(records, fillSelMin, fillBytes);
    status = `Filled ${len.toLocaleString()} bytes at 0x${fillSelMin.toString(16).toUpperCase().padStart(8,'0')}`;
  }

  function handlePaste(addr, mode) {
    if (!binaryClipboard) return;
    const { bytes } = binaryClipboard;
    pushUndo('Paste');
    records = mode === 'overwrite'
      ? writeBytes(records, addr, bytes)
      : writeBytesEmpty(records, addr, bytes);
    const n = bytes.length;
    status = `Pasted ${n.toLocaleString()} byte${n === 1 ? '' : 's'} at 0x${addr.toString(16).toUpperCase().padStart(8,'0')} (${mode})`;
  }

  function handleMoveOpen(lo, hi) {
    moveSelMin = lo;
    moveSelMax = hi;
    showMove = true;
  }

  function handleMoveConfirm({ targetAddr, mode }) {
    showMove = false;
    const bytes = getBytesRange(records, moveSelMin, moveSelMax);
    pushUndo('Move');
    let updated = deleteRange(records, moveSelMin, moveSelMax);
    updated = mode === 'overwrite'
      ? writeBytes(updated, targetAddr, bytes)
      : writeBytesEmpty(updated, targetAddr, bytes);
    records = updated;
    const n = bytes.length;
    status = `Moved ${n.toLocaleString()} bytes to 0x${targetAddr.toString(16).toUpperCase().padStart(8,'0')}`;
  }

  function handleChecksumOpen() {
    const d = findChecksumDefaults(records);
    checksumPrefMin    = d?.firstAddr  ?? 0;
    checksumPrefMax    = d?.rangeEnd   ?? 0;
    checksumPrefTarget = d?.targetAddr ?? null;
    showChecksum = true;
  }

  function handleChecksumInsert({ lo, hi, algo, targetAddr, width, le }) {
    showChecksum = false;
    const val   = computeChecksum(records, lo, hi, algo);
    const bytes = numberToBytes(val, width, le);
    pushUndo('Insert checksum');
    records = writeBytes(records, targetAddr, bytes);
    const hex = '0x' + val.toString(16).toUpperCase().padStart(width * 2, '0');
    status = `Inserted ${algo.toUpperCase()} checksum ${hex} at 0x${targetAddr.toString(16).toUpperCase().padStart(8,'0')}`;
  }

  function handleSelectRangeOpen() {
    if (records.length === 0) return;
    selectRangePrefStart = hexSelection ? hexSelection.start : addrRange.min;
    selectRangePrefEnd   = hexSelection ? hexSelection.end   : addrRange.max;
    showSelectRange = true;
  }

  function handleSelectRangeConfirm({ start, end }) {
    showSelectRange  = false;
    rangeTarget      = { start, end, seq: ++rangeTargetSeq };
    inspectorAddress = start;
    inspectorPinned  = true;
    const n = end - start + 1;
    status = `Selected ${n.toLocaleString()} byte${n === 1 ? '' : 's'}: 0x${start.toString(16).toUpperCase().padStart(8,'0')} – 0x${end.toString(16).toUpperCase().padStart(8,'0')}`;
  }

  // ── A2L handlers ──────────────────────────────────────────────────────────

  // Any change to the byte image invalidates every decoded physical value.
  // Marking stale rather than re-decoding immediately keeps hex-view editing
  // free of an IPC round-trip per keystroke.
  let a2lStale = $state(false);

  $effect(() => {
    void records;
    void showMeasurements;
    if (a2lSummary) a2lStale = true;
  });

  $effect(() => {
    if (viewMode === 'data' && a2lSummary && a2lStale && !a2lDecoding) {
      refreshA2lData();
    }
  });

  // The data view cannot exist without a description.
  $effect(() => { if (!a2lSummary && viewMode === 'data') viewMode = 'hex'; });

  async function refreshA2lData() {
    if (!a2lSummary) return;
    a2lStale    = false;   // cleared first so the effect cannot re-enter
    a2lDecoding = true;
    try {
      const [rows, st] = await Promise.all([
        a2lList(records, showMeasurements),
        a2lStats(records, showMeasurements),
      ]);
      a2lRows      = rows;
      a2lStatsData = st;
      if (a2lSelected) await loadA2lDetail(a2lSelected);
    } catch (err) {
      status = `Could not decode against the A2L: ${err}`;
    } finally {
      a2lDecoding = false;
    }
  }

  async function loadA2lDetail(name) {
    const row = a2lRows.find((r) => r.name === name);
    // Only 1D objects have point arrays worth fetching.
    if (!row || row.category !== 'curve') { a2lDetailData = null; return; }
    try {
      a2lDetailData = await a2lDetail(name, records);
    } catch {
      a2lDetailData = null;
    }
  }

  async function handleA2lSelect(name) {
    a2lSelected = name;
    await loadA2lDetail(name);
  }

  async function handleA2lLoadOpen() {
    const remembered = recallA2l(currentFile);
    const selected = await open({
      multiple: false,
      defaultPath: remembered || undefined,
      filters: [
        { name: 'A2L description', extensions: ['a2l'] },
        { name: 'All files', extensions: ['*'] },
      ],
    });
    if (!selected) return;
    await handleA2lLoadPath(selected);
  }

  async function handleA2lLoadPath(path) {
    a2lLoading = true;
    status     = 'Parsing A2L…';
    try {
      const summary = await a2lLoad(path);
      a2lSummary    = summary;
      a2lPath       = path;
      a2lSelected   = null;
      a2lDetailData = null;
      rememberA2l(currentFile, path);

      await refreshA2lData();
      viewMode = 'data';

      let msg = `A2L loaded · ${summary.characteristic_count} characteristics`
              + `, ${summary.axis_pts_count} axis · ${summary.measurement_count} measurements`;
      if (a2lStatsData) {
        msg += ` · ${a2lStatsData.coverage_pct.toFixed(1)} % of image described`;
      }
      if (summary.warnings.length > 0) {
        msg += ` · ⚠ ${summary.warnings.length} parser warning${summary.warnings.length > 1 ? 's' : ''}`;
      }
      status = msg;
    } catch (err) {
      await message(String(err), { kind: 'error', title: 'Cannot load A2L file' });
      status = '';
    } finally {
      a2lLoading = false;
    }
  }

  async function handleA2lUnload() {
    try { await a2lUnload(); } catch { /* dropping state cannot meaningfully fail */ }
    a2lSummary    = null;
    a2lPath       = '';
    a2lRows       = [];
    a2lStatsData  = null;
    a2lDetailData = null;
    a2lSelected   = null;
    viewMode      = 'hex';
    status        = 'A2L unloaded';
  }

  /**
   * Apply an edited physical value. The bytes come from Rust but are written
   * here through writeBytes, so the change joins the same undo stack and
   * modified flag as a hex edit instead of forming a parallel edit path.
   */
  async function applyA2lWrite(name, encoded) {
    pushUndo(`Edit ${name}`);
    records = writeBytes(records, encoded.address, encoded.bytes);
    const hex = encoded.bytes.map((b) => b.toString(16).toUpperCase().padStart(2, '0')).join(' ');
    status = `${name} → ${encoded.phys} (${hex} @ 0x${encoded.address.toString(16).toUpperCase().padStart(8, '0')})`;
  }

  async function handleA2lEditValue(name, phys) {
    try {
      await applyA2lWrite(name, await a2lEncodeValue(name, phys));
    } catch (err) {
      status = `Cannot write ${name}: ${err}`;
    }
  }

  async function handleA2lEditText(name, text) {
    try {
      await applyA2lWrite(name, await a2lEncodeText(name, text));
    } catch (err) {
      status = `Cannot write ${name}: ${err}`;
    }
  }

  /** Jump from a parameter to its bytes in the hex view. */
  function handleA2lGoto(addr) {
    viewMode         = 'hex';
    gotoTarget       = { addr, seq: ++gotoSeq };
    inspectorAddress = addr;
    inspectorPinned  = true;
    status = `0x${addr.toString(16).toUpperCase().padStart(8, '0')}`;
  }

  function handleImportMergeConfirm({ importedRecords, mode }) {
    showImportMerge = false;
    // Get all bytes from imported records
    const importedNorm = normalize(importedRecords);
    pushUndo('Import from file');
    let updated = cloneRecords(records);
    for (const rec of importedNorm) {
      updated = mode === 'overwrite'
        ? writeBytes(updated, rec.address, rec.data)
        : writeBytesEmpty(updated, rec.address, rec.data);
    }
    records = normalize(updated);
    const total = importedNorm.reduce((s, r) => s + r.data.length, 0);
    status = `Merged ${total.toLocaleString()} bytes from file (${mode})`;
  }

  // ── Global keyboard shortcuts ─────────────────────────────────────────────
  function handleGlobalKey(e) {
    const mod = e.metaKey || e.ctrlKey;
    // Undo / Redo
    if (mod && e.key === 'z' && !e.shiftKey) { undo(); e.preventDefault(); return; }
    if (mod && ((e.key === 'z' && e.shiftKey) || e.key === 'y')) { redo(); e.preventDefault(); return; }
    // Cut (Cmd/Ctrl+X)
    if (mod && e.key === 'x' && hexSelection && records.length > 0) {
      handleCut(hexSelection.start, hexSelection.end);
      e.preventDefault(); return;
    }
    // Paste (Cmd/Ctrl+V)
    if (mod && e.key === 'v' && binaryClipboard && records.length > 0) {
      const pasteAddr = hexSelection ? hexSelection.start : hexTopAddress;
      handlePaste(pasteAddr, 'overwrite');
      e.preventDefault(); return;
    }
    // Select Range (Cmd/Ctrl+Shift+A)
    if (mod && e.shiftKey && e.key === 'A') { handleSelectRangeOpen(); e.preventDefault(); return; }
    // Delete key — remove selection from address space
    if (e.key === 'Delete' && hexSelection && records.length > 0 && !mod) {
      handleDelete(hexSelection.start, hexSelection.end);
      e.preventDefault(); return;
    }
  }

  let unlistenDragDrop;
  let unlistenOpenFile;
  let resizeDebounce = null;

  // Address range of the loaded file (for GoToDialog validation)
  const addrRange = $derived((() => {
    let min = Infinity, max = -Infinity;
    for (const r of records) {
      const isData = r.record_type === 'Data' || r.record_type === 'S1'
                  || r.record_type === 'S2'   || r.record_type === 'S3';
      if (!isData || r.data.length === 0) continue;
      if (r.address < min) min = r.address;
      if (r.address + r.data.length - 1 > max) max = r.address + r.data.length - 1;
    }
    return min === Infinity ? { min: 0, max: 0 } : { min, max };
  })());

  function handleFindOpen() {
    if (records.length === 0) return;
    showFind = true;
  }

  function handleFindNavigate(addr) {
    gotoTarget       = { addr, seq: ++gotoSeq };
    inspectorAddress = addr;
    inspectorPinned  = true;
    status           = `Match at 0x${addr.toString(16).toUpperCase().padStart(8, '0')}`;
  }

  function handleGotoOpen() {
    if (records.length === 0) return;
    showGoto = true;
  }

  function handleCompareOpen() {
    if (records.length === 0) return;
    compareFile = '';
    showCompare = true;
  }

  async function handleCompare(refPath, cmpPath) {
    showCompare = false;
    const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
    const label = `compare_${Date.now()}`;
    // Estimate minimum width to show all hex columns without horizontal scrolling.
    // Each byte cell is ~3ch wide; at 12px Cascadia Code 1ch ≈ 7.2px → ~22px per cell.
    const cellPx    = 22;
    const bpr       = 16;   // diff viewer always renders 16 bytes per row
    const hexSideW  = bpr * cellPx + 8 + 8;   // bytes + mid-gap + side-padding
    const centerW   = 2 + 90 + 2;              // v-sep + addr-col + v-sep
    const outerPad  = 2 * 12;                  // container left+right padding
    const w = Math.max(700, 2 * hexSideW + centerW + outerPad + 40);
    new WebviewWindow(label, {
      url: `compare?ref=${encodeURIComponent(refPath)}&cmp=${encodeURIComponent(cmpPath)}`,
      title: `Compare — ${refPath.split('/').at(-1)} ↔ ${cmpPath.split('/').at(-1)}`,
      width: w,
      height: 800,
      minWidth: w,
      minHeight: 400,
    });
  }

  function handleGotoConfirm(addr) {
    showGoto         = false;
    gotoTarget       = { addr, seq: ++gotoSeq };
    inspectorAddress = addr;
    inspectorPinned  = true;
    status           = `Navigated to 0x${addr.toString(16).toUpperCase().padStart(8, '0')}`;
  }

  function handleSegmentJump(addr) {
    gotoTarget = { addr, seq: ++gotoSeq };
    status     = `Segment at 0x${addr.toString(16).toUpperCase().padStart(8, '0')}`;
  }

  // ── Shared open logic — called by both dialog and drag-drop ──────────────
  async function handleOpenPath(path) {
    loading = true;
    status  = 'Loading…';
    try {
      const format = await detectFileFormat(path);

      if (format === 'binary') {
        // Show the import dialog to ask for base address
        pendingBinaryPath = path;
        showImportBinary  = true;
        loading = false;
        return;
      }

      const bytes = await openFile(path);

      let parsed;
      if (format === 'ihex') {
        parsed = JSON.parse(await parseIntelHex(bytes));
      } else if (format === 'srec') {
        parsed = JSON.parse(await parseSrec(bytes));
      } else {
        await message(`Unsupported format: ${format}`, { kind: 'error', title: 'Cannot open file' });
        loading = false;
        return;
      }

      records          = parsed.records;
      inspectorPinned  = false;
      currentFile      = path;
      currentFormat    = format;
      resetUndoHistory();
      const fileName = path.split('/').at(-1);
      await getCurrentWindow().setTitle(`Hex Studio — ${fileName}`);

      let statusMsg = `Loaded ${parsed.total_data_bytes.toLocaleString()} bytes · ${format.toUpperCase()}`;
      if (parsed.checksum_warnings > 0) {
        statusMsg += ` · ⚠ ${parsed.checksum_warnings} checksum error${parsed.checksum_warnings > 1 ? 's' : ''} corrected`;
      }
      status = statusMsg;
    } catch (err) {
      await message(String(err), { kind: 'error', title: 'Cannot open file' });
    } finally {
      loading = false;
    }
  }

  // ── Open via file dialog ─────────────────────────────────────────────────
  async function handleOpen() {
    const selected = await open({
      multiple: false,
      filters: [
        { name: 'Firmware files', extensions: ['hex', 'ihex', 'srec', 'mot', 's19', 's28', 's37', 'bin'] },
        { name: 'All files', extensions: ['*'] },
      ],
    });

    if (!selected) return; // user cancelled
    await handleOpenPath(selected);
  }

  // ── Import Binary — opens dialog filtered to .bin only ───────────────────
  async function handleImportBinaryOpen() {
    const selected = await open({
      multiple: false,
      filters: [
        { name: 'Binary files', extensions: ['bin'] },
        { name: 'All files', extensions: ['*'] },
      ],
    });
    if (!selected) return;
    await handleOpenPath(selected);
  }

  // ── Called when user confirms the import binary dialog ───────────────────
  async function handleImportBinary(baseAddr) {
    showImportBinary = false;
    const path = pendingBinaryPath;
    pendingBinaryPath = '';
    loading = true;
    status  = 'Loading…';
    try {
      const bytes = await openFile(path);
      records          = [{ record_type: 'Data', address: baseAddr, data: bytes }];
      inspectorPinned  = false;
      currentFile      = path;
      currentFormat    = 'binary';
      resetUndoHistory();
      const fileName = path.split('/').at(-1);
      await getCurrentWindow().setTitle(`Hex Studio — ${fileName}`);
      status = `Loaded ${bytes.length.toLocaleString()} bytes · Binary @ 0x${baseAddr.toString(16).toUpperCase().padStart(8, '0')}`;
    } catch (err) {
      await message(String(err), { kind: 'error', title: 'Cannot open file' });
    } finally {
      loading = false;
    }
  }

  // ── Save as — step 1: open the format picker modal ───────────────────────
  function handleSave() {
    showFormatPicker = true;
  }

  // ── Save as — step 2: format chosen → open native file-save dialog ───────
  async function handleFormatPicked({ fmt, fillByte = 0xFF }) {
    showFormatPicker = false;

    const stem = currentFile
      ? currentFile.replace(/\.[^/.]+$/, '')   // strip original extension
      : 'output';

    if (fmt === 'binary') {
      const dest = await save({
        filters: [{ name: 'Binary', extensions: ['bin'] }],
        defaultPath: stem + '.bin',
      });
      if (!dest) return;

      saving = true;
      status  = 'Saving…';
      try {
        const sizeBytes = await saveBinary(records, dest, fillByte);
        const name = dest.split('/').at(-1);
        status = `Saved ${name} · Binary (fill=0x${fillByte.toString(16).toUpperCase().padStart(2, '0')}) · ${(sizeBytes / 1024).toFixed(1)} KB`;
      } catch (err) {
        await message(String(err), { kind: 'error', title: 'Cannot save file' });
      } finally {
        saving = false;
      }
      return;
    }

    const filters = fmt === 'ihex'
      ? [{ name: 'Intel HEX',         extensions: ['hex', 'ihex'] }]
      : [{ name: 'Motorola S-record', extensions: ['srec', 'mot', 's19', 's28', 's37'] }];

    const defaultExt = fmt === 'ihex' ? '.hex' : '.srec';

    const dest = await save({ filters, defaultPath: stem + defaultExt });
    if (!dest) return; // user cancelled

    saving = true;
    status  = 'Saving…';
    try {
      await saveFile(records, dest, fmt);
      const name = dest.split('/').at(-1);
      status = `Saved ${name} · ${fmt.toUpperCase()}`;
    } catch (err) {
      await message(String(err), { kind: 'error', title: 'Cannot save file' });
    } finally {
      saving = false;
    }
  }

  // ── Native macOS menu bar ──
  onMount(async () => {
    try {
      const isMac = /Mac/i.test(navigator.platform);

      // ── View menu items — created here so we can call setChecked later ──
      segmentListMenuItem = await CheckMenuItem.new({
        id: 'view-segment-list',
        text: 'Segment List',
        checked: showSegmentList,
        accelerator: 'CmdOrCtrl+Shift+L',
        action: () => { showSegmentList = !showSegmentList; },
      });
      dataInspectorMenuItem = await CheckMenuItem.new({
        id: 'view-data-inspector',
        text: 'Data Inspector',
        checked: showDataInspector,
        accelerator: 'CmdOrCtrl+Shift+I',
        action: () => { showDataInspector = !showDataInspector; },
      });

      hexViewMenuItem = await CheckMenuItem.new({
        id: 'view-hex',
        text: 'Hex View',
        checked: viewMode === 'hex',
        accelerator: 'CmdOrCtrl+1',
        action: () => { viewMode = 'hex'; },
      });
      dataViewMenuItem = await CheckMenuItem.new({
        id: 'view-data',
        text: 'Data View',
        checked: viewMode === 'data',
        enabled: false,
        accelerator: 'CmdOrCtrl+2',
        action: () => { if (a2lSummary) viewMode = 'data'; },
      });

      const aboutItem = await MenuItem.new({
        id: 'about',
        text: 'About Hex Studio',
        action: () => (showAbout = true),
      });

      const preferencesItem = await MenuItem.new({
        id: 'preferences',
        text: 'Preferences…',
        accelerator: 'CmdOrCtrl+,',
        action: () => { showPreferences = true; },
      });

      const fileAssocItem = await MenuItem.new({
        id: 'file-associations',
        text: 'File Associations…',
        action: () => { showFileAssoc = true; },
      });

      const menu = await Menu.new({
        items: [
          // ① App menu — macOS only (Services / Hide / Quit)
          ...(isMac ? [
            await Submenu.new({
              text: 'Hex Studio',
              items: [
                preferencesItem,
                aboutItem,
                await PredefinedMenuItem.new({ item: 'Separator' }),
                await PredefinedMenuItem.new({ item: 'Services' }),
                await PredefinedMenuItem.new({ item: 'Separator' }),
                await PredefinedMenuItem.new({ item: 'Hide' }),
                await PredefinedMenuItem.new({ item: 'HideOthers' }),
                await PredefinedMenuItem.new({ item: 'ShowAll' }),
                await PredefinedMenuItem.new({ item: 'Separator' }),
                await PredefinedMenuItem.new({ item: 'Quit' }),
              ],
            }),
          ] : []),
          // ② File
          await Submenu.new({
            text: 'File',
            items: [
              await MenuItem.new({ id: 'open', text: 'Open…', accelerator: 'CmdOrCtrl+O', action: handleOpen }),
              await MenuItem.new({ id: 'save-as', text: 'Save as…', accelerator: 'CmdOrCtrl+Shift+S', action: handleSave }),
              (exportHtmlMenuItem = await MenuItem.new({ id: 'export-html', text: 'Export as HTML…', enabled: false, action: () => (showExportHtml = true) })),
              await MenuItem.new({ id: 'import-binary', text: 'Import Binary…', accelerator: 'CmdOrCtrl+B', action: handleImportBinaryOpen }),
              await PredefinedMenuItem.new({ item: 'Separator' }),
              await MenuItem.new({ id: 'a2l-load', text: 'Load associated A2L to enable data view…', accelerator: 'CmdOrCtrl+Shift+D', action: handleA2lLoadOpen }),
              (a2lUnloadMenuItem = await MenuItem.new({ id: 'a2l-unload', text: 'Unload A2L', enabled: false, action: handleA2lUnload })),
              await PredefinedMenuItem.new({ item: 'Separator' }),
              (compareMenuItem = await MenuItem.new({ id: 'compare', text: 'Compare with…', enabled: false, action: handleCompareOpen })),
              await PredefinedMenuItem.new({ item: 'Separator' }),
              await PredefinedMenuItem.new({ item: 'CloseWindow' }),
            ],
          }),
          // ③ Edit
          await Submenu.new({
            text: 'Edit',
            items: [
              await MenuItem.new({ id: 'undo', text: 'Undo', accelerator: 'CmdOrCtrl+Z', action: undo }),
              await MenuItem.new({ id: 'redo', text: 'Redo', accelerator: 'CmdOrCtrl+Shift+Z', action: redo }),
              await PredefinedMenuItem.new({ item: 'Separator' }),
              await MenuItem.new({ id: 'edit-cut',    text: 'Cut',    accelerator: 'CmdOrCtrl+X', action: () => { if (hexSelection) handleCut(hexSelection.start, hexSelection.end); } }),
              await MenuItem.new({ id: 'edit-paste',  text: 'Paste',  accelerator: 'CmdOrCtrl+V', action: () => { if (binaryClipboard) handlePaste(hexSelection ? hexSelection.start : hexTopAddress, 'overwrite'); } }),
              await MenuItem.new({ id: 'edit-delete', text: 'Delete', action: () => { if (hexSelection) handleDelete(hexSelection.start, hexSelection.end); } }),
              await PredefinedMenuItem.new({ item: 'Separator' }),
              await MenuItem.new({ id: 'edit-select-range', text: 'Select Range…', accelerator: 'CmdOrCtrl+Shift+A', action: () => { if (records.length > 0) handleSelectRangeOpen(); } }),
              await PredefinedMenuItem.new({ item: 'Separator' }),
              await MenuItem.new({ id: 'edit-fill',   text: 'Fill Selection…',   action: () => { if (hexSelection) handleFillOpen(hexSelection.start, hexSelection.end); } }),
              await MenuItem.new({ id: 'edit-move',   text: 'Move Selection…',   action: () => { if (hexSelection) handleMoveOpen(hexSelection.start, hexSelection.end); } }),
              await MenuItem.new({ id: 'edit-import-merge', text: 'Import from File…', action: () => { if (records.length > 0) showImportMerge = true; } }),
              await PredefinedMenuItem.new({ item: 'Separator' }),
              await MenuItem.new({ id: 'edit-checksum', text: 'Insert Checksum…', action: () => { if (records.length > 0) handleChecksumOpen(); } }),
            ],
          }),
          // ④ Search
          await Submenu.new({
            text: 'Search',
            items: [
              await MenuItem.new({ id: 'find', text: 'Find…', accelerator: 'CmdOrCtrl+F', action: handleFindOpen }),
              await PredefinedMenuItem.new({ item: 'Separator' }),
              await MenuItem.new({ id: 'goto-address', text: 'Go to Address…', accelerator: 'CmdOrCtrl+G', action: handleGotoOpen }),
            ],
          }),
          // ⑤ View — view mode, side panels, native fullscreen
          await Submenu.new({
            text: 'View',
            items: [
              hexViewMenuItem,
              dataViewMenuItem,
              await PredefinedMenuItem.new({ item: 'Separator' }),
              segmentListMenuItem,
              dataInspectorMenuItem,
              await PredefinedMenuItem.new({ item: 'Separator' }),
              await PredefinedMenuItem.new({ item: 'Fullscreen' }),
              ...(!isMac ? [
                await PredefinedMenuItem.new({ item: 'Separator' }),
                preferencesItem,
                await PredefinedMenuItem.new({ item: 'Separator' }),
                fileAssocItem,
              ] : [
                await PredefinedMenuItem.new({ item: 'Separator' }),
                fileAssocItem,
              ]),
            ],
          }),
          // ⑥ Help — Windows / Linux only (About lives here)
          ...(!isMac ? [
            await Submenu.new({
              text: 'Help',
              items: [aboutItem],
            }),
          ] : []),
        ],
      });

      await menu.setAsAppMenu();
    } catch (err) {
      // Non-fatal: native menu is best-effort
      console.warn('Could not build native menu:', err);
    }

    // Window size save/restore is handled by tauri-plugin-window-state on the
    // Rust side — no JS code needed here.

    // ── Drag-and-drop support ──
    try {
      unlistenDragDrop = await getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type === 'drop') {
          isDragging = false;
          const paths = event.payload.paths;
          if (paths.length > 0) {
            // Tauri's drag-drop is window-global rather than per-element, so
            // route by extension instead of by where the pointer landed.
            if (/\.a2l$/i.test(paths[0]))   handleA2lLoadPath(paths[0]);
            else if (showCompare)           compareFile = paths[0];
            else                            handleOpenPath(paths[0]);
          }
        } else if (event.payload.type === 'enter') {
          isDragging = true;
        } else if (event.payload.type === 'leave') {
          isDragging = false;
        }
      });
      document.addEventListener('dragover', (e) => e.preventDefault(), { passive: false });
      document.addEventListener('drop', (e) => e.preventDefault(), { passive: false });
    } catch (e) {
      console.warn('Drag-drop setup failed:', e);
    }

    // ── OS file-association open ──────────────────────────────────────────────
    // Warm launch: app already running, macOS sends the file via an event
    try {
      unlistenOpenFile = await listen('open-file', (event) => {
        handleOpenPath(event.payload);
      });
    } catch (e) {
      console.warn('open-file listener failed:', e);
    }
    // Cold launch: path was captured by Rust before the webview was ready
    try {
      const startupPath = await getStartupFile();
      if (startupPath) handleOpenPath(startupPath);
    } catch (e) {
      console.warn('get_startup_file failed:', e);
    }
  });

  onDestroy(() => {
    if (unlistenDragDrop) unlistenDragDrop();
    if (unlistenOpenFile) unlistenOpenFile();
    clearTimeout(resizeDebounce);
  });
</script>

<svelte:window onkeydown={handleGlobalKey} />

<AboutDialog open={showAbout} onClose={() => (showAbout = false)} />

<FindDialog
  open={showFind}
  {records}
  topAddress={hexTopAddress}
  onNavigate={handleFindNavigate}
  onClose={() => (showFind = false)}
/>

<GoToDialog
  open={showGoto}
  prefill={hexTopAddress}
  minAddr={addrRange.min}
  maxAddr={addrRange.max}
  onGoto={handleGotoConfirm}
  onCancel={() => (showGoto = false)}
/>

<SaveFormatDialog
  open={showFormatPicker}
  onPick={handleFormatPicked}
  onCancel={() => (showFormatPicker = false)}
/>

<ImportBinaryDialog
  open={showImportBinary}
  onImport={handleImportBinary}
  onCancel={() => { showImportBinary = false; pendingBinaryPath = ''; }}
/>

<PreferencesDialog
  open={showPreferences}
  {fontSize}
  {bytesPerRow}
  {theme}
  {showMeasurements}
  onFontSize={(n) => { fontSize = n; }}
  onBytesPerRow={(n) => { bytesPerRow = n; }}
  onTheme={(t) => { theme = t; }}
  onShowMeasurements={(v) => { showMeasurements = v; }}
  onClose={() => { showPreferences = false; }}
/>

<FileAssocDialog open={showFileAssoc} onClose={() => { showFileAssoc = false; }} />

<HexExportDialog
  open={showExportHtml}
  {records}
  {bytesPerRow}
  currentFile={currentFile}
  currentFormat={currentFormat}
  onClose={() => showExportHtml = false}
/>

<CompareDialog
  open={showCompare}
  referenceFile={currentFile}
  bind:comparedFile={compareFile}
  {isDragging}
  onCompare={handleCompare}
  onCancel={() => { showCompare = false; }}
/>

<FillDialog
  open={showFill}
  selMin={fillSelMin}
  selMax={fillSelMax}
  onFill={handleFillConfirm}
  onClose={() => (showFill = false)}
/>

<MoveDialog
  open={showMove}
  sourceMin={moveSelMin}
  sourceMax={moveSelMax}
  onMove={handleMoveConfirm}
  onClose={() => (showMove = false)}
/>

<ChecksumDialog
  open={showChecksum}
  {records}
  prefillMin={checksumPrefMin}
  prefillMax={checksumPrefMax}
  prefillTarget={checksumPrefTarget}
  onInsert={handleChecksumInsert}
  onClose={() => (showChecksum = false)}
/>

<ImportMergeDialog
  open={showImportMerge}
  onMerge={handleImportMergeConfirm}
  onClose={() => (showImportMerge = false)}
/>

<SelectRangeDialog
  open={showSelectRange}
  prefillStart={selectRangePrefStart}
  prefillEnd={selectRangePrefEnd}
  onSelect={handleSelectRangeConfirm}
  onClose={() => (showSelectRange = false)}
/>

{#if isDragging}
  <div class="drop-overlay">
    <div class="drop-card">
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none"
           stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <!-- Box bottom -->
        <rect x="8" y="28" width="32" height="12" rx="3"/>
        <!-- Arrow downward into box -->
        <line x1="24" y1="8" x2="24" y2="28"/>
        <polyline points="16,20 24,28 32,20"/>
      </svg>
      <p>Drop to open</p>
    </div>
  </div>
{/if}

<div class="app-shell" onclick={() => { if (!loading && !saving) status = ''; }}>
  <!-- Toolbar: open + save icons -->
  <FileMenu
    onOpen={handleOpen}
    onSave={handleSave}
    onExport={() => showExportHtml = true}
    onFind={handleFindOpen}
    onGoto={handleGotoOpen}
    onCompare={handleCompareOpen}
    onSettings={() => showPreferences = true}
    {loading} {saving}
    hasFile={records.length > 0}
    onUndo={undo}
    onRedo={redo}
    canUndo={undoStack.length > 0}
    canRedo={redoStack.length > 0}
    onFill={() => { if (hexSelection) handleFillOpen(hexSelection.start, hexSelection.end); }}
    onMove={() => { if (hexSelection) handleMoveOpen(hexSelection.start, hexSelection.end); }}
    onChecksum={handleChecksumOpen}
    onImportMerge={() => { if (records.length > 0) showImportMerge = true; }}
    onSelectRange={handleSelectRangeOpen}
    hasSelection={hexSelection !== null}
    {viewMode}
    onViewMode={(m) => { viewMode = m; }}
    a2lName={a2lFileName}
    {a2lLoading}
    onLoadA2l={handleA2lLoadOpen}
    onUnloadA2l={handleA2lUnload}
  />

  <div class="content-area">
    <main class="viewer-area">
      {#if viewMode === 'data'}
        <DataView
          rows={a2lRows}
          stats={a2lStatsData}
          detail={a2lDetailData}
          selected={a2lSelected}
          loading={a2lDecoding}
          {fontSize}
          onSelect={handleA2lSelect}
          onEditValue={handleA2lEditValue}
          onEditText={handleA2lEditText}
          onGoto={handleA2lGoto}
        />
      {:else}
        <HexViewer
          {records}
          {bytesPerRow}
          {fontSize}
          {gotoTarget}
          {rangeTarget}
          onScrolled={() => { if (!loading && !saving) status = ''; }}
          onTopAddress={(addr) => { hexTopAddress = addr; }}
          onByteClick={handleByteClick}
          onSelectionChange={handleSelectionChange}
          editable={records.length > 0}
          onEditByte={handleEditByte}
          onDelete={handleDelete}
          onCut={handleCut}
          onFill={handleFillOpen}
          onMove={handleMoveOpen}
          onPaste={handlePaste}
          clipboardSize={binaryClipboard ? binaryClipboard.bytes.length : 0}
        />
      {/if}
    </main>

    {#if showSegmentList || showDataInspector}
      <aside class="side-panel">
        {#if showSegmentList}
          <div class="side-section">
            <SegmentList {records} topAddress={hexTopAddress} onJump={handleSegmentJump} onClose={() => (showSegmentList = false)} />
          </div>
        {/if}
        {#if showSegmentList && showDataInspector}
          <div class="side-divider"></div>
        {/if}
        {#if showDataInspector}
          <div class="side-section">
            <DataInspector
              {records}
              address={inspectorAddress}
              pinned={inspectorPinned}
              onUnpin={() => (inspectorPinned = false)}
              onClose={() => (showDataInspector = false)}
            />
          </div>
        {/if}
      </aside>
    {/if}
  </div>

  <footer class="statusbar">
    {#if status}
      <span>{status}</span>
    {:else if !currentFile}
      <span class="hint">Open a HEX, S-record or Binary file to get started</span>
    {/if}
    {#if hexSelection}
      <span class="sel-info">
        Sel&nbsp;{hexSelection.start.toString(16).padStart(8,'0').toUpperCase()}
        –&nbsp;{hexSelection.end.toString(16).padStart(8,'0').toUpperCase()}
        &nbsp;·&nbsp;{hexSelection.count.toLocaleString()} byte{hexSelection.count === 1 ? '' : 's'}
      </span>
    {/if}
  </footer>
</div>

<style>
  .app-shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  /* ── Content area: hex viewer + optional side panel ── */
  .content-area {
    flex: 1;
    display: flex;
    flex-direction: row;
    overflow: hidden;
    min-height: 0;
  }

  .viewer-area {
    flex: 1;
    overflow: hidden;
    min-width: 0;
  }

  /* ── Side panel ── */
  .side-panel {
    width: 280px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--c-hover);
    overflow: hidden;
    background: var(--c-bg);
  }

  .side-section {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .side-divider {
    height: 1px;
    flex-shrink: 0;
    background: var(--c-hover);
  }

  .statusbar {
    display: flex;
    align-items: center;
    height: 22px;
    padding: 0 10px;
    background: var(--c-accent);
    color: #fff;
    font-size: 11.5px;
    font-weight: 400;
    letter-spacing: 0.01em;
    user-select: none;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 0;
  }

  .statusbar .hint {
    opacity: 0.75;
  }

  .statusbar .sel-info {
    margin-left: auto;
    opacity: 0.9;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.02em;
  }

  /* Drag-and-drop overlay */
  .drop-overlay {
    position: fixed;
    inset: 0;
    z-index: 200;
    background: var(--c-accent-bg);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
  }

  .drop-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    padding: 36px 48px;
    border: 2.5px dashed var(--c-accent);
    border-radius: 16px;
    color: var(--c-accent);
    background: var(--c-accent-bg);
  }

  .drop-card svg {
    width: 56px;
    height: 56px;
  }

  .drop-card p {
    font-size: 18px;
    font-weight: 600;
    letter-spacing: 0.01em;
    color: var(--c-accent);
  }
</style>
