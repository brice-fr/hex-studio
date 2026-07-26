// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

//! End-to-end checks against the ASAM ASAP2 demo description and its matching
//! firmware image.
//!
//! The demo pair is ASAM-licensed and not vendored into this repository, so
//! every test here skips when the files are absent. Set `A2L_DEMO_DIR` to point
//! at a directory holding `ASAP2_Demo_V171.a2l` and `ASAP2_Demo_V171.hex`.

use std::collections::BTreeMap;

use a2l_data::model::{ByteSource, Category, Presence};
use a2l_data::{decode, stats, A2lDatabase};

const DEFAULT_DIR: &str = "/Users/brice-dev/Downloads/ECU_Description";

fn demo_dir() -> Option<String> {
    let dir = std::env::var("A2L_DEMO_DIR").unwrap_or_else(|_| DEFAULT_DIR.to_string());
    let a2l = format!("{dir}/ASAP2_Demo_V171.a2l");
    if std::path::Path::new(&a2l).exists() {
        Some(dir)
    } else {
        eprintln!("skipping: demo A2L not found at {a2l}");
        None
    }
}

/// A sparse image backed by a map, which is all `ByteSource` really needs.
struct MapImage(BTreeMap<u32, u8>);

impl ByteSource for MapImage {
    fn read(&self, addr: u32, len: u32) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(len as usize);
        for i in 0..len {
            out.push(*self.0.get(&(addr + i))?);
        }
        Some(out)
    }

    fn present_count(&self, addr: u32, len: u32) -> u32 {
        (0..len).filter(|i| self.0.contains_key(&(addr + i))).count() as u32
    }

    fn total_bytes(&self) -> u64 {
        self.0.len() as u64
    }
}

/// Minimal Intel HEX reader: enough for the demo image's data and
/// extended-linear-address records.
fn load_ihex(path: &str) -> MapImage {
    let text = std::fs::read_to_string(path).expect("read hex file");
    let mut mem = BTreeMap::new();
    let mut upper: u32 = 0;
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with(':') || line.len() < 11 {
            continue;
        }
        let bytes: Vec<u8> = (1..line.len() - 1)
            .step_by(2)
            .filter_map(|i| u8::from_str_radix(&line[i..i + 2], 16).ok())
            .collect();
        if bytes.len() < 4 {
            continue;
        }
        let len = bytes[0] as usize;
        let offset = ((bytes[1] as u32) << 8) | bytes[2] as u32;
        let rectype = bytes[3];
        let data = &bytes[4..(4 + len).min(bytes.len())];
        match rectype {
            0 => {
                let base = upper | offset;
                for (i, b) in data.iter().enumerate() {
                    mem.insert(base + i as u32, *b);
                }
            }
            4 => {
                if data.len() >= 2 {
                    upper = (((data[0] as u32) << 8) | data[1] as u32) << 16;
                }
            }
            _ => {}
        }
    }
    MapImage(mem)
}

fn open_demo() -> Option<(A2lDatabase, MapImage)> {
    let dir = demo_dir()?;
    let db = A2lDatabase::load(&format!("{dir}/ASAP2_Demo_V171.a2l")).expect("parse demo A2L");
    let img = load_ihex(&format!("{dir}/ASAP2_Demo_V171.hex"));
    Some((db, img))
}

#[test]
fn parses_the_expected_object_counts() {
    let Some((db, _)) = open_demo() else { return };
    let s = db.summary();
    assert_eq!(s.characteristic_count, 87);
    assert_eq!(s.axis_pts_count, 2);
    assert_eq!(s.measurement_count, 31);
    assert_eq!(s.compu_method_count, 19);
    assert_eq!(s.record_layout_count, 28);
    assert_eq!(s.asap2_version.as_deref(), Some("1.71"));
}

/// An IDENTICAL/UBYTE scalar is the one case where the physical value must
/// equal the byte visible in a hex editor, making it a direct cross-check.
#[test]
fn identical_ubyte_scalar_equals_the_raw_byte() {
    let Some((db, img)) = open_demo() else { return };
    let plan = db
        .plan_characteristic("ASAM.C.SCALAR.UBYTE.IDENTICAL")
        .expect("characteristic present");
    assert_eq!(plan.address, 0x810000);
    assert_eq!(plan.category, Category::Scalar);
    assert_eq!(plan.byte_size(), 1);

    let byte = img.read(0x810000, 1).expect("byte present")[0];
    let row = decode::row_for(&img, &plan);
    assert_eq!(row.presence, Presence::Full);
    assert_eq!(row.display, byte.to_string());
    assert_eq!(row.phys_num, Some(f64::from(byte)));
    assert_eq!(row.raw_hex.as_deref(), Some("14"));
    assert!(row.editable);
}

/// The layout case that alignment padding decides. Axis points are SBYTE and
/// stored INDEX_DECR; function values are SWORD and start one pad byte later.
#[test]
fn std_axis_curve_decodes_axis_and_values() {
    let Some((db, img)) = open_demo() else { return };
    let plan = db
        .plan_characteristic("ASAM.C.CURVE.STD_AXIS")
        .expect("curve present");
    assert_eq!(plan.address, 0x810300);
    assert_eq!(plan.category, Category::Curve);
    assert_eq!(plan.declared_points, 8);
    // 1 count byte + 8 axis bytes + 1 pad + 16 value bytes.
    assert_eq!(plan.byte_size(), 26);

    let detail = decode::detail_for(&db, &img, "ASAM.C.CURVE.STD_AXIS").expect("detail");
    assert_eq!(detail.axis.len(), 8);
    assert_eq!(detail.values.len(), 8);

    // Stored highest-first, presented ascending.
    let axis: Vec<f64> = detail.axis.iter().map(|p| p.phys).collect();
    assert_eq!(axis, vec![-5.0, -1.0, 2.0, 4.0, 5.0, 8.0, 14.0, 22.0]);
    assert!(
        axis.windows(2).all(|w| w[1] > w[0]),
        "axis must ascend after INDEX_DECR reversal: {axis:?}"
    );

    // Function values must sit inside the declared limits; reading them one
    // byte early (no alignment pad) puts them in the thousands.
    let values: Vec<f64> = detail.values.iter().map(|p| p.phys).collect();
    assert_eq!(values, vec![9.0, 13.0, 7.0, 15.0, 71.0, 6.0, -1.0, -3.0]);
    for v in &values {
        assert!(
            *v >= plan.lower_limit && *v <= plan.upper_limit,
            "{v} outside {}..{}",
            plan.lower_limit,
            plan.upper_limit
        );
    }
}

/// A verbal conversion must render its label, not a number.
#[test]
fn tab_verb_scalar_renders_its_label() {
    let Some((db, img)) = open_demo() else { return };
    let rows = decode::list_rows(&db, &img, false);
    let verbal: Vec<_> = rows
        .iter()
        .filter(|r| r.conversion_type == "TAB_VERB" && r.presence == Presence::Full)
        .collect();
    assert!(!verbal.is_empty(), "demo file has TAB_VERB characteristics");

    for row in &verbal {
        assert!(
            row.enum_options.is_some(),
            "{} should offer enum choices",
            row.name
        );
        assert!(
            row.display.chars().any(|c| c.is_alphabetic()),
            "{} should display a label, got {:?}",
            row.name,
            row.display
        );
    }
}

#[test]
fn every_row_is_classified_and_sized_consistently() {
    let Some((db, img)) = open_demo() else { return };
    let rows = decode::list_rows(&db, &img, false);
    // 87 characteristics + 2 axis_pts, measurements excluded.
    assert_eq!(rows.len(), 89);

    for row in &rows {
        if row.category != Category::Unsupported {
            assert!(row.byte_size > 0, "{} has zero size", row.name);
        }
        // A decoded scalar always reports its raw bytes.
        if row.category == Category::Scalar && row.presence == Presence::Full {
            assert!(row.raw_hex.is_some(), "{} lacks raw bytes", row.name);
        }
        // Editability implies there is something concrete to edit: a number, an
        // enum to pick from, or string content.
        if row.editable {
            assert!(
                row.phys_num.is_some() || row.enum_options.is_some() || row.text_value.is_some(),
                "{} is editable but has no value to edit",
                row.name
            );
        }
        // An ASCII row must always report the limit the editor has to enforce.
        if row.category == Category::Ascii && row.presence == Presence::Full {
            let cap = row.text_capacity.expect("capacity");
            let max = row.text_max_len.expect("max length");
            assert!(max <= cap, "{}: max {max} exceeds capacity {cap}", row.name);
            assert_eq!(cap, row.byte_size, "{}: capacity is the byte extent", row.name);
        }
    }
}

#[test]
fn coverage_statistics_are_self_consistent() {
    let Some((db, img)) = open_demo() else { return };
    let s = stats::compute(&db, &img, false);

    assert_eq!(s.total_objects, 89);
    assert_eq!(
        s.scalars + s.curves + s.strings + s.virtuals + s.unsupported,
        s.total_objects
    );
    assert!(s.strings > 0, "the demo file declares an ASCII string");
    assert_eq!(
        s.present_full + s.present_partial + s.absent + s.virtuals,
        s.total_objects
    );

    assert_eq!(s.image_bytes, img.total_bytes());
    assert!(
        s.described_present_bytes <= s.described_bytes,
        "present described bytes cannot exceed described bytes"
    );
    assert!(
        s.described_present_bytes <= s.image_bytes,
        "described bytes present cannot exceed the image"
    );
    assert!(
        s.coverage_pct > 0.0 && s.coverage_pct <= 100.0,
        "coverage should be a sensible percentage, got {}",
        s.coverage_pct
    );
    assert!(s.present_full > 0, "demo image should contain real parameters");
}

/// The slider step must be the physical distance one raw LSB covers, so
/// dragging can only reach values the field can actually store.
#[test]
fn slider_step_follows_the_conversion() {
    let Some((db, img)) = open_demo() else { return };
    let rows = decode::list_rows(&db, &img, false);
    let find = |n: &str| {
        rows.iter()
            .find(|r| r.name == n)
            .unwrap_or_else(|| panic!("{n} missing"))
    };

    let approx = |got: Option<f64>, want: f64, what: &str| {
        let g = got.unwrap_or_else(|| panic!("{what}: no step"));
        assert!(
            (g - want).abs() < 1e-9,
            "{what}: step {g}, expected {want}"
        );
    };

    // IDENTICAL on a byte: one raw count is one physical unit.
    approx(
        find("ASAM.C.SCALAR.UBYTE.IDENTICAL").phys_step,
        1.0,
        "IDENTICAL",
    );
    // LINEAR with a = 2: one raw count is two physical units.
    approx(
        find("ASAM.C.SCALAR.SWORD.LINEAR_MUL_2").phys_step,
        2.0,
        "LINEAR a=2",
    );
    // RAT_FUNC dividing by ten: one raw count is a tenth of a unit.
    approx(
        find("ASAM.C.SCALAR.SWORD.RAT_FUNC_DIV_10").phys_step,
        0.1,
        "RAT_FUNC /10",
    );

    // Every editable numeric row needs a usable step, or its slider is dead.
    for row in rows.iter().filter(|r| r.editable && r.phys_num.is_some()) {
        let step = row.phys_step.unwrap_or_else(|| panic!("{}: no step", row.name));
        assert!(step > 0.0 && step.is_finite(), "{}: step {step}", row.name);
        assert!(
            row.upper_limit > row.lower_limit,
            "{}: limits are not a usable range",
            row.name
        );
    }
}

/// COM_AXIS and RES_AXIS defer to a shared AXIS_PTS object via AXIS_PTS_REF.
#[test]
fn com_axis_curve_reports_its_shared_axis() {
    let Some((db, img)) = open_demo() else { return };
    let detail = decode::detail_for(&db, &img, "ASAM.C.CURVE.COM_AXIS").expect("detail");

    assert_eq!(detail.axis_kind, "COM_AXIS");
    assert_eq!(detail.axis_ref.as_deref(), Some("ASAM.C.AXIS_PTS.UBYTE_8"));
    assert!(
        !detail.axis.is_empty(),
        "breakpoints should be read from the referenced AXIS_PTS object"
    );
}

/// CURVE_AXIS is the trap: it uses CURVE_AXIS_REF, a different field from the
/// one COM_AXIS uses, and points at a CHARACTERISTIC rather than an AXIS_PTS.
/// Reading axis_pts_ref for it silently yields no axis at all.
#[test]
fn curve_axis_resolves_through_curve_axis_ref() {
    let Some((db, img)) = open_demo() else { return };
    let detail = decode::detail_for(&db, &img, "ASAM.C.CURVE.CURVE_AXIS").expect("detail");

    assert_eq!(detail.axis_kind, "CURVE_AXIS");
    assert_eq!(detail.axis_ref.as_deref(), Some("ASAM.C.CURVE_AXIS"));
    assert!(
        !detail.axis.is_empty(),
        "breakpoints come from the referenced curve's function values"
    );
    // The reference must name a real object, or the link would go nowhere.
    assert!(
        db.plan_any("ASAM.C.CURVE_AXIS").is_some(),
        "the referenced characteristic must be resolvable"
    );
}

/// A self-contained axis has no reference to offer.
#[test]
fn std_and_fix_axes_report_no_reference() {
    let Some((db, img)) = open_demo() else { return };

    let std = decode::detail_for(&db, &img, "ASAM.C.CURVE.STD_AXIS").expect("detail");
    assert_eq!(std.axis_kind, "STD_AXIS");
    assert_eq!(std.axis_ref, None);

    let fix = decode::detail_for(&db, &img, "ASAM.C.CURVE.FIX_AXIS.PAR").expect("detail");
    assert_eq!(fix.axis_kind, "FIX_AXIS");
    assert_eq!(fix.axis_ref, None);
    assert!(!fix.axis.is_empty(), "FIX_AXIS points are computed, not stored");
}

/// Every axis reference in the file must resolve, so no link is ever dead.
#[test]
fn all_axis_references_resolve_to_a_listed_object() {
    let Some((db, img)) = open_demo() else { return };
    let listed: std::collections::HashSet<String> = db
        .object_names(true)
        .into_iter()
        .map(|(n, _)| n)
        .collect();

    let mut checked = 0;
    for (name, _) in db.object_names(false) {
        let Some(detail) = decode::detail_for(&db, &img, &name) else { continue };
        if let Some(r) = detail.axis_ref {
            assert!(
                listed.contains(&r),
                "{name} references '{r}', which is not a listed object"
            );
            checked += 1;
        }
    }
    assert!(checked >= 3, "expected several axis references, saw {checked}");
}

/// The demo file's ASCII characteristic is a 42-byte array holding
/// "ASAM Test" followed by NUL padding.
#[test]
fn ascii_string_decodes_with_its_capacity() {
    let Some((db, img)) = open_demo() else { return };
    let plan = db
        .plan_characteristic("ASAM.C.ASCII.UBYTE.NUMBER_42")
        .expect("ASCII characteristic present");
    assert_eq!(plan.address, 0x810200);
    assert_eq!(plan.category, Category::Ascii);
    assert_eq!(plan.byte_size(), 42, "NUMBER 42 characters of UBYTE");

    let row = decode::row_for(&img, &plan);
    assert_eq!(row.text_value.as_deref(), Some("ASAM Test"));
    assert_eq!(row.text_capacity, Some(42));
    // Padded with NULs, so a byte stays reserved for the terminator.
    assert_eq!(row.text_max_len, Some(41));
    assert_eq!(row.display, "ASAM Test");
    assert!(row.editable, "printable content should be editable");

    // A2L makes every CHARACTERISTIC name a COMPU_METHOD, and this one inherits
    // the shared CM.IDENTICAL whose unit is "hours". Text is not a quantity, so
    // that unit must not be carried through.
    assert_eq!(
        db.conversion_for("CM.IDENTICAL").unit,
        "hours",
        "the referenced conversion really does declare a unit"
    );
    assert_eq!(row.unit, "", "a string must not display an inherited unit");

    let detail = decode::detail_for(&db, &img, "ASAM.C.ASCII.UBYTE.NUMBER_42").expect("detail");
    assert_eq!(detail.value_unit, "", "nor in the parameter pane");
}

/// A VIRTUAL_CHARACTERISTIC is computed and never stored. All four in the demo
/// file declare address 0x0, so they must not be mistaken for data that merely
/// happens to be missing from the image.
#[test]
fn virtual_characteristics_are_their_own_category() {
    let Some((db, img)) = open_demo() else { return };
    let rows = decode::list_rows(&db, &img, false);
    let virtuals: Vec<_> = rows
        .iter()
        .filter(|r| r.category == Category::Virtual)
        .collect();

    assert_eq!(virtuals.len(), 4, "demo file declares four virtual parameters");
    for r in &virtuals {
        assert_eq!(r.address, 0, "the declared address is a placeholder");
        assert!(!r.editable, "{}: a computed value cannot be written", r.name);
        assert!(
            r.formula.is_some(),
            "{}: the formula is what there is to show",
            r.name
        );
        // The formula is shown instead of "absent", which would misdescribe it.
        assert_ne!(r.display, "absent", "{}", r.name);
    }

    // REF_3 reads the two other virtual parameters; those links must resolve.
    let ref3 = virtuals
        .iter()
        .find(|r| r.name == "ASAM.C.VIRTUAL.REF_3.SWORD")
        .expect("REF_3 present");
    let deps = ref3.depends_on.as_ref().expect("inputs listed");
    assert_eq!(deps.len(), 2);
    for d in deps {
        assert!(db.plan_any(d).is_some(), "input '{d}' should resolve");
    }
}

/// The presence tallies describe objects that are meant to occupy image bytes,
/// so a computed parameter belongs in none of them.
#[test]
fn virtuals_are_excluded_from_presence_and_coverage() {
    let Some((db, img)) = open_demo() else { return };
    let s = stats::compute(&db, &img, false);

    assert_eq!(s.virtuals, 4);
    assert_eq!(
        s.present_full + s.present_partial + s.absent + s.virtuals,
        s.total_objects,
        "every object is either placed in the image or computed"
    );

    // Their extent must not be credited to the description. All four sit at
    // address 0, which no segment of this image covers, so any contribution
    // would show up as described bytes that describe nothing.
    let described_at_zero = img.present_count(0, 64);
    assert_eq!(described_at_zero, 0, "no image data at address 0");
    assert!(
        s.described_bytes > 0 && s.described_present_bytes <= s.described_bytes,
        "coverage stays self-consistent"
    );
}

/// Three characteristics are different masked views of the single UWORD stored
/// at 0x810002 (0x017F). Without BIT_MASK all three read 383, which sits
/// outside the declared limits of two of them.
#[test]
fn bit_mask_fields_decode_to_their_own_view() {
    let Some((db, img)) = open_demo() else { return };
    let rows = decode::list_rows(&db, &img, false);
    let find = |n: &str| rows.iter().find(|r| r.name == n).expect("row");

    let whole = find("ASAM.C.SCALAR.UWORD.IDENTICAL");
    let f0ff0 = find("ASAM.C.SCALAR.UWORD.IDENTICAL.BITMASK_0FF0");
    let f0001 = find("ASAM.C.SCALAR.UWORD.IDENTICAL.BITMASK_0001");
    let f0010 = find("ASAM.C.SCALAR.UWORD.IDENTICAL.BITMASK_0010");

    // All four read the same word.
    for r in [whole, f0ff0, f0001, f0010] {
        assert_eq!(r.address, 0x810002, "{}", r.name);
        assert_eq!(r.raw_hex.as_deref(), Some("7F 01"), "{}", r.name);
    }

    assert_eq!(whole.phys_num, Some(383.0), "0xFFFF selects the whole word");
    assert_eq!(f0ff0.phys_num, Some(23.0), "(0x017F & 0x0FF0) >> 4");
    assert_eq!(f0001.phys_num, Some(1.0), "(0x017F & 0x0001) >> 0");
    assert_eq!(f0010.phys_num, Some(1.0), "(0x017F & 0x0010) >> 4");

    // Each masked value must now sit inside its own declared limits, which the
    // unmasked 383 did not.
    for r in [whole, f0ff0, f0001, f0010] {
        let v = r.phys_num.unwrap();
        assert!(
            v >= r.lower_limit && v <= r.upper_limit,
            "{}: {v} outside {}..{}",
            r.name,
            r.lower_limit,
            r.upper_limit
        );
    }
}

/// Writing one masked field must leave the other fields in the word intact.
#[test]
fn bit_mask_write_preserves_neighbouring_fields() {
    let Some((db, img)) = open_demo() else { return };
    let name = "ASAM.C.SCALAR.UWORD.IDENTICAL.BITMASK_0FF0";

    // Stored word is 0x017F. Setting the 0x0FF0 field to 0xAB must give
    // 0x0ABF: the new field, with the surrounding 0x000F untouched.
    let w = a2l_data::encode::encode_scalar(&db, &img, name, 0xAB as f64).expect("encode");
    assert_eq!(w.address, 0x810002);
    assert_eq!(w.bytes, vec![0xBF, 0x0A], "little-endian 0x0ABF");
    assert_eq!(w.raw, 0xAB as f64, "reads back as the field value");

    // Setting the single low bit to 0 must clear only that bit.
    let w = a2l_data::encode::encode_scalar(
        &db,
        &img,
        "ASAM.C.SCALAR.UWORD.IDENTICAL.BITMASK_0001",
        0.0,
    )
    .expect("encode");
    assert_eq!(w.bytes, vec![0x7E, 0x01], "0x017F with bit 0 cleared");
}

/// A value too large for the field must be refused, not silently clipped into
/// the neighbouring bits.
#[test]
fn bit_mask_write_rejects_a_value_wider_than_the_field() {
    let Some((db, img)) = open_demo() else { return };
    // The 0x0001 field holds one bit; 1 fits, 2 does not.
    let name = "ASAM.C.SCALAR.UWORD.IDENTICAL.BITMASK_0001";
    assert!(a2l_data::encode::encode_scalar(&db, &img, name, 1.0).is_ok());
    let err = a2l_data::encode::encode_scalar(&db, &img, name, 2.0)
        .expect_err("2 does not fit a one-bit field");
    assert!(err.contains("masked field"), "unhelpful message: {err}");
}

/// Numeric parameters sharing that same conversion must keep their unit.
#[test]
fn numeric_scalars_keep_their_unit() {
    let Some((db, img)) = open_demo() else { return };
    let rows = decode::list_rows(&db, &img, false);
    let row = rows
        .iter()
        .find(|r| r.name == "ASAM.C.SCALAR.UBYTE.IDENTICAL")
        .expect("scalar present");
    assert_eq!(row.conversion, "CM.IDENTICAL", "same conversion as the string");
    assert_eq!(row.unit, "hours", "a quantity keeps its declared unit");
}

/// Writing must rewrite the whole array, not just the new characters, or the
/// tail of a longer previous string survives.
#[test]
fn ascii_write_clears_the_rest_of_the_array() {
    let Some((db, _)) = open_demo() else { return };
    let w = a2l_data::encode::encode_ascii(&db, "ASAM.C.ASCII.UBYTE.NUMBER_42", "Hi")
        .expect("should encode");

    assert_eq!(w.address, 0x810200);
    assert_eq!(w.bytes.len(), 42, "the full array is rewritten");
    assert_eq!(&w.bytes[..2], b"Hi");
    assert!(
        w.bytes[2..].iter().all(|b| *b == 0),
        "everything past the new string must be cleared, leaving no residue \
         of the previous 'ASAM Test'"
    );
}

#[test]
fn ascii_write_rejects_overlong_and_non_printable_input() {
    let Some((db, _)) = open_demo() else { return };
    let name = "ASAM.C.ASCII.UBYTE.NUMBER_42";

    // 42 fits the array exactly; 43 does not.
    assert!(a2l_data::encode::encode_ascii(&db, name, &"x".repeat(42)).is_ok());
    assert!(a2l_data::encode::encode_ascii(&db, name, &"x".repeat(43)).is_err());

    assert!(a2l_data::encode::encode_ascii(&db, name, "tab\there").is_err());
    assert!(a2l_data::encode::encode_ascii(&db, name, "café").is_err());
}

/// The generic text entry point must route an ASCII object to the ASCII
/// encoder rather than the enum one.
#[test]
fn encode_text_routes_ascii_objects() {
    let Some((db, img)) = open_demo() else { return };
    let w = a2l_data::encode::encode_text(&db, &img, "ASAM.C.ASCII.UBYTE.NUMBER_42", "Ok")
        .expect("should route to the ASCII encoder");
    assert_eq!(w.bytes.len(), 42);
    assert_eq!(&w.bytes[..2], b"Ok");
}

/// Encoding a physical value must produce bytes that decode back to it.
#[test]
fn scalar_edits_round_trip_through_the_image() {
    let Some((db, img)) = open_demo() else { return };
    let rows = decode::list_rows(&db, &img, false);

    let mut checked = 0;
    for row in rows.iter().filter(|r| r.editable && r.phys_num.is_some()) {
        let original = row.phys_num.unwrap();
        let write = a2l_data::encode::encode_scalar(&db, &img, &row.name, original)
            .unwrap_or_else(|e| panic!("{} should encode: {e}", row.name));

        assert_eq!(write.bytes.len() as u32, row.byte_size, "{}", row.name);
        assert!(
            (write.phys - original).abs() <= original.abs() * 1e-9 + 1e-9,
            "{}: {} encoded then decoded to {}",
            row.name,
            original,
            write.phys
        );
        checked += 1;
    }
    assert!(checked > 10, "expected many editable scalars, saw {checked}");
}

