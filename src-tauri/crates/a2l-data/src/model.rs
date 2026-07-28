// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

//! Types that cross the IPC boundary, plus the `ByteSource` abstraction that
//! lets this crate stay ignorant of how the firmware image is stored.

use serde::{Deserialize, Serialize};

/// Read access to a sparse firmware image.
///
/// Implemented by the host over its own record representation; tests implement
/// it over a `BTreeMap`.
pub trait ByteSource {
    /// Read `len` bytes starting at `addr`.
    ///
    /// Returns `None` when *any* byte in the range is absent from the image —
    /// a partially present object cannot be decoded meaningfully.
    fn read(&self, addr: u32, len: u32) -> Option<Vec<u8>>;

    /// How many of the `len` bytes starting at `addr` are present.
    fn present_count(&self, addr: u32, len: u32) -> u32;

    /// Total number of bytes present in the whole image.
    fn total_bytes(&self) -> u64;
}

/// Which A2L block a row came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjKind {
    Characteristic,
    AxisPts,
    Measurement,
}

/// How much of an object's byte extent exists in the loaded image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    /// Every byte is present.
    Full,
    /// Some but not all bytes are present.
    Partial,
    /// No bytes are present.
    Absent,
    /// The object's extent could not be determined, so nothing can be said
    /// about the image. Distinct from `Absent`, which is a claim that the
    /// bytes are missing — reporting that for a layout we simply failed to
    /// resolve would blame the image for a gap in this crate.
    Unknown,
}

/// Display/handling category, driven by what this crate can actually decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Single value.
    Scalar,
    /// One-dimensional: curve, axis points, or value block.
    Curve,
    /// A fixed-width character array (A2L `ASCII`).
    Ascii,
    /// Two or more dimensions: `MAP`, `CUBOID`, `CUBE_4` or `CUBE_5`. Values
    /// are held flat in row-major order, with `dims` giving the shape.
    Map,
    /// A2L `VIRTUAL_CHARACTERISTIC`: computed from other parameters by a
    /// formula and never stored, so its declared address is a placeholder.
    Virtual,
    /// Recognised but not decodable: an unresolvable conversion or record
    /// layout, or a shape this crate does not model.
    Unsupported,
}

/// Summary returned right after an A2L file is parsed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2lSummary {
    pub path: String,
    pub project: String,
    pub module: String,
    pub asap2_version: Option<String>,
    pub characteristic_count: usize,
    pub axis_pts_count: usize,
    pub measurement_count: usize,
    pub compu_method_count: usize,
    pub record_layout_count: usize,
    /// Non-fatal parser diagnostics, capped by the caller.
    pub warnings: Vec<String>,
}

/// One row of the parameter table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamRow {
    pub name: String,
    pub description: String,
    pub kind: ObjKind,
    pub category: Category,
    pub address: u32,
    pub byte_size: u32,
    pub datatype: String,
    pub presence: Presence,
    pub unit: String,
    /// COMPU_METHOD name.
    pub conversion: String,
    /// COMPU_METHOD conversion type, e.g. `RAT_FUNC`.
    pub conversion_type: String,
    /// Raw value as hex, scalars only.
    pub raw_hex: Option<String>,
    /// What to show in the physical column: a number, an enum label, or a dash.
    pub display: String,
    /// Numeric physical value when the value is numeric — the edit field's source.
    pub phys_num: Option<f64>,
    /// Physical increment of one raw LSB, so a slider can only land on values
    /// the field is actually able to store. `None` when no sensible step exists.
    pub phys_step: Option<f64>,
    /// For 1D objects, the numeric extent behind the summary in `display`.
    /// Exposed separately so the frontend can re-render at a different decimal
    /// precision without a full re-decode.
    pub phys_min: Option<f64>,
    pub phys_max: Option<f64>,
    /// Choices for a TAB_VERB parameter, so the UI can offer a dropdown.
    pub enum_options: Option<Vec<String>>,
    /// The VIRTUAL_CHARACTERISTIC formula, for a computed parameter.
    pub formula: Option<String>,
    /// Parameters that formula reads, so the UI can link to them.
    pub depends_on: Option<Vec<String>>,
    /// Decoded text of an ASCII characteristic, up to the first NUL.
    pub text_value: Option<String>,
    /// Total bytes the character array occupies.
    pub text_capacity: Option<u32>,
    /// Longest string the field accepts. One byte short of the capacity when
    /// the array is used as a NUL-terminated C string, the full capacity when
    /// it is a fixed-width field with no terminator.
    pub text_max_len: Option<u32>,
    /// For 1D objects: point count.
    pub point_count: Option<u32>,
    /// Element counts per dimension, X first. Length 1 for anything flat, so
    /// `dims.len() > 1` is what marks a row as a map.
    pub dims: Vec<u32>,
    pub lower_limit: f64,
    pub upper_limit: f64,
    /// True when a physical value can be written back.
    pub editable: bool,
    /// Why the row is unsupported or non-editable.
    pub note: Option<String>,
}

/// One point of a 1D object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointValue {
    pub raw: f64,
    pub phys: f64,
    pub display: String,
}

/// One dimension's breakpoints, as presented.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisDetail {
    /// Breakpoints in presentation order.
    pub points: Vec<PointValue>,
    pub unit: String,
    /// `STD_AXIS`, `COM_AXIS`, `FIX_AXIS`, `RES_AXIS` or `CURVE_AXIS`.
    pub kind: String,
    /// The object holding these points, when they live elsewhere.
    pub reference: Option<String>,
    /// Whether these breakpoints can be written through this object.
    pub editable: bool,
    /// Every label a verbal conversion accepts, in table order. `None` for a
    /// numeric axis. A breakpoint chosen from this list is written by name,
    /// since a verbal conversion has no numeric inverse.
    pub enum_options: Option<Vec<String>>,
}

/// Full detail for one object, fetched on selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDetail {
    pub name: String,
    pub description: String,
    pub address: u32,
    pub byte_size: u32,
    /// Axis breakpoints of the X axis; empty for a value block with no axis.
    ///
    /// Retained alongside [`axes`](Self::axes) because the one-dimensional
    /// point table is driven by it directly; for a map it is `axes[0]`.
    pub axis: Vec<PointValue>,
    /// Function values, flat, in row-major presentation order — the first
    /// dimension varies fastest, matching how a CDFX writes them.
    pub values: Vec<PointValue>,
    /// Element counts per dimension, X first. `[n]` for a curve, `[nx, ny]`
    /// for a map. Its product is `values.len()`.
    pub dims: Vec<u32>,
    /// Every axis the object declares, X first. Empty for a value block.
    pub axes: Vec<AxisDetail>,
    pub axis_unit: String,
    pub value_unit: String,
    /// The AXIS_DESCR attribute keyword — `STD_AXIS`, `COM_AXIS`, `FIX_AXIS`,
    /// `RES_AXIS` or `CURVE_AXIS`. Empty when the object has no axis.
    pub axis_kind: String,
    /// The object this axis defers to, for COM_AXIS/RES_AXIS (an AXIS_PTS) and
    /// CURVE_AXIS (a characteristic). `None` when the axis is self-contained.
    pub axis_ref: Option<String>,
    /// Labels the function values accept, when their conversion is verbal.
    pub value_options: Option<Vec<String>>,
    /// Whether each column of the point table accepts edits. An axis stored in
    /// another object is edited there, and a computed one not at all.
    pub values_editable: bool,
    pub axis_editable: bool,
    /// Raw bytes of the whole object, for the byte preview.
    pub bytes: Vec<u8>,
}

/// Coverage of the image by the A2L description.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoverageStats {
    /// Bytes present in the image.
    pub image_bytes: u64,
    /// Bytes covered by at least one object extent, overlaps counted once.
    pub described_bytes: u64,
    /// Described bytes that are actually present in the image.
    pub described_present_bytes: u64,
    /// Image bytes no object describes.
    pub undescribed_bytes: u64,
    /// `described_present_bytes` as a percentage of `image_bytes`.
    pub coverage_pct: f64,
    pub total_objects: usize,
    pub scalars: usize,
    pub curves: usize,
    /// Two-or-more-dimensional objects: maps, cuboids and cubes.
    pub maps: usize,
    pub strings: usize,
    /// Computed parameters. Counted apart from the presence tallies below,
    /// which only describe objects that are meant to occupy image bytes.
    pub virtuals: usize,
    pub unsupported: usize,
    pub present_full: usize,
    pub present_partial: usize,
    pub absent: usize,
    /// Objects whose extent could not be resolved, so nothing is claimed about
    /// whether the image contains them.
    pub presence_unknown: usize,
}

/// Bytes to write, produced by encoding a physical value. Applying them is the
/// caller's job, so an A2L edit reuses the host's existing undo machinery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodedWrite {
    pub address: u32,
    pub bytes: Vec<u8>,
    /// Raw value actually stored after rounding and clamping.
    pub raw: f64,
    /// Physical value corresponding to `raw` — may differ from the request
    /// when the raw domain is coarser than the physical one.
    pub phys: f64,
}
