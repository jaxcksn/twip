//! Spell checking: Hunspell dictionaries through `spellbook`, one per language, the usual skipping rules, doubled
//! words, and the session's ignored and the user's added words. Each run is checked in its language, or the
//! session's when it has none.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use crate::editing::text::{OBJECT, Seen};
use crate::editing::{EditMark, Editor};
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

/// An address: a mail address, a URL or a `www.` name.
fn address(token: &[char]) -> bool {
    token.contains(&'@')
        || token.windows(3).any(|w| w == [':', '/', '/'])
        || token.len() >= 4 && token[..3].iter().all(|c| matches!(c, 'w' | 'W')) && token[3] == '.'
}

/// The defaults: words with digits and words in capitals are not checked.
fn skip(w: &[char]) -> bool {
    w.iter().any(|c| c.is_numeric()) || w.len() > 1 && w.iter().any(|c| c.is_uppercase()) && !w.iter().any(|c| c.is_lowercase())
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

    /// Misspellings in `text`, by character.
    pub fn errors(&self, text: &[char]) -> Rc<[SpellError]> {
        let key: String = text.iter().collect();
        if let Some(hit) = self.cache.borrow().get(&key) {
            return hit.clone();
        }
        let mut list = Vec::new();
        // Characters that go on the grapheme before them (combining marks): part of its word.
        let extends: Vec<bool> = if key.is_ascii() { Vec::new() } else { continuations(&key, text.len()) };
        let extends = |i: usize| extends.get(i).copied().unwrap_or(false);
        let (mut previous, mut previous_end): (Option<std::ops::Range<usize>>, Option<usize>) = (None, None);
        // The token between spaces the last word was in: its end.
        let mut token_end = 0;
        let mut w = String::new();
        let mut i = 0;
        while i < text.len() {
            if !text[i].is_alphanumeric() {
                i += 1;
                continue;
            }
            let start = i;
            while i < text.len()
                && (text[i].is_alphanumeric() || extends(i) || apostrophe(text[i]) && text.get(i + 1).is_some_and(|c| c.is_alphabetic()))
            {
                i += 1;
            }
            let word = &text[start..i];
            // The token between spaces, to skip addresses ("www.example.com", "name@store.com"): looked at once.
            if start >= token_end {
                let ts = text[..start].iter().rposition(|c| c.is_whitespace()).map_or(0, |k| k + 1);
                token_end = text[i..].iter().position(|c| c.is_whitespace()).map_or(text.len(), |k| i + k);
                if address(&text[ts..token_end]) {
                    (previous, i) = (None, token_end);
                    continue;
                }
            }
            let between = previous_end.is_some_and(|pe| start > pe && text[pe..start].iter().all(|&c| c == ' '));
            let same = previous
                .as_ref()
                .is_some_and(|p| p.len() == word.len() && text[p.clone()].iter().zip(word).all(|(a, b)| a.to_lowercase().eq(b.to_lowercase())));
            w.clear();
            w.extend(word);
            if between && same && word.iter().any(|c| c.is_alphabetic()) {
                list.push(SpellError { start, len: word.len(), repeated: true });
            } else if !skip(word) && !self.known(&w) {
                list.push(SpellError { start, len: word.len(), repeated: false });
            }
            (previous, previous_end) = (Some(start..i), Some(i));
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

/// Whether each of the `n` characters of `text` goes on the grapheme before it.
fn continuations(text: &str, n: usize) -> Vec<bool> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut out = vec![true; n];
    let mut k = 0;
    for g in text.graphemes(true) {
        if let Some(x) = out.get_mut(k) {
            *x = false;
        }
        k += g.chars().count();
    }
    out
}

/// A paragraph's text for checking, as it reads: the text the markup view shows, fields as their results; hidden
/// and deleted text (in every view) left out, so a word with a deletion inside reads whole. Tabs, breaks, objects and
/// text marked Do not check spelling (`w:noProof`) read as line ends, which part words and aren't spaces.
pub fn check_text(e: &Editor, para: ParaId) -> Seen {
    let Some(p) = e.index().para(e.document(), para) else { return Seen::default() };
    let view = e.review().view;
    let mut t = Seen::of(p, |r| !(r.hidden || r.rev.del.is_some() || view.hides(&r.rev)), false);
    let mut skip = Vec::with_capacity(t.units.last().map_or(0, |u| *u as usize + 1));
    for i in &p.inlines {
        skip.extend(std::iter::repeat_n(i.run().is_some_and(|r| r.no_proof), i.units()));
    }
    for (c, u) in t.chars.iter_mut().zip(&t.units) {
        if skip.get(*u as usize).copied().unwrap_or(false) || matches!(*c, '\t' | '\u{2028}' | '\u{c}' | OBJECT) {
            *c = '\n';
        }
    }
    t
}

/// `errors` (by character of `text`) by unit.
fn by_unit(errors: Rc<[SpellError]>, text: &Seen) -> Rc<[SpellError]> {
    if text.units.iter().enumerate().all(|(k, u)| *u as usize == k) {
        return errors;
    }
    errors
        .iter()
        .map(|x| {
            let (a, b) = text.units_of(x.start, x.end());
            SpellError { start: a.0 as usize, len: (b.0 - a.0) as usize, ..*x }
        })
        .collect()
}

/// The word an error (by unit) is on, as it reads.
fn word_of(e: &Editor, para: ParaId, x: &SpellError) -> String {
    check_text(e, para).text_in(Unit(x.start as u32), Unit(x.end() as u32)).collect()
}

/// The selection's paragraph and unit range, when it is in one paragraph.
pub fn selected_range(e: &Editor) -> Option<(ParaId, usize, usize)> {
    let (a, b) = e.selection().ordered(e.index())?;
    let ((pa, ua), (pb, ub)) = (a.para_offset()?, b.para_offset()?);
    (pa == pb).then_some((pa, ua.0 as usize, ub.0 as usize))
}

/// The longest word [`word_at_selection`] gives, characters.
pub const MAX_WORD: usize = 64;

/// The word the selection is in: its paragraph, units, text and language.
#[derive(Clone, Debug, PartialEq)]
pub struct WordAt {
    /// The paragraph the word is in.
    pub para: ParaId,
    /// The word's first unit.
    pub start: Unit,
    /// The unit after the word's last.
    pub end: Unit,
    /// The word's text.
    pub word: String,
    /// The word's language (`default` for text without one).
    pub language: String,
}

/// The word the selection is in or selects, when the selection is in one paragraph and within one word: letters,
/// digits and apostrophes and hyphens between them, at most [`MAX_WORD`] characters.
pub fn word_at_selection(e: &Editor, default: &str) -> Option<WordAt> {
    let (para, a, b) = selected_range(e)?;
    let text = check_text(e, para);
    let k = text.units.partition_point(|&u| (u as usize) < a);
    // Letters and digits, and what goes on the grapheme of one (combining marks), as `Speller::errors` reads them.
    let extends = match text.chars.iter().all(char::is_ascii) {
        true => Vec::new(),
        false => continuations(&text.chars.iter().collect::<String>(), text.chars.len()),
    };
    let mut letters = Vec::with_capacity(text.chars.len());
    for (i, c) in text.chars.iter().enumerate() {
        let on = c.is_alphanumeric() || extends.get(i).copied().unwrap_or(false) && i > 0 && letters[i - 1];
        letters.push(on);
    }
    let part = |i: usize| letters.get(i).copied().unwrap_or(false);
    let joins = |i: usize| matches!(text.chars.get(i), Some('\'' | '\u{2019}' | '-')) && i > 0 && part(i - 1) && part(i + 1);
    let inside = |i: usize| part(i) || joins(i);
    let mut start = k;
    while start > 0 && k - start <= MAX_WORD && inside(start - 1) {
        start -= 1;
    }
    let mut end = k;
    while end < text.chars.len() && end - start <= MAX_WORD && inside(end) {
        end += 1;
    }
    if start == end || end - start > MAX_WORD {
        return None;
    }
    let (us, ue) = text.units_of(start, end);
    if b > ue.0 as usize {
        return None;
    }
    let word: String = text.chars[start..end].iter().collect();
    let language = Spellers::language_at(e, para, us.0 as usize, default);
    Some(WordAt { para, start: us, end: ue, word, language })
}

/// The body's paragraph ids in document order, tables included.
pub fn paras_in_order(e: &Editor) -> Vec<ParaId> {
    e.story_paras(crate::editing::find::Stories::default())
}

/// The paragraph ids of every story the canvas edits, the body's first: what spelling and Find go through.
pub fn searched_paras(e: &Editor) -> Vec<ParaId> {
    e.story_paras(crate::editing::find::Stories::ALL)
}

/// Whether `para` is one [`searched_paras`] lists: the sub-template's being edited (its text boxes too), else the
/// body's, the headers' and footers' and the notes'.
fn searched(e: &Editor, para: ParaId, sub_template: Option<u32>) -> bool {
    use crate::editing::position::StoryId;
    let Some(slot) = e.index().para_slot(para) else { return false };
    match (e.index().host(e.index().story(slot.container)), sub_template) {
        (StoryId::BuildingBlock(k), Some(s)) => k == s,
        (StoryId::Body | StoryId::HeaderFooter(_) | StoryId::Footnote(_) | StoryId::Endnote(_), None) => true,
        _ => false,
    }
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
    /// Counts the dictionaries added: a replaced one can hold the same number of words and others.
    dictionaries: u64,
    tally: Tally,
}

/// The misspellings per paragraph of every story [`searched_paras`] lists (only paragraphs that have some), and the
/// checking context and edit mark they were counted at.
#[derive(Default)]
struct Tally {
    at: Option<(u64, EditMark)>,
    by_para: HashMap<ParaId, u32>,
    total: u32,
}

impl Default for Spellers {
    fn default() -> Spellers {
        Spellers {
            by_language: BTreeMap::new(),
            enabled: true,
            ignored: Vec::new(),
            pending: BTreeMap::new(),
            dictionaries: 0,
            tally: Tally::default(),
        }
    }
}

/// Whether a dictionary for `have` checks text in `want`: the same tag, or the same primary language.
fn same_language(have: &str, want: &str) -> bool {
    let primary = |s: &str| s.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    have.eq_ignore_ascii_case(want) || primary(have) == primary(want)
}

impl Spellers {
    /// Changes when what is misspelled could change: checking on or off, dictionaries, ignored and added words.
    pub fn signature(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::hash::DefaultHasher::new();
        (self.enabled, self.ignored.len(), self.dictionaries).hash(&mut h);
        for (language, sp) in &self.by_language {
            (language, sp.ignored.len(), sp.added.len()).hash(&mut h);
        }
        h.finish()
    }

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
        self.dictionaries += 1;
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

    /// The language each unit of `para` is checked in: its run's, else `default`.
    fn languages<'a>(e: &'a Editor, para: ParaId, default: &'a str) -> Vec<&'a str> {
        let Some(p) = e.index().para(e.document(), para) else { return Vec::new() };
        let mut out = Vec::new();
        for i in &p.inlines {
            let l = i.run().map(|r| r.lang.as_str()).filter(|l| !l.is_empty()).unwrap_or(default);
            out.extend(std::iter::repeat_n(l, i.units()));
        }
        out
    }

    /// The language unit `u` of `para` is checked in.
    pub fn language_at(e: &Editor, para: ParaId, u: usize, default: &str) -> String {
        Self::languages(e, para, default).get(u).copied().unwrap_or(default).to_string()
    }

    /// The misspellings of `para`, each stretch of it checked in its runs' language (none while checking is off;
    /// a stretch no dictionary has the language of isn't checked).
    pub fn errors(&self, e: &Editor, para: ParaId, default: &str) -> Option<Rc<[SpellError]>> {
        if !self.enabled {
            return None;
        }
        Some(crate::prof::time("spelling:paragraph", || self.checked(e, para, default)))
    }

    /// [`Self::errors`], checking on.
    fn checked(&self, e: &Editor, para: ParaId, default: &str) -> Rc<[SpellError]> {
        let text = check_text(e, para);
        let langs = Self::languages(e, para, default);
        let lang = |k: usize| text.units.get(k).and_then(|&u| langs.get(u as usize)).copied().unwrap_or(default);
        // A stretch ends where the language changes or a field marker sits, so no word takes part of a field.
        let cut = |k: usize| !lang(k).eq_ignore_ascii_case(lang(k - 1)) || text.breaks.binary_search(&k).is_ok();
        let len = text.chars.len();
        if !(1..len).any(cut) {
            return by_unit(self.speller(lang(0)).map_or_else(|| Rc::from([]), |sp| sp.errors(&text.chars)), &text);
        }
        let mut all = Vec::new();
        let mut a = 0;
        while a < len {
            let l = lang(a);
            let b = (a + 1..len).find(|&k| cut(k)).unwrap_or(len);
            if let Some(sp) = self.speller(l) {
                all.extend(sp.errors(&text.chars[a..b]).iter().map(|x| SpellError { start: x.start + a, ..*x }));
            }
            a = b;
        }
        by_unit(all.into(), &text)
    }

    /// What a paragraph's misspellings depend on besides its own content.
    fn context(&self, e: &Editor, default: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::hash::DefaultHasher::new();
        (self.signature(), e.review().view, default, e.editing_sub_template()).hash(&mut h);
        h.finish()
    }

    fn para_count(&self, e: &Editor, para: ParaId, default: &str) -> u32 {
        self.errors(e, para, default).map_or(0, |errs| errs.len() as u32)
    }

    /// The misspellings in every story: the paragraphs touched since the last count are checked again, all of them
    /// when what checking depends on changed or the editor's log no longer reaches back.
    pub fn count(&mut self, e: &Editor, default: &str) -> u32 {
        if !self.enabled || self.by_language.is_empty() {
            return 0;
        }
        let context = self.context(e, default);
        let touched = self.tally.at.filter(|at| at.0 == context).and_then(|at| e.touched_since(at.1));
        let mut tally = std::mem::take(&mut self.tally);
        match touched {
            Some(paras) => {
                let sub_template = e.editing_sub_template();
                let mut seen = HashSet::new();
                for &p in paras.iter().filter(|p| seen.insert(**p)) {
                    tally.total -= tally.by_para.remove(&p).unwrap_or(0);
                    let n = if searched(e, p, sub_template) { self.para_count(e, p, default) } else { 0 };
                    if n > 0 {
                        tally.by_para.insert(p, n);
                        tally.total += n;
                    }
                }
            }
            None => {
                tally.by_para.clear();
                for p in searched_paras(e) {
                    let n = self.para_count(e, p, default);
                    if n > 0 {
                        tally.by_para.insert(p, n);
                    }
                }
                tally.total = tally.by_para.values().sum();
            }
        }
        tally.at = Some((context, e.edit_mark()));
        self.tally = tally;
        self.tally.total
    }

    /// Forgets the count: the document was replaced.
    pub fn forget(&mut self) {
        self.tally = Tally::default();
    }

    /// The misspelling the selection is on (within one paragraph).
    pub fn at_selection(&self, e: &Editor, default: &str) -> Option<Found> {
        let (para, a, b) = selected_range(e)?;
        let errs = self.errors(e, para, default)?;
        let error = errs.iter().copied().find(|x| a >= x.start && a <= x.end() && b <= x.end())?;
        let (word, language) = (word_of(e, para, &error), Self::language_at(e, para, error.start, default));
        Some(Found { para, error, word, language })
    }

    /// The first misspelling after the selection, wrapping round the document.
    pub fn next(&self, e: &Editor, default: &str) -> Option<Found> {
        let order = searched_paras(e);
        let n = order.len();
        if n == 0 {
            return None;
        }
        let (from_para, from) =
            e.selection().ordered(e.index()).and_then(|(_, b)| b.para_offset()).map_or((order.first().copied(), 0), |(p, u)| (Some(p), u.0 as usize));
        let start = from_para.and_then(|p| order.iter().position(|q| *q == p)).unwrap_or(0);
        for k in 0..=n {
            let para = order[(start + k) % n];
            let Some(errs) = self.errors(e, para, default) else { continue };
            if let Some(error) = errs.iter().copied().find(|x| k > 0 || x.start >= from) {
                let (word, language) = (word_of(e, para, &error), Self::language_at(e, para, error.start, default));
                return Some(Found { para, error, word, language });
            }
        }
        None
    }

    /// The misspellings on `pages` to underline: each word's band on its page. The word being typed isn't flagged
    /// until the caret leaves it.
    pub fn squiggles(&self, e: &mut Editor, pages: std::ops::Range<usize>, default: &str) -> Vec<(usize, crate::editing::view::Band)> {
        if !self.enabled || self.by_language.is_empty() {
            return Vec::new();
        }
        let caret = e.selection().is_collapsed().then(|| e.selection().focus().and_then(|p| p.para_offset())).flatten();
        let mut out = Vec::new();
        for para in e.paras_on(pages.clone()) {
            let Some(errs) = self.errors(e, para, default) else { continue };
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
    fn the_word_at_the_caret_spans_letters_and_inner_apostrophes() {
        use crate::editing::tests::{blank_with, fonts};
        let mut e = Editor::open(&blank_with("A quick fox's den."), fonts()).expect("opens");
        e.complete(fonts());
        let para = crate::editing::tests::para_id(&e, 0);
        e.select_range(para, Unit(4), Unit(4), fonts());
        let at = word_at_selection(&e, "en-US").expect("a word");
        assert_eq!((at.word.as_str(), at.start, at.end, at.language.as_str()), ("quick", Unit(2), Unit(7), "en-US"));
        e.select_range(para, Unit(9), Unit(9), fonts());
        assert_eq!(word_at_selection(&e, "en-US").expect("a word").word, "fox's");
        e.select_range(para, Unit(2), Unit(9), fonts());
        assert!(word_at_selection(&e, "en-US").is_none(), "past the word");
    }

    #[test]
    fn the_word_at_the_caret_takes_its_combining_marks() {
        use crate::editing::tests::{blank_with, fonts};
        let mut e = Editor::open(&blank_with("A cafe\u{301}ine \u{301}x."), fonts()).expect("opens");
        e.complete(fonts());
        let para = crate::editing::tests::para_id(&e, 0);
        for at in [3, 6, 8] {
            e.select_range(para, Unit(at), Unit(at), fonts());
            let w = word_at_selection(&e, "en-US").expect("a word");
            assert_eq!((w.word.as_str(), w.start, w.end), ("cafe\u{301}ine", Unit(2), Unit(10)), "from {at}");
        }
        e.select_range(para, Unit(13), Unit(13), fonts());
        assert_eq!(word_at_selection(&e, "en-US").expect("a word").word, "x", "a mark after a space isn't a letter");
    }

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
        // A dictionary replaced by one of the same size can misspell other words.
        let before = s.signature();
        s.add_dictionary("en-US", &aff, &dic).expect("loads");
        assert_ne!(s.signature(), before);
    }
}
