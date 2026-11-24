//! Line breaking and justification

use crate::docx::Compat;
use crate::docx::model::*;
use crate::docx::props::{Jc, Leader, TabKind};
use crate::font::Fonts;
use crate::layout::text::{Kind, Object, Piece, ShapedPara};
use crate::layout::units::{EPS, LU_PER_PT, Lu};

use super::{Frame, Line, size::size};

struct Fit {
    end: usize,
    xs: Vec<f64>,
    widths: Vec<f64>,
    hard: Option<Kind>,
    hyphen: bool,
    leaders: Vec<(usize, Leader)>,
}

/// The style a line of a joined paragraph takes its indents, alignment and tabs from: while hidden text shows, that of
/// its part ([`ShapedPara::parts`]); else ([`ParaStyle::joined`]) that of the paragraph its first character is in, where
/// a line starting at a paragraph's first character starts at the vanished mark before it, so it is the earlier
/// paragraph's.
pub(super) fn line_style<'a>(sp: &'a ShapedPara, style: &'a ParaStyle, pieces: &[Piece], line: &Line) -> &'a ParaStyle {
    if let Some(part) = sp.parts.as_ref().and_then(|p| p.get(line.part)) {
        return &part.style;
    }
    start_style(style, pieces, line.start)
}

fn start_style<'a>(style: &'a ParaStyle, pieces: &[Piece], start: usize) -> &'a ParaStyle {
    if style.joined.is_empty() {
        return style;
    }
    let u = pieces[start.min(pieces.len())..].iter().find_map(|p| p.src).map_or(0, |u| u.0);
    style.joined.iter().rev().find(|(at, _)| *at < u).map_or(style, |(_, s)| s)
}

/// Where `pieces`, a run of the shaped paragraph's pieces, start in them.
fn origin(sp: &ShapedPara, pieces: &[Piece]) -> usize {
    let bytes = (pieces.as_ptr() as usize).wrapping_sub(sp.pieces.as_ptr() as usize);
    let k = bytes / std::mem::size_of::<Piece>();
    if bytes.is_multiple_of(std::mem::size_of::<Piece>()) && k.checked_add(pieces.len()).is_some_and(|e| e <= sp.pieces.len()) {
        k
    } else {
        sp.pieces.len().saturating_sub(pieces.len())
    }
}

/// While hidden text shows, where each part of a joined paragraph after the first starts in `sp.pieces`.
fn part_starts(sp: &ShapedPara) -> Vec<usize> {
    let Some(parts) = &sp.parts else { return Vec::new() };
    let mut k = 0;
    parts[1..]
        .iter()
        .map(|p| {
            k += sp.pieces[k..].iter().position(|q| q.src.is_some_and(|u| u.0 >= p.start)).unwrap_or(sp.pieces.len() - k);
            k
        })
        .collect()
}

/// Where each part after the first starts in `pieces`, and the part a line from `pieces[start]` is in: `wanted` when it
/// is one that can be (one that starts there, or goes on there or ends there with its last line still to come).
fn parts_from(sp: &ShapedPara, pieces: &[Piece], start: usize, wanted: usize) -> (Vec<usize>, usize) {
    let mut bounds = part_starts(sp);
    let (o, at) = (origin(sp, pieces), origin(sp, pieces) + start);
    let part = wanted.clamp(bounds.partition_point(|&b| b < at), bounds.partition_point(|&b| b <= at));
    for b in &mut bounds {
        *b = b.saturating_sub(o);
    }
    (bounds, part)
}

/// Whether the line at `pieces[0]` in shown part `part` is the part's first, after one before it.
pub fn opens_part(sp: &ShapedPara, pieces: &[Piece], part: usize) -> bool {
    part > 0 && part_starts(sp).get(part - 1) == Some(&origin(sp, pieces))
}

pub fn break_paragraph(sp: &ShapedPara, pieces: &[Piece], style: &ParaStyle, mark: &Run, frame: &Frame, fonts: &Fonts, continued: bool) -> Vec<Line> {
    let mut lines = Vec::new();
    let line_size = style;
    let justify_smart = matches!(style.jc, Jc::Both | Jc::Distribute) && frame.compat == Compat::Generic && !frame.no_wrap;
    let mut start = 0;
    let n = pieces.len();
    let with_tabs = justify_smart && pieces.iter().any(|p| p.kind == Kind::Tab);
    let mut hyphens = 0;
    let mut word_from = frame.word_from;
    let all = *frame;
    let ((bounds, mut part), all_pieces) = (parts_from(sp, pieces, 0, frame.part), pieces);
    let mut first = if part == 0 { !continued } else { opens_part(sp, pieces, part) };
    loop {
        let at_limit = all.hyphen_limit > 0 && hyphens >= all.hyphen_limit;
        let frame = &Frame { hyphenate: all.hyphenate && !at_limit, word_from, ..all };
        // Shown joined parts
        let shown = sp.parts.as_ref().and_then(|p| p.get(part));
        let hi = bounds.get(part).copied().unwrap_or(n);
        let pieces = &all_pieces[..hi.max(start).min(n)];
        let (style, line_size) = match shown {
            Some(part) => (&*part.style, &*part.style),
            None => (start_style(style, pieces, start), line_size),
        };
        if shown.is_some() && start >= hi {
            // Empty part
            let mut line = Line { part, ..Line::empty(start) };
            size(&mut line, sp, all_pieces, line_size, mark, fonts, true, frame);
            part_end(&mut line, sp, part);
            lines.push(line);
            if part >= bounds.len() {
                break;
            }
            (part, first) = (part + 1, true);
            continue;
        }
        let right = frame.width - style.ind_right;
        let wrap_right = if frame.no_wrap { f64::INFINITY } else { right + wp_extension(style, frame) };
        let left = style.ind_left + if first { style.ind_first } else { 0.0 };
        let planned = match justify_smart {
            false => None,
            true if with_tabs => justified_tab_end(sp, pieces, start, left, right, style, frame),
            true => justified_end(sp, pieces, start, right - left, style, frame),
        };
        let mut fit = fit_line(sp, pieces, start, left, wrap_right, style, frame, planned.map(|e| e.0), false);
        fit.hyphen |= planned.is_some_and(|e| e.1);
        let end = fit.end.max(start + 1).min(hi.max(start + 1));
        let is_last = end >= hi;
        // Compat 15: a line at the limit that ends at an optional hyphen starts the count again.
        let counts = fit.hyphen && !(at_limit && frame.compat == Compat::Generic);
        hyphens = if counts { hyphens + 1 } else { 0 };
        if !fit.hyphen {
            word_from = Some(end);
        }
        let mut line = Line {
            end: end.min(n),
            xs: fit.xs,
            widths: fit.widths,
            hard: fit.hard,
            hyphen: fit.hyphen,
            leaders: fit.leaders,
            part,
            ..Line::empty(start)
        };
        if !frame.no_wrap {
            align(&mut line, sp, pieces, style, frame, left, right, is_last);
        } else {
            fit_text(&mut line, pieces, right - 1.0);
        }
        size(&mut line, sp, pieces, line_size, mark, fonts, end >= n, frame);
        first = false;
        start = end;
        let trailing = is_last && closes(&line, frame);
        lines.push(line);
        if trailing {
            let mut line = Line { part, ..Line::empty(end.min(n)) };
            size(&mut line, sp, all_pieces, line_size, mark, fonts, true, frame);
            lines.push(line);
        }
        if !is_last {
            continue;
        }
        if shown.is_none() || part >= bounds.len() {
            break;
        }
        if let Some(last) = lines.last_mut() {
            part_end(last, sp, part);
        }
        (part, first) = (part + 1, true);
    }
    if lines.is_empty() || n == 0 && sp.parts.is_none() {
        lines.clear();
        let mut line = Line::empty(0);
        size(&mut line, sp, pieces, style, mark, fonts, true, frame);
        lines.push(line);
    }
    lines
}

/// A closing line, column or page break: an empty line after it.
fn closes(line: &Line, frame: &Frame) -> bool {
    matches!(line.hard, Some(Kind::ColumnBreak | Kind::LineBreak)) || line.hard == Some(Kind::PageBreak) && frame.split_page_break
}

/// The space after a joined paragraph's part, below its last line: the gap to the next part.
fn part_end(line: &mut Line, sp: &ShapedPara, part: usize) {
    let gap = sp.parts.as_ref().and_then(|p| p.get(part + 1)).map_or(0.0, |p| p.gap);
    line.pitch += gap;
    line.after = gap;
}

fn wp_extension(style: &ParaStyle, frame: &Frame) -> f64 {
    if !frame.wp_justification || style.jc != Jc::Both {
        return 0.0;
    }
    let w = ((frame.width - style.ind_left - style.ind_right) * 20.0).round().min(31680.0);
    (w * 562.0 / 14400.0).round() / 20.0
}

fn fit_text(line: &mut Line, pieces: &[Piece], fit: f64) {
    let n = line.xs.len();
    let content = (0..n)
        .rev()
        .find(|&k| !matches!(pieces[line.start + k].kind, Kind::Space | Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak | Kind::Anchor));
    let Some(last) = content else { return };
    if last == 0 {
        return;
    }
    let delta = fit - (line.xs[last] + line.widths[last]);
    for k in 1..n {
        line.xs[k] += delta * k.min(last) as f64 / last as f64;
    }
}

// Wrap bands
#[allow(
    clippy::too_many_arguments,
    reason = "the measured line-breaking helpers take the shaped paragraph, its pieces, style and frame separately, as they were ported"
)]
/// The floats a line's closing text-wrapping break clears: left, right.
pub fn clear_of(sp: &ShapedPara, pieces: &[Piece], line: &Line) -> Option<(bool, bool)> {
    let last = pieces.get(line.end.checked_sub(1)?).filter(|p| p.kind == Kind::LineBreak && line.hard == Some(Kind::LineBreak))?;
    match &sp.objects[last.object? as usize] {
        Object::Clear(l, r) => Some((*l, *r)),
        _ => None,
    }
}

pub fn band(
    sp: &ShapedPara,
    pieces: &[Piece],
    start: usize,
    intervals: &[(f64, f64)],
    first_line: bool,
    style: &ParaStyle,
    mark: &Run,
    frame: &Frame,
    fonts: &Fonts,
    force: bool,
) -> Result<(Vec<Line>, usize, Option<usize>), f64> {
    let (bounds, part) = parts_from(sp, pieces, start, frame.part);
    let total = pieces.len();
    let hi = bounds.get(part).copied().unwrap_or(total);
    let shown = sp.parts.as_ref().and_then(|p| p.get(part)).map(|q| &*q.style);
    let (line_size, style) = match shown {
        Some(s) => (s, s),
        None => (style, start_style(style, pieces, start)),
    };
    let more = part < bounds.len();
    if shown.is_some() && start >= hi {
        // Empty part, or the empty line after a part's closing break
        let mut line = Line { part, ..Line::empty(start) };
        size(&mut line, sp, pieces, line_size, mark, fonts, true, frame);
        part_end(&mut line, sp, part);
        return Ok((vec![line], start, more.then_some(part + 1)));
    }
    let first_line = first_line || shown.is_some() && part > 0 && bounds.get(part - 1) == Some(&start);
    let pieces = &pieces[..hi.max(start).min(total)];
    let n = pieces.len();
    let right_edge = frame.width - style.ind_right;
    let mut segs: Vec<Line> = Vec::new();
    let mut s = start;
    let mut gap = None;
    let mut word_from = frame.word_from;
    for &(a, b) in intervals {
        if s >= n && !segs.is_empty() {
            break;
        }
        let first = if first_line && segs.is_empty() { style.ind_first } else { 0.0 };
        let l = style.ind_left;
        // A piece cut on the left by a float: compat 15 keeps the indents measured from the column, within the piece.
        // Compat ≤ 14 measures them from the piece: the left indent loses the cut, a hanging first line keeps its
        // own indent from the piece's start; with the cut sides' indents dropped only a first-line indent stays.
        // A side no float cuts keeps its indents, a negative one past the column edge too.
        let left = match (a > EPS, frame.compat) {
            (false, _) => l + first,
            (true, Compat::Generic) => (a.max(l) + first).max(a),
            (true, Compat::Legacy) if frame.drop_cut_indents => a + first.max(0.0),
            (true, Compat::Legacy) => {
                let l2 = (l - a).max(0.0);
                a + l2 + if first < 0.0 && l != l2 { first + l - l2 } else { first }
            }
        };
        let mut right = match b >= frame.width - EPS {
            true => right_edge,
            false if frame.drop_cut_indents => b,
            false => b.min(right_edge),
        };
        // Compat ≤ 14, a line of several pieces: Word drops a piece only when its line is force-broken, and a line
        // that ends at the paragraph mark isn't, so the mark alone, spaces or one character or object before it take
        // the piece however narrow, from its start. After a piece of tabs alone the whole line is retried (below).
        let tabs_only = s > start && pieces[start..s].iter().all(|q| matches!(q.kind, Kind::Tab | Kind::Anchor));
        if frame.compat == Compat::Legacy
            && intervals.len() > 1
            && !tabs_only
            && let Some(w) = mark_rest(sp, pieces, s)
        {
            right = right.max(left + w + 2.0 * EPS);
        }
        if right - left <= EPS {
            continue;
        }
        gap.get_or_insert(right - left);
        let piece_frame = Frame { tab_origin: if frame.compat == Compat::Legacy { a } else { 0.0 }, word_from, ..*frame };
        let frame = &piece_frame;
        let planned = if matches!(style.jc, Jc::Both | Jc::Distribute) && frame.compat == Compat::Generic && s < n {
            let seg_end = segment_end(pieces, s);
            if !pieces[s..seg_end].iter().any(|p| p.kind == Kind::Tab) {
                match justified_line(sp, pieces, s, seg_end, right - left, style, frame) {
                    LineChoice::End(e, hyphen) => Some((e, hyphen)),
                    _ => None,
                }
            } else if a <= EPS && b >= frame.width - EPS {
                justified_tab_end(sp, pieces, s, left, right, style, frame)
            } else {
                None
            }
        } else {
            None
        };
        let mut fit = fit_line(sp, pieces, s, left, right, style, frame, planned.map(|e| e.0), !force && planned.is_none());
        fit.hyphen |= planned.is_some_and(|e| e.1);
        if fit.end <= s && s < n {
            continue;
        }
        let mut line =
            Line { end: fit.end.min(n), xs: fit.xs, widths: fit.widths, hard: fit.hard, hyphen: fit.hyphen, leaders: fit.leaders, ..Line::empty(s) };
        align(&mut line, sp, pieces, style, frame, left, right, fit.end >= n);
        s = fit.end;
        if !line.hyphen {
            word_from = Some(s);
        }
        let hard = line.hard.is_some_and(|k| k != Kind::LineBreak);
        let cleared = match clear_of(sp, pieces, &line) {
            Some((l, r)) => {
                let (a, b) = if frame.compat == Compat::Legacy { intervals[0] } else { (a, b) };
                (!l || a <= EPS) && (!r || b >= frame.width - EPS)
            }
            None => true,
        };
        segs.push(line);
        if hard || !cleared || s >= n {
            break;
        }
    }
    let own = |k: usize| pieces[k].ascent + pieces[k].descent;
    if segs.is_empty() {
        let mut x = 0.0;
        let k = (start..n).filter(|&k| pieces[k].kind != Kind::Anchor).find(|&k| {
            x += pieces[k].width;
            x > gap.unwrap_or(0.0) + EPS
        });
        return Err(k.or((start..n).find(|&k| pieces[k].kind != Kind::Anchor)).map_or(0.0, own));
    }
    if !force
        && frame.compat == Compat::Legacy
        && s < n
        && matches!(pieces[s].kind, Kind::Text | Kind::Object)
        && segs.last().is_some_and(|l| l.hard.is_none())
        && pieces[start..s].iter().all(|q| matches!(q.kind, Kind::Tab | Kind::Anchor))
    {
        let placed = start..s;
        let a = placed.clone().map(|k| pieces[k].ascent).fold(0.0, f64::max);
        return Err(a + placed.map(|k| pieces[k].descent).fold(0.0, f64::max));
    }
    let mut merged = segs[0].clone();
    merged.end = s;
    size(&mut merged, sp, pieces, line_size, mark, fonts, s >= total, frame);
    let trailing = shown.is_some() && s >= n && segs.last().is_some_and(|l| closes(l, frame));
    let next = match (s >= n, shown) {
        (false, _) => Some(part),
        (true, Some(_)) if trailing => Some(part),
        (true, Some(_)) if more => {
            part_end(&mut merged, sp, part);
            Some(part + 1)
        }
        (true, _) => None,
    };
    for l in &mut segs {
        l.ascent = merged.ascent;
        l.descent = merged.descent;
        l.pitch = merged.pitch;
        l.baseline = merged.baseline;
        l.below = merged.below;
        (l.align, l.vertical, l.part, l.after) = (merged.align, merged.vertical, part, merged.after);
    }
    Ok((segs, s, next))
}

// Tabs
fn ptab_stop(t: &crate::docx::PTab, style: &ParaStyle, frame: &Frame) -> (f64, TabKind, Leader) {
    let (l, r) = if t.margin { (0.0, frame.width) } else { (style.ind_left, frame.width - style.ind_right) };
    let pos = match t.align {
        TabKind::Center => (l + r) / 2.0,
        TabKind::Right => r,
        _ => l,
    };
    (pos, t.align, t.leader)
}

/// The pen as compat 15 compares it with tab stops: in layout units, the line's start from its indents each rounded
/// on their own, the text after it as laid out. None in compat ≤ 14, which compares in points.
fn pen_units(pen: f64, left: f64, style: &ParaStyle, frame: &Frame) -> Option<f64> {
    let lu = |x: f64| Lu::round_half_up(x).0 as f64;
    let start = if (left - style.ind_left - style.ind_first).abs() < EPS {
        lu(style.ind_left) + lu(style.ind_first)
    } else if (left - style.ind_left).abs() < EPS {
        lu(style.ind_left)
    } else {
        lu(left)
    };
    (frame.compat == Compat::Generic && frame.tab_origin == 0.0).then(|| start + ((pen - left) * LU_PER_PT).round())
}

/// A stop lies past the pen.
fn past_pen(stop: f64, pen: f64, units: Option<f64>) -> bool {
    match units {
        Some(u) => f64::from(Lu::round_half_up(stop).0) > u,
        None => stop > pen + EPS,
    }
}

/// The stop a tab at `pen` goes to; `number`: the tab after a list number.
fn next_stop(pen: f64, units: Option<f64>, style: &ParaStyle, frame: &Frame, number: bool) -> (f64, TabKind, Leader) {
    let o = frame.tab_origin;
    let (stop, kind, leader) = stop_from(pen - o, units, style, frame, if number { frame.number_hang_stop } else { frame.hang_stop });
    (stop + o, kind, leader)
}

/// The custom stops a tab may go to: past the pen, not bar or clear.
fn custom_stops(pen: f64, units: Option<f64>, style: &ParaStyle) -> impl Iterator<Item = &crate::docx::props::Tab> {
    style.tabs.iter().filter(move |t| !matches!(t.kind, TabKind::Clear | TabKind::Bar) && past_pen(t.pos, pen, units))
}

fn stop_from(pen: f64, units: Option<f64>, style: &ParaStyle, frame: &Frame, hang_stop: bool) -> (f64, TabKind, Leader) {
    let custom = custom_stops(pen, units, style).next();
    let (mut stop, mut kind, mut leader) = match custom {
        Some(t) => (t.pos, t.kind, t.leader),
        None => {
            let inc = frame.default_tab.max(1.0);
            let q = pen / inc;
            let on_negative = pen < -EPS && (q - q.round()).abs() * inc < EPS;
            let s = if frame.compat == Compat::Generic && on_negative { q.round() * inc } else { (q.floor() + 1.0) * inc };
            (s, TabKind::Left, Leader::None)
        }
    };
    if hang_stop && past_pen(style.ind_left, pen, units) && (stop > style.ind_left + EPS || custom.is_none()) {
        stop = style.ind_left;
        kind = TabKind::Left;
        leader = Leader::None;
    }
    if kind == TabKind::Num {
        kind = TabKind::Left;
    }
    (stop, kind, leader)
}

struct Pending {
    /// The tab, none for the one a decimal-only cell paragraph's lines start with, and its text's first piece.
    tab: Option<usize>,
    from: usize,
    stop: f64,
    kind: TabKind,
    pen_at_tab: f64,
    /// The fit drift before the tab.
    drift: f64,
    /// The tab has no width (its text wraps).
    collapse: bool,
    /// Where its text narrows the tab to end ([`f64::INFINITY`]: it doesn't), and whether decimal text narrows too.
    edge: f64,
    shift: bool,
}

/// An East Asian language's kinsoku classes: what doesn't end a line, what doesn't start one (standard and strict
/// level), and what may hang past the right margin.
struct Kinsoku {
    no_end: &'static str,
    no_start: &'static str,
    no_start_strict: &'static str,
    hanging: &'static str,
}

const JAPANESE: Kinsoku = Kinsoku {
    no_end: "$([\\{£¥‘“〈《「『【〔＄（［｛｢￡￥",
    no_start: "!%),.:;?]}¢°’”‰′″℃、。々〉》」』】〕゛゜ゝゞ・ヽヾ！％），．：；？］｝｡｣､･ﾞﾟ￠",
    no_start_strict: "!%),.:;?]}¢°’”‰′″℃、。々〉》」』】〕ぁぃぅぇぉっゃゅょゎ゛゜ゝゞァィゥェォッャュョヮヵヶ・ーヽヾ！％），．：；？］｝｡｣､･ｧｨｩｪｫｬｭｮｯｰﾞﾟ￠",
    hanging: ",.’”、。」』】），．］｝｡､",
};

const SIMPLIFIED_CHINESE: Kinsoku = Kinsoku {
    no_end: "$([{£¥·‘“〈《「『【〔〖〝﹙﹛﹝＄（．［｛￡￥",
    no_start: "!%),.:;>?]}¢¨°·ˇˉ―‖’”…‰′″›℃∶、。〃〉》」』】〕〗〞︶︺︾﹀﹄﹚﹜﹞！＂％＇），．：；？］｀｜｝～￠",
    no_start_strict: "!%),.:;>?]}¢¨°·ˇˉ―‖’”…‰′″›℃∶、。〃〉》」』】〕〗〞︶︺︾﹀﹄﹚﹜﹞！＂％＇），．：；？］｀｜｝～￠",
    hanging: "!%),.:;>?]}¢°·ˇ’”‰′″℃∶、。〃〉》」』】〕〗〞﹚﹜﹞！＂％＇），．：；？］｝￠",
};

const TRADITIONAL_CHINESE: Kinsoku = Kinsoku {
    no_end: "([{£¥‘“‵〈《「『【〔〝︵︷︹︻︽︿﹁﹃﹙﹛﹝（｛",
    no_start: "!),.:;?]}¢·–—’”•‥…‧′╴、。〉》」』】〕〞︰︱︳︴︶︸︺︼︾﹀﹂﹄﹏﹐﹑﹒﹔﹕﹖﹗﹚﹜﹞！），．：；？］｜｝､",
    no_start_strict: "!),.:;?]}¢·–—’”•‥…‧′╴、。〉》」』】〕〞︰︱︳︴︶︸︺︼︾﹀﹂﹄﹏﹐﹑﹒﹔﹕﹖﹗﹚﹜﹞！），．：；？］｜｝､",
    hanging: "!),.:;?]}’”′、。〉》」』】〕〞﹚﹜﹞！），．：；？］｝",
};

const KOREAN: Kinsoku = Kinsoku {
    no_end: "$([\\{£¥‘“〈《「『【〔＄（［｛￡￥￦",
    no_start: "!%),.:;?]}¢°’”′″℃〉》」』】〕！％），．：；？］｝￠",
    no_start_strict: "!%),.:;?]}¢°’”′″℃〉》」』】〕！％），．：；？］｝￠",
    hanging: "!%),.:;?]}¢°’”′″℃〉》」』】〕！％），．：；？］｝￠",
};

/// The kinsoku classes of a run's East Asian language (`w:lang w:eastAsia`); none for other languages.
fn kinsoku(lang: &str) -> Option<&'static Kinsoku> {
    let is = |l: &str| lang.eq_ignore_ascii_case(l);
    if is("ko-KR") {
        Some(&KOREAN)
    } else if is("ja-JP") {
        Some(&JAPANESE)
    } else if is("zh-CN") || is("ii-CN") {
        Some(&SIMPLIFIED_CHINESE)
    } else if is("zh-TW") {
        Some(&TRADITIONAL_CHINESE)
    } else {
        None
    }
}

fn hangul(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF)
}

pub(super) fn text_of<'a>(sp: &'a ShapedPara, p: &Piece) -> &'a str {
    &sp.text[p.text.0 as usize..p.text.1 as usize]
}

#[allow(
    clippy::too_many_arguments,
    reason = "the measured line-breaking helpers take the shaped paragraph, its pieces, style and frame separately, as they were ported"
)]
fn resolve_tab(sp: &ShapedPara, p: &Pending, pieces: &[Piece], xs: &mut [f64], ws: &mut [f64], start: usize, upto: usize) -> f64 {
    let seg: Vec<usize> = (p.from..upto).collect();
    let w: f64 = seg.iter().map(|&k| ws[k - start]).sum();
    let trail: f64 = seg.iter().rev().take_while(|&&k| pieces[k].kind == Kind::Space).map(|&k| ws[k - start]).sum();
    let d = match p.kind {
        TabKind::Right => w - trail,
        TabKind::Center => (w - trail) / 2.0,
        TabKind::Decimal => {
            let mut acc = 0.0;
            let mut found = false;
            for &k in &seg {
                let t = text_of(sp, &pieces[k]);
                if t.contains('.') {
                    found = true;
                    break;
                }
                acc += ws[k - start];
            }
            if found { acc } else { w - trail }
        }
        _ => 0.0,
    };
    let mut tw = (p.stop - p.pen_at_tab) - d;
    if p.pen_at_tab + tw + w - trail > p.edge + EPS && p.stop <= p.edge + EPS {
        tw = if p.kind == TabKind::Decimal && !p.shift { 0.0 } else { p.edge - (p.pen_at_tab + w) + trail };
    }
    let tw = if p.collapse { 0.0 } else { tw.max(0.0) };
    if let Some(t) = p.tab {
        ws[t - start] = tw;
    }
    let mut x = p.pen_at_tab + tw;
    for &k in &seg {
        xs[k - start] = x;
        x += ws[k - start];
    }
    x
}

/// The sides a line's part of a text border's box gets where the line breaks the box (as laid out, as drawn): before
/// piece `start` when the box goes on from the piece before it.
fn lead_side(sp: &ShapedPara, pieces: &[Piece], start: usize) -> (f64, f64) {
    let at = origin(sp, pieces) + start;
    at.checked_sub(1).and_then(|k| crate::layout::text::box_continues(&sp.pieces, &sp.styles, k)).map_or((0.0, 0.0), crate::layout::text::border_side)
}

/// The side the line's part of a box needs after piece `k` when the line ends there and the box goes on.
fn open_side(sp: &ShapedPara, pieces: &[Piece], k: usize) -> f64 {
    let at = origin(sp, pieces) + k;
    crate::layout::text::box_continues(&sp.pieces, &sp.styles, at).map_or(0.0, |b| crate::layout::text::border_side(b).0)
}

// Greedy fit
#[allow(
    clippy::too_many_arguments,
    reason = "the measured line-breaking helpers take the shaped paragraph, its pieces, style and frame separately, as they were ported"
)]
fn fit_line(
    sp: &ShapedPara,
    pieces: &[Piece],
    start: usize,
    left: f64,
    right: f64,
    style: &ParaStyle,
    frame: &Frame,
    limit_end: Option<usize>,
    strict: bool,
) -> Fit {
    // A text border's box broken across lines: each line's part has both sides, in fitting and drawn.
    let lead = lead_side(sp, pieces, start);
    let mut fit = fit_greedy(sp, pieces, start, left, right, style, frame, limit_end, strict, lead);
    if lead.1 > 0.0 && !fit.xs.is_empty() {
        fit.xs[0] -= lead.1;
        fit.widths[0] += lead.1;
    }
    let (all, o) = (&sp.pieces[..], origin(sp, pieces));
    if let Some(k) = (start..fit.end.min(pieces.len())).rev().find(|&k| pieces[k].kind != Kind::Space)
        && fit.end < pieces.len()
        && let Some(b) = crate::layout::text::boxed(all, &sp.styles, o + k)
        && (k..fit.end).all(|j| crate::layout::text::box_continues(all, &sp.styles, o + j).is_some())
    {
        let drawn = crate::layout::text::border_side(b).1;
        fit.widths[k - start] += drawn;
        for x in &mut fit.xs[k - start + 1..] {
            *x += drawn;
        }
    }
    fit
}

#[allow(
    clippy::too_many_arguments,
    reason = "the measured line-breaking helpers take the shaped paragraph, its pieces, style and frame separately, as they were ported"
)]
fn fit_greedy(
    sp: &ShapedPara,
    pieces: &[Piece],
    start: usize,
    left: f64,
    mut right: f64,
    style: &ParaStyle,
    frame: &Frame,
    limit_end: Option<usize>,
    strict: bool,
    lead: (f64, f64),
) -> Fit {
    let mut xs: Vec<f64> = Vec::new();
    let mut ws: Vec<f64> = Vec::new();
    let mut pen = left + lead.1;
    // How much wider the pieces so far count for breaking than they are drawn.
    let lead_drift = lead.0 - lead.1;
    let mut drift = lead_drift;
    if start == 0 && style.list_jc != "left" {
        let w: f64 = pieces.iter().take_while(|p| p.kind == Kind::ListNumber).map(|p| p.width).sum();
        pen -= if style.list_jc == "right" { w } else { w / 2.0 };
    }
    // A cell paragraph whose only stop is a decimal one: each line starts as if with a tab to it, the first after its
    // list number and the number's suffix, when the stop lies past them.
    let edge = right;
    let lead = |pen: f64, from: usize| {
        custom_stops(pen, pen_units(pen, left, style, frame), style)
            .find(|t| t.kind == TabKind::Decimal)
            .filter(|_| sp.decimal_lead)
            .map(|t| Pending { tab: None, from, stop: t.pos, kind: t.kind, pen_at_tab: pen, drift: lead_drift, collapse: false, edge, shift: false })
    };
    let numbered = pieces[start..].iter().take_while(|p| p.kind == Kind::ListNumber).count();
    let lead_at = if numbered == 0 {
        start
    } else {
        start + numbered + usize::from(pieces.get(start + numbered).is_some_and(|p| matches!(p.kind, Kind::Tab | Kind::Space)))
    };
    let mut pending = if lead_at == start { lead(pen, start) } else { None };
    let mut breaks: Vec<(usize, bool)> = Vec::new();
    let n = pieces.len();
    let (all, o) = (&sp.pieces[..], origin(sp, pieces));
    let in_word = |k: usize| in_word(all, k);
    let points = Points::new(sp, pieces, start, frame);
    let mut leaders: Vec<(usize, Leader)> = Vec::new();
    let mut squeeze: Option<(usize, f64)> = None;
    let mut past_custom = false;
    // Compat ≤ 14, justified: centre-tab text that wraps leaves its tab no width on the line it wraps from, and the
    // line's justification fills the gap.
    let collapse_on_wrap = frame.compat == Compat::Legacy && style.jc == Jc::Both;
    // East Asian paragraph layout: distributed, or the document's default East Asian font is an East Asian one.
    let east_asian = style.jc == Jc::Distribute || sp.east_asian_default;
    let rules = frame.breaking_rules || east_asian;
    let classes = |k: usize| kinsoku(&sp.styles[pieces[k].style as usize].run.lang_east_asia).filter(|_| rules && !style.no_kinsoku);
    let first_char = |k: usize| text_of(sp, &pieces[k]).chars().next();
    let last_char = |k: usize| text_of(sp, &pieces[k]).chars().last();
    // A break at `at` (after any spaces) is refused when the last character before it doesn't end a line or the
    // next one doesn't start one.
    let refused = |at: usize| {
        let no_start = |t: &Kinsoku| if frame.strict_kinsoku { t.no_start_strict } else { t.no_start };
        (start..at)
            .rev()
            .find(|&k| pieces[k].kind != Kind::Space)
            .is_some_and(|k| classes(k).is_some_and(|t| last_char(k).is_some_and(|c| t.no_end.contains(c))))
            || (at..n)
                .find(|&k| pieces[k].kind != Kind::Space)
                .is_some_and(|k| classes(k).is_some_and(|t| first_char(k).is_some_and(|c| no_start(t).contains(c))))
    };
    // Under the breaking rules a line may break between two characters unless both wrap as words: Hangul and every
    // character of a Korean run with `w:wordWrap` on (its punctuation too: such a run breaks only at spaces), other
    // text outside the East Asian characters only with it on.
    let korean_run = |k: usize| sp.styles[pieces[k].style as usize].run.lang_east_asia.eq_ignore_ascii_case("ko-KR");
    let wraps = |k: usize, c: char| {
        let east = crate::layout::text::is_east_asian(c as u32);
        if hangul(c) || korean_run(k) { !style.no_word_wrap || !east && c.is_alphanumeric() } else { !style.no_word_wrap && !east }
    };
    let between = |k: usize| {
        rules
            && k + 1 < n
            && pieces[k].kind == Kind::Text
            && pieces[k + 1].kind == Kind::Text
            && !pieces[k + 1].glued
            && !(last_char(k).is_some_and(|c| wraps(k, c)) && first_char(k + 1).is_some_and(|c| wraps(k + 1, c)))
    };
    // topLinePunct: an opening bracket that starts a line keeps only its right half; an opening quote too when it is
    // in the run's East Asian font (`w:hint="eastAsia"`).
    let hinted = |k: usize| sp.styles[pieces[k].style as usize].run.hint_east_asia;
    if east_asian
        && style.top_line_punct
        && pieces.get(start).is_some_and(|p| p.kind == Kind::Text)
        && first_char(start).is_some_and(|c| "〈《「『【〔（［｛".contains(c) || "‘“".contains(c) && hinted(start))
    {
        pen -= pieces[start].width / 2.0;
    }
    // Punctuation that may hang past the right margin, in East Asian paragraphs; compat 15 hangs none.
    let hangs = |k: usize| {
        east_asian
            && frame.compat == Compat::Legacy
            && !style.no_overflow_punct
            && !style.in_frame
            && classes(k).is_some_and(|t| first_char(k).is_some_and(|c| t.hanging.contains(c)))
    };
    // Compat ≤ 14 measures the hyphen's fit and the hyphenation zone on the widths it fits with, not as drawn.
    let fit_shift = |k: usize| {
        if frame.compat == Compat::Legacy { lead_drift + pieces[start..k].iter().map(|q| q.break_extra).sum::<f64>() } else { 0.0 }
    };
    let mut i = start;
    while i < n {
        if let Some(le) = limit_end
            && i >= le
        {
            break;
        }
        if i == lead_at && i > start && pending.is_none() {
            pending = lead(pen, i);
        }
        let p = &pieces[i];
        match p.kind {
            Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak => {
                if let Some(pt) = pending.take() {
                    pen = resolve_tab(sp, &pt, pieces, &mut xs, &mut ws, start, i);
                }
                xs.push(pen);
                ws.push(0.0);
                if let Some((w, cap)) = squeeze {
                    shrink_spaces(sp, pieces, start, w, cap, right, &mut xs, &mut ws);
                }
                let mut end = i + 1;
                if p.kind == Kind::PageBreak && !frame.split_page_break && end < n && pieces[end..].iter().all(|q| q.kind == Kind::Anchor) {
                    for _ in end..n {
                        xs.push(pen);
                        ws.push(0.0);
                    }
                    end = n;
                }
                return Fit { end, xs, widths: ws, hard: Some(p.kind), hyphen: false, leaders };
            }
            Kind::Tab => {
                if let Some(pt) = pending.take() {
                    pen = resolve_tab(sp, &pt, pieces, &mut xs, &mut ws, start, i);
                }
                let number_tab = i > 0 && pieces[..i].iter().all(|q| q.kind == Kind::ListNumber);
                let units = pen_units(pen, left, style, frame);
                let (mut stop, mut kind, leader) = match p.object.map(|o| &sp.objects[o as usize]) {
                    Some(Object::PTab(t)) => ptab_stop(t, style, frame),
                    _ => next_stop(pen, units, style, frame, number_tab),
                };
                // A list number's tab to a decimal stop uses it as a left stop.
                if number_tab && kind == TabKind::Decimal {
                    kind = TabKind::Left;
                }
                let stop_custom = p.object.is_none() && custom_stops(pen, units, style).next().is_some();
                let custom = p.object.is_some() || stop_custom;
                if !custom && !number_tab && stop > right + EPS && !past_custom && i > start && limit_end.is_none() {
                    return Fit { end: i, xs, widths: ws, hard: None, hyphen: false, leaders };
                }
                let (edge, margin) = (frame.width - style.ind_right, frame.width);
                // Compat 15 with a right indent: a custom stop from the right edge to the margin is used where it is
                // and its text may run on to the margin, the tab staying on the line.
                let mut indent_stop = false;
                if frame.compat == Compat::Generic && stop_custom {
                    let beyond = stop > margin + EPS;
                    indent_stop = !beyond && edge < margin - EPS && stop >= edge - EPS && right >= edge - EPS;
                    if indent_stop {
                        right = margin;
                        breaks.clear();
                    } else if stop > edge + EPS || beyond {
                        if kind != TabKind::Left && !pieces[i + 1..].iter().any(|q| q.kind == Kind::Tab) {
                            stop = edge;
                            kind = TabKind::Right;
                        } else if i > start && limit_end.is_none() {
                            return Fit { end: i, xs, widths: ws, hard: None, hyphen: false, leaders };
                        } else {
                            stop = edge;
                            kind = TabKind::Left;
                        }
                    }
                }
                // Compat ≤ 14: a custom stop at the right edge counts as past it.
                let past = frame.compat == Compat::Legacy && custom && (stop > right + EPS || stop_custom && stop >= right - EPS);
                past_custom |= past;
                if leader != Leader::None {
                    leaders.push((i - start, leader));
                }
                xs.push(pen);
                if kind == TabKind::Left || kind == TabKind::Bar {
                    let w = (stop - pen).max(0.0);
                    ws.push(w);
                    pen += w;
                } else {
                    ws.push(0.0);
                    let edge = if past { f64::INFINITY } else { right };
                    pending =
                        Some(Pending { tab: Some(i), from: i + 1, stop, kind, pen_at_tab: pen, drift, collapse: false, edge, shift: indent_stop });
                }
                // A line breaks before a tab, not after it: a tab whose next word doesn't fit goes to the next line
                // with it, and a word too long after a tab first on its line is broken on that line (beside a
                // float such a tab stands alone in its piece). A list number's tab isn't a break opportunity.
                if !number_tab && !indent_stop && (i > start || strict) {
                    breaks.push((if i > start { i } else { i + 1 }, false));
                }
                i += 1;
                continue;
            }
            Kind::Space | Kind::Anchor => {
                let ink = (start..i).rev().find(|&j| pieces[j].kind != Kind::Space);
                if p.kind == Kind::Space
                    && !p.glued
                    && frame.compat == Compat::Legacy
                    && ink.is_some_and(|j| pen - pieces[j + 1..i].iter().map(|q| q.width).sum::<f64>() > right + EPS)
                    && squeeze.is_none()
                    && pending.is_none()
                    && limit_end.is_none()
                    && !pieces[start..i].iter().any(|q| matches!(q.kind, Kind::Tab | Kind::ListNumber))
                {
                    return Fit { end: i, xs, widths: ws, hard: None, hyphen: false, leaders };
                }
                xs.push(pen);
                ws.push(p.width);
                pen += p.width;
                drift += p.break_extra;
                if p.break_after && !pieces[start..=i].iter().all(|q| q.kind == Kind::Anchor) && !refused(i + 1) {
                    breaks.push((i + 1, false));
                }
                i += 1;
                continue;
            }
            _ => {}
        }
        let right_edge = match &pending {
            Some(pt) if pt.kind == TabKind::Right => {
                let w: f64 = ws[pt.from - start..].iter().sum::<f64>() + p.width + drift - pt.drift + p.break_extra;
                (pt.pen_at_tab + pt.drift + w).max(pt.stop)
            }
            Some(pt) if pt.kind == TabKind::Center => {
                let w: f64 = ws[pt.from - start..].iter().sum::<f64>() + p.width + drift - pt.drift + p.break_extra;
                // A stop at or before the right edge narrows its tab so the text ends at the edge.
                let centred = pt.stop + w / 2.0;
                (pt.pen_at_tab + pt.drift + w).max(if pt.stop <= right + EPS { centred.min(right) } else { centred })
            }
            _ => pen + p.width + drift + p.break_extra,
        } + if p.kind == Kind::Text { open_side(sp, pieces, i) } else { 0.0 };
        let forced = limit_end.is_some();
        let limit = match &pending {
            Some(pt) if pt.kind == TabKind::Right && pt.stop > right => pt.stop,
            _ if past_custom => f64::INFINITY,
            _ => right,
        };
        let over =
            past(right_edge - left, limit - left) && !forced && p.kind != Kind::ListNumber && !(pen <= limit + EPS && pending.is_none() && hangs(i));
        let word = breaks.last().map_or(start, |b| b.0);
        let room: f64 = match squeeze {
            _ if !over => 0.0,
            Some((w, cap)) if w == word => cap,
            None if frame.compat == Compat::Legacy && word > start && pending.is_none() => {
                (squeeze_from(pieces, start, word)..word).map(|k| shrink(sp, pieces, k)).sum()
            }
            _ => 0.0,
        };
        if over && right_edge <= limit + room + EPS {
            squeeze = Some((word, room));
        } else if over {
            // Compat ≤ 14: at least three letters must fit, and the hyphen goes at least one letter before the first
            // letter that doesn't fit.
            let mut first_unfit = i;
            while let Some(&(at, soft)) = breaks.last() {
                if at <= start {
                    breaks.pop();
                    continue;
                }
                if soft {
                    let auto = !pieces[at - 1].soft_hyphen;
                    // A word without proofing isn't hyphenated automatically: compat 15 asks the run of the word's
                    // first letter, compat ≤ 14 that of the first character that doesn't fit.
                    let asked = if frame.compat == Compat::Legacy { &pieces[i] } else { &all[(0..o + at).rev().find(|&k| !in_word(k)).unwrap_or(0)] };
                    if auto && sp.styles[asked.style as usize].run.no_proof {
                        breaks.pop();
                        continue;
                    }
                    let hy = hyphen_width(sp, &pieces[at - 1]);
                    let pen_at = xs[at - 1 - start] + if auto { ws[at - 1 - start] } else { 0.0 } + fit_shift(if auto { at } else { at - 1 });
                    let fits = !past(pen_at + hy - left, right - left);
                    let legacy = auto && frame.compat == Compat::Legacy;
                    if legacy {
                        let word = (start..at).rev().find(|&k| pieces[k].kind != Kind::Text).map_or(start, |k| k + 1);
                        if first_unfit <= word + 2 || at + 1 > first_unfit {
                            breaks.pop();
                            continue;
                        }
                        if !fits {
                            first_unfit = at - 1;
                        }
                    }
                    if !fits || auto && !zoned(pieces, start, at, &xs, right, frame, &fit_shift) {
                        breaks.pop();
                        continue;
                    }
                }
                xs.truncate(at - start);
                ws.truncate(at - start);
                leaders.retain(|(k, _)| *k < at - start);
                if let Some((w, cap)) = squeeze.filter(|&(w, _)| at > w) {
                    shrink_spaces(sp, pieces, start, w, cap, limit, &mut xs, &mut ws);
                }
                if let Some(pt) = pending.take()
                    && pt.from <= at
                {
                    let pt = Pending { collapse: collapse_on_wrap && pt.kind == TabKind::Center, ..pt };
                    resolve_tab(sp, &pt, pieces, &mut xs, &mut ws, start, at);
                }
                return Fit { end: at, xs, widths: ws, hard: None, hyphen: soft, leaders };
            }
            if strict {
                return Fit { end: start, xs: Vec::new(), widths: Vec::new(), hard: None, hyphen: false, leaders: Vec::new() };
            }
            // Before a fitted region unless it starts the line; no kinsoku, and spaces that don't fit go down.
            let mut at = (start..=i).rev().find(|&k| !pieces[k].glued).unwrap_or(start);
            while at > start + 1 && pieces[at - 1].kind == Kind::Space && xs[at - 1 - start] + ws[at - 1 - start] > right + EPS {
                at -= 1;
            }
            // The list number and its tab aren't the character a line takes at least.
            let number_part = |k: usize| {
                pieces[..=k].iter().all(|q| q.kind == Kind::ListNumber)
                    || k > 0 && pieces[k].kind == Kind::Tab && pieces[..k].iter().all(|q| q.kind == Kind::ListNumber)
            };
            if at > start && (start..at).any(|k| pieces[k].kind != Kind::Anchor && !number_part(k)) {
                xs.truncate(at - start);
                ws.truncate(at - start);
                leaders.retain(|(k, _)| *k < at - start);
                if let Some(pt) = pending.take().filter(|pt| pt.from <= at) {
                    let pt = Pending { collapse: collapse_on_wrap && pt.kind == TabKind::Center, ..pt };
                    resolve_tab(sp, &pt, pieces, &mut xs, &mut ws, start, at);
                }
                return Fit { end: at, xs, widths: ws, hard: None, hyphen: false, leaders };
            }
        }
        xs.push(pen);
        ws.push(p.width);
        pen += p.width;
        drift += p.break_extra;
        if p.break_after {
            breaks.push((i + 1, p.soft_hyphen));
        } else if points.at(pieces, i) {
            breaks.push((i + 1, true));
        } else if between(i) && !refused(i + 1) {
            breaks.push((i + 1, false));
        }
        i += 1;
    }
    if let Some(pt) = pending.take() {
        resolve_tab(sp, &pt, pieces, &mut xs, &mut ws, start, i);
    }
    if let Some((w, cap)) = squeeze {
        shrink_spaces(sp, pieces, start, w, cap, right, &mut xs, &mut ws);
    }
    let hyphen = i < n && i > start && pieces[i - 1].soft_hyphen;
    Fit { end: i, xs, widths: ws, hard: None, hyphen, leaders }
}

fn in_word(all: &[Piece], k: usize) -> bool {
    k > 0 && k < all.len() && all[k].kind == Kind::Text && all[k - 1].kind == Kind::Text && !all[k - 1].break_after
}

/// The automatic hyphenation points a line may end at. A line that starts inside a word hyphenates the rest of it as
/// a word: from the line start in compat ≤ 14, from where it was last broken between letters in compat 15.
struct Points {
    part: Option<(usize, Vec<usize>)>,
    on: bool,
}

impl Points {
    /// For the line at `start` of `pieces`, a run of the paragraph's pieces.
    fn new(sp: &ShapedPara, pieces: &[Piece], start: usize, frame: &Frame) -> Points {
        let (all, o) = (&sp.pieces[..], origin(sp, pieces));
        let at = o + start;
        let word_start = (0..=at).rev().find(|&k| !in_word(all, k)).unwrap_or(0);
        // A word continued from an earlier page counts as broken at a hyphen when it was broken at a point.
        let broke = frame.word_from.map(|w| w + o).unwrap_or(if word_start < o && !all[o - 1].auto_hyphen { o } else { 0 });
        let from = if frame.compat == Compat::Legacy { at } else { broke.clamp(word_start, at) };
        let part = (frame.hyphenate && from > word_start).then(|| {
            let (e, at) = sp.points_from(all, from);
            (e - o, at.into_iter().map(|k| k - o).collect::<Vec<_>>())
        });
        Points { part, on: frame.hyphenate }
    }

    /// The line may break after piece `i`.
    fn at(&self, pieces: &[Piece], i: usize) -> bool {
        self.on
            && match &self.part {
                Some((e, at)) if i < *e => !pieces[i].glued && at.contains(&i),
                _ => pieces[i].auto_hyphen,
            }
    }
}

// Hyphenation zone
fn zoned(pieces: &[Piece], start: usize, at: usize, xs: &[f64], right: f64, frame: &Frame, shift: &dyn Fn(usize) -> f64) -> bool {
    if frame.hyphen_zone <= 0.0 {
        return true;
    }
    // Measured from the word's first letter; a word that starts the line isn't tested.
    let word = (start..at).rev().find(|&k| pieces[k].kind != Kind::Text).map_or(start, |k| k + 1);
    word == start || right - xs[word - start] - shift(word) >= frame.hyphen_zone - EPS
}

// Space squeeze
fn squeeze_from(pieces: &[Piece], start: usize, word: usize) -> usize {
    pieces[start..word].iter().rposition(|q| q.kind == Kind::Tab).map_or(start, |t| start + t + 1)
}

fn shrink(sp: &ShapedPara, pieces: &[Piece], k: usize) -> f64 {
    let p = &pieces[k];
    let st = &sp.styles[p.style as usize];
    let space = |j: Option<usize>| j.and_then(|j| pieces.get(j)).is_some_and(|q| q.kind == Kind::Space);
    if p.kind != Kind::Space || st.squeeze_avg == 0.0 || text_of(sp, p) != " " || space(k.checked_sub(1)) || space(Some(k + 1)) {
        return 0.0;
    }
    (p.width - st.squeeze_avg * st.size * st.run.scale / 100.0 / 2.0).max(0.0)
}

#[allow(clippy::too_many_arguments, reason = "the line's pieces, the squeezed word and its room, and the positions it rewrites")]
fn shrink_spaces(sp: &ShapedPara, pieces: &[Piece], start: usize, word: usize, cap: f64, limit: f64, xs: &mut [f64], ws: &mut [f64]) {
    let Some(last) = (0..xs.len()).rev().find(|&k| pieces[start + k].kind != Kind::Space) else { return };
    let over = xs[last] + ws[last] - limit;
    if over <= 0.0 {
        return;
    }
    let f = (over / cap).min(1.0);
    for k in squeeze_from(pieces, start, word) - start..(word - start).min(ws.len()) {
        ws[k] -= shrink(sp, pieces, start + k) * f;
    }
    for k in 1..xs.len() {
        xs[k] = xs[k - 1] + ws[k - 1];
    }
}

/// A space a justified line widens: not the four-per-em or zero-width space, which only hang.
fn stretches(sp: &ShapedPara, p: &Piece) -> bool {
    p.kind == Kind::Space && !matches!(text_of(sp, p), "\u{2005}" | "\u{200B}")
}

fn hyphen_width(sp: &ShapedPara, p: &Piece) -> f64 {
    let st = &sp.styles[p.style as usize];
    match &st.face {
        Some(f) => f.glyph('-').map(|g| f.advance(g) * f.scale(st.size)).unwrap_or(0.0),
        None => 0.0,
    }
}

// Justified line ends
/// The compat 15 justified end of the line at `start` and whether it ends at a hyphenation point; None leaves the
/// line to [`fit_line`].
fn justified_end(sp: &ShapedPara, pieces: &[Piece], start: usize, column: f64, style: &ParaStyle, frame: &Frame) -> Option<(usize, bool)> {
    let seg_end = segment_end(pieces, start);
    match justified_line(sp, pieces, start, seg_end, column, style, frame) {
        LineChoice::End(e, hyphen) => Some((e, hyphen)),
        LineChoice::Last | LineChoice::Empty => (seg_end < pieces.len()).then_some((seg_end, false)),
        LineChoice::Abort => None,
    }
}

#[allow(clippy::too_many_arguments, reason = "the same inputs as fit_line, which it runs per end")]
fn justified_tab_end(
    sp: &ShapedPara,
    pieces: &[Piece],
    start: usize,
    left: f64,
    right: f64,
    style: &ParaStyle,
    frame: &Frame,
) -> Option<(usize, bool)> {
    if pieces.iter().any(|p| p.kind == Kind::Tab && p.object.is_some()) {
        return None;
    }
    let greedy = fit_line(sp, pieces, start, left, right, style, frame, None, false);
    let seg_end = segment_end(pieces, start);
    if greedy.hard.is_some() || greedy.end >= seg_end {
        return None;
    }
    let lu = |x: f64| x * LU_PER_PT;
    let column = line_column(right - left);
    let points = Points::new(sp, pieces, start, frame);
    let point = |e: usize| e < seg_end && (pieces[e - 1].soft_hyphen || points.at(pieces, e - 1) && proofed(sp, pieces, e - 1));
    let end_at = |e: usize| -> End {
        let all = fit_line(sp, pieces, start, left, right, style, frame, Some(e), false);
        let ink = (start..e).rev().find(|&k| !matches!(pieces[k].kind, Kind::Space | Kind::Anchor));
        let x = ink.map_or(left, |k| all.xs[k - start] + all.widths[k - start]) + if point(e) { hyphen_width(sp, &pieces[e - 1]) } else { 0.0 };
        let from = (start..e).rev().find(|&k| pieces[k].kind == Kind::Tab).map_or(start, |t| t + 1);
        let first = (from..e).find(|&k| pieces[k].kind != Kind::Space).unwrap_or(e);
        let last = ink.map_or(first, |k| k + 1);
        let (mut shrink, mut stretch) = (0, 0);
        for k in (first..last).filter(|&k| stretches(sp, &pieces[k])) {
            let (e, c) = space_room(&pieces[k], all.widths[k - start]);
            shrink += c;
            stretch += e;
        }
        End { at: e, over: lu(x - left).round() - column, shrink, stretch, point: point(e) }
    };
    // A line ends after a word or before a tab after something other than anchors (the tab and its text start the
    // next line), never between a tab and its text.
    let before_tab = |e: usize| pieces[e].kind == Kind::Tab && pieces[start..e].iter().any(|p| p.kind != Kind::Anchor);
    let is_end = |e: usize| pieces[e - 1].break_after && pieces[e].kind != Kind::Space || before_tab(e);
    let is_end = |e: usize| e == seg_end || is_end(e) || point(e);
    // From the last word end the greedy line reaches; no end past another tab.
    let lo = (start + 1..=greedy.end).rev().find(|&e| is_end(e) && !point(e)).unwrap_or(greedy.end);
    let mut ends: Vec<End> = Vec::new();
    for e in (lo..=seg_end).filter(|&e| e == lo || is_end(e)) {
        if e > greedy.end && pieces[greedy.end..e].iter().any(|p| p.kind == Kind::Tab) {
            break;
        }
        let end = end_at(e);
        if end.over > end.shrink as f64 + LU_EPS {
            break;
        }
        ends.push(end);
    }
    let best = choose_end(&ends)?;
    (best.at != greedy.end).then_some((best.at, best.point))
}

/// The width of the rest of the paragraph from `from` when it is the paragraph mark alone, or spaces, one character or
/// one object before it: what a line ending at the mark holds.
fn mark_rest(sp: &ShapedPara, pieces: &[Piece], from: usize) -> Option<f64> {
    let mut seen = false;
    let mut w = 0.0;
    for q in pieces.get(from..)? {
        match q.kind {
            Kind::Anchor => {}
            Kind::Space => w += q.width,
            Kind::Object if !seen => (seen, w) = (true, w + q.width),
            Kind::Text if !seen && sp.text.get(q.text.0 as usize..q.text.1 as usize).is_some_and(|t| t.chars().count() == 1) => {
                (seen, w) = (true, w + q.width)
            }
            _ => return None,
        }
    }
    Some(w)
}

fn segment_end(pieces: &[Piece], from: usize) -> usize {
    pieces[from..]
        .iter()
        .position(|p| matches!(p.kind, Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak))
        .map(|k| from + k + 1)
        .unwrap_or(pieces.len())
}

enum LineChoice {
    Empty,
    Last,
    Abort,
    End(usize, bool),
}

const LU_EPS: f64 = 1e-6;

fn end_demerits(over: f64, shrink: i64, stretch: i64) -> Option<i64> {
    let (used, cap) = if over > LU_EPS { (over, shrink) } else { (-over.min(0.0), stretch) };
    if cap <= 0 {
        return None;
    }
    let l = (1000.0 * used / cap as f64).floor() as i64;
    (l <= 4700).then(|| (1000 + l * l * l / 100_000).pow(2))
}

/// A line end the compat 15 justified breaker weighs: where the line ends, how far its natural width runs past the
/// column (layout units, the hyphen of a hyphenation point included), the shrink and stretch of its spaces, and
/// whether it ends at a hyphenation point.
#[derive(Clone, Copy)]
struct End {
    at: usize,
    over: f64,
    shrink: i64,
    stretch: i64,
    point: bool,
}

/// What a line ending at a hyphenation point (automatic or optional) pays on top of its demerits.
const POINT_DEMERITS: i64 = 103_693_489;

/// The compat 15 justified choice among a line's ends, in text order and each fitting with its spaces shrunk: the
/// longest, then each end before it down to the last word end that fits unshrunk; the cheapest wins, the first of
/// equals. None without an end.
fn choose_end(ends: &[End]) -> Option<End> {
    let longest = ends.len().checked_sub(1)?;
    let word = ends.iter().rposition(|e| !e.point && e.over <= LU_EPS).unwrap_or(0);
    let cost = |e: &End| end_demerits(e.over, e.shrink, e.stretch).map(|d| d + if e.point { POINT_DEMERITS } else { 0 });
    let (mut best, mut best_cost) = (ends[longest], cost(&ends[longest]));
    for e in ends[word.min(longest)..longest].iter().rev() {
        let c = cost(e);
        if c.is_some_and(|c| best_cost.is_none_or(|b| c < b)) {
            (best, best_cost) = (*e, c);
        }
    }
    Some(best)
}

/// A line's width in layout units, rounded half up.
fn line_column(width: f64) -> f64 {
    (width * LU_PER_PT + 0.5).floor()
}

/// Text `width` wide doesn't fit a line `column` wide: compared in layout units, the column rounded half up.
fn past(width: f64, column: f64) -> bool {
    width * LU_PER_PT > line_column(column) + LU_EPS
}

/// Compat 15 asks the run of a word's first letter whether the word is proofed.
fn proofed(sp: &ShapedPara, pieces: &[Piece], i: usize) -> bool {
    let all = &sp.pieces[..];
    let w = (0..=origin(sp, pieces) + i).rev().find(|&k| !in_word(all, k)).unwrap_or(0);
    !sp.styles[all[w].style as usize].run.no_proof
}

/// The compat 15 justified end of a line without tabs from `from`, in a column `column` points wide. Its ends are the
/// word ends and the hyphenation points: the automatic ones the line may take and every optional hyphen.
fn justified_line(sp: &ShapedPara, pieces: &[Piece], from: usize, seg_end: usize, column: f64, style: &ParaStyle, frame: &Frame) -> LineChoice {
    let points = Points::new(sp, pieces, from, frame);
    let lu = |w: f64| (w * LU_PER_PT).round();
    // Distributed text: every glyph but the line's last stretches by a 32nd of its font's average character width,
    // unless its run is glyph-shaped.
    let glyph_stretch = |p: &Piece| -> i64 { if style.jc == Jc::Distribute { glyph_stretch(sp, p).unwrap_or(0) } else { 0 } };
    let column = line_column(column);
    let mut ends: Vec<End> = Vec::new();
    let (mut natural, mut content, mut shrink, mut stretch, mut glyphs, mut last_glyph) = (lu(lead_side(sp, pieces, from).0), 0.0, 0, 0, 0, 0);
    // A line ending inside a text border's box closes it with a side.
    let open = |k: usize| lu(open_side(sp, pieces, k));
    let (mut word, mut seen) = (false, false);
    // The end of the word before the spaces being read: a space that doesn't break joins the words.
    let mut word_end: Option<usize> = None;
    let end_word = |ends: &mut Vec<End>, at: usize, natural: f64, shrink: i64, stretch: i64| {
        ends.push(End { at, over: natural - column, shrink, stretch, point: false });
        ends.len() - 1
    };
    for k in from..seg_end {
        let p = &pieces[k];
        match p.kind {
            Kind::Space => {
                if std::mem::take(&mut word) {
                    word_end = Some(end_word(&mut ends, k, natural + open(k - 1), shrink, stretch + glyphs - last_glyph));
                }
                natural += lu(p.width) + lu(p.break_extra);
                if seen && stretches(sp, p) {
                    let (e, c) = space_room(p, p.width);
                    shrink += c;
                    stretch += e;
                }
                if !p.break_after
                    && let Some(j) = word_end.take()
                {
                    ends.remove(j);
                }
            }
            Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak => {
                if std::mem::take(&mut word) {
                    end_word(&mut ends, k, natural + open(k - 1), shrink, stretch + glyphs - last_glyph);
                }
            }
            Kind::Anchor if !word => {}
            _ => {
                (word, seen, word_end) = (true, true, None);
                natural += lu(p.width) + lu(p.break_extra);
                content = natural;
                if !p.soft_hyphen {
                    last_glyph = glyph_stretch(p);
                    glyphs += last_glyph;
                }
                if k + 1 < seg_end && (p.soft_hyphen || points.at(pieces, k) && proofed(sp, pieces, k)) {
                    let over = natural + lu(hyphen_width(sp, p)) + open(k) - column;
                    ends.push(End { at: k + 1, over, shrink, stretch: stretch + glyphs, point: true });
                } else if p.break_after && !p.soft_hyphen {
                    word = false;
                    word_end = Some(end_word(&mut ends, k + 1, natural + open(k), shrink, stretch + glyphs - last_glyph));
                }
            }
        }
        // Past an end that doesn't fit no later one does (`over - shrink` only grows), and the text is past the column.
        if content > column + LU_EPS && ends.last().is_some_and(|e| e.over > e.shrink as f64 + LU_EPS) {
            break;
        }
    }
    if word {
        end_word(&mut ends, seg_end, natural + open(seg_end - 1), shrink, stretch + glyphs - last_glyph);
    }
    if !seen {
        return LineChoice::Empty;
    }
    if content <= column + LU_EPS {
        return LineChoice::Last;
    }
    let fitting = ends.iter().take_while(|e| e.over <= e.shrink as f64 + LU_EPS).count();
    let Some(best) = choose_end(&ends[..fitting]) else { return LineChoice::Abort };
    let mut end = best.at;
    while !best.point && end < seg_end && pieces[end].kind == Kind::Space {
        end += 1;
    }
    LineChoice::End(end, best.point)
}

// Alignment
#[allow(
    clippy::too_many_arguments,
    reason = "the measured line-breaking helpers take the shaped paragraph, its pieces, style and frame separately, as they were ported"
)]
fn align(line: &mut Line, sp: &ShapedPara, pieces: &[Piece], style: &ParaStyle, frame: &Frame, left: f64, right: f64, is_last: bool) {
    let n = line.xs.len();
    if n == 0 {
        return;
    }
    let idx = |k: usize| line.start + k;
    let mut content_end = n;
    while content_end > 0
        && matches!(pieces[idx(content_end - 1)].kind, Kind::Space | Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak | Kind::Anchor)
    {
        content_end -= 1;
    }
    let line_end_x = if content_end == 0 { left } else { line.xs[content_end - 1] + line.widths[content_end - 1] }
        + if line.hyphen { hyphen_width(sp, &pieces[idx(n - 1)]) } else { 0.0 };
    let ends_with_line_break = line.end > 0 && pieces.get(line.end - 1).is_some_and(|p| p.kind == Kind::LineBreak);
    let jc = match style.jc {
        Jc::Left if style.bidi => Jc::Right,
        Jc::Right if style.bidi => Jc::Left,
        jc => jc,
    };
    let to_right = |line: &mut Line| {
        let free = right - line_end_x;
        if free > 0.0 {
            for x in &mut line.xs {
                *x += free;
            }
        }
    };
    match jc {
        Jc::Both => {
            // A line ending in a text-wrapping break is justified too (the one before an empty last line included),
            // unless the document doesn't expand them.
            let justify = (!is_last || ends_with_line_break) && (line.hard.is_none() || line.hard == Some(Kind::LineBreak));
            let justify = justify && !(ends_with_line_break && !frame.expand_shift_return);
            let overflow = line_end_x - right;
            if !(justify || (frame.compat == Compat::Generic && overflow > EPS)) {
                if style.bidi {
                    to_right(line);
                }
                return;
            }
            let last_tab = (0..content_end).rev().find(|&k| pieces[idx(k)].kind == Kind::Tab).map(|k| k + 1).unwrap_or(0);
            let mut first = last_tab;
            while first < content_end && pieces[idx(first)].kind == Kind::Space {
                first += 1;
            }
            let spaces: Vec<usize> = (first..content_end).filter(|&k| stretches(sp, &pieces[idx(k)])).collect();
            if spaces.is_empty() {
                if style.bidi {
                    to_right(line);
                }
                return;
            }
            let extra = right - line_end_x;
            if justify && extra > 0.0 && frame.compat == Compat::Generic {
                let caps: Vec<(usize, f64)> = spaces.iter().map(|&k| (k, stretch_cap(&pieces[idx(k)], line.widths[k]))).collect();
                tiered(extra, &[&caps], &mut line.widths);
            } else if justify
                && extra < 0.0
                && frame.wp_justification
                && sp.styles[pieces[idx(spaces[0])].style as usize].face.as_ref().is_some_and(|f| f.fixed_pitch)
            {
                // wpJustification with fixed-pitch spaces: every advance but the last gives way, in 600 dpi pixels,
                // two short of the overflow; the last advances one more each for the remainder.
                let advances: Vec<usize> = (first..content_end - 1).collect();
                let t = ((-extra * 600.0 / 72.0).floor() as i64 - 2).max(0);
                let (base, rem) = (t / advances.len() as i64, (t % advances.len() as i64) as usize);
                for (j, &k) in advances.iter().enumerate() {
                    let px = base + i64::from(j >= advances.len() - rem);
                    line.widths[k] -= px as f64 * 72.0 / 600.0;
                }
            } else if justify && (extra > 0.0 || (extra < 0.0 && frame.wp_justification)) {
                // Equal shares; a balanced space counts its widening as share already taken.
                let taken: Vec<f64> = spaces.iter().map(|&k| pieces[idx(k)].natural.map_or(0.0, |n| (line.widths[k] - n).max(0.0))).collect();
                if extra > 0.0 {
                    let q = water_level(extra, &taken);
                    for (&k, t) in spaces.iter().zip(&taken) {
                        line.widths[k] += (q - t).max(0.0);
                    }
                } else {
                    for &k in &spaces {
                        line.widths[k] += extra / spaces.len() as f64;
                    }
                }
            } else if extra < 0.0 && frame.compat == Compat::Generic {
                let caps: Vec<(usize, f64)> = spaces.iter().map(|&k| (k, shrink_cap(&pieces[idx(k)], line.widths[k]))).collect();
                shrink_shared(-extra, &caps, &mut line.widths);
            } else {
                if style.bidi {
                    to_right(line);
                }
                return;
            }
            let mut x = line.xs[0];
            for k in 0..n {
                if k >= first || k == 0 {
                    line.xs[k] = x;
                }
                x = line.xs[k] + line.widths[k];
            }
        }
        Jc::Center | Jc::Right => {
            let free = right - line_end_x;
            if free > 0.0 {
                let off = if jc == Jc::Center { free / 2.0 } else { free };
                for x in &mut line.xs {
                    *x += off;
                }
            }
        }
        // Compat 15, every line, the last too: the spaces stretch first, up to half their width, then the glyphs of
        // runs that aren't glyph-shaped, up to a 32nd of their font's average width, each in proportion to what it
        // may take; the rest goes equally to all of them. Every character but the line's last takes part (the last
        // letter too before a hyphen). A line with none is centred; an overflowing one shrinks its spaces.
        Jc::Distribute if frame.compat == Compat::Generic => {
            let chars: Vec<usize> = (0..content_end).filter(|&k| pieces[idx(k)].kind != Kind::Tab).collect();
            let gaps = &chars[..chars.len() - usize::from(!line.hyphen && !chars.is_empty())];
            let bonus: Vec<(usize, f64)> = chars.windows(2).map(|w| (w[0], auto_space(sp, &pieces[idx(w[0])], &pieces[idx(w[1])], style))).collect();
            let extra = right - line_end_x - bonus.iter().map(|b| b.1).sum::<f64>();
            let spaces: Vec<(usize, f64)> =
                gaps.iter().filter(|&&k| stretches(sp, &pieces[idx(k)])).map(|&k| (k, stretch_cap(&pieces[idx(k)], line.widths[k]))).collect();
            let glyphs: Vec<(usize, f64)> = gaps
                .iter()
                .filter(|&&k| !pieces[idx(k)].soft_hyphen)
                .filter_map(|&k| glyph_stretch(sp, &pieces[idx(k)]).map(|g| (k, g as f64 / LU_PER_PT)))
                .collect();
            if spaces.is_empty() && glyphs.is_empty() {
                if extra > 0.0 {
                    for x in &mut line.xs {
                        *x += extra / 2.0;
                    }
                }
                return;
            }
            for &(k, b) in &bonus {
                line.widths[k] += b;
            }
            if extra > 0.0 {
                tiered(extra, &[&spaces, &glyphs], &mut line.widths);
            } else if extra < 0.0 {
                let caps: Vec<(usize, f64)> = spaces.iter().map(|&(k, _)| (k, shrink_cap(&pieces[idx(k)], line.widths[k]))).collect();
                shrink_shared(-extra, &caps, &mut line.widths);
            }
            let mut x = line.xs[0];
            for k in 0..n {
                line.xs[k] = x;
                x += line.widths[k];
            }
        }
        // Compat ≤ 14, every line, the last too: the gaps between the characters (spaces included), the first at the
        // left edge and the last ending at the right; a character alone is centred. Each space first grows towards its
        // font's average character width, then the rest goes equally to every gap. Where East Asian text meets a Latin
        // letter or a digit the gap takes half an em more.
        Jc::Distribute => {
            let chars: Vec<usize> = (0..content_end).filter(|&k| pieces[idx(k)].kind != Kind::Tab).collect();
            let extra = right - line_end_x;
            if chars.len() == 1 && extra > 0.0 {
                for x in &mut line.xs {
                    *x += extra / 2.0;
                }
            } else if chars.len() > 1 {
                let bonus: Vec<f64> = chars.windows(2).map(|w| auto_space(sp, &pieces[idx(w[0])], &pieces[idx(w[1])], style)).collect();
                let extra = extra - bonus.iter().sum::<f64>();
                if extra > 0.0 {
                    let gaps = &chars[..chars.len() - 1];
                    // A space's room to its target counts as share taken below nothing: one level over every gap.
                    let taken: Vec<f64> = gaps.iter().map(|&k| -distribute_room(sp, &pieces[idx(k)], line.widths[k])).collect();
                    let q = water_level(extra, &taken);
                    for (j, &k) in gaps.iter().enumerate() {
                        line.widths[k] += (q - taken[j]).max(0.0) + bonus[j];
                    }
                    let mut x = line.xs[0];
                    for k in 0..n {
                        line.xs[k] = x;
                        x += line.widths[k];
                    }
                }
            }
        }
        Jc::Left => {}
    }
}

/// A compat 15 space `w` points wide: how far it stretches before the rest is shared equally and how far it shrinks,
/// in layout units: half and a quarter of its width. A balanced space counts its widening as stretch already taken
/// (what is left of half its natural width) and doesn't shrink.
fn space_room(p: &Piece, w: f64) -> (i64, i64) {
    let lu = |w: f64| (w * LU_PER_PT).round() as i64;
    let w = lu(w);
    match p.natural {
        None => ((w + 1) >> 1, (w + 2) >> 2),
        Some(n) => {
            let n = lu(n);
            ((((n + 1) >> 1) - (w - n)).max(0), 0)
        }
    }
}

/// How far a space `w` points wide grows first in a compat ≤ 14 distributed line: up to its font's average character
/// width at its size, or half an em where the document balances its spaces and the run's East Asian font is an East
/// Asian one; letters none.
fn distribute_room(sp: &ShapedPara, p: &Piece, w: f64) -> f64 {
    let st = &sp.styles[p.style as usize];
    if !stretches(sp, p) {
        return 0.0;
    }
    let em = if st.half_em_spaces { 0.5 } else { st.face.as_ref().map_or(0.0, |f| f.avg_width) };
    ((em * st.size * LU_PER_PT).round() / LU_PER_PT - w).max(0.0)
}

fn stretch_cap(p: &Piece, w: f64) -> f64 {
    space_room(p, w).0 as f64 / LU_PER_PT
}

fn shrink_cap(p: &Piece, w: f64) -> f64 {
    space_room(p, w).1 as f64 / LU_PER_PT
}

/// The equal share `s` that spreads `extra` over spaces that have each taken `taken` already: Σ max(0, s − taken)
/// = extra. Without any taken, `extra` over their number.
fn water_level(extra: f64, taken: &[f64]) -> f64 {
    let mut t = taken.to_vec();
    t.sort_by(f64::total_cmp);
    let mut sum = 0.0;
    for (j, &d) in t.iter().enumerate() {
        sum += d;
        let s = (extra + sum) / (j + 1) as f64;
        if t.get(j + 1).is_none_or(|&next| s <= next) {
            return s;
        }
    }
    0.0
}

/// Compat 15's share of a line's `extra` width: each tier of opportunities (piece, capacity) in turn takes what is
/// left in proportion to its capacities, up to them; anything still left goes equally to every opportunity.
fn tiered(extra: f64, tiers: &[&[(usize, f64)]], widths: &mut [f64]) {
    let mut rest = extra;
    for tier in tiers {
        let cap: f64 = tier.iter().map(|t| t.1).sum();
        if cap > 0.0 {
            let take = rest.min(cap);
            for &(k, c) in tier.iter() {
                widths[k] += take * c / cap;
            }
            rest -= take;
        }
    }
    let n: usize = tiers.iter().map(|t| t.len()).sum();
    if rest > 0.0 && n > 0 {
        for &(k, _) in tiers.iter().flat_map(|t| t.iter()) {
            widths[k] += rest / n as f64;
        }
    }
}

/// Compat 15's squeeze of an overflowing line: its spaces (piece, shrink) give way in proportion to their shrink, all
/// of it at most.
fn shrink_shared(over: f64, caps: &[(usize, f64)], widths: &mut [f64]) {
    let c: f64 = caps.iter().map(|s| s.1).sum();
    if c <= 0.0 {
        return;
    }
    let used = over.min(c);
    for &(k, cap) in caps {
        widths[k] -= used * cap / c;
    }
}

/// A run is glyph-shaped when its kerning is active or it asks for OpenType typography (ligatures, number forms or
/// spacing, contextual alternates, stylistic sets): its glyphs don't stretch in a compat 15 distributed line.
fn glyph_shaped(r: &RunStyle) -> bool {
    r.kern > 0.0 && r.size >= r.kern - 1e-9 || r.ligatures != 0 || r.figures != 0
}

/// How far a glyph of a compat 15 distributed line stretches before the rest is shared (layout units): a 32nd of
/// its font's average character width; none when it isn't a glyph or its run is glyph-shaped.
fn glyph_stretch(sp: &ShapedPara, p: &Piece) -> Option<i64> {
    let st = &sp.styles[p.style as usize];
    (matches!(p.kind, Kind::Text | Kind::Object | Kind::ListNumber) && !glyph_shaped(&st.run))
        .then(|| st.face.as_ref().map_or(0, |f| ((f.avg_width * st.size * LU_PER_PT).round() as i64 + 16) >> 5))
}

/// The extra space between two characters of a distributed line: half the East Asian one's em where East Asian
/// text meets a Latin letter (`w:autoSpaceDE`) or a digit (`w:autoSpaceDN`).
fn auto_space(sp: &ShapedPara, a: &Piece, b: &Piece, style: &ParaStyle) -> f64 {
    let (Some(ca), Some(cb)) = (text_of(sp, a).chars().last(), text_of(sp, b).chars().next()) else { return 0.0 };
    let ea = |c: char| crate::layout::text::is_east_asian(c as u32);
    let latin = |c: char| !ea(c) && (c.is_ascii_digit() && !style.no_auto_space_dn || c.is_alphabetic() && !style.no_auto_space_de);
    let east = match (ea(ca), ea(cb)) {
        (true, false) if latin(cb) => a,
        (false, true) if latin(ca) => b,
        _ => return 0.0,
    };
    sp.styles[east.style as usize].size / 2.0
}
