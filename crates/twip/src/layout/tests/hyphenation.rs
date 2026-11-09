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
