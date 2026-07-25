// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

//! Raw ↔ physical conversion for the COMPU_METHOD types.
//!
//! The direction asymmetry between `LINEAR` and `RAT_FUNC` is the single
//! easiest thing to get wrong in an A2L implementation, so it is spelled out:
//!
//! * `COEFFS_LINEAR a b` gives the **forward** map, `phys = a·raw + b`.
//! * `COEFFS a b c d e f` gives the **inverse** map,
//!   `raw = (a·p² + b·p + c) / (d·p² + e·p + f)`.
//!
//! Reading `COEFFS` as if it were forward silently scales every value in the
//! file, which is why this module is unit-tested per conversion type.

/// Tolerance for matching an integer-valued raw against a table key.
const EPS: f64 = 1e-9;

/// A physical value, or an explanation of why there isn't one.
#[derive(Debug, Clone, PartialEq)]
pub enum Phys {
    Num(f64),
    Text(String),
    Unavailable(String),
}

/// A COMPU_METHOD reduced to just what conversion needs.
#[derive(Debug, Clone)]
pub enum Conversion {
    Identical,
    Linear {
        a: f64,
        b: f64,
    },
    RatFunc {
        a: f64,
        b: f64,
        c: f64,
        d: f64,
        e: f64,
        f: f64,
    },
    /// COMPU_TAB, numeric output. `interpolate` distinguishes TAB_INTP from
    /// TAB_NOINTP. Pairs are sorted by input value.
    Tab {
        pairs: Vec<(f64, f64)>,
        interpolate: bool,
        default: Option<f64>,
    },
    /// COMPU_VTAB, verbal output.
    Verb {
        pairs: Vec<(f64, String)>,
        default: Option<String>,
    },
    /// COMPU_VTAB_RANGE, verbal output keyed by an inclusive `[min, max]` band.
    /// Inverting one picks the band's lower bound as its representative.
    VerbRange {
        ranges: Vec<(f64, f64, String)>,
        default: Option<String>,
    },
    /// Recognised but not implemented — FORM, or a RAT_FUNC we refuse to guess at.
    Unsupported(String),
}

impl Conversion {
    /// Convert a raw (implementation) value to its physical counterpart.
    pub fn to_phys(&self, raw: f64) -> Phys {
        match self {
            Conversion::Identical => Phys::Num(raw),

            Conversion::Linear { a, b } => Phys::Num(a * raw + b),

            // raw = (a·p² + b·p + c) / (d·p² + e·p + f); inverted for a = d = 0.
            Conversion::RatFunc { a, b, c, d, e, f } => {
                if *a != 0.0 || *d != 0.0 {
                    return Phys::Unavailable("quadratic RAT_FUNC".into());
                }
                let denom = raw * e - b;
                if denom.abs() < EPS {
                    return Phys::Unavailable("RAT_FUNC singular".into());
                }
                Phys::Num((c - raw * f) / denom)
            }

            Conversion::Tab {
                pairs,
                interpolate,
                default,
            } => {
                if pairs.is_empty() {
                    return Phys::Unavailable("empty COMPU_TAB".into());
                }
                // Exact hit always wins, in both table flavours.
                if let Some((_, out)) = pairs.iter().find(|(i, _)| (i - raw).abs() < EPS) {
                    return Phys::Num(*out);
                }
                if *interpolate {
                    // Clamp outside the table, interpolate within it.
                    let (first_in, first_out) = pairs[0];
                    let (last_in, last_out) = pairs[pairs.len() - 1];
                    if raw <= first_in {
                        return Phys::Num(first_out);
                    }
                    if raw >= last_in {
                        return Phys::Num(last_out);
                    }
                    for w in pairs.windows(2) {
                        let (x0, y0) = w[0];
                        let (x1, y1) = w[1];
                        if raw >= x0 && raw <= x1 {
                            let t = (raw - x0) / (x1 - x0);
                            return Phys::Num(y0 + t * (y1 - y0));
                        }
                    }
                    Phys::Num(last_out)
                } else {
                    // No interpolation: a declared default takes precedence,
                    // otherwise hold the last value at or below the input.
                    if let Some(d) = default {
                        return Phys::Num(*d);
                    }
                    let mut held = pairs[0].1;
                    for (i, out) in pairs {
                        if *i <= raw {
                            held = *out;
                        } else {
                            break;
                        }
                    }
                    Phys::Num(held)
                }
            }

            Conversion::Verb { pairs, default } => {
                if let Some((_, text)) = pairs.iter().find(|(i, _)| (i - raw).abs() < EPS) {
                    return Phys::Text(text.clone());
                }
                match default {
                    Some(d) => Phys::Text(d.clone()),
                    None => Phys::Unavailable(format!("no COMPU_VTAB entry for {raw}")),
                }
            }

            Conversion::VerbRange { ranges, default } => {
                if let Some((_, _, text)) = ranges
                    .iter()
                    .find(|(lo, hi, _)| raw >= *lo - EPS && raw <= *hi + EPS)
                {
                    return Phys::Text(text.clone());
                }
                match default {
                    Some(d) => Phys::Text(d.clone()),
                    None => Phys::Unavailable(format!("no COMPU_VTAB_RANGE band for {raw}")),
                }
            }

            Conversion::Unsupported(reason) => Phys::Unavailable(reason.clone()),
        }
    }

    /// Convert a numeric physical value back to raw.
    pub fn to_raw(&self, phys: f64) -> Option<f64> {
        match self {
            Conversion::Identical => Some(phys),

            Conversion::Linear { a, b } => {
                if a.abs() < EPS {
                    None
                } else {
                    Some((phys - b) / a)
                }
            }

            // COEFFS is already the phys → raw direction, so evaluate directly.
            Conversion::RatFunc { a, b, c, d, e, f } => {
                if *a != 0.0 || *d != 0.0 {
                    return None;
                }
                let denom = e * phys + f;
                if denom.abs() < EPS {
                    None
                } else {
                    Some((b * phys + c) / denom)
                }
            }

            Conversion::Tab {
                pairs, interpolate, ..
            } => {
                let dir = table_monotonic_dir(pairs)?;
                // Walk the output axis in increasing order to bracket `phys`.
                let ordered: Vec<(f64, f64)> = if dir > 0 {
                    pairs.iter().map(|(i, o)| (*o, *i)).collect()
                } else {
                    pairs.iter().rev().map(|(i, o)| (*o, *i)).collect()
                };
                if let Some((_, raw)) = ordered.iter().find(|(o, _)| (o - phys).abs() < EPS) {
                    return Some(*raw);
                }
                if !*interpolate {
                    // Step tables only invert on exact outputs.
                    return None;
                }
                let (first_o, first_r) = ordered[0];
                let (last_o, last_r) = ordered[ordered.len() - 1];
                if phys <= first_o {
                    return Some(first_r);
                }
                if phys >= last_o {
                    return Some(last_r);
                }
                for w in ordered.windows(2) {
                    let (o0, r0) = w[0];
                    let (o1, r1) = w[1];
                    if phys >= o0 && phys <= o1 {
                        let t = (phys - o0) / (o1 - o0);
                        return Some(r0 + t * (r1 - r0));
                    }
                }
                None
            }

            // A verbal value is not a number; use `text_to_raw`.
            Conversion::Verb { .. } | Conversion::VerbRange { .. } => None,

            Conversion::Unsupported(_) => None,
        }
    }

    /// Convert a verbal physical value back to raw.
    pub fn text_to_raw(&self, text: &str) -> Option<f64> {
        match self {
            Conversion::Verb { pairs, .. } => {
                pairs.iter().find(|(_, t)| t == text).map(|(i, _)| *i)
            }
            Conversion::VerbRange { ranges, .. } => ranges
                .iter()
                .find(|(_, _, t)| t == text)
                .map(|(lo, _, _)| *lo),
            _ => None,
        }
    }

    /// Whether a physical value can be written back through this conversion.
    pub fn is_invertible(&self) -> bool {
        match self {
            Conversion::Identical => true,
            Conversion::Linear { a, .. } => a.abs() >= EPS,
            Conversion::RatFunc { a, b, d, e, f, .. } => {
                *a == 0.0 && *d == 0.0 && (e.abs() >= EPS || f.abs() >= EPS) && b.abs() >= EPS
            }
            Conversion::Tab { pairs, .. } => table_monotonic_dir(pairs).is_some(),
            Conversion::Verb { pairs, .. } => !pairs.is_empty(),
            Conversion::VerbRange { ranges, .. } => !ranges.is_empty(),
            Conversion::Unsupported(_) => false,
        }
    }

    /// The enum labels of a verbal conversion, for a dropdown editor.
    pub fn enum_options(&self) -> Option<Vec<String>> {
        match self {
            Conversion::Verb { pairs, .. } => {
                Some(pairs.iter().map(|(_, t)| t.clone()).collect())
            }
            Conversion::VerbRange { ranges, .. } => {
                Some(ranges.iter().map(|(_, _, t)| t.clone()).collect())
            }
            _ => None,
        }
    }
}

/// Returns `Some(1)` when the table's outputs strictly increase, `Some(-1)`
/// when they strictly decrease, `None` when it is not invertible.
fn table_monotonic_dir(pairs: &[(f64, f64)]) -> Option<i32> {
    if pairs.len() < 2 {
        return if pairs.is_empty() { None } else { Some(1) };
    }
    let inc = pairs.windows(2).all(|w| w[1].1 > w[0].1);
    if inc {
        return Some(1);
    }
    let dec = pairs.windows(2).all(|w| w[1].1 < w[0].1);
    if dec {
        return Some(-1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(p: Phys) -> f64 {
        match p {
            Phys::Num(v) => v,
            other => panic!("expected a number, got {other:?}"),
        }
    }

    #[test]
    fn identical_is_a_passthrough() {
        let c = Conversion::Identical;
        assert_eq!(num(c.to_phys(20.0)), 20.0);
        assert_eq!(c.to_raw(20.0), Some(20.0));
    }

    #[test]
    fn linear_coeffs_are_the_forward_direction() {
        // COEFFS_LINEAR 2 5  =>  phys = 2*raw + 5
        let c = Conversion::Linear { a: 2.0, b: 5.0 };
        assert_eq!(num(c.to_phys(10.0)), 25.0);
        assert_eq!(c.to_raw(25.0), Some(10.0));
    }

    #[test]
    fn linear_with_zero_slope_is_not_invertible() {
        let c = Conversion::Linear { a: 0.0, b: 5.0 };
        assert!(!c.is_invertible());
        assert_eq!(c.to_raw(5.0), None);
    }

    #[test]
    fn rat_func_coeffs_are_the_inverse_direction() {
        // The demo file's CM.RAT_FUNC.IDENT: COEFFS 0 1 0 0 0 1
        // means raw = (1*p + 0) / 1 = p, so it must behave as identity.
        let c = Conversion::RatFunc {
            a: 0.0,
            b: 1.0,
            c: 0.0,
            d: 0.0,
            e: 0.0,
            f: 1.0,
        };
        assert_eq!(num(c.to_phys(42.0)), 42.0);
        assert_eq!(c.to_raw(42.0), Some(42.0));
    }

    #[test]
    fn rat_func_scaling_inverts_correctly() {
        // COEFFS 0 2 0 0 0 1  =>  raw = 2*phys, therefore phys = raw/2.
        // Reading these coefficients forward would give phys = 2*raw — the
        // exact mistake this test exists to catch.
        let c = Conversion::RatFunc {
            a: 0.0,
            b: 2.0,
            c: 0.0,
            d: 0.0,
            e: 0.0,
            f: 1.0,
        };
        assert_eq!(num(c.to_phys(100.0)), 50.0);
        assert_eq!(c.to_raw(50.0), Some(100.0));
    }

    #[test]
    fn rat_func_with_offset_round_trips() {
        // raw = (2*p + 10) / 1
        let c = Conversion::RatFunc {
            a: 0.0,
            b: 2.0,
            c: 10.0,
            d: 0.0,
            e: 0.0,
            f: 1.0,
        };
        // phys = (c - raw*f) / (raw*e - b) = (10 - 30) / (0 - 2) = 10
        assert_eq!(num(c.to_phys(30.0)), 10.0);
        assert_eq!(c.to_raw(10.0), Some(30.0));
    }

    #[test]
    fn quadratic_rat_func_is_refused_rather_than_guessed() {
        let c = Conversion::RatFunc {
            a: 1.0,
            b: 1.0,
            c: 0.0,
            d: 0.0,
            e: 0.0,
            f: 1.0,
        };
        assert!(matches!(c.to_phys(5.0), Phys::Unavailable(_)));
        assert!(!c.is_invertible());
    }

    #[test]
    fn tab_intp_interpolates_and_clamps() {
        // Mirrors CM.TAB_INTP.*.REF in the demo file.
        let c = Conversion::Tab {
            pairs: vec![(-3.0, 98.0), (-1.0, 99.0), (0.0, 100.0), (2.0, 102.0)],
            interpolate: true,
            default: None,
        };
        assert_eq!(num(c.to_phys(0.0)), 100.0); // exact
        assert_eq!(num(c.to_phys(1.0)), 101.0); // halfway between 0 and 2
        assert_eq!(num(c.to_phys(-99.0)), 98.0); // clamped low
        assert_eq!(num(c.to_phys(99.0)), 102.0); // clamped high
    }

    #[test]
    fn tab_intp_inverts() {
        let c = Conversion::Tab {
            pairs: vec![(0.0, 100.0), (2.0, 102.0)],
            interpolate: true,
            default: None,
        };
        assert_eq!(c.to_raw(101.0), Some(1.0));
    }

    #[test]
    fn tab_nointp_holds_the_last_value_below_the_input() {
        let c = Conversion::Tab {
            pairs: vec![(0.0, 10.0), (5.0, 20.0), (10.0, 30.0)],
            interpolate: false,
            default: None,
        };
        assert_eq!(num(c.to_phys(5.0)), 20.0);
        assert_eq!(num(c.to_phys(7.0)), 20.0);
    }

    #[test]
    fn tab_nointp_prefers_a_declared_default_for_a_miss() {
        let c = Conversion::Tab {
            pairs: vec![(0.0, 10.0), (5.0, 20.0)],
            interpolate: false,
            default: Some(300.56),
        };
        assert_eq!(num(c.to_phys(5.0)), 20.0); // exact hit still wins
        assert_eq!(num(c.to_phys(3.0)), 300.56);
    }

    #[test]
    fn non_monotonic_table_is_not_invertible() {
        let c = Conversion::Tab {
            pairs: vec![(0.0, 10.0), (1.0, 5.0), (2.0, 10.0)],
            interpolate: true,
            default: None,
        };
        assert!(!c.is_invertible());
    }

    #[test]
    fn tab_verb_resolves_labels_and_falls_back_to_default() {
        // Mirrors CM.TAB_VERB.DEFAULT_VALUE.REF in the demo file.
        let c = Conversion::Verb {
            pairs: vec![
                (1.0, "SawTooth".into()),
                (2.0, "Square".into()),
                (3.0, "Sinus".into()),
            ],
            default: Some("unknown signal type".into()),
        };
        assert_eq!(c.to_phys(2.0), Phys::Text("Square".into()));
        assert_eq!(c.to_phys(9.0), Phys::Text("unknown signal type".into()));
        assert_eq!(c.text_to_raw("Sinus"), Some(3.0));
        assert_eq!(c.text_to_raw("nope"), None);
        assert_eq!(
            c.enum_options(),
            Some(vec![
                "SawTooth".to_string(),
                "Square".to_string(),
                "Sinus".to_string()
            ])
        );
    }

    #[test]
    fn tab_verb_range_matches_inclusive_bands() {
        // Mirrors CM.VTAB_RANGE.DEFAULT_VALUE.REF in the demo file.
        let c = Conversion::VerbRange {
            ranges: vec![
                (0.0, 1.0, "Zero_to_one".into()),
                (2.0, 3.0, "two_to_three".into()),
                (4.0, 7.0, "four_to_seven".into()),
                (100.0, 100.0, "hundred".into()),
            ],
            default: Some("out of range value".into()),
        };
        assert_eq!(c.to_phys(0.0), Phys::Text("Zero_to_one".into()));
        assert_eq!(c.to_phys(1.0), Phys::Text("Zero_to_one".into()));
        assert_eq!(c.to_phys(5.0), Phys::Text("four_to_seven".into()));
        assert_eq!(c.to_phys(100.0), Phys::Text("hundred".into()));
        // Between bands falls to the default, not to a neighbour.
        assert_eq!(c.to_phys(8.0), Phys::Text("out of range value".into()));
        // Inverting picks the band's lower bound.
        assert_eq!(c.text_to_raw("four_to_seven"), Some(4.0));
        assert!(c.is_invertible());
    }

    #[test]
    fn tab_verb_without_default_reports_the_miss() {
        let c = Conversion::Verb {
            pairs: vec![(253.0, "Sensor not calibrated".into())],
            default: None,
        };
        assert!(matches!(c.to_phys(1.0), Phys::Unavailable(_)));
    }

    /// Every invertible conversion must survive a raw → phys → raw round trip.
    #[test]
    fn round_trips_preserve_raw_values() {
        let cases = vec![
            Conversion::Identical,
            Conversion::Linear { a: 0.5, b: -3.0 },
            Conversion::RatFunc {
                a: 0.0,
                b: 4.0,
                c: 7.0,
                d: 0.0,
                e: 0.0,
                f: 2.0,
            },
            Conversion::Tab {
                pairs: vec![(0.0, 0.0), (10.0, 50.0), (20.0, 200.0)],
                interpolate: true,
                default: None,
            },
        ];
        for c in cases {
            assert!(c.is_invertible(), "{c:?} should be invertible");
            for raw in [0.0_f64, 1.0, 7.0, 12.5, 20.0] {
                let phys = match c.to_phys(raw) {
                    Phys::Num(v) => v,
                    other => panic!("{c:?} gave {other:?} for {raw}"),
                };
                let back = c.to_raw(phys).expect("inverse exists");
                assert!(
                    (back - raw).abs() < 1e-6,
                    "{c:?}: {raw} -> {phys} -> {back}"
                );
            }
        }
    }
}
