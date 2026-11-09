//! Hyphenation patterns

use std::collections::HashMap;
use std::rc::Rc;

const LEFT: usize = 2;

/// One language's hyphenation patterns (TeX patterns, as in a `hyph_*.dic` file).
#[derive(Clone, Debug, Default)]
pub struct Patterns {
    map: HashMap<Box<[char]>, Box<[u8]>>,
    longest: usize,
}

impl Patterns {
    pub fn parse(text: &str) -> Patterns {
        let mut p = Patterns::default();
        for (k, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('%') || line.starts_with('#') || k == 0 && !line.chars().any(|c| c.is_ascii_digit()) {
                continue;
            }
            let mut words = line.split_whitespace();
            match words.next() {
                Some("LEFTHYPHENMIN" | "RIGHTHYPHENMIN") => {}
                Some("NEXTLEVEL") => break,
                Some(w) if w.chars().all(|c| c.is_alphabetic() || c.is_ascii_digit() || c == '.') => {
                    for w in std::iter::once(w).chain(words) {
                        p.add(w);
                    }
                }
                _ => {}
            }
        }
        p
    }

    fn add(&mut self, pattern: &str) {
        let mut letters = Vec::new();
        let mut values = vec![0u8];
        for c in pattern.chars() {
            match c.to_digit(10) {
                Some(d) => *values.last_mut().expect("one value per gap") = d as u8,
                None => {
                    letters.extend(c.to_lowercase());
                    values.push(0);
                }
            }
        }
        self.longest = self.longest.max(letters.len());
        self.map.insert(letters.into(), values.into());
    }

    /// The char offsets inside `word` after which it may break, leaving at least two letters before the break and
    /// `right` after it.
    pub fn points(&self, word: &str, right: usize) -> Vec<usize> {
        let chars: Vec<char> = word.chars().flat_map(char::to_lowercase).collect();
        let n = chars.len();
        if n < LEFT + right {
            return Vec::new();
        }
        let w: Vec<char> = std::iter::once('.').chain(chars).chain(std::iter::once('.')).collect();
        let mut levels = vec![0u8; w.len() + 1];
        for i in 0..w.len() {
            for j in i + 1..=(i + self.longest).min(w.len()) {
                if let Some(v) = self.map.get(&w[i..j]) {
                    for (k, &d) in v.iter().enumerate() {
                        levels[i + k] = levels[i + k].max(d);
                    }
                }
            }
        }
        (LEFT..=n - right).filter(|&p| levels[p + 1] % 2 == 1).collect()
    }
}

/// The patterns the host supplied, by language.
#[derive(Clone, Debug, Default)]
pub struct Hyphenation(pub Rc<HashMap<String, Patterns>>);

impl PartialEq for Hyphenation {
    fn eq(&self, o: &Hyphenation) -> bool {
        Rc::ptr_eq(&self.0, &o.0)
    }
}

impl Hyphenation {
    /// The patterns for a BCP 47 language (`en-US`), else for its primary language (`en`).
    pub fn for_language(&self, lang: &str) -> Option<&Patterns> {
        let lang = lang.to_ascii_lowercase();
        self.0.get(&lang).or_else(|| self.0.get(lang.split(['-', '_']).next()?))
    }

    /// These patterns with `patterns` for `lang` added.
    pub fn with(&self, lang: &str, patterns: Patterns) -> Hyphenation {
        let mut map = (*self.0).clone();
        map.insert(lang.to_ascii_lowercase(), patterns);
        Hyphenation(Rc::new(map))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_give_the_odd_levels_between_the_minimums() {
        let p = Patterns::parse("UTF-8\nLEFTHYPHENMIN 2\nRIGHTHYPHENMIN 3\nhy3ph\nhe2n\nhena4\nhen5at\n1na\nn2at\n1tio\n2io\no2n");
        assert_eq!(p.points("hyphenation", 3), vec![2, 6]);
        assert!(p.points("hyp", 2).is_empty());
        let h = Hyphenation::default().with("en-US", p);
        assert!(h.for_language("en-us").is_some() && h.for_language("EN").is_none());
        assert!(Hyphenation::default().with("en", Patterns::default()).for_language("en-GB").is_some());
    }

    #[test]
    fn the_caller_sets_the_letters_after_a_break() {
        let p = Patterns::parse("1ty");
        assert_eq!(p.points("pity", 2), vec![2]);
        assert!(p.points("pity", 3).is_empty());
    }
}
