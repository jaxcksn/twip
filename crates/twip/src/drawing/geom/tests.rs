//! Geometry evaluator tests

use super::{Geom, Geometry, Preset, Seg};
use crate::drawing::units::Emu;

fn preset(name: &str, adj: &[(String, i64)]) -> Geometry {
    Geometry::Preset { preset: Preset::from_name(name).expect("a preset"), adj: adj.to_vec() }
}

fn emu(pt: f64) -> Emu {
    Emu::from_pt(pt)
}

fn flatten(g: &Geom) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for p in &g.paths {
        let (mut cur, mut start) = ([0.0, 0.0], [0.0, 0.0]);
        let line = |a: [f64; 2], b: [f64; 2], out: &mut Vec<[f64; 2]>| {
            let n = (((b[0] - a[0]).hypot(b[1] - a[1])) / 0.1).ceil().max(1.0) as usize;
            for i in 0..=n {
                let t = i as f64 / n as f64;
                out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
            }
        };
        for s in &p.segs {
            match *s {
                Seg::Move(q) => {
                    (cur, start) = (q, q);
                    out.push(q);
                }
                Seg::Line(q) => {
                    line(cur, q, &mut out);
                    cur = q;
                }
                Seg::Quad(c, q) => {
                    let mut prev = cur;
                    for i in 1..=64 {
                        let t = i as f64 / 64.0;
                        let u = 1.0 - t;
                        let p = [u * u * cur[0] + 2.0 * u * t * c[0] + t * t * q[0], u * u * cur[1] + 2.0 * u * t * c[1] + t * t * q[1]];
                        line(prev, p, &mut out);
                        prev = p;
                    }
                    cur = q;
                }
                Seg::Cubic(c1, c2, q) => {
                    let mut prev = cur;
                    for i in 1..=64 {
                        let t = i as f64 / 64.0;
                        let u = 1.0 - t;
                        let f = |a: f64, b: f64, c: f64, d: f64| u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d;
                        let p = [f(cur[0], c1[0], c2[0], q[0]), f(cur[1], c1[1], c2[1], q[1])];
                        line(prev, p, &mut out);
                        prev = p;
                    }
                    cur = q;
                }
                Seg::Close => {
                    line(cur, start, &mut out);
                    cur = start;
                }
            }
        }
    }
    out
}

const EXPECTED_DIFFERENT: &[&str] = &[];

#[derive(serde::Deserialize)]
struct Reference {
    preset: String,
    adj: std::collections::BTreeMap<String, i64>,
    w: f64,
    h: f64,
    pts: Vec<[i32; 2]>,
}

#[test]
fn every_preset_outline_lies_on_its_reference_points() {
    let refs: Vec<Reference> = serde_json::from_str(include_str!("reference-outlines.json")).expect("the reference table");
    let mut bad = Vec::new();
    for r in refs.iter().filter(|r| !r.pts.is_empty() && !EXPECTED_DIFFERENT.contains(&r.preset.as_str())) {
        let adj: Vec<(String, i64)> = r.adj.iter().map(|(k, v)| (k.clone(), *v)).collect();
        let g = preset(&r.preset, &adj).evaluate(emu(r.w), emu(r.h));
        let ours = flatten(&g);
        let worst = r
            .pts
            .iter()
            .map(|p| {
                let (x, y) = (f64::from(p[0]) / 10.0, f64::from(p[1]) / 10.0);
                ours.iter().map(|q| (q[0] - x).hypot(q[1] - y)).fold(f64::INFINITY, f64::min)
            })
            .fold(0.0, f64::max);
        if worst > 0.5 {
            bad.push(format!("{} {:?}: {worst:.2} pt", r.preset, r.adj));
        }
    }
    assert!(bad.is_empty(), "{} outlines off by more than 0.5 pt:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn text_rectangles_of_the_four_basic_presets() {
    let at = |name: &str, adj: &[(String, i64)], w: i64, h: i64| preset(name, adj).evaluate(Emu(w), Emu(h)).text_rect.map(Emu::from_pt);
    let r = at("roundRect", &[], 2_540_000, 1_016_000);
    assert!((r[0].0 - 49597).abs() <= 1 && (r[1].0 - 49597).abs() <= 1, "{r:?}");
    assert!((2_540_000 - r[2].0 - 49597).abs() <= 1 && (1_016_000 - r[3].0 - 49597).abs() <= 1, "{r:?}");
    let e = at("ellipse", &[], 1_000_000, 500_000);
    assert!((e[0].0 - 146_447).abs() <= 2 && (e[1].0 - 73_223).abs() <= 2, "{e:?}");
    let t = at("triangle", &[], 1_000_000, 600_000);
    assert_eq!(t.map(|v| v.0), [250_000, 300_000, 750_000, 600_000]);
    assert_eq!(at("rect", &[], 1_000, 2_000).map(|v| v.0), [0, 0, 1_000, 2_000]);
    // The pie's is its inner rectangle, as the ellipse's.
    let (p, e) = (at("pie", &[], 1_828_800, 1_371_600), at("ellipse", &[], 1_828_800, 1_371_600));
    assert!(p.iter().zip(&e).all(|(a, b)| (a.0 - b.0).abs() <= 4_000), "{p:?} {e:?}");
    assert!(!preset("chartX", &[]).holds_text() && !preset("lineInv", &[]).holds_text() && preset("pie", &[]).holds_text());
}

#[test]
fn every_preset_evaluates_to_finite_values_at_any_size_and_adjust_value() {
    for &p in Preset::ALL {
        let names = Geometry::Preset { preset: p, adj: Vec::new() }.adjust_names();
        for adj in [0i64, 1_000_000, -1_000_000] {
            let g = Geometry::Preset { preset: p, adj: names.iter().map(|n| (n.clone(), adj)).collect() };
            for (w, h) in [(0, 0), (0, 500_000), (500_000, 0), (1_000_000, 300_000)] {
                let r = g.evaluate(Emu(w), Emu(h));
                let finite = |v: &[f64; 2]| v[0].is_finite() && v[1].is_finite();
                assert!(r.text_rect.iter().all(|v| v.is_finite()), "{} text rectangle", p.name());
                assert!(r.handles.iter().all(|h| finite(&h.at)), "{} handles", p.name());
                for path in &r.paths {
                    for s in &path.segs {
                        let ok = match s {
                            Seg::Move(a) | Seg::Line(a) => finite(a),
                            Seg::Quad(a, b) => finite(a) && finite(b),
                            Seg::Cubic(a, b, c) => finite(a) && finite(b) && finite(c),
                            Seg::Close => true,
                        };
                        assert!(ok, "{} at {w} x {h}, adjust {adj}", p.name());
                    }
                }
            }
        }
    }
}
