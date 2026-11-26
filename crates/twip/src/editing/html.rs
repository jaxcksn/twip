//! HTML paste: paragraphs, headings, lists, tables, links and character formatting from another application's HTML

use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::resolvers::Resolvers;
use crate::docx::{AltContent, Block, Inline, Paragraph, VMerge};
use crate::font::Fonts;

use super::Editor;
use super::clipboard::Fragment;
use super::commands::Outcome;
use crate::document::format::Jc;
use crate::document::paragraph::ListId;
use crate::docx::char_props::RunDirect;
use crate::docx::para_props::ParaDirect;
use crate::docx::run_xml;
use crate::format::{CharProps, ListChoice, ParaProps};

/// The most HTML read; more pastes as text.
pub(super) const MAX_HTML: usize = 8 << 20;
/// The most elements open at once; deeper ones format like their parent.
const MAX_OPEN: usize = 256;
/// The deepest table nesting kept; deeper tables read as their cells' paragraphs.
const MAX_TABLES: usize = 8;
/// The largest side of a pasted picture, points: Word's, 22 inches.
const MAX_PICTURE_SIDE: f64 = 1584.0;
/// The most table cells a paste makes, nested tables' included; more pastes as text.
const MAX_CELLS: usize = 1 << 17;

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
    /// A picture in place of text.
    image: Option<Image>,
}

/// An `<img>` whose source is the picture itself (a `data:` URL): its bytes, and the width and height it is given,
/// CSS px.
#[derive(Clone, Debug, PartialEq)]
struct Image {
    data: Vec<u8>,
    w: Option<f64>,
    h: Option<f64>,
    alt: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Body,
    Heading(u8),
    /// A list item: its list (by order of the lists read; a list nested in one of the other kind is a list of its
    /// own), numbered, and its level.
    Item {
        list: usize,
        numbered: bool,
        level: u8,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct Para {
    kind: Kind,
    align: Option<Jc>,
    runs: Vec<Run>,
}

#[derive(Clone, Debug, PartialEq)]
enum HBlock {
    Para(Para),
    /// A table on its grid: its columns and its rows' cells.
    Table {
        cols: usize,
        rows: Vec<Vec<Slot>>,
    },
}

/// A cell as read: its column span, the rows it spans (0: to the table's end) and its content.
#[derive(Clone, Debug, PartialEq)]
struct HCell {
    span: usize,
    rows: usize,
    blocks: Vec<HBlock>,
}

/// A cell on a table's grid: its column, span, vertical merge and content (none where a cell above spans the row).
#[derive(Clone, Debug, PartialEq)]
struct Slot {
    col: usize,
    span: usize,
    merge: VMerge,
    blocks: Vec<HBlock>,
}

/// An element open while reading.
struct Open {
    name: String,
    fmt: Fmt,
    link: Option<String>,
    align: Option<Jc>,
}

struct Reader {
    out: Vec<HBlock>,
    /// The tables being read, innermost last: rows, and whether a cell is open.
    tables: Vec<(Vec<Vec<HCell>>, bool)>,
    /// Tables open past [`MAX_TABLES`], read as their cells' paragraphs.
    deep: usize,
    open: Vec<Open>,
    /// Elements open past [`MAX_OPEN`], by name: they are inside every element `open` has.
    over: HashMap<String, usize>,
    /// The lists open: numbered, and the list they are in.
    lists: Vec<(bool, usize)>,
    next_list: usize,
    para: Option<Para>,
    space: bool,
    pre: usize,
    skip: usize,
    /// What was read besides structure: text (a picture's alternative text not counted), pictures drawn, and
    /// pictures that aren't (remote ones).
    seen: Seen,
}

/// What a read of HTML met.
#[derive(Clone, Copy, Debug, Default)]
struct Seen {
    text: bool,
    drawn: bool,
    remote: bool,
    table: bool,
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

/// A start tag's attribute `name` (lower case), decoded; empty when it has no value.
fn attr(tag: &str, name: &str) -> Option<String> {
    let b = tag.as_bytes();
    let space = |i: usize| b.get(i).is_some_and(u8::is_ascii_whitespace);
    let mut i = b.iter().position(|c| c.is_ascii_whitespace() || *c == b'/').unwrap_or(b.len());
    while i < b.len() {
        let from = i;
        while space(i) || b.get(i) == Some(&b'/') {
            i += 1;
        }
        let start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && !matches!(b[i], b'=' | b'/' | b'>') {
            i += 1;
        }
        let key = &tag[start..i];
        while space(i) {
            i += 1;
        }
        let mut value = "";
        if b.get(i) == Some(&b'=') {
            i += 1;
            while space(i) {
                i += 1;
            }
            match b.get(i) {
                Some(&q @ (b'"' | b'\'')) => {
                    let s = i + 1;
                    let e = tag[s..].find(char::from(q)).map_or(b.len(), |e| s + e);
                    value = &tag[s..e];
                    i = (e + 1).min(b.len());
                }
                _ => {
                    let s = i;
                    while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                        i += 1;
                    }
                    value = &tag[s..i];
                }
            }
        }
        if !key.is_empty() && key.eq_ignore_ascii_case(name) {
            return Some(decode(value));
        }
        if i == from {
            i += 1;
        }
    }
    None
}

/// Where the tag `rest` starts with ends: its `>`, past those in quoted attribute values (the first `>` when a quote
/// isn't closed).
fn tag_end(rest: &str) -> Option<usize> {
    let mut quote = None;
    let mut after_eq = false;
    for (i, &c) in rest.as_bytes().iter().enumerate().skip(1) {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            b'>' => return Some(i),
            b'"' | b'\'' if after_eq => quote = Some(c),
            _ => {}
        }
        if !c.is_ascii_whitespace() {
            after_eq = c == b'=';
        }
    }
    rest.find('>')
}

/// A link's address when it is one to follow: without the tabs and line breaks URL parsers drop, and with a web,
/// mail, FTP or phone scheme, or none (a relative address).
pub(super) fn web_address(href: &str) -> Option<String> {
    let a: String = href.chars().filter(|c| !matches!(c, '\t' | '\n' | '\r')).collect();
    let a = a.trim_matches(|c: char| c <= ' ');
    if a.is_empty() {
        return None;
    }
    let scheme = a
        .split_once(':')
        .map(|s| s.0)
        .filter(|s| s.starts_with(|c: char| c.is_ascii_alphabetic()) && s.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c)));
    match scheme {
        Some(s) if !["http", "https", "mailto", "ftp", "tel"].iter().any(|k| s.eq_ignore_ascii_case(k)) => None,
        _ => Some(a.to_string()),
    }
}

/// A table's rows placed on its grid; its cells' content in order instead when it has more rows or columns than a
/// table can.
fn placed(rows: Vec<Vec<HCell>>) -> Vec<HBlock> {
    use super::tables::{MAX_COLUMNS, MAX_ROWS};
    let rows: Vec<Vec<HCell>> = rows.into_iter().filter(|r| !r.is_empty()).collect();
    let flat = |rows: Vec<Vec<HCell>>| rows.into_iter().flatten().flat_map(|c| c.blocks).collect();
    if rows.len() > MAX_ROWS {
        return flat(rows);
    }
    let n = rows.len();
    // Per column: the rows below that a cell starting there still spans, and its span
    let mut down = [(0usize, 0usize); MAX_COLUMNS];
    let mut grid: Vec<Vec<(usize, usize, VMerge, bool)>> = Vec::with_capacity(n);
    let mut cols = 1;
    for (r, row) in rows.iter().enumerate() {
        let mut line = Vec::with_capacity(row.len());
        let mut col = 0;
        let mut cells = row.iter();
        while col < MAX_COLUMNS {
            if down[col].0 > 0 {
                let span = down[col].1;
                down[col].0 -= 1;
                line.push((col, span, VMerge::Continue, false));
                col += span;
                continue;
            }
            let Some(c) = cells.next() else {
                // Past the row's last cell, only those above spanning down
                col += 1;
                continue;
            };
            if col + c.span > MAX_COLUMNS {
                return flat(rows);
            }
            down[col + 1..col + c.span].fill((0, 0));
            let below = if c.rows == 0 { n - r - 1 } else { (c.rows - 1).min(n - r - 1) };
            down[col] = (below, c.span);
            line.push((col, c.span, if below > 0 { VMerge::Restart } else { VMerge::None }, true));
            col += c.span;
        }
        if cells.next().is_some() {
            return flat(rows);
        }
        cols = line.last().map_or(cols, |&(c, s, _, _)| cols.max(c + s));
        grid.push(line);
    }
    let rows = rows
        .into_iter()
        .zip(grid)
        .map(|(row, line)| {
            let mut cells = row.into_iter();
            line.into_iter()
                .map(|(col, span, merge, own)| Slot {
                    col,
                    span,
                    merge,
                    blocks: if own { cells.next().map(|c| c.blocks).unwrap_or_default() } else { Vec::new() },
                })
                .collect()
        })
        .collect();
    vec![HBlock::Table { cols, rows }]
}

/// The table cells `blocks` make, nested tables' included.
fn cells(blocks: &[HBlock]) -> usize {
    blocks.iter().fold(0, |n, b| match b {
        HBlock::Para(_) => n,
        HBlock::Table { cols, rows } => {
            rows.iter().flatten().fold(n.saturating_add(rows.len().saturating_mul(*cols)), |n, s| n.saturating_add(cells(&s.blocks)))
        }
    })
}

/// A CSS declaration's value in a `style` attribute, lower case.
fn css(style: &str, property: &str) -> Option<String> {
    style.split(';').find_map(|d| {
        let (k, v) = d.split_once(':')?;
        (k.trim().eq_ignore_ascii_case(property)).then(|| v.trim().to_ascii_lowercase())
    })
}

impl Reader {
    fn fmt(&self) -> (Fmt, Option<String>, Option<Jc>) {
        self.open.last().map_or((Fmt::default(), None, None), |o| (o.fmt, o.link.clone(), o.align))
    }

    /// Where blocks go now: the open cell, or the document.
    fn target(&mut self) -> &mut Vec<HBlock> {
        match self.tables.last_mut() {
            Some((rows, true)) => match rows.last_mut().and_then(|r| r.last_mut()) {
                Some(c) => &mut c.blocks,
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
            p.runs.retain(|r| !r.text.is_empty() || r.image.is_some());
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
        self.seen.text |= !t.trim().is_empty();
        self.append(&t);
    }

    /// `t` added to the paragraph being read, as the open elements format it.
    fn append(&mut self, t: &str) {
        if self.para.is_none() {
            self.para = Some(Para { kind: Kind::Body, align: self.fmt().2, runs: Vec::new() });
        }
        let (fmt, link, _) = self.fmt();
        let Some(p) = self.para.as_mut() else { return };
        match p.runs.last_mut() {
            Some(r) if r.fmt == fmt && r.link == link && r.image.is_none() => r.text.push_str(t),
            _ => p.runs.push(Run { text: t.to_string(), fmt, link, image: None }),
        }
    }

    /// An `<img>`: the picture when its source is one (a `data:` URL), else its alternative text; nothing is fetched.
    fn image(&mut self, tag: &str) {
        let alt = attr(tag, "alt").unwrap_or_default();
        let Some(data) = attr(tag, "src").as_deref().and_then(data_image) else {
            self.seen.remote = true;
            let alt = alt.trim();
            if !alt.is_empty() {
                self.append(alt);
                self.space = false;
            }
            return;
        };
        let style = attr(tag, "style").unwrap_or_default();
        let px = |name: &str| {
            let v = css(&style, name).or_else(|| attr(tag, name))?;
            let v = v.trim().trim_end_matches("px").trim().parse::<f64>().ok()?;
            (v.is_finite() && v > 0.0).then_some(v)
        };
        let (w, h) = (px("width"), px("height"));
        self.seen.drawn = true;
        if self.para.is_none() {
            self.para = Some(Para { kind: Kind::Body, align: self.fmt().2, runs: Vec::new() });
        }
        let (fmt, link, _) = self.fmt();
        if let Some(p) = self.para.as_mut() {
            p.runs.push(Run { text: String::new(), fmt, link, image: Some(Image { data, w, h, alt }) });
        }
        self.space = false;
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
            "a" => link = attr(tag, "href").as_deref().and_then(web_address),
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
                "center" => Some(Jc::Center),
                "right" | "end" => Some(Jc::Right),
                "justify" => Some(Jc::Both),
                _ => None,
            };
        }
        match name {
            "br" => {
                if self.skip == 0 {
                    self.append("\u{2028}");
                }
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
                let numbered = name == "ol";
                let list = match self.lists.last() {
                    Some(&(n, l)) if n == numbered => l,
                    _ => {
                        self.next_list += 1;
                        self.next_list - 1
                    }
                };
                self.lists.push((numbered, list));
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
                if self.deep > 0 || self.tables.len() == MAX_TABLES {
                    self.deep += 1;
                } else {
                    self.tables.push((Vec::new(), false));
                }
            }
            "tr" => {
                self.end_para();
                if self.deep == 0
                    && let Some((rows, open)) = self.tables.last_mut()
                {
                    rows.push(Vec::new());
                    *open = false;
                }
            }
            "td" | "th" => {
                self.end_para();
                if name == "th" {
                    fmt.bold = true;
                }
                let number = |a: &str| attr(tag, a).and_then(|s| s.trim().parse::<usize>().ok());
                let span = number("colspan").unwrap_or(1).clamp(1, super::tables::MAX_COLUMNS);
                let down = number("rowspan").unwrap_or(1);
                if self.deep == 0
                    && let Some((rows, open)) = self.tables.last_mut()
                {
                    if rows.is_empty() {
                        rows.push(Vec::new());
                    }
                    if let Some(r) = rows.last_mut() {
                        r.push(HCell { span, rows: down, blocks: Vec::new() });
                    }
                    *open = true;
                }
            }
            "img" => {
                if self.skip == 0 {
                    self.image(tag);
                }
                return;
            }
            "input" | "meta" | "link" | "area" | "base" | "col" | "embed" | "param" | "source" | "track" | "wbr" => return,
            _ => {}
        }
        if name == "pre" {
            self.pre += 1;
        }
        self.push_open(name, fmt, link, align);
    }

    fn push_open(&mut self, name: &str, fmt: Fmt, link: Option<String>, align: Option<Jc>) {
        if self.open.len() < MAX_OPEN {
            self.open.push(Open { name: name.to_string(), fmt, link, align });
        } else {
            *self.over.entry(name.to_string()).or_default() += 1;
        }
    }

    fn end(&mut self, name: &str) {
        if matches!(name, "head" | "style" | "script" | "title" | "template") {
            self.skip = self.skip.saturating_sub(1);
        }
        match self.over.get_mut(name) {
            Some(n) if *n > 0 => *n -= 1,
            _ => {
                if let Some(k) = self.open.iter().rposition(|o| o.name == name) {
                    self.open.truncate(k);
                    self.over.clear();
                }
            }
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
                if self.deep == 0
                    && let Some((_, open)) = self.tables.last_mut()
                {
                    *open = false;
                }
            }
            "table" => {
                self.end_para();
                if self.deep > 0 {
                    self.deep -= 1;
                } else if let Some((rows, _)) = self.tables.pop() {
                    let blocks = placed(rows);
                    self.seen.table |= !blocks.is_empty();
                    self.target().extend(blocks);
                }
            }
            _ => {}
        }
    }
}

/// The picture a `data:` URL holds, when it is one Twip draws (`image/...;base64`).
fn data_image(src: &str) -> Option<Vec<u8>> {
    let src = src.trim();
    let (meta, payload) = src.get(..5).filter(|s| s.eq_ignore_ascii_case("data:")).and_then(|_| src[5..].split_once(','))?;
    let mut parts = meta.split(';');
    let mime = parts.next()?.trim();
    if !mime.get(..6).is_some_and(|m| m.eq_ignore_ascii_case("image/")) || !parts.any(|p| p.trim().eq_ignore_ascii_case("base64")) {
        return None;
    }
    let clean: String = payload.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let data = crate::api::value::from_base64(&clean)?;
    (crate::docx::image::image_type(&data).is_some() && crate::docx::image::image_info(&data).is_some()).then_some(data)
}

/// Keep Text Only's text of HTML: each paragraph's text (a picture's left out), a list item's number or bullet before
/// it when `bullets`, a table's rows as lines of cells between tabs.
pub(super) fn text_only(html: &str, bullets: bool) -> Option<String> {
    if html.len() > MAX_HTML {
        return None;
    }
    fn walk(blocks: &[HBlock], bullets: bool, counts: &mut HashMap<usize, [u32; 9]>, out: &mut String) {
        for b in blocks {
            match b {
                HBlock::Para(p) => {
                    if let Kind::Item { list, numbered, level } = p.kind {
                        let c = counts.entry(list).or_insert([0; 9]);
                        let l = usize::from(level.min(8));
                        c[l] += 1;
                        c[l + 1..].fill(0);
                        if bullets {
                            if numbered {
                                out.push_str(&format!("{}.\t", c[l]));
                            } else {
                                out.push_str("\u{2022}\t");
                            }
                        }
                    }
                    for r in p.runs.iter().filter(|r| r.image.is_none()) {
                        out.extend(r.text.chars().map(|c| if c == '\u{2028}' { '\n' } else { c }));
                    }
                    out.push('\n');
                }
                HBlock::Table { rows, .. } => {
                    for row in rows {
                        for (k, slot) in row.iter().enumerate() {
                            if k > 0 {
                                out.push('\t');
                            }
                            let mut s = String::new();
                            walk(&slot.blocks, bullets, counts, &mut s);
                            out.push_str(s.trim_end_matches('\n').replace('\n', " ").as_str());
                        }
                        out.push('\n');
                    }
                }
            }
        }
    }
    let read = read(html);
    let mut out = String::new();
    walk(&read, bullets, &mut HashMap::new(), &mut out);
    if matches!(read.last(), Some(HBlock::Para(p)) if p.kind == Kind::Body) && out.ends_with('\n') {
        out.pop();
    }
    Some(out)
}

/// The blocks of an HTML clipboard fragment (only what its `StartFragment` and `EndFragment` comments mark, when it
/// has them).
fn read(html: &str) -> Vec<HBlock> {
    scan(html).out
}

/// Whether HTML holds nothing but pictures the engine doesn't draw (remote ones, never fetched): a browser's Copy
/// Image, whose picture the clipboard also holds.
pub(super) fn only_remote_pictures(html: &str) -> bool {
    if html.len() > MAX_HTML {
        return false;
    }
    let s = scan(html).seen;
    s.remote && !s.drawn && !s.text && !s.table
}

/// HTML read: its blocks, and what it met.
fn scan(html: &str) -> Reader {
    let html = match (html.find("<!--StartFragment-->"), html.find("<!--EndFragment-->")) {
        (Some(a), Some(b)) if a < b => &html[a + "<!--StartFragment-->".len()..b],
        _ => html,
    };
    let mut r = Reader {
        out: Vec::new(),
        tables: Vec::new(),
        deep: 0,
        open: Vec::new(),
        over: HashMap::new(),
        lists: Vec::new(),
        next_list: 0,
        para: None,
        space: true,
        pre: 0,
        skip: 0,
        seen: Seen::default(),
    };
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
        let Some(e) = tag_end(rest) else { break };
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
        let blocks = placed(rows);
        r.seen.table |= !blocks.is_empty();
        r.target().extend(blocks);
    }
    r
}

/// The lists `blocks` use, each with whether it is numbered.
fn lists(blocks: &[HBlock], out: &mut Vec<(usize, bool)>) {
    for b in blocks {
        match b {
            HBlock::Para(Para { kind: Kind::Item { list, numbered, .. }, .. }) if !out.iter().any(|(l, _)| l == list) => out.push((*list, *numbered)),
            HBlock::Para(_) => {}
            HBlock::Table { rows, .. } => rows.iter().flatten().for_each(|s| lists(&s.blocks, out)),
        }
    }
}

impl Editor {
    /// Pastes HTML from another application as one transaction: its paragraphs, headings (in the Heading styles the
    /// document has), lists (as new lists), tables, links and bold, italic, underline, strikethrough, superscript and
    /// subscript. `Unchanged` when it holds no text, for a paste of the plain text instead.
    pub fn paste_html(&mut self, html: &str, fonts: &Fonts) -> Outcome {
        self.paste_html_as(html, super::paste::PasteChoice::KeepSource, fonts)
    }

    /// [`Editor::paste_html`] as `choice` takes it: keep source (and destination styles, the same for HTML) or merge
    /// formatting. Its `data:` pictures come in line.
    pub(super) fn paste_html_as(&mut self, html: &str, choice: super::paste::PasteChoice, fonts: &Fonts) -> Outcome {
        if html.len() > MAX_HTML {
            return Outcome::Unchanged("too much HTML");
        }
        let read = read(html);
        if read.is_empty() {
            return Outcome::Unchanged("no HTML content");
        }
        if cells(&read) > MAX_CELLS {
            return Outcome::Unchanged("too many table cells");
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
            let mut images = Vec::new();
            let blocks = self.html_blocks(&read, &res, &nums, &template, width, &mut images);
            // A last paragraph of a kind or alignment of its own is pasted with its mark; plain text joins the caret's.
            let open_end = matches!(read.last(), Some(HBlock::Para(p)) if p.kind == Kind::Body && p.align.is_none());
            let mut f = Fragment { id: 0, blocks, open_end, ..Default::default() };
            if !images.is_empty() {
                // Its pictures, by their index in `images`, become this document's as another document's do.
                let o =
                    super::import::Origin { key: super::import::next_key(), images, resolver: None, people: Default::default(), main: String::new() };
                f.origin = Some(std::rc::Rc::new(o));
            }
            if choice == super::paste::PasteChoice::Merge {
                match self.merged(&f) {
                    Some(m) => f = m,
                    None => break 'paste Outcome::Unchanged("no caret"),
                }
            }
            self.paste_fragment(&f, fonts)
        };
        // The new lists go with a paste that pasted nothing.
        let end = self.end_batch(matches!(out, Outcome::Changed(_)), fonts);
        if matches!(end, Outcome::Changed(_)) { end } else { out }
    }

    fn html_blocks(
        &mut self,
        blocks: &[HBlock],
        res: &Resolvers,
        nums: &[(usize, i64)],
        template: &Paragraph,
        width: f64,
        images: &mut Vec<(usize, Vec<u8>)>,
    ) -> Vec<Block> {
        let mut out = Vec::new();
        for b in blocks {
            match b {
                HBlock::Para(p) => {
                    let mut pp = ParaProps::default();
                    match p.kind {
                        Kind::Heading(n) => {
                            let id = format!("Heading{n}");
                            if res.styles.by_id.contains_key(&id) {
                                pp.style = Some(id.into());
                            }
                        }
                        Kind::Item { list, level, .. } => {
                            if let Some(&(_, num)) = nums.iter().find(|(l, _)| *l == list) {
                                pp.list = Some(if num == 0 { ListChoice::None } else { ListChoice::In(ListId(num)) });
                                pp.list_level = Some(i64::from(level));
                            }
                        }
                        Kind::Body => {}
                    }
                    pp.align = p.align;
                    let para = crate::docx::para_xml::with(res, ParaDirect::typed(Rc::new(pp)), None, None);
                    let base = para.base.clone();
                    let mark = Rc::new(run_xml::resolve(res, &base, RunDirect::default(), false));
                    let mut inlines = Vec::new();
                    for r in &p.runs {
                        use crate::format::Toggle as T;
                        let f = r.fmt;
                        let mut q = CharProps::default();
                        for (on, t) in [(f.bold, T::BOLD), (f.italic, T::ITALIC), (f.strike, T::STRIKE)] {
                            if on {
                                q.toggles.put(t, true);
                            }
                        }
                        if f.underline || r.link.is_some() {
                            q.underline = Some(crate::format::UnderlineProps { style: crate::docx::Underline::Single, color: None });
                        }
                        if f.sup {
                            q.vertical = Some(crate::docx::props::VertAlign::Superscript);
                        } else if f.sub {
                            q.vertical = Some(crate::docx::props::VertAlign::Subscript);
                        }
                        if r.link.is_some() && res.styles.by_id.contains_key("Hyperlink") {
                            q.underline = None;
                            q.style = Some("Hyperlink".into());
                        }
                        let direct = RunDirect::typed((q != CharProps::default()).then(|| Rc::new(q)));
                        let run = Rc::new(run_xml::resolve(res, &base, direct, false));
                        if let Some(img) = &r.image {
                            let pic = self.html_picture(img, width, images, &run);
                            match &r.link {
                                Some(address) => {
                                    let code = crate::docx::fields::link_code(address);
                                    let mut field = crate::docx::fields::field_inlines(&code, "\u{fffc}", &run);
                                    if let Some(k) = field.iter().position(|i| matches!(i, Inline::Text(t, _) if t == "\u{fffc}")) {
                                        field[k] = pic;
                                    }
                                    inlines.extend(field);
                                }
                                None => inlines.push(pic),
                            }
                            continue;
                        }
                        match &r.link {
                            Some(address) => {
                                let code = crate::docx::fields::link_code(address);
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
                HBlock::Table { cols, rows } => {
                    let mut t = self.new_table(vec![Vec::new()], *cols, width, template);
                    let col = t.grid.first().copied().unwrap_or(width);
                    let Some(mut like) = t.rows.pop() else { continue };
                    let Some(blank) = like.cells.first().cloned() else { continue };
                    like.cells.clear();
                    t.rows.reserve(rows.len());
                    for slots in rows {
                        let mut row = like.clone();
                        row.cells.reserve(*cols);
                        let mut at = 0;
                        for s in slots {
                            row.cells.extend((at..s.col).map(|_| blank.clone()));
                            let mut cell = blank.clone();
                            cell.span = s.span;
                            cell.v_merge = s.merge;
                            cell.width = Some(crate::docx::props::Width::Dxa(col * s.span as f64));
                            let blocks = self.html_blocks(&s.blocks, res, nums, template, col * s.span as f64, images);
                            if !blocks.is_empty() {
                                let end = std::mem::replace(&mut cell.blocks, blocks);
                                // A cell ends in a paragraph
                                if matches!(cell.blocks.last(), Some(Block::Table(_))) {
                                    cell.blocks.extend(end);
                                }
                            }
                            row.cells.push(cell);
                            at = s.col + s.span;
                        }
                        row.cells.extend((at..*cols).map(|_| blank.clone()));
                        t.rows.push(row);
                    }
                    out.push(Block::Table(t));
                }
            }
        }
        out
    }
}

impl Editor {
    /// The document's imported HTML (`w:altChunk`) brought in as content in place of the paragraphs standing for it,
    /// as Word does when it opens the document: not an edit (no undo step, not tracked, the document unchanged as far
    /// as saving goes until then), and saved as the content it became. HTML the importer finds nothing in leaves
    /// nothing; one in a story the editor doesn't hold is left as read.
    pub(super) fn import_alt_chunks(&mut self, fonts: &Fonts) {
        use super::index::Container;
        if self.doc.alt_chunks.is_empty() {
            return;
        }
        if self.doc.resolver.is_none() {
            return;
        }
        let chunks = std::mem::take(&mut self.doc.alt_chunks);
        let tracking = std::mem::replace(&mut self.doc.settings.track_revisions, false);
        // Each placeholder's HTML, read, by container in block order
        let mut read_in: Vec<(Container, usize, Paragraph, Chunk)> = chunks
            .iter()
            .filter_map(|c| {
                let para = crate::layout::ParaId(c.para);
                let (slot, template) = (self.index.para_slot(para)?, self.index.para(&self.doc, para)?.clone());
                let read = match &c.content {
                    AltContent::Html(html) => {
                        let read = if html.len() > MAX_HTML { Vec::new() } else { read(html) };
                        Chunk::Html(if cells(&read) > MAX_CELLS { Vec::new() } else { read })
                    }
                    AltContent::Text(t) => Chunk::Text(t.clone()),
                    AltContent::Document { doc, match_src } => Chunk::Document(doc.clone(), *match_src),
                };
                Some((slot.container, slot.block, template, read))
            })
            .collect();
        let mut first: HashMap<Container, usize> = HashMap::new();
        for r in &read_in {
            let n = first.len();
            first.entry(r.0).or_insert(n);
        }
        read_in.sort_by_key(|r| (first[&r.0], r.1));
        // Their lists first, in one step: the paragraphs resolve with them.
        let mut nums: Vec<Vec<(usize, i64)>> = vec![Vec::new(); read_in.len()];
        let mut tx = self.begin("Open");
        let mut any = false;
        for (k, (.., read)) in read_in.iter().enumerate() {
            let Chunk::Html(read) = read else { continue };
            let mut used = Vec::new();
            lists(read, &mut used);
            any |= !used.is_empty();
            nums[k] = used.iter().map(|&(list, numbered)| (list, self.new_list(&mut tx, !numbered))).collect();
        }
        if any {
            self.commit(tx, None, fonts);
        } else {
            self.abort(tx);
        }
        // Read after the lists step: it holds their numbering.
        let Some(res) = self.doc.resolver.clone() else {
            (self.doc.alt_chunks, self.doc.settings.track_revisions) = (chunks, tracking);
            return;
        };
        let mut tx = self.begin("Open");
        let mut taken = super::clipboard::Taken::default();
        // One step a container: its blocks from its first placeholder to its last, each placeholder its HTML's.
        let mut k = 0;
        while k < read_in.len() {
            let container = read_in[k].0;
            let end = k + read_in[k..].iter().take_while(|r| r.0 == container).count();
            let Some(len) = self.index.blocks(&self.doc, container).map(Vec::len) else {
                k = end;
                continue;
            };
            let width = self.text_width(container);
            let (lo, hi) = (read_in[k].1, read_in[end - 1].1 + 1);
            let mut with = Vec::new();
            let mut at = lo;
            for j in k..end {
                let (_, block, template, read) = &read_in[j];
                if let Some(between) = self.index.blocks(&self.doc, container).and_then(|b| b.get(at..*block)) {
                    with.extend_from_slice(between);
                }
                let mut f = Fragment { id: 0, open_end: false, ..Default::default() };
                let mut blocks: Vec<Block> = match read {
                    Chunk::Html(read) => {
                        let mut images = Vec::new();
                        let blocks =
                            self.html_blocks(read, &res, &nums[j], template, width, &mut images).iter().map(super::clipboard::fresh).collect();
                        if !images.is_empty() {
                            let o = super::import::Origin {
                                key: super::import::next_key(),
                                images,
                                resolver: None,
                                people: Default::default(),
                                main: String::new(),
                            };
                            f.origin = Some(std::rc::Rc::new(o));
                        }
                        blocks
                    }
                    Chunk::Text(t) => text_blocks(t, template),
                    Chunk::Document(doc, match_src) => match self.document_body((**doc).clone(), fonts) {
                        Ok(d) => {
                            f = d;
                            f.look = if *match_src { super::clipboard::Look::Source } else { super::clipboard::Look::Destination };
                            f.blocks.iter().map(super::clipboard::fresh).collect()
                        }
                        Err(_) => Vec::new(),
                    },
                };
                taken.next_fragment();
                self.take_in(&mut tx, &mut blocks, &f, &mut taken);
                with.append(&mut blocks);
                at = block + 1;
            }
            // A story (a cell) ends in a paragraph.
            if hi == len {
                let before = lo.checked_sub(1).and_then(|i| self.index.blocks(&self.doc, container)?.get(i));
                if !matches!(with.last().or(before), Some(Block::Para(_))) {
                    with.push(Block::Para(super::tables::blank_para(&read_in[end - 1].2)));
                }
            }
            // A container that can't take its HTML keeps its placeholders; the others still come in. Pictures taken
            // for it stay unreferenced: saving writes only the media a part names.
            let _ = self.step(&mut tx, super::step::Step::Blocks { container, range: lo..hi, with });
            k = end;
        }
        self.commit(tx, None, fonts);
        self.doc.settings.track_revisions = tracking;
        self.history = super::history::History::default();
        self.saved.0 = self.history.position();
    }

    /// An HTML picture in line: its own size, or the width and height it is given (one of them keeping its shape),
    /// no wider than `width`; its bytes go in `images` under the index it shows.
    fn html_picture(&mut self, img: &Image, width: f64, images: &mut Vec<(usize, Vec<u8>)>, run: &crate::docx::Run) -> Inline {
        let (pw, ph, dx, dy) = crate::docx::image::image_info(&img.data).unwrap_or((96.0, 96.0, 96.0, 96.0));
        let natural = (pw * 72.0 / if dx > 1.0 { dx } else { 96.0 }, ph * 72.0 / if dy > 1.0 { dy } else { 96.0 });
        let pt = 72.0 / 96.0;
        let (w, h) = match (img.w, img.h) {
            (Some(w), Some(h)) => (w * pt, h * pt),
            (Some(w), None) if natural.0 > 0.0 => (w * pt, natural.1 * (w * pt / natural.0)),
            (None, Some(h)) if natural.1 > 0.0 => (natural.0 * (h * pt / natural.1), h * pt),
            _ => natural,
        };
        let (mut w, mut h) = if w.is_finite() && h.is_finite() { (w.max(0.0), h.max(0.0)) } else { natural };
        if w > width && w > 0.0 {
            (w, h) = (width, h * (width / w));
        }
        // Both sides by one factor, the larger within the largest side
        let k = (MAX_PICTURE_SIDE / w.max(h)).min(1.0);
        let side = |v: f64| ((v * k * 20.0).round() / 20.0).clamp(1.0, MAX_PICTURE_SIDE);
        let (w, h) = (side(w), side(h));
        let k = images.len();
        images.push((k, img.data.clone()));
        let emu = crate::drawing::units::Emu::from_pt;
        let mut shape = crate::drawing::Shape::picture(k, emu(w), emu(h));
        shape.nv.name = self.new_object_name("Picture");
        Inline::Picture(
            crate::docx::Picture {
                w,
                h,
                image: Some(k),
                alt: img.alt.trim().into(),
                lock_aspect: true,
                effect: [0.0; 4],
                shape: Some(Box::new(shape)),
                text: None,
                src: Default::default(),
            },
            run.clone(),
        )
    }
}

/// A `w:altChunk`'s content, read.
enum Chunk {
    Html(Vec<HBlock>),
    Text(std::rc::Rc<str>),
    Document(std::rc::Rc<crate::docx::Document>, bool),
}

/// Plain text as paragraphs like `template`: a line each (ended by CR LF, CR or LF; a last line ending ends the last
/// paragraph), tabs as tabs, other control characters left out.
fn text_blocks(t: &str, template: &Paragraph) -> Vec<Block> {
    let t = t.strip_suffix("\r\n").or_else(|| t.strip_suffix(['\r', '\n'])).unwrap_or(t);
    if t.is_empty() {
        return Vec::new();
    }
    let base = super::tables::blank_para(template);
    t.split("\r\n")
        .flat_map(|l| l.split(['\r', '\n']))
        .map(|line| {
            let mut inlines = Vec::new();
            for (k, piece) in line.split('\t').enumerate() {
                if k > 0 {
                    inlines.push(Inline::Tab(base.mark.clone()));
                }
                let text: String = piece.chars().filter(|c| !c.is_control()).collect();
                if !text.is_empty() {
                    inlines.push(Inline::Text(text, base.mark.clone()));
                }
            }
            if inlines.is_empty() {
                inlines.push(Inline::Text(String::new(), base.mark.clone()));
            }
            Block::Para(Paragraph { inlines, id: crate::docx::next_para_id(), ..base.clone() })
        })
        .collect()
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
        let HBlock::Table { rows, .. } = &b[5] else { panic!("{b:?}") };
        assert_eq!(rows[0].iter().map(|c| c.span).collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn decodes_references_and_skips_scripts() {
        let b = read("<p>a&amp;b &#x41;&#66; &bogus; x</p><script>alert(1)</script>");
        let HBlock::Para(p) = &b[0] else { panic!("{b:?}") };
        assert_eq!(p.runs[0].text, "a&b AB &bogus; x");
        assert_eq!(b.len(), 1);
    }

    /// Each paragraph's text, table cells' in order, with the depth of tables it is in.
    fn paras(blocks: &[HBlock], depth: usize, out: &mut Vec<(usize, String)>) {
        for b in blocks {
            match b {
                HBlock::Para(p) => out.push((depth, p.runs.iter().map(|r| r.text.as_str()).collect())),
                HBlock::Table { rows, .. } => rows.iter().flatten().for_each(|s| paras(&s.blocks, depth + 1, out)),
            }
        }
    }

    fn all(html: &str) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        paras(&read(html), 0, &mut out);
        out
    }

    fn link(html: &str) -> Option<String> {
        let b = read(html);
        let HBlock::Para(p) = &b[0] else { panic!("{b:?}") };
        p.runs.iter().find_map(|r| r.link.clone())
    }

    #[test]
    fn a_table_past_the_deepest_nesting_reads_as_its_cells_paragraphs() {
        let open = "<table><tr><td>".repeat(MAX_TABLES);
        let close = "</td></tr></table>".repeat(MAX_TABLES);
        let html = format!("{open}<table><tr><td>x</td></tr><tr><td>y</td></tr></table><p>after</p>{close}<p>out</p>");
        let got = all(&html);
        assert_eq!(
            got,
            [(MAX_TABLES, "x".to_string()), (MAX_TABLES, "y".into()), (MAX_TABLES, "after".into()), (0, "out".into())],
            "the deeper table's cells go in its parent's cell, which stays open"
        );
        let mut t = &read(&html)[0];
        for _ in 1..MAX_TABLES {
            let HBlock::Table { rows, .. } = t else { panic!("{t:?}") };
            t = &rows[0][0].blocks[0];
        }
        let HBlock::Table { rows, .. } = t else { panic!("{t:?}") };
        assert_eq!(rows.len(), 1, "the deepest table kept has its own one row");
    }

    #[test]
    fn an_element_past_the_most_open_closes_itself_only() {
        let html = format!("<p>{}<b><b>x</b>y</b>z</p>", "<span>".repeat(MAX_OPEN - 2));
        let b = read(&html);
        let HBlock::Para(p) = &b[0] else { panic!("{b:?}") };
        let runs: Vec<(&str, bool)> = p.runs.iter().map(|r| (r.text.as_str(), r.fmt.bold)).collect();
        assert_eq!(runs, [("xy", true), ("z", false)], "the inner end closes the inner element");
    }

    #[test]
    fn cells_below_a_row_spanning_cell_keep_their_columns() {
        let b = read("<table><tr><td rowspan=2>A</td><td>B</td></tr><tr><td>C</td></tr></table>");
        let HBlock::Table { cols, rows } = &b[0] else { panic!("{b:?}") };
        assert_eq!(*cols, 2);
        let at = |r: usize| rows[r].iter().map(|s| (s.col, s.merge)).collect::<Vec<_>>();
        assert_eq!(at(0), [(0, VMerge::Restart), (1, VMerge::None)]);
        assert_eq!(at(1), [(0, VMerge::Continue), (1, VMerge::None)], "C is in the second column");
        assert_eq!(all("<table><tr><td rowspan=2>A</td><td>B</td></tr><tr><td>C</td></tr></table>").len(), 3);
    }

    #[test]
    fn a_table_wider_than_a_table_can_be_reads_as_its_cells_paragraphs() {
        let cells: String = (0..64).map(|k| format!("<td>c{k}</td>")).collect();
        let got = all(&format!("<table><tr>{cells}</tr></table>"));
        assert_eq!(got.len(), 64, "no cell is dropped");
        assert!(got.iter().all(|(d, _)| *d == 0) && got[63].1 == "c63", "{got:?}");
    }

    #[test]
    fn links_to_follow_keep_their_address_and_others_go() {
        assert_eq!(link("<a href=\"https://x.test/a\">l</a>").as_deref(), Some("https://x.test/a"));
        assert_eq!(link("<a href=\"page.html#top\">l</a>").as_deref(), Some("page.html#top"));
        for bad in ["java&#9;script:alert(1)", "javascript:alert(1)", " JavaScript:x", "data:text/html,x", "vbscript:x", "java\nscript:x"] {
            assert_eq!(link(&format!("<a href=\"{bad}\">l</a>")), None, "{bad}");
        }
    }

    #[test]
    fn a_quoted_attribute_holds_its_greater_than_sign() {
        let b = read("<p><a title=\" href=javascript:x\" href=\"a>b\">Link</a></p>");
        let HBlock::Para(p) = &b[0] else { panic!("{b:?}") };
        assert_eq!(p.runs.iter().map(|r| (r.text.as_str(), r.link.as_deref())).collect::<Vec<_>>(), [("Link", Some("a>b"))]);
    }

    #[test]
    fn a_line_break_stays_a_line_break() {
        let b = read("<p>A<br>B <br/> C</p>");
        let HBlock::Para(p) = &b[0] else { panic!("{b:?}") };
        assert_eq!(p.runs[0].text, "A\u{2028}B \u{2028}C");
    }

    #[test]
    fn a_list_in_a_list_of_the_other_kind_is_a_list_of_its_own() {
        let b = read("<ul><li>a<ol><li>one</li></ol></li><li>b<ul><li>deep</li></ul></li></ul>");
        let kinds: Vec<(usize, bool, u8)> = b
            .iter()
            .filter_map(|x| {
                if let HBlock::Para(Para { kind: Kind::Item { list, numbered, level }, .. }) = x { Some((*list, *numbered, *level)) } else { None }
            })
            .collect();
        assert_eq!(kinds, [(0, false, 0), (1, true, 1), (0, false, 0), (0, false, 1)]);
        let mut used = Vec::new();
        lists(&b, &mut used);
        assert_eq!(used, [(0, false), (1, true)]);
    }

    fn editor() -> Editor {
        Editor::open(&crate::editing::tests::blank_with(""), crate::editing::tests::fonts()).expect("opens")
    }

    fn body(e: &Editor) -> &[Block] {
        &e.document().sections[0].blocks
    }

    #[test]
    fn a_cell_holding_only_a_table_ends_in_a_paragraph() {
        let mut e = editor();
        let out = e.paste_html("<table><tr><td><table><tr><td>in</td></tr></table></td></tr></table>", crate::editing::tests::fonts());
        assert!(matches!(out, Outcome::Changed(_)), "{out:?}");
        let Some(Block::Table(t)) = body(&e).iter().find(|b| matches!(b, Block::Table(_))) else { panic!("a table") };
        assert!(matches!(t.rows[0].cells[0].blocks.as_slice(), [Block::Table(_), Block::Para(_)]), "{:?}", t.rows[0].cells[0].blocks);
        let v = super::super::integrity::check(e.document(), e.index(), &e.selection());
        assert!(v.is_empty(), "{:?}", v.iter().map(|v| v.detail.clone()).collect::<Vec<_>>());
    }

    #[test]
    fn a_paste_making_too_many_cells_pastes_as_text() {
        let mut e = editor();
        let wide: String = (0..63).map(|_| "<td>w</td>").collect();
        let html = format!("<table><tr>{wide}</tr>{}</table>", "<tr><td>t</td></tr>".repeat(MAX_CELLS / 63));
        assert!(matches!(e.paste_html(&html, crate::editing::tests::fonts()), Outcome::Unchanged("too many table cells")));
        assert_eq!(body(&e).len(), 1, "the document is as it was");
    }

    #[test]
    fn a_link_address_reads_back_from_its_field() {
        let mut e = editor();
        e.paste_html(r#"<p><a href="https://x.test/a\b&quot;c">l</a></p>"#, crate::editing::tests::fonts());
        let code = body(&e)
            .iter()
            .find_map(|b| {
                if let Block::Para(p) = b {
                    crate::docx::fields::spans(&p.inlines).first().map(|s| crate::docx::fields::code_text(&p.inlines, *s))
                } else {
                    None
                }
            })
            .expect("a field");
        assert_eq!(crate::docx::fields::parse(&code).args.first().map(String::as_str), Some(r#"https://x.test/a\b"c"#));
    }
}
