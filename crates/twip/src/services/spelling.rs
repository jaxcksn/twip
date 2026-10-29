//! Spell checking: Hunspell dictionaries through `spellbook`, one per language, the usual skipping rules, doubled
//! words, and the session's ignored and the user's added words. A paragraph is checked in its runs' language, or the
//! session's when they have none.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use crate::editing::Editor;
use crate::editing::text::{OBJECT, UnitText};
use crate::layout::{ParaId, Unit};

/// A misspelled word in a paragraph (unit offsets); `repeated` marks a doubled word ("the the").
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpellError {
    pub start: usize,
    pub len: usize,
    pub repeated: bool,
}

impl SpellError {
    pub fn end(&self) -> usize {
        self.start + self.len
    }
}

/// A Hunspell dictionary with the session's ignored and the user's added words; results cached per paragraph text.
pub struct Speller {
    pub language: String,
    dict: spellbook::Dictionary,
    ignored: HashSet<String>,
    /// Words added with Add to Dictionary (the host persists them and passes them back with [`Speller::add_words`]).
    pub added: HashSet<String>,
    cache: RefCell<HashMap<String, Rc<[SpellError]>>>,
}

fn apostrophe(c: char) -> bool {
    c == '\'' || c == '\u{2019}'
}

fn normalize(w: &str) -> String {
    w.replace('\u{2019}', "'")
}

/// The defaults: words with digits and words in capitals are not checked.
fn skip(w: &[char]) -> bool {
    w.iter().any(|c| c.is_numeric()) || w.len() > 1 && !w.iter().any(|c| c.is_lowercase())
}

impl Speller {
    /// A checker from a Hunspell .aff/.dic pair (e.g. en_US).
    pub fn new(language: &str, aff: &[u8], dic: &[u8]) -> Result<Speller, String> {
        let (aff, dic) = (String::from_utf8_lossy(aff), String::from_utf8_lossy(dic));
        let dict = spellbook::Dictionary::new(&aff, &dic).map_err(|e| e.to_string())?;
        Ok(Speller { language: language.into(), dict, ignored: HashSet::new(), added: HashSet::new(), cache: RefCell::default() })
    }

    pub fn add_words<'a>(&mut self, words: impl IntoIterator<Item = &'a str>) {
        self.added.extend(words.into_iter().map(normalize));
        self.cache.borrow_mut().clear();
    }

    /// Ignores the word for the rest of the session (Ignore All).
    pub fn ignore(&mut self, word: &str) {
        self.ignored.insert(normalize(word));
        self.cache.borrow_mut().clear();
    }

    /// Adds the word to the user's dictionary.
    pub fn add(&mut self, word: &str) {
        self.added.insert(normalize(word));
        self.cache.borrow_mut().clear();
    }

    fn known(&self, word: &str) -> bool {
        let w = normalize(word);
        self.ignored.contains(&w) || self.added.contains(&w) || self.dict.check(&w)
    }

    /// Up to eight suggestions: swapped neighbours first ("Teh": "The"), then the dictionary's, in the word's case
    /// (a capitalised word's suggestions are capitalised).
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let w = normalize(word);
        let chars: Vec<char> = w.chars().collect();
        let mut out: Vec<String> = (1..chars.len())
            .map(|i| {
                let mut c = chars.clone();
                c.swap(i - 1, i);
                // The first letter keeps its case where it moves.
                if c[0] != chars[0] && chars[0].is_uppercase() {
                    (c[0], c[1]) = (c[0].to_uppercase().next().unwrap_or(c[0]), c[1].to_lowercase().next().unwrap_or(c[1]));
                }
                c.into_iter().collect::<String>()
            })
            .filter(|s| *s != w && self.dict.check(s))
            .collect();
        let mut more = Vec::new();
        self.dict.suggest(&w, &mut more);
        let title = chars.first().is_some_and(|c| c.is_uppercase()) && !chars[1..].iter().any(|c| c.is_uppercase());
        for s in more {
            let s = if title { s.chars().take(1).flat_map(char::to_uppercase).chain(s.chars().skip(1)).collect() } else { s };
            if !out.contains(&s) {
                out.push(s);
            }
        }
        out.truncate(8);
        out
    }

    /// Misspellings in `text` (one character per unit, objects and hidden text as spaces).
    pub fn errors(&self, text: &[char]) -> Rc<[SpellError]> {
        let key: String = text.iter().collect();
        if let Some(hit) = self.cache.borrow().get(&key) {
            return hit.clone();
        }
        let mut list = Vec::new();
        let (mut previous, mut previous_end): (Option<Vec<char>>, Option<usize>) = (None, None);
        let mut i = 0;
        while i < text.len() {
            if !text[i].is_alphanumeric() {
                i += 1;
                continue;
            }
            let start = i;
            while i < text.len() && (text[i].is_alphanumeric() || apostrophe(text[i]) && text.get(i + 1).is_some_and(|c| c.is_alphabetic())) {
                i += 1;
            }
            let word = &text[start..i];
            // The token between spaces, to skip addresses ("www.example.com", "name@store.com").
            let (mut ts, mut te) = (start, i);
            while ts > 0 && !text[ts - 1].is_whitespace() {
                ts -= 1;
            }
            while te < text.len() && !text[te].is_whitespace() {
                te += 1;
            }
            let token: String = text[ts..te].iter().collect();
            if token.contains('@') || token.contains("://") || token.to_lowercase().starts_with("www.") {
                (previous, i) = (None, te);
                continue;
            }
            let between = previous_end.is_some_and(|pe| start > pe && text[pe..start].iter().all(|&c| c == ' '));
            let same =
                previous.as_ref().is_some_and(|p| p.len() == word.len() && p.iter().zip(word).all(|(a, b)| a.to_lowercase().eq(b.to_lowercase())));
            let w: String = word.iter().collect();
            if between && same && word.iter().any(|c| c.is_alphabetic()) {
                list.push(SpellError { start, len: word.len(), repeated: true });
            } else if !skip(word) && !self.known(&w) {
                list.push(SpellError { start, len: word.len(), repeated: false });
            }
            (previous, previous_end) = (Some(word.to_vec()), Some(i));
        }
        let mut cache = self.cache.borrow_mut();
        // shortcut: the whole cache goes when large; LRU if long sessions on huge documents thrash it.
        if cache.len() > 20000 {
            cache.clear();
        }
        let list: Rc<[SpellError]> = list.into();
        cache.insert(key, list.clone());
        list
    }
}

/// A paragraph's text for checking: one character per unit, the text the markup view shows as is; hidden text,
/// deleted text (in every view), tabs, breaks and objects as spaces.
pub fn check_text(e: &Editor, para: ParaId) -> Vec<char> {
    let Some(p) = e.index().para(e.document(), para) else { return Vec::new() };
    let view = e.document().review.view;
    let t = UnitText::masked(p, |rev| rev.del.is_some() || view.hides(rev));
    t.text
        .chars()
        .enumerate()
        .map(|(u, c)| if t.is_hidden(Unit(u as u32)) || matches!(c, '\t' | '\u{2028}' | '\u{c}' | OBJECT) { ' ' } else { c })
        .collect()
}

/// The selection's paragraph and unit range, when it is in one paragraph.
pub fn selected_range(e: &Editor) -> Option<(ParaId, usize, usize)> {
    let (a, b) = e.selection().ordered(e.index())?;
    let ((pa, ua), (pb, ub)) = (a.para_offset()?, b.para_offset()?);
    (pa == pb).then_some((pa, ua.0 as usize, ub.0 as usize))
}

/// The body's paragraph ids in document order, tables included.
pub fn paras_in_order(e: &Editor) -> Vec<ParaId> {
    fn walk(blocks: &[crate::docx::Block], out: &mut Vec<ParaId>) {
        for b in blocks {
            match b {
                crate::docx::Block::Para(p) => out.push(ParaId(p.id)),
                crate::docx::Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = Vec::new();
    for s in &e.document().sections {
        walk(&s.blocks, &mut out);
    }
    out
}

/// A misspelling found in the document: where, the error, the word and the language it was checked in.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub para: ParaId,
    pub error: SpellError,
    pub word: String,
    pub language: String,
}

/// A session's spell checking: a speller per language, whether checking is on, the words ignored for the session,
/// and user words waiting for their language's dictionary.
pub struct Spellers {
    /// By language, lower case.
    by_language: BTreeMap<String, Speller>,
    pub enabled: bool,
    ignored: Vec<String>,
    pending: BTreeMap<String, Vec<String>>,
}

impl Default for Spellers {
    fn default() -> Spellers {
        Spellers { by_language: BTreeMap::new(), enabled: true, ignored: Vec::new(), pending: BTreeMap::new() }
    }
}

/// Whether a dictionary for `have` checks text in `want`: the same tag, or the same primary language.
fn same_language(have: &str, want: &str) -> bool {
    let primary = |s: &str| s.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    have.eq_ignore_ascii_case(want) || primary(have) == primary(want)
}

impl Spellers {
    /// Adds (or replaces) the dictionary for `language`, with the words ignored so far and the user's words loaded
    /// for it.
    pub fn add_dictionary(&mut self, language: &str, aff: &[u8], dic: &[u8]) -> Result<(), String> {
        let mut sp = Speller::new(language, aff, dic)?;
        let key = language.to_ascii_lowercase();
        if let Some(old) = self.by_language.remove(&key) {
            sp.added = old.added;
        }
        if let Some(words) = self.pending.remove(&key) {
            sp.add_words(words.iter().map(String::as_str));
        }
        for w in &self.ignored {
            sp.ignore(w);
        }
        self.by_language.insert(key, sp);
        Ok(())
    }

    /// The speller for text in `language`: its own, or one for the same primary language.
    pub fn speller(&self, language: &str) -> Option<&Speller> {
        self.by_language.get(&language.to_ascii_lowercase()).or_else(|| self.by_language.values().find(|s| same_language(&s.language, language)))
    }

    fn speller_mut(&mut self, language: &str) -> Option<&mut Speller> {
        let key = self
            .by_language
            .keys()
            .find(|k| k.eq_ignore_ascii_case(language))
            .or_else(|| self.by_language.iter().find(|(_, s)| same_language(&s.language, language)).map(|(k, _)| k))?;
        let key = key.clone();
        self.by_language.get_mut(&key)
    }

    /// Ignores `word` in every language for the rest of the session.
    pub fn ignore(&mut self, word: &str) {
        self.ignored.push(word.to_string());
        self.by_language.values_mut().for_each(|s| s.ignore(word));
    }

    /// Adds `word` to the user's words for `language`.
    pub fn add(&mut self, word: &str, language: &str) {
        match self.speller_mut(language) {
            Some(s) => s.add(word),
            None => self.pending.entry(language.to_ascii_lowercase()).or_default().push(word.to_string()),
        }
    }

    /// The user's words from earlier sessions, kept until `language`'s dictionary arrives.
    pub fn load_user_words(&mut self, language: &str, words: &[String]) {
        match self.speller_mut(language) {
            Some(s) => s.add_words(words.iter().map(String::as_str)),
            None => self.pending.entry(language.to_ascii_lowercase()).or_default().extend(words.iter().cloned()),
        }
    }

    pub fn suggestions(&self, word: &str, language: &str) -> Vec<String> {
        self.speller(language).map(|s| s.suggest(word)).unwrap_or_default()
    }

    /// The language `para` is checked in: its first run's that names one, else `default`.
    pub fn language_of(e: &Editor, para: ParaId, default: &str) -> String {
        let p = e.index().para(e.document(), para);
        p.and_then(|p| p.inlines.iter().filter_map(|i| i.run()).map(|r| r.lang.as_str()).find(|l| !l.is_empty())).unwrap_or(default).to_string()
    }

    /// The misspellings of `para` and the language they were checked in (none while checking is off or no dictionary
    /// has its language).
    pub fn errors(&self, e: &Editor, para: ParaId, default: &str) -> Option<(Rc<[SpellError]>, String)> {
        if !self.enabled {
            return None;
        }
        let language = Self::language_of(e, para, default);
        let sp = self.speller(&language)?;
        Some((sp.errors(&check_text(e, para)), language))
    }

    /// The misspellings in the body.
    // shortcut: every paragraph is checked per call (results are cached per paragraph text); count per changed
    // paragraph if state builds show it on large documents.
    pub fn count(&self, e: &Editor, default: &str) -> u32 {
        if !self.enabled || self.by_language.is_empty() {
            return 0;
        }
        paras_in_order(e).into_iter().filter_map(|p| self.errors(e, p, default)).map(|(errs, _)| errs.len() as u32).sum()
    }

    /// The misspelling the selection is on (within one paragraph).
    pub fn at_selection(&self, e: &Editor, default: &str) -> Option<Found> {
        let (para, a, b) = selected_range(e)?;
        let (errs, language) = self.errors(e, para, default)?;
        let error = errs.iter().copied().find(|x| a >= x.start && a <= x.end() && b <= x.end())?;
        let word = check_text(e, para)[error.start..error.end()].iter().collect();
        Some(Found { para, error, word, language })
    }

    /// The first misspelling after the selection, wrapping round the document.
    pub fn next(&self, e: &Editor, default: &str) -> Option<Found> {
        let order = paras_in_order(e);
        let n = order.len();
        let (from_para, from) =
            e.selection().ordered(e.index()).and_then(|(_, b)| b.para_offset()).map_or((order.first().copied(), 0), |(p, u)| (Some(p), u.0 as usize));
        let start = from_para.and_then(|p| order.iter().position(|q| *q == p)).unwrap_or(0);
        for k in 0..=n {
            let para = order[(start + k) % n.max(1)];
            let Some((errs, language)) = self.errors(e, para, default) else { continue };
            if let Some(error) = errs.iter().copied().find(|x| k > 0 || x.start >= from) {
                let word = check_text(e, para)[error.start..error.end()].iter().collect();
                return Some(Found { para, error, word, language });
            }
        }
        None
    }

    /// The misspellings on `pages` to underline: (page, [x0, top, x1, bottom]). The word being typed isn't flagged
    /// until the caret leaves it.
    pub fn squiggles(&self, e: &mut Editor, pages: std::ops::Range<usize>, default: &str) -> Vec<(usize, [f64; 4])> {
        if !self.enabled || self.by_language.is_empty() {
            return Vec::new();
        }
        let caret = e.selection().is_collapsed().then(|| e.selection().focus().and_then(|p| p.para_offset())).flatten();
        let mut out = Vec::new();
        for para in e.paras_on(pages.clone()) {
            let Some((errs, _)) = self.errors(e, para, default) else { continue };
            for err in errs.iter() {
                if caret == Some((para, Unit(err.end() as u32))) {
                    continue;
                }
                out.extend(e.unit_rects(para, Unit(err.start as u32), Unit(err.end() as u32), pages.clone()));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_match_by_tag_or_primary_language() {
        assert!(same_language("en-US", "en-us"));
        assert!(same_language("en-US", "en-GB"));
        assert!(!same_language("en-US", "de-DE"));
    }

    #[test]
    fn user_words_wait_for_their_dictionary() {
        let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dictionaries");
        let (Ok(aff), Ok(dic)) = (std::fs::read(d.join("en_US.aff")), std::fs::read(d.join("en_US.dic"))) else { return };
        let mut s = Spellers::default();
        s.load_user_words("en-US", &["brwon".to_string()]);
        s.ignore("Teh");
        s.add_dictionary("en-US", &aff, &dic).expect("loads");
        let sp = s.speller("en-US").expect("a speller");
        let text: Vec<char> = "Teh brwon quikc fox".chars().collect();
        let errs = sp.errors(&text);
        assert_eq!(errs.iter().map(|e| e.start).collect::<Vec<_>>(), [10], "only quikc");
        assert!(s.suggestions("quikc", "en-GB").contains(&"quick".to_string()), "a regional tag finds the dictionary");
    }
}
