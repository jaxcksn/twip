//! HTML paste: paragraphs, headings, lists, tables, links and character formatting from another application's HTML

use crate::docx::write::props::{Cx, Props};
use crate::docx::{Block, Inline, Paragraph};
use crate::font::Fonts;

use super::Editor;
use super::clipboard::Fragment;
use super::commands::Outcome;

/// The most HTML read; more pastes as text.
pub(super) const MAX_HTML: usize = 8 << 20;
/// The most elements open at once; deeper ones format like their parent.
const MAX_OPEN: usize = 256;
/// The deepest table nesting kept; deeper tables read as their cells' paragraphs.
const MAX_TABLES: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Fmt {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    sup: bool,
    sub: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct Run {
    text: String,
    fmt: Fmt,
    link: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Body,
    Heading(u8),
    /// A list item: its list (by order of the outermost lists), numbered, and its level.
    Item {
        list: usize,
        numbered: bool,
        level: u8,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct Para {
    kind: Kind,
    align: Option<&'static str>,
    runs: Vec<Run>,
}

#[derive(Clone, Debug, PartialEq)]
enum HBlock {
    Para(Para),
    /// Rows of cells, each with its column span.
    Table(Vec<Vec<(usize, Vec<HBlock>)>>),
}

/// An element open while reading.
struct Open {
    name: String,
    fmt: Fmt,
    link: Option<String>,
    align: Option<&'static str>,
}

struct Reader {
    out: Vec<HBlock>,
    /// The tables being read, innermost last: rows, and whether a cell is open.
    tables: Vec<(Vec<Vec<(usize, Vec<HBlock>)>>, bool)>,
    open: Vec<Open>,
    /// The lists open: numbered, and the list they are in.
    lists: Vec<(bool, usize)>,
    next_list: usize,
    para: Option<Para>,
    space: bool,
    pre: usize,
    skip: usize,
}

fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        "ndash" => Some('\u{2013}'),
        "mdash" => Some('\u{2014}'),
        "lsquo" => Some('\u{2018}'),
        "rsquo" => Some('\u{2019}'),
        "ldquo" => Some('\u{201C}'),
        "rdquo" => Some('\u{201D}'),
        "hellip" => Some('\u{2026}'),
        "bull" => Some('\u{2022}'),
        "copy" => Some('\u{a9}'),
        "reg" => Some('\u{ae}'),
        "trade" => Some('\u{2122}'),
        n if n.starts_with("#x") || n.starts_with("#X") => u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32),
        n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
        _ => None,
    }
}

/// Text with its character references decoded.
fn decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(k) = rest.find('&') {
        out.push_str(&rest[..k]);
        rest = &rest[k..];
        let end = rest[1..].find(|c: char| c == ';' || c == '&' || c.is_whitespace()).map(|e| e + 1).filter(|&e| e <= 32);
        match end.filter(|&e| rest.as_bytes().get(e) == Some(&b';')).and_then(|e| entity(&rest[1..e]).map(|c| (c, e))) {
            Some((c, e)) => {
                out.push(c);
                rest = &rest[e + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A start tag's attribute, decoded.
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(k) = lower[from..].find(name).map(|k| k + from) {
        from = k + name.len();
        let before = lower[..k].chars().next_back();
        let after = lower[from..].trim_start();
        if !before.is_some_and(char::is_whitespace) || !after.starts_with('=') {
            continue;
        }
        let v = after[1..].trim_start();
        let at = tag.len() - v.len();
        let raw = &tag[at..];
        let value = match raw.chars().next() {
            Some(q @ ('"' | '\'')) => raw[1..].split(q).next().unwrap_or(""),
            _ => raw.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or(""),
        };
        return Some(decode(value));
    }
    None
}

/// A CSS declaration's value in a `style` attribute, lower case.
fn css(style: &str, property: &str) -> Option<String> {
    style.split(';').find_map(|d| {
        let (k, v) = d.split_once(':')?;
        (k.trim().eq_ignore_ascii_case(property)).then(|| v.trim().to_ascii_lowercase())
    })
}

impl Reader {
    fn fmt(&self) -> (Fmt, Option<String>, Option<&'static str>) {
        self.open.last().map_or((Fmt::default(), None, None), |o| (o.fmt, o.link.clone(), o.align))
    }

    /// Where blocks go now: the open cell, or the document.
    fn target(&mut self) -> &mut Vec<HBlock> {
        match self.tables.last_mut() {
            Some((rows, true)) => match rows.last_mut().and_then(|r| r.last_mut()) {
                Some((_, blocks)) => blocks,
                None => &mut self.out,
            },
            _ => &mut self.out,
        }
    }

    fn end_para(&mut self) {
        if let Some(mut p) = self.para.take() {
            if let Some(last) = p.runs.last_mut() {
                let trimmed = last.text.trim_end_matches(' ').len();
                last.text.truncate(trimmed);
            }
            p.runs.retain(|r| !r.text.is_empty());
            if !p.runs.is_empty() || p.kind != Kind::Body {
                self.target().push(HBlock::Para(p));
            }
        }
        self.space = true;
    }

    fn start_para(&mut self, kind: Kind) {
        self.end_para();
        let align = self.fmt().2;
        self.para = Some(Para { kind, align, runs: Vec::new() });
    }

    fn text(&mut self, raw: &str) {
        if self.skip > 0 || raw.is_empty() {
            return;
        }
        let decoded = decode(raw);
        let mut t = String::with_capacity(decoded.len());
        for c in decoded.chars() {
            if self.pre > 0 && c == '\n' {
                t.push('\n');
            } else if c.is_whitespace() && c != '\u{a0}' && self.pre == 0 {
                if !self.space {
                    t.push(' ');
                }
                self.space = true;
            } else {
                t.push(c);
                self.space = false;
            }
        }
        if t.is_empty() || t == " " && self.para.is_none() {
            return;
        }
        if self.para.is_none() {
            self.para = Some(Para { kind: Kind::Body, align: self.fmt().2, runs: Vec::new() });
        }
        let (fmt, link, _) = self.fmt();
        let Some(p) = self.para.as_mut() else { return };
        match p.runs.last_mut() {
            Some(r) if r.fmt == fmt && r.link == link => r.text.push_str(&t),
            _ => p.runs.push(Run { text: t, fmt, link }),
        }
    }

    fn start(&mut self, name: &str, tag: &str) {
        if matches!(name, "head" | "style" | "script" | "title" | "template") {
            self.skip += 1;
        }
        let (mut fmt, mut link, mut align) = self.fmt();
        let style = attr(tag, "style").unwrap_or_default();
        match name {
            "b" | "strong" => fmt.bold = true,
            "i" | "em" | "cite" | "var" => fmt.italic = true,
            "u" | "ins" => fmt.underline = true,
            "s" | "strike" | "del" => fmt.strike = true,
            "sup" => fmt.sup = true,
            "sub" => fmt.sub = true,
            "a" => link = attr(tag, "href").filter(|h| !h.trim().is_empty() && !h.trim_start().to_ascii_lowercase().starts_with("javascript:")),
            _ => {}
        }
        if let Some(w) = css(&style, "font-weight") {
            fmt.bold = w == "bold" || w == "bolder" || w.parse::<u32>().is_ok_and(|n| n >= 600);
        }
        if let Some(s) = css(&style, "font-style") {
            fmt.italic = s == "italic" || s == "oblique";
        }
        if let Some(d) = css(&style, "text-decoration").or_else(|| css(&style, "text-decoration-line")) {
            fmt.underline |= d.contains("underline");
            fmt.strike |= d.contains("line-through");
        }
        if let Some(a) = css(&style, "text-align").or_else(|| attr(tag, "align").map(|a| a.to_ascii_lowercase())) {
            align = match a.as_str() {
                "center" => Some("center"),
                "right" | "end" => Some("right"),
                "justify" => Some("both"),
                _ => None,
            };
        }
        match name {
            "br" => {
                self.text("\u{2028}");
                self.space = true;
                return;
            }
            "p" | "div" | "blockquote" | "pre" | "address" | "section" | "article" | "header" | "footer" | "dt" | "dd" | "figcaption" => {
                self.end_para();
            }
            "hr" => {
                self.end_para();
                return;
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = name.as_bytes()[1] - b'0';
                self.push_open(name, fmt, link, align);
                self.start_para(Kind::Heading(level));
                return;
            }
            "ul" | "ol" => {
                self.end_para();
                let list = match self.lists.last() {
                    Some(&(_, l)) => l,
                    None => {
                        self.next_list += 1;
                        self.next_list - 1
                    }
                };
                self.lists.push((name == "ol", list));
            }
            "li" => {
                let (numbered, list) = self.lists.last().copied().unwrap_or((false, usize::MAX));
                let level = self.lists.len().saturating_sub(1).min(8) as u8;
                self.push_open(name, fmt, link, align);
                self.start_para(if list == usize::MAX { Kind::Body } else { Kind::Item { list, numbered, level } });
                return;
            }
            "table" => {
                self.end_para();
                if self.tables.len() < MAX_TABLES {
                    self.tables.push((Vec::new(), false));
                }
            }
            "tr" => {
                self.end_para();
                if let Some((rows, open)) = self.tables.last_mut() {
                    rows.push(Vec::new());
                    *open = false;
                }
            }
            "td" | "th" => {
                self.end_para();
                if name == "th" {
                    fmt.bold = true;
                }
                let span = attr(tag, "colspan").and_then(|s| s.trim().parse::<usize>().ok()).unwrap_or(1).clamp(1, 63);
                if let Some((rows, open)) = self.tables.last_mut() {
                    if rows.is_empty() {
                        rows.push(Vec::new());
                    }
                    if let Some(r) = rows.last_mut() {
                        r.push((span, Vec::new()));
                    }
                    *open = true;
                }
            }
            _ => {}
        }
        if name == "pre" {
            self.pre += 1;
        }
        self.push_open(name, fmt, link, align);
    }

    fn push_open(&mut self, name: &str, fmt: Fmt, link: Option<String>, align: Option<&'static str>) {
        if self.open.len() < MAX_OPEN {
            self.open.push(Open { name: name.to_string(), fmt, link, align });
        }
    }

    fn end(&mut self, name: &str) {
        if matches!(name, "head" | "style" | "script" | "title" | "template") {
            self.skip = self.skip.saturating_sub(1);
        }
        if let Some(k) = self.open.iter().rposition(|o| o.name == name) {
            self.open.truncate(k);
        }
        match name {
            "p" | "div" | "blockquote" | "address" | "section" | "article" | "header" | "footer" | "dt" | "dd" | "figcaption" | "li" | "h1"
            | "h2" | "h3" | "h4" | "h5" | "h6" => self.end_para(),
            "pre" => {
                self.end_para();
                self.pre = self.pre.saturating_sub(1);
            }
            "ul" | "ol" => {
                self.end_para();
                self.lists.pop();
            }
            "td" | "th" => {
                self.end_para();
                if let Some((_, open)) = self.tables.last_mut() {
                    *open = false;
                }
            }
            "table" => {
                self.end_para();
                if let Some((rows, _)) = self.tables.pop() {
                    let rows: Vec<_> = rows.into_iter().filter(|r| !r.is_empty()).collect();
                    if !rows.is_empty() {
                        self.target().push(HBlock::Table(rows));
                    }
                }
            }
            _ => {}
        }
    }
}

/// The blocks of an HTML clipboard fragment (only what its `StartFragment` and `EndFragment` comments mark, when it
/// has them).
fn read(html: &str) -> Vec<HBlock> {
    let html = match (html.find("<!--StartFragment-->"), html.find("<!--EndFragment-->")) {
        (Some(a), Some(b)) if a < b => &html[a + "<!--StartFragment-->".len()..b],
        _ => html,
    };
    let mut r =
        Reader { out: Vec::new(), tables: Vec::new(), open: Vec::new(), lists: Vec::new(), next_list: 0, para: None, space: true, pre: 0, skip: 0 };
    let mut rest = html;
    while !rest.is_empty() {
        let Some(k) = rest.find('<') else {
            r.text(rest);
            break;
        };
        r.text(&rest[..k]);
        rest = &rest[k..];
        if let Some(body) = rest.strip_prefix("<!--") {
            rest = body.find("-->").map_or("", |e| &body[e + 3..]);
            continue;
        }
        let Some(e) = rest.find('>') else { break };
        let tag = &rest[1..e];
        rest = &rest[e + 1..];
        if tag.starts_with('!') || tag.starts_with('?') {
            continue;
        }
        let closing = tag.starts_with('/');
        let name: String =
            tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric() || *c == ':').collect::<String>().to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        if closing {
            r.end(&name);
        } else {
            r.start(&name, tag);
            if tag.ends_with('/') && name != "br" {
                r.end(&name);
            }
        }
    }
    r.end_para();
    while let Some((rows, _)) = r.tables.pop() {
        let rows: Vec<_> = rows.into_iter().filter(|r| !r.is_empty()).collect();
        if !rows.is_empty() {
            r.target().push(HBlock::Table(rows));
        }
    }
    r.out
}

/// The lists `blocks` use, each with whether it is numbered.
fn lists(blocks: &[HBlock], out: &mut Vec<(usize, bool)>) {
    for b in blocks {
        match b {
            HBlock::Para(Para { kind: Kind::Item { list, numbered, level: 0 }, .. }) if !out.iter().any(|(l, _)| l == list) => {
                out.push((*list, *numbered))
            }
            HBlock::Para(_) => {}
            HBlock::Table(rows) => rows.iter().flatten().for_each(|(_, b)| lists(b, out)),
        }
    }
}

impl Editor {
    /// Pastes HTML from another application as one transaction: its paragraphs, headings (in the Heading styles the
    /// document has), lists (as new lists), tables, links and bold, italic, underline, strikethrough, superscript and
    /// subscript. `Unchanged` when it holds no text, for a paste of the plain text instead.
    pub fn paste_html(&mut self, html: &str, fonts: &Fonts) -> Outcome {
        if html.len() > MAX_HTML {
            return Outcome::Unchanged("too much HTML");
        }
        let read = read(html);
        if read.is_empty() {
            return Outcome::Unchanged("no HTML content");
        }
        let Some(caret) = self.selection.ordered(&self.index).map(|(a, _)| a) else { return Outcome::Unchanged("no caret") };
        let Some(template) = caret.para_offset().and_then(|(p, _)| self.index.para(&self.doc, p)).cloned() else {
            return Outcome::Unchanged("no paragraph");
        };
        let width = self.index.para_slot(crate::layout::ParaId(template.id)).map_or(468.0, |s| self.text_width(s.container));
        let mut used = Vec::new();
        lists(&read, &mut used);
        self.begin_batch(Some("Paste".into()));
        let out = 'paste: {
            let mut nums = Vec::new();
            if !used.is_empty() {
                let mut tx = self.begin("Paste");
                nums = used.iter().map(|&(list, numbered)| (list, self.new_list(&mut tx, !numbered))).collect();
                self.commit(tx, None, fonts);
            }
            let Some(res) = self.doc.resolver.clone() else { break 'paste Outcome::Unchanged("no styles") };
            let cx = self.cx();
            let blocks = self.html_blocks(&read, &res, &cx, &nums, &template, width, 0);
            let f = Fragment { id: 0, blocks, open_end: true, ..Default::default() };
            self.paste_fragment(&f, fonts)
        };
        // The new lists go with a paste that pasted nothing.
        let end = self.end_batch(matches!(out, Outcome::Changed(_)), fonts);
        if matches!(end, Outcome::Changed(_)) { end } else { out }
    }

    #[allow(clippy::too_many_arguments)]
    fn html_blocks(
        &self,
        blocks: &[HBlock],
        res: &crate::docx::resolve::Resolver,
        cx: &Cx,
        nums: &[(usize, i64)],
        template: &Paragraph,
        width: f64,
        depth: usize,
    ) -> Vec<Block> {
        let w = &cx.w;
        let mut out = Vec::new();
        for b in blocks {
            match b {
                HBlock::Para(p) => {
                    let mut pp = Props::parse(None, "pPr", cx);
                    match p.kind {
                        Kind::Heading(n) if res.styles.by_id.contains_key(&format!("Heading{n}")) => {
                            pp.set("pStyle", format!("<{w}pStyle {w}val=\"Heading{n}\"/>"));
                        }
                        Kind::Item { list, level, .. } => {
                            if let Some(&(_, num)) = nums.iter().find(|(l, _)| *l == list) {
                                pp.set("numPr", format!("<{w}numPr><{w}ilvl {w}val=\"{level}\"/><{w}numId {w}val=\"{num}\"/></{w}numPr>"));
                            }
                        }
                        _ => {}
                    }
                    if let Some(jc) = p.align {
                        pp.set("jc", format!("<{w}jc {w}val=\"{jc}\"/>"));
                    }
                    let para = res.para_from_xml(pp.xml().as_deref(), None, cx);
                    let mark = res.run_from_xml(&para.base, None, cx);
                    let mut inlines = Vec::new();
                    for r in &p.runs {
                        let mut rp = Props::parse(None, "rPr", cx);
                        let f = r.fmt;
                        for (on, name, xml) in [
                            (f.bold, "b", format!("<{w}b/>")),
                            (f.italic, "i", format!("<{w}i/>")),
                            (f.underline || r.link.is_some(), "u", format!("<{w}u {w}val=\"single\"/>")),
                            (f.strike, "strike", format!("<{w}strike/>")),
                            (f.sup, "vertAlign", format!("<{w}vertAlign {w}val=\"superscript\"/>")),
                            (f.sub && !f.sup, "vertAlign", format!("<{w}vertAlign {w}val=\"subscript\"/>")),
                        ] {
                            if on {
                                rp.set(name, xml);
                            }
                        }
                        if r.link.is_some() && res.styles.by_id.contains_key("Hyperlink") {
                            rp.remove("u");
                            rp.set("rStyle", format!("<{w}rStyle {w}val=\"Hyperlink\"/>"));
                        }
                        let run = res.run_from_xml(&para.base, rp.xml().as_deref(), cx);
                        match &r.link {
                            Some(address) => {
                                let code = format!(" HYPERLINK \"{}\" ", address.replace('"', ""));
                                inlines.extend(crate::docx::fields::field_inlines(&code, &r.text.replace('\u{2028}', " "), &run));
                            }
                            None => {
                                for (k, line) in r.text.split(['\u{2028}', '\n']).enumerate() {
                                    if k > 0 {
                                        inlines.push(Inline::Break(crate::docx::BreakKind::Line, run.clone()));
                                    }
                                    if !line.is_empty() {
                                        inlines.push(Inline::Text(line.to_string(), run.clone()));
                                    }
                                }
                            }
                        }
                    }
                    if inlines.is_empty() {
                        inlines.push(Inline::Text(String::new(), mark.clone()));
                    }
                    out.push(Block::Para(Paragraph {
                        style: std::rc::Rc::new(para.style),
                        mark,
                        list: None,
                        inlines,
                        src: None,
                        id: crate::docx::next_para_id(),
                        div: None,
                        controls: Vec::new(),
                    }));
                }
                HBlock::Table(rows) if depth < MAX_TABLES => {
                    let ncols = rows.iter().map(|r| r.iter().map(|c| c.0).sum::<usize>()).max().unwrap_or(1).clamp(1, super::tables::MAX_COLUMNS);
                    let empty = (0..rows.len().min(super::tables::MAX_ROWS)).map(|_| Vec::new()).collect();
                    let mut t = self.new_table(empty, ncols, width, template);
                    let col = t.grid.first().copied().unwrap_or(width);
                    for (row, cells) in t.rows.iter_mut().zip(rows) {
                        let mut kept = Vec::new();
                        let mut at = 0;
                        for (span, content) in cells {
                            if at >= ncols {
                                break;
                            }
                            let span = (*span).min(ncols - at);
                            let Some(mut cell) = row.cells.first().cloned() else { break };
                            cell.span = span;
                            cell.width = Some(crate::docx::props::Width::Dxa(col * span as f64));
                            let blocks = self.html_blocks(content, res, cx, nums, template, col * span as f64, depth + 1);
                            if !blocks.is_empty() {
                                cell.blocks = blocks;
                            }
                            kept.push(cell);
                            at += span;
                        }
                        while at < ncols {
                            if let Some(c) = row.cells.first().cloned() {
                                kept.push(c);
                            }
                            at += 1;
                        }
                        row.cells = kept;
                    }
                    out.push(Block::Table(t));
                }
                HBlock::Table(rows) => {
                    for (_, content) in rows.iter().flatten() {
                        out.extend(self.html_blocks(content, res, cx, nums, template, width, depth));
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_paragraphs_formatting_lists_and_tables() {
        let html = "<html><head><style>p{}</style></head><body><!--StartFragment--><h1>Title</h1><p>a <b>bold</b> <a href=\"https://x.test\">link</a></p><ul><li>one</li><li>two<ol><li>deep</li></ol></li></ul><table><tr><td>c1</td><td colspan=2>c2</td></tr></table><!--EndFragment--></body></html>";
        let b = read(html);
        assert!(matches!(&b[0], HBlock::Para(Para { kind: Kind::Heading(1), .. })), "{b:?}");
        let HBlock::Para(p) = &b[1] else { panic!("{b:?}") };
        assert_eq!(
            p.runs.iter().map(|r| (r.text.as_str(), r.fmt.bold, r.link.is_some())).collect::<Vec<_>>(),
            [("a ", false, false), ("bold", true, false), (" ", false, false), ("link", false, true)]
        );
        assert!(matches!(&b[2], HBlock::Para(Para { kind: Kind::Item { level: 0, numbered: false, .. }, .. })));
        assert!(matches!(&b[4], HBlock::Para(Para { kind: Kind::Item { level: 1, numbered: true, .. }, .. })), "{b:?}");
        let HBlock::Table(rows) = &b[5] else { panic!("{b:?}") };
        assert_eq!(rows[0].iter().map(|c| c.0).collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn decodes_references_and_skips_scripts() {
        let b = read("<p>a&amp;b &#x41;&#66; &bogus; x</p><script>alert(1)</script>");
        let HBlock::Para(p) = &b[0] else { panic!("{b:?}") };
        assert_eq!(p.runs[0].text, "a&b AB &bogus; x");
        assert_eq!(b.len(), 1);
    }
}
