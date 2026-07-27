// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

//! The loaded A2L description, and resolution of a named object into
//! everything needed to read or write its bytes.

use a2lfile::{
    A2lObjectName, A2lFile, ByteOrderEnum, Characteristic, CharacteristicType, CompuMethod,
    ConversionType, DataType, Module, RecordLayout,
};

use crate::convert::Conversion;
use crate::layout::{self, Alignments, Field, ResolvedLayout};
use crate::model::{A2lSummary, Category, ObjKind};

/// Byte order reduced to what decoding actually needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

fn endian_of(e: ByteOrderEnum) -> Endian {
    match e {
        ByteOrderEnum::LittleEndian | ByteOrderEnum::MsbLast | ByteOrderEnum::MsbLastMswFirst => {
            Endian::Little
        }
        ByteOrderEnum::BigEndian | ByteOrderEnum::MsbFirst | ByteOrderEnum::MsbFirstMswLast => {
            Endian::Big
        }
    }
}

/// A COMPU_METHOD reduced for use, with its presentation metadata.
#[derive(Debug, Clone)]
pub struct ConvInfo {
    pub name: String,
    pub type_name: &'static str,
    pub conversion: Conversion,
    pub unit: String,
    /// The A2L FORMAT string, e.g. `%8.3`. Note A2L omits the conversion
    /// character that printf would require.
    pub format: String,
}

impl ConvInfo {
    /// The identity conversion, used when a COMPU_METHOD is `NO_COMPU_METHOD`
    /// or cannot be found.
    fn identity(name: &str) -> Self {
        ConvInfo {
            name: name.to_string(),
            type_name: "IDENTICAL",
            conversion: Conversion::Identical,
            unit: String::new(),
            format: String::new(),
        }
    }
}

/// Where a 1D object's axis breakpoints come from.
#[derive(Debug, Clone)]
pub enum AxisSource {
    /// No axis: a scalar, or a value block.
    None,
    /// Stored inside this object's own record (STD_AXIS).
    Internal,
    /// A shared AXIS_PTS object, named by AXIS_PTS_REF (COM_AXIS, RES_AXIS).
    AxisPts(String),
    /// Another characteristic's function values, named by CURVE_AXIS_REF
    /// (CURVE_AXIS). Note this is a different reference field from the one
    /// COM_AXIS uses, and points at a CHARACTERISTIC rather than an AXIS_PTS.
    CurveRef(String),
    /// Computed from FIX_AXIS_PAR / _DIST / _LIST — occupies no image bytes.
    Fixed(Vec<f64>),
}

impl AxisSource {
    /// The object this axis defers to, when it defers to one.
    pub fn reference(&self) -> Option<&str> {
        match self {
            AxisSource::AxisPts(n) | AxisSource::CurveRef(n) => Some(n),
            _ => None,
        }
    }
}

/// Split a flat presentation index into per-dimension subscripts, first
/// dimension varying fastest — the order a CDFX writes and a grid reads.
///
/// Returns `None` when the index does not fit the shape.
fn unpack(index: u32, dims: &[u32]) -> Option<Vec<u32>> {
    if dims.is_empty() || dims.contains(&0) {
        return None;
    }
    let mut rest = index;
    let mut subs = Vec::with_capacity(dims.len());
    for d in dims {
        subs.push(rest % d);
        rest /= d;
    }
    (rest == 0).then_some(subs)
}

/// Recombine subscripts into a storage slot.
///
/// `ROW_DIR` walks X fastest, which is the order the subscripts came apart in.
/// `COLUMN_DIR` walks Y fastest — and *only* swaps X with Y: any dimension
/// beyond the second stays stacked exactly as it was, because row versus column
/// is a statement about the plane, not about the whole array.
///
/// The demo file settles this. Its `ASAM.C.CUBOID.COLUMN_DIR` is a 2x3x4 twin
/// of `ASAM.C.CUBOID.ROW_DIR` holding the same 1..24, stored as
/// `1,3,5,2,4,6,7,9,11,…`. Swapping X and Y recovers 1..24; reversing the full
/// dimension order yields `1,13,4,16,11,…`, which is not the twin's content and
/// not what the shipped CDFX records.
fn pack(subs: &[u32], dims: &[u32], column_dir: bool) -> u32 {
    let mut order: Vec<usize> = (0..dims.len()).collect();
    if column_dir && dims.len() >= 2 {
        order.swap(0, 1);
    }
    let mut slot = 0;
    let mut stride = 1;
    for &d in &order {
        slot += subs[d] * stride;
        stride *= dims[d];
    }
    slot
}

/// One dimension of an object: where its breakpoints come from and how to
/// present them.
#[derive(Debug, Clone)]
pub struct AxisSpec {
    pub source: AxisSource,
    pub conv: Option<ConvInfo>,
    /// The AXIS_DESCR attribute keyword, for display.
    pub kind: &'static str,
    /// Breakpoints along this dimension.
    pub points: u32,
}

/// Everything needed to decode, encode or measure one A2L object.
#[derive(Debug, Clone)]
pub struct ObjectPlan {
    pub name: String,
    pub description: String,
    pub kind: ObjKind,
    pub category: Category,
    pub address: u32,
    pub layout: ResolvedLayout,
    pub conv: ConvInfo,
    /// One entry per declared AXIS_DESCR, X first. Empty for a scalar, a
    /// VAL_BLK, or a standalone AXIS_PTS object, none of which has an axis of
    /// its own.
    pub axes: Vec<AxisSpec>,
    /// A2L `BIT_MASK`: the bits of the stored word this object occupies.
    /// 0 means the whole word, which is also the default.
    pub bit_mask: u64,
    /// For a VIRTUAL_CHARACTERISTIC: the formula and the parameters it reads.
    pub virtual_formula: Option<String>,
    pub virtual_inputs: Vec<String>,
    /// Element counts per dimension, X first: `[1]` for a scalar, `[n]` for a
    /// curve or axis, `[nx, ny]` for a map. The function values span the
    /// product, and this is what indexes them.
    pub dims: Vec<u32>,
    /// Per-dimension INDEX_DECR, aligned with `dims`.
    ///
    /// An axis stored highest-first is presented ascending, and the function
    /// values sit alongside it element by element — so presentation order
    /// reverses *along that dimension only*. For a COM_AXIS the order belongs
    /// to the referenced AXIS_PTS object rather than to this record layout.
    pub dims_reversed: Vec<bool>,
    pub endian: Endian,
    pub lower_limit: f64,
    pub upper_limit: f64,
    /// Allocated point count — the record is sized for this many.
    pub declared_points: u32,
    /// A FORMAT on the object itself, which overrides the COMPU_METHOD's.
    pub format_override: Option<String>,
    pub note: Option<String>,
}

impl ObjectPlan {
    /// The effective FORMAT string for displaying values.
    pub fn format(&self) -> &str {
        self.format_override
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.conv.format)
    }
}

impl ObjectPlan {
    /// The X axis, which is the only one anything below a MAP has.
    pub fn axis(&self) -> &AxisSource {
        self.axes
            .first()
            .map(|a| &a.source)
            .unwrap_or(&AxisSource::None)
    }

    /// The X axis's conversion.
    pub fn axis_conv(&self) -> Option<&ConvInfo> {
        self.axes.first().and_then(|a| a.conv.as_ref())
    }

    /// The X axis's AXIS_DESCR keyword, empty when there is no axis.
    pub fn axis_kind(&self) -> &'static str {
        self.axes.first().map(|a| a.kind).unwrap_or("")
    }

    /// Whether presentation reverses along dimension `d`.
    pub fn dim_reversed(&self, d: usize) -> bool {
        self.dims_reversed.get(d).copied().unwrap_or(false)
    }

    /// Where the element shown at `index` actually lives in the record.
    ///
    /// Three things can put presentation order at odds with storage order, and
    /// all of them are subscript arithmetic once the flat index is unpacked:
    ///
    /// * `INDEX_DECR`, which stores an axis highest-first. The function values
    ///   sit alongside their breakpoints element by element, so an axis shown
    ///   ascending drags its values with it — but *only along that axis*.
    ///   Reversing the whole flat array happens to be right in one dimension
    ///   and is wrong in two.
    /// * `COLUMN_DIR`, which stores the grid transposed. A CDFX writes it row
    ///   by row, and the demo file holds the same 3x4 block twice, once each
    ///   way, to make the point: read linearly the `COLUMN_DIR` copy comes out
    ///   `1,4,7,10,2,…` where its `ROW_DIR` twin comes out `1,2,3,…`.
    /// * Both at once, which a map with a decreasing Y axis really does.
    ///
    /// `count` is the number of elements in the field being indexed, which for
    /// a rescale axis is not the same as the object's point count.
    pub fn storage_slot(&self, index: u32, count: u32) -> u32 {
        let dims = self.effective_dims(count);
        let Some(mut subs) = unpack(index, &dims) else {
            // Out of range for the declared shape; leave it to the caller's
            // bounds check rather than inventing a slot.
            return index;
        };
        for (d, s) in subs.iter_mut().enumerate() {
            if self.dim_reversed(d) {
                *s = dims[d] - 1 - *s;
            }
        }
        pack(&subs, &dims, self.layout.fnc_column_dir)
    }

    /// Where breakpoint `index` of dimension `d` lives in that axis's field.
    pub fn axis_slot(&self, d: usize, index: u32, count: u32) -> u32 {
        if self.dim_reversed(d) && count > 0 && index < count {
            count - 1 - index
        } else {
            index
        }
    }

    /// The grid to index, falling back to a flat run when the declared
    /// dimensions do not account for the field being addressed — a rescale
    /// axis, or an object whose MATRIX_DIM disagrees with its record layout.
    fn effective_dims(&self, count: u32) -> Vec<u32> {
        let declared: u32 = self.dims.iter().product();
        if !self.dims.is_empty() && declared == count {
            self.dims.clone()
        } else {
            vec![count]
        }
    }

    /// Total bytes the object occupies in the image.
    pub fn byte_size(&self) -> u32 {
        self.layout.total_size
    }

    /// The datatype shown in the table: the function values' type.
    pub fn datatype(&self) -> Option<DataType> {
        self.layout.fnc.map(|f| f.datatype)
    }

    /// The unit to display, which is empty for text.
    ///
    /// A2L requires every CHARACTERISTIC to name a COMPU_METHOD, including an
    /// ASCII one where no conversion is meaningful. Such a reference commonly
    /// points at a shared identity conversion — in the ASAM demo file every
    /// ASCII string inherits `CM.IDENTICAL`, which declares "hours". A
    /// character array is not a quantity, so that unit is an artefact of the
    /// reference rather than a property of the data.
    pub fn display_unit(&self) -> &str {
        match self.category {
            Category::Ascii => "",
            // A map's table cell carries its shape rather than a quantity, and
            // "4 × 5 hours" would read as nonsense. The unit still reaches the
            // detail pane through ParamDetail::value_unit.
            Category::Map => "",
            _ => &self.conv.unit,
        }
    }
}

/// A parsed A2L description plus the derived defaults decoding depends on.
pub struct A2lDatabase {
    file: A2lFile,
    module_index: usize,
    aligns: Alignments,
    default_endian: Endian,
    /// MOD_PAR SYSTEM_CONSTANTs that parse as numbers. Textual ones are
    /// omitted, so a formula referring to one fails rather than reading zero.
    system_constants: std::collections::HashMap<String, f64>,
    summary: A2lSummary,
}

impl A2lDatabase {
    /// Parse an A2L file from disk.
    ///
    /// Parsing is non-strict: real-world files routinely carry constructs a
    /// strict parser rejects, and a warning list is more useful than a refusal.
    pub fn load(path: &str) -> Result<Self, String> {
        let (file, errors) =
            a2lfile::load(path, None, false).map_err(|e| format!("cannot parse A2L: {e}"))?;

        if file.project.module.is_empty() {
            return Err("A2L file contains no MODULE".to_string());
        }

        let warnings: Vec<String> = errors.iter().take(50).map(|e| e.to_string()).collect();
        let module = &file.project.module[0];
        let aligns = Alignments::from_mod_common(module.mod_common.as_ref());
        let default_endian = module
            .mod_common
            .as_ref()
            .and_then(|mc| mc.byte_order.as_ref())
            .map(|b| endian_of(b.byte_order))
            .unwrap_or(Endian::Little);

        let summary = A2lSummary {
            path: path.to_string(),
            project: file.project.name.clone(),
            module: module.get_name().to_string(),
            asap2_version: file
                .asap2_version
                .as_ref()
                .map(|v| format!("{}.{}", v.version_no, v.upgrade_no)),
            characteristic_count: module.characteristic.len(),
            axis_pts_count: module.axis_pts.len(),
            measurement_count: module.measurement.len(),
            compu_method_count: module.compu_method.len(),
            record_layout_count: module.record_layout.len(),
            warnings,
        };

        let system_constants = module
            .mod_par
            .as_ref()
            .map(|mp| {
                mp.system_constant
                    .iter()
                    .filter_map(|sc| {
                        let name = system_constant_name(sc)?;
                        let value = sc.value.trim().parse::<f64>().ok()?;
                        Some((name, value))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(A2lDatabase {
            file,
            module_index: 0,
            aligns,
            default_endian,
            system_constants,
            summary,
        })
    }

    pub fn summary(&self) -> &A2lSummary {
        &self.summary
    }

    /// Numeric SYSTEM_CONSTANTs, for `sysc()` references in formulas.
    pub fn system_constants(&self) -> &std::collections::HashMap<String, f64> {
        &self.system_constants
    }

    pub fn module(&self) -> &Module {
        &self.file.project.module[self.module_index]
    }

    /// Build the usable form of a COMPU_METHOD, resolving any table reference.
    pub fn conversion_for(&self, name: &str) -> ConvInfo {
        if name.is_empty() || name == "NO_COMPU_METHOD" {
            return ConvInfo::identity(name);
        }
        let Some(cm) = self.module().compu_method.get(name) else {
            return ConvInfo::identity(name);
        };
        let conversion = self.build_conversion(cm);
        ConvInfo {
            name: name.to_string(),
            type_name: conversion_type_name(cm.conversion_type),
            conversion,
            unit: cm.unit.clone(),
            format: cm.format.clone(),
        }
    }

    fn build_conversion(&self, cm: &CompuMethod) -> Conversion {
        match cm.conversion_type {
            ConversionType::Identical => Conversion::Identical,

            ConversionType::Linear => match &cm.coeffs_linear {
                Some(c) => Conversion::Linear { a: c.a, b: c.b },
                None => Conversion::Unsupported("LINEAR without COEFFS_LINEAR".into()),
            },

            ConversionType::RatFunc => match &cm.coeffs {
                Some(c) => Conversion::RatFunc {
                    a: c.a,
                    b: c.b,
                    c: c.c,
                    d: c.d,
                    e: c.e,
                    f: c.f,
                },
                None => Conversion::Unsupported("RAT_FUNC without COEFFS".into()),
            },

            ConversionType::TabIntp | ConversionType::TabNointp => {
                let interpolate = cm.conversion_type == ConversionType::TabIntp;
                let Some(r) = &cm.compu_tab_ref else {
                    return Conversion::Unsupported("table conversion without COMPU_TAB_REF".into());
                };
                let Some(tab) = self.module().compu_tab.get(&r.conversion_table) else {
                    return Conversion::Unsupported(format!(
                        "COMPU_TAB '{}' not found",
                        r.conversion_table
                    ));
                };
                let mut pairs: Vec<(f64, f64)> =
                    tab.tab_entry.iter().map(|e| (e.in_val, e.out_val)).collect();
                pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
                Conversion::Tab {
                    pairs,
                    interpolate,
                    default: tab.default_value_numeric.as_ref().map(|d| d.display_value),
                }
            }

            ConversionType::TabVerb => {
                let Some(r) = &cm.compu_tab_ref else {
                    return Conversion::Unsupported("TAB_VERB without COMPU_TAB_REF".into());
                };
                // TAB_VERB may reference either a per-value COMPU_VTAB or a
                // banded COMPU_VTAB_RANGE; both are verbal, so try each.
                if let Some(vtab) = self.module().compu_vtab.get(&r.conversion_table) {
                    let mut pairs: Vec<(f64, String)> = vtab
                        .value_pairs
                        .iter()
                        .map(|p| (p.in_val, p.out_val.clone()))
                        .collect();
                    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
                    return Conversion::Verb {
                        pairs,
                        default: vtab.default_value.as_ref().map(|d| d.display_string.clone()),
                    };
                }
                if let Some(vrange) = self.module().compu_vtab_range.get(&r.conversion_table) {
                    let mut ranges: Vec<(f64, f64, String)> = vrange
                        .value_triples
                        .iter()
                        .map(|t| (t.in_val_min, t.in_val_max, t.out_val.clone()))
                        .collect();
                    ranges.sort_by(|a, b| a.0.total_cmp(&b.0));
                    return Conversion::VerbRange {
                        ranges,
                        default: vrange
                            .default_value
                            .as_ref()
                            .map(|d| d.display_string.clone()),
                    };
                }
                Conversion::Unsupported(format!(
                    "no COMPU_VTAB or COMPU_VTAB_RANGE named '{}'",
                    r.conversion_table
                ))
            }

            ConversionType::Form => {
                let Some(f) = &cm.formula else {
                    return Conversion::Unsupported("FORM without a FORMULA block".into());
                };
                let forward = match crate::formula::Formula::parse(&f.fx) {
                    Ok(p) => Box::new(p),
                    Err(e) => return Conversion::Unsupported(format!("formula: {e}")),
                };
                // FORMULA_INV is optional; without it the conversion is
                // display-only, which `is_invertible` reports.
                let inverse = f
                    .formula_inv
                    .as_ref()
                    .and_then(|i| crate::formula::Formula::parse(&i.gx).ok())
                    .map(Box::new);
                Conversion::Form {
                    forward,
                    inverse,
                    constants: self.system_constants.clone(),
                }
            }
        }
    }

    /// Resolve a CHARACTERISTIC into a plan.
    pub fn plan_characteristic(&self, name: &str) -> Option<ObjectPlan> {
        let ch = self.module().characteristic.get(name)?;
        let conv = self.conversion_for(&ch.conversion);
        let endian = ch
            .byte_order
            .as_ref()
            .map(|b| endian_of(b.byte_order))
            .unwrap_or(self.default_endian);

        let rl = self.module().record_layout.get(&ch.deposit);

        // Shape classification comes first: it decides whether the rest is
        // even meaningful.
        let (mut category, mut note) = classify_characteristic(ch, rl);

        let axes = self.axis_specs(ch, category);
        let dims = self.characteristic_dims(ch, category, rl, &axes);
        let declared_points: u32 = dims.iter().product();
        let layout = match rl {
            Some(rl) => layout::resolve(rl, &self.aligns, &dims),
            None => {
                note = Some(format!("RECORD_LAYOUT '{}' not found", ch.deposit));
                category = Category::Unsupported;
                ResolvedLayout::default()
            }
        };

        // A conversion we cannot evaluate makes the physical value unavailable,
        // which from the user's side is indistinguishable from an unsupported
        // shape — so report it the same way rather than showing a blank row.
        if let Conversion::Unsupported(reason) = &conv.conversion {
            if category != Category::Unsupported {
                category = Category::Unsupported;
                note = Some(reason.clone());
            }
        }

        // Presentation order per dimension. STD_AXIS order lives in this
        // record layout; a shared axis carries its own.
        let dims_reversed: Vec<bool> = (0..dims.len())
            .map(|d| match axes.get(d).map(|a| &a.source) {
                Some(AxisSource::Internal) => {
                    layout.axes.get(d).map(|a| a.index_decr).unwrap_or(false)
                }
                Some(AxisSource::AxisPts(n)) => self.axis_pts_is_decreasing(n),
                _ => false,
            })
            .collect();

        Some(ObjectPlan {
            name: name.to_string(),
            description: ch.long_identifier.clone(),
            kind: ObjKind::Characteristic,
            category,
            address: ch.address,
            layout,
            conv,
            axes,
            bit_mask: ch.bit_mask.as_ref().map(|b| b.mask).unwrap_or(0),
            virtual_formula: ch.virtual_characteristic.as_ref().map(|v| v.formula.clone()),
            virtual_inputs: ch
                .virtual_characteristic
                .as_ref()
                .map(|v| v.characteristic_list.clone())
                .unwrap_or_default(),
            dims,
            dims_reversed,
            endian,
            lower_limit: ch.lower_limit,
            upper_limit: ch.upper_limit,
            declared_points,
            format_override: ch.format.as_ref().map(|f| f.format_string.clone()),
            note,
        })
    }

    /// Resolve an AXIS_PTS object into a plan. Its "function values" are the
    /// axis breakpoints themselves.
    pub fn plan_axis_pts(&self, name: &str) -> Option<ObjectPlan> {
        let ap = self.module().axis_pts.get(name)?;
        let conv = self.conversion_for(&ap.conversion);
        let endian = ap
            .byte_order
            .as_ref()
            .map(|b| endian_of(b.byte_order))
            .unwrap_or(self.default_endian);

        let rl = self.module().record_layout.get(&ap.deposit_record);
        let declared_points = ap.max_axis_points as u32;

        let (category, note, layout) = match rl {
            Some(rl) => {
                let resolved = layout::resolve(rl, &self.aligns, &[declared_points]);
                if resolved.axis_pts().is_none()
                    && resolved.fnc.is_none()
                    && resolved.rescale.is_none()
                {
                    (
                        Category::Unsupported,
                        Some("record layout defines no axis points".to_string()),
                        resolved,
                    )
                } else {
                    (Category::Curve, None, resolved)
                }
            }
            None => (
                Category::Unsupported,
                Some(format!("RECORD_LAYOUT '{}' not found", ap.deposit_record)),
                ResolvedLayout::default(),
            ),
        };
        // The object *is* an axis, so its own INDEX_DECR governs how its
        // points are presented even though it has no AXIS_DESCR of its own.
        let dims_reversed = vec![layout.axis_index_decr()];

        Some(ObjectPlan {
            name: name.to_string(),
            description: ap.long_identifier.clone(),
            kind: ObjKind::AxisPts,
            category,
            address: ap.address,
            layout,
            conv,
            axes: Vec::new(),
            // AXIS_PTS carries no BIT_MASK.
            bit_mask: 0,
            virtual_formula: None,
            virtual_inputs: Vec::new(),
            dims: vec![declared_points],
            dims_reversed,
            endian,
            lower_limit: ap.lower_limit,
            upper_limit: ap.upper_limit,
            declared_points,
            format_override: ap.format.as_ref().map(|f| f.format_string.clone()),
            note,
        })
    }

    /// Resolve a MEASUREMENT. These have a datatype directly rather than a
    /// record layout, and live in RAM, so they are usually absent from a flash
    /// image — which is exactly what makes them useful for RAM-dump analysis.
    pub fn plan_measurement(&self, name: &str) -> Option<ObjectPlan> {
        let m = self.module().measurement.get(name)?;
        let conv = self.conversion_for(&m.conversion);
        let endian = m
            .byte_order
            .as_ref()
            .map(|b| endian_of(b.byte_order))
            .unwrap_or(self.default_endian);

        let address = m.ecu_address.as_ref().map(|a| a.address).unwrap_or(0);
        let mut note = None;
        let mut category = Category::Scalar;

        if m.ecu_address.is_none() {
            category = Category::Unsupported;
            note = Some("no ECU_ADDRESS".to_string());
        } else if m.array_size.is_some() || m.matrix_dim.is_some() {
            category = Category::Unsupported;
            note = Some("measurement arrays are not decoded".to_string());
        } else if let Conversion::Unsupported(reason) = &conv.conversion {
            category = Category::Unsupported;
            note = Some(reason.clone());
        }

        // Synthesise a single-element layout from the declared datatype.
        let layout = ResolvedLayout {
            total_size: layout::datatype_size(m.datatype),
            fnc: Some(Field {
                offset: 0,
                datatype: m.datatype,
                count: 1,
            }),
            ..Default::default()
        };

        Some(ObjectPlan {
            name: name.to_string(),
            description: m.long_identifier.clone(),
            kind: ObjKind::Measurement,
            category,
            address,
            layout,
            conv,
            axes: Vec::new(),
            bit_mask: m.bit_mask.as_ref().map(|b| b.mask).unwrap_or(0),
            virtual_formula: None,
            virtual_inputs: Vec::new(),
            dims: vec![1],
            dims_reversed: vec![false],
            endian,
            lower_limit: m.lower_limit,
            upper_limit: m.upper_limit,
            declared_points: 1,
            format_override: m.format.as_ref().map(|f| f.format_string.clone()),
            note,
        })
    }

    /// Resolve every AXIS_DESCR into where its breakpoints live and how many
    /// there are.
    fn axis_specs(&self, ch: &Characteristic, category: Category) -> Vec<AxisSpec> {
        if !matches!(category, Category::Curve | Category::Map) {
            return Vec::new();
        }
        ch.axis_descr
            .iter()
            .take(layout::MAX_AXES)
            .map(|descr| {
                use a2lfile::AxisDescrAttribute as A;
                let (source, kind) = match descr.attribute {
                    A::StdAxis => (AxisSource::Internal, "STD_AXIS"),
                    // COM_AXIS and RES_AXIS share an AXIS_PTS object…
                    A::ComAxis => (axis_pts_ref(descr), "COM_AXIS"),
                    A::ResAxis => (axis_pts_ref(descr), "RES_AXIS"),
                    // …whereas CURVE_AXIS borrows another curve's values
                    // through a different field entirely.
                    A::CurveAxis => (
                        descr
                            .curve_axis_ref
                            .as_ref()
                            .map(|r| AxisSource::CurveRef(r.curve_axis.clone()))
                            .unwrap_or(AxisSource::None),
                        "CURVE_AXIS",
                    ),
                    A::FixAxis => (AxisSource::Fixed(fixed_axis_values(descr)), "FIX_AXIS"),
                };
                let points = self.axis_point_count(descr, &source);
                AxisSpec {
                    source,
                    conv: Some(self.conversion_for(&descr.conversion)),
                    kind,
                    points,
                }
            })
            .collect()
    }

    /// How many breakpoints one dimension really has.
    ///
    /// `max_axis_points` on the AXIS_DESCR is only the declaration's own
    /// estimate. When the axis lives in a shared AXIS_PTS object that object's
    /// allocation wins — the demo A2L spells this out on
    /// `ASAM.C.MAP.COM_AXIS.FIX_AXIS`, whose descriptor says 8 with the comment
    /// that it "will be overwritten by max number of axis points of AXIS_PTS".
    /// Believing the descriptor there would mis-size the function values and
    /// every byte after them.
    fn axis_point_count(&self, descr: &a2lfile::AxisDescr, source: &AxisSource) -> u32 {
        match source {
            AxisSource::AxisPts(name) => self
                .module()
                .axis_pts
                .get(name)
                .map(|ap| ap.max_axis_points as u32)
                .unwrap_or(u32::from(descr.max_axis_points)),
            AxisSource::CurveRef(name) => self
                .module()
                .characteristic
                .get(name)
                .map(characteristic_declared_points)
                .unwrap_or(u32::from(descr.max_axis_points)),
            // A computed axis is exactly as long as its parameters say.
            AxisSource::Fixed(values) if !values.is_empty() => values.len() as u32,
            _ => u32::from(descr.max_axis_points),
        }
        .max(1)
    }

    /// Element counts per dimension for a characteristic.
    fn characteristic_dims(
        &self,
        ch: &Characteristic,
        category: Category,
        rl: Option<&RecordLayout>,
        axes: &[AxisSpec],
    ) -> Vec<u32> {
        if category == Category::Scalar || category == Category::Ascii {
            return vec![characteristic_declared_points(ch)];
        }
        // An axed object is shaped by its axes.
        if !axes.is_empty() {
            return axes.iter().map(|a| a.points).collect();
        }
        // A VAL_BLK has no axes; MATRIX_DIM is its shape.
        let dims = matrix_dims(ch);
        if !dims.is_empty() {
            return dims;
        }
        // Neither: fall back to the single declared count, but never claim
        // fewer dimensions than the record layout actually stores.
        let n = rl.map(layout::layout_axis_count).unwrap_or(0).max(1);
        let mut out = vec![characteristic_declared_points(ch)];
        out.resize(n, 1);
        out
    }

    /// All object names to list, in A2L declaration order.
    pub fn object_names(&self, include_measurements: bool) -> Vec<(String, ObjKind)> {
        let m = self.module();
        let mut out: Vec<(String, ObjKind)> = Vec::with_capacity(
            m.characteristic.len() + m.axis_pts.len() + m.measurement.len(),
        );
        for c in m.characteristic.iter() {
            out.push((c.get_name().to_string(), ObjKind::Characteristic));
        }
        for a in m.axis_pts.iter() {
            out.push((a.get_name().to_string(), ObjKind::AxisPts));
        }
        if include_measurements {
            for x in m.measurement.iter() {
                out.push((x.get_name().to_string(), ObjKind::Measurement));
            }
        }
        out
    }

    /// Resolve any object by name and kind.
    pub fn plan(&self, name: &str, kind: ObjKind) -> Option<ObjectPlan> {
        match kind {
            ObjKind::Characteristic => self.plan_characteristic(name),
            ObjKind::AxisPts => self.plan_axis_pts(name),
            ObjKind::Measurement => self.plan_measurement(name),
        }
    }

    /// Find an object by name alone, trying each block in turn.
    pub fn plan_any(&self, name: &str) -> Option<ObjectPlan> {
        self.plan_characteristic(name)
            .or_else(|| self.plan_axis_pts(name))
            .or_else(|| self.plan_measurement(name))
    }
}

/// The name of a SYSTEM_CONSTANT.
///
/// a2lfile 3.5 keeps `SystemConstant::name` crate-private and implements
/// neither `A2lObjectName` nor a getter for it, so its `Debug` output is the
/// only public route to the name. The parse below is deliberately narrow, and
/// `system_constant_name_survives_debug_format` fails loudly if a future
/// a2lfile changes that formatting — without it, every `sysc()` reference would
/// quietly stop resolving.
impl A2lDatabase {
    /// Whether a referenced AXIS_PTS object stores its points highest-first.
    fn axis_pts_is_decreasing(&self, name: &str) -> bool {
        self.module()
            .axis_pts
            .get(name)
            .and_then(|ap| self.module().record_layout.get(&ap.deposit_record))
            .map(|rl| {
                rl.axis_pts_x
                    .as_ref()
                    .map(|f| f.index_incr == a2lfile::IndexOrder::IndexDecr)
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }
}

fn system_constant_name(sc: &a2lfile::SystemConstant) -> Option<String> {
    let rendered = format!("{sc:?}");
    let after = rendered.split("name: \"").nth(1)?;
    let name = after.split('"').next()?;
    (!name.is_empty()).then(|| name.to_string())
}

/// The shared AXIS_PTS object an axis descriptor points at.
fn axis_pts_ref(descr: &a2lfile::AxisDescr) -> AxisSource {
    descr
        .axis_pts_ref
        .as_ref()
        .map(|r| AxisSource::AxisPts(r.axis_points.clone()))
        .unwrap_or(AxisSource::None)
}

/// FIX_AXIS breakpoints, which are computed rather than stored.
fn fixed_axis_values(descr: &a2lfile::AxisDescr) -> Vec<f64> {
    if let Some(p) = &descr.fix_axis_par {
        // Offset plus a power-of-two step: value[i] = offset + i * 2^shift.
        let step = 2f64.powf(p.shift);
        return (0..p.number_apo as u32)
            .map(|i| p.offset + f64::from(i) * step)
            .collect();
    }
    if let Some(p) = &descr.fix_axis_par_dist {
        return (0..p.number_apo as u32)
            .map(|i| p.offset + f64::from(i) * p.distance)
            .collect();
    }
    if let Some(p) = &descr.fix_axis_par_list {
        return p.axis_pts_value_list.clone();
    }
    Vec::new()
}

/// Classify a characteristic by shape alone.
fn classify_characteristic(
    ch: &Characteristic,
    rl: Option<&RecordLayout>,
) -> (Category, Option<String>) {
    // A computed parameter is never stored. Its address is a placeholder — all
    // four in the demo file declare 0x0 — so it must not be treated as data
    // that merely happens to be missing from the image.
    if ch.virtual_characteristic.is_some() {
        return (Category::Virtual, None);
    }
    let multi_dim_layout = rl.map(layout::layout_axis_count).unwrap_or(0) > 1;
    match ch.characteristic_type {
        // A VALUE or CURVE whose record layout stores Y or higher is really
        // multi-dimensional, whatever the type keyword claims.
        CharacteristicType::Value if !multi_dim_layout => (Category::Scalar, None),
        CharacteristicType::Ascii if !multi_dim_layout => (Category::Ascii, None),
        CharacteristicType::Curve | CharacteristicType::ValBlk if !multi_dim_layout => {
            (Category::Curve, None)
        }
        CharacteristicType::Map
        | CharacteristicType::Cuboid
        | CharacteristicType::Cube4
        | CharacteristicType::Cube5 => (Category::Map, None),
        _ => (Category::Map, None),
    }
}

/// The declared MATRIX_DIM, zero and absent dimensions dropped.
fn matrix_dims(ch: &Characteristic) -> Vec<u32> {
    let Some(md) = ch.matrix_dim.as_ref() else {
        return Vec::new();
    };
    let mut dims: Vec<u32> = md
        .dim_list
        .iter()
        .copied()
        .filter(|d| *d > 0)
        .map(u32::from)
        .collect();

    // ASAP2 up to 1.6 required all three dimensions to be written, so a plain
    // array of eight is spelled `MATRIX_DIM 8 1 1`. Those trailing ones are
    // padding rather than shape and would otherwise be presented as a 8 x 1 x 1
    // grid. Dropping them cannot move any element: a trailing dimension of
    // extent one has a subscript that is always zero.
    //
    // Only *trailing* ones go. An interior one is load-bearing — COLUMN_DIR
    // swaps the first two dimensions, so `3 1 4` and `3 4` store differently.
    while dims.len() > 1 && dims.last() == Some(&1) {
        dims.pop();
    }
    dims
}

/// Total elements a characteristic declares, ignoring how they are shaped.
///
/// MATRIX_DIM is the modern spelling and wins — its dimensions multiply, since
/// taking only the first truncates a 3x4 VAL_BLK to 3 of its 12 elements and
/// sizes it at a quarter of the bytes it occupies. NUMBER is the deprecated
/// spelling; failing both, the axis description's maximum is the allocation.
fn characteristic_declared_points(ch: &Characteristic) -> u32 {
    if ch.characteristic_type == CharacteristicType::Value {
        return 1;
    }
    let dims = matrix_dims(ch);
    if !dims.is_empty() {
        return dims.iter().product();
    }
    if let Some(n) = &ch.number {
        if n.number > 0 {
            return u32::from(n.number);
        }
    }
    if let Some(d) = ch.axis_descr.first() {
        if d.max_axis_points > 0 {
            return u32::from(d.max_axis_points);
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards the Debug-based workaround in `system_constant_name`.
    #[test]
    fn system_constant_name_survives_debug_format() {
        let sc = a2lfile::SystemConstant::new("System_Constant_1".into(), "-3.45".into());
        assert_eq!(
            system_constant_name(&sc).as_deref(),
            Some("System_Constant_1"),
            "a2lfile's Debug output for SystemConstant changed shape; the name \
             can no longer be recovered and every sysc() reference would break"
        );
    }

    /// A bare plan carrying only the fields the index mapping consults.
    fn plan_for(dims: &[u32], column_dir: bool, reversed: &[bool]) -> ObjectPlan {
        ObjectPlan {
            name: "T".into(),
            description: String::new(),
            kind: ObjKind::Characteristic,
            category: Category::Curve,
            address: 0,
            layout: ResolvedLayout {
                fnc_column_dir: column_dir,
                ..Default::default()
            },
            conv: ConvInfo::identity("CM"),
            axes: Vec::new(),
            bit_mask: 0,
            virtual_formula: None,
            virtual_inputs: Vec::new(),
            dims: dims.to_vec(),
            dims_reversed: reversed.to_vec(),
            endian: Endian::Little,
            lower_limit: 0.0,
            upper_limit: 0.0,
            declared_points: dims.iter().product::<u32>().max(1),
            format_override: None,
            note: None,
        }
    }

    fn slots(plan: &ObjectPlan, count: u32) -> Vec<u32> {
        (0..count).map(|i| plan.storage_slot(i, count)).collect()
    }

    fn dims_of(list: &[u16]) -> Vec<u32> {
        let mut ch = a2lfile::Characteristic::new(
            "C".into(),
            String::new(),
            CharacteristicType::ValBlk,
            0,
            "RL".into(),
            0.0,
            "CM".into(),
            0.0,
            0.0,
        );
        let mut md = a2lfile::MatrixDim::new();
        md.dim_list = list.to_vec();
        ch.matrix_dim = Some(md);
        matrix_dims(&ch)
    }

    /// Trailing ones are the ASAP2 <= 1.6 padding and carry no shape; an
    /// interior one does, because COLUMN_DIR swaps the first two dimensions.
    #[test]
    fn matrix_dim_drops_only_trailing_ones() {
        assert_eq!(dims_of(&[8, 1, 1]), vec![8]);
        assert_eq!(dims_of(&[3, 4, 1]), vec![3, 4]);
        assert_eq!(dims_of(&[1, 8, 1]), vec![1, 8]);
        assert_eq!(dims_of(&[3, 1, 4]), vec![3, 1, 4]);
        assert_eq!(dims_of(&[1, 1, 1]), vec![1]);
        assert_eq!(dims_of(&[3, 4]), vec![3, 4]);
    }

    #[test]
    fn row_dir_stores_in_presentation_order() {
        let plan = plan_for(&[3, 4], false, &[false, false]);
        assert_eq!(slots(&plan, 12), (0..12).collect::<Vec<_>>());
    }

    /// A 3x4 COLUMN_DIR block is stored transposed: presentation walks the
    /// first dimension fastest, storage walks the last.
    #[test]
    fn column_dir_maps_row_major_onto_column_major() {
        let plan = plan_for(&[3, 4], true, &[false, false]);
        assert_eq!(
            slots(&plan, 12),
            vec![0, 4, 8, 1, 5, 9, 2, 6, 10, 3, 7, 11]
        );
    }

    /// Whatever the shape, the mapping must be a permutation — every element
    /// reachable exactly once, or reading loses data and writing doubles up.
    #[test]
    fn the_mapping_is_always_a_permutation() {
        let shapes = [
            vec![3, 4],
            vec![4, 3],
            vec![2, 3, 4],
            vec![5, 1, 2],
            vec![2, 3, 4, 5],
        ];
        for dims in shapes {
            let count: u32 = dims.iter().product();
            for column_dir in [false, true] {
                // Every combination of per-axis reversal, so a map with a
                // decreasing Y is covered as well as one with neither.
                for mask in 0..(1u32 << dims.len()) {
                    let rev: Vec<bool> =
                        (0..dims.len()).map(|d| mask >> d & 1 == 1).collect();
                    let plan = plan_for(&dims, column_dir, &rev);
                    let mut seen = slots(&plan, count);
                    seen.sort_unstable();
                    assert_eq!(
                        seen,
                        (0..count).collect::<Vec<_>>(),
                        "dims {dims:?} column_dir {column_dir} reversed {rev:?}"
                    );
                }
            }
        }
    }

    /// COLUMN_DIR only means something across dimensions; a flat array is
    /// stored in the order it is read, whatever the record layout declares.
    #[test]
    fn column_dir_is_identity_for_one_dimension() {
        assert_eq!(
            slots(&plan_for(&[6], true, &[false]), 6),
            (0..6).collect::<Vec<_>>()
        );
    }

    #[test]
    fn index_decr_reverses_a_curve() {
        let plan = plan_for(&[4], false, &[true]);
        assert_eq!(slots(&plan, 4), vec![3, 2, 1, 0]);
    }

    /// An INDEX_DECR axis on a map reverses along *its own* dimension. The
    /// whole-array reversal that is correct in one dimension would here also
    /// flip X, mispairing every value with the wrong breakpoint.
    #[test]
    fn index_decr_on_y_reverses_only_y() {
        // 3 wide, 4 tall, Y stored highest-first.
        let plan = plan_for(&[3, 4], false, &[false, true]);
        // Presentation row 0 must read storage row 3, left to right.
        assert_eq!(slots(&plan, 12), vec![9, 10, 11, 6, 7, 8, 3, 4, 5, 0, 1, 2]);

        // And with X reversed instead, each row is mirrored in place.
        let plan = plan_for(&[3, 4], false, &[true, false]);
        assert_eq!(slots(&plan, 12), vec![2, 1, 0, 5, 4, 3, 8, 7, 6, 11, 10, 9]);
    }

    /// A field the declared shape does not account for — a rescale axis, or a
    /// MATRIX_DIM at odds with the record layout — is treated as a flat run
    /// rather than indexed through a shape that does not fit it.
    #[test]
    fn a_mismatched_extent_falls_back_to_a_flat_run() {
        let plan = plan_for(&[3, 4], true, &[false, false]);
        assert_eq!(slots(&plan, 5), (0..5).collect::<Vec<_>>());
    }
}

fn conversion_type_name(t: ConversionType) -> &'static str {
    match t {
        ConversionType::Identical => "IDENTICAL",
        ConversionType::Form => "FORM",
        ConversionType::Linear => "LINEAR",
        ConversionType::RatFunc => "RAT_FUNC",
        ConversionType::TabIntp => "TAB_INTP",
        ConversionType::TabNointp => "TAB_NOINTP",
        ConversionType::TabVerb => "TAB_VERB",
    }
}
