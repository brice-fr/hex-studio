// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

//! Turning an edited physical value back into image bytes.
//!
//! Nothing here writes to the image. The caller receives the bytes and applies
//! them through its own edit path, so an A2L edit lands in the same undo
//! history as a hex edit.

use crate::convert::Phys;
use crate::db::{A2lDatabase, ObjectPlan};
use crate::decode::write_element;
use crate::layout::{self, Field};
use crate::model::{ByteSource, Category, EncodedWrite};

/// Turn a raw field value into the bytes to store, preserving any bits of the
/// word that belong to neighbouring BIT_MASK fields.
///
/// Without the read-modify-write, writing one masked field would zero every
/// other field sharing the same word — the demo file has three views of the
/// single UWORD at 0x810002, so editing one would silently destroy the others.
fn bytes_for_raw(
    plan: &ObjectPlan,
    field: Field,
    src: &dyn ByteSource,
    raw: f64,
) -> Result<Vec<u8>, String> {
    if plan.bit_mask == 0 {
        return Ok(write_element(raw, field.datatype, plan.endian));
    }

    let capacity = layout::mask_capacity(plan.bit_mask) as f64;
    if raw.round() < 0.0 || raw.round() > capacity {
        return Err(format!(
            "value needs raw {}, but the masked field 0x{:X} holds 0…{capacity}",
            raw.round(),
            plan.bit_mask
        ));
    }

    let address = plan.address + field.offset;
    let size = layout::datatype_size(field.datatype);
    let current = src
        .read(address, size)
        .and_then(|b| crate::decode::read_element(&b, field.datatype, plan.endian))
        .ok_or_else(|| {
            "the current word is not fully present, so the untouched bits of the \
             masked field cannot be preserved"
                .to_string()
        })?;

    let merged = layout::mask_insert(current, raw, plan.bit_mask, field.datatype);
    Ok(write_element(merged, field.datatype, plan.endian))
}

/// Encode a numeric physical value for a scalar object.
pub fn encode_scalar(
    db: &A2lDatabase,
    src: &dyn ByteSource,
    name: &str,
    phys: f64,
) -> Result<EncodedWrite, String> {
    let plan = db
        .plan_any(name)
        .ok_or_else(|| format!("'{name}' not found in the A2L description"))?;

    if plan.category != Category::Scalar {
        return Err(format!("'{name}' is not a scalar"));
    }
    let field = plan
        .layout
        .fnc
        .ok_or_else(|| format!("'{name}' has no function values"))?;

    let raw = plan
        .conv
        .conversion
        .to_raw(phys)
        .ok_or_else(|| format!("conversion '{}' cannot be inverted", plan.conv.name))?;

    let bytes = bytes_for_raw(&plan, field, src, raw)?;

    // Round-trip through the stored representation so the caller learns the
    // value that will actually be read back — integer raw domains, clamping and
    // masking all mean the request is not always honoured exactly.
    let stored_word = crate::decode::read_element(&bytes, field.datatype, plan.endian)
        .ok_or_else(|| "failed to re-read the encoded value".to_string())?;
    let stored_raw = layout::mask_extract(stored_word, plan.bit_mask, field.datatype);
    let stored_phys = match plan.conv.conversion.to_phys(stored_raw) {
        Phys::Num(v) => v,
        Phys::Text(_) => stored_raw,
        Phys::Unavailable(reason) => return Err(reason),
    };

    Ok(EncodedWrite {
        address: plan.address + field.offset,
        bytes,
        raw: stored_raw,
        phys: stored_phys,
    })
}

/// Which part of an object's point table is being edited.
///
/// The wire form accepts both `"axis"` and `{"axis": 1}`: a one-dimensional
/// caller means the only axis there is, and a map has to say which. Making the
/// dimension mandatory would break every existing caller for the sake of a
/// number that is always zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", try_from = "WireTarget")]
pub enum PointTarget {
    /// A function value, or the points of a standalone AXIS_PTS object.
    Value,
    /// A breakpoint of the axis at this dimension, X being 0. A curve has only
    /// dimension 0; a map addresses its Y breakpoints as `Axis(1)`.
    Axis(usize),
}

/// How a [`PointTarget`] arrives from the frontend.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum WireTarget {
    /// `"value"`, or `"axis"` for the only axis a curve has.
    Named(String),
    /// `{"axis": 1}` — the Y breakpoints of a map.
    Indexed { axis: usize },
}

impl TryFrom<WireTarget> for PointTarget {
    type Error = String;

    fn try_from(w: WireTarget) -> Result<Self, Self::Error> {
        match w {
            WireTarget::Named(n) => match n.as_str() {
                "value" => Ok(PointTarget::Value),
                "axis" => Ok(PointTarget::AXIS),
                other => Err(format!("unknown point target '{other}'")),
            },
            WireTarget::Indexed { axis } => Ok(PointTarget::Axis(axis)),
        }
    }
}

impl PointTarget {
    /// The X axis, which is what a one-dimensional caller means by "the axis".
    pub const AXIS: PointTarget = PointTarget::Axis(0);

    fn is_value(&self) -> bool {
        matches!(self, PointTarget::Value)
    }
}

/// Write one point of a 1D object.
///
/// `index` is the index *as displayed*. An INDEX_DECR axis is stored
/// highest-first while the table shows it ascending, so the index is mapped
/// back to storage here — doing that in the frontend would put every edit on
/// the mirror-image point.
pub fn encode_point(
    db: &A2lDatabase,
    src: &dyn ByteSource,
    name: &str,
    target: PointTarget,
    index: u32,
    phys: f64,
) -> Result<EncodedWrite, String> {
    encode_point_inner(db, src, name, target, index, PointInput::Num(phys))
}

/// Write one point given a label rather than a number.
///
/// A verbal conversion has no numeric inverse — `to_raw` returns None for it —
/// but a label does map back to a raw through `text_to_raw`. Without this the
/// verbal axes and value blocks that `is_invertible` reports as editable would
/// refuse every value they were given.
pub fn encode_point_text(
    db: &A2lDatabase,
    src: &dyn ByteSource,
    name: &str,
    target: PointTarget,
    index: u32,
    text: &str,
) -> Result<EncodedWrite, String> {
    encode_point_inner(db, src, name, target, index, PointInput::Text(text))
}

/// What the caller supplied for a point: a physical value or a label.
#[derive(Clone, Copy)]
enum PointInput<'a> {
    Num(f64),
    Text(&'a str),
}

impl PointInput<'_> {
    /// Invert through `conv`, by the route the input actually has.
    fn to_raw(&self, conv: &crate::convert::Conversion, conv_name: &str) -> Result<f64, String> {
        match self {
            PointInput::Num(v) => conv
                .to_raw(*v)
                .ok_or_else(|| format!("conversion '{conv_name}' cannot be inverted")),
            PointInput::Text(t) => conv
                .text_to_raw(t)
                .ok_or_else(|| format!("'{t}' is not a value of conversion '{conv_name}'")),
        }
    }
}

fn encode_point_inner(
    db: &A2lDatabase,
    src: &dyn ByteSource,
    name: &str,
    target: PointTarget,
    index: u32,
    input: PointInput,
) -> Result<EncodedWrite, String> {
    let plan = db
        .plan_any(name)
        .ok_or_else(|| format!("'{name}' not found in the A2L description"))?;

    if plan.category == Category::Virtual {
        return Err(format!("'{name}' is computed, not stored"));
    }

    let bytes = src
        .read(plan.address, plan.byte_size())
        .ok_or_else(|| format!("'{name}' is not fully present in the image"))?;
    let points = crate::decode::effective_points(&plan, Some(&bytes));

    // A rescale axis interleaves (axis value, index) pairs in one array, so the
    // two columns are the even and odd elements of the same field.
    if let Some(field) = plan.layout.rescale {
        if index >= points {
            return Err(format!("point {index} is past the end ({points} pairs)"));
        }
        let slot = index * 2 + u32::from(target.is_value());
        // The paired index is a position in the virtual axis, not a physical
        // quantity, so only the axis half goes through the conversion.
        let raw = if target.is_value() {
            // The paired element is an index into the virtual full axis, not a
            // physical quantity, so it neither converts nor names anything.
            match input {
                PointInput::Num(v) => v,
                PointInput::Text(_) => {
                    return Err("a rescale index is a position, not a label".to_string())
                }
            }
        } else {
            input.to_raw(&plan.conv.conversion, &plan.conv.name)?
        };
        return element_write(&plan, field, slot, raw, &plan.conv.conversion);
    }

    let dims = crate::decode::effective_dims(&plan, Some(&bytes));

    // Values are indexed across the whole grid; breakpoints only along their
    // own dimension. Each therefore has its own extent and its own mapping
    // from presentation index back to stored slot.
    let (field, conv, extent, slot) = match target {
        PointTarget::Value => {
            let field = plan
                .layout
                .fnc
                .or(plan.layout.axis_pts())
                .ok_or_else(|| format!("'{name}' stores no values"))?;
            (
                field,
                &plan.conv.conversion,
                points,
                plan.storage_slot(index, points),
            )
        }
        PointTarget::Axis(d) => {
            let spec = plan
                .axes
                .get(d)
                .ok_or_else(|| format!("'{name}' has no axis {d}"))?;
            match &spec.source {
                crate::db::AxisSource::Internal => {
                    let field = plan
                        .layout
                        .axes
                        .get(d)
                        .and_then(|a| a.axis_pts)
                        .ok_or_else(|| format!("'{name}' stores no axis points"))?;
                    let extent = dims.get(d).copied().unwrap_or(field.count);
                    (
                        field,
                        spec.conv
                            .as_ref()
                            .map(|c| &c.conversion)
                            .unwrap_or(&plan.conv.conversion),
                        extent,
                        plan.axis_slot(d, index, extent),
                    )
                }
                // A shared axis belongs to another object; edit it there so one
                // write cannot silently change every curve that references it.
                crate::db::AxisSource::AxisPts(r) | crate::db::AxisSource::CurveRef(r) => {
                    return Err(format!("this axis belongs to '{r}' — edit it there"));
                }
                crate::db::AxisSource::Fixed(_) => {
                    return Err("a FIX_AXIS is computed and occupies no bytes".to_string());
                }
                crate::db::AxisSource::None => {
                    return Err(format!("'{name}' has no axis"));
                }
            }
        }
    };

    if index >= extent {
        return Err(format!("point {index} is past the end ({extent} points)"));
    }

    let raw = input.to_raw(conv, &plan.conv.name)?;
    element_write(&plan, field, slot, raw, conv)
}

/// Encode a single array element at `slot`.
fn element_write(
    plan: &ObjectPlan,
    field: Field,
    slot: u32,
    raw: f64,
    conv: &crate::convert::Conversion,
) -> Result<EncodedWrite, String> {
    if slot >= field.count {
        return Err(format!("element {slot} is outside the stored array"));
    }
    let size = layout::datatype_size(field.datatype);
    let address = plan.address + field.offset + slot * size;
    let bytes = write_element(raw, field.datatype, plan.endian);

    // Report what will actually be read back, since the raw domain is coarser
    // than the physical one.
    let stored_raw = crate::decode::read_element(&bytes, field.datatype, plan.endian)
        .ok_or_else(|| "failed to re-read the encoded value".to_string())?;
    let stored_phys = match conv.to_phys(stored_raw) {
        Phys::Num(v) => v,
        _ => stored_raw,
    };

    Ok(EncodedWrite {
        address,
        bytes,
        raw: stored_raw,
        phys: stored_phys,
    })
}

/// Encode a textual value: an enum label for a verbal scalar, or the contents
/// of an ASCII character array.
pub fn encode_text(
    db: &A2lDatabase,
    src: &dyn ByteSource,
    name: &str,
    text: &str,
) -> Result<EncodedWrite, String> {
    let plan = db
        .plan_any(name)
        .ok_or_else(|| format!("'{name}' not found in the A2L description"))?;
    match plan.category {
        Category::Ascii => encode_ascii(db, name, text),
        _ => encode_scalar_text(db, src, name, text),
    }
}

/// Encode an ASCII character array.
///
/// The whole array is rewritten, NUL-padded to its full width. Writing only the
/// new characters would leave the tail of a longer previous string in place —
/// shortening "ASAM Test" to "Hi" would read back as "Hi" followed by "AM Test".
pub fn encode_ascii(db: &A2lDatabase, name: &str, text: &str) -> Result<EncodedWrite, String> {
    let plan = db
        .plan_any(name)
        .ok_or_else(|| format!("'{name}' not found in the A2L description"))?;

    if plan.category != Category::Ascii {
        return Err(format!("'{name}' is not an ASCII string"));
    }
    let field = plan
        .layout
        .fnc
        .ok_or_else(|| format!("'{name}' has no character storage"))?;

    let capacity = field.size() as usize;
    if !text.is_ascii() {
        return Err("only ASCII characters can be stored".to_string());
    }
    if text.bytes().any(|b| !(0x20..0x7F).contains(&b)) {
        return Err("control characters cannot be stored".to_string());
    }
    if text.len() > capacity {
        return Err(format!(
            "'{name}' holds at most {capacity} characters, got {}",
            text.len()
        ));
    }

    let mut bytes = vec![0u8; capacity];
    bytes[..text.len()].copy_from_slice(text.as_bytes());

    Ok(EncodedWrite {
        address: plan.address + field.offset,
        bytes,
        raw: text.len() as f64,
        phys: text.len() as f64,
    })
}

/// Encode a verbal (enumerated) physical value for a scalar object.
pub fn encode_scalar_text(
    db: &A2lDatabase,
    src: &dyn ByteSource,
    name: &str,
    text: &str,
) -> Result<EncodedWrite, String> {
    let plan = db
        .plan_any(name)
        .ok_or_else(|| format!("'{name}' not found in the A2L description"))?;

    if plan.category != Category::Scalar {
        return Err(format!("'{name}' is not a scalar"));
    }
    let field = plan
        .layout
        .fnc
        .ok_or_else(|| format!("'{name}' has no function values"))?;

    let raw = plan
        .conv
        .conversion
        .text_to_raw(text)
        .ok_or_else(|| format!("'{text}' is not a value of '{}'", plan.conv.name))?;

    // An enum can sit in a masked field too, so it takes the same path.
    let bytes = bytes_for_raw(&plan, field, src, raw)?;
    Ok(EncodedWrite {
        address: plan.address + field.offset,
        bytes,
        raw,
        phys: raw,
    })
}

#[cfg(test)]
mod target_tests {
    use super::*;

    /// The frontend sends this value across the Tauri boundary, so its JSON
    /// shape is part of the contract rather than an implementation detail.
    #[test]
    fn point_target_round_trips_through_json() {
        let value = serde_json::to_string(&PointTarget::Value).unwrap();
        let axis0 = serde_json::to_string(&PointTarget::Axis(0)).unwrap();
        let axis1 = serde_json::to_string(&PointTarget::Axis(1)).unwrap();
        println!("Value  -> {value}");
        println!("Axis(0)-> {axis0}");
        println!("Axis(1)-> {axis1}");

        // What the one-dimensional frontend has always sent must keep working;
        // making the dimension mandatory silently broke every axis edit.
        let parse = |s: &str| serde_json::from_str::<PointTarget>(s);
        assert_eq!(parse("\"value\"").unwrap(), PointTarget::Value);
        assert_eq!(parse("\"axis\"").unwrap(), PointTarget::AXIS);
        assert_eq!(parse("{\"axis\":0}").unwrap(), PointTarget::Axis(0));
        assert_eq!(parse("{\"axis\":2}").unwrap(), PointTarget::Axis(2));

        // And everything we emit must come back as itself.
        for t in [PointTarget::Value, PointTarget::Axis(0), PointTarget::Axis(3)] {
            let json = serde_json::to_string(&t).unwrap();
            assert_eq!(parse(&json).unwrap(), t, "round trip of {json}");
        }

        assert!(parse("\"elbow\"").is_err(), "an unknown target must not pass silently");
    }
}
