//! Numbers as text in a numbering format

/// `v` written in numbering format `fmt` (`decimal`, `lowerRoman`, `cardinalText`, ...): decimal for a format
/// without its own digits.
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
        "chicago" if v > 0 && v <= REPEAT_LIMIT => ["*", "†", "‡", "§"][(v as usize - 1) % 4].repeat((v as usize - 1) / 4 + 1),
        "russianLower" => repeated(v, &chars(&[(0x430, 0x438), (0x43A, 0x43F), (0x440, 0x449), (0x44B, 0x44B), (0x44D, 0x44F)])),
        "russianUpper" => repeated(v, &chars(&[(0x410, 0x418), (0x41A, 0x41F), (0x420, 0x429), (0x42B, 0x42B), (0x42D, 0x42F)])),
        "hindiVowels" => repeated(v, &chars(&[(0x915, 0x939)])),
        "hindiConsonants" => repeated(v, &[chars(&[(0x905, 0x914)]), vec!["\u{905}\u{902}".into(), "\u{905}\u{903}".into()]].concat()),
        "thaiLetters" => repeated(v, &chars(&[(0xE01, 0xE02), (0xE04, 0xE04), (0xE07, 0xE23), (0xE25, 0xE25), (0xE27, 0xE2E)])),
        "hindiNumbers" => digits(v, 0x966),
        "thaiNumbers" => digits(v, 0xE50),
        _ => v.to_string(),
    }
}

/// The characters of the code point ranges, in order.
fn chars(ranges: &[(u32, u32)]) -> Vec<String> {
    ranges.iter().flat_map(|&(a, b)| (a..=b).filter_map(char::from_u32).map(String::from)).collect()
}

/// The set's `v`th member, written once more for each time the set was gone through.
fn repeated(v: i64, set: &[String]) -> String {
    if v <= 0 {
        return String::new();
    }
    if v > REPEAT_LIMIT {
        return v.to_string();
    }
    let n = (v - 1) as usize;
    set[n % set.len()].repeat(n / set.len() + 1)
}

/// `v` in decimal with the digits from `zero` on.
fn digits(v: i64, zero: u32) -> String {
    v.to_string().chars().map(|c| c.to_digit(10).and_then(|d| char::from_u32(zero + d)).unwrap_or(c)).collect()
}

/// Values past which letters and roman numerals are written as decimal: the value comes from the file, and each
/// repetition is another character, so `i64::MAX` would be an allocation of petabytes.
// ponytail: a safety bound, not Word's behaviour for huge list values; replace with Word's if it is ever measured.
const REPEAT_LIMIT: i64 = 1_000_000;

/// a … z, aa, bb, … (the letter repeated).
fn letters(v: i64, base: u8) -> String {
    if v <= 0 {
        return String::new();
    }
    if v > REPEAT_LIMIT {
        return v.to_string();
    }
    let n = (v - 1) as usize;
    let c = (base + (n % 26) as u8) as char;
    std::iter::repeat_n(c, n / 26 + 1).collect()
}

/// `v` in upper-case roman numerals; decimal for a value of 0 or less, or past the repetition bound.
pub fn roman(mut v: i64) -> String {
    if v <= 0 || v > REPEAT_LIMIT {
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

/// `v` with its English ordinal suffix (`1st`, `12th`, `22nd`).
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

/// `v` in English words, lower case.
pub fn cardinal(v: i64) -> String {
    // Unsigned, so `i64::MIN` has a magnitude.
    if v < 0 { format!("minus {}", cardinal_of(v.unsigned_abs())) } else { cardinal_of(v as u64) }
}

fn cardinal_of(v: u64) -> String {
    if v < 20 {
        return ONES[v as usize].into();
    }
    if v < 100 {
        let t = TENS[(v / 10) as usize];
        return if v.is_multiple_of(10) { t.into() } else { format!("{t}-{}", ONES[(v % 10) as usize]) };
    }
    for (unit, name) in [(1_000_000_000, "billion"), (1_000_000, "million"), (1000, "thousand"), (100, "hundred")] {
        if v >= unit {
            let rest = v % unit;
            let head = format!("{} {name}", cardinal_of(v / unit));
            return if rest == 0 { head } else { format!("{head} {}", cardinal_of(rest)) };
        }
    }
    v.to_string()
}

/// `v` as an English ordinal in words, lower case (`twenty-first`).
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

/// `s` with its first character upper case.
pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
