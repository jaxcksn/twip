//! Fields whose value doesn't depend on pagination (FIELD-1), computed once after reading: `SEQ`, `REF`, `IF`, `DATE`,
//! `TIME` and body `STYLEREF`. The result replaces the cached text; page-dependent fields are evaluated at layout.

use std::collections::HashMap;

use super::model::{Block, Document, FieldId, FieldKind, FieldView, Inline, Paragraph, Run};

/// A field's markers in a paragraph's inlines: the indexes of its begin, separator (if it has one) and
/// end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldSpan {
    pub begin: usize,
    pub sep: Option<usize>,
    pub end: usize,
}

impl FieldSpan {
    /// Where its code ends: its separator, or its end when it has none.
    pub fn code_end(&self) -> usize {
        self.sep.unwrap_or(self.end)
    }
}

/// Every complete field of `inlines`, by where it begins (an outer field before the fields inside it).
pub fn spans(inlines: &[Inline]) -> Vec<FieldSpan> {
    let mut open: Vec<(FieldId, usize, Option<usize>)> = Vec::new();
    let mut out = Vec::new();
    for (k, i) in inlines.iter().enumerate() {
        match i {
            Inline::FieldBegin { id, .. } => open.push((*id, k, None)),
            Inline::FieldSep { id, .. } => {
                if let Some(o) = open.iter_mut().rev().find(|o| o.0 == *id) {
                    o.2.get_or_insert(k);
                }
            }
            Inline::FieldEnd { id, .. } => {
                if let Some(p) = open.iter().rposition(|o| o.0 == *id) {
                    let (_, begin, sep) = open.remove(p);
                    out.push(FieldSpan { begin, sep, end: k });
                }
            }
            _ => {}
        }
    }
    out.sort_by_key(|s| s.begin);
    out
}

/// The span of the field beginning at inline `begin`.
pub fn span_at(inlines: &[Inline], begin: usize) -> Option<FieldSpan> {
    spans(inlines).into_iter().find(|s| s.begin == begin)
}

/// A field's own code text: the text from its begin to its separator, the fields nested in it left out.
pub fn code_text(inlines: &[Inline], s: FieldSpan) -> String {
    let mut depth = 0usize;
    let mut out = String::new();
    for i in &inlines[s.begin + 1..s.code_end()] {
        match i {
            Inline::FieldBegin { .. } => depth += 1,
            Inline::FieldEnd { .. } => depth = depth.saturating_sub(1),
            Inline::Text(t, _) if depth == 0 => out.push_str(t),
            Inline::Tab(_) | Inline::PTab(..) if depth == 0 => out.push('\t'),
            _ => {}
        }
    }
    out
}

/// A field's result as shown: its result's text, the fields nested in it as theirs.
pub fn result_text(inlines: &[Inline], s: FieldSpan) -> String {
    let Some(sep) = s.sep else { return String::new() };
    visible(&inlines[sep + 1..s.end])
}

/// How results show an inline, whatever the fields' views.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shown {
    Itself,
    /// A marker, a code, or the result of an evaluated field.
    Not,
    /// An evaluated field's begin marker: the field shows this (its value, or its result's text).
    Value(String),
}

/// How results show each of `inlines`.
pub fn shown(inlines: &[Inline]) -> Vec<Shown> {
    show(inlines, false)
}

/// How layout shows each of `inlines`: each field in its view. Showing its code, a field's begin
/// and end markers are its braces and its result isn't drawn; showing its result, an empty one shows its label.
pub fn layout_shown(inlines: &[Inline]) -> Vec<Shown> {
    show(inlines, true)
}

fn show(inlines: &[Inline], views: bool) -> Vec<Shown> {
    let mut out = vec![Shown::Itself; inlines.len()];
    for s in spans(inlines) {
        // Outer fields come first: a field inside a part that isn't drawn isn't drawn either.
        let visible = out[s.begin] == Shown::Itself;
        let Inline::FieldBegin { kind, view, label, value, .. } = &inlines[s.begin] else { continue };
        let not = |out: &mut Vec<Shown>, r: std::ops::RangeInclusive<usize>| r.for_each(|k| out[k] = Shown::Not);
        if views && *view == FieldView::Code {
            not(&mut out, s.code_end()..=s.end);
            if visible {
                out[s.begin] = Shown::Value("{".into());
                out[s.end] = Shown::Value("}".into());
            } else {
                out[s.begin] = Shown::Not;
            }
            continue;
        }
        not(&mut out, s.begin..=s.code_end());
        out[s.end] = Shown::Not;
        if kind.is_some() {
            not(&mut out, s.code_end()..=s.end);
            if visible {
                out[s.begin] = Shown::Value(value.clone().unwrap_or_else(|| result_text(inlines, s)));
            }
        } else if let (true, true, Some(l)) = (views, visible, label)
            && s.sep.is_none_or(|sep| sep + 1 == s.end)
        {
            out[s.begin] = Shown::Value(l.to_string());
        }
    }
    out
}

/// The text of `inlines` as results show it: codes left out, an evaluated field as its value (or its result).
fn visible(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for (i, s) in inlines.iter().zip(shown(inlines)) {
        match (s, i) {
            (Shown::Value(v), _) => out.push_str(&v),
            (Shown::Itself, Inline::Text(t, _)) => out.push_str(t),
            (Shown::Itself, Inline::Tab(_) | Inline::PTab(..)) => out.push('\t'),
            _ => {}
        }
    }
    out
}

/// A new field's inlines: begin, its code, separator, its display text, end, all in `run`.
pub fn field_inlines(code: &str, display: &str, run: &Run) -> Vec<Inline> {
    field_inlines_with(FieldId::next(), code, display, run)
}

/// [`field_inlines`] with the field's id given.
pub fn field_inlines_with(id: FieldId, code: &str, display: &str, run: &Run) -> Vec<Inline> {
    let kind = parse(code).kind();
    let mut v = vec![Inline::FieldBegin { id, kind, lock: false, dirty: false, view: FieldView::Result, label: None, value: None, run: run.clone() }];
    if !code.is_empty() {
        v.push(Inline::Text(code.into(), run.clone()));
    }
    v.push(Inline::FieldSep { id, run: run.clone() });
    if !display.is_empty() {
        v.push(Inline::Text(display.into(), run.clone()));
    }
    v.push(Inline::FieldEnd { id, run: run.clone() });
    v
}

/// A field code's name, arguments and switches: Word's field syntax, quoted arguments keeping their
/// spaces and `\"` escapes. The fields nested in a code aren't in its text, so they aren't here either.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldCode {
    /// Upper-cased.
    pub name: String,
    pub args: Vec<String>,
    pub switches: Vec<Switch>,
}

/// A field switch: `\@ "M/d/yyyy"` is `@` with an argument; `\* MERGEFORMAT` is `*` with one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Switch {
    pub flag: char,
    pub arg: Option<String>,
}

/// Parses a field code. An empty code gives an empty name.
pub fn parse(code: &str) -> FieldCode {
    // Tokens: (text, quoted).
    let mut tokens: Vec<(String, bool)> = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = code.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => {
                if quoted {
                    tokens.push((std::mem::take(&mut cur), true));
                } else if !cur.is_empty() {
                    tokens.push((std::mem::take(&mut cur), false));
                }
                quoted = !quoted;
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    tokens.push((std::mem::take(&mut cur), false));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push((cur, quoted));
    }
    let mut it = tokens.into_iter().peekable();
    let name = it.next().map(|(t, _)| t.to_uppercase()).unwrap_or_default();
    let (mut args, mut switches) = (Vec::new(), Vec::new());
    while let Some((t, q)) = it.next() {
        let flag = (!q).then(|| t.strip_prefix('\\')).flatten().and_then(|f| f.chars().next());
        match flag {
            Some(flag) => {
                let rest: String = t.chars().skip(2).collect();
                // `\@"M/d"` and `\@ "M/d"` alike; a flag alone takes the next token unless it is a switch too.
                let arg = if !rest.is_empty() {
                    Some(rest)
                } else if it.peek().is_some_and(|(n, nq)| *nq || !n.starts_with('\\')) {
                    it.next().map(|(n, _)| n)
                } else {
                    None
                };
                switches.push(Switch { flag, arg });
            }
            None if switches.is_empty() => args.push(t),
            None => {}
        }
    }
    FieldCode { name, args, switches }
}

impl FieldCode {
    /// The kind layout evaluates (FIELD-1), if it is one.
    pub fn kind(&self) -> Option<FieldKind> {
        let arg = || self.args.first().cloned().unwrap_or_default();
        Some(match self.name.as_str() {
            "PAGE" => FieldKind::Page,
            "NUMPAGES" => FieldKind::NumPages,
            "SECTIONPAGES" => FieldKind::SectionPages,
            "SECTION" => FieldKind::Section,
            "PAGEREF" => FieldKind::PageRef(arg()),
            "REF" => FieldKind::Ref(arg()),
            "SEQ" => FieldKind::Seq(arg()),
            "STYLEREF" => FieldKind::StyleRef(arg()),
            "DATE" | "TIME" => FieldKind::Date,
            "IF" => FieldKind::If,
            _ => return None,
        })
    }

    /// What an empty result shows while designing: «first argument».
    pub fn label(&self) -> String {
        format!("«{}»", self.args.first().map_or(self.name.as_str(), String::as_str))
    }
}

/// The kind of the field beginning at `begin`, from its code.
pub fn kind_at(inlines: &[Inline], s: FieldSpan) -> Option<FieldKind> {
    parse(&code_text(inlines, s)).kind()
}

fn paras(blocks: &mut [Block], f: &mut dyn FnMut(&mut Paragraph)) {
    for b in blocks {
        match b {
            Block::Para(p) => f(p),
            Block::Table(t) => {
                for r in &mut t.rows {
                    for c in &mut r.cells {
                        paras(&mut c.blocks, f);
                    }
                }
            }
        }
    }
}

/// A paragraph's visible text (fields as their current results).
pub fn text(p: &Paragraph) -> String {
    visible(&p.inlines)
}

/// Style ids compare without case and spaces, so a STYLEREF's "Heading 1" finds `Heading1`.
pub fn style_key(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).collect()
}

/// Field-code words, quotes removed from quoted ones.
pub fn words(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in code.chars() {
        match c {
            '"' => {
                if quoted {
                    out.push(std::mem::take(&mut cur));
                }
                quoted = !quoted;
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn evaluate(doc: &mut Document) {
    // Bookmark texts (to the end of the bookmark's paragraph) and every paragraph's style and text, in order.
    let mut marks: HashMap<String, String> = HashMap::new();
    let mut order: Vec<(String, String)> = Vec::new();
    for s in &mut doc.sections {
        paras(&mut s.blocks, &mut |p| {
            for (k, i) in p.inlines.iter().enumerate() {
                if let Inline::Bookmark(name) = i {
                    let rest = Paragraph { inlines: p.inlines[k..].to_vec(), ..p.clone() };
                    marks.entry(name.clone()).or_insert_with(|| text(&rest));
                }
            }
            order.push((p.style.style_id.as_deref().map(style_key).unwrap_or_default(), text(p)));
        });
    }
    let mut seq: HashMap<String, i64> = HashMap::new();
    let mut at = 0;
    for s in &mut doc.sections {
        paras(&mut s.blocks, &mut |p| {
            for sp in spans(&p.inlines) {
                let code = code_text(&p.inlines, sp).trim().to_string();
                let Inline::FieldBegin { kind: Some(kind), value: slot, .. } = &mut p.inlines[sp.begin] else { continue };
                let code = &code;
                let w = words(code);
                let has = |sw: &str| w.iter().any(|x| x.eq_ignore_ascii_case(sw));
                let value = match kind {
                    FieldKind::Seq(id) => {
                        let n = seq.entry(id.clone()).or_insert(0);
                        if let Some(r) = w.iter().position(|x| x.eq_ignore_ascii_case("\\r")).and_then(|k| w.get(k + 1)?.parse().ok()) {
                            *n = r;
                        } else if !has("\\c") {
                            *n += 1;
                        }
                        if has("\\h") { Some(String::new()) } else { Some(format_field(&n.to_string(), code, None)) }
                    }
                    FieldKind::Ref(name) => marks.get(name).cloned(),
                    FieldKind::If => if_field(&w),
                    FieldKind::Date => Some(date(code)),
                    // In the body: the nearest paragraph with the style before the field, else the next one.
                    FieldKind::StyleRef(st) => {
                        let key = style_key(st);
                        order[..=at.min(order.len().saturating_sub(1))]
                            .iter()
                            .rev()
                            .chain(order[at.min(order.len())..].iter())
                            .find(|(k, _)| *k == key)
                            .map(|(_, t)| t.clone())
                    }
                    _ => None,
                };
                if let Some(v) = value {
                    *slot = Some(v);
                }
            }
            at += 1;
        });
    }
}

/// Whether the body or notes hold fields that need the page count or bookmark pages (a second layout pass).
pub fn page_dependent(doc: &Document) -> bool {
    doc.sections.iter().any(|s| blocks_page_dependent(&s.blocks))
        || doc.footnotes.values().chain(doc.endnotes.values()).any(|b| blocks_page_dependent(b))
}

/// [`page_dependent`] for some blocks.
pub fn blocks_page_dependent(blocks: &[Block]) -> bool {
    blocks.iter().any(|b| match b {
        Block::Para(p) => p
            .inlines
            .iter()
            .any(|i| matches!(i, Inline::FieldBegin { kind: Some(FieldKind::NumPages | FieldKind::SectionPages | FieldKind::PageRef(_)), .. })),
        Block::Table(t) => t.rows.iter().any(|r| r.cells.iter().any(|c| blocks_page_dependent(&c.blocks))),
    })
}

/// `IF a op b "then" "else"` with literal operands: numbers compare numerically, anything else as text.
fn if_field(w: &[String]) -> Option<String> {
    let [_, a, op, b, yes, rest @ ..] = w else { return None };
    let no = rest.first().cloned().unwrap_or_default();
    let ord = match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y)?,
        _ => a.cmp(b),
    };
    use std::cmp::Ordering::*;
    let hit = match op.as_str() {
        "=" => ord == Equal,
        "<>" => ord != Equal,
        "<" => ord == Less,
        ">" => ord == Greater,
        "<=" => ord != Greater,
        ">=" => ord != Less,
        _ => return None,
    };
    Some(if hit { yes.clone() } else { no })
}

/// `DATE`/`TIME` with a `\@` picture (default M/d/yyyy), from the system clock in UTC.
// shortcut: UTC, no time zone; English month/day names.
fn date(code: &str) -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, sod) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Civil date from days since 1970-01-01 (H. Hinnant).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let wd = (days + 4).rem_euclid(7) as usize; // 1970-01-01 was a Thursday
    let (hh, mi, ss) = (sod / 3600, sod / 60 % 60, sod % 60);
    const MONTHS: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    let pic = code.split("\\@").nth(1).map(|p| words(p).into_iter().next().unwrap_or_default()).unwrap_or_else(|| "M/d/yyyy".into());
    let mut out = String::new();
    let chars: Vec<char> = pic.chars().collect();
    let mut k = 0;
    while k < chars.len() {
        let c = chars[k];
        let n = chars[k..].iter().take_while(|&&x| x == c).count();
        let s = match (c, n) {
            ('y', 4..) => format!("{y:04}"),
            ('y', _) => format!("{:02}", y % 100),
            ('M', 4..) => MONTHS[(m - 1) as usize].to_string(),
            ('M', 3) => MONTHS[(m - 1) as usize][..3].to_string(),
            ('M', 2) => format!("{m:02}"),
            ('M', _) => m.to_string(),
            ('d', 4..) => DAYS[wd].to_string(),
            ('d', 3) => DAYS[wd][..3].to_string(),
            ('d', 2) => format!("{d:02}"),
            ('d', _) => d.to_string(),
            ('H', 2..) => format!("{hh:02}"),
            ('H', _) => hh.to_string(),
            ('h', 2..) => format!("{:02}", (hh + 11) % 12 + 1),
            ('h', _) => ((hh + 11) % 12 + 1).to_string(),
            ('m', 2..) => format!("{mi:02}"),
            ('m', _) => mi.to_string(),
            ('s', 2..) => format!("{ss:02}"),
            ('s', _) => ss.to_string(),
            _ => c.to_string().repeat(n),
        };
        out.push_str(&s);
        k += n;
    }
    out
}

/// A number field's result with its switches (FIELD-3). `value` is the plain number; `default` is the format without a
/// number switch (a section's `pgNumType/@w:fmt` for PAGE). Case switches apply only after the number format switch.
pub(crate) fn format_field(value: &str, code: &str, default: Option<&str>) -> String {
    let Ok(n) = value.parse::<i64>() else { return value.to_string() };
    let mut fmt: Option<&str> = None;
    let mut case: Option<String> = None;
    for sw in code.split("\\*").skip(1) {
        let w = sw.split_whitespace().next().unwrap_or("");
        let f = match w {
            "Arabic" | "arabic" => "decimal",
            "roman" => "lowerRoman",
            "ROMAN" | "Roman" => "upperRoman",
            "alphabetic" => "lowerLetter",
            "ALPHABETIC" | "Alphabetic" => "upperLetter",
            "ArabicDash" => "numberInDash",
            "CardText" | "cardtext" => "cardinalTextLower",
            "OrdText" | "ordtext" => "ordinalTextLower",
            "Ordinal" | "ordinal" => "ordinal",
            "Hex" | "hex" => "hex",
            "Caps" | "FirstCap" | "Upper" | "Lower" | "caps" | "upper" | "lower" => {
                if fmt.is_some() {
                    case = Some(w.to_lowercase());
                }
                continue;
            }
            // MERGEFORMAT, CHARFORMAT: formatting only.
            _ => continue,
        };
        fmt.get_or_insert(f);
    }
    if let Some(pic) = code.split("\\#").nth(1) {
        let pic = pic.trim().trim_matches('"');
        let zeros = pic.chars().filter(|c| *c == '0').count();
        return format!("{n:0zeros$}");
    }
    let text = match fmt.or(default) {
        Some("cardinalTextLower") => crate::docx::numbering::cardinal(n),
        Some("ordinalTextLower") => crate::docx::numbering::ordinal_text(n),
        Some(f) => crate::docx::numbering::format_number(n, f),
        None => n.to_string(),
    };
    match case.as_deref() {
        Some("upper") => text.to_uppercase(),
        Some("lower") => text.to_lowercase(),
        Some("caps") => text
            .split(' ')
            .map(|w| {
                let mut c = w.chars();
                c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
            })
            .collect::<Vec<_>>()
            .join(" "),
        Some("firstcap") => {
            let mut c = text.chars();
            c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
        }
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn if_and_words() {
        assert_eq!(words(r#"IF 1 = 1 "yes" "no""#), ["IF", "1", "=", "1", "yes", "no"]);
        assert_eq!(if_field(&words(r#"IF 1 = 1 "yes" "no""#)).as_deref(), Some("yes"));
        assert_eq!(if_field(&words(r#"IF 2 < 1 "yes" "no""#)).as_deref(), Some("no"));
        assert_eq!(date(r#"DATE \@ "yyyy""#).len(), 4);
    }

    #[test]
    fn parses_field_codes() {
        let c = parse(r#" mergefield Statement.Cost_Amount \# "$,0.00" \* MERGEFORMAT "#);
        assert_eq!(c.name, "MERGEFIELD");
        assert_eq!(c.args, ["Statement.Cost_Amount"]);
        assert_eq!(c.switches, [Switch { flag: '#', arg: Some("$,0.00".into()) }, Switch { flag: '*', arg: Some("MERGEFORMAT".into()) }]);
        assert_eq!(c.kind(), None);
        assert_eq!(c.label(), "«Statement.Cost_Amount»");
        let d = parse(r#"DATE \@"M/d/yyyy" \! "#);
        assert_eq!(d.switches, [Switch { flag: '@', arg: Some("M/d/yyyy".into()) }, Switch { flag: '!', arg: None }]);
        assert_eq!(d.kind(), Some(FieldKind::Date));
        let q = parse(r#"IF "a b" = "say \"hi\"" "x""#);
        assert_eq!(q.args, ["a b", "=", "say \"hi\"", "x"]);
        assert_eq!(q.kind(), Some(FieldKind::If));
        for (code, kind) in [
            (" PAGE ", Some(FieldKind::Page)),
            ("NUMPAGES", Some(FieldKind::NumPages)),
            ("SECTIONPAGES", Some(FieldKind::SectionPages)),
            ("SECTION", Some(FieldKind::Section)),
            ("PAGEREF bm \\h", Some(FieldKind::PageRef("bm".into()))),
            ("REF bm", Some(FieldKind::Ref("bm".into()))),
            ("SEQ Figure", Some(FieldKind::Seq("Figure".into()))),
            ("STYLEREF \"Heading 1\"", Some(FieldKind::StyleRef("Heading 1".into()))),
            ("TIME", Some(FieldKind::Date)),
            ("QUOTE x", None),
        ] {
            assert_eq!(parse(code).kind(), kind, "{code}");
        }
        assert_eq!(parse("").name, "");
        assert_eq!(parse("   ").args, Vec::<String>::new());
        assert_eq!(parse("\\").switches, Vec::<Switch>::new());
    }

    #[test]
    fn spans_and_texts() {
        let doc = crate::docx::read(&crate::docx::blank::blank_docx()).expect("the blank document reads");
        let Some(Block::Para(p)) = doc.sections[0].blocks.first() else { panic!("a paragraph") };
        let run = p.mark.clone();
        let b = |id| Inline::FieldBegin {
            id: FieldId(id),
            kind: None,
            lock: false,
            dirty: false,
            view: FieldView::Result,
            label: None,
            value: None,
            run: run.clone(),
        };
        let t = |s: &str| Inline::Text(s.into(), run.clone());
        let sep = |id| Inline::FieldSep { id: FieldId(id), run: run.clone() };
        let end = |id| Inline::FieldEnd { id: FieldId(id), run: run.clone() };
        // "x {IF {MERGEFIELD A} = "1" "y" "n"}y z"
        let v = vec![
            t("x "),
            b(1),
            t(" IF "),
            b(2),
            t(" MERGEFIELD A "),
            sep(2),
            t("«A»"),
            end(2),
            t(r#" = "1" "y" "n" "#),
            sep(1),
            t("y"),
            end(1),
            t(" z"),
        ];
        let s = spans(&v);
        assert_eq!(s, [FieldSpan { begin: 1, sep: Some(9), end: 11 }, FieldSpan { begin: 3, sep: Some(5), end: 7 }]);
        assert_eq!(code_text(&v, s[0]), r#" IF  = "1" "y" "n" "#);
        assert_eq!(code_text(&v, s[1]), " MERGEFIELD A ");
        assert_eq!(result_text(&v, s[0]), "y");
        assert_eq!(visible(&v), "x y z");
    }
}
