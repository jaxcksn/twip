//! `=` formula fields

/// The numbers a formula reads: its table's cells (by row, then grid column) and where it is, and bookmarks.
pub struct Env<'a> {
    /// `w:decimalSymbol`.
    pub decimal: char,
    /// `w:listSeparator`.
    pub list: char,
    /// The text of each cell of the formula's table, and the formula's cell (row, column).
    pub table: Option<(&'a [Vec<String>], (usize, usize))>,
    /// A bookmark's text.
    pub bookmark: &'a dyn Fn(&str) -> Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Name(String),
    Op(&'static str),
    Open,
    Close,
    Sep,
    Colon,
}

/// Word's error texts.
const SYNTAX: &str = "!Syntax Error";
const END: &str = "!Unexpected End of Formula";
const ZERO: &str = "!Zero Divide";

/// Nesting beyond this is refused rather than recursed into.
const MAX_DEPTH: usize = 64;

fn lex(s: &str, env: &Env) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let c: Vec<char> = s.chars().collect();
    let mut k = 0;
    while k < c.len() {
        let ch = c[k];
        match ch {
            _ if ch.is_whitespace() => k += 1,
            _ if ch.is_ascii_digit() || (ch == env.decimal && c.get(k + 1).is_some_and(char::is_ascii_digit)) => {
                let start = k;
                while k < c.len() && (c[k].is_ascii_digit() || c[k] == env.decimal) {
                    k += 1;
                }
                let text: String = c[start..k].iter().map(|&d| if d == env.decimal { '.' } else { d }).collect();
                out.push(Tok::Num(text.parse().map_err(|_| format!("{SYNTAX}, {text}"))?));
            }
            _ if ch.is_alphabetic() || ch == '_' => {
                let start = k;
                while k < c.len() && (c[k].is_alphanumeric() || c[k] == '_') {
                    k += 1;
                }
                out.push(Tok::Name(c[start..k].iter().collect()));
            }
            '(' => (out.push(Tok::Open), k += 1).1,
            ')' => (out.push(Tok::Close), k += 1).1,
            ':' => (out.push(Tok::Colon), k += 1).1,
            _ if ch == env.list => (out.push(Tok::Sep), k += 1).1,
            _ => {
                let two: String = c[k..(k + 2).min(c.len())].iter().collect();
                let op = ["<=", ">=", "<>"].into_iter().find(|o| *o == two);
                let op = op.or_else(|| ["+", "-", "*", "/", "^", "%", "=", "<", ">"].into_iter().find(|o| o.starts_with(ch)));
                let Some(op) = op else { return Err(format!("{SYNTAX}, {ch}")) };
                k += op.len();
                out.push(Tok::Op(op));
            }
        }
    }
    Ok(out)
}

struct Parser<'a, 'e> {
    toks: Vec<Tok>,
    at: usize,
    depth: usize,
    env: &'a Env<'e>,
}

/// A cell reference's (row, column), from `A1` or `R1C1`.
fn cell(name: &str) -> Option<(usize, usize)> {
    let up = name.to_ascii_uppercase();
    if let Some(rest) = up.strip_prefix('R')
        && let Some((r, c)) = rest.split_once('C')
        && let (Ok(r), Ok(c)) = (r.parse::<usize>(), c.parse::<usize>())
    {
        return (r > 0 && c > 0).then(|| (r - 1, c - 1));
    }
    let letters = up.chars().take_while(char::is_ascii_uppercase).count();
    let (col, row) = up.split_at(letters);
    if letters == 0 || letters > 2 || row.is_empty() {
        return None;
    }
    let row: usize = row.parse().ok()?;
    let col = col.bytes().fold(0usize, |n, b| n * 26 + usize::from(b - b'A' + 1));
    (row > 0).then(|| (row - 1, col - 1))
}

/// The first number in a cell's or bookmark's text (grouping and currency symbols skipped), if it has one.
fn number_in(text: &str, decimal: char) -> Option<f64> {
    let group = if decimal == ',' { '.' } else { ',' };
    let c: Vec<char> = text.chars().collect();
    let start = c.iter().position(|ch| ch.is_ascii_digit())?;
    let negative = c[..start].iter().rev().find(|ch| !ch.is_whitespace()).is_some_and(|ch| *ch == '-' || *ch == '(');
    let mut n = String::new();
    for &ch in &c[start..] {
        match ch {
            _ if ch.is_ascii_digit() => n.push(ch),
            _ if ch == decimal && !n.contains('.') => n.push('.'),
            _ if ch == group => {}
            _ => break,
        }
    }
    n.parse::<f64>().ok().map(|v| if negative { -v } else { v })
}

impl Parser<'_, '_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn next(&mut self) -> Result<Tok, String> {
        let t = self.toks.get(self.at).cloned().ok_or_else(|| END.to_string())?;
        self.at += 1;
        Ok(t)
    }

    fn deeper(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH { Err(SYNTAX.into()) } else { Ok(()) }
    }

    fn expr(&mut self) -> Result<f64, String> {
        self.deeper()?;
        let a = self.sum()?;
        let v = match self.peek() {
            Some(Tok::Op(op @ ("=" | "<>" | "<" | "<=" | ">" | ">="))) => {
                let op = *op;
                self.at += 1;
                let b = self.sum()?;
                let t = match op {
                    "=" => a == b,
                    "<>" => a != b,
                    "<" => a < b,
                    "<=" => a <= b,
                    ">" => a > b,
                    _ => a >= b,
                };
                f64::from(u8::from(t))
            }
            _ => a,
        };
        self.depth -= 1;
        Ok(v)
    }

    fn sum(&mut self) -> Result<f64, String> {
        let mut a = self.product()?;
        while let Some(Tok::Op(op @ ("+" | "-"))) = self.peek() {
            let plus = *op == "+";
            self.at += 1;
            let b = self.product()?;
            a = if plus { a + b } else { a - b };
        }
        Ok(a)
    }

    fn product(&mut self) -> Result<f64, String> {
        let mut a = self.power()?;
        while let Some(Tok::Op(op @ ("*" | "/"))) = self.peek() {
            let times = *op == "*";
            self.at += 1;
            let b = self.power()?;
            if !times && b == 0.0 {
                return Err(ZERO.into());
            }
            a = if times { a * b } else { a / b };
        }
        Ok(a)
    }

    fn power(&mut self) -> Result<f64, String> {
        let a = self.unary()?;
        if let Some(Tok::Op("^")) = self.peek() {
            self.at += 1;
            self.deeper()?;
            let b = self.power()?;
            self.depth -= 1;
            return Ok(a.powf(b));
        }
        Ok(a)
    }

    fn unary(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some(Tok::Op(op @ ("-" | "+"))) => {
                let neg = *op == "-";
                self.at += 1;
                self.deeper()?;
                let v = self.unary()?;
                self.depth -= 1;
                Ok(if neg { -v } else { v })
            }
            _ => {
                let v = self.primary()?;
                if let Some(Tok::Op("%")) = self.peek() {
                    self.at += 1;
                    return Ok(v / 100.0);
                }
                Ok(v)
            }
        }
    }

    fn primary(&mut self) -> Result<f64, String> {
        match self.next()? {
            Tok::Num(n) => Ok(n),
            Tok::Open => {
                let v = self.expr()?;
                match self.next()? {
                    Tok::Close => Ok(v),
                    t => Err(format!("{SYNTAX}, {}", show(&t))),
                }
            }
            Tok::Name(name) if self.peek() == Some(&Tok::Open) => {
                self.at += 1;
                let args = self.args()?;
                function(&name, &args)
            }
            Tok::Name(name) => match name.to_ascii_uppercase().as_str() {
                "TRUE" => Ok(1.0),
                "FALSE" => Ok(0.0),
                _ => {
                    if let Some(at) = cell(&name)
                        && let Some((rows, _)) = self.env.table
                    {
                        return Ok(self.cell_value(rows, at).unwrap_or(0.0));
                    }
                    let text = (self.env.bookmark)(&name).ok_or_else(|| format!("!Undefined Bookmark, {name}"))?;
                    Ok(number_in(&text, self.env.decimal).unwrap_or(0.0))
                }
            },
            t => Err(format!("{SYNTAX}, {}", show(&t))),
        }
    }

    fn cell_value(&self, rows: &[Vec<String>], (r, c): (usize, usize)) -> Option<f64> {
        rows.get(r).and_then(|row| row.get(c)).and_then(|t| number_in(t, self.env.decimal))
    }

    /// A function's arguments: expressions, cell ranges and LEFT, RIGHT, ABOVE or BELOW, each its numbers.
    fn args(&mut self) -> Result<Vec<Vec<f64>>, String> {
        let mut out = Vec::new();
        if self.peek() == Some(&Tok::Close) {
            self.at += 1;
            return Ok(out);
        }
        loop {
            out.push(self.arg()?);
            match self.next()? {
                Tok::Sep => {}
                Tok::Close => return Ok(out),
                t => return Err(format!("{SYNTAX}, {}", show(&t))),
            }
        }
    }

    fn arg(&mut self) -> Result<Vec<f64>, String> {
        if let (Some(Tok::Name(name)), Some((rows, here))) = (self.peek().cloned(), self.env.table) {
            let next = self.toks.get(self.at + 1);
            let edge = matches!(next, Some(Tok::Sep | Tok::Close)).then(|| name.to_ascii_uppercase());
            if let Some(dir @ ("LEFT" | "RIGHT" | "ABOVE" | "BELOW")) = edge.as_deref() {
                self.at += 1;
                return Ok(self.toward(rows, here, dir));
            }
            if next == Some(&Tok::Colon)
                && let (Some(a), Some(Tok::Name(to))) = (cell(&name), self.toks.get(self.at + 2))
                && let Some(b) = cell(to)
            {
                self.at += 3;
                let mut v = Vec::new();
                for r in a.0.min(b.0)..=a.0.max(b.0) {
                    for c in a.1.min(b.1)..=a.1.max(b.1) {
                        v.extend(self.cell_value(rows, (r, c)));
                    }
                }
                return Ok(v);
            }
        }
        Ok(vec![self.expr()?])
    }

    /// The numbers from the formula's cell toward an edge, up to the first cell without one after a cell with one.
    fn toward(&self, rows: &[Vec<String>], (r, c): (usize, usize), dir: &str) -> Vec<f64> {
        let cells: Vec<(usize, usize)> = match dir {
            "LEFT" => (0..c).rev().map(|k| (r, k)).collect(),
            "RIGHT" => (c + 1..rows.get(r).map_or(0, Vec::len)).map(|k| (r, k)).collect(),
            "ABOVE" => (0..r).rev().map(|k| (k, c)).collect(),
            _ => (r + 1..rows.len()).map(|k| (k, c)).collect(),
        };
        let mut out = Vec::new();
        for at in cells {
            match self.cell_value(rows, at) {
                Some(v) => out.push(v),
                None if !out.is_empty() => break,
                None => {}
            }
        }
        out
    }
}

fn show(t: &Tok) -> String {
    match t {
        Tok::Num(n) => n.to_string(),
        Tok::Name(n) => n.clone(),
        Tok::Op(o) => (*o).into(),
        Tok::Open => "(".into(),
        Tok::Close => ")".into(),
        Tok::Sep => ",".into(),
        Tok::Colon => ":".into(),
    }
}

fn function(name: &str, args: &[Vec<f64>]) -> Result<f64, String> {
    let all: Vec<f64> = args.iter().flatten().copied().collect();
    let one = |k: usize| args.get(k).and_then(|a| a.first().copied()).ok_or_else(|| END.to_string());
    let truth = |v: f64| f64::from(u8::from(v != 0.0));
    Ok(match name.to_ascii_uppercase().as_str() {
        "SUM" => all.iter().sum(),
        "PRODUCT" => all.iter().product(),
        "AVERAGE" if all.is_empty() => return Err(ZERO.into()),
        "AVERAGE" => all.iter().sum::<f64>() / all.len() as f64,
        "COUNT" => all.len() as f64,
        "MIN" => all.iter().copied().reduce(f64::min).unwrap_or(0.0),
        "MAX" => all.iter().copied().reduce(f64::max).unwrap_or(0.0),
        "ABS" => one(0)?.abs(),
        "INT" => one(0)?.trunc(),
        "SIGN" => {
            let v = one(0)?;
            if v == 0.0 { 0.0 } else { v.signum() }
        }
        "MOD" if one(1)? == 0.0 => return Err(ZERO.into()),
        "MOD" => one(0)? % one(1)?,
        "ROUND" => {
            let p = 10f64.powi(one(1)?.clamp(-15.0, 15.0) as i32);
            (one(0)? * p).round() / p
        }
        "NOT" => truth(f64::from(u8::from(one(0)? == 0.0))),
        "AND" => truth(f64::from(u8::from(one(0)? != 0.0 && one(1)? != 0.0))),
        "OR" => truth(f64::from(u8::from(one(0)? != 0.0 || one(1)? != 0.0))),
        "IF" => {
            if one(0)? != 0.0 {
                one(1)?
            } else {
                one(2)?
            }
        }
        "TRUE" => 1.0,
        "FALSE" => 0.0,
        _ => return Err(format!("{SYNTAX}, {name}")),
    })
}

/// A formula's value, or Word's error text for it.
pub fn evaluate(expr: &str, env: &Env) -> Result<f64, String> {
    let toks = lex(expr, env)?;
    if toks.is_empty() {
        return Err(END.into());
    }
    let mut p = Parser { toks, at: 0, depth: 0, env };
    let v = p.expr()?;
    match p.peek() {
        None => Ok(v),
        Some(t) => Err(format!("{SYNTAX}, {}", show(t))),
    }
}

/// A number in a `\#` picture (`0` a digit, `#` a digit or a space, `.` the decimal symbol, `,` grouping, `x` a digit
/// that cuts those left of it; anything else, or text in single quotes, as it is; `positive;negative;zero`).
pub fn picture(v: f64, pic: &str, decimal: char) -> String {
    let group = if decimal == ',' { '.' } else { ',' };
    let sections: Vec<&str> = pic.split(';').collect();
    let (sec, signed) = match (v < 0.0, v == 0.0, sections.len()) {
        (true, _, 2..) => (sections[1], false),
        (_, true, 3..) => (sections[2], false),
        (true, ..) => (sections[0], true),
        _ => (sections[0], false),
    };
    let chars: Vec<char> = sec.chars().collect();
    let digit = |c: char| matches!(c, '0' | '#' | 'x');
    let point = chars.iter().position(|c| *c == '.');
    let (int_pic, frac_pic) = point.map_or((&chars[..], &[][..]), |p| (&chars[..p], &chars[p + 1..]));
    let places = frac_pic.iter().filter(|c| digit(**c)).count();
    let rounded = format!("{:.*}", places, v.abs());
    let (int_digits, frac_digits) = rounded.split_once('.').unwrap_or((&rounded, ""));
    // The integer placeholders' digits as one run: missing ones `0` or a space, extra ones before the first.
    let slots: Vec<char> = int_pic.iter().copied().filter(|c| digit(*c)).collect();
    let mut digits: Vec<char> = int_digits.trim_start_matches('0').chars().collect();
    if let Some(x) = slots.iter().position(|c| *c == 'x') {
        let cut = digits.len().saturating_sub(slots.len() - x);
        digits.drain(..cut);
    }
    let pad = slots.len().saturating_sub(digits.len());
    let mut run: Vec<char> = slots[..pad].iter().map(|c| if *c == '#' { ' ' } else { '0' }).collect();
    run.extend(digits);
    if int_pic.contains(&',') {
        let n = run.iter().filter(|c| c.is_ascii_digit()).count();
        let mut grouped = Vec::with_capacity(run.len() + n / 3);
        let mut left = n;
        for c in run {
            grouped.push(c);
            if c.is_ascii_digit() {
                left -= 1;
                if left > 0 && left % 3 == 0 {
                    grouped.push(group);
                }
            }
        }
        run = grouped;
    }
    // Text before the first placeholder and after the last, as it is.
    let literal = |cs: &[char], quoted: &mut bool, out: &mut String| {
        for &c in cs {
            if c == '\'' {
                *quoted = !*quoted;
            } else if *quoted || (!digit(c) && c != ',') {
                out.push(c);
            }
        }
    };
    let mut out = String::new();
    if signed {
        out.push('-');
    }
    let mut quoted = false;
    let first = int_pic.iter().position(|c| digit(*c)).unwrap_or(int_pic.len());
    let last = int_pic.iter().rposition(|c| digit(*c)).map_or(int_pic.len(), |k| k + 1);
    literal(&int_pic[..first], &mut quoted, &mut out);
    out.extend(run);
    literal(&int_pic[last.max(first)..], &mut quoted, &mut out);
    if point.is_some() {
        out.push(decimal);
        let mut fd = frac_digits.chars();
        for &c in frac_pic {
            if c == '\'' {
                quoted = !quoted;
                continue;
            }
            match (quoted, c) {
                (false, '0' | 'x') => out.push(fd.next().unwrap_or('0')),
                (false, '#') => {
                    if let Some(d) = fd.next() {
                        out.push(d);
                    }
                }
                _ => out.push(c),
            }
        }
    }
    out
}

/// A formula's value without a picture: at most two decimal places, without trailing zeros.
pub fn general(v: f64, decimal: char) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    let s = if s == "-0" { "0" } else { s };
    s.replace('.', &decimal.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(rows: &'a [Vec<String>], at: (usize, usize), bookmark: &'a dyn Fn(&str) -> Option<String>) -> Env<'a> {
        Env { decimal: '.', list: ',', table: Some((rows, at)), bookmark }
    }

    #[test]
    fn formulas_compute_as_word_does() {
        let none = |_: &str| None;
        let rows: Vec<Vec<String>> = [["Item", "Price"], ["Pens", "$1,200.50"], ["Ink", "3"], ["Total", ""]]
            .iter()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .collect();
        let e = env(&rows, (3, 1), &none);
        assert_eq!(evaluate("SUM(ABOVE)", &e), Ok(1203.5));
        assert_eq!(evaluate("B2+B3*2", &e), Ok(1206.5));
        assert_eq!(evaluate("AVERAGE(B2:B3)", &e), Ok(601.75));
        assert_eq!(evaluate("IF(B3>2,10,20)", &e), Ok(10.0));
        assert_eq!(evaluate("-2^2 + 50%", &e), Ok(4.5));
        assert_eq!(evaluate("1/0", &e), Err(ZERO.into()));
        assert_eq!(evaluate("2+", &e), Err(END.into()));
        assert_eq!(evaluate("Price", &e), Err("!Undefined Bookmark, Price".into()));
        let deep = format!("{}1{}", "(".repeat(200), ")".repeat(200));
        assert_eq!(evaluate(&deep, &e), Err(SYNTAX.into()));
        let mark = |n: &str| (n == "Rate").then(|| "7,5 %".to_string());
        let semi = Env { decimal: ',', list: ';', table: None, bookmark: &mark };
        assert_eq!(evaluate("ROUND(Rate*2;0)+1,5", &semi), Ok(16.5));
    }

    #[test]
    fn pictures_and_the_general_format() {
        assert_eq!(picture(1234.5, "#,##0.00", '.'), "1,234.50");
        assert_eq!(picture(-1234.5, "$#,##0.00;($#,##0.00)", '.'), "($1,234.50)");
        assert_eq!(picture(0.0, "0.0;-0.0;'none'", '.'), "none");
        assert_eq!(picture(7.0, "000", '.'), "007");
        assert_eq!(picture(15.0, "$###", '.'), "$ 15");
        assert_eq!(picture(1234567.0, "#,##0", ','), "1.234.567");
        assert_eq!(picture(-3.0, "0", '.'), "-3");
        assert_eq!(general(1.0 / 3.0, '.'), "0.33");
        assert_eq!(general(2.5, ','), "2,5");
        assert_eq!(general(4.0, '.'), "4");
    }
}
