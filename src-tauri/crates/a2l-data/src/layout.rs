// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

//! Turning a RECORD_LAYOUT into concrete byte offsets.
//!
//! Fields carry a *position* number that fixes their order in memory; the
//! layout is rebuilt by walking positions in ascending order and aligning each
//! field to the border its datatype requires.
//!
//! Alignment padding is real and must be applied. The demo file's
//! `RL.CURVE.SWORD.SBYTE.DECR` is the cautionary case: a UBYTE count plus eight
//! SBYTE axis points end at offset 9, but the SWORD function values start at
//! offset 10, not 9. Skipping the pad byte decodes every function value from
//! one byte too early and yields values far outside the declared limits.

use a2lfile::{DataType, IndexOrder, ModCommon, RecordLayout};

/// Size in bytes of an A2L datatype.
pub fn datatype_size(dt: DataType) -> u32 {
    match dt {
        DataType::Ubyte | DataType::Sbyte => 1,
        DataType::Uword | DataType::Sword | DataType::Float16Ieee => 2,
        DataType::Ulong | DataType::Slong | DataType::Float32Ieee => 4,
        DataType::AUint64 | DataType::AInt64 | DataType::Float64Ieee => 8,
    }
}

/// The A2L keyword for a datatype, for display.
pub fn datatype_name(dt: DataType) -> &'static str {
    match dt {
        DataType::Ubyte => "UBYTE",
        DataType::Sbyte => "SBYTE",
        DataType::Uword => "UWORD",
        DataType::Sword => "SWORD",
        DataType::Ulong => "ULONG",
        DataType::Slong => "SLONG",
        DataType::AUint64 => "A_UINT64",
        DataType::AInt64 => "A_INT64",
        DataType::Float16Ieee => "FLOAT16_IEEE",
        DataType::Float32Ieee => "FLOAT32_IEEE",
        DataType::Float64Ieee => "FLOAT64_IEEE",
    }
}

/// True for the signed integer types.
pub fn is_signed(dt: DataType) -> bool {
    matches!(dt, DataType::Sbyte | DataType::Sword | DataType::Slong | DataType::AInt64)
}

/// True for the IEEE float types.
pub fn is_float(dt: DataType) -> bool {
    matches!(
        dt,
        DataType::Float16Ieee | DataType::Float32Ieee | DataType::Float64Ieee
    )
}

/// Alignment borders per datatype class, from MOD_COMMON and optionally
/// overridden by a RECORD_LAYOUT.
#[derive(Debug, Clone, Copy)]
pub struct Alignments {
    pub byte: u32,
    pub word: u32,
    pub long: u32,
    pub int64: u32,
    pub float16: u32,
    pub float32: u32,
    pub float64: u32,
}

impl Default for Alignments {
    /// Absent an explicit declaration, each type aligns to its natural size.
    fn default() -> Self {
        Alignments {
            byte: 1,
            word: 2,
            long: 4,
            int64: 8,
            float16: 2,
            float32: 4,
            float64: 8,
        }
    }
}

impl Alignments {
    pub fn from_mod_common(mc: Option<&ModCommon>) -> Self {
        let mut a = Alignments::default();
        if let Some(mc) = mc {
            if let Some(v) = &mc.alignment_byte {
                a.byte = v.alignment_border as u32;
            }
            if let Some(v) = &mc.alignment_word {
                a.word = v.alignment_border as u32;
            }
            if let Some(v) = &mc.alignment_long {
                a.long = v.alignment_border as u32;
            }
            if let Some(v) = &mc.alignment_int64 {
                a.int64 = v.alignment_border as u32;
            }
            if let Some(v) = &mc.alignment_float16_ieee {
                a.float16 = v.alignment_border as u32;
            }
            if let Some(v) = &mc.alignment_float32_ieee {
                a.float32 = v.alignment_border as u32;
            }
            if let Some(v) = &mc.alignment_float64_ieee {
                a.float64 = v.alignment_border as u32;
            }
        }
        a
    }

    /// A RECORD_LAYOUT may restate any alignment, taking precedence locally.
    pub fn overridden_by(mut self, rl: &RecordLayout) -> Self {
        if let Some(v) = &rl.alignment_byte {
            self.byte = v.alignment_border as u32;
        }
        if let Some(v) = &rl.alignment_word {
            self.word = v.alignment_border as u32;
        }
        if let Some(v) = &rl.alignment_long {
            self.long = v.alignment_border as u32;
        }
        if let Some(v) = &rl.alignment_int64 {
            self.int64 = v.alignment_border as u32;
        }
        if let Some(v) = &rl.alignment_float16_ieee {
            self.float16 = v.alignment_border as u32;
        }
        if let Some(v) = &rl.alignment_float32_ieee {
            self.float32 = v.alignment_border as u32;
        }
        if let Some(v) = &rl.alignment_float64_ieee {
            self.float64 = v.alignment_border as u32;
        }
        self
    }

    /// The border a field of this datatype must start on.
    pub fn border(&self, dt: DataType) -> u32 {
        match dt {
            DataType::Ubyte | DataType::Sbyte => self.byte,
            DataType::Uword | DataType::Sword => self.word,
            DataType::Ulong | DataType::Slong => self.long,
            DataType::AUint64 | DataType::AInt64 => self.int64,
            DataType::Float16Ieee => self.float16,
            DataType::Float32Ieee => self.float32,
            DataType::Float64Ieee => self.float64,
        }
        .max(1)
    }
}

/// One resolved field: where it starts, what it holds, how many elements.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub offset: u32,
    pub datatype: DataType,
    pub count: u32,
}

impl Field {
    pub fn size(&self) -> u32 {
        datatype_size(self.datatype) * self.count
    }
}

/// A RECORD_LAYOUT resolved against a concrete point count.
#[derive(Debug, Clone, Default)]
pub struct ResolvedLayout {
    pub total_size: u32,
    /// Where the current point count is stored, when the layout has one.
    pub no_axis_pts: Option<Field>,
    /// The axis breakpoints, when stored inside this record.
    pub axis_pts: Option<Field>,
    /// True when the axis is stored highest-value-first.
    pub axis_index_decr: bool,
    /// The function values.
    pub fnc: Option<Field>,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    NoAxisPts,
    AxisPts,
    Fnc,
}

fn align_up(offset: u32, border: u32) -> u32 {
    if border <= 1 {
        return offset;
    }
    offset.div_ceil(border) * border
}

/// Resolve the X-dimension fields of a record layout for `n_points` points.
///
/// Only the fields this milestone understands take part: `NO_AXIS_PTS_X`,
/// `AXIS_PTS_X` and `FNC_VALUES`. Higher dimensions are handled by the caller
/// classifying the object as unsupported before it gets here.
pub fn resolve(rl: &RecordLayout, aligns: &Alignments, n_points: u32) -> ResolvedLayout {
    let aligns = aligns.overridden_by(rl);

    // Gather the fields we support, tagged with their declared position.
    let mut items: Vec<(u16, Kind, DataType, u32)> = Vec::new();
    if let Some(f) = &rl.no_axis_pts_x {
        items.push((f.position, Kind::NoAxisPts, f.datatype, 1));
    }
    if let Some(f) = &rl.axis_pts_x {
        items.push((f.position, Kind::AxisPts, f.datatype, n_points));
    }
    if let Some(f) = &rl.fnc_values {
        items.push((f.position, Kind::Fnc, f.datatype, n_points));
    }
    items.sort_by_key(|(pos, _, _, _)| *pos);

    let mut out = ResolvedLayout {
        axis_index_decr: rl
            .axis_pts_x
            .as_ref()
            .map(|f| f.index_incr == IndexOrder::IndexDecr)
            .unwrap_or(false),
        ..Default::default()
    };

    let mut offset = 0u32;
    for (_, kind, datatype, count) in items {
        offset = align_up(offset, aligns.border(datatype));
        let field = Field {
            offset,
            datatype,
            count,
        };
        offset += field.size();
        match kind {
            Kind::NoAxisPts => out.no_axis_pts = Some(field),
            Kind::AxisPts => out.axis_pts = Some(field),
            Kind::Fnc => out.fnc = Some(field),
        }
    }
    out.total_size = offset;
    out
}

/// True when the record layout uses any field beyond the X dimension, i.e. the
/// object is at least two-dimensional.
pub fn is_multi_dimensional(rl: &RecordLayout) -> bool {
    rl.axis_pts_y.is_some()
        || rl.axis_pts_z.is_some()
        || rl.axis_pts_4.is_some()
        || rl.axis_pts_5.is_some()
        || rl.no_axis_pts_y.is_some()
        || rl.no_axis_pts_z.is_some()
        || rl.no_axis_pts_4.is_some()
        || rl.no_axis_pts_5.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_up_rounds_to_the_border() {
        assert_eq!(align_up(9, 2), 10);
        assert_eq!(align_up(10, 2), 10);
        assert_eq!(align_up(9, 1), 9);
        assert_eq!(align_up(9, 4), 12);
        assert_eq!(align_up(0, 8), 0);
    }

    #[test]
    fn datatype_sizes_are_right() {
        assert_eq!(datatype_size(DataType::Ubyte), 1);
        assert_eq!(datatype_size(DataType::Sword), 2);
        assert_eq!(datatype_size(DataType::Float32Ieee), 4);
        assert_eq!(datatype_size(DataType::Float64Ieee), 8);
    }

    /// The layout that motivated applying alignment at all: a UBYTE count and
    /// eight SBYTE axis points end at offset 9, but the SWORD function values
    /// must start at offset 10.
    #[test]
    fn curve_layout_pads_before_word_aligned_function_values() {
        let mut rl = RecordLayout::new("RL.CURVE.SWORD.SBYTE.DECR".to_string());
        rl.no_axis_pts_x = Some(a2lfile::NoAxisPtsDim::new(1, DataType::Ubyte));
        rl.axis_pts_x = Some(a2lfile::AxisPtsDim::new(
            2,
            DataType::Sbyte,
            IndexOrder::IndexDecr,
            a2lfile::AddrType::Direct,
        ));
        rl.fnc_values = Some(a2lfile::FncValues::new(
            3,
            DataType::Sword,
            a2lfile::IndexMode::RowDir,
            a2lfile::AddrType::Direct,
        ));

        let resolved = resolve(&rl, &Alignments::default(), 8);

        let count = resolved.no_axis_pts.expect("count field");
        assert_eq!(count.offset, 0);

        let axis = resolved.axis_pts.expect("axis field");
        assert_eq!(axis.offset, 1);
        assert_eq!(axis.size(), 8);

        let fnc = resolved.fnc.expect("function values");
        assert_eq!(fnc.offset, 10, "one pad byte at offset 9");
        assert_eq!(fnc.size(), 16);

        assert_eq!(resolved.total_size, 26);
        assert!(resolved.axis_index_decr);
    }

    #[test]
    fn scalar_layout_is_a_single_field_at_offset_zero() {
        let mut rl = RecordLayout::new("RL.FNC.UBYTE.ROW_DIR".to_string());
        rl.fnc_values = Some(a2lfile::FncValues::new(
            1,
            DataType::Ubyte,
            a2lfile::IndexMode::RowDir,
            a2lfile::AddrType::Direct,
        ));

        let resolved = resolve(&rl, &Alignments::default(), 1);
        let fnc = resolved.fnc.expect("function values");
        assert_eq!(fnc.offset, 0);
        assert_eq!(resolved.total_size, 1);
        assert!(resolved.axis_pts.is_none());
    }

    #[test]
    fn positions_drive_field_order_not_declaration_order() {
        // Declare FNC first but give it the later position; it must still land last.
        let mut rl = RecordLayout::new("RL.REORDERED".to_string());
        rl.fnc_values = Some(a2lfile::FncValues::new(
            2,
            DataType::Ubyte,
            a2lfile::IndexMode::RowDir,
            a2lfile::AddrType::Direct,
        ));
        rl.no_axis_pts_x = Some(a2lfile::NoAxisPtsDim::new(1, DataType::Ubyte));

        let resolved = resolve(&rl, &Alignments::default(), 4);
        assert_eq!(resolved.no_axis_pts.unwrap().offset, 0);
        assert_eq!(resolved.fnc.unwrap().offset, 1);
    }
}
