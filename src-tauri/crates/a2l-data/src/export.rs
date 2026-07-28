// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

//! Flattening the description into one row per value, for a spreadsheet.
//!
//! Export only: nothing here reads a spreadsheet back. That makes the shape a
//! presentation choice rather than a format contract — a curve's breakpoint
//! rides along on the row of the value it belongs to, which duplicates a shared
//! axis but lets one row be read on its own.
//!
//! The XLSX writing lives in the application crate; this produces the rows and
//! nothing else, so the layout can be tested without a spreadsheet library.

use serde::{Deserialize, Serialize};

use crate::db::A2lDatabase;
use crate::decode;
use crate::model::{ByteSource, Category, Presence};

/// One exported row: a single value, with the object it came from repeated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRow {
    pub name: String,
    pub description: String,
    /// `Scalar`, `String`, `Curve`, `Axis`, `Map`, `Virtual`, `Unsupported`.
    pub category: String,
    /// Where this value sits in its object: `Scalar` or `String` for the
    /// single-valued shapes, otherwise a 0-based tuple in A2L dimension order —
    /// `(3)` along a curve, `(2,1)` in a map, `(2,1,3)` in a cuboid. The first
    /// component varies fastest, matching `MATRIX_DIM` and the storage order.
    pub index: String,
    /// The breakpoints at that position, one per dimension, as displayed — so
    /// a verbal axis reads `(red)` rather than the number behind it. Empty
    /// where the object has no axis.
    pub breakpoints: String,
    /// The first axis's breakpoint as a number, filled only for a
    /// one-dimensional object. A curve is the case where charting the pair in
    /// a spreadsheet is worth the column; beyond one axis the tuple says it.
    pub axis_value: Option<f64>,
    /// The physical value, when it is a number.
    pub value: Option<f64>,
    /// The enum label or string contents, when the value is textual.
    pub text: Option<String>,
    pub unit: String,
    /// Absent for a computed parameter, which occupies no image bytes.
    pub address: Option<u32>,
    pub datatype: String,
    pub conversion: String,
    pub conversion_type: String,
    /// `full`, `partial` or `absent`; `unknown` when the extent cannot be
    /// resolved at all.
    pub presence: String,
}

fn category_name(c: Category, is_axis: bool) -> &'static str {
    match c {
        Category::Scalar => "Scalar",
        Category::Ascii => "String",
        Category::Curve if is_axis => "Axis",
        Category::Curve => "Curve",
        Category::Map => "Map",
        Category::Virtual => "Virtual",
        Category::Unsupported => "Unsupported",
    }
}

fn presence_name(p: Presence) -> &'static str {
    match p {
        Presence::Full => "full",
        Presence::Partial => "partial",
        Presence::Absent => "absent",
        Presence::Unknown => "unknown",
    }
}

/// Split a flat index into subscripts, first dimension varying fastest.
fn subscripts(index: u32, dims: &[u32]) -> Vec<u32> {
    let mut rest = index;
    let mut out = Vec::with_capacity(dims.len());
    for d in dims {
        if *d == 0 {
            return Vec::new();
        }
        out.push(rest % d);
        rest /= d;
    }
    out
}

fn tuple(parts: &[String]) -> String {
    format!("({})", parts.join(","))
}

/// Every value the description accounts for, one row each.
///
/// Objects that cannot be decoded still produce a row, with the value columns
/// empty and `presence` saying why — an export that silently omitted them would
/// disagree with the object count on screen.
pub fn rows(db: &A2lDatabase, src: &dyn ByteSource, include_measurements: bool) -> Vec<ExportRow> {
    let mut out = Vec::new();

    for (name, kind) in db.object_names(include_measurements) {
        let Some(plan) = db.plan(&name, kind) else { continue };
        let row = decode::row_for(db, src, &plan);
        let is_axis = kind == crate::model::ObjKind::AxisPts;

        let base = ExportRow {
            name: name.clone(),
            description: row.description.clone(),
            category: category_name(row.category, is_axis).to_string(),
            index: String::new(),
            breakpoints: String::new(),
            axis_value: None,
            value: None,
            text: None,
            unit: row.unit.clone(),
            // A computed parameter's declared address is a placeholder; every
            // VIRTUAL_CHARACTERISTIC in the demo file says 0x0.
            address: (row.category != Category::Virtual).then_some(row.address),
            datatype: row.datatype.clone(),
            conversion: row.conversion.clone(),
            conversion_type: row.conversion_type.clone(),
            presence: presence_name(row.presence).to_string(),
        };

        match row.category {
            // Single-valued shapes: the index column names the shape instead.
            Category::Scalar | Category::Virtual | Category::Unsupported => {
                out.push(ExportRow {
                    index: "Scalar".to_string(),
                    value: row.phys_num,
                    text: row.phys_num.is_none().then(|| row.display.clone()),
                    ..base
                });
            }
            Category::Ascii => {
                out.push(ExportRow {
                    index: "String".to_string(),
                    text: row.text_value.clone().or(Some(row.display.clone())),
                    ..base
                });
            }
            Category::Curve | Category::Map => {
                let Some(detail) = decode::detail_for(db, src, &name) else {
                    out.push(base);
                    continue;
                };
                if detail.values.is_empty() {
                    out.push(base);
                    continue;
                }
                let dims = &detail.dims;
                let one_d = dims.len() <= 1;
                // The table blanks a map's unit — its cell carries a shape
                // rather than a quantity, and "4 x 5 hours" reads as nonsense.
                // Here every row *is* one quantity, so the unit comes straight
                // from the COMPU_METHOD instead of the display-facing field.
                let unit = plan.conv.unit.clone();

                for (i, pt) in detail.values.iter().enumerate() {
                    let subs = subscripts(i as u32, dims);
                    let idx = tuple(
                        &subs.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                    );

                    // The breakpoint on each axis at this position. A shared or
                    // computed axis has points too, so they travel with the
                    // value rather than only existing on their own object.
                    let bps: Vec<String> = subs
                        .iter()
                        .enumerate()
                        .filter_map(|(d, s)| {
                            detail.axes.get(d)?.points.get(*s as usize).map(|p| p.display.clone())
                        })
                        .collect();

                    let axis_value = one_d
                        .then(|| {
                            detail
                                .axes
                                .first()
                                .and_then(|a| a.points.get(i))
                                .map(|p| p.phys)
                        })
                        .flatten();

                    // A verbal point renders a label; the number behind it is a
                    // position in the COMPU_METHOD's table, not a quantity.
                    let verbal = pt.display.trim().parse::<f64>().is_err();

                    out.push(ExportRow {
                        index: idx,
                        unit: unit.clone(),
                        breakpoints: if bps.len() == subs.len() && !bps.is_empty() {
                            tuple(&bps)
                        } else {
                            String::new()
                        },
                        axis_value,
                        value: (!verbal).then_some(pt.phys),
                        text: verbal.then(|| pt.display.clone()),
                        ..base.clone()
                    });
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscripts_run_first_dimension_fastest() {
        // A2L order: (x, y) with x moving first, matching MATRIX_DIM and the
        // order the values are stored in.
        let d = [4, 5];
        assert_eq!(subscripts(0, &d), vec![0, 0]);
        assert_eq!(subscripts(1, &d), vec![1, 0]);
        assert_eq!(subscripts(3, &d), vec![3, 0]);
        assert_eq!(subscripts(4, &d), vec![0, 1]);
        assert_eq!(subscripts(19, &d), vec![3, 4]);
    }

    #[test]
    fn subscripts_extend_to_five_dimensions() {
        let d = [2, 3, 4, 2, 2];
        assert_eq!(subscripts(0, &d), vec![0, 0, 0, 0, 0]);
        assert_eq!(subscripts(1, &d), vec![1, 0, 0, 0, 0]);
        assert_eq!(subscripts(2, &d), vec![0, 1, 0, 0, 0]);
        assert_eq!(subscripts(95, &d), vec![1, 2, 3, 1, 1]);
    }

    #[test]
    fn a_zero_dimension_yields_nothing_rather_than_dividing_by_it() {
        assert_eq!(subscripts(3, &[4, 0]), Vec::<u32>::new());
    }

    #[test]
    fn tuples_read_as_written() {
        assert_eq!(tuple(&["3".into()]), "(3)");
        assert_eq!(tuple(&["2".into(), "1".into()]), "(2,1)");
        assert_eq!(tuple(&["red".into(), "4".into()]), "(red,4)");
    }
}
