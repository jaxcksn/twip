//! numbering.xml: list definitions, counters (LIST-12) and label text (LIST-9–LIST-11).

use std::collections::HashMap;

use super::props::*;
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
    pub ppr: ParaProps,
    pub rpr: RunProps,
}

#[derive(Clone, Debug, Default)]
pub struct AbstractNum {
    pub levels: Vec<Option<Level>>,
    pub num_style_link: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Num {
    pub abstract_id: i64,
    pub start_overrides: HashMap<i64, i64>,
}

#[derive(Clone, Debug, Default)]
pub struct Numbering {
    pub abstracts: HashMap<i64, AbstractNum>,
    pub nums: HashMap<i64, Num>,
}

impl Numbering {
    pub fn parse(xml: Option<&str>) -> Numbering {
        let mut n = Numbering::default();
        let Some(doc) = xml.and_then(|x| roxmltree::Document::parse(x).ok()) else { return n };
        let root = doc.root_element();
        for a in children(root, "abstractNum") {
            let Some(id) = int(attr(a, "abstractNumId")) else { continue };
            let mut levels: Vec<Option<Level>> = vec![None; 9];
            for l in children(a, "lvl") {
                let ilvl = int(attr(l, "ilvl")).unwrap_or(0);
                if !(0..9).contains(&ilvl) {
                    continue;
                }
                let v = |name| child(l, name).and_then(val).map(str::to_string);
                levels[ilvl as usize] = Some(Level {
                    start: child(l, "start").and_then(|s| int(val(s))).unwrap_or(0),
                    fmt: v("numFmt").unwrap_or_else(|| "decimal".into()),
                    text: v("lvlText").unwrap_or_default(),
                    jc: v("lvlJc").unwrap_or_else(|| "left".into()),
                    suff: v("suff").unwrap_or_else(|| "tab".into()),
                    is_lgl: child(l, "isLgl").map(on_off).unwrap_or(false),
                    restart: child(l, "lvlRestart").and_then(|s| int(val(s))),
                    pstyle: v("pStyle"),
                    ppr: ParaProps::parse(child(l, "pPr")),
                    rpr: RunProps::parse(child(l, "rPr")),
                });
            }
            n.abstracts.insert(id, AbstractNum { levels, num_style_link: child(a, "numStyleLink").and_then(val).map(str::to_string) });
        }
        for m in children(root, "num") {
            let Some(id) = int(attr(m, "numId")) else { continue };
            let mut num = Num { abstract_id: child(m, "abstractNumId").and_then(|a| int(val(a))).unwrap_or(-1), ..Default::default() };
            for o in children(m, "lvlOverride") {
                if let (Some(il), Some(s)) = (int(attr(o, "ilvl")), child(o, "startOverride").and_then(|s| int(val(s)))) {
                    num.start_overrides.insert(il, s);
                }
            }
            n.nums.insert(id, num);
        }
        n
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
        let a = self.abstract_of(num_id, styles)?;
        self.abstracts.get(&a)?.levels.get(ilvl.clamp(0, 8) as usize)?.as_ref()
    }
}

/// List counters in document order (LIST-12): per abstractNum, per level.
#[derive(Default)]
pub struct Counters {
    values: HashMap<i64, [Option<i64>; 9]>,
    overrides_applied: std::collections::HashSet<(i64, i64)>,
}

impl Counters {
    /// Advances the counter for an item at `ilvl` of `num_id` and returns the label text.
    pub fn next(&mut self, numbering: &Numbering, styles: &super::styles::Styles, num_id: i64, ilvl: i64) -> Option<String> {
        let a = numbering.abstract_of(num_id, styles)?;
        let abs = numbering.abstracts.get(&a)?;
        let ilvl = ilvl.clamp(0, 8) as usize;
        let level = abs.levels[ilvl].as_ref()?;
        let vals = self.values.entry(a).or_insert([None; 9]);
        let over = numbering.nums.get(&num_id).and_then(|n| n.start_overrides.get(&(ilvl as i64)));
        vals[ilvl] = match over {
            Some(&s) if self.overrides_applied.insert((num_id, ilvl as i64)) => Some(s),
            _ => Some(vals[ilvl].map_or(level.start, |v| v + 1)),
        };
        for k in ilvl + 1..9 {
            let restart = abs.levels[k].as_ref().and_then(|l| l.restart);
            let reset = match restart {
                None => true,
                Some(0) => false,
                Some(n) => (ilvl as i64) < n,
            };
            if reset {
                vals[k] = None;
            }
        }
        // A level shown before its parents appeared counts from the parents' start values (LIST-12: "1.1.", then "2.").
        for k in 0..ilvl {
            if vals[k].is_none() {
                vals[k] = Some(abs.levels[k].as_ref().map_or(1, |l| l.start));
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
                let fmt = if level.is_lgl { "decimal" } else { abs.levels[k].as_ref().map_or("decimal", |l| l.fmt.as_str()) };
                out.push_str(&format_number(v, fmt));
                continue;
            }
            out.push(c);
        }
        Some(out)
    }
}

/// A number in an OOXML number format (LIST-9, FIELD-3).
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
