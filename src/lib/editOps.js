// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

/**
 * editOps.js — pure functions for in-place record manipulation.
 *
 * All mutating functions return a NEW records array (no mutation of input).
 * Records are plain objects: { record_type: string, address: number, data: number[] }
 * Only 'Data', 'S1', 'S2', 'S3' type records carry meaningful byte data;
 * other record types (EOF, StartSegmentAddress, etc.) are dropped in
 * normalised output — the Rust writer re-adds them on save.
 */

/** @param {{ record_type: string; address: number; data: number[] }} rec */
function isDataRecord(rec) {
  return rec.record_type === 'Data' || rec.record_type === 'S1'
      || rec.record_type === 'S2'   || rec.record_type === 'S3';
}

/**
 * Deep-clone a records array.
 * @param {Array} records
 * @returns {Array}
 */
export function cloneRecords(records) {
  return records.map(r => ({ ...r, data: [...r.data] }));
}

/**
 * Flatten all data records into a Map<address, byte>.
 * @internal
 * @param {Array} records
 * @returns {Map<number, number>}
 */
function toByteMap(records) {
  const map = new Map();
  for (const rec of records) {
    if (!isDataRecord(rec) || !rec.data.length) continue;
    for (let i = 0; i < rec.data.length; i++) {
      map.set(rec.address + i, rec.data[i]);
    }
  }
  return map;
}

/**
 * Rebuild a records array from a Map<address, byte>.
 * Consecutive addresses are grouped into contiguous Data records.
 * @internal
 * @param {Map<number, number>} map
 * @returns {Array}
 */
function fromByteMap(map) {
  if (map.size === 0) return [];
  const addrs = Array.from(map.keys()).sort((a, b) => a - b);
  const result = [];
  let i = 0;
  while (i < addrs.length) {
    const segAddr = addrs[i];
    const data = [map.get(addrs[i])];
    let j = i + 1;
    while (j < addrs.length && addrs[j] === addrs[j - 1] + 1) {
      data.push(map.get(addrs[j]));
      j++;
    }
    result.push({ record_type: 'Data', address: segAddr, data });
    i = j;
  }
  return result;
}

/**
 * Normalise records: sort by address, merge contiguous segments, remove empty records.
 * Returns a new Data-only records array.
 * @param {Array} records
 * @returns {Array}
 */
export function normalize(records) {
  return fromByteMap(toByteMap(records));
}

/**
 * Get the byte value at a given address, or null if no data exists there.
 * @param {Array} records
 * @param {number} addr
 * @returns {number|null}
 */
export function getByteAt(records, addr) {
  for (const rec of records) {
    if (!isDataRecord(rec) || !rec.data.length) continue;
    const off = addr - rec.address;
    if (off >= 0 && off < rec.data.length) return rec.data[off];
  }
  return null;
}

/**
 * Extract bytes in the address range [lo, hi] as a Uint8Array.
 * Addresses with no data are filled with 0x00.
 * @param {Array} records
 * @param {number} lo  Inclusive start address
 * @param {number} hi  Inclusive end address
 * @returns {Uint8Array}
 */
export function getBytesRange(records, lo, hi) {
  const len = hi - lo + 1;
  const buf = new Uint8Array(len);
  for (const rec of records) {
    if (!isDataRecord(rec) || !rec.data.length) continue;
    const rEnd = rec.address + rec.data.length - 1;
    if (rEnd < lo || rec.address > hi) continue;
    const from = Math.max(rec.address, lo);
    const to   = Math.min(rEnd, hi);
    for (let a = from; a <= to; a++) {
      buf[a - lo] = rec.data[a - rec.address];
    }
  }
  return buf;
}

/**
 * Remove all bytes in the address range [lo, hi] from the address space.
 * Segments spanning the range are split; bytes outside are untouched.
 * No byte shifting occurs.
 * @param {Array} records
 * @param {number} lo  Inclusive start address
 * @param {number} hi  Inclusive end address
 * @returns {Array}
 */
export function deleteRange(records, lo, hi) {
  const map = toByteMap(records);
  for (let a = lo; a <= hi; a++) map.delete(a);
  return fromByteMap(map);
}

/**
 * Write bytes to the address space in OVERWRITE mode.
 * Existing bytes are overwritten; new addresses extend or create segments.
 * @param {Array} records
 * @param {number} addr  Starting address
 * @param {Uint8Array|number[]} bytes
 * @returns {Array}
 */
export function writeBytes(records, addr, bytes) {
  const map = toByteMap(records);
  for (let i = 0; i < bytes.length; i++) {
    map.set(addr + i, bytes[i]);
  }
  return fromByteMap(map);
}

/**
 * Write bytes to the address space in FILL-EMPTY-ONLY mode.
 * Only writes to addresses that currently contain no data (gaps).
 * @param {Array} records
 * @param {number} addr  Starting address
 * @param {Uint8Array|number[]} bytes
 * @returns {Array}
 */
export function writeBytesEmpty(records, addr, bytes) {
  const map = toByteMap(records);
  for (let i = 0; i < bytes.length; i++) {
    if (!map.has(addr + i)) {
      map.set(addr + i, bytes[i]);
    }
  }
  return fromByteMap(map);
}

/**
 * Build a fill buffer by repeating `pattern` across `length` bytes.
 * @param {Uint8Array|number[]} pattern  Must be non-empty
 * @param {number}              length   Total bytes to produce
 * @returns {{ filled: Uint8Array, truncated: boolean }}
 *   truncated = true when the pattern does not divide evenly into length
 */
export function buildFill(pattern, length) {
  const buf = new Uint8Array(length);
  for (let i = 0; i < length; i++) {
    buf[i] = pattern[i % pattern.length];
  }
  return { filled: buf, truncated: length % pattern.length !== 0 };
}

/**
 * Generate `length` cryptographically random bytes.
 * @param {number} length
 * @returns {Uint8Array}
 */
export function randomBytes(length) {
  const buf = new Uint8Array(length);
  crypto.getRandomValues(buf);
  return buf;
}

/**
 * Compute a checksum over the bytes in address range [lo, hi].
 * Gap addresses are treated as 0x00.
 *
 * Streams through the sorted data records directly — never allocates a flat
 * buffer of (hi − lo + 1) bytes, so it is safe for sparse files whose segments
 * span a wide virtual address space.
 *
 * @param {Array}  records
 * @param {number} lo         Inclusive start address
 * @param {number} hi         Inclusive end address
 * @param {'xor'|'sum8'|'crc16'|'crc32'} algorithm
 * @returns {number}
 */
export function computeChecksum(records, lo, hi, algorithm) {
  // Collect only the records that overlap [lo, hi], sorted by address
  const relevant = [];
  for (const r of records) {
    if (!isDataRecord(r) || !r.data.length) continue;
    if (r.address > hi || r.address + r.data.length - 1 < lo) continue;
    relevant.push(r);
  }
  relevant.sort((a, b) => a.address - b.address);

  // XOR and sum8 are unaffected by zero bytes, so gaps can be skipped entirely
  const gapsAreNoop = algorithm === 'xor' || algorithm === 'sum8';

  let crc = algorithm === 'crc16' ? 0xFFFF : algorithm === 'crc32' ? 0xFFFFFFFF : 0;

  /** Feed a single byte into the running state */
  function feed(b) {
    switch (algorithm) {
      case 'xor':  crc ^= b; break;
      case 'sum8': crc = (crc + b) & 0xFF; break;
      case 'crc16':
        crc ^= (b << 8);
        for (let i = 0; i < 8; i++)
          crc = (crc & 0x8000) ? ((crc << 1) ^ 0x1021) & 0xFFFF : (crc << 1) & 0xFFFF;
        break;
      case 'crc32':
        crc ^= b;
        for (let i = 0; i < 8; i++)
          crc = (crc & 1) ? (crc >>> 1) ^ 0xEDB88320 : crc >>> 1;
        break;
    }
  }

  let addr = lo;
  for (const rec of relevant) {
    const from = Math.max(rec.address, lo);
    const to   = Math.min(rec.address + rec.data.length - 1, hi);

    // Gap before this record — feed zeros unless the algorithm ignores them
    if (!gapsAreNoop) {
      for (; addr < from; addr++) feed(0);
    } else {
      addr = from;
    }

    // Actual data bytes
    for (let a = from; a <= to; a++, addr++) feed(rec.data[a - rec.address]);
  }

  // Trailing gap after last record
  if (!gapsAreNoop) {
    for (; addr <= hi; addr++) feed(0);
  }

  return algorithm === 'crc32' ? (crc ^ 0xFFFFFFFF) >>> 0 : crc;
}

/**
 * Convert a numeric value to a byte array of specified width.
 * @param {number}  value
 * @param {number}  width        1–4 bytes
 * @param {boolean} littleEndian
 * @returns {number[]}
 */
export function numberToBytes(value, width, littleEndian) {
  const bytes = [];
  let v = value >>> 0; // treat as unsigned 32-bit
  for (let i = 0; i < width; i++) {
    bytes.push(v & 0xFF);
    v >>>= 8;
  }
  return littleEndian ? bytes : bytes.reverse();
}

/**
 * Compute smart defaults for the Insert Checksum dialog.
 *
 * Works directly with the records array — never builds a full address→byte Map,
 * so it stays fast even for large firmware images.
 *
 * Returns { firstAddr, rangeEnd, targetAddr } where:
 *   firstAddr  = address of the first data byte in the file (→ source start)
 *   rangeEnd   = first of the last 5 consecutive data bytes (→ source end),
 *                or last data address when no run of ≥5 consecutive bytes exists
 *   targetAddr = second of the last 5 consecutive data bytes (→ write location),
 *                or null when no run of ≥5 consecutive bytes exists
 *
 * Returns null when the file has no data at all.
 *
 * @param {Array} records
 * @returns {{ firstAddr: number, rangeEnd: number, targetAddr: number|null }|null}
 */
export function findChecksumDefaults(records) {
  // Collect sorted data records — O(n_records), not O(n_bytes)
  const dataRecs = records.filter(r => isDataRecord(r) && r.data.length > 0);
  if (dataRecs.length === 0) return null;
  dataRecs.sort((a, b) => a.address - b.address);

  const firstAddr = dataRecs[0].address;
  const lastRec   = dataRecs[dataRecs.length - 1];
  let rangeEnd    = lastRec.address + lastRec.data.length - 1; // fallback
  let targetAddr  = null;

  // Scan backward through records to find the last window of ≥5 consecutive bytes.
  // Tracks a run extending from runTail backward; crosses record boundaries when
  // two adjacent records are contiguous in address space.
  let runTail = 0;
  let runLen  = 0;

  for (let i = dataRecs.length - 1; i >= 0; i--) {
    const rec    = dataRecs[i];
    const recEnd = rec.address + rec.data.length - 1;

    if (runLen === 0) {
      runTail = recEnd;
      runLen  = rec.data.length;
    } else {
      const runHead = runTail - runLen + 1;
      if (recEnd + 1 === runHead) {
        // This record is immediately before the current run — extend it
        runLen += rec.data.length;
      } else {
        // Gap — restart the run from this record
        runTail = recEnd;
        runLen  = rec.data.length;
      }
    }

    if (runLen >= 5) {
      rangeEnd   = runTail - 4; // 1st of the last 5 consecutive bytes
      targetAddr = runTail - 3; // 2nd of the last 5 consecutive bytes
      break;
    }
  }

  return { firstAddr, rangeEnd, targetAddr };
}

/**
 * Parse a hex string (e.g. "FF", "DE AD BE EF", "DEADBEEF") to a Uint8Array.
 * Returns null if the string is empty or contains invalid characters.
 * @param {string} hex
 * @returns {Uint8Array|null}
 */
export function parseHexPattern(hex) {
  // Remove all whitespace, validate characters
  const stripped = hex.replace(/\s+/g, '');
  if (stripped.length === 0) return null;
  if (stripped.length % 2 !== 0) return null;
  if (!/^[0-9a-fA-F]+$/.test(stripped)) return null;
  const buf = new Uint8Array(stripped.length / 2);
  for (let i = 0; i < buf.length; i++) {
    buf[i] = parseInt(stripped.slice(i * 2, i * 2 + 2), 16);
  }
  return buf;
}
