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
