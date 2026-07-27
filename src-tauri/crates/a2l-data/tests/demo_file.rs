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
    let row = decode::row_for(&db, &img, &plan);
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

    // Expected values come from ASAP2_Demo_V171.CDFX, the calibration-data
    // counterpart shipped with this A2L, so the pairing is checked against the
    // file's own record rather than against an assumption.
    //
    // The axis is stored INDEX_DECR (highest-first) and the function values sit
    // alongside it element by element, so presenting the axis ascending
    // requires reversing both. Reversing only the axis pairs -5 with 9 instead
    // of -3 and mispairs every point.
    let axis: Vec<f64> = detail.axis.iter().map(|p| p.phys).collect();
    let values: Vec<f64> = detail.values.iter().map(|p| p.phys).collect();
    assert_eq!(axis, vec![-5.0, -1.0, 2.0, 4.0, 5.0, 8.0, 14.0, 22.0]);
    assert_eq!(values, vec![-3.0, -1.0, 6.0, 71.0, 15.0, 7.0, 13.0, 9.0]);
    assert!(
        axis.windows(2).all(|w| w[1] > w[0]),
        "axis must ascend after INDEX_DECR reversal: {axis:?}"
    );

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
        s.scalars + s.curves + s.maps + s.strings + s.virtuals + s.unsupported,
        s.total_objects
    );
    assert!(s.maps > 0, "the demo file declares maps and cuboids");
    assert!(s.strings > 0, "the demo file declares an ASCII string");
    assert_eq!(
        s.present_full + s.present_partial + s.absent + s.presence_unknown + s.virtuals,
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

    // Per the CDFX. The shared axis is INDEX_DECR, so the curve's own values
    // must be reversed to match it even though the curve's record layout says
    // nothing about ordering — the order belongs to the referenced axis.
    let axis: Vec<f64> = detail.axis.iter().map(|p| p.phys).collect();
    let values: Vec<f64> = detail.values.iter().map(|p| p.phys).collect();
    assert_eq!(axis, vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 13.0, 15.0]);
    assert_eq!(values, vec![13.0, 15.0, 16.0, 18.0, 43.0, 16.0, 33.0, 14.0]);
}

/// Editing a point must land on the element the table showed, which for an
/// INDEX_DECR axis is the mirror of the storage position. Getting this wrong
/// writes to the opposite end of the curve and looks plausible.
#[test]
fn editing_a_point_maps_display_index_back_to_storage() {
    use a2l_data::encode::{encode_point, PointTarget};
    let Some((db, img)) = open_demo() else { return };
    let name = "ASAM.C.CURVE.STD_AXIS";
    let plan = db.plan_characteristic(name).expect("curve");
    assert!(plan.dim_reversed(0), "this curve is stored INDEX_DECR");

    // Displayed values are -3, -1, 6, 71, 15, 7, 13, 9 against axis -5 … 22.
    // Display index 0 pairs with axis -5, whose value is the *last* stored
    // element, at offset 10 + 7*2 = 24.
    let w = encode_point(&db, &img, name, PointTarget::Value, 0, -30.0).expect("encode");
    assert_eq!(w.address, plan.address + 24, "first shown point is stored last");

    // The other end: display index 7 pairs with axis 22, the first stored.
    let w = encode_point(&db, &img, name, PointTarget::Value, 7, 90.0).expect("encode");
    assert_eq!(w.address, plan.address + 10, "last shown point is stored first");

    // The axis column reverses the same way. Axis field starts at offset 1 and
    // holds SBYTEs, so display index 0 is at 1 + 7 = 8.
    let w = encode_point(&db, &img, name, PointTarget::AXIS, 0, -6.0).expect("encode");
    assert_eq!(w.address, plan.address + 8);
    assert_eq!(w.bytes, vec![0xFA], "-6 as an SBYTE");

    // Past the end is refused rather than writing into the next object.
    assert!(encode_point(&db, &img, name, PointTarget::Value, 8, 1.0).is_err());
}

/// Writing a point and reading the object back must show the new value at the
/// same row the edit was made on.
#[test]
fn edited_point_reads_back_at_the_same_row() {
    use a2l_data::encode::{encode_point, PointTarget};
    let Some((db, img)) = open_demo() else { return };
    let name = "ASAM.C.CURVE.STD_AXIS";

    for row in [0usize, 3, 7] {
        let target_value = 40.0 + row as f64;
        let w = encode_point(&db, &img, name, PointTarget::Value, row as u32, target_value)
            .expect("encode");

        // Apply the write over a copy of the image and re-decode.
        let mut edited = MapImage(img.0.clone());
        for (i, b) in w.bytes.iter().enumerate() {
            edited.0.insert(w.address + i as u32, *b);
        }
        let detail = decode::detail_for(&db, &edited, name).expect("detail");
        assert_eq!(
            detail.values[row].phys, target_value,
            "row {row} should read back what was written to it"
        );

        // And nothing else moved.
        let before = decode::detail_for(&db, &img, name).expect("detail");
        for (i, (a, b)) in before.values.iter().zip(&detail.values).enumerate() {
            if i != row {
                assert_eq!(a.phys, b.phys, "row {i} changed while editing row {row}");
            }
        }
    }
}

/// A shared axis must be edited on the object that owns it, so one write
/// cannot silently retune every curve referencing it.
#[test]
fn a_shared_axis_refuses_edits_and_says_where_to_go() {
    use a2l_data::encode::{encode_point, PointTarget};
    let Some((db, img)) = open_demo() else { return };

    let err = encode_point(&db, &img, "ASAM.C.CURVE.COM_AXIS", PointTarget::AXIS, 0, 1.0)
        .expect_err("a shared axis is not editable through the curve");
    assert!(
        err.contains("ASAM.C.AXIS_PTS.UBYTE_8"),
        "the message should name where to edit it: {err}"
    );

    // Its own values remain editable.
    assert!(encode_point(&db, &img, "ASAM.C.CURVE.COM_AXIS", PointTarget::Value, 0, 20.0).is_ok());
    // And the axis object itself accepts edits.
    assert!(
        encode_point(&db, &img, "ASAM.C.AXIS_PTS.UBYTE_8", PointTarget::Value, 0, 3.0).is_ok()
    );

    // A computed axis has no bytes to write.
    let err = encode_point(&db, &img, "ASAM.C.CURVE.FIX_AXIS.PAR", PointTarget::AXIS, 0, 1.0)
        .expect_err("FIX_AXIS is computed");
    assert!(err.contains("FIX_AXIS"), "{err}");
}

/// A VAL_BLK declaring MATRIX_DIM 3 4 holds all twelve elements, not three.
/// Expected values are those the shipped CDFX records for this array.
#[test]
fn multi_dimensional_val_blk_reads_every_element() {
    let Some((db, img)) = open_demo() else { return };
    let name = "ASAM.C.ARRAY.SWORD.MATRIX_DIM_3_4.ROW_DIR";
    let plan = db.plan_characteristic(name).expect("array present");

    assert_eq!(plan.declared_points, 12, "3 x 4, not just the first dimension");
    assert_eq!(plan.byte_size(), 24, "twelve SWORDs");

    let detail = decode::detail_for(&db, &img, name).expect("detail");
    let values: Vec<f64> = detail.values.iter().map(|p| p.phys).collect();
    assert_eq!(values, (1..=12).map(f64::from).collect::<Vec<_>>());
}

/// A standalone AXIS_PTS object is itself INDEX_DECR here, so its points must
/// also read ascending. Per the CDFX.
#[test]
fn shared_axis_object_reads_ascending() {
    let Some((db, img)) = open_demo() else { return };
    let detail = decode::detail_for(&db, &img, "ASAM.C.AXIS_PTS.UBYTE_8").expect("detail");
    let pts: Vec<f64> = detail.values.iter().map(|p| p.phys).collect();
    assert_eq!(pts, vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 13.0, 15.0]);
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

    let row = decode::row_for(&db, &img, &plan);
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

/// The virtual parameters now evaluate rather than showing their formula.
///
/// The chain is worth spelling out, because it exercises every part of the
/// evaluator at once: SCALAR.UBYTE.IDENTICAL holds 20, so
/// `X1 + sysc(System_Constant_1)` is 20 + (-3.45) = 16.55, and REF_3 reads two
/// other computed parameters rather than stored bytes.
#[test]
fn virtual_parameters_evaluate_their_formulas() {
    let Some((db, img)) = open_demo() else { return };
    let rows = decode::list_rows(&db, &img, false);
    let val = |n: &str| {
        rows.iter()
            .find(|r| r.name == n)
            .unwrap_or_else(|| panic!("{n} missing"))
            .phys_num
            .unwrap_or_else(|| panic!("{n} did not evaluate"))
    };

    // X1 = SCALAR.SBYTE.IDENTICAL = 6, formula "X1 - 9".
    assert_eq!(val("ASAM.C.VIRTUAL.REF_1.SWORD"), -3.0);
    // X1 = SCALAR.UBYTE.IDENTICAL = 20, formula "X1 + 19".
    assert_eq!(val("ASAM.C.VIRTUAL.REF_2.UWORD"), 39.0);
    // "X1 + X2" over the two above: a virtual reading virtuals.
    assert_eq!(val("ASAM.C.VIRTUAL.REF_3.SWORD"), 36.0);
    // "X1 + sysc(System_Constant_1)" with the constant at -3.45.
    let sysc = val("ASAM.C.VIRTUAL.SYSTEM_CONSTANT_1");
    assert!((sysc - 16.55).abs() < 1e-9, "got {sysc}");
}

/// The strongest available check on the evaluator: DEPENDENT_CHARACTERISTICs
/// carry the same formulas but *are* stored, so recomputing one and comparing
/// against the bytes in the image validates the whole chain against ground
/// truth rather than against my own expectations.
#[test]
fn dependent_characteristics_match_their_stored_values() {
    let Some((db, img)) = open_demo() else { return };
    let rows = decode::list_rows(&db, &img, false);
    let stored = |n: &str| {
        rows.iter()
            .find(|r| r.name == n)
            .unwrap_or_else(|| panic!("{n} missing"))
            .phys_num
            .unwrap_or_else(|| panic!("{n} has no stored value"))
    };
    let constants = db.system_constants();
    let eval = |src: &str, vars: &[f64]| {
        a2l_data::formula::Formula::parse(src)
            .expect("parse")
            .eval(&a2l_data::formula::Context { vars, constants })
            .expect("eval")
    };

    let sbyte = stored("ASAM.C.SCALAR.SBYTE.IDENTICAL"); // 6
    let ubyte = stored("ASAM.C.SCALAR.UBYTE.IDENTICAL"); // 20

    // REF_1 = X1 + 5 over the SBYTE scalar.
    let r1 = eval("X1 + 5", &[sbyte]);
    assert_eq!(r1, stored("ASAM.C.DEPENDENT.REF_1.SWORD"));

    // REF_2 = X1 + 25 over the UBYTE scalar.
    let r2 = eval("X1 + 25", &[ubyte]);
    assert_eq!(r2, stored("ASAM.C.DEPENDENT.REF_2.UWORD"));

    // REF_3 = X1 + X2 over the two above.
    assert_eq!(eval("X1 + X2", &[r1, r2]), stored("ASAM.C.DEPENDENT.REF_3.SWORD"));

    // REF_4 exercises a system constant.
    let r4 = eval("X1 + sysc(System_Constant_1)", &[r1]);
    assert!(
        (r4 - stored("ASAM.C.DEPENDENT.REF_4.FLOAT64_IEEE")).abs() < 1e-9,
        "computed {r4}"
    );

    // REF_5 reads a *virtual* parameter, so this closes the loop between the
    // two formula paths: 20 + (-3.45) = 16.55, doubled is 33.1.
    let virt = stored("ASAM.C.VIRTUAL.SYSTEM_CONSTANT_1");
    let r5 = eval("X1 * 2", &[virt]);
    assert!(
        (r5 - stored("ASAM.C.DEPENDENT.REF_5.FLOAT64_IEEE")).abs() < 1e-9,
        "computed {r5} from virtual {virt}"
    );
}

/// A FORM conversion is now evaluated, and inverts only when FORMULA_INV says how.
#[test]
fn form_conversions_evaluate_and_invert() {
    let Some((db, img)) = open_demo() else { return };
    let rows = decode::list_rows(&db, &img, false);
    let row = rows
        .iter()
        .find(|r| r.name == "ASAM.C.SCALAR.SWORD.FORM_X_PLUS_4")
        .expect("FORM scalar present");

    // CM.FORM.X_PLUS_4 is "X1+4" over the stored SWORD, which holds 2.
    assert_eq!(row.conversion_type, "FORM");
    assert_eq!(row.phys_num, Some(6.0), "no longer unevaluated");
    assert!(row.editable, "FORMULA_INV 'X1-4' makes it writable");

    // The inverse must round-trip: asking for 6 must store the raw 2 again.
    let w = a2l_data::encode::encode_scalar(&db, &img, &row.name, 6.0).expect("encode");
    assert_eq!(w.raw, 2.0, "FORMULA_INV inverts the forward expression");

    // CM.VIRTUAL.EXTERNAL_VALUE is "4*X1" with no FORMULA_INV, so it displays
    // but cannot be written.
    let conv = db.conversion_for("CM.VIRTUAL.EXTERNAL_VALUE");
    assert!(
        !conv.conversion.is_invertible(),
        "without FORMULA_INV there is no way back"
    );
}

/// A rescale axis uses NO_RESCALE_X / RESERVED / AXIS_RESCALE_X, none of which
/// the resolver originally understood. Every field was skipped, the extent came
/// out zero, and the object was reported as missing from the image even though
/// its bytes are plainly there at 0x8103D0.
#[test]
fn rescale_axis_resolves_its_layout_and_pairs() {
    let Some((db, img)) = open_demo() else { return };
    let plan = db
        .plan_axis_pts("ASAM.C.AXIS_PTS.RESCALE")
        .expect("axis present");

    assert_eq!(plan.address, 0x8103D0);
    assert_eq!(plan.category, Category::Curve, "no longer unsupported");
    // 1 count byte + 1 RESERVED pad + 5 pairs of UBYTE.
    assert_eq!(plan.byte_size(), 12);

    let row = decode::row_for(&db, &img, &plan);
    assert_eq!(
        row.presence,
        Presence::Full,
        "the image does contain these bytes"
    );

    let detail = decode::detail_for(&db, &img, "ASAM.C.AXIS_PTS.RESCALE").expect("detail");
    assert_eq!(detail.axis_kind, "RES_AXIS");
    // Stored 05 FF | 11 20 14 40 20 80 B0 D0 D2 FF — five (value, index) pairs
    // after the count byte and its padding.
    let axis: Vec<f64> = detail.axis.iter().map(|p| p.phys).collect();
    let idx: Vec<f64> = detail.values.iter().map(|p| p.phys).collect();
    assert_eq!(axis, vec![17.0, 20.0, 32.0, 176.0, 210.0]);
    assert_eq!(idx, vec![32.0, 64.0, 128.0, 208.0, 255.0]);
    assert!(
        axis.windows(2).all(|w| w[1] > w[0]),
        "a rescale axis ascends: {axis:?}"
    );
}

/// An extent we cannot resolve must not be reported as missing data — that
/// would blame the image for a gap in this crate.
#[test]
fn unresolvable_extent_reports_unknown_not_absent() {
    let Some((db, img)) = open_demo() else { return };
    let rows = decode::list_rows(&db, &img, false);

    for row in &rows {
        if row.byte_size == 0 {
            assert_eq!(
                row.presence,
                Presence::Unknown,
                "{}: zero extent must read as unknown",
                row.name
            );
        } else {
            assert_ne!(
                row.presence,
                Presence::Unknown,
                "{}: a known extent must yield a real verdict",
                row.name
            );
        }
    }

    let s = stats::compute(&db, &img, false);
    assert_eq!(
        s.present_full + s.present_partial + s.absent + s.presence_unknown + s.virtuals,
        s.total_objects,
        "every object lands in exactly one bucket"
    );
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
        s.present_full + s.present_partial + s.absent + s.presence_unknown + s.virtuals,
        s.total_objects,
        "every object is either placed in the image, unresolvable, or computed"
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


/// The parser must handle the real file, not just a hand-written sample.
#[test]
fn parses_the_shipped_cdfx() {
    let Some(dir) = demo_dir() else { return };
    let path = format!("{dir}/ASAP2_Demo_V171.CDFX");
    if !std::path::Path::new(&path).exists() {
        eprintln!("skipping: no CDFX alongside the demo A2L");
        return;
    }
    let xml = std::fs::read_to_string(&path).expect("read CDFX");
    let f = a2l_data::cdfx::parse(&xml).expect("parse the shipped CDFX");

    assert_eq!(f.instances.len(), 86, "every SW-INSTANCE is read");

    // Spot-checks against values verified by hand earlier in this work.
    let s = f.get("ASAM.C.SCALAR.UBYTE.IDENTICAL").expect("scalar");
    assert_eq!(s.category, "VALUE");
    assert_eq!(s.values, vec![a2l_data::cdfx::CdfxValue::Num(20.0)]);

    // A verbal value arrives as text, not a number.
    let e = f.get("ASAM.C.SCALAR.SWORD.TAB_VERB_DEFAULT_VALUE").expect("enum");
    assert_eq!(e.values, vec![a2l_data::cdfx::CdfxValue::Text("Square".into())]);

    // The string is stored as character entities and must come back readable.
    let a = f.get("ASAM.C.ASCII.UBYTE.NUMBER_42").expect("ascii");
    assert_eq!(a.values, vec![a2l_data::cdfx::CdfxValue::Text("ASAM Test".into())]);

    // Curve values and axis breakpoints stay in their own lists.
    let c = f.get("ASAM.C.CURVE.STD_AXIS").expect("curve");
    let vals: Vec<f64> = c.values.iter().filter_map(|v| v.as_num()).collect();
    let axis: Vec<f64> = c.axes[0].values.iter().filter_map(|v| v.as_num()).collect();
    assert_eq!(vals, vec![-3.0, -1.0, 6.0, 71.0, 15.0, 7.0, 13.0, 9.0]);
    assert_eq!(axis, vec![-5.0, -1.0, 2.0, 4.0, 5.0, 8.0, 14.0, 22.0]);

    // A shared axis is a reference rather than duplicated values.
    let com = f.get("ASAM.C.CURVE.COM_AXIS").expect("com axis curve");
    assert_eq!(
        com.axes[0].instance_ref.as_deref(),
        Some("ASAM.C.AXIS_PTS.UBYTE_8")
    );
}

fn load_demo_cdfx(dir: &str) -> Option<a2l_data::cdfx::CdfxFile> {
    let path = format!("{dir}/ASAP2_Demo_V171.CDFX");
    if !std::path::Path::new(&path).exists() {
        eprintln!("skipping: no CDFX alongside the demo A2L");
        return None;
    }
    Some(a2l_data::cdfx::parse(&std::fs::read_to_string(&path).ok()?).expect("parse CDFX"))
}

/// The shipped CDFX describes the shipped hex, so importing one into the other
/// must be a no-op. Any reported change is a disagreement between this crate's
/// conversions and the tool that produced the file — which is exactly what this
/// test exists to catch.
#[test]
fn importing_the_shipped_cdfx_changes_nothing() {
    let Some(dir) = demo_dir() else { return };
    let Some((db, img)) = open_demo() else { return };
    let Some(file) = load_demo_cdfx(&dir) else { return };

    let report = a2l_data::sync::plan_import(&db, &img, &file);

    eprintln!(
        "instances {} | matched {} | unchanged {} | changed {} | skipped {} | not in A2L {}",
        report.file_instances,
        report.matched,
        report.unchanged,
        report.changed_parameters(),
        report.skipped.len(),
        report.not_in_a2l.len()
    );
    for c in report.changes.iter().take(10) {
        eprintln!("  CHANGE {} {}[{:?}] {} -> {}", c.name, c.target, c.index, c.current, c.incoming);
    }

    assert_eq!(report.file_instances, 86);
    assert!(report.matched > 60, "most instances should resolve");

    // Two families of expected difference, and nothing else.
    //
    // The file rounds every value to its A2L FORMAT, which for the two IEEE
    // scalars loses real precision — it records -3 for a stored
    // -3.000000491882041.
    //
    // The cuboid is the shipped file disagreeing with itself. Its X axis is
    // ASAM.C.AXIS_PTS.UBYTE_8, stored INDEX_DECR, and three other objects
    // sharing that same axis — ASAM.C.CURVE.COM_AXIS,
    // ASAM.C.MAP.COM_AXIS.FIX_AXIS and ASAM.C.MAP.COM_AXIS.FIX_AXIS_2 — all
    // record their values mirrored along X, which is what a decreasing axis
    // presented ascending requires. This one cuboid does not. Mirroring
    // uniformly is the defensible reading, so the deviation is recorded here
    // rather than special-cased in the decoder.
    const CUBOID: &str = "ASAM.C.CUBOID.COM_AXIS.FIX_AXIS.STD_AXIS";
    let mut names: Vec<&str> = report.changes.iter().map(|c| c.name.as_str()).collect();
    names.dedup();
    assert_eq!(
        names,
        vec![
            CUBOID,
            "ASAM.C.SCALAR.FLOAT32_IEEE.IDENTICAL",
            "ASAM.C.SCALAR.FLOAT64_IEEE.IDENTICAL",
        ],
        "only the file's float rounding and its one inconsistent cuboid should differ"
    );

    // The cuboid's whole 96 values differ, and only along X: the values it
    // reports are its own, read the other way round.
    let cuboid: Vec<f64> = report
        .changes
        .iter()
        .filter(|c| c.name == CUBOID)
        .filter_map(|c| c.incoming.parse::<f64>().ok())
        .collect();
    assert_eq!(cuboid.len(), 96, "the disagreement is total, not partial");

    let f64_change = report
        .changes
        .iter()
        .find(|c| c.name == "ASAM.C.SCALAR.FLOAT64_IEEE.IDENTICAL")
        .expect("the float64 scalar differs");
    assert_eq!(f64_change.current, "-3.000000491882041");
    assert_eq!(f64_change.incoming, "-3");
}

/// A changed CDFX must produce exactly the writes needed, and no others.
#[test]
fn import_reports_only_what_actually_differs() {
    let Some(dir) = demo_dir() else { return };
    let Some((db, img)) = open_demo() else { return };
    let Some(mut file) = load_demo_cdfx(&dir) else { return };

    // Nudge one scalar and one curve point.
    for inst in &mut file.instances {
        if inst.name == "ASAM.C.SCALAR.UBYTE.IDENTICAL" {
            inst.values = vec![a2l_data::cdfx::CdfxValue::Num(30.0)];
        }
        if inst.name == "ASAM.C.CURVE.STD_AXIS" {
            inst.values[0] = a2l_data::cdfx::CdfxValue::Num(-9.0);
        }
    }

    let report = a2l_data::sync::plan_import(&db, &img, &file);
    // The two edits, plus the two float scalars the file rounds (see
    // `importing_the_shipped_cdfx_changes_nothing`).
    assert_eq!(report.changes.len(), 4 + 96);

    let scalar = report
        .changes
        .iter()
        .find(|c| c.name == "ASAM.C.SCALAR.UBYTE.IDENTICAL")
        .expect("scalar change");
    assert_eq!(scalar.current, "20");
    assert_eq!(scalar.incoming, "30");
    assert_eq!(scalar.address, 0x810000);
    assert_eq!(scalar.bytes, vec![30]);

    // The curve's first displayed point is the last stored element, so the
    // change must carry that address rather than the start of the array.
    let point = report
        .changes
        .iter()
        .find(|c| c.name == "ASAM.C.CURVE.STD_AXIS")
        .expect("curve change");
    assert_eq!(point.index, Some(0));
    assert_eq!(point.address, 0x810300 + 24);
}

/// Export then re-import must be a no-op, and the written file must reparse.
#[test]
fn exported_cdfx_round_trips_against_the_image() {
    let Some((db, img)) = open_demo() else { return };

    let instances = a2l_data::sync::export(&db, &img);
    assert!(instances.len() > 50, "got {}", instances.len());

    let xml = a2l_data::cdfx::write("hex-studio-export", "test", &instances).expect("write");
    let reparsed = a2l_data::cdfx::parse(&xml).expect("reparse our own output");
    assert_eq!(reparsed.instances.len(), instances.len());

    // Feeding our export back in must find nothing to do.
    let report = a2l_data::sync::plan_import(&db, &img, &reparsed);
    for c in report.changes.iter().take(10) {
        eprintln!("  UNEXPECTED {} {} {} -> {}", c.name, c.target, c.current, c.incoming);
    }
    assert!(
        report.changes.is_empty(),
        "{} value(s) changed when re-importing our own export",
        report.changes.len()
    );
}

/// How many decimals a parsed value was written with — the fewest that
/// reproduce it exactly.
fn decimals_shown(v: f64) -> usize {
    (0..=12)
        .find(|d| {
            let f = 10f64.powi(*d as i32);
            (v * f).round() / f == v
        })
        .unwrap_or(12)
}

/// Values we export must agree with the ones the reference tool wrote.
#[test]
fn export_matches_the_shipped_cdfx_values() {
    let Some(dir) = demo_dir() else { return };
    let Some((db, img)) = open_demo() else { return };
    let Some(shipped) = load_demo_cdfx(&dir) else { return };

    let ours = a2l_data::sync::export(&db, &img);
    let mut compared = 0;
    for inst in &ours {
        let Some(theirs) = shipped.get(&inst.name) else { continue };
        if inst.values.len() != theirs.values.len() {
            continue; // shapes we model differently, e.g. rescale pairs
        }
        // See `importing_the_shipped_cdfx_changes_nothing`: this one cuboid
        // contradicts every other user of its own INDEX_DECR axis.
        if inst.name == "ASAM.C.CUBOID.COM_AXIS.FIX_AXIS.STD_AXIS" {
            continue;
        }
        for (a, b) in inst.values.iter().zip(&theirs.values) {
            match (a, b) {
                (a2l_data::cdfx::CdfxValue::Num(x), a2l_data::cdfx::CdfxValue::Num(y)) => {
                    // The shipped file rounds — to the A2L FORMAT where there
                    // is one, and to whole numbers where there is not, so its
                    // FLOAT32 scalar reads "33" for a stored 33.23455810546875.
                    // Comparing to half a unit in its own last decimal place is
                    // therefore as tight as the file allows. Byte-exactness is
                    // what `importing_the_shipped_cdfx_changes_nothing` covers.
                    let tol = 0.5 * 10f64.powi(-(decimals_shown(*y) as i32));
                    assert!(
                        (x - y).abs() <= tol,
                        "{}: we say {x}, the shipped file says {y}",
                        inst.name
                    );
                }
                (a2l_data::cdfx::CdfxValue::Text(x), a2l_data::cdfx::CdfxValue::Text(y)) => {
                    assert_eq!(x, y, "{}", inst.name);
                }
                _ => {}
            }
            compared += 1;
        }
    }
    assert!(compared > 100, "expected many comparisons, made {compared}");
}

/// The demo file stores one 3x4 matrix twice — once ROW_DIR, once COLUMN_DIR —
/// specifically so a reader can be checked against itself. Both must present
/// the same matrix, and the CDFX's row-by-row grouping says which one that is.
#[test]
fn row_dir_and_column_dir_matrices_agree() {
    let Some((db, img)) = open_demo() else { return };

    let read = |name: &str| -> Vec<f64> {
        decode::detail_for(&db, &img, name)
            .unwrap_or_else(|| panic!("{name} missing"))
            .values
            .iter()
            .map(|p| p.phys)
            .collect()
    };

    let row = read("ASAM.C.ARRAY.SWORD.MATRIX_DIM_3_4.ROW_DIR");
    let col = read("ASAM.C.ARRAY.SWORD.MATRIX_DIM_3_4.COLUMN_DIR");

    assert_eq!(row.len(), 12, "3x4 is twelve elements, not three");
    assert_eq!(row, (1..=12).map(f64::from).collect::<Vec<_>>());
    assert_eq!(col, row, "COLUMN_DIR is stored transposed, not different data");
}

/// Writing a COLUMN_DIR element must land on the byte the same index reads.
#[test]
fn column_dir_writes_where_it_reads() {
    let Some((db, img)) = open_demo() else { return };
    let name = "ASAM.C.ARRAY.SWORD.MATRIX_DIM_3_4.COLUMN_DIR";
    let plan = db.plan_characteristic(name).expect("present");

    for index in 0..12u32 {
        let w = a2l_data::encode::encode_point(
            &db,
            &img,
            name,
            a2l_data::encode::PointTarget::Value,
            index,
            99.0,
        )
        .expect("encodable");
        // Element `index` is displayed as value `index + 1`, so the byte it
        // writes must be the one currently holding that value.
        let stored = img.read(w.address, 2).expect("in image");
        let current = i16::from_le_bytes([stored[0], stored[1]]);
        assert_eq!(
            current,
            index as i16 + 1,
            "index {index} at {:#x} holds {current}",
            w.address
        );
        assert!(w.address >= plan.address && w.address < plan.address + plan.byte_size());
    }
}

// ── Multi-dimensional objects ────────────────────────────────────────────────

/// Maps, cuboids and cubes must resolve to a shape and a size, not to
/// "unsupported".
#[test]
fn multi_dimensional_objects_are_shaped_and_sized() {
    let Some((db, _)) = open_demo() else { return };

    let cases: [(&str, &[u32], u32); 4] = [
        // 4 x 5 SWORD values, plus two count bytes and 4+5 SBYTE axis bytes.
        ("ASAM.C.MAP.STD_AXIS.STD_AXIS", &[4, 5], 2 + 9 + 1 + 40),
        // A shared X axis and a computed Y axis occupy no bytes of their own.
        ("ASAM.C.MAP.COM_AXIS.FIX_AXIS", &[8, 3], 48),
        ("ASAM.C.CUBOID.ROW_DIR", &[2, 3, 4], 3 + 9 + 24),
        ("ASAM.C.CUBE_4.ROW_DIR", &[2, 3, 4, 2], 4 + 11 + 48),
    ];

    for (name, dims, bytes) in cases {
        let plan = db.plan_characteristic(name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(plan.category, Category::Map, "{name}");
        assert_eq!(plan.dims, dims.to_vec(), "{name}");
        assert_eq!(
            plan.declared_points,
            dims.iter().product::<u32>(),
            "{name} element count is the product of its dimensions"
        );
        assert_eq!(plan.byte_size(), bytes, "{name}");
        assert_eq!(plan.axes.len(), dims.len(), "{name} declares one axis per dimension");
    }
}

/// The demo file stores the same 2x3x4 cuboid twice, once ROW_DIR and once
/// COLUMN_DIR, so the pair checks the reader against itself — and pins down
/// that COLUMN_DIR swaps X with Y rather than reversing the whole dimension
/// order, which for three dimensions is a different permutation.
#[test]
fn row_dir_and_column_dir_cuboids_agree() {
    let Some((db, img)) = open_demo() else { return };

    let read = |name: &str| -> Vec<f64> {
        decode::detail_for(&db, &img, name)
            .unwrap_or_else(|| panic!("{name} missing"))
            .values
            .iter()
            .map(|p| p.phys)
            .collect()
    };

    let row = read("ASAM.C.CUBOID.ROW_DIR");
    assert_eq!(row, (1..=24).map(f64::from).collect::<Vec<_>>());
    assert_eq!(read("ASAM.C.CUBOID.COLUMN_DIR"), row);

    let row4 = read("ASAM.C.CUBE_4.ROW_DIR");
    assert_eq!(row4.len(), 48, "2 x 3 x 4 x 2");
    assert_eq!(row4, (1..=48).map(f64::from).collect::<Vec<_>>());
    assert_eq!(read("ASAM.C.CUBE_4.COLUMN_DIR"), row4);
}

/// A map's values and both its axes, against the shipped CDFX.
#[test]
fn map_decodes_its_values_and_every_axis() {
    let Some((db, img)) = open_demo() else { return };
    let detail =
        decode::detail_for(&db, &img, "ASAM.C.MAP.STD_AXIS.STD_AXIS").expect("detail");

    assert_eq!(detail.dims, vec![4, 5]);
    // The CDFX writes this map as five groups of four holding 0..19, so the
    // first dimension is the one that varies fastest.
    let values: Vec<f64> = detail.values.iter().map(|p| p.phys).collect();
    assert_eq!(values, (0..20).map(f64::from).collect::<Vec<_>>());

    assert_eq!(detail.axes.len(), 2);
    let x: Vec<f64> = detail.axes[0].points.iter().map(|p| p.phys).collect();
    assert_eq!(x, vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(detail.axes[0].kind, "STD_AXIS");
    assert!(detail.axes[0].editable, "a STD_AXIS is stored here and writable");

    // The Y axis has a verbal conversion, so it renders labels rather than
    // numbers — the CDFX records 'red', 'orange', 'yellow', 'green', 'blue'.
    let y: Vec<String> = detail.axes[1].points.iter().map(|p| p.display.clone()).collect();
    assert_eq!(y, vec!["red", "orange", "yellow", "green", "blue"]);

    // The X axis is mirrored into the flat fields the 1D table reads.
    assert_eq!(detail.axis.len(), 4);
    assert_eq!(detail.axis_kind, "STD_AXIS");
}

/// A shared or computed axis is reported but not writable through the map.
#[test]
fn map_axis_ownership_is_reported() {
    let Some((db, img)) = open_demo() else { return };
    let detail = decode::detail_for(&db, &img, "ASAM.C.MAP.COM_AXIS.FIX_AXIS").expect("detail");

    assert_eq!(detail.axes.len(), 2);
    assert_eq!(detail.axes[0].kind, "COM_AXIS");
    assert_eq!(
        detail.axes[0].reference.as_deref(),
        Some("ASAM.C.AXIS_PTS.UBYTE_8")
    );
    assert!(!detail.axes[0].editable, "a shared axis is edited on its own object");

    assert_eq!(detail.axes[1].kind, "FIX_AXIS");
    assert!(!detail.axes[1].editable, "a FIX_AXIS occupies no bytes");
    let fixed: Vec<f64> = detail.axes[1].points.iter().map(|p| p.phys).collect();
    assert_eq!(fixed, vec![1.0, 2.0, 3.0], "FIX_AXIS_PAR_DIST 1 1 3");
}

/// Writing any element of a map must land on the byte the same index reads —
/// for both storage directions, and for the axes of each dimension.
#[test]
fn map_writes_where_it_reads() {
    let Some((db, img)) = open_demo() else { return };
    use a2l_data::encode::{encode_point, PointTarget};

    for name in ["ASAM.C.CUBOID.ROW_DIR", "ASAM.C.CUBOID.COLUMN_DIR"] {
        let detail = decode::detail_for(&db, &img, name).expect("detail");
        let plan = db.plan_characteristic(name).expect("present");

        for (index, point) in detail.values.iter().enumerate() {
            let w = encode_point(&db, &img, name, PointTarget::Value, index as u32, 99.0)
                .unwrap_or_else(|e| panic!("{name}[{index}]: {e}"));
            let stored = img.read(w.address, 1).expect("in image")[0] as i8;
            assert_eq!(
                f64::from(stored),
                point.phys,
                "{name}[{index}] writes {:#x}, which holds {stored} not {}",
                w.address,
                point.phys
            );
            assert!(w.address >= plan.address && w.address < plan.address + plan.byte_size());
        }

        // Every dimension's own breakpoints, addressed by dimension index.
        for (d, axis) in detail.axes.iter().enumerate() {
            for (i, point) in axis.points.iter().enumerate() {
                let w = encode_point(&db, &img, name, PointTarget::Axis(d), i as u32, 7.0)
                    .unwrap_or_else(|e| panic!("{name} axis {d}[{i}]: {e}"));
                let stored = img.read(w.address, 1).expect("in image")[0] as i8;
                assert_eq!(f64::from(stored), point.phys, "{name} axis {d}[{i}]");
            }
        }
    }
}

/// Exported maps carry their shape and one container per axis.
#[test]
fn exported_maps_declare_their_shape_and_axes() {
    let Some((db, img)) = open_demo() else { return };
    let instances = a2l_data::sync::export(&db, &img);

    let by_name = |n: &str| instances.iter().find(|i| i.name == n);

    let map = by_name("ASAM.C.MAP.STD_AXIS.STD_AXIS").expect("map exported");
    assert_eq!(map.category, "MAP");
    assert_eq!(map.array_size, vec![4, 5]);
    assert_eq!(map.values.len(), 20);
    assert_eq!(map.axes.len(), 2, "one SW-AXIS-CONT per dimension");
    assert_eq!(map.axes[0].category, "STD_AXIS");
    assert_eq!(map.axes[1].category, "STD_AXIS");

    let cuboid = by_name("ASAM.C.CUBOID.ROW_DIR").expect("cuboid exported");
    assert_eq!(cuboid.category, "CUBOID");
    assert_eq!(cuboid.array_size, vec![2, 3, 4]);

    let cube = by_name("ASAM.C.CUBE_4.ROW_DIR").expect("cube exported");
    assert_eq!(cube.category, "CUBE_4");
    assert_eq!(cube.values.len(), 48);

    // A shared axis is exported as a reference, not as duplicated points.
    let shared = by_name("ASAM.C.MAP.COM_AXIS.FIX_AXIS").expect("map exported");
    assert_eq!(
        shared.axes[0].instance_ref.as_deref(),
        Some("ASAM.C.AXIS_PTS.UBYTE_8")
    );
    assert!(shared.axes[0].values.is_empty());
}

/// Nothing should still be reported as an unsupported shape: every remaining
/// "unsupported" must be a conversion or record-layout problem, not a
/// dimension count.
#[test]
fn no_object_is_rejected_for_being_multi_dimensional() {
    let Some((db, img)) = open_demo() else { return };
    for row in decode::list_rows(&db, &img, false) {
        if row.category != Category::Unsupported {
            continue;
        }
        let note = row.note.clone().unwrap_or_default();
        assert!(
            !note.contains("dimension") && !note.contains("maps") && !note.contains("cub"),
            "{} is still rejected for its shape: {note}",
            row.name
        );
    }
}

/// The table cell says what each shape is: a curve by the span of its values,
/// a map by its dimensions. A lowest-and-highest across a whole grid carries
/// almost no information, and the decimals stepper must not turn it back into
/// a range, so the span is deliberately left unset for a map.
#[test]
fn the_table_summarises_curves_by_span_and_maps_by_shape() {
    let Some((db, img)) = open_demo() else { return };
    let rows = decode::list_rows(&db, &img, false);
    let find = |n: &str| rows.iter().find(|r| r.name == n).unwrap_or_else(|| panic!("{n}"));

    let curve = find("ASAM.C.CURVE.STD_AXIS");
    assert_eq!(curve.display, "-3.000 … 71.000");
    assert!(curve.phys_min.is_some() && curve.phys_max.is_some());

    for (name, shape) in [
        ("ASAM.C.MAP.STD_AXIS.STD_AXIS", "4 × 5"),
        ("ASAM.C.CUBOID.ROW_DIR", "2 × 3 × 4"),
        ("ASAM.C.CUBE_4.ROW_DIR", "2 × 3 × 4 × 2"),
    ] {
        let row = find(name);
        assert_eq!(row.display, shape, "{name}");
        assert!(row.phys_min.is_none(), "{name} must not also carry a span");
        assert!(row.unit.is_empty(), "{name}: a shape takes no unit");
    }
}
