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
    /// Stored in a separate AXIS_PTS object (COM_AXIS).
    External(String),
    /// Computed from FIX_AXIS_PAR / _DIST / _LIST — occupies no image bytes.
    Fixed(Vec<f64>),
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
    pub axis_conv: Option<ConvInfo>,
    pub axis: AxisSource,
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
    /// Total bytes the object occupies in the image.
    pub fn byte_size(&self) -> u32 {
        self.layout.total_size
    }

    /// The datatype shown in the table: the function values' type.
    pub fn datatype(&self) -> Option<DataType> {
        self.layout.fnc.map(|f| f.datatype)
    }
}

/// A parsed A2L description plus the derived defaults decoding depends on.
pub struct A2lDatabase {
    file: A2lFile,
    module_index: usize,
    aligns: Alignments,
    default_endian: Endian,
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

        Ok(A2lDatabase {
            file,
            module_index: 0,
            aligns,
            default_endian,
            summary,
        })
    }

    pub fn summary(&self) -> &A2lSummary {
        &self.summary
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
                Conversion::Unsupported("FORM formulas are not evaluated".into())
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

        let declared_points = characteristic_point_count(ch, category);
        let layout = match rl {
            Some(rl) => layout::resolve(rl, &self.aligns, declared_points),
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

        let (axis, axis_conv) = self.axis_source(ch, category);

        Some(ObjectPlan {
            name: name.to_string(),
            description: ch.long_identifier.clone(),
            kind: ObjKind::Characteristic,
            category,
            address: ch.address,
            layout,
            conv,
            axis_conv,
            axis,
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
                let resolved = layout::resolve(rl, &self.aligns, declared_points);
                if resolved.axis_pts.is_none() && resolved.fnc.is_none() {
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

        Some(ObjectPlan {
            name: name.to_string(),
            description: ap.long_identifier.clone(),
            kind: ObjKind::AxisPts,
            category,
            address: ap.address,
            layout,
            conv,
            axis_conv: None,
            axis: AxisSource::None,
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
            axis_conv: None,
            axis: AxisSource::None,
            endian,
            lower_limit: m.lower_limit,
            upper_limit: m.upper_limit,
            declared_points: 1,
            format_override: m.format.as_ref().map(|f| f.format_string.clone()),
            note,
        })
    }

    /// Determine where a characteristic's axis lives, and how to convert it.
    fn axis_source(
        &self,
        ch: &Characteristic,
        category: Category,
    ) -> (AxisSource, Option<ConvInfo>) {
        if category != Category::Curve {
            return (AxisSource::None, None);
        }
        let Some(descr) = ch.axis_descr.first() else {
            // A VAL_BLK is 1D but has no axis.
            return (AxisSource::None, None);
        };
        let axis_conv = Some(self.conversion_for(&descr.conversion));

        use a2lfile::AxisDescrAttribute as A;
        let source = match descr.attribute {
            A::StdAxis => AxisSource::Internal,
            A::ComAxis | A::ResAxis | A::CurveAxis => match &descr.axis_pts_ref {
                Some(r) => AxisSource::External(r.axis_points.clone()),
                None => AxisSource::None,
            },
            A::FixAxis => AxisSource::Fixed(fixed_axis_values(descr)),
        };
        (source, axis_conv)
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
    // A record layout with Y or higher dimensions is multi-dimensional
    // regardless of what the characteristic type claims.
    if let Some(rl) = rl {
        if layout::is_multi_dimensional(rl) {
            return (
                Category::Unsupported,
                Some("multi-dimensional record layout".to_string()),
            );
        }
    }
    match ch.characteristic_type {
        CharacteristicType::Value => (Category::Scalar, None),
        CharacteristicType::Curve => (Category::Curve, None),
        CharacteristicType::ValBlk => (Category::Curve, None),
        CharacteristicType::Ascii => (
            Category::Unsupported,
            Some("ASCII strings are not decoded".to_string()),
        ),
        CharacteristicType::Map => (
            Category::Unsupported,
            Some("2D maps are not decoded yet".to_string()),
        ),
        CharacteristicType::Cuboid => (
            Category::Unsupported,
            Some("3D cuboids are not decoded yet".to_string()),
        ),
        CharacteristicType::Cube4 => (
            Category::Unsupported,
            Some("4D cubes are not decoded yet".to_string()),
        ),
        CharacteristicType::Cube5 => (
            Category::Unsupported,
            Some("5D cubes are not decoded yet".to_string()),
        ),
    }
}

/// How many points the record is sized for.
///
/// MATRIX_DIM is the modern spelling and wins; NUMBER is the deprecated one;
/// failing both, the axis description's maximum is the allocation.
fn characteristic_point_count(ch: &Characteristic, category: Category) -> u32 {
    if category == Category::Scalar {
        return 1;
    }
    if let Some(md) = &ch.matrix_dim {
        if let Some(first) = md.dim_list.first() {
            if *first > 0 {
                return u32::from(*first);
            }
        }
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
