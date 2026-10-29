//! Menus, dialogs, find, spelling, navigation, styles, accessibility and word count.

use super::{Commit, Session};
use crate::api::command::Command;
use crate::api::engine::Dictionary;
use crate::api::error::{Error, ErrorCode};
use crate::api::event::Event;
use crate::api::record::Origin;
use crate::api::services::{
    AccessibleNode, ContextMenu, DialogKind, DialogModel, DialogPatch, FindDirection, FindMatch, FindQuery, FindResult, Heading, MenuAction,
    Misspelling, StyleInfo, StyleType, WordCount,
};
use crate::api::state::State;
use crate::api::value::{HeadingId, MatchId, Order};
use crate::editing::commands::{Dir, deleting};
use crate::layout::Unit;
use crate::services::{dialogs, find, menu};

/// The spelling and review values of the state, which the services keep.
pub(super) fn fill_state(s: &mut Session, state: &mut State) {
    let sp = &s.services.spelling;
    state.spelling.enabled = sp.enabled;
    state.spelling.misspellings = sp.count(&s.editor, &s.language);
}

impl Session {
    /// The context menu for the selection, as the context-menu key would open it.
    pub fn context_menu(&mut self) -> ContextMenu {
        let misspelled = self.services.spelling.at_selection(&self.editor, &self.language).map(|f| menu::Misspelled {
            suggestions: if f.error.repeated { Vec::new() } else { self.services.spelling.suggestions(&f.word, &f.language) },
            repeated: f.error.repeated,
            word: f.word,
            language: f.language,
        });
        menu::build(&self.state, misspelled.as_ref())
    }

    /// Runs a menu item as one transaction.
    pub fn invoke(&mut self, action: MenuAction) -> Result<Commit, Error> {
        match action {
            MenuAction::Command { command } => self.run(command),
            MenuAction::ReplaceWord { with } => {
                let f = self
                    .services
                    .spelling
                    .at_selection(&self.editor, &self.language)
                    .ok_or_else(|| Error::unavailable("no misspelling is selected"))?;
                self.refuse_read_only()?;
                // A doubled word goes with the space before it.
                let text = crate::services::spelling::check_text(&self.editor, f.para);
                let start =
                    if with.is_empty() && f.error.start > 0 && text.get(f.error.start - 1) == Some(&' ') { f.error.start - 1 } else { f.error.start };
                let edit = if with.is_empty() { deleting(Dir::Forward, false) } else { Command::InsertText(with) };
                self.apply_with(Origin::Api, |e, fonts| {
                    e.joined("Spelling", fonts, |e| {
                        e.select_range(f.para, Unit(start as u32), Unit(f.error.end() as u32), fonts);
                        e.execute(edit, fonts)
                    })
                })
            }
            MenuAction::IgnoreWord { word } => {
                self.spelling().ignore(&word);
                Ok(Commit { record: None, value: None })
            }
            MenuAction::AddWord { word, language } => {
                self.services.spelling.add(&word, &language);
                self.changed(vec![Event::WordAdded { word, language }]);
                Ok(Commit { record: None, value: None })
            }
        }
    }

    fn refuse_read_only(&self) -> Result<(), Error> {
        if self.read_only { Err(Error::new(ErrorCode::ReadOnly, "the session is read only")) } else { Ok(()) }
    }

    /// What dialogs read besides the state.
    fn dialog_context(&mut self, kind: DialogKind) -> dialogs::Context {
        let mut cx = dialogs::Context::default();
        match kind {
            DialogKind::Hyperlink => cx.selected_text = self.editor.selected_text(),
            DialogKind::Accessibility => cx.issues = dialogs::issues(self.editor.document()),
            DialogKind::Font => cx.fonts = self.fonts.families(),
            _ => {}
        }
        cx
    }

    /// Opens a dialog on the selection's values.
    pub fn dialog_open(&mut self, kind: DialogKind) -> Result<DialogModel, Error> {
        let cx = self.dialog_context(kind);
        dialogs::open(kind, &self.state, &cx)
    }

    /// Changes some of a dialog's values; dependent values and field states are recomputed. Fields the patch leaves
    /// out keep the model's values.
    pub fn dialog_update(&mut self, model: DialogModel, patch: DialogPatch) -> Result<DialogModel, Error> {
        let mut values = model.values.clone();
        if !patch.apply(&mut values) {
            return Err(Error::invalid("the patch is for another dialog"));
        }
        let cx = self.dialog_context(model.kind);
        dialogs::update(&model, values, &cx)
    }

    /// Validates a dialog's values and applies them as one transaction.
    pub fn dialog_apply(&mut self, model: DialogModel) -> Result<Commit, Error> {
        let cx = self.dialog_context(model.kind);
        let mut commands = dialogs::commands(&model.values, &self.state, &cx)?;
        match commands.len() {
            0 => Ok(Commit { record: None, value: None }),
            1 => self.run(commands.remove(0)),
            _ => self.batch(commands, Some(dialogs::label(model.kind).to_string())),
        }
    }

    /// Selects what an accessibility issue (from the Accessibility dialog) is on: its picture, or the first cell of
    /// its table.
    pub fn select_accessibility_issue(&mut self, id: &str) -> Result<(), Error> {
        let mut parts = id.split(':');
        let num = |p: Option<&str>| p.and_then(|p| p.parse::<u64>().ok());
        let (para, a, b) = match (parts.next(), num(parts.next()), num(parts.next())) {
            (Some("picture"), Some(p), Some(u)) => (p, u, u + 1),
            (Some("table"), Some(p), None) => (p, 0, 0),
            _ => return Err(Error::invalid("not an accessibility issue id")),
        };
        let para = crate::layout::ParaId(para);
        let unit = |u: u64| Unit(u32::try_from(u).unwrap_or(u32::MAX));
        if self.editor.index().para(self.editor.document(), para).is_none() {
            return Err(Error::unavailable("the issue is gone"));
        }
        self.apply_with(Origin::Api, |e, fonts| e.select_range(para, unit(a), unit(b), fonts)).map(|_| ())
    }

    /// Finds and selects the next or previous match.
    pub fn find(&mut self, query: &FindQuery, dir: FindDirection) -> FindResult {
        let Some((hit, wrapped)) = find::next(&self.editor, query, dir == Order::Previous) else { return FindResult::default() };
        let _ = self.apply_with(Origin::Api, |e, fonts| hit.select(e, fonts));
        FindResult { found: true, wrapped }
    }

    /// Every match, for a results list.
    pub fn find_all(&mut self, query: &FindQuery) -> Vec<FindMatch> {
        let hits = find::all(&self.editor, query);
        self.complete_layout();
        let pages = crate::services::para_pages(&mut self.editor);
        hits.into_iter().map(|h| find::describe(&self.editor, h, pages.get(&h.para).copied().unwrap_or(1))).collect()
    }

    /// Selects and reveals a match.
    pub fn select_match(&mut self, id: MatchId) -> Result<(), Error> {
        let hit = find::Hit::parse(&id).ok_or_else(|| Error::invalid("not a match id"))?;
        if self.editor.index().para(self.editor.document(), hit.para).is_none() {
            return Err(Error::unavailable("the match is gone"));
        }
        self.apply_with(Origin::Api, |e, fonts| hit.select(e, fonts)).map(|_| ())
    }

    /// Replaces the current match, then finds the next.
    pub fn replace(&mut self, query: &FindQuery, with: &str) -> Result<FindResult, Error> {
        if let Some(hit) = find::at_selection(&self.editor, query) {
            self.refuse_read_only()?;
            self.apply_with(Origin::Api, |e, fonts| find::replace(e, &[hit], with, fonts))?;
        }
        Ok(self.find(query, Order::Next))
    }

    /// Replaces every match as one transaction; how many.
    pub fn replace_all(&mut self, query: &FindQuery, with: &str) -> Result<u32, Error> {
        let hits = find::all(&self.editor, query);
        if hits.is_empty() {
            return Ok(0);
        }
        self.refuse_read_only()?;
        let (whole, case) = (query.whole_words.unwrap_or(false), query.match_case.unwrap_or(false));
        self.apply_with(
            Origin::Api,
            |e, fonts| {
                if whole { find::replace(e, &hits, with, fonts) } else { e.replace_all(&query.text, with, case, fonts).1 }
            },
        )?;
        Ok(hits.len() as u32)
    }

    /// Spell checking: dictionaries, ignored and added words, suggestions.
    pub fn spelling(&mut self) -> Spelling<'_> {
        Spelling { session: self }
    }

    /// The document's headings, in order, with the page each starts on.
    pub fn headings(&mut self) -> Vec<Heading> {
        // Pages count the whole layout.
        let fonts = self.fonts.fonts();
        self.editor.complete(&fonts);
        crate::services::headings(&mut self.editor)
    }

    /// Puts the caret at the start of a heading and reveals it.
    pub fn select_heading(&mut self, id: HeadingId) -> Result<(), Error> {
        let para = id.0.parse::<u64>().map(crate::layout::ParaId).map_err(|_| Error::invalid("not a heading id"))?;
        if self.editor.index().para(self.editor.document(), para).is_none() {
            return Err(Error::unavailable("the heading is gone"));
        }
        self.apply_with(Origin::Api, |e, fonts| e.select_range(para, Unit(0), Unit(0), fonts)).map(|_| ())
    }

    /// The document's styles, of one type or all.
    pub fn styles(&self, kind: Option<StyleType>) -> Vec<StyleInfo> {
        crate::services::styles::list(self.editor.document(), kind)
    }

    /// The document's accessibility tree.
    pub fn accessible_tree(&mut self) -> AccessibleNode {
        crate::services::a11y::tree(&self.editor, &self.services.spelling, &self.language)
    }

    /// The document's counts, computed now.
    pub fn word_count(&mut self) -> WordCount {
        // Lines and pages count the whole layout.
        let fonts = self.fonts.fonts();
        self.editor.complete(&fonts);
        crate::services::word_count(&mut self.editor)
    }
}

/// A session's spell checking.
pub struct Spelling<'a> {
    session: &'a mut Session,
}

impl Spelling<'_> {
    /// Adds a dictionary for its language.
    pub fn add_dictionary(&mut self, dictionary: Dictionary) -> Result<(), Error> {
        let s = &mut *self.session;
        s.services.spelling.add_dictionary(&dictionary.language, &dictionary.aff.0, &dictionary.dic.0).map_err(Error::invalid)?;
        s.changed(Vec::new());
        Ok(())
    }

    /// Spell checking on or off.
    pub fn set_enabled(&mut self, on: bool) {
        let s = &mut *self.session;
        if s.services.spelling.enabled != on {
            s.services.spelling.enabled = on;
            s.changed(Vec::new());
        }
    }

    /// The language for text with none.
    pub fn set_language(&mut self, language: &str) {
        let s = &mut *self.session;
        if s.language != language {
            s.language = language.to_string();
            s.changed(Vec::new());
        }
    }

    /// Ignores a word for this session.
    pub fn ignore(&mut self, word: &str) {
        self.session.services.spelling.ignore(word);
        self.session.changed(Vec::new());
    }

    /// Adds a word to the user's dictionary (emits `wordAdded`), in the selected misspelling's language or the
    /// session's.
    pub fn add(&mut self, word: &str) {
        let s = &mut *self.session;
        let language =
            s.services.spelling.at_selection(&s.editor, &s.language).filter(|f| f.word == word).map_or_else(|| s.language.clone(), |f| f.language);
        s.services.spelling.add(word, &language);
        s.changed(vec![Event::WordAdded { word: word.to_string(), language }]);
    }

    /// Words the user added earlier, kept until their language's dictionary arrives.
    pub fn load_user_words(&mut self, language: &str, words: &[String]) {
        self.session.services.spelling.load_user_words(language, words);
        self.session.changed(Vec::new());
    }

    /// Suggestions for a word.
    pub fn suggestions(&mut self, word: &str, language: Option<&str>) -> Vec<String> {
        let s = &*self.session;
        s.services.spelling.suggestions(word, language.unwrap_or(&s.language))
    }

    /// Selects the next misspelling (F7).
    pub fn next_misspelling(&mut self) -> Option<Misspelling> {
        let s = &mut *self.session;
        let f = s.services.spelling.next(&s.editor, &s.language)?;
        let suggestions = s.services.spelling.suggestions(&f.word, &f.language);
        let _ = s.apply_with(Origin::Api, |e, fonts| e.select_range(f.para, Unit(f.error.start as u32), Unit(f.error.end() as u32), fonts));
        Some(Misspelling { word: f.word, suggestions })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::api::engine::{Dictionary, Engine, SessionId, SessionOptions};
    use crate::api::event::{Event, KeyOutcome};
    use crate::api::fonts::FontLibrary;
    use crate::api::input::{Key, Modifiers};
    use crate::api::services::{DialogKind, DialogValues, FindQuery, MenuAction, MenuItem, MenuTarget};
    use crate::api::value::{Bytes, Order};

    type Events = Rc<RefCell<Vec<Event>>>;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).expect("fixture")
    }

    /// An engine with a session on a fixture, and the events it hears.
    fn open(name: &str) -> (Engine, SessionId, Events) {
        let mut engine = Engine::new(FontLibrary::system());
        let events: Events = Rc::default();
        let sink = events.clone();
        engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
        let id = engine.create_session(SessionOptions::default());
        engine.session(id).expect("a session").open(&fixture(name)).expect("opens");
        (engine, id, events)
    }

    fn query(text: &str) -> FindQuery {
        FindQuery { text: text.into(), ..FindQuery::default() }
    }

    #[test]
    fn find_and_replace_follow_the_view() {
        let (mut engine, id, _) = open("editor/FX-services.docx");
        let s = engine.session(id).expect("a session");
        for _ in 0..3 {
            assert!(s.find(&query("apple"), Order::Next).found);
        }
        let again = s.find(&query("apple"), Order::Next);
        assert!(again.found && again.wrapped, "past the last, it wraps");
        let case = FindQuery { match_case: Some(true), ..query("aPPLE") };
        assert!(!s.find(&case, Order::Next).found);
        let all = s.find_all(&query("apple"));
        assert_eq!(all.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(), ["Apple", "apple", "APPLE"]);
        assert_eq!((all[1].before.as_str(), all[1].page), ("Apple, ", 1));
        s.select_match(all[2].id.clone()).expect("selects");
        let revision = s.state().revision;
        s.replace(&query("apple"), "pear").expect("replaces");
        assert_eq!(s.state().revision, revision + 1);
        assert_eq!(s.replace_all(&query("apple"), "plum").expect("replaces"), 2);
        assert_eq!(s.state().revision, revision + 2, "one transaction");
        assert!(s.find_all(&query("apple")).is_empty());
        let whole = FindQuery { whole_words: Some(true), ..query("plum") };
        assert_eq!(s.find_all(&whole).len(), 2);
    }

    #[test]
    fn spelling_checks_ignores_adds_and_menus() {
        let (mut engine, id, events) = open("editor/FX-services.docx");
        let s = engine.session(id).expect("a session");
        let (aff, dic) = (fixture("dictionaries/en_US.aff"), fixture("dictionaries/en_US.dic"));
        s.spelling().load_user_words("en-US", &["brwon".to_string()]);
        s.spelling().add_dictionary(Dictionary { language: "en-US".into(), aff: Bytes(aff), dic: Bytes(dic) }).expect("loads");
        assert_eq!(s.state().spelling.misspellings, 3, "Teh, quikc, zorbly");
        assert!(s.spelling().suggestions("quikc", None).contains(&"quick".to_string()));
        let next = s.spelling().next_misspelling().expect("one");
        assert_eq!(next.word, "Teh");
        assert!(next.suggestions.contains(&"The".to_string()), "a swap, in the word's case: {:?}", next.suggestions);
        s.spelling().ignore("Teh");
        assert_eq!(s.state().spelling.misspellings, 2);
        // The menu on the next one: its suggestions, Ignore and Add to Dictionary.
        assert_eq!(s.spelling().next_misspelling().map(|m| m.word).as_deref(), Some("quikc"));
        let menu = s.context_menu();
        assert_eq!(menu.target, MenuTarget::Misspelling { word: "quikc".into(), language: "en-US".into() });
        let action = |label: &str| {
            menu.items.iter().find_map(|i| match i {
                MenuItem::Suggestion { label: l, action } if l == label => Some(action.clone()),
                _ => None,
            })
        };
        let revision = s.state().revision;
        s.invoke(action("Add to Dictionary").expect("an item")).expect("adds");
        assert!(events.borrow().iter().any(|e| matches!(e, Event::WordAdded { word, language } if word == "quikc" && language == "en-US")));
        assert_eq!((s.state().spelling.misspellings, s.state().revision), (1, revision), "no edit");
        assert_eq!(s.spelling().next_misspelling().map(|m| m.word).as_deref(), Some("zorbly"));
        s.invoke(MenuAction::ReplaceWord { with: "zebra".into() }).expect("replaces");
        assert_eq!((s.state().spelling.misspellings, s.state().revision), (0, revision + 1));
        s.spelling().set_enabled(false);
        assert!(!s.state().spelling.enabled);
    }

    #[test]
    fn the_menu_key_emits_a_menu_and_the_tree_reads_the_document() {
        let (mut engine, id, events) = open("editor/FX-services.docx");
        let s = engine.session(id).expect("a session");
        assert_eq!(s.key(Key::ContextMenu, Modifiers::default()), KeyOutcome::Handled);
        assert_eq!(s.key(Key::F10, Modifiers { shift: true, ..Modifiers::default() }), KeyOutcome::Handled);
        assert_eq!(events.borrow().iter().filter(|e| matches!(e, Event::ContextMenu(_))).count(), 2);
        let tree = s.accessible_tree();
        let text: Vec<String> = tree.children.iter().filter_map(|n| n.text.clone()).collect();
        assert!(text.iter().any(|t| t.starts_with("Teh quick brown fox")), "{text:?}");
        let count = s.word_count();
        assert!(count.words > 30 && count.pages >= 1 && count.lines >= count.paragraphs, "{count:?}");
        assert!(count.characters_with_spaces > count.characters);
    }

    #[test]
    fn headings_select_and_styles_preview() {
        use crate::api::services::StyleType;
        let (mut engine, id, _) = open("editor/FX-outline.docx");
        let s = engine.session(id).expect("a session");
        let h = s.headings();
        let got: Vec<(u32, &str, u32)> = h.iter().map(|h| (h.level, h.text.as_str(), h.page)).collect();
        assert_eq!(got[..4], [(1, "Overview", 1), (2, "Background", 1), (3, "Earlier work", 1), (1, "Method", 2)]);
        s.select_heading(h[4].id.clone()).expect("selects");
        assert_eq!(s.state().paragraph.style, crate::api::state::Tri::Uniform(crate::api::value::StyleId("Heading2".into())));
        assert!(s.select_heading(crate::api::value::HeadingId("x".into())).is_err());
        let styles = s.styles(Some(StyleType::Paragraph));
        let h1 = styles.iter().find(|s| s.id.0 == "Heading1").expect("Heading 1");
        assert_eq!((h1.name.as_str(), h1.quick, h1.preview.bold, h1.preview.size.0), ("Heading 1", true, true, 16.0));
        assert_eq!(h1.based_on.as_ref().map(|b| b.0.as_str()), Some("Normal"));
        assert!(styles.iter().all(|s| s.kind == StyleType::Paragraph));
        assert!(s.styles(None).len() > styles.len());
    }

    #[test]
    fn dialogs_open_on_the_selection() {
        let (mut engine, id, _) = open("editor/FX-services.docx");
        let s = engine.session(id).expect("a session");
        let check = s.dialog_open(DialogKind::Accessibility).expect("opens");
        let DialogValues::Accessibility(a) = &check.values else { panic!("{check:?}") };
        assert!(!a.issues.is_empty(), "the picture has no alt text");
        s.select_accessibility_issue(&a.issues[0].id).expect("selects it");
        assert!(s.find(&query("the example site"), Order::Next).found);
        let link = s.dialog_open(DialogKind::Hyperlink).expect("opens");
        let DialogValues::Hyperlink(h) = &link.values else { panic!() };
        assert_eq!(h.text, "the example site");
        assert!(s.dialog_open(DialogKind::TableProperties).is_err(), "not in a table");
        let patch: crate::api::services::DialogPatch =
            serde_json::from_str(r#"{"kind":"hyperlink","address":"https://example.org/"}"#).expect("a patch");
        let link = s.dialog_update(link, patch).expect("updates");
        let DialogValues::Hyperlink(h) = &link.values else { panic!() };
        assert_eq!((h.text.as_str(), h.address.as_str()), ("the example site", "https://example.org/"), "fields left out keep their values");
    }
}
