// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

//! Tauri bridge to the `a2l-data` crate.
//!
//! The parsed A2L lives in managed state because real descriptions run to
//! hundreds of thousands of lines — parsing per call would be untenable.
//!
//! `a2l_encode_*` deliberately returns bytes instead of writing them. The
//! frontend applies them through its existing edit path, so an A2L edit shares
//! the hex editor's undo history and modified flag rather than opening a second,
//! parallel way to mutate the image.

use std::sync::Mutex;

use a2l_data::model::ByteSource;
use a2l_data::{
    decode, encode, stats, A2lDatabase, A2lSummary, CoverageStats, EncodedWrite, ParamDetail,
    ParamRow,
};

use crate::file_operations::RecordData;

/// Holds the currently loaded A2L description, if any.
pub struct A2lState(pub Mutex<Option<A2lDatabase>>);

impl A2lState {
    pub fn new() -> Self {
        A2lState(Mutex::new(None))
    }
}

impl Default for A2lState {
    fn default() -> Self {
        Self::new()
    }
}

/// The firmware image as merged, sorted, non-overlapping segments.
///
/// A byte map would be simpler but allocates an entry per byte; firmware images
/// run to hundreds of kilobytes and this is rebuilt on every decode pass.
/// Segments plus a binary search keep it proportional to record count instead.
pub struct RecordImage {
    segments: Vec<(u32, Vec<u8>)>,
    total: u64,
}

fn is_data_record(rt: &str) -> bool {
    matches!(rt, "Data" | "S1" | "S2" | "S3")
}

impl RecordImage {
    pub fn from_records(records: &[RecordData]) -> Self {
        let mut sorted: Vec<&RecordData> = records
            .iter()
            .filter(|r| is_data_record(&r.record_type) && !r.data.is_empty())
            .collect();
        sorted.sort_by_key(|r| r.address);

        let mut segments: Vec<(u32, Vec<u8>)> = Vec::new();
        for rec in sorted {
            if let Some(last) = segments.last_mut() {
                let last_end = u64::from(last.0) + last.1.len() as u64;
                if u64::from(rec.address) <= last_end {
                    // Contiguous or overlapping: extend, letting later records
                    // win on any overlap — the same precedence the frontend's
                    // overwrite mode uses.
                    let offset = (u64::from(rec.address) - u64::from(last.0)) as usize;
                    for (i, b) in rec.data.iter().enumerate() {
                        match last.1.get_mut(offset + i) {
                            Some(slot) => *slot = *b,
                            None => last.1.push(*b),
                        }
                    }
                    continue;
                }
            }
            segments.push((rec.address, rec.data.clone()));
        }

        let total = segments.iter().map(|(_, d)| d.len() as u64).sum();
        RecordImage { segments, total }
    }

    /// Index of the last segment starting at or before `addr`.
    fn segment_at(&self, addr: u32) -> Option<usize> {
        match self.segments.binary_search_by_key(&addr, |(a, _)| *a) {
            Ok(i) => Some(i),
            Err(0) => None,
            Err(i) => Some(i - 1),
        }
    }
}

impl ByteSource for RecordImage {
    fn read(&self, addr: u32, len: u32) -> Option<Vec<u8>> {
        if len == 0 {
            return Some(Vec::new());
        }
        let i = self.segment_at(addr)?;
        let (seg_addr, data) = &self.segments[i];
        let offset = (u64::from(addr) - u64::from(*seg_addr)) as usize;
        let end = offset.checked_add(len as usize)?;
        // A read that runs past this segment crosses a gap, so it has no value.
        data.get(offset..end).map(|s| s.to_vec())
    }

    fn present_count(&self, addr: u32, len: u32) -> u32 {
        if len == 0 {
            return 0;
        }
        let lo = u64::from(addr);
        let hi = lo + u64::from(len);
        let mut count: u64 = 0;
        // Segments are sorted, so start at the one covering `addr` and walk
        // forward until past the requested range.
        let start = self.segment_at(addr).unwrap_or(0);
        for (seg_addr, data) in &self.segments[start..] {
            let s_lo = u64::from(*seg_addr);
            if s_lo >= hi {
                break;
            }
            let s_hi = s_lo + data.len() as u64;
            if s_hi <= lo {
                continue;
            }
            count += s_hi.min(hi) - s_lo.max(lo);
        }
        count as u32
    }

    fn total_bytes(&self) -> u64 {
        self.total
    }
}

/// Run `f` against the loaded description, or report that none is loaded.
fn with_db<T>(
    state: &tauri::State<A2lState>,
    f: impl FnOnce(&A2lDatabase) -> Result<T, String>,
) -> Result<T, String> {
    let guard = state
        .0
        .lock()
        .map_err(|_| "A2L state is poisoned".to_string())?;
    match guard.as_ref() {
        Some(db) => f(db),
        None => Err("no A2L file is loaded".to_string()),
    }
}

/// Parse an A2L file and keep it for subsequent calls.
#[tauri::command]
pub fn a2l_load(path: String, state: tauri::State<A2lState>) -> Result<A2lSummary, String> {
    let db = A2lDatabase::load(&path)?;
    let summary = db.summary().clone();
    *state
        .0
        .lock()
        .map_err(|_| "A2L state is poisoned".to_string())? = Some(db);
    Ok(summary)
}

/// Drop the loaded description.
#[tauri::command]
pub fn a2l_unload(state: tauri::State<A2lState>) -> Result<(), String> {
    *state
        .0
        .lock()
        .map_err(|_| "A2L state is poisoned".to_string())? = None;
    Ok(())
}

/// Decode every object into a table row.
#[tauri::command]
pub fn a2l_list(
    records: Vec<RecordData>,
    include_measurements: bool,
    state: tauri::State<A2lState>,
) -> Result<Vec<ParamRow>, String> {
    let image = RecordImage::from_records(&records);
    with_db(&state, |db| {
        Ok(decode::list_rows(db, &image, include_measurements))
    })
}

/// Full axis and value arrays for one 1D object.
#[tauri::command]
pub fn a2l_detail(
    name: String,
    records: Vec<RecordData>,
    state: tauri::State<A2lState>,
) -> Result<ParamDetail, String> {
    let image = RecordImage::from_records(&records);
    with_db(&state, |db| {
        decode::detail_for(db, &image, &name)
            .ok_or_else(|| format!("'{name}' not found in the A2L description"))
    })
}

/// Coverage of the image by the description.
#[tauri::command]
pub fn a2l_stats(
    records: Vec<RecordData>,
    include_measurements: bool,
    state: tauri::State<A2lState>,
) -> Result<CoverageStats, String> {
    let image = RecordImage::from_records(&records);
    with_db(&state, |db| {
        Ok(stats::compute(db, &image, include_measurements))
    })
}

/// Encode a numeric physical value into bytes for the caller to apply.
#[tauri::command]
pub fn a2l_encode_value(
    name: String,
    phys: f64,
    state: tauri::State<A2lState>,
) -> Result<EncodedWrite, String> {
    with_db(&state, |db| encode::encode_scalar(db, &name, phys))
}

/// Encode a textual value — an enum label or an ASCII string — into bytes.
#[tauri::command]
pub fn a2l_encode_text(
    name: String,
    text: String,
    state: tauri::State<A2lState>,
) -> Result<EncodedWrite, String> {
    with_db(&state, |db| encode::encode_text(db, &name, &text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(address: u32, data: &[u8]) -> RecordData {
        RecordData {
            record_type: "Data".to_string(),
            address,
            data: data.to_vec(),
        }
    }

    #[test]
    fn reads_within_a_segment() {
        let img = RecordImage::from_records(&[rec(0x100, &[1, 2, 3, 4])]);
        assert_eq!(img.read(0x100, 2), Some(vec![1, 2]));
        assert_eq!(img.read(0x102, 2), Some(vec![3, 4]));
        assert_eq!(img.total_bytes(), 4);
    }

    #[test]
    fn refuses_a_read_that_crosses_a_gap() {
        let img = RecordImage::from_records(&[rec(0x100, &[1, 2]), rec(0x200, &[3, 4])]);
        assert_eq!(img.read(0x100, 4), None);
        assert_eq!(img.read(0x0FF, 1), None);
        assert_eq!(img.read(0x200, 2), Some(vec![3, 4]));
    }

    #[test]
    fn merges_contiguous_records() {
        let img = RecordImage::from_records(&[rec(0x100, &[1, 2]), rec(0x102, &[3, 4])]);
        assert_eq!(img.segments.len(), 1);
        assert_eq!(img.read(0x100, 4), Some(vec![1, 2, 3, 4]));
    }

    #[test]
    fn later_records_win_on_overlap() {
        let img = RecordImage::from_records(&[rec(0x100, &[1, 2, 3]), rec(0x101, &[9, 9])]);
        assert_eq!(img.read(0x100, 3), Some(vec![1, 9, 9]));
    }

    #[test]
    fn sorts_records_given_out_of_order() {
        let img = RecordImage::from_records(&[rec(0x102, &[3, 4]), rec(0x100, &[1, 2])]);
        assert_eq!(img.read(0x100, 4), Some(vec![1, 2, 3, 4]));
    }

    #[test]
    fn counts_partial_presence() {
        let img = RecordImage::from_records(&[rec(0x100, &[1, 2])]);
        assert_eq!(img.present_count(0x100, 2), 2);
        // Half the requested range exists.
        assert_eq!(img.present_count(0x101, 4), 1);
        assert_eq!(img.present_count(0x200, 4), 0);
    }

    #[test]
    fn counts_presence_across_several_segments() {
        let img = RecordImage::from_records(&[rec(0x100, &[1, 2]), rec(0x104, &[3, 4])]);
        // 0x100..0x106 holds 2 bytes, a 2-byte gap, then 2 more.
        assert_eq!(img.present_count(0x100, 6), 4);
    }

    #[test]
    fn ignores_non_data_records() {
        let img = RecordImage::from_records(&[
            RecordData {
                record_type: "EndOfFile".to_string(),
                address: 0,
                data: vec![],
            },
            rec(0x100, &[1]),
        ]);
        assert_eq!(img.total_bytes(), 1);
    }

    #[test]
    fn handles_addresses_near_the_top_of_the_space() {
        // The demo A2L puts a segment at 0x7FFF0000; arithmetic must not wrap.
        let img = RecordImage::from_records(&[rec(0xFFFF_FFFC, &[1, 2, 3, 4])]);
        assert_eq!(img.read(0xFFFF_FFFC, 4), Some(vec![1, 2, 3, 4]));
        assert_eq!(img.present_count(0xFFFF_FFFC, 4), 4);
    }
}
