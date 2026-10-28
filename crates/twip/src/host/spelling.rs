//! Spell checking and the context menu: Hunspell dictionaries through `spellbook`, the usual skipping rules,
//! doubled words, the misspellings on the visible pages for the renderer to underline, F7, and the right-click menu.
//! Selections and edits go through the editor's commands.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::command_bar::entry;
use super::{Engine, json_str};
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

    pub fn suggest(&self, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.dict.suggest(&normalize(word), &mut out);
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
fn selected_range(e: &Editor) -> Option<(ParaId, usize, usize)> {
    let (a, b) = e.selection().ordered(e.index())?;
    let ((pa, ua), (pb, ub)) = (a.para_offset()?, b.para_offset()?);
    (pa == pb).then_some((pa, ua.0 as usize, ub.0 as usize))
}

/// Every paragraph id in document order (body, then headers and footers, notes).
fn paras_in_order(e: &Editor) -> Vec<ParaId> {
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

impl Engine {
    /// Loads a Hunspell dictionary and turns spell checking on.
    pub fn add_dictionary(&mut self, language: &str, aff: &[u8], dic: &[u8]) -> Result<(), String> {
        let mut sp = Speller::new(language, aff, dic)?;
        if let Some(old) = self.speller.take() {
            sp.added = old.added;
        }
        self.speller = Some(sp);
        self.view.check_spelling = true;
        Ok(())
    }

    /// Words added to the dictionary in earlier sessions (a JSON array of strings).
    pub fn add_user_words(&mut self, json: &str) {
        let words: Vec<String> = serde_json::from_str(json).unwrap_or_default();
        if let Some(sp) = &mut self.speller {
            sp.add_words(words.iter().map(String::as_str));
        }
    }

    pub fn set_spelling(&mut self, on: bool) {
        self.view.check_spelling = on;
    }

    fn checker(&self) -> Option<&Speller> {
        self.speller.as_ref().filter(|_| self.view.check_spelling)
    }

    /// The misspellings on `pages` to underline. The word being typed isn't flagged until the caret leaves it.
    pub(super) fn squiggles(&mut self, pages: std::ops::Range<usize>) -> Vec<(usize, [f64; 4])> {
        let (Some(sp), Some(e)) = (self.speller.as_ref().filter(|_| self.view.check_spelling), self.editor.as_mut()) else { return Vec::new() };
        let caret = e.selection().is_collapsed().then(|| e.selection().focus().and_then(|p| p.para_offset())).flatten();
        let mut out = Vec::new();
        for para in e.paras_on(pages.clone()) {
            let text = check_text(e, para);
            for err in sp.errors(&text).iter() {
                if caret == Some((para, Unit(err.end() as u32))) {
                    continue;
                }
                out.extend(e.unit_rects(para, Unit(err.start as u32), Unit(err.end() as u32), pages.clone()));
            }
        }
        out
    }

    /// The misspelling the selection is on (within one paragraph): its paragraph, the error and the word.
    fn misspelling(&self) -> Option<(ParaId, SpellError, String)> {
        let (sp, e) = (self.checker()?, self.editor.as_ref()?);
        let (para, a, b) = selected_range(e)?;
        let text = check_text(e, para);
        let err = sp.errors(&text).iter().copied().find(|x| a >= x.start && a <= x.end() && b <= x.end())?;
        Some((para, err, text[err.start..err.end()].iter().collect()))
    }

    /// F7 / Review → Spelling: selects the next misspelling after the selection (wrapping) and asks for the context
    /// menu on it; "SpellingComplete" when there is none.
    pub(super) fn spelling_step(&mut self) -> String {
        let Some(e) = self.editor.as_ref() else { return "SpellingComplete".into() };
        let Some(sp) = self.checker() else { return "SpellingComplete".into() };
        let order = paras_in_order(e);
        let (from_para, from) =
            e.selection().ordered(e.index()).and_then(|(_, b)| b.para_offset()).map_or((order.first().copied(), 0), |(p, u)| (Some(p), u.0 as usize));
        let start = from_para.and_then(|p| order.iter().position(|q| *q == p)).unwrap_or(0);
        let n = order.len();
        let mut hit = None;
        for k in 0..=n {
            let p = order[(start + k) % n.max(1)];
            if let Some(err) = sp.errors(&check_text(e, p)).iter().find(|x| k > 0 || x.start >= from) {
                hit = Some((p, err.start, err.end()));
                break;
            }
        }
        match hit {
            Some((p, a, b)) => {
                self.command("edit.select-range", Some(&format!("{},{a},{b}", p.0)));
                "ContextMenu".into()
            }
            None => "SpellingComplete".into(),
        }
    }

    /// The context menu for a right-click at a content point (CSS px): a click outside the selection first moves the
    /// caret there, so spelling suggestions apply to that word.
    pub fn context_menu_at(&mut self, x: f64, y: f64) -> String {
        let k = self.px_per_pt();
        let inside = self.editor.as_mut().is_some_and(|e| e.selection_contains(x / k, y / k));
        if !inside {
            self.pointer("down", x, y, 1, Default::default());
            self.pointer("up", x, y, 1, Default::default());
        }
        self.context_menu_at_caret()
    }

    /// What the selection is in, for host context-menu items: `field`,
    /// `control` and its kind, `region` (a repeating section), `table`, and `text` when the innermost element isn't a
    /// field. JSON array.
    pub fn context_tags(&mut self) -> String {
        let cx = self.send(serde_json::json!({ "type": "query", "q": "context" }))["answer"].clone();
        let mut tags: Vec<String> = Vec::new();
        for el in cx.as_array().into_iter().flatten() {
            let kind = el["kind"].as_str().unwrap_or("");
            let add: &[&str] = match kind {
                "field" => &["field"],
                "repeating_section" => &["control", kind, "region"],
                _ => &["control", kind],
            };
            tags.extend(add.iter().map(|t| t.to_string()));
        }
        if self.editor.as_mut().and_then(|e| e.format_state()).is_some_and(|f| f.table.is_some()) {
            tags.push("table".into());
        }
        if cx[0]["kind"] != "field" {
            tags.push("text".into());
        }
        tags.sort();
        tags.dedup();
        serde_json::to_string(&tags).unwrap_or_else(|_| "[]".into())
    }

    /// The right-click menu for the selection, as JSON.
    pub fn context_menu_at_caret(&mut self) -> String {
        let mut menu: Vec<String> = Vec::new();
        if let Some((_, err, word)) = self.misspelling() {
            if err.repeated {
                menu.push(entry("spell:delete-repeated", "Delete repeated word", false, None));
            } else {
                let s = self.checker().map(|sp| sp.suggest(&word)).unwrap_or_default();
                for (i, w) in s.iter().take(5).enumerate() {
                    menu.push(format!("{{\"id\":\"spell:suggest:{i}\",\"label\":{},\"bold\":true,\"disabled\":false}}", json_str(w)));
                }
                if s.is_empty() {
                    menu.push(entry("spell:none", "(No spelling suggestions)", true, None));
                }
                menu.push(entry("spell:ignore", "Ignore All", false, None));
                menu.push(entry("spell:add", "Add to Dictionary", false, None));
            }
            menu.push(entry("-", "", false, None));
        }
        let Some(e) = self.editor.as_mut() else { return "[]".into() };
        let f = e.format_state();
        let sel = f.as_ref().is_some_and(|f| f.has_selection);
        menu.push(icon(entry("cut", "Cut", !sel, None), "cut"));
        menu.push(icon(entry("copy", "Copy", !sel, None), "copy"));
        menu.push(icon(entry("paste", "Paste", false, None), "clipboard_paste"));
        menu.push(entry("-", "", false, None));
        if f.as_ref().is_some_and(|f| f.table.is_some()) {
            let ins = [
                ("table:row-above", "Row above"),
                ("table:row-below", "Row below"),
                ("table:col-left", "Column left"),
                ("table:col-right", "Column right"),
            ];
            let del = [("table:delete-row", "Row"), ("table:delete-col", "Column"), ("table:delete-table", "Table")];
            let items = |v: &[(&str, &str)]| v.iter().map(|(i, l)| entry(i, l, false, None)).collect::<Vec<_>>().join(",");
            menu.push(with_items(entry("table:insert", "Insert", false, None), &items(&ins)));
            menu.push(with_items(entry("table:delete", "Delete", false, None), &items(&del)));
            menu.push(entry("table:merge", "Merge cells", false, None));
            // Ctrl+Tab is the browser's: a tab character in the cell from here.
            menu.push(entry("table:insert-tab", "Insert tab character", false, None));
            menu.push(entry("-", "", false, None));
        }
        if f.as_ref().is_some_and(|f| f.object.is_some()) {
            menu.push(entry("picture:size", "Size...", false, None));
            menu.push(entry("picture:alt", "Edit Alt Text...", false, None));
            menu.push(entry("-", "", false, None));
        }
        menu.push(icon(entry("font", "Font...", false, None), "text_font"));
        menu.push(icon(entry("paragraph", "Paragraph...", false, None), "text_paragraph"));
        menu.push(icon(entry("link", "Link...", false, None), "link"));
        format!("[{}]", menu.join(","))
    }

    /// Carries out a menu entry: JSON `{ command, userWord }` for the host.
    pub fn invoke_menu(&mut self, id: &str) -> String {
        if id.starts_with("spell:") {
            let Some((para, err, word)) = self.misspelling() else { return "{}".into() };
            match id {
                "spell:ignore" => {
                    if let Some(sp) = &mut self.speller {
                        sp.ignore(&word);
                    }
                }
                "spell:add" => {
                    if let Some(sp) = &mut self.speller {
                        sp.add(&word);
                    }
                    return format!("{{\"userWord\":{}}}", json_str(&word));
                }
                "spell:delete-repeated" => {
                    let text = self.editor.as_ref().map(|e| check_text(e, para)).unwrap_or_default();
                    let from = if err.start > 0 && text.get(err.start - 1) == Some(&' ') { err.start - 1 } else { err.start };
                    self.command("edit.select-range", Some(&format!("{},{from},{}", para.0, err.end())));
                    self.key("Delete", false, false, false);
                }
                _ => {
                    let s = self.checker().map(|sp| sp.suggest(&word)).unwrap_or_default();
                    if let Some(w) = id.strip_prefix("spell:suggest:").and_then(|i| i.parse::<usize>().ok()).and_then(|i| s.get(i)) {
                        self.command("edit.select-range", Some(&format!("{},{},{}", para.0, err.start, err.end())));
                        self.command("edit.insert_text", Some(w));
                    }
                }
            }
            return "{}".into();
        }
        let command = match id {
            "cut" => "Cut",
            "copy" => "Copy",
            "paste" => "Paste",
            "font" => "Font",
            "paragraph" => "Paragraph",
            "link" => "Hyperlink",
            "picture:size" => "PictureSize",
            "picture:alt" => "AltText",
            other => {
                let cmd = match other {
                    "table:row-above" => "table.insert-above",
                    "table:row-below" => "table.insert-below",
                    "table:col-left" => "table.insert-left",
                    "table:col-right" => "table.insert-right",
                    "table:delete-row" => "table.delete.row",
                    "table:delete-col" => "table.delete.column",
                    "table:delete-table" => "table.delete.table",
                    "table:merge" => "table.merge",
                    "table:insert-tab" => "edit.tab",
                    _ => return "{}".into(),
                };
                self.command(cmd, None);
                return "{}".into();
            }
        };
        format!("{{\"command\":{}}}", json_str(command))
    }
}

fn icon(e: String, name: &str) -> String {
    format!("{},\"icon\":{}}}", &e[..e.len() - 1], json_str(name))
}

fn with_items(e: String, items: &str) -> String {
    format!("{},\"items\":[{items}]}}", &e[..e.len() - 1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::Fonts;

    fn dictionary() -> Option<(Vec<u8>, Vec<u8>)> {
        let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dictionaries");
        Some((std::fs::read(d.join("en_US.aff")).ok()?, std::fs::read(d.join("en_US.dic")).ok()?))
    }

    #[test]
    fn spelling_and_menu() {
        let Some((aff, dic)) = dictionary() else { return };
        let mut e = Engine::with_fonts(Fonts::system());
        e.add_dictionary("en-US", &aff, &dic).unwrap();
        e.new_document();
        e.text("I recieve the the mail. NASA 2nd www.exmple.com nme@x.org");
        let ed = e.editor.as_ref().unwrap();
        let para = ParaId(
            ed.document().sections[0].blocks.iter().find_map(|b| if let crate::docx::Block::Para(p) = b { Some(p.id) } else { None }).unwrap(),
        );
        let errs = e.speller.as_ref().unwrap().errors(&check_text(ed, para));
        assert_eq!(errs.len(), 2, "{errs:?}");
        assert_eq!((errs[0].start, errs[0].repeated), (2, false));
        assert_eq!((errs[1].start, errs[1].len, errs[1].repeated), (14, 3, true));
        // F7 selects the first misspelling; the menu suggests "receive"; picking it replaces the word.
        e.key("Home", true, false, false);
        assert_eq!(e.spelling_step(), "ContextMenu");
        let menu = e.context_menu_at_caret();
        assert!(menu.starts_with("[{\"id\":\"spell:suggest:0\""), "{menu}");
        let first: serde_json::Value = serde_json::from_str(&menu).unwrap();
        assert_eq!(first[0]["label"].as_str(), Some("receive"));
        assert_eq!(e.invoke_menu("spell:suggest:0"), "{}");
        let text = |e: &Engine| check_text(e.editor.as_ref().unwrap(), para).iter().collect::<String>();
        assert!(text(&e).starts_with("I receive the the"), "{}", text(&e));
        // The doubled word goes with its space.
        assert_eq!(e.spelling_step(), "ContextMenu");
        assert!(e.context_menu_at_caret().contains("spell:delete-repeated"));
        e.invoke_menu("spell:delete-repeated");
        assert!(text(&e).starts_with("I receive the mail."), "{}", text(&e));
        assert_eq!(e.spelling_step(), "SpellingComplete");
        // Add to dictionary reports the word; it is no longer flagged.
        e.key("End", true, false, false);
        e.text(" Zorbl");
        e.key("ArrowLeft", false, false, false);
        assert_eq!(e.invoke_menu("spell:add"), "{\"userWord\":\"Zorbl\"}");
        assert!(e.speller.as_ref().unwrap().errors(&check_text(e.editor.as_ref().unwrap(), para)).is_empty());
        // The squiggles: none now; one under a new misspelling once the caret leaves it.
        e.key("End", true, false, false);
        e.text(" qwzxv.");
        let n = e.editor.as_ref().unwrap().stack().pages.len();
        assert_eq!(e.squiggles(0..n).len(), 1);
        e.set_spelling(false);
        assert!(e.squiggles(0..n).is_empty());
    }

    /// Context-menu tags and the `keymap` query.
    #[test]
    fn context_tags_and_keymap_query() {
        let mut e = crate::host::Engine::with_fonts(Fonts::system());
        let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-MERGE-FIELDS.docx")).unwrap();
        e.open(&bytes);
        let ask = |e: &mut crate::host::Engine, v: serde_json::Value| {
            serde_json::from_str::<serde_json::Value>(&e.send_json(&v.to_string())).unwrap()["answer"].clone()
        };
        assert_eq!(e.context_tags(), r#"["text"]"#);
        let fields = ask(&mut e, serde_json::json!({ "type": "query", "q": "elements", "kind": "field" }));
        let id = fields[0]["id"].clone();
        e.send_json(&serde_json::json!({ "type": "command", "id": "element.select", "arg": { "id": id } }).to_string());
        assert_eq!(e.context_tags(), r#"["field"]"#);
        let key = |e: &mut crate::host::Engine, code: &str, ctrl: bool, shift: bool| {
            ask(e, serde_json::json!({ "type": "query", "q": "keymap", "code": code, "mods": { "ctrl": ctrl, "shift": shift, "alt": false } }))
        };
        assert_eq!(key(&mut e, "KeyB", true, false), serde_json::json!({ "bound": true, "command": "Toggle", "reserved": false }));
        assert_eq!(key(&mut e, "KeyF", true, false), serde_json::json!({ "bound": true, "command": "Find", "reserved": false }));
        assert_eq!(key(&mut e, "KeyT", true, false)["reserved"], true);
        assert_eq!(key(&mut e, "KeyQ", true, true), serde_json::json!({ "bound": false, "reserved": false }));
    }

    /// Two engines on one page: a copy in one pastes into the other with its fields, under new ids,
    /// and leaves the first engine's document and history alone.
    #[test]
    fn paste_between_engines_keeps_fields() {
        let fonts = std::rc::Rc::new(Fonts::system());
        let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-MERGE-FIELDS.docx")).unwrap();
        let mut a = crate::host::Engine::default();
        let mut b = crate::host::Engine::default();
        a.fonts = fonts.clone();
        b.fonts = fonts;
        a.open(&bytes);
        b.open(&bytes);
        let fields = |e: &mut crate::host::Engine| -> Vec<serde_json::Value> {
            let r: serde_json::Value = serde_json::from_str(&e.send_json(r#"{"type":"query","q":"elements","kind":"field"}"#)).unwrap();
            r["answer"].as_array().unwrap().clone()
        };
        let before = fields(&mut b).len();
        let first = fields(&mut a)[0].clone();
        a.send_json(&serde_json::json!({ "type": "command", "id": "element.select", "arg": { "id": first["id"] } }).to_string());
        let clip: serde_json::Value = serde_json::from_str(&a.copy(false)).unwrap();
        let depth = a.editor.as_ref().unwrap().history().undo_depth();
        b.key("End", true, false, false);
        b.paste(clip["text"].as_str().unwrap(), clip["token"].as_str().unwrap());
        let after = fields(&mut b);
        assert_eq!(after.len(), before + 1, "the field came across, not its text");
        let pasted = after.iter().find(|f| f["code"] == first["code"] && f["id"] != first["id"]).expect("same code, a new id");
        assert!(fields(&mut a).iter().all(|f| f["id"] != pasted["id"]));
        assert_eq!(a.editor.as_ref().unwrap().history().undo_depth(), depth);
    }
}
