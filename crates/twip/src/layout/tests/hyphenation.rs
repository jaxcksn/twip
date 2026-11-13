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
fn a_run_without_a_language_hyphenates_as_en_us() {
    let text = "on hyphenation ".repeat(30);
    if plain_doc(&[String::new()]).is_none() {
        return;
    }
    let unset = |d: &mut Document| {
        if let Some(Block::Para(p)) = d.sections[0].blocks.first_mut() {
            for i in &mut p.inlines {
                if let crate::docx::Inline::Text(_, r) = i {
                    Rc::make_mut(r).lang = String::new();
                }
            }
        }
    };
    let en = swept(&text, 15, |_| {});
    assert!(en > 0 && swept(&text, 15, unset) == en);
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
    courier(settings, r#"<w:ind w:right="5699"/>"#, 20, "a1t", fill, t)
}

/// `fill` filler lines, then `t`, in Courier New at `sz` half-points with the paragraph properties `ppr` and the
/// patterns `pat`; the lines of the first two pages.
fn courier(settings: &str, ppr: &str, sz: u32, pat: &str, fill: usize, t: &str) -> Option<[Vec<String>; 2]> {
    let ppr = format!(r#"<w:pPr><w:widowControl w:val="0"/><w:spacing w:after="0" w:line="250" w:lineRule="exact"/>{ppr}</w:pPr>"#);
    let fill: String = (0..fill).map(|k| format!("<w:p>{ppr}<w:r><w:t>Fill {k}</w:t></w:r></w:p>")).collect();
    let rpr = format!(r#"<w:rFonts w:ascii="Courier New" w:hAnsi="Courier New"/><w:sz w:val="{sz}"/><w:lang w:val="en-US"/>"#);
    let mut d = super::breaking::test_doc(settings, &rpr, &format!("{fill}<w:p>{ppr}{t}</w:p>"), "")?;
    let pat = format!("UTF-8\nLEFTHYPHENMIN 2\nRIGHTHYPHENMIN 3\n{pat}");
    d.hyphenation = Some(Hyphenation::default().with("en-US", Patterns::parse(&pat)));
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
fn compat_15_checks_the_hyphen_on_the_line_widow_control_leaves_last() {
    if !super::fonts().is_installed("Courier New") {
        return;
    }
    // Four lines, the first three on the page's last three: widow control keeps two, the second ends in a hyphen and
    // moves, and the first, left alone, goes with it.
    let t = r#"<w:r><w:t>Bob and Sam met by the old inn The plain cart ran representative bottom text. Then the old fox ran on and on.</w:t></w:r>"#;
    let ppr = r#"<w:ind w:right="5699"/><w:widowControl/>"#;
    let Some([p1, p2]) = courier(&track_settings(15, true, ""), ppr, 20, "a1t", 48, t) else { return };
    assert_eq!((p1.len(), p1.last().map(String::as_str)), (48, Some("Fill 47")), "{p1:?}");
    assert_eq!(p2[..2], ["Bob and Sam met by the old inn", "The plain cart ran representa-"], "{p2:?}");
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

#[test]
fn a_word_without_proofing_isnt_hyphenated() {
    if !super::fonts().is_installed("Courier New") {
        return;
    }
    // The break would follow letter 10; letter 12 is the first that doesn't fit.
    let first = |compat: i64, a: &str, b: &str| {
        let t = format!(r#"<w:r><w:t xml:space="preserve">The plain cart ran </w:t></w:r><w:r>{a}<w:t>repre</w:t></w:r><w:r>{b}<w:t xml:space="preserve">sentative words go here.</w:t></w:r>"#);
        courier_lines(&track_settings(compat, true, ""), 0, &t).map(|[p1, _]| p1[0].clone())
    };
    let Some(plain) = first(15, "", "") else { return };
    assert_eq!(plain, "The plain cart ran representa-");
    let np = "<w:rPr><w:noProof/></w:rPr>";
    // Compat 15 asks the word's first letter, compat 12 the letter that doesn't fit.
    assert_eq!(first(15, np, "").as_deref(), Some("The plain cart ran"));
    assert_eq!(first(15, "", np).as_deref(), Some("The plain cart ran representa-"));
    assert_eq!(first(12, np, "").as_deref(), Some("The plain cart ran representa-"));
    assert_eq!(first(12, "", np).as_deref(), Some("The plain cart ran"));
}

/// The first page's lines of `t` in Courier New 10 pt, justified (`jc`) in compat 15 with the right indent `ind`.
fn justified_lines(jc: &str, auto: bool, ind: u32, t: &str) -> Option<Vec<String>> {
    if !super::fonts().is_installed("Courier New") {
        return None;
    }
    let ppr = format!(r#"<w:jc w:val="{jc}"/><w:ind w:right="{ind}"/>"#);
    let run = format!(r#"<w:r><w:t xml:space="preserve">{t}</w:t></w:r>"#);
    courier(&track_settings(15, auto, ""), &ppr, 20, ".rep1re1sen1ta1tive.", 0, &run).map(|[p1, _]| p1)
}

#[test]
fn compat_15_justified_text_ends_lines_at_hyphenation_points() {
    // `representa-` fits the 3661 tw line unshrunk; the word end before it would stretch past its tolerance.
    let Some(lines) = justified_lines("both", true, 5699, "The plain cart ran representative bottom text.") else { return };
    assert_eq!(lines[0], "The plain cart ran representa-", "{lines:?}");
    // Thirteen `ab`, a double space and `representative`: a hyphenation point pays for its flag, so the line takes
    // `rep-` only once the word end would stretch about as far as the point's demerits.
    let t = format!("{}ab  representative and more words to follow.", "ab ".repeat(12));
    let first = |w: u32| justified_lines("both", true, 9360 - w, &t).map(|l| l[0].clone());
    assert_eq!(first(5101).as_deref(), Some(&*format!("{}ab", "ab ".repeat(12))));
    assert_eq!(first(5281).as_deref(), Some(&*format!("{}ab  rep-", "ab ".repeat(12))));
}

#[test]
fn compat_15_distributed_text_stretches_every_glyph_but_the_last() {
    // Sixty letters, then `b`: the long word alone stretches less than the two words shrink only with the glyphs'
    // share of the stretch.
    let t = format!("{} b more", "x".repeat(60));
    let first = |jc: &str| justified_lines(jc, false, 9360 - 7412, &t).map(|l| l[0].clone());
    let Some(both) = first("both") else { return };
    assert_eq!(both, format!("{} b", "x".repeat(60)));
    assert_eq!(first("distribute"), Some("x".repeat(60)));
}

#[test]
fn compat_15_a_line_at_the_limit_ending_at_an_optional_hyphen_isnt_counted() {
    // Limit 1: line 1 ends at a point, line 2 (at the limit) at an optional hyphen; line 3 may take a point again.
    let t = format!(
        r#"<w:r><w:t xml:space="preserve">{} zzattttt {}</w:t><w:softHyphen/><w:t xml:space="preserve">qqqqqq {} zzattttt end</w:t></w:r>"#,
        "y".repeat(25),
        "q".repeat(20),
        "y".repeat(18)
    );
    if !super::fonts().is_installed("Courier New") {
        return;
    }
    let settings = track_settings(15, true, "").replace("<w:compat>", r#"<w:consecutiveHyphenLimit w:val="1"/><w:compat>"#);
    let Some([p1, _]) = courier_lines(&settings, 0, &t) else { return };
    assert_eq!(&p1[..3], [format!("{} zza-", "y".repeat(25)), format!("ttttt {}-", "q".repeat(20)), format!("qqqqqq {} zza-", "y".repeat(18))], "{p1:?}");
}

#[test]
fn a_line_fits_text_up_to_its_width_rounded_to_layout_units() {
    // 25 Courier New glyphs at 20.5pt are 1259725 lu; the 6151 tw line is 1259724.8 lu, rounded 1259725.
    if !super::fonts().is_installed("Courier New") {
        return;
    }
    let t = format!("<w:r><w:t>{}</w:t></w:r>", "x".repeat(40));
    let Some([p1, _]) = courier(&track_settings(15, false, ""), r#"<w:ind w:right="3209"/>"#, 41, "a1t", 0, &t) else { return };
    assert_eq!(p1[0].len(), 25, "{p1:?}");
}

#[test]
fn compat_15_a_word_broken_between_letters_at_a_page_end_goes_on_as_a_fragment() {
    // `pres` ends page 1 between letters, at a point whose hyphen doesn't fit: page 2 hyphenates `ident` alone,
    // which has no points.
    if !super::fonts().is_installed("Courier New") {
        return;
    }
    let t = format!("<w:r><w:t>{}president</w:t></w:r>", "abcd ".repeat(50));
    let settings = track_settings(15, true, &track_setting("allowHyphenationAtTrackBottom", true));
    let Some([p1, p2]) = courier(&settings, r#"<w:ind w:right="8820"/>"#, 20, ".pres1i1dent.", 0, &t) else { return };
    assert_eq!(p1.last().map(String::as_str), Some("pres"), "{p1:?}");
    assert_eq!(p2[..2], ["iden", "t"], "{p2:?}");
}
