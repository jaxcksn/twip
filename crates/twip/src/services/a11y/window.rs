//! The text around the caret for the element that has the keyboard focus ([`CaretWindow`]): the caret's paragraph
//! and its neighbours in the same container, split into their laid-out lines, with the selection in it. A screen
//! reader reads characters, words, lines and paragraphs from that element as the caret moves, and echoes edits.

use crate::api::services::{AccessibleId, AccessibleRole, CaretWindow, WindowLine, WindowParagraph, WindowPoint, WindowSegment};
use crate::docx::Block;
use crate::editing::Editor;
use crate::editing::position::Position;
use crate::layout::{ParaId, Unit};

use super::super::spelling::Spellers;
use super::tree::{heading_level, list_label};
use super::{Reading, document_language};

/// How much text after the caret's paragraph a window holds, characters.
const AFTER: usize = 2000;

/// A caret window and the units its text came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub window: CaretWindow,
    /// Each paragraph's id, and the unit of each of its characters, then the unit after the last.
    units: Vec<(ParaId, Vec<u32>)>,
}

impl Window {
    /// The document place of a place in the window: `None` outside it.
    pub fn place(&self, at: WindowPoint) -> Option<(ParaId, Unit)> {
        let (para, units) = self.units.get(at.paragraph as usize)?;
        units.get(at.offset as usize).map(|&u| (*para, Unit(u)))
    }

    /// The window place of a document position, or none when it is outside the window.
    fn point(&self, p: &Position) -> Option<WindowPoint> {
        let (para, u) = p.para_offset()?;
        let k = self.units.iter().position(|(id, _)| *id == para)?;
        let units = &self.units[k].1;
        // The characters before it: a unit that isn't read (an object, a hidden one) is at the next character.
        let offset = units[..units.len() - 1].partition_point(|&x| x < u.0);
        Some(WindowPoint { paragraph: k as u32, offset: offset as u32 })
    }

    fn start(&self) -> WindowPoint {
        WindowPoint::default()
    }

    fn end(&self) -> WindowPoint {
        let last = self.units.len().saturating_sub(1);
        WindowPoint { paragraph: last as u32, offset: self.units.get(last).map_or(0, |u| u.1.len() as u32 - 1) }
    }
}

/// The window around the caret, or none when the selection holds no text position (an object, cells). `read_only`:
/// the session takes no edits.
pub fn window(e: &mut Editor, spelling: &Spellers, language: &str, read_only: bool) -> Option<Window> {
    let focus = e.selection().focus()?;
    let (para, _) = focus.para_offset()?;
    let slot = e.index().para_slot(para)?;
    // The paragraph before it and those after it in its container, up to a table on either side.
    let ids: Vec<ParaId> = {
        let blocks = e.index().blocks(e.document(), slot.container)?;
        let para_at = |i: usize| match blocks.get(i) {
            Some(Block::Para(p)) => Some(p),
            _ => None,
        };
        let mut ids: Vec<ParaId> = slot.block.checked_sub(1).and_then(para_at).map(|p| ParaId(p.id)).into_iter().collect();
        ids.push(para);
        let mut after = 0;
        for p in (slot.block + 1..).map_while(para_at) {
            if after >= AFTER {
                break;
            }
            after += p.inlines.iter().map(|i| i.units()).sum::<usize>();
            ids.push(ParaId(p.id));
        }
        ids
    };
    let starts: Vec<Vec<Unit>> = ids.iter().map(|&p| e.line_starts(p)).collect();
    let all = !e.selection().is_collapsed() && e.selects_story();
    let read_only = read_only || !e.caret_editable();

    let e: &Editor = e;
    let doc = e.document();
    let doc_lang = document_language(doc, language);
    // shortcut: comment ranges opened before the window aren't followed (that walks the body from its start on every
    // keystroke); the window carries no comments, and the caret's announcements say them.
    let mut open = Vec::new();
    let mut paragraphs = Vec::new();
    let mut units = Vec::new();
    let mut lang = doc_lang.clone();
    // Misspellings are marked in the caret's paragraph: the others' are said as the caret reaches them.
    let mut unchecked = Spellers::default();
    unchecked.enabled = false;
    for (&id, starts) in ids.iter().zip(&starts) {
        let Some(p) = e.index().para(doc, id) else { continue };
        let r = Reading::new(e, p, if id == para { spelling } else { &unchecked }, language, &doc_lang, &mut open);
        let mut lines = vec![WindowLine::default()];
        let mut map = Vec::new();
        let mut last = None;
        for u in 0..r.len() {
            if u > 0 && starts.contains(&Unit(u)) && lines.last().is_some_and(|l| !l.segments.is_empty()) {
                lines.push(WindowLine::default());
                last = None;
            }
            let Some(c) = r.char(u) else { continue };
            if c == '\n' {
                // A line break ends the line; it isn't text.
                lines.push(WindowLine::default());
                last = None;
                continue;
            }
            let m = r.marks(u);
            let key = (m.marks.clone(), m.author.clone(), m.lang.clone());
            let line = lines.last_mut().expect("a line was pushed");
            match line.segments.last_mut() {
                Some(s) if last.as_ref() == Some(&key) => s.text.push(c),
                _ => {
                    line.segments.push(WindowSegment { text: c.to_string(), marks: m.marks, author: m.author, lang: m.lang });
                    last = Some(key);
                }
            }
            map.push(u);
        }
        map.push(r.len());
        if id == para
            && let Some((_, at)) = focus.para_offset()
        {
            lang = r.marks(at.0.min(r.len().saturating_sub(1))).lang.unwrap_or_else(|| doc_lang.clone());
        }
        let level = heading_level(p);
        let role = match (level, p.style.num) {
            (Some(_), _) => AccessibleRole::Heading,
            (None, Some(_)) => AccessibleRole::ListItem,
            (None, None) => AccessibleRole::Paragraph,
        };
        let label = (role == AccessibleRole::ListItem).then(|| list_label(p)).flatten();
        paragraphs.push(WindowParagraph { id: AccessibleId(id.0), role, level, label, lines });
        units.push((id, map));
    }
    let mut w = Window { window: CaretWindow { paragraphs, all, lang, read_only, ..CaretWindow::default() }, units };
    let at = |p: Option<Position>, before: bool| p.and_then(|p| w.point(&p)).unwrap_or(if before { w.start() } else { w.end() });
    let ordered = e.selection().ordered(e.index());
    let anchor = e.selection().anchor();
    // An anchor outside the window is at its edge: the start when it is the selection's start.
    let anchor_first = ordered.is_some_and(|(a, _)| Some(a) == anchor);
    let (a, f) = (at(anchor, anchor_first), at(Some(focus), !anchor_first));
    w.window.anchor = a;
    w.window.focus = f;
    Some(w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::services::{AccessibleMark, AccessibleNode};
    use crate::font::Fonts;

    fn open(name: &str) -> (Editor, Fonts) {
        let fonts = Fonts::system();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor").join(name);
        let e = Editor::open(&std::fs::read(path).expect("fixture"), &fonts).expect("opens");
        (e, fonts)
    }

    /// The paragraph whose text starts with `start`.
    fn para(e: &Editor, start: &str) -> ParaId {
        fn find(n: &AccessibleNode, start: &str) -> Option<ParaId> {
            let text: String = n.children.iter().filter_map(|c| c.text.as_deref()).collect();
            if matches!(n.role, AccessibleRole::Paragraph | AccessibleRole::Heading | AccessibleRole::ListItem) && text.starts_with(start) {
                return Some(ParaId(n.id.0));
            }
            n.children.iter().find_map(|c| find(c, start))
        }
        find(&super::super::tree(e, &Spellers::default(), "en-US"), start).expect("the paragraph")
    }

    fn text(p: &WindowParagraph) -> String {
        p.lines.iter().flat_map(|l| &l.segments).map(|s| s.text.as_str()).collect()
    }

    fn at(e: &mut Editor, fonts: &Fonts, para: ParaId, a: u32, b: u32) -> Window {
        e.select_range(para, Unit(a), Unit(b), fonts);
        window(e, &Spellers::default(), "en-US", false).expect("a window")
    }

    #[test]
    fn the_window_holds_the_caret_paragraph_and_its_neighbours() {
        let (mut e, fonts) = open("FX-A11Y-STRUCTURE.docx");
        // The paragraph after the first heading.
        let intro = ParaId(super::super::tree(&e, &Spellers::default(), "en-US").children[1].id.0);
        let w = at(&mut e, &fonts, intro, 3, 3);
        let texts: Vec<String> = w.window.paragraphs.iter().map(text).collect();
        assert_eq!(texts[0], "Introduction", "the paragraph before");
        assert_eq!(w.window.paragraphs[1].id.0, intro.0, "the caret's paragraph");
        assert_eq!(w.window.paragraphs[0].role, AccessibleRole::Heading);
        assert_eq!(w.window.paragraphs[0].level, Some(1));
        assert!(texts.iter().any(|t| t == "First item"), "the paragraphs after it: {texts:?}");
        let labels: Vec<_> = w.window.paragraphs.iter().filter(|p| p.role == AccessibleRole::ListItem).map(|p| p.label.clone()).collect();
        assert_eq!(labels[0], Some("1.".to_string()), "a numbered item's label");
        assert!(w.window.paragraphs[1].lines.len() >= 2, "the long paragraph is in its laid-out lines");
        assert_eq!((w.window.anchor, w.window.focus), (WindowPoint { paragraph: 1, offset: 3 }, WindowPoint { paragraph: 1, offset: 3 }));
        assert_eq!(w.place(w.window.focus), Some((intro, Unit(3))), "a window place is the caret's");
        assert!(!w.window.all && !w.window.read_only);
        assert_eq!(w.window.lang, "en-US");
    }

    #[test]
    fn a_cell_window_holds_the_cell_and_a_selection_has_two_ends() {
        let (mut e, fonts) = open("FX-A11Y-STRUCTURE.docx");
        let ada = para(&e, "Ada");
        let w = at(&mut e, &fonts, ada, 0, 2);
        assert_eq!(w.window.paragraphs.iter().map(text).collect::<Vec<_>>(), ["Ada"], "only the cell's paragraphs");
        assert_eq!((w.window.anchor.offset, w.window.focus.offset), (0, 2));
        // A field shows its result, not its code.
        let links = para(&e, "Read the");
        let w = at(&mut e, &fonts, links, 0, 0);
        let line = w.window.paragraphs.iter().map(text).find(|t| t.starts_with("Read the")).expect("the links");
        assert_eq!(line, "Read the agenda and the minutes.");
    }

    #[test]
    fn marks_and_select_all() {
        let (mut e, fonts) = open("FX-A11Y-REVIEW.docx");
        let board = para(&e, "The board");
        let w = at(&mut e, &fonts, board, 0, 0);
        let segments: Vec<&WindowSegment> = w.window.paragraphs.iter().flat_map(|p| &p.lines).flat_map(|l| &l.segments).collect();
        let deleted = segments.iter().find(|s| s.marks.contains(&AccessibleMark::Deletion)).expect("the deletion");
        assert_eq!((deleted.text.as_str(), deleted.author.as_deref()), ("for next year ", Some("Grace")));
        e.execute(crate::api::command::Command::SelectAll, &fonts);
        let w = window(&mut e, &Spellers::default(), "en-US", false).expect("a window");
        assert!(w.window.all, "select all");
        assert_eq!(w.window.anchor, WindowPoint::default(), "the start is before the window: at its start");
    }
}
