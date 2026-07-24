// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

use std::fs;
use std::path::Path;

/// Unified record structure used for serialising files.
/// Matches the JSON shape produced by both hex_parser and srec_parser.
#[derive(serde::Deserialize)]
pub struct RecordData {
    pub record_type: String,
    pub address: u32,
    pub data: Vec<u8>,
}

// ─── Shared helper ───────────────────────────────────────────────────────────

/// Returns true for any record type that carries actual data bytes,
/// regardless of whether the record came from an IHex or SREC file.
fn is_data_record(rt: &str) -> bool {
    matches!(rt, "Data"          // Intel HEX
               | "S1" | "S2" | "S3") // Motorola S-record
}

// ─── IHex writer ─────────────────────────────────────────────────────────────

/// Serialise a slice of records to Intel HEX text.
/// Only "Data" records are emitted; ExtendedLinearAddress records are
/// generated automatically whenever the upper 16 bits of the address change.
pub fn write_ihex(records: &[RecordData]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut last_upper: Option<u32> = None;

    for rec in records {
        if !is_data_record(&rec.record_type) {
            continue;
        }

        // Chunk large data slices into 32-byte rows (standard IHex width).
        for (i, chunk) in rec.data.chunks(32).enumerate() {
            let abs_addr = rec.address + (i * 32) as u32;
            let upper = abs_addr >> 16;

            // Emit extended linear address record when the upper word changes.
            if last_upper != Some(upper) {
                last_upper = Some(upper);
                let hi = (upper >> 8) as u8;
                let lo = upper as u8;
                let sum = 0x02u8
                    .wrapping_add(0x04) // record type
                    .wrapping_add(hi)
                    .wrapping_add(lo);
                let ck = (!sum).wrapping_add(1);
                lines.push(format!(":02000004{hi:02X}{lo:02X}{ck:02X}"));
            }

            let offset = (abs_addr & 0xFFFF) as u16;
            let ah = (offset >> 8) as u8;
            let al = offset as u8;
            let bc = chunk.len() as u8;

            let mut sum = bc.wrapping_add(ah).wrapping_add(al); // record type 00
            for b in chunk {
                sum = sum.wrapping_add(*b);
            }
            let ck = (!sum).wrapping_add(1);
            let data_hex: String = chunk.iter().map(|b| format!("{b:02X}")).collect();
            lines.push(format!(":{bc:02X}{offset:04X}00{data_hex}{ck:02X}"));
        }
    }

    lines.push(":00000001FF".to_string());
    lines.join("\r\n") + "\r\n"
}

// ─── SREC writer ─────────────────────────────────────────────────────────────

/// Serialise a slice of records to Motorola S-record text.
/// The address width (S1/S2/S3) is chosen automatically from the highest address.
pub fn write_srec(records: &[RecordData]) -> String {
    // Determine address width needed.
    let max_addr = records
        .iter()
        .filter(|r| is_data_record(&r.record_type))
        .map(|r| r.address + r.data.len().saturating_sub(1) as u32)
        .max()
        .unwrap_or(0);

    let addr_len: usize = if max_addr <= 0xFFFF {
        2
    } else if max_addr <= 0xFF_FFFF {
        3
    } else {
        4
    };
    let data_type = match addr_len { 2 => "S1", 3 => "S2", _ => "S3" };
    let end_type  = match addr_len { 2 => "S9", 3 => "S8", _ => "S7" };

    let mut lines: Vec<String> = Vec::new();

    // S0 header.
    {
        let hdr = b"hex-studio";
        let bc = (2 + hdr.len() + 1) as u8; // 2-byte addr + data + checksum
        let mut sum = bc; // address bytes are both 0x00
        for b in hdr {
            sum = sum.wrapping_add(*b);
        }
        let ck = !sum;
        let hdr_hex: String = hdr.iter().map(|b| format!("{b:02X}")).collect();
        lines.push(format!("S0{bc:02X}0000{hdr_hex}{ck:02X}"));
    }

    let mut record_count: u32 = 0;

    for rec in records {
        if !is_data_record(&rec.record_type) {
            continue;
        }

        for (i, chunk) in rec.data.chunks(32).enumerate() {
            let addr = rec.address + (i * 32) as u32;
            let bc = (addr_len + chunk.len() + 1) as u8; // addr + data + checksum

            let mut sum = bc;
            // Address bytes, big-endian, addr_len bytes.
            for byte_idx in (0..addr_len).rev() {
                let b = ((addr >> (byte_idx * 8)) & 0xFF) as u8;
                sum = sum.wrapping_add(b);
            }
            for b in chunk {
                sum = sum.wrapping_add(*b);
            }
            let ck = !sum;

            let addr_hex = match addr_len {
                2 => format!("{addr:04X}"),
                3 => format!("{addr:06X}"),
                _ => format!("{addr:08X}"),
            };
            let data_hex: String = chunk.iter().map(|b| format!("{b:02X}")).collect();
            lines.push(format!("{data_type}{bc:02X}{addr_hex}{data_hex}{ck:02X}"));
            record_count += 1;
        }
    }

    // S5 record count (only when count fits in 16 bits).
    if record_count <= 0xFFFF {
        let hi = ((record_count >> 8) & 0xFF) as u8;
        let lo = (record_count & 0xFF) as u8;
        let bc = 0x03u8;
        let sum = bc.wrapping_add(hi).wrapping_add(lo);
        let ck = !sum;
        lines.push(format!("S5{bc:02X}{hi:02X}{lo:02X}{ck:02X}"));
    }

    // End record (execution start address = 0).
    {
        let bc = (addr_len + 1) as u8;
        let mut sum = bc; // all address bytes are 0x00
        for _ in 0..addr_len {
            sum = sum.wrapping_add(0x00);
        }
        let ck = !sum;
        let addr_hex = match addr_len {
            2 => "0000".to_string(),
            3 => "000000".to_string(),
            _ => "00000000".to_string(),
        };
        lines.push(format!("{end_type}{bc:02X}{addr_hex}{ck:02X}"));
    }

    lines.join("\r\n") + "\r\n"
}

/// Read an entire file into memory, returning raw bytes.
pub fn read_file(path: &str) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("failed to read '{path}': {e}"))
}

/// Read a text file (UTF-8), returning the string content.
pub fn read_text_file(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("failed to read text file '{path}': {e}"))
}

/// Write raw bytes to a file, overwriting if it exists.
pub fn write_file(path: &str, data: &[u8]) -> Result<(), String> {
    fs::write(path, data).map_err(|e| format!("failed to write '{path}': {e}"))
}

/// Return a file's size in bytes without reading its contents.
pub fn file_size(path: &str) -> Result<u64, String> {
    let meta = fs::metadata(path).map_err(|e| format!("failed to stat '{path}': {e}"))?;
    Ok(meta.len())
}

/// Flatten all data records into a contiguous binary blob, filling gaps with
/// `fill_byte`, and write it to `path`. Returns the number of bytes written.
pub fn write_binary(records: &[RecordData], path: &str, fill_byte: u8) -> Result<u64, String> {
    let mut min_addr = u64::MAX;
    let mut max_addr = 0u64;

    for rec in records {
        if !is_data_record(&rec.record_type) || rec.data.is_empty() {
            continue;
        }
        let start = rec.address as u64;
        let end   = start + rec.data.len() as u64;
        if start < min_addr { min_addr = start; }
        if end   > max_addr { max_addr = end;   }
    }

    if min_addr == u64::MAX {
        return Err("No data records to export".into());
    }

    let size = (max_addr - min_addr) as usize;
    let mut buf = vec![fill_byte; size];

    for rec in records {
        if !is_data_record(&rec.record_type) || rec.data.is_empty() {
            continue;
        }
        let offset = (rec.address as u64 - min_addr) as usize;
        buf[offset..offset + rec.data.len()].copy_from_slice(&rec.data);
    }

    write_file(path, &buf)?;
    Ok(size as u64)
}

/// Detect the file format based on extension and/or magic bytes.
/// Returns "ihex", "srec", "binary", or "unknown".
pub fn detect_format(path: &str) -> String {
    let p = Path::new(path);
    match p.extension().and_then(|e| e.to_str()) {
        Some("hex") | Some("ihex") => return "ihex".into(),
        Some("srec") | Some("mot") | Some("s19") | Some("s28") | Some("s37") => {
            return "srec".into()
        }
        Some("bin") => return "binary".into(),
        _ => {}
    }

    // Fall back to magic-byte sniffing (read first 2 bytes)
    if let Ok(mut f) = fs::File::open(path) {
        use std::io::Read;
        let mut buf = [0u8; 2];
        if f.read_exact(&mut buf).is_ok() {
            if buf[0] == b':' {
                return "ihex".into();
            }
            if buf[0] == b'S' && buf[1].is_ascii_digit() {
                return "srec".into();
            }
        }
    }

    "unknown".into()
}
