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
