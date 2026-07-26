// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

//! Reading and writing ASAM CDF 2.1 calibration data files (`.cdfx`).
//!
//! A CDFX records *physical* values keyed by A2L object name, which is exactly
//! the currency the rest of this crate deals in — importing is therefore a
//! matter of looking each name up and handing the values to the existing
//! encoders, and exporting a matter of emitting what the decoders produce.
//!
//! Only the value-bearing subset of the schema is modelled. Structures this
//! crate cannot decode — maps, cuboids, blobs — are skipped rather than
//! guessed at, and counted so the user is told what was left alone.

use std::collections::HashMap;

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use serde::{Deserialize, Serialize};

/// A value in a CDFX file: numeric, or verbal for an enum or string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CdfxValue {
    Num(f64),
    Text(String),
}

impl CdfxValue {
    pub fn as_num(&self) -> Option<f64> {
        match self {
            CdfxValue::Num(v) => Some(*v),
            CdfxValue::Text(_) => None,
        }
    }

    pub fn display(&self) -> String {
        match self {
            CdfxValue::Num(v) => crate::decode::format_number(*v, ""),
            CdfxValue::Text(t) => t.clone(),
        }
    }
}

/// One axis of an instance. Either the breakpoints themselves, or a reference
/// to the shared AXIS_PTS object that holds them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CdfxAxis {
    pub category: String,
    pub values: Vec<CdfxValue>,
    pub instance_ref: Option<String>,
}

/// One `SW-INSTANCE`: a single calibration parameter.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CdfxInstance {
    pub name: String,
    /// `VALUE`, `CURVE`, `MAP`, `ASCII`, … as the file declares it.
    pub category: String,
    pub values: Vec<CdfxValue>,
    /// `SW-ARRAYSIZE`: the declared dimensions, first dimension first. Empty
    /// for a scalar or a plain one-dimensional list.
    ///
    /// The values are always a flat list in row-major order; this says how to
    /// fold it back into a matrix, which is otherwise unrecoverable from the
    /// file alone.
    pub array_size: Vec<u32>,
    pub axes: Vec<CdfxAxis>,
}

/// A parsed CDFX file.
#[derive(Debug, Clone, Default)]
pub struct CdfxFile {
    pub short_name: String,
    pub instances: Vec<CdfxInstance>,
}

impl CdfxFile {
    pub fn get(&self, name: &str) -> Option<&CdfxInstance> {
        self.instances.iter().find(|i| i.name == name)
    }
}

/// Parse a CDFX document.
///
/// The schema is walked with a small state machine rather than a full DOM: the
/// handful of elements that carry values are unambiguous once you know whether
/// you are inside a value container or an axis container.
pub fn parse(xml: &str) -> Result<CdfxFile, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    // Report a truncated or mismatched document instead of returning whatever
    // was parsed before the input ran out.
    reader.config_mut().check_end_names = true;

    let mut out = CdfxFile::default();
    let mut inst: Option<CdfxInstance> = None;
    let mut axis: Option<CdfxAxis> = None;

    // Where the parser currently is. `SW-VALUES-PHYS` appears in both value and
    // axis containers, so the enclosing container decides where text lands.
    let mut in_axis_cont = false;
    let mut in_values_phys = false;
    let mut in_arraysize = false;
    let mut tag = String::new();
    let mut seen_root_name = false;

    // Element content arrives as several events when it contains entity
    // references: `&#65;&#66;` is two GeneralRefs with no Text between them, and
    // an escaped `a&lt;b` is Text, GeneralRef, Text. Content is therefore
    // accumulated and only committed when the element closes — treating each
    // event as a whole value splits one string into several.
    let mut buf = String::new();
    // Guards against a truncated document: quick-xml reaches Eof happily with
    // elements still open, which would otherwise yield a half-read file.
    let mut depth: i32 = 0;

    loop {
        match reader.read_event() {
            Err(e) => return Err(format!("XML error at {}: {e}", reader.buffer_position())),
            Ok(Event::Eof) => {
                if depth != 0 {
                    return Err(format!("unexpected end of document: {depth} element(s) left open"));
                }
                break;
            }

            Ok(Event::Start(e)) => {
                let name = start_name(&e);
                match name.as_str() {
                    "SW-INSTANCE" => inst = Some(CdfxInstance::default()),
                    "SW-AXIS-CONT" => {
                        in_axis_cont = true;
                        axis = Some(CdfxAxis::default());
                    }
                    "SW-VALUES-PHYS" => in_values_phys = true,
                    "SW-ARRAYSIZE" => in_arraysize = true,
                    _ => {}
                }
                tag = name;
                buf.clear();
                depth += 1;
            }

            Ok(Event::Text(t)) => buf.push_str(&decode_text(&t)?),

            // A reference arrives as its name only — `lt`, or `#65` for a
            // character entity — so it is rebuilt and handed to the library's
            // resolver rather than decoded by hand.
            Ok(Event::GeneralRef(r)) => {
                let name = r
                    .xml10_content()
                    .map_err(|e| format!("bad entity: {e}"))?;
                let reference = format!("&{name};");
                let resolved = quick_xml::escape::unescape(&reference)
                    .map_err(|e| format!("unknown entity '{reference}': {e}"))?;
                buf.push_str(&resolved);
            }

            Ok(Event::End(e)) => {
                let name = end_name(&e);

                // Commit this element's accumulated content, if it is one that
                // carries any.
                if name == tag {
                    let text = std::mem::take(&mut buf);
                    match name.as_str() {
                        "SHORT-NAME" => {
                            if let Some(i) = inst.as_mut() {
                                if i.name.is_empty() {
                                    i.name = text;
                                }
                            } else if !seen_root_name {
                                out.short_name = text;
                                seen_root_name = true;
                            }
                        }
                        "CATEGORY" => {
                            if let Some(a) = axis.as_mut() {
                                a.category = text;
                            } else if let Some(i) = inst.as_mut() {
                                if i.category.is_empty() {
                                    i.category = text;
                                }
                            }
                        }
                        "SW-INSTANCE-REF" => {
                            if let Some(a) = axis.as_mut() {
                                a.instance_ref = Some(text);
                            }
                        }
                        // Dimensions are structural rather than data: the flat
                        // value list is what the encoders consume, but the
                        // shape is kept so a round-trip preserves it.
                        "V" if in_arraysize => {
                            if let (Some(i), Ok(n)) = (inst.as_mut(), text.trim().parse::<u32>()) {
                                i.array_size.push(n);
                            }
                        }
                        "V" | "VT" if in_values_phys => {
                            let v = if name == "VT" {
                                CdfxValue::Text(text)
                            } else {
                                match text.trim().parse::<f64>() {
                                    Ok(n) => CdfxValue::Num(n),
                                    // A non-numeric <V> is malformed; keep it
                                    // as text so the mismatch surfaces
                                    // downstream rather than becoming a zero.
                                    Err(_) => CdfxValue::Text(text),
                                }
                            };
                            if in_axis_cont {
                                if let Some(a) = axis.as_mut() {
                                    a.values.push(v);
                                }
                            } else if let Some(i) = inst.as_mut() {
                                i.values.push(v);
                            }
                        }
                        _ => {}
                    }
                }

                match name.as_str() {
                    "SW-INSTANCE" => {
                        if let Some(i) = inst.take() {
                            if !i.name.is_empty() {
                                out.instances.push(i);
                            }
                        }
                    }
                    "SW-AXIS-CONT" => {
                        in_axis_cont = false;
                        if let (Some(a), Some(i)) = (axis.take(), inst.as_mut()) {
                            i.axes.push(a);
                        }
                    }
                    "SW-VALUES-PHYS" => in_values_phys = false,
                    "SW-ARRAYSIZE" => in_arraysize = false,
                    _ => {}
                }
                tag.clear();
                buf.clear();
                depth -= 1;
            }

            _ => {}
        }
    }

    Ok(out)
}

fn start_name(e: &BytesStart) -> String {
    String::from_utf8_lossy(e.local_name().as_ref()).to_string()
}

fn end_name(e: &BytesEnd) -> String {
    String::from_utf8_lossy(e.local_name().as_ref()).to_string()
}

/// Text with entity references resolved, so `&#65;` arrives as `A`.
fn decode_text(t: &BytesText) -> Result<String, String> {
    t.xml10_content()
        .map(|s| s.into_owned())
        .map_err(|e| format!("bad text: {e}"))
}

// ── Writing ──────────────────────────────────────────────────────────────────

/// Serialise instances as a CDF 2.1 document.
///
/// The result carries the values and the structure needed to read them back,
/// but not the descriptive metadata a calibration tool would add — feature
/// groups, revision history, unit declarations beyond the display name. That is
/// a deliberate limit: this crate does not model those, and inventing them
/// would put invented provenance in the file.
/// Serialise instances as a CDFX document.
///
/// `creator` names the application that produced the file, version included —
/// it is passed in rather than taken from this crate's own `CARGO_PKG_VERSION`,
/// which would stamp every export with the library's version number and read
/// to anyone opening the file as the application's.
pub fn write(
    short_name: &str,
    creator: &str,
    instances: &[CdfxInstance],
) -> Result<String, String> {
    let mut w = Writer::new_with_indent(Vec::new(), b'\t', 1);
    let err = |e: std::io::Error| format!("write error: {e}");

    w.write_event(Event::Decl(quick_xml::events::BytesDecl::new(
        "1.0",
        Some("utf-8"),
        None,
    )))
    .map_err(err)?;
    w.write_event(Event::DocType(BytesText::from_escaped(
        r#" MSRSW PUBLIC "-//ASAM//DTD CALIBRATION DATA FORMAT:V2.1:LAI:IAI:XML //EN" "cdf_v2.1.0.sl.dtd""#,
    )))
    .map_err(err)?;

    let mut msrsw = BytesStart::new("MSRSW");
    msrsw.push_attribute(("CREATOR", creator));
    w.write_event(Event::Start(msrsw)).map_err(err)?;

    text_el(&mut w, "SHORT-NAME", short_name)?;
    text_el(&mut w, "CATEGORY", "CDF21")?;
    open(&mut w, "SW-SYSTEMS")?;
    open(&mut w, "SW-SYSTEM")?;
    text_el(&mut w, "SHORT-NAME", short_name)?;
    open(&mut w, "SW-INSTANCE-SPEC")?;
    open(&mut w, "SW-INSTANCE-TREE")?;
    text_el(&mut w, "SHORT-NAME", short_name)?;
    text_el(&mut w, "CATEGORY", "NO_VCD")?;

    for inst in instances {
        open(&mut w, "SW-INSTANCE")?;
        text_el(&mut w, "SHORT-NAME", &inst.name)?;
        text_el(&mut w, "CATEGORY", &inst.category)?;

        open(&mut w, "SW-VALUE-CONT")?;
        if inst.array_size.len() > 1 {
            open(&mut w, "SW-ARRAYSIZE")?;
            for d in &inst.array_size {
                text_el(&mut w, "V", &d.to_string())?;
            }
            close(&mut w, "SW-ARRAYSIZE")?;
        }
        open(&mut w, "SW-VALUES-PHYS")?;
        for v in &inst.values {
            write_value(&mut w, v)?;
        }
        close(&mut w, "SW-VALUES-PHYS")?;
        close(&mut w, "SW-VALUE-CONT")?;

        if !inst.axes.is_empty() {
            open(&mut w, "SW-AXIS-CONTS")?;
            for a in &inst.axes {
                open(&mut w, "SW-AXIS-CONT")?;
                text_el(&mut w, "CATEGORY", &a.category)?;
                if let Some(r) = &a.instance_ref {
                    text_el(&mut w, "SW-INSTANCE-REF", r)?;
                } else {
                    open(&mut w, "SW-VALUES-PHYS")?;
                    for v in &a.values {
                        write_value(&mut w, v)?;
                    }
                    close(&mut w, "SW-VALUES-PHYS")?;
                }
                close(&mut w, "SW-AXIS-CONT")?;
            }
            close(&mut w, "SW-AXIS-CONTS")?;
        }

        close(&mut w, "SW-INSTANCE")?;
    }

    close(&mut w, "SW-INSTANCE-TREE")?;
    close(&mut w, "SW-INSTANCE-SPEC")?;
    close(&mut w, "SW-SYSTEM")?;
    close(&mut w, "SW-SYSTEMS")?;
    close(&mut w, "MSRSW")?;

    String::from_utf8(w.into_inner()).map_err(|e| format!("not valid UTF-8: {e}"))
}

fn write_value(w: &mut Writer<Vec<u8>>, v: &CdfxValue) -> Result<(), String> {
    match v {
        CdfxValue::Num(n) => text_el(w, "V", &exact(*n)),
        CdfxValue::Text(t) => {
            let mut e = BytesStart::new("VT");
            e.push_attribute(("xml:space", "preserve"));
            w.write_event(Event::Start(e))
                .map_err(|e: std::io::Error| format!("write error: {e}"))?;
            w.write_event(Event::Text(BytesText::new(t)))
                .map_err(|e: std::io::Error| format!("write error: {e}"))?;
            close(w, "VT")
        }
    }
}

/// Render a number so that reading it back yields the same `f64`.
///
/// Tools that write CDFX usually round to the A2L `FORMAT`, which is right for
/// something a person reads and wrong for something re-imported: the demo
/// file's `-3` is really -3.000000491882041, and re-importing the rounded text
/// would rewrite the image. Rust's `Display` for `f64` gives the shortest
/// decimal that round-trips, which is exactly what is wanted here; the
/// exponent form is a guard against a subnormal turning into 300 digits.
pub(crate) fn exact(n: f64) -> String {
    if !n.is_finite() {
        return "0".into();
    }
    let plain = format!("{n}");
    if plain.len() > 24 {
        format!("{n:e}")
    } else {
        plain
    }
}

fn open(w: &mut Writer<Vec<u8>>, name: &str) -> Result<(), String> {
    w.write_event(Event::Start(BytesStart::new(name)))
        .map_err(|e: std::io::Error| format!("write error: {e}"))
}

fn close(w: &mut Writer<Vec<u8>>, name: &str) -> Result<(), String> {
    w.write_event(Event::End(BytesEnd::new(name)))
        .map_err(|e: std::io::Error| format!("write error: {e}"))
}

fn text_el(w: &mut Writer<Vec<u8>>, name: &str, text: &str) -> Result<(), String> {
    open(w, name)?;
    w.write_event(Event::Text(BytesText::new(text)))
        .map_err(|e: std::io::Error| format!("write error: {e}"))?;
    close(w, name)
}

/// Index instances by name for lookup during import.
pub fn index(file: &CdfxFile) -> HashMap<&str, &CdfxInstance> {
    file.instances
        .iter()
        .map(|i| (i.name.as_str(), i))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<MSRSW>
  <SHORT-NAME>Demo</SHORT-NAME>
  <SW-SYSTEMS><SW-SYSTEM>
    <SHORT-NAME>sys</SHORT-NAME>
    <SW-INSTANCE-SPEC><SW-INSTANCE-TREE>
      <SHORT-NAME>tree</SHORT-NAME>
      <SW-INSTANCE>
        <SHORT-NAME>SCALAR.ONE</SHORT-NAME>
        <CATEGORY>VALUE</CATEGORY>
        <SW-VALUE-CONT><SW-VALUES-PHYS><V>20</V></SW-VALUES-PHYS></SW-VALUE-CONT>
      </SW-INSTANCE>
      <SW-INSTANCE>
        <SHORT-NAME>ENUM.ONE</SHORT-NAME>
        <CATEGORY>VALUE</CATEGORY>
        <SW-VALUE-CONT><SW-VALUES-PHYS><VT xml:space="preserve">Square</VT></SW-VALUES-PHYS></SW-VALUE-CONT>
      </SW-INSTANCE>
      <SW-INSTANCE>
        <SHORT-NAME>STR.ONE</SHORT-NAME>
        <CATEGORY>ASCII</CATEGORY>
        <SW-VALUE-CONT><SW-VALUES-PHYS><VT xml:space="preserve">&#65;&#66;</VT></SW-VALUES-PHYS></SW-VALUE-CONT>
      </SW-INSTANCE>
      <SW-INSTANCE>
        <SHORT-NAME>CURVE.ONE</SHORT-NAME>
        <CATEGORY>CURVE</CATEGORY>
        <SW-VALUE-CONT><SW-VALUES-PHYS><V>-3</V><V>7</V></SW-VALUES-PHYS></SW-VALUE-CONT>
        <SW-AXIS-CONTS><SW-AXIS-CONT>
          <CATEGORY>STD_AXIS</CATEGORY>
          <SW-VALUES-PHYS><V>-5</V><V>22</V></SW-VALUES-PHYS>
        </SW-AXIS-CONT></SW-AXIS-CONTS>
      </SW-INSTANCE>
      <SW-INSTANCE>
        <SHORT-NAME>CURVE.SHARED</SHORT-NAME>
        <CATEGORY>CURVE</CATEGORY>
        <SW-VALUE-CONT>
          <SW-ARRAYSIZE><V>2</V></SW-ARRAYSIZE>
          <SW-VALUES-PHYS><V>1</V><V>2</V></SW-VALUES-PHYS>
        </SW-VALUE-CONT>
        <SW-AXIS-CONTS><SW-AXIS-CONT>
          <CATEGORY>COM_AXIS</CATEGORY>
          <SW-INSTANCE-REF>THE.AXIS</SW-INSTANCE-REF>
        </SW-AXIS-CONT></SW-AXIS-CONTS>
      </SW-INSTANCE>
      <SW-INSTANCE>
        <SHORT-NAME>MAP.ONE</SHORT-NAME>
        <CATEGORY>MAP</CATEGORY>
        <SW-VALUE-CONT><SW-VALUES-PHYS>
          <VG><LABEL>r</LABEL><V>1</V><V>2</V></VG>
          <VG><LABEL>g</LABEL><V>3</V><V>4</V></VG>
        </SW-VALUES-PHYS></SW-VALUE-CONT>
      </SW-INSTANCE>
    </SW-INSTANCE-TREE></SW-INSTANCE-SPEC>
  </SW-SYSTEM></SW-SYSTEMS>
</MSRSW>"#;

    #[test]
    fn reads_every_instance() {
        let f = parse(SAMPLE).expect("parse");
        assert_eq!(f.short_name, "Demo");
        assert_eq!(f.instances.len(), 6);
        // The tree and system SHORT-NAMEs must not be mistaken for instances.
        assert!(f.get("sys").is_none());
        assert!(f.get("tree").is_none());
    }

    #[test]
    fn reads_numeric_and_verbal_values() {
        let f = parse(SAMPLE).expect("parse");
        assert_eq!(f.get("SCALAR.ONE").unwrap().values, vec![CdfxValue::Num(20.0)]);
        assert_eq!(
            f.get("ENUM.ONE").unwrap().values,
            vec![CdfxValue::Text("Square".into())]
        );
    }

    /// The demo file writes its strings as character entities.
    #[test]
    fn decodes_character_entities() {
        let f = parse(SAMPLE).expect("parse");
        assert_eq!(
            f.get("STR.ONE").unwrap().values,
            vec![CdfxValue::Text("AB".into())]
        );
    }

    #[test]
    fn separates_axis_values_from_function_values() {
        let f = parse(SAMPLE).expect("parse");
        let c = f.get("CURVE.ONE").unwrap();
        assert_eq!(c.values, vec![CdfxValue::Num(-3.0), CdfxValue::Num(7.0)]);
        assert_eq!(c.axes.len(), 1);
        assert_eq!(c.axes[0].category, "STD_AXIS");
        assert_eq!(
            c.axes[0].values,
            vec![CdfxValue::Num(-5.0), CdfxValue::Num(22.0)]
        );
        assert_eq!(c.axes[0].instance_ref, None);
    }

    #[test]
    fn records_a_shared_axis_as_a_reference() {
        let f = parse(SAMPLE).expect("parse");
        let c = f.get("CURVE.SHARED").unwrap();
        assert_eq!(c.axes[0].instance_ref.as_deref(), Some("THE.AXIS"));
        assert!(c.axes[0].values.is_empty(), "a reference carries no values");
        // SW-ARRAYSIZE must not leak into the value list.
        assert_eq!(c.values, vec![CdfxValue::Num(1.0), CdfxValue::Num(2.0)]);
    }

    /// Grouped values flatten in document order, which is what an importer
    /// consumes; the grouping itself is only meaningful for shapes this crate
    /// does not write back.
    #[test]
    fn flattens_grouped_values() {
        let f = parse(SAMPLE).expect("parse");
        let m = f.get("MAP.ONE").unwrap();
        assert_eq!(m.category, "MAP");
        assert_eq!(
            m.values,
            vec![
                CdfxValue::Num(1.0),
                CdfxValue::Num(2.0),
                CdfxValue::Num(3.0),
                CdfxValue::Num(4.0)
            ]
        );
    }

    #[test]
    fn round_trips_through_write_and_parse() {
        let original = parse(SAMPLE).expect("parse");
        // Drop the shapes this crate does not model, as an export would.
        let keep: Vec<CdfxInstance> = original
            .instances
            .iter()
            .filter(|i| i.category != "MAP")
            .cloned()
            .collect();

        let xml = write("Demo", "test", &keep).expect("write");
        let back = parse(&xml).expect("reparse");

        assert_eq!(back.short_name, "Demo");
        assert_eq!(back.instances.len(), keep.len());
        for (a, b) in keep.iter().zip(&back.instances) {
            assert_eq!(a.name, b.name, "name survives");
            assert_eq!(a.category, b.category, "category survives");
            assert_eq!(a.values, b.values, "{}: values survive", a.name);
            assert_eq!(a.axes.len(), b.axes.len(), "{}: axis count", a.name);
            for (x, y) in a.axes.iter().zip(&b.axes) {
                assert_eq!(x.values, y.values, "{}: axis values", a.name);
                assert_eq!(x.instance_ref, y.instance_ref, "{}: axis ref", a.name);
            }
        }
    }

    /// Without SW-ARRAYSIZE a 3x4 block is indistinguishable from a flat list
    /// of twelve, so the shape has to survive the round trip.
    #[test]
    fn array_dimensions_survive_a_round_trip() {
        let inst = CdfxInstance {
            name: "M".into(),
            category: "VAL_BLK".into(),
            values: (1..=12).map(|v| CdfxValue::Num(f64::from(v))).collect(),
            array_size: vec![3, 4],
            axes: vec![],
        };
        let back = parse(&write("D", "test", &[inst]).expect("write")).expect("reparse");
        assert_eq!(back.instances[0].array_size, vec![3, 4]);
        assert_eq!(back.instances[0].values.len(), 12, "dimensions are not values");
    }

    #[test]
    fn written_text_is_escaped_and_read_back_intact() {
        let inst = CdfxInstance {
            name: "T".into(),
            category: "ASCII".into(),
            values: vec![CdfxValue::Text("a<b&c\"d".into())],
            array_size: Vec::new(),
            axes: vec![],
        };
        let xml = write("D", "test", &[inst]).expect("write");
        let back = parse(&xml).expect("reparse");
        assert_eq!(
            back.instances[0].values,
            vec![CdfxValue::Text("a<b&c\"d".into())]
        );
    }

    #[test]
    fn rejects_malformed_xml() {
        assert!(parse("<MSRSW><SW-INSTANCE>").is_err());
    }
}
