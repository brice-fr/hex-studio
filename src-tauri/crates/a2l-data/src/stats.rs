// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

//! Coverage of the image by the A2L description.
//!
//! Extents are merged before being counted: a COM_AXIS `AXIS_PTS` object is
//! referenced by several curves, and objects can legitimately overlap, so
//! summing raw sizes would over-report described bytes — sometimes past the
//! size of the image itself.

use crate::db::A2lDatabase;
use crate::decode::presence_of;
use crate::model::{ByteSource, Category, CoverageStats, Presence};

/// Merge a list of `[start, end)` ranges into disjoint ascending ranges.
fn merge(mut ranges: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    if ranges.is_empty() {
        return ranges;
    }
    ranges.sort_unstable();
    let mut out: Vec<(u64, u64)> = Vec::with_capacity(ranges.len());
    let mut cur = ranges[0];
    for r in ranges.into_iter().skip(1) {
        if r.0 <= cur.1 {
            // Overlapping or touching — extend the open range.
            cur.1 = cur.1.max(r.1);
        } else {
            out.push(cur);
            cur = r;
        }
    }
    out.push(cur);
    out
}

/// Compute coverage statistics over every object the A2L describes.
pub fn compute(
    db: &A2lDatabase,
    src: &dyn ByteSource,
    include_measurements: bool,
) -> CoverageStats {
    let mut s = CoverageStats {
        image_bytes: src.total_bytes(),
        ..Default::default()
    };

    let mut extents: Vec<(u64, u64)> = Vec::new();

    for (name, kind) in db.object_names(include_measurements) {
        let Some(plan) = db.plan(&name, kind) else {
            continue;
        };
        s.total_objects += 1;
        match plan.category {
            Category::Scalar => s.scalars += 1,
            Category::Curve => s.curves += 1,
            Category::Ascii => s.strings += 1,
            Category::Virtual => s.virtuals += 1,
            Category::Unsupported => s.unsupported += 1,
        }

        // A computed parameter occupies no image bytes and its declared address
        // is a placeholder — every VIRTUAL_CHARACTERISTIC in the demo file says
        // 0x0. Counting it as absent would overstate what is missing, and
        // taking its extent would credit the description with bytes at address
        // zero that it does not describe at all.
        if plan.category == Category::Virtual {
            continue;
        }

        match presence_of(src, &plan) {
            Presence::Full => s.present_full += 1,
            Presence::Partial => s.present_partial += 1,
            Presence::Absent => s.absent += 1,
            // Extent unknown, so it can be neither counted as missing nor
            // credited as described.
            Presence::Unknown => s.presence_unknown += 1,
        }
        let size = plan.byte_size();
        if size > 0 {
            let start = u64::from(plan.address);
            extents.push((start, start + u64::from(size)));
        }
    }

    let merged = merge(extents);
    s.described_bytes = merged.iter().map(|(a, b)| b - a).sum();

    // Of the described span, how much is actually in the image. Counted per
    // merged range so overlaps are not double counted.
    s.described_present_bytes = merged
        .iter()
        .map(|(a, b)| {
            let len = (b - a).min(u64::from(u32::MAX));
            u64::from(src.present_count(*a as u32, len as u32))
        })
        .sum();

    s.undescribed_bytes = s.image_bytes.saturating_sub(s.described_present_bytes);
    s.coverage_pct = if s.image_bytes == 0 {
        0.0
    } else {
        (s.described_present_bytes as f64 / s.image_bytes as f64) * 100.0
    };

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_joins_overlapping_ranges() {
        assert_eq!(merge(vec![(0, 10), (5, 20)]), vec![(0, 20)]);
    }

    #[test]
    fn merge_joins_touching_ranges() {
        assert_eq!(merge(vec![(0, 10), (10, 20)]), vec![(0, 20)]);
    }

    #[test]
    fn merge_keeps_disjoint_ranges_apart() {
        assert_eq!(merge(vec![(20, 30), (0, 10)]), vec![(0, 10), (20, 30)]);
    }

    #[test]
    fn merge_handles_full_containment() {
        assert_eq!(merge(vec![(0, 100), (10, 20)]), vec![(0, 100)]);
    }

    #[test]
    fn merge_of_nothing_is_nothing() {
        assert!(merge(vec![]).is_empty());
    }

    /// Two objects sharing an axis must not have those bytes counted twice.
    #[test]
    fn overlapping_extents_are_counted_once() {
        let merged = merge(vec![(0x1000, 0x1010), (0x1000, 0x1010), (0x1008, 0x1020)]);
        let total: u64 = merged.iter().map(|(a, b)| b - a).sum();
        assert_eq!(total, 0x20);
    }
}
