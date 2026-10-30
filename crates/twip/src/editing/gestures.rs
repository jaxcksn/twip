//! Pointer gestures (`contextClick`, `textDoubleClick`, ...): which one a press or a release makes. The pointer state
//! machine ([`Editor::pointer`]) runs each gesture's default; the checks here are its own, in its order, and only name
//! the gesture, so a session can run another binding instead.

use crate::api::services::{AreaKind, GestureId, GestureTarget, MenuTarget};
use crate::font::Fonts;

use super::input::PointerKind;
use super::keymap::Mods;
use super::position::{Position, StoryId};
use super::selection::Selection;
use super::state::ObjectKind;
use super::{Editor, Mode, resize};

/// A gesture: which, the page it is on (from 1) and what it is over.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Gesture {
    pub id: GestureId,
    pub page: u32,
    pub target: GestureTarget,
}

fn area(kind: AreaKind) -> GestureTarget {
    GestureTarget::Area { kind }
}

const TEXT: GestureTarget = GestureTarget::Menu(MenuTarget::Text);

impl Editor {
    /// Pointer input at a document point, every gesture running its default. In design mode a double-click on a
    /// field or control also reports it, the one under the pointer; the selection is the usual double-click's.
    pub fn pointer_input(&mut self, kind: PointerKind, x: f64, y: f64, clicks: u8, mods: Mods, fonts: &Fonts) {
        let activate = (kind == PointerKind::Down && clicks == 2 && self.mode == Mode::Design).then(|| self.hit_laid_out(x, y));
        self.pointer(kind, x, y, clicks, mods, fonts);
        self.settle_provisional(fonts);
        if let Some(at) = activate {
            let object = matches!(self.selection, Selection::Object(_));
            match at.filter(|_| !object) {
                Some(p) => {
                    let kept = std::mem::replace(&mut self.selection, Selection::Caret(p));
                    self.activate(true);
                    self.selection = kept;
                }
                None => {
                    self.activate(true);
                }
            }
        }
    }

    /// The gesture a primary press at a document point makes, changing nothing. Presses that only place the caret,
    /// and those that start a drag (whose gesture is its release's), make none.
    // shortcut: a design-mode region title and a note reference take their press before the gestures below; those
    // presses are named as the gesture under them, which matters only when that gesture is rebound.
    pub(crate) fn press_gesture(&mut self, x: f64, y: f64, clicks: u8, mods: Mods, fonts: &Fonts) -> Option<Gesture> {
        use GestureId as G;
        let (page, _, py) = self.stack().page_at(x, y)?;
        self.ensure_page(page, fonts);
        let band = self.pages.page(page, &self.index)?.body_area;
        let found = |id, target| Some(Gesture { id, page: page as u32 + 1, target });
        let shift = mods.shift || (self.modes.extend > 0 && clicks == 1);
        let ctrl = mods.ctrl || mods.meta;
        // Headers and footers (`story_click`).
        let in_body = band.is_some_and(|(t, b)| py >= t && py <= b);
        let editing_hf = matches!(self.editing(), StoryId::HeaderFooter(_));
        match (editing_hf, in_body) {
            (false, false) if clicks >= 2 => {
                return if band.is_some_and(|(_, b)| py > b) {
                    found(G::FooterAreaDoubleClick, area(AreaKind::FooterArea))
                } else {
                    found(G::HeaderAreaDoubleClick, area(AreaKind::HeaderArea))
                };
            }
            (true, true) if clicks >= 2 => return found(G::BodyDoubleClickInHeaderFooter, TEXT),
            (true, true) if self.object_hit(x, y, fonts).is_none() => return None,
            _ => {}
        }
        if self.mode == Mode::Design
            && clicks == 2
            && let Some(p) = self.hit_laid_out(x, y)
            && let Some(&(id, _)) = super::elements::context(&self.doc, &self.index, &Selection::Caret(p)).first()
        {
            return found(G::ElementDoubleClick, GestureTarget::Menu(MenuTarget::Element { id: super::api_id(id) }));
        }
        if !shift && let Some(s) = self.object_hit(x, y, fonts) {
            let placeholder = self.mode != Mode::Design && self.with_selection(s, |e| e.selected_placeholder().is_some());
            return if clicks == 1 && placeholder { found(G::PlaceholderClick, GestureTarget::Menu(MenuTarget::Picture)) } else { None };
        }
        // A checkbox takes every click: it toggles, naming no gesture.
        if !shift && self.checkbox_toggles_at(x, y, fonts).is_some() {
            return None;
        }
        if clicks == 1
            && !shift
            && (ctrl || self.mode == Mode::ReadOnly)
            && let Some(address) = self.link_under(x, y, fonts)
        {
            return found(G::LinkActivate, GestureTarget::Menu(MenuTarget::Link { address }));
        }
        match self.border_at(x, y, fonts) {
            Some((_, _, resize::Border::Column(k), ..)) if clicks == 2 && k > 0 => {
                return found(G::TableBorderDoubleClick, area(AreaKind::TableBorder));
            }
            Some(_) if clicks < 2 => return None,
            _ => {}
        }
        if self.zone_hit(x, y, fonts).is_some() {
            return None;
        }
        if self.bar_hit(x, y, fonts).is_some() {
            return match clicks {
                _ if ctrl => None,
                1 => found(G::MarginClick, area(AreaKind::Margin)),
                2 => found(G::MarginDoubleClick, area(AreaKind::Margin)),
                _ => None,
            };
        }
        let p = self.hit(x, y, fonts)?;
        match clicks {
            2 => found(G::TextDoubleClick, TEXT),
            3.. => found(G::TextTripleClick, TEXT),
            1 if ctrl && !shift && !self.inside_selection(&p) => found(G::TextCtrlClick, TEXT),
            _ => None,
        }
    }

    /// The drag gesture a primary release at a document point ends: a table border, an object or the selected text
    /// moved since the press.
    pub(crate) fn release_gesture(&mut self, x: f64, y: f64) -> Option<Gesture> {
        let (id, target) = if self.border_drag.as_ref().is_some_and(|d| d.now != d.start) {
            (GestureId::TableBorderDrag, area(AreaKind::TableBorder))
        } else if self.object_drag.as_ref().is_some_and(|d| d.now != d.start) {
            let text_box = self.selected_object().is_some_and(|o| o.kind == ObjectKind::TextBox);
            (GestureId::ObjectDrag, GestureTarget::Menu(if text_box { MenuTarget::TextBox } else { MenuTarget::Picture }))
        } else if self.text_drag.as_ref().is_some_and(|d| d.moved) {
            (GestureId::TextDrag, TEXT)
        } else {
            return None;
        };
        let page = self.stack().page_at(x, y).map_or(self.near, |(p, ..)| p);
        Some(Gesture { id, page: page as u32 + 1, target })
    }

    /// A secondary press: the menu is for what is under the pointer, so the selection goes there, unless the press
    /// is inside it.
    pub(crate) fn context_press(&mut self, x: f64, y: f64, fonts: &Fonts) {
        self.history.break_typing();
        if let Some(s) = self.object_hit(x, y, fonts) {
            if self.selection != s {
                self.set_selection(s);
            }
            return;
        }
        let Some(p) = self.hit(x, y, fonts) else { return };
        let keep = match self.selection {
            Selection::Cells { table, .. } => self.index.chain_of(&p).iter().any(|l| l.table == table),
            Selection::Object(_) => false,
            Selection::Caret(c) => c == p,
            _ => self.inside_selection(&p),
        };
        if !keep {
            self.set_selection(Selection::Caret(p));
        }
    }

    /// The pointer left the view: nothing is hovered, unless a drag holds the pointer.
    pub(crate) fn pointer_leave(&mut self) {
        if self.drag.is_none() && self.text_drag.is_none() && self.object_drag.is_none() && self.border_drag.is_none() && self.zone_drag.is_none() {
            self.hover = None;
            self.hover_region = None;
        }
    }

    /// The address of the link under a document point. A link's text is one unit to hit testing, so a point on its right half hits
    /// the place after it: that place, left of the point, is the link's too.
    pub(super) fn link_under(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<String> {
        let p = self.hit(x, y, fonts)?;
        if let Some(link) = self.link_at(p) {
            return Some(link.address);
        }
        let (para, offset) = p.para_offset()?;
        let (_, px, _) = self.stack().page_at(x, y)?;
        let before = Position::text(p.story, para, crate::layout::Unit(offset.0.checked_sub(1)?));
        self.caret_at(p).filter(|c| px < c.x).and_then(|_| self.link_at(before)).map(|l| l.address)
    }

    /// `f` with `s` as the selection, which is restored after.
    fn with_selection<T>(&mut self, s: Selection, f: impl FnOnce(&mut Editor) -> T) -> T {
        let kept = std::mem::replace(&mut self.selection, s);
        let out = f(self);
        self.selection = kept;
        out
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::api::command::{Command, MoveSelection, MoveUnit};
    use crate::api::engine::{Engine, SessionId, SessionOptions};
    use crate::api::event::{Event, LinkVia};
    use crate::api::fonts::FontLibrary;
    use crate::api::input::{Modifiers, Point, PointerButton, PointerKind};
    use crate::api::services::{AreaKind, ClientRect, FindQuery, GestureAction, GestureEvent, GestureId, GestureTarget, MenuTarget, MenuVia};
    use crate::api::session::Session;
    use crate::api::state::SelectionKind;
    use crate::api::value::{Direction, Order, Story};

    const CTRL: Modifiers = Modifiers { ctrl: true, shift: false, alt: false, meta: false };
    const NONE: Modifiers = Modifiers { ctrl: false, shift: false, alt: false, meta: false };

    /// A session on a fixture, attached to a headless host, and the events it emits.
    struct Fixture {
        engine: Engine,
        id: SessionId,
        events: Rc<RefCell<Vec<Event>>>,
    }

    impl Fixture {
        fn open(name: &str) -> Fixture {
            let mut engine = Engine::new(FontLibrary::system());
            let events: Rc<RefCell<Vec<Event>>> = Rc::default();
            let sink = events.clone();
            engine.set_listener(move |_, e| sink.borrow_mut().push(e.clone()));
            let id = engine.create_session(SessionOptions::default());
            let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).expect("fixture");
            let s = engine.session(id).expect("a session");
            s.open(&bytes).expect("opens");
            s.host().attach(|_| {});
            s.host().set_size(800.0, 600.0, 1.0);
            events.borrow_mut().clear();
            Fixture { engine, id, events }
        }

        fn s(&mut self) -> &mut Session {
            self.engine.session(self.id).expect("a session")
        }

        /// The first page, viewport px.
        fn page(&mut self) -> ClientRect {
            self.s().host().visible_pages()[0].rect
        }

        /// The middle of the first occurrence of `text` (on one line); the caret goes to its start.
        fn at(&mut self, text: &str) -> Point {
            let s = self.s();
            assert!(s.find(&FindQuery { text: text.into(), ..FindQuery::default() }, Order::Next).found, "{text}");
            let end = s.host().frame_info();
            let back = MoveSelection { by: MoveUnit::Character, direction: Direction::Backward, extend: None };
            s.run(Command::MoveSelection(back)).expect("moves");
            let start = s.host().frame_info();
            Point { x: (start.caret_x + end.caret_x) / 2.0, y: start.caret_y + start.caret_height / 2.0 }
        }

        fn select(&mut self, text: &str) {
            assert!(self.s().find(&FindQuery { text: text.into(), ..FindQuery::default() }, Order::Next).found, "{text}");
        }

        /// A press and release of the primary button.
        fn click(&mut self, at: Point, clicks: u8, mods: Modifiers) {
            for kind in [PointerKind::Down, PointerKind::Up] {
                self.s().pointer(kind, at, PointerButton::Primary, clicks, mods);
            }
        }

        /// A click, then a double-click, as a host reports them.
        fn double_click(&mut self, at: Point) {
            self.click(at, 1, NONE);
            self.click(at, 2, NONE);
        }

        /// A primary-button drag.
        fn drag(&mut self, from: Point, to: Point) {
            let s = self.s();
            s.pointer(PointerKind::Down, from, PointerButton::Primary, 1, NONE);
            s.pointer(PointerKind::Move, Point { x: from.x + 20.0, ..from }, PointerButton::Primary, 0, NONE);
            s.pointer(PointerKind::Move, to, PointerButton::Primary, 0, NONE);
            s.pointer(PointerKind::Up, to, PointerButton::Primary, 1, NONE);
        }

        fn has(&mut self, text: &str) -> bool {
            !self.s().find_all(&FindQuery { text: text.into(), ..FindQuery::default() }).is_empty()
        }

        fn take(&mut self) -> Vec<Event> {
            std::mem::take(&mut *self.events.borrow_mut())
        }

        fn gestures(&mut self) -> Vec<GestureEvent> {
            let gesture = |e| match e {
                Event::Gesture(g) => Some(g),
                _ => None,
            };
            self.take().into_iter().filter_map(gesture).collect()
        }

        fn undo(&mut self) {
            self.s().run(Command::Undo).expect("undoes");
        }
    }

    #[test]
    fn margin_double_clicks_edit_headers_and_footers_and_the_body_closes_them() {
        let mut f = Fixture::open("editor/FX-bad-image.docx");
        let page = f.page();
        let header = Point { x: page.left + page.width / 2.0, y: page.top + 30.0 };
        let footer = Point { x: header.x, y: page.top + page.height - 30.0 };
        let body = f.at("Type here.");
        f.double_click(header);
        let hf = f.s().state().selection.header_footer.clone().expect("a header is edited");
        assert_eq!((f.s().state().selection.story, hf.page), (Story::Header, 1));
        f.double_click(body);
        assert_eq!(f.s().state().selection.header_footer, None, "the body closes it");
        f.double_click(footer);
        assert_eq!(f.s().state().selection.story, Story::Footer);
        // A single click in the body does nothing while a footer is edited.
        f.click(body, 1, NONE);
        assert_eq!(f.s().state().selection.story, Story::Footer);
    }

    #[test]
    fn text_and_margin_gestures_select_by_their_unit() {
        let mut f = Fixture::open("editor/FX-bad-image.docx");
        // A double-click: the word, with its space.
        let at = f.at("Type");
        f.double_click(at);
        f.s().text("X");
        assert!(f.has("Xhere.") && !f.has("Type"));
        f.undo();
        // A triple-click: the paragraph.
        let at = f.at("here.");
        for clicks in 1..=3 {
            f.click(at, clicks, NONE);
        }
        f.s().text("X");
        assert!(!f.has("here."));
        f.undo();
        // Ctrl+click: the sentence.
        let at = f.at("for more");
        f.click(at, 1, CTRL);
        f.s().text("X");
        assert!(!f.has("Visit") && !f.has("for more"));
        f.undo();
        // A click left of a line selects the line, a double-click there the paragraph.
        let page = f.page();
        let line = f.at("Type here.");
        let margin = Point { x: page.left + 60.0, y: line.y };
        f.click(margin, 1, NONE);
        f.s().text("X");
        assert!(!f.has("Type here."));
        f.undo();
        f.double_click(margin);
        f.s().text("X");
        assert!(!f.has("Type here."));
    }

    #[test]
    fn hovering_a_link_shows_its_address() {
        let mut f = Fixture::open("editor/FX-bad-image.docx");
        let link = f.at("the example site");
        f.take();
        f.s().pointer(PointerKind::Move, link, PointerButton::Primary, 0, NONE);
        let tip = f.take().into_iter().find_map(|e| if let Event::Tooltip(t) = e { t } else { None }).expect("a tooltip");
        assert_eq!((tip.kind, tip.text.as_str()), (crate::api::services::TooltipKind::Link, "https://example.com/"));
        assert!(tip.rect.left <= link.x && link.x <= tip.rect.left + tip.rect.width && tip.at.client_y >= link.y);
        // Moving off it takes the tooltip away.
        f.s().pointer(PointerKind::Move, Point { x: 5.0, y: 5.0 }, PointerButton::Primary, 0, NONE);
        assert!(f.take().contains(&Event::Tooltip(None)));
    }

    #[test]
    fn links_activate_and_a_right_click_opens_the_menu_there() {
        let mut f = Fixture::open("editor/FX-bad-image.docx");
        let link = f.at("the example site");
        f.take();
        f.click(link, 1, CTRL);
        let events = f.take();
        let activated = |e: &Event| matches!(e, Event::LinkActivated { address, via: LinkVia::CtrlClick } if address == "https://example.com/");
        assert!(events.iter().any(activated), "{events:?}");
        // Its right half too, where hits snap to the place after it.
        f.select("the example site");
        let end = f.s().host().frame_info();
        f.click(Point { x: end.caret_x - 3.0, ..link }, 1, CTRL);
        assert!(f.take().iter().any(activated));
        // A plain click on a link places the caret.
        f.click(link, 1, NONE);
        assert!(!f.take().iter().any(|e| matches!(e, Event::LinkActivated { .. })));
        // A right-click elsewhere: the caret goes under the pointer, and the menu opens there for it.
        let _ = f.at("Type here.");
        f.take();
        f.s().pointer(PointerKind::Down, link, PointerButton::Secondary, 1, NONE);
        f.s().pointer(PointerKind::Up, link, PointerButton::Secondary, 1, NONE);
        let menus: Vec<_> = f
            .take()
            .into_iter()
            .filter_map(|e| match e {
                Event::ContextMenu(m) => Some(m),
                _ => None,
            })
            .collect();
        assert_eq!(menus.len(), 1);
        assert_eq!((menus[0].via, menus[0].at.client_x, menus[0].at.client_y), (MenuVia::Pointer, link.x, link.y));
        assert!(matches!(menus[0].menu.target, MenuTarget::Link { .. }), "{:?}", menus[0].menu.target);
        assert!(f.s().state().selection.link.is_some(), "the caret is in the link");
    }

    #[test]
    fn bindings_emit_ignore_or_run_an_op_and_reset() {
        let mut f = Fixture::open("editor/FX-bad-image.docx");
        let bindings = f.s().gestures().bindings();
        assert_eq!(bindings.len(), 20);
        assert!(bindings.iter().all(|b| b.action == GestureAction::Default && !b.label.is_empty()));
        let at = f.at("Type");
        f.click(at, 1, NONE);
        let revision = f.s().state().revision;
        // Emit: one event, and nothing else happens.
        f.s().gestures().set(GestureId::TextDoubleClick, GestureAction::Emit).expect("binds");
        let selection = f.s().state().selection.clone();
        f.take();
        f.click(at, 2, NONE);
        let g = f.gestures();
        assert_eq!(g.len(), 1, "{g:?}");
        assert_eq!((g[0].gesture, g[0].page, &g[0].target), (GestureId::TextDoubleClick, 1, &GestureTarget::Menu(MenuTarget::Text)));
        assert_eq!((g[0].at.client_x, g[0].at.client_y), (at.x, at.y));
        assert_eq!(f.s().state().selection, selection, "no word selected");
        // None: nothing.
        f.s().gestures().set(GestureId::TextDoubleClick, GestureAction::None).expect("binds");
        f.click(at, 2, NONE);
        assert!(f.take().is_empty());
        // An op: it runs, as one transaction.
        f.s().gestures().set(GestureId::TextDoubleClick, GestureAction::Op(Command::InsertText("Z".into()))).expect("binds");
        f.click(at, 2, NONE);
        let changes = f.take().into_iter().filter(|e| matches!(e, Event::DocumentChanged(_))).count();
        assert_eq!((changes, f.s().state().revision), (1, revision + 1));
        // Reset: the default again.
        f.s().gestures().reset(None);
        assert!(f.s().gestures().bindings().iter().all(|b| b.action == GestureAction::Default));
        f.click(at, 1, NONE);
        f.click(at, 2, NONE);
        assert!(f.gestures().is_empty());
        assert_eq!(f.s().state().selection.kind, SelectionKind::Text, "a word selected");
        // The right-click emitted: the menu's target, and no menu.
        f.s().gestures().set(GestureId::ContextClick, GestureAction::Emit).expect("binds");
        f.s().pointer(PointerKind::Down, at, PointerButton::Secondary, 1, NONE);
        let events = f.take();
        assert!(!events.iter().any(|e| matches!(e, Event::ContextMenu(_))));
        assert!(events.iter().any(|e| matches!(e, Event::Gesture(g) if g.gesture == GestureId::ContextClick)), "{events:?}");
        f.s().gestures().reset(Some(GestureId::ContextClick));
        assert!(f.s().gestures().bindings().iter().all(|b| b.action == GestureAction::Default));
    }

    #[test]
    fn table_borders_fit_and_move_unless_rebound() {
        let mut f = Fixture::open("editor/FX-TBL-BASIC.docx");
        // The border left of a cell: its text's start less the cell's 5.4 pt margin.
        let text = f.at("T2 R1 C2");
        let start = f.s().host().frame_info();
        let border = Point { x: start.caret_x - 5.4 * 4.0 / 3.0, y: text.y };
        let revision = f.s().state().revision;
        f.s().gestures().set(GestureId::TableBorderDoubleClick, GestureAction::Emit).expect("binds");
        f.take();
        f.click(border, 1, NONE);
        f.click(border, 2, NONE);
        let g = f.gestures();
        assert_eq!(
            g.iter().map(|g| (g.gesture, &g.target)).collect::<Vec<_>>(),
            [(GestureId::TableBorderDoubleClick, &GestureTarget::Area { kind: AreaKind::TableBorder })]
        );
        assert_eq!(f.s().state().revision, revision);
        f.s().gestures().set(GestureId::TableBorderDrag, GestureAction::None).expect("binds");
        f.drag(border, Point { x: border.x + 40.0, ..border });
        assert_eq!(f.s().state().revision, revision, "nothing moved");
        // The defaults: a drag moves the border, a double-click fits the column.
        f.s().gestures().reset(None);
        f.drag(border, Point { x: border.x + 40.0, ..border });
        assert_eq!(f.s().state().revision, revision + 1);
        let moved = Point { x: border.x + 40.0, ..border };
        f.click(moved, 1, NONE);
        f.click(moved, 2, NONE);
        assert_eq!(f.s().state().revision, revision + 2);
    }

    #[test]
    fn a_text_drag_moves_the_text_unless_rebound() {
        let mut f = Fixture::open("editor/FX-bad-image.docx");
        let to = f.at("for more");
        let from = f.at("Type");
        f.select("Type");
        f.s().gestures().set(GestureId::TextDrag, GestureAction::Emit).expect("binds");
        let revision = f.s().state().revision;
        f.take();
        f.drag(from, to);
        assert_eq!(f.gestures().iter().map(|g| g.gesture).collect::<Vec<_>>(), [GestureId::TextDrag]);
        assert_eq!(f.s().state().revision, revision, "nothing moved");
        f.s().gestures().reset(None);
        f.select("Type");
        f.drag(from, to);
        assert_eq!(f.s().state().revision, revision + 1);
        assert!(!f.has("Type here."));
    }
}
