// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

//! Moving calibration values between a CDFX file and the firmware image.
//!
//! Import never writes anything itself. It returns the bytes each change would
//! produce, so the caller can show what would happen and then apply the lot as
//! a single undoable edit.

use serde::{Deserialize, Serialize};

use crate::cdfx::{CdfxAxis, CdfxFile, CdfxInstance, CdfxValue};
use crate::db::{A2lDatabase, AxisSource};
use crate::decode;
use crate::encode::{self, PointTarget};
use crate::model::{ByteSource, Category, ObjKind, Presence};

/// One value the import would change, with the bytes to write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdfxChange {
    pub name: String,
    /// `value`, `axis` or `text`.
    pub target: String,
    /// Point index for a 1D object; absent for a scalar or string.
    pub index: Option<u32>,
    pub current: String,
    pub incoming: String,
    pub address: u32,
    pub bytes: Vec<u8>,
}

/// A parameter the import passed over, and why.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdfxSkip {
    pub name: String,
    pub reason: String,
}

/// What an import would do, without having done any of it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CdfxImport {
    pub file_name: String,
    /// SW-INSTANCEs found in the file.
    pub file_instances: usize,
    /// Instances resolved to an A2L object this crate can write.
    pub matched: usize,
    /// Matched parameters whose stored bytes already agree with the file.
    pub unchanged: usize,
    /// Named in the file but absent from the loaded A2L.
    pub not_in_a2l: Vec<String>,
    /// Matched but deliberately passed over — maps, computed values, and so on.
    pub skipped: Vec<CdfxSkip>,
    pub changes: Vec<CdfxChange>,
}

impl CdfxImport {
    /// Parameters with at least one differing element.
    pub fn changed_parameters(&self) -> usize {
        let mut names: Vec<&str> = self.changes.iter().map(|c| c.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        names.len()
    }
}

/// How one decoded point is recorded in a CDFX.
///
/// A verbal conversion renders a label, and the label is what the file holds —
/// the demo's map has a Y axis of `red`, `orange`, … which its own CDFX writes
/// as `VT` elements. Writing the underlying breakpoint number instead would
/// name a different thing to every tool that reads it back. A formatted number
/// still parses as one, so only genuine labels take the text path.
fn point_value(p: &crate::model::PointValue) -> CdfxValue {
    if p.display.trim().parse::<f64>().is_ok() {
        CdfxValue::Num(p.phys)
    } else {
        CdfxValue::Text(p.display.clone())
    }
}

/// Render a before/after pair so the difference between them is visible.
///
/// The A2L `FORMAT` is the right precision for reading, but a change can be
/// real and still invisible at it: a CDFX written by another tool rounds, so
/// the demo file's `-3` against a stored -3.000000491882041 is a genuine write
/// that would otherwise be presented as "-3 → -3". When the formatted forms
/// collide, both are re-rendered at full precision instead.
fn pair(fmt: &str, current: f64, incoming: f64) -> (String, String) {
    let a = crate::decode::format_number(current, fmt);
    let b = crate::decode::format_number(incoming, fmt);
    if a == b {
        (crate::cdfx::exact(current), crate::cdfx::exact(incoming))
    } else {
        (a, b)
    }
}

/// Would writing `w` alter the image?
///
/// Comparing the bytes rather than the physical values sidesteps float
/// tolerance entirely: what matters is whether the stored representation
/// changes, and a value that rounds to the same raw is not an edit.
fn differs(src: &dyn ByteSource, address: u32, bytes: &[u8]) -> bool {
    match src.read(address, bytes.len() as u32) {
        Some(current) => current != bytes,
        // Absent bytes would be created by the write, which is a change.
        None => true,
    }
}

/// Compare a CDFX file against the image and describe what importing it would
/// alter. Nothing is written.
pub fn plan_import(db: &A2lDatabase, src: &dyn ByteSource, file: &CdfxFile) -> CdfxImport {
    let mut out = CdfxImport {
        file_name: file.short_name.clone(),
        file_instances: file.instances.len(),
        ..Default::default()
    };

    for inst in &file.instances {
        let Some(plan) = db.plan_any(&inst.name) else {
            out.not_in_a2l.push(inst.name.clone());
            continue;
        };

        let skip = |reason: &str| CdfxSkip {
            name: inst.name.clone(),
            reason: reason.to_string(),
        };

        match plan.category {
            Category::Virtual => {
                out.skipped.push(skip("computed, not stored"));
                continue;
            }
            Category::Unsupported => {
                out.skipped
                    .push(skip(plan.note.as_deref().unwrap_or("shape not supported")));
                continue;
            }
            _ => {}
        }
        if plan.kind == ObjKind::Measurement {
            out.skipped.push(skip("measurement, not calibration data"));
            continue;
        }
        if decode::presence_of(src, &plan) != Presence::Full {
            out.skipped.push(skip("not fully present in the image"));
            continue;
        }

        out.matched += 1;
        let before = out.changes.len();
        match plan.category {
            Category::Scalar | Category::Ascii => {
                apply_single(db, src, inst, &mut out);
            }
            Category::Curve | Category::Map => {
                apply_points(db, src, inst, &mut out);
            }
            _ => {}
        }
        if out.changes.len() == before {
            out.unchanged += 1;
        }
    }

    out
}

/// A scalar, enum or string: a single value in the file.
fn apply_single(db: &A2lDatabase, src: &dyn ByteSource, inst: &CdfxInstance, out: &mut CdfxImport) {
    let Some(incoming) = inst.values.first() else {
        out.skipped.push(CdfxSkip {
            name: inst.name.clone(),
            reason: "no value in the file".into(),
        });
        return;
    };
    let Some(plan) = db.plan_any(&inst.name) else { return };
    let row = decode::row_for(db, src, &plan);

    let encoded = match incoming {
        CdfxValue::Num(v) => encode::encode_scalar(db, src, &inst.name, *v),
        CdfxValue::Text(t) => encode::encode_text(db, src, &inst.name, t),
    };
    match encoded {
        Ok(w) => {
            if differs(src, w.address, &w.bytes) {
                let (current, incoming_text) = match (incoming, row.phys_num) {
                    (CdfxValue::Num(v), Some(now)) => pair(plan.format(), now, *v),
                    _ => (row.display.clone(), incoming.display()),
                };
                out.changes.push(CdfxChange {
                    name: inst.name.clone(),
                    target: if matches!(incoming, CdfxValue::Text(_)) {
                        "text".into()
                    } else {
                        "value".into()
                    },
                    index: None,
                    current,
                    incoming: incoming_text,
                    address: w.address,
                    bytes: w.bytes,
                });
            }
        }
        Err(e) => out.skipped.push(CdfxSkip {
            name: inst.name.clone(),
            reason: e,
        }),
    }
}

/// A curve, value block or axis: element-by-element.
fn apply_points(db: &A2lDatabase, src: &dyn ByteSource, inst: &CdfxInstance, out: &mut CdfxImport) {
    let Some(plan) = db.plan_any(&inst.name) else { return };
    let Some(detail) = decode::detail_for(db, src, &inst.name) else {
        return;
    };

    // A length mismatch means the file describes a differently shaped object.
    // Writing the overlap would silently half-apply a calibration, so the whole
    // parameter is passed over instead.
    if inst.values.len() != detail.values.len() {
        out.skipped.push(CdfxSkip {
            name: inst.name.clone(),
            reason: format!(
                "file has {} values, the object holds {}",
                inst.values.len(),
                detail.values.len()
            ),
        });
        return;
    }

    for (i, incoming) in inst.values.iter().enumerate() {
        let Some(v) = incoming.as_num() else { continue };
        push_point(
            db, src, inst, &detail, PointTarget::Value, i, v, out,
        );
    }

    // Breakpoints, dimension by dimension. Only an axis stored in this object's
    // own record is written here; a shared one belongs to its AXIS_PTS object
    // and arrives as its own instance.
    for (d, axis_cont) in inst.axes.iter().enumerate() {
        if axis_cont.instance_ref.is_some() {
            continue;
        }
        if !matches!(
            plan.axes.get(d).map(|a| &a.source),
            Some(AxisSource::Internal)
        ) {
            continue;
        }
        let held = detail.axes.get(d).map(|a| a.points.len()).unwrap_or(0);
        if axis_cont.values.len() != held {
            if !axis_cont.values.is_empty() {
                out.skipped.push(CdfxSkip {
                    name: format!("{} axis {d}", inst.name),
                    reason: format!(
                        "file has {} axis points, the object holds {held}",
                        axis_cont.values.len(),
                    ),
                });
            }
            continue;
        }
        for (i, incoming) in axis_cont.values.iter().enumerate() {
            // A verbal axis records labels; those are the input quantity's
            // conversion, not a breakpoint this object stores.
            let Some(v) = incoming.as_num() else { continue };
            push_point(db, src, inst, &detail, PointTarget::Axis(d), i, v, out);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_point(
    db: &A2lDatabase,
    src: &dyn ByteSource,
    inst: &CdfxInstance,
    detail: &crate::model::ParamDetail,
    target: PointTarget,
    i: usize,
    value: f64,
    out: &mut CdfxImport,
) {
    let fmt = db
        .plan_any(&inst.name)
        .map(|p| p.format().to_string())
        .unwrap_or_default();
    match encode::encode_point(db, src, &inst.name, target, i as u32, value) {
        Ok(w) => {
            if differs(src, w.address, &w.bytes) {
                let now = match target {
                    PointTarget::Value => detail.values.get(i),
                    PointTarget::Axis(d) => detail.axes.get(d).and_then(|a| a.points.get(i)),
                };
                let (current, incoming) = match now {
                    Some(p) => pair(&fmt, p.phys, value),
                    None => (String::new(), crate::cdfx::exact(value)),
                };
                out.changes.push(CdfxChange {
                    name: inst.name.clone(),
                    target: match target {
                        PointTarget::Value => "value".to_string(),
                        // One axis needs no qualifier; a map's do.
                        PointTarget::Axis(0) => "axis".to_string(),
                        PointTarget::Axis(d) => format!("axis {d}"),
                    },
                    index: Some(i as u32),
                    current,
                    incoming,
                    address: w.address,
                    bytes: w.bytes,
                });
            }
        }
        Err(e) => out.skipped.push(CdfxSkip {
            name: format!("{}[{i}]", inst.name),
            reason: e,
        }),
    }
}

// ── Export ───────────────────────────────────────────────────────────────────

/// Build CDFX instances from everything in the image this crate can decode.
///
/// Shapes it cannot decode are omitted rather than written as placeholders — a
/// CDFX entry with invented values would be worse than an absent one.
pub fn export(db: &A2lDatabase, src: &dyn ByteSource) -> Vec<CdfxInstance> {
    let mut out = Vec::new();

    for (name, kind) in db.object_names(false) {
        let Some(plan) = db.plan(&name, kind) else { continue };
        if decode::presence_of(src, &plan) != Presence::Full {
            continue;
        }
        let row = decode::row_for(db, src, &plan);

        match plan.category {
            Category::Scalar => {
                // A verbal conversion renders a label, and the label is what a
                // CDFX records — writing the underlying number instead would
                // be a different value to every tool that reads it back.
                let values = match (&row.enum_options, row.phys_num) {
                    (Some(_), _) => vec![CdfxValue::Text(row.display.clone())],
                    (None, Some(v)) => vec![CdfxValue::Num(v)],
                    (None, None) => continue,
                };
                out.push(CdfxInstance {
                    name,
                    category: "VALUE".into(),
                    values,
                    array_size: Vec::new(),
                    axes: vec![],
                });
            }

            Category::Ascii => {
                let Some(text) = row.text_value.clone() else { continue };
                out.push(CdfxInstance {
                    name,
                    category: "ASCII".into(),
                    values: vec![CdfxValue::Text(text)],
                    array_size: Vec::new(),
                    axes: vec![],
                });
            }

            Category::Curve | Category::Map => {
                let Some(detail) = decode::detail_for(db, src, &name) else {
                    continue;
                };
                let values = detail.values.iter().map(point_value).collect();

                // The A2L kind decides how the shape is named: a standalone
                // axis object is an axis, a characteristic without one is an
                // array rather than a curve, and anything with two or more
                // axes is named for how many it has.
                let category = match (kind, plan.layout.rescale.is_some(), plan.axes.len()) {
                    (ObjKind::AxisPts, true, _) => "RES_AXIS",
                    (ObjKind::AxisPts, false, _) => "COM_AXIS",
                    (_, _, 0) => "VAL_BLK",
                    (_, _, 1) => "CURVE",
                    (_, _, 2) => "MAP",
                    (_, _, 3) => "CUBOID",
                    (_, _, 4) => "CUBE_4",
                    _ => "CUBE_5",
                };

                // One SW-AXIS-CONT per dimension, holding either the
                // breakpoints or a reference to the object that owns them.
                let axes = plan
                    .axes
                    .iter()
                    .enumerate()
                    .filter_map(|(d, spec)| {
                        let kind = spec.kind.to_string();
                        match &spec.source {
                            AxisSource::Internal | AxisSource::Fixed(_) => {
                                let points = detail.axes.get(d)?;
                                (!points.points.is_empty()).then(|| CdfxAxis {
                                    category: kind,
                                    values: points.points.iter().map(point_value).collect(),
                                    instance_ref: None,
                                })
                            }
                            AxisSource::AxisPts(r) | AxisSource::CurveRef(r) => Some(CdfxAxis {
                                category: kind,
                                values: vec![],
                                instance_ref: Some(r.clone()),
                            }),
                            AxisSource::None => None,
                        }
                    })
                    .collect();

                out.push(CdfxInstance {
                    name,
                    category: category.into(),
                    values,
                    // Only a genuinely multi-dimensional object needs its shape
                    // spelled out; a flat list is unambiguous without it.
                    array_size: if detail.dims.len() > 1 {
                        detail.dims.clone()
                    } else {
                        Vec::new()
                    },
                    axes,
                });
            }

            // Computed values are not stored, and unsupported shapes cannot be
            // decoded, so neither is written.
            Category::Virtual | Category::Unsupported => {}
        }
    }

    out
}
