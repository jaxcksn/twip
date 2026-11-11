//! Automatic hyphenation

use std::rc::Rc;

use crate::docx::{Block, Document};
use crate::layout::hyphenate::{Hyphenation, Patterns};

use super::{page_chars, plain_doc};

const LIANG: &str = "UTF-8\nLEFTHYPHENMIN 2\nRIGHTHYPHENMIN 3\nhy3ph\nhe2n\nhena4\nhen5at\n1na\nn2at\n1tio\n2io\no2n";

fn doc_at(text: &str, compat: i64, indent: f64, set: impl Fn(&mut Document)) -> Option<Document> {
    let mut d = plain_doc(&[text.to_string()])?;
    d.settings.compat_mode = compat;
    d.settings.auto_hyphenation = true;
    d.hyphenation = Some(Hyphenation::default().with("en-US", Patterns::parse(LIANG)));
    if let Some(Block::Para(p)) = d.sections[0].blocks.first_mut() {
        Rc::make_mut(&mut p.style).ind_right = indent;
        for i in &mut p.inlines {
            if let crate::docx::Inline::Text(_, r) = i {
                Rc::make_mut(r).lang = "en-US".into();
            }
        }
    }
    set(&mut d);
    Some(d)
}

fn line_ends(d: &Document) -> Vec<char> {
    let chars = &page_chars(d)[0];
    let mut ends: Vec<(f64, char)> = Vec::new();
    for &(c, _, y) in chars.iter().filter(|c| c.0 != ' ') {
        match ends.last_mut() {
            Some(e) if (e.0 - y).abs() < 0.01 => e.1 = c,
            _ => ends.push((y, c)),
        }
    }
    ends.into_iter().map(|e| e.1).collect()
}

fn hyphens(d: &Document) -> usize {
    line_ends(d).iter().filter(|&&c| c == '-').count()
}

fn swept(text: &str, compat: i64, set: impl Fn(&mut Document) + Copy) -> usize {
    swept_to(10, text, compat, set)
}

fn swept_to(n: usize, text: &str, compat: i64, set: impl Fn(&mut Document) + Copy) -> usize {
    (0..n).filter_map(|k| doc_at(text, compat, 150.0 + 20.0 * k as f64, set)).map(|d| hyphens(&d)).sum()
}

#[test]
fn words_break_at_their_pattern_points_when_the_settings_ask() {
    let text = "on hyphenation ".repeat(30);
    if plain_doc(&[String::new()]).is_none() {
        return;
    }
    assert!(swept(&text, 15, |_| {}) > 0);
    assert_eq!(swept(&text, 15, |d| d.settings.auto_hyphenation = false), 0);
    assert_eq!(swept(&text, 15, |d| d.hyphenation = None), 0);
    let suppress = |d: &mut Document| {
        if let Some(Block::Para(p)) = d.sections[0].blocks.first_mut() {
            Rc::make_mut(&mut p.style).suppress_auto_hyphens = true;
        }
    };
    assert_eq!(swept(&text, 15, suppress), 0);
    let caps = "HYPHENATION ".repeat(40);
    assert!(swept(&caps, 15, |_| {}) > 0);
    assert_eq!(swept(&caps, 15, |d| d.settings.do_not_hyphenate_caps = true), 0);
}

#[test]
fn compat_14_hyphenates_only_past_the_zone() {
    let text = "on hyphenation ".repeat(30);
    if plain_doc(&[String::new()]).is_none() {
        return;
    }
    assert_eq!(swept_to(7, &text, 14, |d| d.settings.hyphenation_zone = 1000.0), 0, "the whole word fits a line");
    assert!(swept(&text, 14, |d| d.settings.hyphenation_zone = 0.05) > 0);
}

#[test]
fn the_consecutive_limit_caps_hyphenated_line_ends() {
    let text = "on hyphenation ".repeat(30);
    let runs = |limit: i64| -> usize {
        (0..10)
            .filter_map(|k| doc_at(&text, 15, 150.0 + 20.0 * k as f64, |d| d.settings.consecutive_hyphen_limit = limit))
            .map(|d| line_ends(&d).windows(2).filter(|w| w[0] == '-' && w[1] == '-').count())
            .sum()
    };
    if plain_doc(&[String::new()]).is_none() || runs(0) == 0 {
        return;
    }
    assert_eq!(runs(1), 0);
}

#[test]
fn a_line_inside_a_word_hyphenates_the_rest_of_it_as_a_word() {
    // Only the whole word has points. Compat 14 hyphenates the letters from the line start, so a word breaks at a
    // hyphen at most once; compat 15 keeps the whole word's points after a hyphen.
    let text = "particularly";
    if plain_doc(&[String::new()]).is_none() {
        return;
    }
    let most = |compat| {
        (0..20)
            .filter_map(|k| {
                doc_at(text, compat, 400.0 + 3.0 * k as f64, |d| {
                    d.hyphenation = Some(Hyphenation::default().with("en-US", Patterns::parse(".par1tic1u1lar1ly.")));
                })
            })
            .map(|d| hyphens(&d))
            .max()
    };
    assert_eq!(most(14), Some(1));
    assert!(most(15) >= Some(2));
}

/// `fill` filler lines, then `t`, in Courier New 10 pt, 30 characters a line and 51 lines a page; the lines of the
/// first two pages.
fn courier_lines(settings: &str, fill: usize, t: &str) -> Option<[Vec<String>; 2]> {
    let ppr = r#"<w:pPr><w:widowControl w:val="0"/><w:spacing w:after="0" w:line="250" w:lineRule="exact"/><w:ind w:right="5699"/></w:pPr>"#;
    let fill: String = (0..fill).map(|k| format!("<w:p>{ppr}<w:r><w:t>Fill {k}</w:t></w:r></w:p>")).collect();
    let rpr = r#"<w:rFonts w:ascii="Courier New" w:hAnsi="Courier New"/><w:sz w:val="20"/><w:lang w:val="en-US"/>"#;
    let mut d = super::breaking::test_doc(settings, rpr, &format!("{fill}<w:p>{ppr}{t}</w:p>"), "")?;
    d.hyphenation = Some(Hyphenation::default().with("en-US", Patterns::parse("UTF-8\nLEFTHYPHENMIN 2\nRIGHTHYPHENMIN 3\na1t")));
    let pages = page_chars(&d);
    let lines = |k: usize| {
        let mut out: Vec<(f64, String)> = Vec::new();
        for &(c, _, y) in pages.get(k).into_iter().flatten() {
            match out.last_mut() {
                Some(l) if (l.0 - y).abs() < 0.01 => l.1.push(c),
                _ => out.push((y, c.to_string())),
            }
        }
        out.into_iter().map(|l| l.1.trim_end().to_string()).collect::<Vec<_>>()
    };
    Some([lines(0), lines(1)])
}

const TRACK_T: &str = r#"<w:r><w:t>The plain cart ran representative bottom text.</w:t></w:r>"#;

fn track_settings(compat: i64, auto: bool, extra: &str) -> String {
    let auto = if auto { "<w:autoHyphenation/>" } else { "" };
    let uri = "http://schemas.microsoft.com/office/word";
    format!(r#"{auto}<w:compat>{extra}<w:compatSetting w:name="compatibilityMode" w:uri="{uri}" w:val="{compat}"/></w:compat>"#)
}

fn track_setting(name: &str, on: bool) -> String {
    format!(r#"<w:compatSetting w:name="{name}" w:uri="http://schemas.microsoft.com/office/word" w:val="{}"/>"#, on as u8)
}

#[test]
fn compat_15_moves_a_hyphenated_last_line_to_the_next_page() {
    if !super::fonts().is_installed("Courier New") {
        return;
    }
    let Some([p1, p2]) = courier_lines(&track_settings(15, true, ""), 50, TRACK_T) else { return };
    assert_eq!((p1.len(), p1.last().map(String::as_str)), (50, Some("Fill 49")), "{p1:?}");
    assert_eq!(p2[0], "The plain cart ran representa-", "{p2:?}");
    // An optional hyphen moves it too, without automatic hyphenation.
    let soft = r#"<w:r><w:t xml:space="preserve">The plain cart ran representa</w:t><w:softHyphen/><w:t xml:space="preserve">tive bottom text.</w:t></w:r>"#;
    let [p1, _] = courier_lines(&track_settings(15, false, ""), 50, soft).expect("lays out");
    assert_eq!(p1.len(), 50, "{p1:?}");
    // `allowHyphenationAtTrackBottom`, and compat 12, hyphenate the last line like any other.
    let [p1, _] = courier_lines(&track_settings(15, true, &track_setting("allowHyphenationAtTrackBottom", true)), 50, TRACK_T).expect("lays out");
    assert_eq!(p1.last().map(String::as_str), Some("The plain cart ran representa-"), "{p1:?}");
    let [p1, _] = courier_lines(&track_settings(12, true, ""), 50, TRACK_T).expect("lays out");
    assert_eq!(p1.last().map(String::as_str), Some("The plain cart ran representa-"), "{p1:?}");
}

#[test]
fn compat_15_breaks_the_last_line_again_without_automatic_hyphenation() {
    if !super::fonts().is_installed("Courier New") {
        return;
    }
    let settings = track_settings(15, true, &track_setting("useWord2013TrackBottomHyphenation", false));
    let Some([p1, p2]) = courier_lines(&settings, 50, TRACK_T) else { return };
    assert_eq!((p1.len(), p1.last().map(String::as_str)), (51, Some("The plain cart ran")), "{p1:?}");
    assert_eq!(p2[0], "representative bottom text.", "{p2:?}");
}
