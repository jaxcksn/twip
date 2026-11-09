//! Numbering

use std::collections::HashMap;

use super::props::*;
use super::styles::StyleOrigin;
use super::xml::*;

#[derive(Clone, Debug)]
pub struct Level {
    pub start: i64,
    pub fmt: String,
    pub text: String,
    /// `left`, `center`, `right`.
    pub jc: String,
    /// `tab`, `space`, `nothing`.
    pub suff: String,
    pub is_lgl: bool,
    /// `w:lvlRestart` (1-based level that restarts this one; 0 = never).
    pub restart: Option<i64>,
    pub pstyle: Option<String>,
    /// `w:lvlPicBulletId`: the picture bullet (a `w:numPicBullet`) drawn for the label.
    pub pic_bullet: Option<i64>,
    pub ppr: ParaProps,
    pub rpr: RunProps,
}

#[derive(Clone, Debug, Default)]
pub struct AbstractNum {
    pub levels: Vec<Option<Level>>,
    pub num_style_link: Option<String>,
    /// `w:styleLink`: the numbering style this definition is.
    pub style_link: Option<String>,
    /// `w:multiLevelType`: `singleLevel`, `multilevel` or `hybridMultilevel`.
    pub multi_level_type: Option<String>,
    /// `w:name`.
    pub name: Option<String>,
    pub origin: StyleOrigin,
    /// The `w:abstractNum` as read or as last edited.
    pub raw: String,
}

#[derive(Clone, Debug, Default)]
pub struct Num {
    pub abstract_id: i64,
    pub start_overrides: HashMap<i64, i64>,
    /// `w:lvlOverride/w:lvl`: levels this `w:num` defines instead of its abstractNum's.
    pub level_overrides: HashMap<i64, Level>,
    pub origin: StyleOrigin,
    /// The `w:num` as read or as last edited.
    pub raw: String,
}

/// A `w:numPicBullet`: its id, its XML, and the relationship id of its image in the numbering part.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PicBullet {
    pub raw: String,
    pub rel: Option<String>,
    /// Its image's index in the document's images, once read.
    pub image: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct Numbering {
    pub abstracts: HashMap<i64, AbstractNum>,
    pub nums: HashMap<i64, Num>,
    pub pic_bullets: std::collections::BTreeMap<i64, PicBullet>,
    /// The `w:numbering` start tag, with its namespace declarations.
    pub root: String,
}

fn level(l: roxmltree::Node, colours: &super::props::ThemeColours) -> Level {
    let v = |name| child(l, name).and_then(val).map(str::to_string);
    Level {
        start: child(l, "start").and_then(|s| int(val(s))).unwrap_or(0),
        fmt: v("numFmt").unwrap_or_else(|| "decimal".into()),
        text: v("lvlText").unwrap_or_default(),
        jc: v("lvlJc").unwrap_or_else(|| "left".into()),
        suff: v("suff").unwrap_or_else(|| "tab".into()),
        is_lgl: child(l, "isLgl").map(on_off).unwrap_or(false),
        restart: child(l, "lvlRestart").and_then(|s| int(val(s))),
        pstyle: v("pStyle"),
        pic_bullet: child(l, "lvlPicBulletId").and_then(|s| int(val(s))),
        ppr: ParaProps::parse(child(l, "pPr"), colours),
        rpr: RunProps::parse(child(l, "rPr"), colours),
    }
}

/// A `w:abstractNum`, with its raw XML.
pub fn parse_abstract(a: roxmltree::Node, text: &str, colours: &super::props::ThemeColours) -> Option<(i64, AbstractNum)> {
    let id = int(attr(a, "abstractNumId"))?;
    let mut levels: Vec<Option<Level>> = vec![None; 9];
    for l in children(a, "lvl") {
        let ilvl = int(attr(l, "ilvl")).unwrap_or(0);
        if (0..9).contains(&ilvl) {
            levels[ilvl as usize] = Some(level(l, colours));
        }
    }
    let v = |name| child(a, name).and_then(val).map(str::to_string);
    let abs = AbstractNum {
        levels,
        num_style_link: v("numStyleLink"),
        style_link: v("styleLink"),
        multi_level_type: v("multiLevelType"),
        name: v("name"),
        origin: StyleOrigin::Document,
        raw: text[a.range()].to_string(),
    };
    Some((id, abs))
}

/// A `w:num`, with its raw XML.
pub fn parse_num(m: roxmltree::Node, text: &str, colours: &super::props::ThemeColours) -> Option<(i64, Num)> {
    let id = int(attr(m, "numId"))?;
    let mut num = Num {
        abstract_id: child(m, "abstractNumId").and_then(|a| int(val(a))).unwrap_or(-1),
        raw: text[m.range()].to_string(),
        ..Default::default()
    };
    for o in children(m, "lvlOverride") {
        let Some(il) = int(attr(o, "ilvl")) else { continue };
        if let Some(s) = child(o, "startOverride").and_then(|s| int(val(s))) {
            num.start_overrides.insert(il, s);
        }
        if let Some(l) = child(o, "lvl") {
            num.level_overrides.insert(il, level(l, colours));
        }
    }
    Some((id, num))
}

impl Numbering {
    pub fn parse(xml: Option<&str>, colours: &super::props::ThemeColours) -> Numbering {
        let mut n = Numbering::default();
        let Some(text) = xml else { return n };
        let Ok(doc) = roxmltree::Document::parse(text) else { return n };
        let root = doc.root_element();
        let start = root.range().start;
        n.root = text[start..start + text[start..].find('>').map_or(0, |e| e + 1)].to_string();
        for p in children(root, "numPicBullet") {
            let Some(id) = int(attr(p, "numPicBulletId")) else { continue };
            let rel = p
                .descendants()
                .find_map(|d| d.attributes().find(|a| matches!(a.name(), "id" | "embed") && a.namespace() == Some(R)).map(|a| a.value().to_string()));
            n.pic_bullets.insert(id, PicBullet { raw: text[p.range()].to_string(), rel, image: None });
        }
        for a in children(root, "abstractNum") {
            if let Some((id, abs)) = parse_abstract(a, text, colours) {
                n.abstracts.insert(id, abs);
            }
        }
        for m in children(root, "num") {
            if let Some((id, num)) = parse_num(m, text, colours) {
                n.nums.insert(id, num);
            }
        }
        n
    }

    /// The namespaces to parse this part's elements with.
    pub fn cx(&self) -> super::write::props::Cx {
        if self.root.is_empty() {
            return super::write::props::Cx::of_xml(&format!("<w:numbering xmlns:w=\"{}\">", super::write::props::W_NS));
        }
        super::write::props::Cx::of_xml(&self.root)
    }

    /// Adds or replaces a `w:abstractNum` from its XML; its id, or `None` when it isn't one.
    pub fn set_abstract_xml(&mut self, raw: String, colours: &super::props::ThemeColours) -> Option<i64> {
        let (id, mut abs) = self.cx().parse(&raw, |text, el| parse_abstract(el, text, colours))??;
        abs.raw = raw;
        self.abstracts.insert(id, abs);
        Some(id)
    }

    /// Adds or replaces a `w:num` from its XML; its id, or `None` when it isn't one.
    pub fn set_num_xml(&mut self, raw: String, colours: &super::props::ThemeColours) -> Option<i64> {
        let (id, mut num) = self.cx().parse(&raw, |text, el| parse_num(el, text, colours))??;
        num.raw = raw;
        self.nums.insert(id, num);
        Some(id)
    }

    /// A free `w:abstractNumId` and `w:numId`.
    pub fn free_ids(&self) -> (i64, i64) {
        (self.abstracts.keys().max().map_or(0, |m| m + 1), self.nums.keys().max().map_or(1, |m| m + 1).max(1))
    }

    /// The abstractNum a `numId` uses, following `numStyleLink` through the numbering style's own `numPr`.
    pub fn abstract_of(&self, num_id: i64, styles: &super::styles::Styles) -> Option<i64> {
        let a = self.nums.get(&num_id)?.abstract_id;
        if let Some(link) = self.abstracts.get(&a).and_then(|x| x.num_style_link.as_deref()) {
            let linked = styles.chain_ppr(Some(link)).num_id?;
            if linked != num_id {
                return self.nums.get(&linked).map(|n| n.abstract_id);
            }
        }
        Some(a)
    }

    pub fn level(&self, num_id: i64, ilvl: i64, styles: &super::styles::Styles) -> Option<&Level> {
        let ilvl = ilvl.clamp(0, 8);
        if let Some(l) = self.nums.get(&num_id).and_then(|n| n.level_overrides.get(&ilvl)) {
            return Some(l);
        }
        let a = self.abstract_of(num_id, styles)?;
        self.abstracts.get(&a)?.levels.get(ilvl as usize)?.as_ref()
    }

    /// The image a bullet level draws: its picture bullet's.
    pub fn bullet_image(&self, level: &Level) -> Option<usize> {
        (level.fmt == "bullet").then_some(())?;
        self.pic_bullets.get(&level.pic_bullet?)?.image
    }

    /// The nine levels a `numId` numbers with: its abstractNum's, with the `w:num`'s level overrides.
    pub fn levels(&self, num_id: i64, styles: &super::styles::Styles) -> Option<[Option<&Level>; 9]> {
        let abs = self.abstracts.get(&self.abstract_of(num_id, styles)?)?;
        let over = self.nums.get(&num_id).map(|n| &n.level_overrides);
        Some(std::array::from_fn(|k| over.and_then(|o| o.get(&(k as i64))).or(abs.levels.get(k).and_then(|l| l.as_ref()))))
    }
}

#[derive(Default)]
pub struct Counters {
    values: HashMap<i64, [Option<i64>; 9]>,
    overrides_applied: std::collections::HashSet<(i64, i64)>,
}

impl Counters {
    /// The value the counter for `num_id`'s level `ilvl` has now.
    pub fn value(&self, numbering: &Numbering, styles: &super::styles::Styles, num_id: i64, ilvl: i64) -> Option<i64> {
        self.values.get(&numbering.abstract_of(num_id, styles)?)?.get(ilvl.clamp(0, 8) as usize).copied().flatten()
    }

    /// Advances the counter for an item at `ilvl` of `num_id` and returns the label text.
    pub fn next(&mut self, numbering: &Numbering, styles: &super::styles::Styles, num_id: i64, ilvl: i64) -> Option<String> {
        let a = numbering.abstract_of(num_id, styles)?;
        let levels = numbering.levels(num_id, styles)?;
        let ilvl = ilvl.clamp(0, 8) as usize;
        let level = levels[ilvl]?;
        let vals = self.values.entry(a).or_insert([None; 9]);
        let over = numbering.nums.get(&num_id).and_then(|n| n.start_overrides.get(&(ilvl as i64)));
        vals[ilvl] = match over {
            Some(&s) if self.overrides_applied.insert((num_id, ilvl as i64)) => Some(s),
            _ => Some(vals[ilvl].map_or(level.start, |v| v + 1)),
        };
        for k in ilvl + 1..9 {
            let restart = levels[k].and_then(|l| l.restart);
            let reset = match restart {
                None => true,
                Some(0) => false,
                Some(n) => (ilvl as i64) < n,
            };
            if reset {
                vals[k] = None;
            }
        }
        for k in 0..ilvl {
            if vals[k].is_none() {
                vals[k] = Some(levels[k].map_or(1, |l| l.start));
            }
        }
        let vals = *vals;
        if level.fmt == "bullet" {
            return Some(level.text.clone());
        }
        let mut out = String::new();
        let mut chars = level.text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '%'
                && let Some(d) = chars.peek().and_then(|d| d.to_digit(10))
            {
                chars.next();
                let k = (d as usize).saturating_sub(1).min(8);
                let v = vals[k].unwrap_or(1);
                // Legal numbering shows every level in Arabic numerals, keeping a level's leading zero.
                let fmt = levels[k].map_or("decimal", |l| l.fmt.as_str());
                let fmt = if level.is_lgl && fmt != "decimalZero" { "decimal" } else { fmt };
                out.push_str(&format_number(v, fmt));
                continue;
            }
            out.push(c);
        }
        Some(out)
    }
}

pub fn format_number(v: i64, fmt: &str) -> String {
    match fmt {
        "decimalZero" => {
            if (0..10).contains(&v) {
                format!("0{v}")
            } else {
                v.to_string()
            }
        }
        "lowerLetter" => letters(v, b'a'),
        "upperLetter" => letters(v, b'A'),
        "lowerRoman" => roman(v).to_lowercase(),
        "upperRoman" => roman(v),
        "ordinal" => ordinal(v),
        "cardinalText" => capitalize(&cardinal(v)),
        "ordinalText" => capitalize(&ordinal_text(v)),
        "none" => String::new(),
        "numberInDash" => format!("- {v} -"),
        "hex" => format!("{v:X}"),
        "chicago" if v > 0 => ["*", "†", "‡", "§"][(v as usize - 1) % 4].repeat((v as usize - 1) / 4 + 1),
        _ => v.to_string(),
    }
}

/// a … z, aa, bb, … (the letter repeated).
fn letters(v: i64, base: u8) -> String {
    if v <= 0 {
        return String::new();
    }
    let n = (v - 1) as usize;
    let c = (base + (n % 26) as u8) as char;
    std::iter::repeat_n(c, n / 26 + 1).collect()
}

pub fn roman(mut v: i64) -> String {
    if v <= 0 {
        return v.to_string();
    }
    const T: [(i64, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut s = String::new();
    for (n, r) in T {
        while v >= n {
            s.push_str(r);
            v -= n;
        }
    }
    s
}

pub fn ordinal(v: i64) -> String {
    let suffix = match (v % 100, v % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{v}{suffix}")
}

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];

pub fn cardinal(v: i64) -> String {
    if v < 0 {
        return format!("minus {}", cardinal(-v));
    }
    if v < 20 {
        return ONES[v as usize].into();
    }
    if v < 100 {
        let t = TENS[(v / 10) as usize];
        return if v % 10 == 0 { t.into() } else { format!("{t}-{}", ONES[(v % 10) as usize]) };
    }
    for (unit, name) in [(1_000_000_000, "billion"), (1_000_000, "million"), (1000, "thousand"), (100, "hundred")] {
        if v >= unit {
            let rest = v % unit;
            let head = format!("{} {name}", cardinal(v / unit));
            return if rest == 0 { head } else { format!("{head} {}", cardinal(rest)) };
        }
    }
    v.to_string()
}

pub fn ordinal_text(v: i64) -> String {
    let c = cardinal(v);
    let (head, last) = match c.rfind([' ', '-']) {
        Some(i) => (&c[..=i], &c[i + 1..]),
        None => ("", c.as_str()),
    };
    let last = match last {
        "one" => "first".to_string(),
        "two" => "second".into(),
        "three" => "third".into(),
        "five" => "fifth".into(),
        "eight" => "eighth".into(),
        "nine" => "ninth".into(),
        "twelve" => "twelfth".into(),
        w if w.ends_with('y') => format!("{}ieth", &w[..w.len() - 1]),
        w => format!("{w}th"),
    };
    format!("{head}{last}")
}

pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(format_number(27, "lowerLetter"), "aa");
        assert_eq!(format_number(28, "upperLetter"), "BB");
        assert_eq!(format_number(14, "lowerRoman"), "xiv");
        assert_eq!(format_number(3, "decimalZero"), "03");
        assert_eq!(format_number(12, "ordinal"), "12th");
        assert_eq!(format_number(22, "ordinal"), "22nd");
        assert_eq!(format_number(1, "cardinalText"), "One");
        assert_eq!(format_number(14, "ordinalText"), "Fourteenth");
        assert_eq!(cardinal(14), "fourteen");
        assert_eq!(ordinal_text(20), "twentieth");
        assert_eq!(ordinal_text(21), "twenty-first");
    }
}
