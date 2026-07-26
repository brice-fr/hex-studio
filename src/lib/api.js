// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

/**
 * api.js — thin abstraction over Tauri's `invoke` for the hex-studio backend.
 *
 * All functions return Promises. Errors from Rust propagate as rejected
 * Promises with a string message.
 */

import { invoke } from '@tauri-apps/api/core';

/**
 * Read a file from the filesystem.
 * @param {string} path  Absolute path to the file.
 * @returns {Promise<number[]>}  Raw bytes as a JSON array.
 */
export async function openFile(path) {
  return invoke('open_file', { path });
}

/**
 * Read and parse a file in a single IPC call, returning its records array.
 * Replaces `openFile` + `parseIntelHex`/`parseSrec` for the diff viewer:
 * eliminates the round-trip that serialises raw file bytes through JS.
 * @param {string} path  Absolute path to the file.
 * @returns {Promise<Array>}  Parsed records array.
 */
export async function parseFile(path) {
  const json = await invoke('parse_file', { path });
  return JSON.parse(json).records;
}

/**
 * Parse an Intel HEX payload.
 * @param {number[]} data  Raw bytes (from openFile).
 * @returns {Promise<string>}  JSON string of HexFile structure.
 */
export async function parseIntelHex(data) {
  return invoke('parse_intel_hex', { data });
}

/**
 * Parse a Motorola S-record payload.
 * @param {number[]} data  Raw bytes (from openFile).
 * @returns {Promise<string>}  JSON string of SrecFile structure.
 */
export async function parseSrec(data) {
  return invoke('parse_srec', { data });
}

/**
 * Detect the file format from its path (extension + magic bytes).
 * @param {string} path
 * @returns {Promise<string>}  "ihex" | "srec" | "binary" | "unknown"
 */
export async function detectFileFormat(path) {
  return invoke('detect_file_format', { path });
}

/**
 * Serialise records to the given format and write them to disk.
 * @param {Array<{record_type: string, address: number, data: number[]}>} records
 * @param {string} path    Absolute destination path.
 * @param {string} format  "ihex" or "srec"
 * @returns {Promise<void>}
 */
export async function saveFile(records, path, format) {
  return invoke('save_file', { records, path, format });
}

/**
 * Flatten records into a raw binary blob and write to disk.
 * @param {Array} records
 * @param {string} path  Absolute destination path.
 * @param {number} fillByte  0–255, used to fill gaps between records.
 * @returns {Promise<number>}  Number of bytes written.
 */
export async function saveBinary(records, path, fillByte) {
  return invoke('save_binary', { records, path, fillByte });
}

/**
 * Returns (and clears) the startup file path queued by the Rust backend,
 * or null if the app was not launched via a file-association double-click.
 * @returns {Promise<string|null>}
 */
export async function getStartupFile() {
  return invoke('get_startup_file');
}

/**
 * Write a UTF-8 text file to disk (used by the HTML diff-report exporter).
 * @param {string} path     Absolute destination path.
 * @param {string} content  Text content to write.
 * @returns {Promise<void>}
 */
export async function writeTextFile(path, content) {
  return invoke('write_text_file', { path, content });
}

export async function getFileAssociations() {
  return await invoke('get_file_associations');
}

export async function applyFileAssociations(changes) {
  return await invoke('apply_file_associations', { changes });
}

// ── A2L data view ───────────────────────────────────────────────────────────

/**
 * A parsed record from the loaded image.
 * @typedef {{record_type: string, address: number, data: number[]}} HexRecord
 */

// The parsed A2L is held in Rust state, so `a2lLoad` must succeed before any
// of the others will do anything.

/**
 * Parse an A2L description and keep it loaded for subsequent calls.
 * @param {string} path  Absolute path to the .a2l file.
 * @returns {Promise<Object>}  Summary: object counts, version, parse warnings.
 */
export async function a2lLoad(path) {
  return invoke('a2l_load', { path });
}

/**
 * Drop the loaded A2L description.
 * @returns {Promise<void>}
 */
export async function a2lUnload() {
  return invoke('a2l_unload');
}

/**
 * Decode every described object against the current image.
 * @param {HexRecord[]} records
 * @param {boolean} includeMeasurements  Also list RAM-resident MEASUREMENTs.
 * @returns {Promise<Object[]>}  One row per object.
 */
export async function a2lList(records, includeMeasurements) {
  return invoke('a2l_list', { records, includeMeasurements });
}

/**
 * Full axis and value arrays for one 1D object.
 * @param {string} name
 * @param {HexRecord[]}  records
 * @returns {Promise<Object>}
 */
export async function a2lDetail(name, records) {
  return invoke('a2l_detail', { name, records });
}

/**
 * Coverage of the image by the A2L description.
 * @param {HexRecord[]} records
 * @param {boolean} includeMeasurements
 * @returns {Promise<Object>}
 */
export async function a2lStats(records, includeMeasurements) {
  return invoke('a2l_stats', { records, includeMeasurements });
}

/**
 * Encode a numeric physical value to bytes. Does NOT modify the image —
 * apply the returned bytes with writeBytes so the edit joins the undo stack.
 *
 * `records` is required because a BIT_MASK field shares its stored word with
 * other fields, so encoding one is a read-modify-write over the current image.
 * @param {string} name
 * @param {number} phys
 * @param {HexRecord[]}  records
 * @returns {Promise<{address: number, bytes: number[], raw: number, phys: number}>}
 */
export async function a2lEncodeValue(name, phys, records) {
  return invoke('a2l_encode_value', { name, phys, records });
}

/**
 * Encode a verbal (enumerated) or ASCII string value to bytes.
 * @param {string} name
 * @param {string} text
 * @param {HexRecord[]}  records
 * @returns {Promise<{address: number, bytes: number[], raw: number, phys: number}>}
 */
export async function a2lEncodeText(name, text, records) {
  return invoke('a2l_encode_text', { name, text, records });
}

/**
 * Encode one point of a 1D object (curve, axis or value block).
 *
 * `index` is the row as displayed. An INDEX_DECR axis is shown in the reverse
 * of its storage order, and the backend maps the index back, so callers must
 * pass what the user sees rather than trying to correct for it.
 * @param {string} name
 * @param {'value'|'axis'} target  Which column of the point table.
 * @param {number} index
 * @param {number} phys
 * @param {HexRecord[]} records
 * @returns {Promise<{address: number, bytes: number[], raw: number, phys: number}>}
 */
export async function a2lEncodePoint(name, target, index, phys, records) {
  return invoke('a2l_encode_point', { name, target, index, phys, records });
}

/**
 * @typedef {Object} CdfxChange
 * @property {string} name           A2L object name.
 * @property {'value'|'axis'|'text'} target
 * @property {number|null} index     Point index for a 1D object.
 * @property {string} current        The value in the image now.
 * @property {string} incoming       The value the file would write.
 * @property {number} address
 * @property {number[]} bytes
 */

/**
 * @typedef {Object} CdfxImport
 * @property {string} file_name
 * @property {number} file_instances  Parameters found in the file.
 * @property {number} matched         Resolved to a writable A2L object.
 * @property {number} unchanged       Matched and already in agreement.
 * @property {string[]} not_in_a2l
 * @property {{name: string, reason: string}[]} skipped
 * @property {CdfxChange[]} changes
 */

/**
 * Read a CDFX file and report what importing it would change.
 *
 * Nothing is written: each change carries its own bytes so the caller can show
 * the difference first and then apply them all as one undoable edit.
 * @param {string} path
 * @param {HexRecord[]} records
 * @returns {Promise<CdfxImport>}
 */
export async function cdfxPreview(path, records) {
  return invoke('cdfx_preview', { path, records });
}

/**
 * Write every decodable value out to a CDFX file.
 * @param {string} path
 * @param {HexRecord[]} records
 * @returns {Promise<number>} How many parameters were written.
 */
export async function cdfxExport(path, records) {
  return invoke('cdfx_export', { path, records });
}
