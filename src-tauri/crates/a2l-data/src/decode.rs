// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

//! Reading raw values out of the image and presenting them physically.

use a2lfile::DataType;

use crate::convert::Phys;
use crate::db::{A2lDatabase, AxisSource, Endian, ObjectPlan};
use crate::layout::{self, Field};
use crate::model::{ByteSource, Category, ObjKind, ParamDetail, ParamRow, Presence, PointValue};

/// Decode one element of `dt` from the front of `bytes`.
pub fn read_element(bytes: &[u8], dt: DataType, endian: Endian) -> Option<f64> {
    let size = layout::datatype_size(dt) as usize;
    if bytes.len() < size {
        return None;
    }
    // Normalise to big-endian order, then interpret.
    let mut buf = bytes[..size].to_vec();
    if endian == Endian::Little {
        buf.reverse();
    }
    let mut acc: u64 = 0;
    for b in &buf {
        acc = (acc << 8) | u64::from(*b);
    }
    Some(match dt {
        DataType::Ubyte => acc as f64,
        DataType::Uword => acc as f64,
        DataType::Ulong => acc as f64,
        DataType::AUint64 => acc as f64,
        DataType::Sbyte => (acc as u8 as i8) as f64,
        DataType::Sword => (acc as u16 as i16) as f64,
        DataType::Slong => (acc as u32 as i32) as f64,
        DataType::AInt64 => (acc as i64) as f64,
        DataType::Float16Ieee => f64::from(f16_from_bits(acc as u16)),
        DataType::Float32Ieee => f64::from(f32::from_bits(acc as u32)),
        DataType::Float64Ieee => f64::from_bits(acc),
    })
}

/// Decode an IEEE half-precision value. Rust has no `f16` on stable, so this
/// expands the 16-bit encoding by hand.
fn f16_from_bits(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0f32 } else { 1.0f32 };
    let exponent = ((bits >> 10) & 0x1F) as i32;
    let mantissa = (bits & 0x03FF) as f32;
    match exponent {
        // Subnormal or zero.
        0 => sign * mantissa * 2f32.powi(-24),
        // Infinity or NaN.
        0x1F => {
            if mantissa == 0.0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => sign * (1.0 + mantissa / 1024.0) * 2f32.powi(exponent - 15),
    }
}

/// Encode one element of `dt` into bytes, saturating at the type's range.
pub fn write_element(value: f64, dt: DataType, endian: Endian) -> Vec<u8> {
    let size = layout::datatype_size(dt) as usize;
    let acc: u64 = match dt {
        DataType::Ubyte => value.round().clamp(0.0, u8::MAX as f64) as u64,
        DataType::Uword => value.round().clamp(0.0, u16::MAX as f64) as u64,
        DataType::Ulong => value.round().clamp(0.0, u32::MAX as f64) as u64,
        DataType::AUint64 => value.round().max(0.0) as u64,
        DataType::Sbyte => {
            (value.round().clamp(i8::MIN as f64, i8::MAX as f64) as i8 as u8) as u64
        }
        DataType::Sword => {
            (value.round().clamp(i16::MIN as f64, i16::MAX as f64) as i16 as u16) as u64
        }
        DataType::Slong => {
            (value.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32 as u32) as u64
        }
        DataType::AInt64 => (value.round() as i64) as u64,
        DataType::Float16Ieee => u64::from(f16_to_bits(value as f32)),
        DataType::Float32Ieee => u64::from((value as f32).to_bits()),
        DataType::Float64Ieee => value.to_bits(),
    };
    // Emit big-endian, then flip if the target is little-endian.
    let mut out: Vec<u8> = (0..size)
        .rev()
        .map(|i| ((acc >> (i * 8)) & 0xFF) as u8)
        .collect();
    if endian == Endian::Little {
        out.reverse();
    }
    out
}

/// Encode an IEEE half-precision value, rounding toward zero on the mantissa.
fn f16_to_bits(v: f32) -> u16 {
    if v.is_nan() {
        return 0x7E00;
    }
    let sign: u16 = if v.is_sign_negative() { 0x8000 } else { 0 };
    let a = v.abs();
    if a.is_infinite() {
        return sign | 0x7C00;
    }
    if a == 0.0 {
        return sign;
    }
    let exp = a.log2().floor() as i32;
    if exp < -14 {
        // Subnormal range.
        let mant = (a / 2f32.powi(-24)).round() as u16;
        return sign | (mant & 0x03FF);
    }
    if exp > 15 {
        return sign | 0x7C00;
    }
    let mant = ((a / 2f32.powi(exp) - 1.0) * 1024.0).round() as u16;
    sign | (((exp + 15) as u16) << 10) | (mant & 0x03FF)
}

/// Read every element of a field.
fn read_field(bytes: &[u8], field: Field, endian: Endian) -> Vec<f64> {
    let size = layout::datatype_size(field.datatype) as usize;
    let start = field.offset as usize;
    (0..field.count as usize)
        .filter_map(|i| {
            let from = start + i * size;
            bytes.get(from..from + size)?;
            read_element(&bytes[from..], field.datatype, endian)
        })
        .collect()
}

/// How much of an object exists in the image.
pub fn presence_of(src: &dyn ByteSource, plan: &ObjectPlan) -> Presence {
    let size = plan.byte_size();
    if size == 0 {
        // A zero extent means the record layout could not be resolved, not
        // that the image is missing anything.
        return Presence::Unknown;
    }
    let present = src.present_count(plan.address, size);
    if present == 0 {
        Presence::Absent
    } else if present == size {
        Presence::Full
    } else {
        Presence::Partial
    }
}

/// Number of decimals to show, taken from an A2L FORMAT string such as `%8.3`.
fn decimals_from_format(fmt: &str) -> Option<usize> {
    let (_, frac) = fmt.split_once('.')?;
    let digits: String = frac.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Render a number, honouring the A2L format when it gives a precision.
pub fn format_number(v: f64, fmt: &str) -> String {
    if let Some(d) = decimals_from_format(fmt) {
        return format!("{v:.*}", d);
    }
    if v == v.trunc() && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    // Trim trailing zeros from a fixed rendering rather than risk exponent form.
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Render raw bytes as uppercase hex.
fn hex_of(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Same, but elided after `max` bytes — a 42-character string would otherwise
/// produce a wall of hex in the detail pane.
fn hex_of_capped(bytes: &[u8], max: usize) -> String {
    if bytes.len() <= max {
        return hex_of(bytes);
    }
    format!("{} …", hex_of(&bytes[..max]))
}

/// The physical increment of one raw LSB, used to step a slider so it can only
/// land on values the field is able to store.
///
/// Integer fields have a real LSB, so the step is the physical distance between
/// two adjacent raw values — 1 for an IDENTICAL byte, 0.1 for a RAT_FUNC that
/// divides by ten. Floats have no meaningful LSB at this scale, and neither do
/// conversions that fail or flatten, so the declared range is subdivided instead.
fn phys_step_for(plan: &ObjectPlan, raw: f64, dt: DataType) -> Option<f64> {
    let span = plan.upper_limit - plan.lower_limit;
    let fallback = (span.is_finite() && span > 0.0).then(|| span / 1000.0);

    if layout::is_float(dt) {
        return fallback;
    }
    match (
        plan.conv.conversion.to_phys(raw),
        plan.conv.conversion.to_phys(raw + 1.0),
    ) {
        (Phys::Num(a), Phys::Num(b)) => {
            let step = (b - a).abs();
            if step.is_finite() && step > 0.0 {
                Some(step)
            } else {
                fallback
            }
        }
        _ => fallback,
    }
}

/// Guards against a formula chain that never terminates, alongside the cycle
/// check — a malformed file could nest references arbitrarily deep.
const MAX_FORMULA_DEPTH: usize = 16;

/// The physical value of a stored scalar, for use as a formula input.
fn scalar_phys(src: &dyn ByteSource, plan: &ObjectPlan) -> Option<f64> {
    let field = plan.layout.fnc?;
    let size = layout::datatype_size(field.datatype);
    let bytes = src.read(plan.address + field.offset, size)?;
    let stored = read_element(&bytes, field.datatype, plan.endian)?;
    let raw = layout::mask_extract(stored, plan.bit_mask, field.datatype);
    match plan.conv.conversion.to_phys(raw) {
        Phys::Num(v) => Some(v),
        _ => None,
    }
}

/// Evaluate a VIRTUAL_CHARACTERISTIC.
///
/// Inputs bind to `X1`, `X2`, … in declaration order and may themselves be
/// virtual — the demo file's REF_3 reads two other computed parameters — so
/// this recurses, tracking the chain to refuse a circular definition rather
/// than looping until the stack gives out.
fn eval_computed(
    db: &A2lDatabase,
    src: &dyn ByteSource,
    plan: &ObjectPlan,
    chain: &mut Vec<String>,
) -> Result<f64, String> {
    if chain.iter().any(|n| n == &plan.name) {
        return Err(format!("'{}' refers to itself", plan.name));
    }
    if chain.len() >= MAX_FORMULA_DEPTH {
        return Err("formula references nest too deeply".to_string());
    }

    let text = plan
        .virtual_formula
        .as_deref()
        .ok_or_else(|| "no formula".to_string())?;
    let formula = crate::formula::Formula::parse(text)?;

    chain.push(plan.name.clone());
    let mut vars = Vec::with_capacity(plan.virtual_inputs.len());
    for input in &plan.virtual_inputs {
        let ip = db
            .plan_any(input)
            .ok_or_else(|| format!("input '{input}' not found"))?;
        let value = if ip.category == Category::Virtual {
            eval_computed(db, src, &ip, chain)?
        } else {
            scalar_phys(src, &ip).ok_or_else(|| format!("input '{input}' has no value"))?
        };
        vars.push(value);
    }
    chain.pop();

    formula.eval(&crate::formula::Context {
        vars: &vars,
        constants: db.system_constants(),
    })
}

/// What a fixed-width character array holds.
struct AsciiField {
    /// Text up to the first NUL, with non-printable bytes shown as `.`.
    text: String,
    /// Total bytes of the array.
    capacity: u32,
    /// Longest string the field accepts.
    max_len: u32,
    /// False when the text contains bytes outside printable ASCII, in which
    /// case editing would silently rewrite them and is refused.
    printable: bool,
}

fn decode_ascii(bytes: &[u8]) -> AsciiField {
    let capacity = bytes.len() as u32;
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    let used = &bytes[..end];
    let printable = used.iter().all(|b| (0x20..0x7F).contains(b));
    let text = used
        .iter()
        .map(|b| if (0x20..0x7F).contains(b) { *b as char } else { '.' })
        .collect();

    // A NUL anywhere in the array means it is being used as a C string, so one
    // byte stays reserved for the terminator. An array with no NUL at all is a
    // fixed-width field that may be filled edge to edge.
    let max_len = if bytes.contains(&0) {
        capacity.saturating_sub(1)
    } else {
        capacity
    };

    AsciiField {
        text,
        capacity,
        max_len,
        printable,
    }
}

/// The points in use along one dimension: the stored count when the layout has
/// a NO_AXIS_PTS field for it, clamped to the allocation.
pub(crate) fn effective_dim(plan: &ObjectPlan, d: usize, bytes: Option<&[u8]>) -> u32 {
    let declared = plan.dims.get(d).copied().unwrap_or(plan.declared_points);
    let field = plan.layout.axes.get(d).and_then(|a| a.no_axis_pts);
    let (Some(field), Some(bytes)) = (field, bytes) else {
        return declared;
    };
    let start = field.offset as usize;
    match bytes
        .get(start..)
        .and_then(|b| read_element(b, field.datatype, plan.endian))
    {
        Some(n) if n >= 0.0 => (n as u32).min(declared),
        _ => declared,
    }
}

/// Points in use along every dimension.
pub(crate) fn effective_dims(plan: &ObjectPlan, bytes: Option<&[u8]>) -> Vec<u32> {
    (0..plan.dims.len().max(1))
        .map(|d| effective_dim(plan, d, bytes))
        .collect()
}

/// The total element count in use — the product across dimensions.
pub(crate) fn effective_points(plan: &ObjectPlan, bytes: Option<&[u8]>) -> u32 {
    effective_dims(plan, bytes).iter().product()
}

/// Build one table row for an object.
pub fn row_for(db: &A2lDatabase, src: &dyn ByteSource, plan: &ObjectPlan) -> ParamRow {
    let presence = presence_of(src, plan);
    let bytes = if presence == Presence::Full {
        src.read(plan.address, plan.byte_size())
    } else {
        None
    };
    let fmt = plan.format().to_string();

    let mut raw_hex = None;
    let mut display = String::from("—");
    let mut phys_num = None;
    let mut phys_step = None;
    let mut phys_min = None;
    let mut phys_max = None;
    let mut point_count = None;
    let mut text_value = None;
    let mut text_capacity = None;
    let mut text_max_len = None;
    let mut ascii_printable = false;
    let dims = effective_dims(plan, bytes.as_deref());

    match plan.category {
        Category::Scalar => {
            if let (Some(bytes), Some(field)) = (&bytes, plan.layout.fnc) {
                let size = layout::datatype_size(field.datatype) as usize;
                let from = field.offset as usize;
                if let Some(slice) = bytes.get(from..from + size) {
                    raw_hex = Some(hex_of(slice));
                    if let Some(stored) = read_element(slice, field.datatype, plan.endian) {
                        // BIT_MASK selects bits of the stored word and shifts
                        // them down; the conversion applies to that field, not
                        // to the whole word.
                        let raw = layout::mask_extract(stored, plan.bit_mask, field.datatype);
                        match plan.conv.conversion.to_phys(raw) {
                            Phys::Num(v) => {
                                display = format_number(v, &fmt);
                                phys_num = Some(v);
                                phys_step = phys_step_for(plan, raw, field.datatype);
                            }
                            Phys::Text(t) => display = t,
                            Phys::Unavailable(_) => display = "—".into(),
                        }
                    }
                }
            } else if presence == Presence::Absent {
                display = "absent".into();
            }
        }

        // A rescale axis stores (axis value, index) pairs rather than plain
        // points, so it is summarised by its pair count and the span of the
        // axis values — the first of each pair.
        Category::Curve if plan.layout.rescale.is_some() => {
            let field = plan.layout.rescale.expect("checked by the guard");
            let n = effective_points(plan, bytes.as_deref());
            point_count = Some(n);
            if let Some(bytes) = &bytes {
                let used = Field {
                    count: (n * 2).min(field.count),
                    ..field
                };
                let raws = read_field(bytes, used, plan.endian);
                let axis_phys: Vec<f64> = raws
                    .iter()
                    .step_by(2)
                    .filter_map(|r| match plan.conv.conversion.to_phys(*r) {
                        Phys::Num(v) => Some(v),
                        _ => None,
                    })
                    .collect();
                if axis_phys.is_empty() {
                    display = format!("{n} pairs");
                } else {
                    let lo = axis_phys.iter().cloned().fold(f64::INFINITY, f64::min);
                    let hi = axis_phys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    phys_min = Some(lo);
                    phys_max = Some(hi);
                    display = format!(
                        "{} … {}",
                        format_number(lo, &fmt),
                        format_number(hi, &fmt)
                    );
                }
            } else if presence == Presence::Absent {
                display = "absent".into();
            } else {
                display = format!("{n} pairs");
            }
        }

        // A map is identified by its shape, not by a value span. The lowest and
        // highest cell of a whole grid say very little about it, whereas
        // "4 × 5" says what it is at a glance — and leaving the span unset
        // keeps the decimals stepper from reformatting the shape as a range.
        Category::Map => {
            let n = effective_points(plan, bytes.as_deref());
            point_count = Some(n);
            display = if presence == Presence::Absent {
                "absent".into()
            } else {
                shape_label(&dims, n)
            };
        }

        Category::Curve => {
            let n = effective_points(plan, bytes.as_deref());
            point_count = Some(n);
            if let (Some(bytes), Some(field)) = (&bytes, plan.layout.fnc.or(plan.layout.axis_pts())) {
                let used = Field { count: n, ..field };
                let values = read_field(bytes, used, plan.endian);
                let phys: Vec<f64> = values
                    .iter()
                    .filter_map(|r| match plan.conv.conversion.to_phys(*r) {
                        Phys::Num(v) => Some(v),
                        _ => None,
                    })
                    .collect();
                if phys.is_empty() {
                    display = shape_label(&dims, n);
                } else {
                    let lo = phys.iter().cloned().fold(f64::INFINITY, f64::min);
                    let hi = phys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    phys_min = Some(lo);
                    phys_max = Some(hi);
                    display = format!(
                        "{} … {}",
                        format_number(lo, &fmt),
                        format_number(hi, &fmt)
                    );
                }
            } else if presence == Presence::Absent {
                display = "absent".into();
            } else {
                display = shape_label(&dims, n);
            }
        }

        Category::Ascii => {
            if let (Some(bytes), Some(field)) = (&bytes, plan.layout.fnc) {
                let from = field.offset as usize;
                let len = field.size() as usize;
                if let Some(slice) = bytes.get(from..from + len) {
                    raw_hex = Some(hex_of_capped(slice, 12));
                    let ascii = decode_ascii(slice);
                    display = if ascii.text.is_empty() {
                        "(empty)".into()
                    } else {
                        ascii.text.clone()
                    };
                    text_value = Some(ascii.text);
                    text_capacity = Some(ascii.capacity);
                    text_max_len = Some(ascii.max_len);
                    ascii_printable = ascii.printable;
                }
            } else if presence == Presence::Absent {
                display = "absent".into();
            }
        }

        // Computed from other parameters rather than read from the image.
        // Falls back to showing the formula when it cannot be evaluated, which
        // is more use than a bare dash.
        Category::Virtual => match eval_computed(db, src, plan, &mut Vec::new()) {
            Ok(v) => {
                display = format_number(v, &fmt);
                phys_num = Some(v);
            }
            Err(_) => {
                display = plan
                    .virtual_formula
                    .clone()
                    .unwrap_or_else(|| "computed".to_string());
            }
        },

        Category::Unsupported => {
            if presence == Presence::Absent {
                display = "absent".into();
            }
        }
    }

    // Editing always needs real bytes; what else it needs depends on the shape.
    let editable = presence == Presence::Full
        && plan.kind != ObjKind::Measurement
        && match plan.category {
            Category::Scalar => plan.conv.conversion.is_invertible(),
            Category::Ascii => ascii_printable,
            _ => false,
        };

    let note = plan.note.clone().or_else(|| {
        if presence != Presence::Full {
            return None;
        }
        match plan.category {
            Category::Scalar if !plan.conv.conversion.is_invertible() => {
                Some("conversion is not invertible — read only".to_string())
            }
            // Masking is applied per scalar value; an array of masked fields is
            // rare and not handled, so say so rather than show wrong numbers.
            Category::Curve if plan.bit_mask != 0 => {
                Some("BIT_MASK on an array is not applied".to_string())
            }
            // Rewriting bytes we had to render as '.' would destroy them.
            Category::Ascii if !ascii_printable => {
                Some("contains non-printable bytes — read only".to_string())
            }
            _ => None,
        }
    });

    ParamRow {
        name: plan.name.clone(),
        description: plan.description.clone(),
        kind: plan.kind,
        category: plan.category,
        address: plan.address,
        byte_size: plan.byte_size(),
        datatype: plan
            .datatype()
            .or_else(|| plan.layout.axis_pts().map(|f| f.datatype))
            .map(layout::datatype_name)
            .unwrap_or("—")
            .to_string(),
        presence,
        unit: plan.display_unit().to_string(),
        conversion: plan.conv.name.clone(),
        conversion_type: plan.conv.type_name.to_string(),
        raw_hex,
        display,
        phys_num,
        phys_step,
        phys_min,
        phys_max,
        enum_options: plan.conv.conversion.enum_options(),
        formula: plan.virtual_formula.clone(),
        depends_on: (!plan.virtual_inputs.is_empty()).then(|| plan.virtual_inputs.clone()),
        text_value,
        text_capacity,
        text_max_len,
        point_count,
        dims,
        lower_limit: plan.lower_limit,
        upper_limit: plan.upper_limit,
        editable,
        note,
    }
}

/// Build every row, in A2L declaration order.
pub fn list_rows(
    db: &A2lDatabase,
    src: &dyn ByteSource,
    include_measurements: bool,
) -> Vec<ParamRow> {
    db.object_names(include_measurements)
        .into_iter()
        .filter_map(|(name, kind)| db.plan(&name, kind))
        .map(|plan| row_for(db, src, &plan))
        .collect()
}

/// Full axis and value arrays for one 1D object.
/// Reorder a storage-order array into presentation order.
///
/// `dims` is the shape actually in use, which for a layout carrying a
/// NO_AXIS_PTS field can be smaller than the declared allocation.
fn gather<T: Clone>(stored: Vec<T>, plan: &ObjectPlan, dims: &[u32]) -> Vec<T> {
    let n = stored.len() as u32;
    if dims.iter().product::<u32>() != n {
        // The shape does not describe what was read; leaving it in storage
        // order is wrong but recoverable, whereas indexing past the end is not.
        return stored;
    }
    (0..n)
        .map(|i| stored[plan.storage_slot(i, n) as usize].clone())
        .collect()
}

/// Whether a declared shape describes a real grid rather than a flat run that
/// merely spells out its unused dimensions.
pub(crate) fn is_grid(dims: &[u32]) -> bool {
    dims.iter().filter(|d| **d > 1).count() > 1
}

/// How an object's extent is labelled: `4 × 5` for a real grid, a point count
/// for anything that is a flat run however it was declared.
///
/// A shape is only worth spelling out when two or more dimensions actually
/// extend. `MATRIX_DIM 1 8 1` describes eight values in a column, not a
/// 1 × 8 × 1 volume, and reading it as one tells the user nothing they want.
fn shape_label(dims: &[u32], n: u32) -> String {
    if is_grid(dims) {
        dims.iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(" × ")
    } else {
        format!("{n} pts")
    }
}

/// Present an array in the opposite order when its axis is stored INDEX_DECR.
fn reverse_if<T>(v: Vec<T>, reversed: bool) -> Vec<T> {
    if reversed {
        v.into_iter().rev().collect()
    } else {
        v
    }
}

pub fn detail_for(db: &A2lDatabase, src: &dyn ByteSource, name: &str) -> Option<ParamDetail> {
    let plan = db.plan_any(name)?;
    let size = plan.byte_size();
    let bytes = src.read(plan.address, size).unwrap_or_default();
    let readable = if bytes.is_empty() { None } else { Some(bytes.as_slice()) };
    let dims = effective_dims(&plan, readable);
    let n: u32 = dims.iter().product();
    let fmt = plan.format().to_string();
    // Points can only be written when the whole object is present — a partial
    // read would put an edit at an address whose neighbours are unknown.
    let editable_points =
        presence_of(src, &plan) == Presence::Full && plan.category != Category::Virtual;

    let to_points = |raws: Vec<f64>, conv: &crate::convert::Conversion, fmt: &str| -> Vec<PointValue> {
        raws.into_iter()
            .map(|raw| match conv.to_phys(raw) {
                Phys::Num(v) => PointValue {
                    raw,
                    phys: v,
                    display: format_number(v, fmt),
                },
                Phys::Text(t) => PointValue {
                    raw,
                    phys: raw,
                    display: t,
                },
                Phys::Unavailable(_) => PointValue {
                    raw,
                    phys: raw,
                    display: "—".into(),
                },
            })
            .collect()
    };

    // Read a referenced object's array — used when the axis lives in a separate
    // AXIS_PTS object or in another curve.
    let points_from = |p: &crate::db::ObjectPlan, field: Option<Field>| -> Vec<PointValue> {
        let Some(field) = field else { return Vec::new() };
        let b = src.read(p.address, p.byte_size()).unwrap_or_default();
        if b.is_empty() {
            return Vec::new();
        }
        let count = effective_points(p, Some(&b));
        // Storage order: the caller reverses axis and values together.
        let raws = read_field(&b, Field { count, ..field }, p.endian);
        to_points(raws, &p.conv.conversion, &p.conv.format)
    };

    // A rescale axis is a list of (axis value, index) pairs. Presenting the
    // axis value against its index is the honest reading: the second element
    // is a position in the virtual full axis, not a physical quantity, so it
    // deliberately skips the conversion.
    if let Some(field) = plan.layout.rescale {
        let used = Field {
            count: (n * 2).min(field.count),
            ..field
        };
        let raws = read_field(&bytes, used, plan.endian);
        let axis = to_points(
            raws.iter().step_by(2).copied().collect(),
            &plan.conv.conversion,
            &fmt,
        );
        let values = to_points(
            raws.iter().skip(1).step_by(2).copied().collect(),
            &crate::convert::Conversion::Identical,
            "",
        );
        let pairs = axis.len() as u32;
        return Some(ParamDetail {
            name: plan.name.clone(),
            description: plan.description.clone(),
            address: plan.address,
            byte_size: size,
            axes: vec![crate::model::AxisDetail {
                points: axis.clone(),
                unit: plan.display_unit().to_string(),
                kind: "RES_AXIS".to_string(),
                reference: None,
                editable: editable_points,
            }],
            dims: vec![pairs],
            axis,
            values,
            axis_unit: plan.display_unit().to_string(),
            value_unit: "index".to_string(),
            axis_kind: "RES_AXIS".to_string(),
            axis_ref: None,
            values_editable: editable_points,
            axis_editable: editable_points,
            bytes,
        });
    }

    // Function values.
    let values = match plan.layout.fnc.or(plan.layout.axis_pts()) {
        Some(field) if !bytes.is_empty() => {
            let used = Field { count: n, ..field };
            to_points(read_field(&bytes, used, plan.endian), &plan.conv.conversion, &fmt)
        }
        _ => Vec::new(),
    };

    // Axis breakpoints, one dimension at a time, from wherever each keeps them.
    let mut axes: Vec<crate::model::AxisDetail> = Vec::with_capacity(plan.axes.len());
    for (d, spec) in plan.axes.iter().enumerate() {
        let conv = spec
            .conv
            .as_ref()
            .map(|c| c.conversion.clone())
            .unwrap_or(crate::convert::Conversion::Identical);
        let axis_fmt = spec.conv.as_ref().map(|c| c.format.clone()).unwrap_or_default();
        let count = dims.get(d).copied().unwrap_or(0);

        let stored = match &spec.source {
            AxisSource::Internal => {
                match plan.layout.axes.get(d).and_then(|a| a.axis_pts) {
                    Some(field) if !bytes.is_empty() => {
                        let used = Field { count, ..field };
                        to_points(read_field(&bytes, used, plan.endian), &conv, &axis_fmt)
                    }
                    _ => Vec::new(),
                }
            }
            // A shared axis lives in its own AXIS_PTS object, where the
            // breakpoints are the axis field.
            AxisSource::AxisPts(axis_name) => db
                .plan_axis_pts(axis_name)
                .map(|ap| {
                    let field = ap.layout.axis_pts().or(ap.layout.fnc);
                    points_from(&ap, field)
                })
                .unwrap_or_default(),
            // CURVE_AXIS borrows another characteristic's *function* values as
            // its breakpoints, so the preferred field is the other way round.
            AxisSource::CurveRef(curve_name) => db
                .plan_characteristic(curve_name)
                .map(|cv| {
                    let field = cv.layout.fnc.or(cv.layout.axis_pts());
                    points_from(&cv, field)
                })
                .unwrap_or_default(),
            AxisSource::Fixed(values) => to_points(values.clone(), &conv, &axis_fmt),
            AxisSource::None => Vec::new(),
        };

        // Only breakpoints held in this object's own record can be written
        // here; a shared one is edited on its own object, a computed one not
        // at all.
        let editable = editable_points
            && matches!(spec.source, AxisSource::Internal)
            && conv.is_invertible();

        axes.push(crate::model::AxisDetail {
            points: reverse_if(stored, plan.dim_reversed(d)),
            unit: spec.conv.as_ref().map(|c| c.unit.clone()).unwrap_or_default(),
            kind: spec.kind.to_string(),
            reference: spec.source.reference().map(str::to_string),
            editable,
        });
    }

    // Function values were read in storage order. An INDEX_DECR axis is stored
    // highest-first and its values sit alongside it element by element, so
    // presenting the axis ascending drags the values with it — the CDFX for
    // this file pairs axis -5 with value -3, the last stored element of each,
    // and reversing only the axis mispairs every point. A COLUMN_DIR grid is
    // likewise stored transposed. `storage_slot` knows about both, and about
    // the two combined, so gathering through it covers every case.
    let values = gather(values, &plan, &dims);
    let axis = axes.first().map(|a| a.points.clone()).unwrap_or_default();

    Some(ParamDetail {
        name: plan.name.clone(),
        description: plan.description.clone(),
        address: plan.address,
        byte_size: size,
        // The X axis is mirrored out of `axes` so the one-dimensional point
        // table can keep reading it directly.
        axis_unit: axes.first().map(|a| a.unit.clone()).unwrap_or_default(),
        axis_kind: axes.first().map(|a| a.kind.clone()).unwrap_or_default(),
        axis_ref: axes.first().and_then(|a| a.reference.clone()),
        axis_editable: axes.first().map(|a| a.editable).unwrap_or(false),
        axis,
        values,
        dims,
        axes,
        value_unit: plan.display_unit().to_string(),
        values_editable: editable_points && plan.conv.conversion.is_invertible(),
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_is_only_spelled_out_when_it_is_really_a_grid() {
        assert_eq!(shape_label(&[4, 5], 20), "4 × 5");
        assert_eq!(shape_label(&[2, 3, 4], 24), "2 × 3 × 4");
        // ASAP2 <= 1.6 pads a plain array out to three dimensions; a column
        // orientation puts the extent second. Neither is a grid.
        assert_eq!(shape_label(&[8, 1, 1], 8), "8 pts");
        assert_eq!(shape_label(&[1, 8, 1], 8), "8 pts");
        assert_eq!(shape_label(&[8], 8), "8 pts");
        assert_eq!(shape_label(&[1, 1, 1], 1), "1 pts");
    }

    #[test]
    fn reads_unsigned_little_endian() {
        assert_eq!(
            read_element(&[0xB8, 0x0B], DataType::Uword, Endian::Little),
            Some(3000.0)
        );
    }

    #[test]
    fn reads_unsigned_big_endian() {
        assert_eq!(
            read_element(&[0x0B, 0xB8], DataType::Uword, Endian::Big),
            Some(3000.0)
        );
    }

    #[test]
    fn reads_signed_negatives() {
        assert_eq!(
            read_element(&[0xFB], DataType::Sbyte, Endian::Little),
            Some(-5.0)
        );
        assert_eq!(
            read_element(&[0xFD, 0xFF], DataType::Sword, Endian::Little),
            Some(-3.0)
        );
    }

    #[test]
    fn reads_floats() {
        let bits = 1.5f32.to_bits().to_le_bytes();
        assert_eq!(
            read_element(&bits, DataType::Float32Ieee, Endian::Little),
            Some(1.5)
        );
    }

    #[test]
    fn refuses_a_short_slice() {
        assert_eq!(read_element(&[0x01], DataType::Uword, Endian::Little), None);
    }

    #[test]
    fn write_element_round_trips_through_read() {
        for dt in [
            DataType::Ubyte,
            DataType::Sbyte,
            DataType::Uword,
            DataType::Sword,
            DataType::Ulong,
            DataType::Slong,
            DataType::Float32Ieee,
            DataType::Float64Ieee,
        ] {
            for endian in [Endian::Little, Endian::Big] {
                for v in [0.0f64, 1.0, -1.0, 100.0] {
                    if v < 0.0 && !layout::is_signed(dt) && !layout::is_float(dt) {
                        continue;
                    }
                    let bytes = write_element(v, dt, endian);
                    assert_eq!(bytes.len(), layout::datatype_size(dt) as usize);
                    let back = read_element(&bytes, dt, endian).expect("readable");
                    assert!((back - v).abs() < 1e-9, "{dt:?} {endian:?} {v} -> {back}");
                }
            }
        }
    }

    #[test]
    fn write_element_saturates_instead_of_wrapping() {
        // 300 does not fit a UBYTE; clamping to 255 is far safer than the
        // silent wrap to 44 that a cast would give.
        assert_eq!(write_element(300.0, DataType::Ubyte, Endian::Little), vec![255]);
        assert_eq!(write_element(-5.0, DataType::Ubyte, Endian::Little), vec![0]);
    }

    #[test]
    fn half_precision_round_trips() {
        for v in [0.0f32, 1.0, -2.5, 100.0] {
            let bits = f16_to_bits(v);
            assert!((f16_from_bits(bits) - v).abs() < 1e-2, "{v}");
        }
    }

    #[test]
    fn format_honours_the_a2l_precision() {
        assert_eq!(format_number(1.5, "%8.3"), "1.500");
        assert_eq!(format_number(1.0, "%3.0"), "1");
        assert_eq!(format_number(20.0, ""), "20");
        assert_eq!(format_number(1.25, ""), "1.25");
    }

    #[test]
    fn ascii_reads_up_to_the_first_nul() {
        // "Hi" then padding, as the demo file stores its strings.
        let a = decode_ascii(&[b'H', b'i', 0, 0, 0, 0]);
        assert_eq!(a.text, "Hi");
        assert_eq!(a.capacity, 6);
        assert!(a.printable);
    }

    /// A NUL anywhere means the array is a C string, so a byte is reserved.
    #[test]
    fn ascii_reserves_a_byte_when_nul_terminated() {
        let a = decode_ascii(&[b'H', b'i', 0, 0]);
        assert_eq!(a.capacity, 4);
        assert_eq!(a.max_len, 3, "one byte kept for the terminator");
    }

    /// With no NUL at all the array is a fixed-width field and may be filled.
    #[test]
    fn ascii_allows_the_full_width_when_not_terminated() {
        let a = decode_ascii(&[b'A', b'B', b'C', b'D']);
        assert_eq!(a.text, "ABCD");
        assert_eq!(a.capacity, 4);
        assert_eq!(a.max_len, 4, "no terminator in use, so no byte reserved");
    }

    #[test]
    fn ascii_all_nuls_is_an_empty_string() {
        let a = decode_ascii(&[0, 0, 0]);
        assert_eq!(a.text, "");
        assert_eq!(a.max_len, 2);
        assert!(a.printable);
    }

    #[test]
    fn ascii_flags_non_printable_content() {
        // A control byte before the NUL cannot be round-tripped through a text
        // field, so the row must be reported as non-printable.
        let a = decode_ascii(&[b'A', 0x07, b'B', 0]);
        assert!(!a.printable);
        assert_eq!(a.text, "A.B", "shown with a placeholder");
    }

    #[test]
    fn hex_is_elided_past_the_cap() {
        assert_eq!(hex_of_capped(&[1, 2], 4), "01 02");
        assert_eq!(hex_of_capped(&[1, 2, 3, 4, 5], 3), "01 02 03 …");
    }

    #[test]
    fn decimals_are_parsed_from_the_format() {
        assert_eq!(decimals_from_format("%8.3"), Some(3));
        assert_eq!(decimals_from_format("%6.1"), Some(1));
        assert_eq!(decimals_from_format("%12"), None);
        assert_eq!(decimals_from_format(""), None);
    }
}
