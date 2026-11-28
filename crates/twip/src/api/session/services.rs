//! Menus, dialogs, find, spelling, navigation, styles, accessibility and word count.

use super::{Commit, Session};
use crate::api::command::Command;
use crate::api::engine::Dictionary;
use crate::api::error::{Error, ErrorCode};
use crate::api::event::Event;
use crate::api::record::Origin;
use crate::api::services::{
    AccessibleNode, ContextMenu, DialogKind, DialogModel, DialogPatch, FindCount, FindDirection, FindFormat, FindMatch, FindMatches, FindQuery,
    FindResult, Heading, MenuAction, Misspelling, StyleInfo, StyleType, WordCount,
};
use crate::api::state::State;
use crate::api::value::{HeadingId, MatchId, Order};
use crate::editing::commands::{Dir, Outcome, deleting};
use crate::layout::Unit;
use crate::services::{dialogs, find, menu};

/// The most labels the undo and redo lists give: Word's list length.
const HISTORY_LIST: u32 = 100;

/// The spelling and review values of the state, which the services keep.
pub(super) fn fill_state(s: &mut Session, state: &mut State) {
    let sp = &mut s.services.spelling;
    state.spelling.enabled = sp.enabled;
    state.spelling.misspellings = sp.count(&s.editor, &s.language);
}

/// `now`, where it differs from `was`.
fn changed<T: PartialEq + Clone>(now: &Option<T>, was: &Option<T>) -> Option<T> {
    if now != was { now.clone() } else { None }
}

/// The fields of Set Defaults that changed.
fn defaults_changed(
    now: &crate::api::command::DocumentDefaults,
    was: &crate::api::command::DocumentDefaults,
) -> crate::api::command::DocumentDefaults {
    use crate::api::command::{DocumentDefaults, Indents, ParagraphSpacing};
    let spacing = now.spacing.as_ref().map(|s| {
        let w = was.spacing.clone().unwrap_or_default();
        ParagraphSpacing {
            before: changed(&s.before, &w.before),
            after: changed(&s.after, &w.after),
            contextual: changed(&s.contextual, &w.contextual),
        }
    });
    let indents = now.indents.as_ref().map(|i| {
        let w = was.indents.clone().unwrap_or_default();
        Indents {
            left: changed(&i.left, &w.left),
            right: changed(&i.right, &w.right),
            first_line: changed(&i.first_line, &w.first_line),
            hanging: changed(&i.hanging, &w.hanging),
            mirror: changed(&i.mirror, &w.mirror),
        }
    });
    DocumentDefaults {
        font: changed(&now.font, &was.font),
        size: changed(&now.size, &was.size),
        color: changed(&now.color, &was.color),
        spacing: spacing.filter(|s| *s != ParagraphSpacing::default()),
        line_spacing: changed(&now.line_spacing, &was.line_spacing),
        indents: indents.filter(|i| *i != Indents::default()),
    }
}

impl Session {
    /// The context menu for the selection, as the context-menu key would open it.
    pub fn context_menu(&mut self) -> ContextMenu {
        self.context_menu_with(None)
    }

    /// The context menu for the selection, with a Synonyms submenu of `synonyms` (the host's thesaurus's words for
    /// [`Session::word_at_selection`], at most 8 shown; an empty list is an empty submenu); `None` shows none.
    pub fn context_menu_with(&mut self, synonyms: Option<Vec<String>>) -> ContextMenu {
        let mut facts = menu::MenuFacts::of(&self.editor);
        facts.synonyms = synonyms.map(|mut w| {
            w.truncate(8);
            w
        });
        let misspelled = self.services.spelling.at_selection(&self.editor, &self.language).map(|f| menu::Misspelled {
            suggestions: if f.error.repeated { Vec::new() } else { self.services.spelling.suggestions(&f.word, &f.language) },
            repeated: f.error.repeated,
            word: f.word,
            language: f.language,
        });
        menu::build_with(&self.state, misspelled.as_ref(), &facts)
    }

    /// The word the selection is in or selects, for the host's thesaurus: `None` unless the selection is in one
    /// paragraph and within one word of at most 64 characters.
    pub fn word_at_selection(&self) -> Option<crate::api::services::WordAt> {
        let w = crate::services::spelling::word_at_selection(&self.editor, &self.language)?;
        Some(crate::api::services::WordAt { word: w.word, language: w.language })
    }

    /// The floating objects on page `page` (from 0) the selection's story reaches, front first, hidden ones too,
    /// each group's objects within it, as Word's Selection Pane lists them. Empty past the last page.
    pub fn object_list(&mut self, page: u32) -> Vec<crate::api::state::ObjectListEntry> {
        fn entry(e: crate::editing::object_props::ObjectEntry) -> crate::api::state::ObjectListEntry {
            crate::api::state::ObjectListEntry {
                object: crate::api::value::ObjectId { path: e.path, ..crate::api::value::ObjectId::of(e.id) },
                name: e.name,
                kind: e.kind,
                hidden: e.hidden,
                children: e.children.into_iter().map(entry).collect(),
            }
        }
        let fonts = self.fonts.fonts();
        self.editor.object_list(page as usize, &fonts).into_iter().map(entry).collect()
    }

    /// The selected picture's image (a group child's when one is selected), as the document holds it; `None` without
    /// one.
    pub fn object_image(&self) -> Option<crate::api::value::Bytes> {
        self.editor.object_image().map(|b| crate::api::value::Bytes(b.to_vec()))
    }

    /// The selected shape's picture fill's image; `None` without one.
    pub fn shape_fill_image(&self) -> Option<crate::api::value::Bytes> {
        self.editor.shape_fill_image().map(|b| crate::api::value::Bytes(b.to_vec()))
    }

    /// Section `section`'s (from 0) picture watermark's image; `None` without one.
    pub fn watermark_image(&self, section: u32) -> Option<crate::api::value::Bytes> {
        self.editor.watermark_image(section as usize).map(|b| crate::api::value::Bytes(b.to_vec()))
    }

    /// The page colour's picture or texture image; `None` without one.
    pub fn page_fill_image(&self) -> Option<crate::api::value::Bytes> {
        self.editor.page_fill_image().map(|b| crate::api::value::Bytes(b.to_vec()))
    }

    /// The rows and columns `convertTextToTable` with `options` would make of the selection (Convert Text to Table's
    /// Number of rows and columns); `None` when it can't convert it (it crosses a table, holds a content control).
    pub fn text_to_table_size(&self, options: &crate::api::command::TextToTable) -> Option<crate::api::command::TableSize> {
        self.editor.text_to_table_size(options).map(|(rows, columns)| crate::api::command::TableSize { rows, columns })
    }

    /// Runs a menu item as one transaction.
    pub fn invoke(&mut self, action: MenuAction) -> Result<Commit, Error> {
        match action {
            MenuAction::Command { command } => self.run(*command),
            MenuAction::ReplaceWord { with } => {
                let Some(f) = self.services.spelling.at_selection(&self.editor, &self.language) else {
                    // A synonym: the word at the selection.
                    let w = crate::services::spelling::word_at_selection(&self.editor, &self.language)
                        .filter(|_| !with.is_empty())
                        .ok_or_else(|| Error::unavailable("no misspelling or word is selected"))?;
                    self.refuse_edit(crate::api::command::CommandId::InsertText)?;
                    return self.apply_with(Origin::Api, |e, fonts| {
                        e.joined("Replace", fonts, |e| {
                            e.select_range(w.para, w.start, w.end, fonts);
                            e.execute(Command::InsertText(with), fonts)
                        })
                    });
                };
                self.refuse_read_only()?;
                // A doubled word goes with the space before it.
                let start =
                    if with.is_empty() && f.error.repeated && f.error.start > 0 && self.editor.space_at(f.para, Unit(f.error.start as u32 - 1)) {
                        f.error.start - 1
                    } else {
                        f.error.start
                    };
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
            MenuAction::OpenLink { address } => match address.strip_prefix('#') {
                Some(bookmark) => self.run(Command::GoToBookmark(bookmark.to_string())),
                None => {
                    self.emit(Event::LinkActivated { address, via: crate::api::event::LinkVia::Menu });
                    Ok(Commit { record: None, value: None })
                }
            },
            MenuAction::CopyLink { address } => {
                let data = crate::api::input::ClipboardData { text: address, html: None, internal: None, image: None };
                self.request(super::HostRequest::Clipboard { data });
                self.deliver_requests();
                Ok(Commit { record: None, value: None })
            }
            MenuAction::Commands { commands, label } => self.batch(commands, Some(label)),
            MenuAction::SetDefaultPictureWrap { wrap } => {
                self.set_default_picture_wrap(wrap);
                Ok(Commit { record: None, value: None })
            }
            MenuAction::Drop { what } => {
                use crate::api::services::DropChoice as A;
                use crate::editing::move_to::DropChoice as D;
                if self.editor.drop_menu().is_none() {
                    return Err(Error::unavailable("no drop menu is open"));
                }
                let what = match what {
                    A::Move => D::Move,
                    A::Copy => D::Copy,
                    A::Link => D::Link,
                    A::Hyperlink => D::Hyperlink,
                    A::Cancel => D::Cancel,
                };
                if what != D::Cancel {
                    self.refuse_edit(crate::api::command::CommandId::Paste)?;
                }
                self.apply_with(Origin::User, |e, fonts| e.choose_drop(what, fonts))
            }
        }
    }

    fn refuse_read_only(&self) -> Result<(), Error> {
        if self.read_only { Err(Error::new(ErrorCode::ReadOnly, "the session is read only")) } else { Ok(()) }
    }

    /// An edit that isn't a command's but would be command `id`'s: refused as `id` would be in a read-only session or
    /// a protected document.
    fn refuse_edit(&self, id: crate::api::command::CommandId) -> Result<(), Error> {
        self.refuse_read_only()?;
        self.editor.protection_refuses(id).map_or(Ok(()), Err)
    }

    /// What dialogs read besides the state.
    fn dialog_context(&mut self, kind: DialogKind) -> dialogs::Context {
        let mut cx = dialogs::Context::default();
        match kind {
            DialogKind::Hyperlink => {
                cx.selected_text = self.editor.selected_text();
                cx.bookmarks = self.editor.bookmarks().into_iter().map(|b| b.name).filter(|n| !n.starts_with('_')).collect();
            }
            DialogKind::Accessibility => cx.issues = dialogs::issues(self.editor.document()),
            DialogKind::Font => cx.fonts = self.fonts.families(),
            DialogKind::BordersShading => cx.in_table = self.editor.in_table(),
            DialogKind::TableProperties => cx.cell_margins = self.editor.cell_margins_here(),
            DialogKind::PictureSize => cx.original_size = self.editor.original_object_size(),
            _ => {}
        }
        cx
    }

    /// Manage Styles' values: the styles in `sort` order (only those that show in lists, with `recommended_only`), the
    /// selected one's description, and the document defaults.
    fn manage_styles(&self, mut v: crate::api::services::ManageStylesDialog) -> crate::api::services::ManageStylesDialog {
        use crate::api::services::{ManagedStyle, StyleSort, StyleVisibility};
        let mut styles: Vec<ManagedStyle> = self
            .styles(None)
            .into_iter()
            .filter(|s| !v.recommended_only || s.visibility == StyleVisibility::Show)
            .map(|s| ManagedStyle {
                id: s.id,
                name: s.name,
                kind: s.kind,
                built_in: s.built_in,
                visibility: s.visibility,
                priority: s.priority,
                locked: s.locked,
            })
            .collect();
        match v.sort {
            StyleSort::AsRecommended => {}
            StyleSort::Alphabetical => styles.sort_by_key(|s| s.name.to_lowercase()),
            StyleSort::ByType => styles.sort_by_key(|s| s.kind as u8),
            StyleSort::BasedOn => {
                let base: std::collections::HashMap<String, String> =
                    self.styles(None).into_iter().map(|s| (s.id.0, s.based_on.map(|b| b.0).unwrap_or_default())).collect();
                styles.sort_by(|a, b| base.get(&a.id.0).cmp(&base.get(&b.id.0)).then(a.name.cmp(&b.name)));
            }
        }
        // The user's changes to visibility, priority and locks stay over a re-sort.
        for s in &mut styles {
            if let Some(was) = v.styles.iter().find(|w| w.id == s.id) {
                (s.visibility, s.priority, s.locked) = (was.visibility, was.priority, was.locked);
            }
        }
        v.styles = styles;
        let res = self.editor.document().resolver.clone();
        v.description = match (&res, &v.selected) {
            (Some(r), Some(id)) => {
                let c = r.cascade();
                c.styles.styles.get(&*id.0).map(|e| crate::services::styles::describe::describe(e, &c)).unwrap_or_default()
            }
            _ => String::new(),
        };
        v
    }

    /// The document defaults as Set Defaults shows them.
    fn defaults_dialog_values(&self) -> crate::api::command::DocumentDefaults {
        use crate::api::command::{DocumentDefaults, Indents, ParagraphSpacing};
        use crate::api::value::{AutoOr, Color, LineSpacing, Points};
        let Some(r) = self.editor.document().resolver.as_ref() else { return DocumentDefaults::default() };
        let c = r.cascade();
        let (rpr, ppr) =
            (&crate::docx::char_props::to_run_props(c.doc_char(), &c.colours), &crate::docx::para_props::to_para_props(c.doc_para(), &c.colours));
        let font = rpr.ascii.as_ref().map(|f| match f {
            crate::docx::props::FontRef::Name(n) => n.clone(),
            crate::docx::props::FontRef::Theme(t) => c.theme.resolve(t),
        });
        DocumentDefaults {
            font: font.map(crate::api::command::FontChoice::Name),
            size: rpr.size.map(Points),
            color: Some(rpr.color.flatten().map_or(AutoOr::Auto, |c| AutoOr::Value(crate::api::drawing::ColourValue::rgb(Color(c))))),
            spacing: Some(ParagraphSpacing {
                before: Some(AutoOr::Value(Points(ppr.space_before.unwrap_or(0.0)))),
                after: Some(AutoOr::Value(Points(ppr.space_after.unwrap_or(0.0)))),
                contextual: None,
            }),
            line_spacing: Some(match (ppr.line, ppr.line_rule) {
                (Some(l), Some(crate::docx::props::LineRule::Exact)) => LineSpacing::Exact { size: Points(l / 20.0) },
                (Some(l), Some(crate::docx::props::LineRule::AtLeast)) => LineSpacing::AtLeast { size: Points(l / 20.0) },
                (Some(l), _) => LineSpacing::Multiple { lines: l / 240.0 },
                (None, _) => LineSpacing::Multiple { lines: 1.0 },
            }),
            indents: Some(Indents {
                left: Some(Points(ppr.ind_left.unwrap_or(0.0))),
                right: Some(Points(ppr.ind_right.unwrap_or(0.0))),
                first_line: None,
                hanging: None,
                mirror: None,
            }),
        }
    }

    /// Borders and Shading's values as they are now: the selection's, or text in a style's XML. The shading applies
    /// to the text when the selection is in one paragraph, or the style is a character style.
    fn borders_shading(&mut self, style: Option<&str>) -> Result<crate::api::services::BordersShadingDialog, Error> {
        let text = match style {
            Some(raw) => raw.contains("w:type=\"character\""),
            None => {
                let t = self.editor.selected_text();
                !t.is_empty() && !t.contains('\n')
            }
        };
        let target = match () {
            _ if style.is_none() && self.editor.cells_selected() => crate::api::command::ShadingTarget::Cell,
            _ if text => crate::api::command::ShadingTarget::Text,
            _ => crate::api::command::ShadingTarget::Paragraph,
        };
        self.borders_shading_for(style, target)
    }

    /// Borders and Shading's values for the paragraphs, the text, or the selection's cells or table.
    fn borders_shading_for(
        &self,
        style: Option<&str>,
        target: crate::api::command::ShadingTarget,
    ) -> Result<crate::api::services::BordersShadingDialog, Error> {
        use crate::api::command::ShadingTarget;
        match target {
            ShadingTarget::Cell | ShadingTarget::Table if style.is_none() => {
                self.editor.table_borders_shading(target).ok_or_else(|| Error::unavailable("the selection isn't in a table"))
            }
            _ => self
                .editor
                .borders_shading_values(style, target == ShadingTarget::Text)
                .ok_or_else(|| Error::unavailable("the selection has no paragraph")),
        }
    }

    /// The Frame dialog's properties: the selection's frame's, or a style's.
    fn frame(&self, style: Option<&str>) -> Result<crate::api::command::FrameProperties, Error> {
        self.editor.frame_properties(style).ok_or_else(|| Error::unavailable("the selection isn't in a frame"))
    }

    /// Opens a dialog on the selection's values.
    pub fn dialog_open(&mut self, kind: DialogKind) -> Result<DialogModel, Error> {
        if kind == DialogKind::BordersShading {
            return Ok(dialogs::borders_shading_model(self.borders_shading(None)?, self.editor.in_table()));
        }
        if kind == DialogKind::Frame {
            return Ok(dialogs::frame_model(crate::api::services::FrameDialog::of(self.frame(None)?)));
        }
        if kind.is_list() {
            let v = self.editor.list_dialog_values(kind, None).ok_or_else(|| Error::unavailable("the selection isn't in a list"))?;
            return Ok(dialogs::list_model(v));
        }
        if kind == DialogKind::ManageStyles {
            let first = self.styles(None).into_iter().next().map(|s| s.id);
            let v = crate::api::services::ManageStylesDialog { selected: first, defaults: self.defaults_dialog_values(), ..Default::default() };
            return Ok(dialogs::manage_styles_model(self.manage_styles(v)));
        }
        let cx = self.dialog_context(kind);
        dialogs::open(kind, &self.state, &cx)
    }

    /// Draws blocks laid out `size.width` px wide into a picture of `size`, on the page's colour as the canvas shows it.
    fn picture(
        &mut self,
        blocks: Vec<crate::docx::Block>,
        size: crate::api::services::PreviewSize,
        fit: bool,
    ) -> Result<crate::api::services::PreviewPicture, Error> {
        if !(size.width > 0.0 && size.height > 0.0 && size.scale > 0.0) || size.width * size.scale > 4096.0 || size.height * size.scale > 4096.0 {
            return Err(Error::invalid("a preview is 1 to 4096 device pixels on each side"));
        }
        let fonts = self.fonts.fonts();
        let width = size.width * 72.0 / 96.0;
        let (height, children) = crate::layout::expansion::lay(
            self.editor.document(),
            &self.editor.document().review,
            &blocks,
            if fit { f64::MAX / 4.0 } else { width },
            &fonts,
            None,
            self.editor.hyphenation(),
        );
        let items = crate::layout::traverse::items_of(&children);
        let appearance = self.presentation.appearance();
        let (png, w, h) = crate::render::preview::png(
            &items,
            &fonts,
            height,
            &self.editor.document().images,
            (size.width, size.height, size.scale),
            fit,
            appearance,
        )
        .ok_or_else(|| Error::new(ErrorCode::Internal, "the preview couldn't be drawn"))?;
        Ok(crate::api::services::PreviewPicture { png: crate::api::value::Bytes(png), width: w, height: h })
    }

    /// A style's gallery tile: its sample text in the style's character formatting, scaled to fit.
    pub fn style_tile(
        &mut self,
        id: &crate::api::value::StyleId,
        size: crate::api::services::PreviewSize,
    ) -> Result<crate::api::services::PreviewPicture, Error> {
        let raw = self
            .editor
            .document()
            .resolver
            .as_ref()
            .and_then(|r| r.typed().style_xml(&id.0).map(std::borrow::Cow::into_owned))
            .ok_or_else(|| Error::unavailable("no such style"))?;
        let blocks = self.editor.style_preview(&raw, true).ok_or_else(|| Error::invalid("not a style"))?;
        self.picture(blocks, size, true)
    }

    /// The characters a font has, for a symbol picker: runs of code points in its Basic Multilingual Plane.
    pub fn font_characters(&self, family: &str) -> Result<Vec<crate::api::services::CharRange>, Error> {
        let fonts = self.fonts.fonts();
        let face = fonts
            .face(family, None, false, false)
            .filter(|f| f.family.eq_ignore_ascii_case(family))
            .ok_or_else(|| Error::unavailable("no such font"))?;
        Ok(face.character_ranges().into_iter().map(|(first, last)| crate::api::services::CharRange { first, last }).collect())
    }

    /// A character in a font, drawn to fit a picture of `size`, for a symbol picker.
    pub fn character_picture(
        &mut self,
        family: &str,
        c: char,
        size: crate::api::services::PreviewSize,
    ) -> Result<crate::api::services::PreviewPicture, Error> {
        let blocks = self.editor.character_preview(family, c).ok_or_else(|| Error::unavailable("no styles"))?;
        self.picture(blocks, size, true)
    }

    /// A list gallery's sections: the built-in library, the recently used, the document's lists, and its list styles.
    pub fn list_galleries(&self, kind: crate::api::services::ListGalleryKind) -> Vec<crate::api::services::ListGallerySection> {
        self.editor.list_galleries(kind)
    }

    /// A list gallery entry's preview: three items, or three levels of a multilevel list.
    pub fn list_entry_preview(
        &mut self,
        id: &crate::api::services::ListEntryId,
        size: crate::api::services::PreviewSize,
    ) -> Result<crate::api::services::PreviewPicture, Error> {
        let blocks = self.editor.list_entry_preview(id).ok_or_else(|| Error::invalid("no such list entry"))?;
        self.picture(blocks, size, true)
    }

    /// An open dialog's preview: for the style dialog, its preview box with the changes not yet applied.
    pub fn dialog_preview(
        &mut self,
        model: &DialogModel,
        size: crate::api::services::PreviewSize,
    ) -> Result<crate::api::services::PreviewPicture, Error> {
        if matches!(model.kind, DialogKind::DefineBullet | DialogKind::DefineNumberFormat | DialogKind::DefineMultilevelList) {
            let blocks = self.editor.list_dialog_preview(&model.values).ok_or_else(|| Error::invalid("no list to preview"))?;
            return self.picture(blocks, size, true);
        }
        let crate::api::services::DialogValues::Style(v) = &model.values else { return Err(Error::unavailable("this dialog has no preview")) };
        match v.kind {
            crate::api::services::StyleDialogType::Table => {
                let blocks = self.editor.table_style_preview(&v.pending).ok_or_else(|| Error::invalid("not a style"))?;
                return self.picture(blocks, size, true);
            }
            crate::api::services::StyleDialogType::List if !v.list_levels.is_empty() => {
                let blocks = self.editor.list_preview(&v.list_levels, true).ok_or_else(|| Error::invalid("not a style"))?;
                return self.picture(blocks, size, true);
            }
            _ => {}
        }
        let blocks = self.editor.style_preview(&v.pending, false).ok_or_else(|| Error::invalid("not a style"))?;
        self.picture(blocks, size, false)
    }

    /// Opens the style dialog: Modify Style on a style, or Create New Style from Formatting on the selection's
    /// formatting (in its short form).
    pub fn dialog_open_style(&mut self, open: crate::api::services::StyleOpen) -> Result<DialogModel, Error> {
        Ok(dialogs::style_model(self.editor.style_dialog_open(open)?))
    }

    /// The state a Format menu dialog reads: the selection's, with the formatting of text in the style dialog's style.
    fn style_state(&self, style: &crate::api::services::StyleDialog) -> Result<crate::api::state::State, Error> {
        let (formatting, paragraph, tabs) = self.editor.style_formatting_state(&style.pending).ok_or_else(|| Error::invalid("not a style"))?;
        let mut state = self.state.clone();
        state.formatting = formatting;
        state.paragraph = paragraph;
        state.ruler.horizontal.tabs = tabs;
        Ok(state)
    }

    /// Opens a dialog of the style dialog's Format menu (Font, Paragraph, Tabs) on its style: applying it
    /// (`dialog_apply_to_style`) changes the style dialog, not the document.
    pub fn dialog_open_on_style(&mut self, kind: DialogKind, style: crate::api::services::StyleDialog) -> Result<DialogModel, Error> {
        if !dialogs::formats_a_style(kind) {
            return Err(Error::invalid("the style dialog's Format menu doesn't open that dialog"));
        }
        let table = style.kind == crate::api::services::StyleDialogType::Table;
        let mut model = if table && matches!(kind, DialogKind::BordersShading | DialogKind::TableProperties | DialogKind::Banding) {
            dialogs::model_of(self.table_style_format(kind, &style)?)
        } else if kind == DialogKind::Banding || kind == DialogKind::TableProperties {
            return Err(Error::invalid("that Format menu dialog formats a table style"));
        } else if kind == DialogKind::BordersShading {
            dialogs::borders_shading_model(self.borders_shading(Some(&style.pending))?, false)
        } else if kind == DialogKind::Frame {
            dialogs::frame_model(crate::api::services::FrameDialog::of(self.frame(Some(&style.pending))?))
        } else if kind == DialogKind::BulletsNumbering {
            let v = self.editor.list_dialog_values(kind, Some(&style.pending)).ok_or_else(|| Error::invalid("not a style"))?;
            dialogs::list_model(v)
        } else {
            let state = self.style_state(&style)?;
            let cx = self.dialog_context(kind);
            dialogs::open(kind, &state, &cx)?
        };
        model.on_style = Some(Box::new(style));
        Ok(model)
    }

    /// A table style's Format entry's values: Borders and Shading on its part, Table Properties, Banding.
    fn table_style_format(&self, kind: DialogKind, style: &crate::api::services::StyleDialog) -> Result<crate::api::services::DialogValues, Error> {
        use crate::api::services::DialogValues;
        let not = || Error::invalid("not a table style");
        Ok(match kind {
            DialogKind::BordersShading => {
                DialogValues::BordersShading(Box::new(self.editor.table_part_borders(&style.pending, style.table_part.region()).ok_or_else(not)?))
            }
            DialogKind::TableProperties => DialogValues::TableProperties(self.editor.style_table_properties(&style.pending).ok_or_else(not)?),
            _ => DialogValues::Banding(self.editor.style_banding(&style.pending).ok_or_else(not)?),
        })
    }

    /// Applies a Format menu dialog to its style dialog: the style dialog after it, the document unchanged.
    pub fn dialog_apply_to_style(&mut self, model: DialogModel) -> Result<DialogModel, Error> {
        let style = model.on_style.as_deref().cloned().ok_or_else(|| Error::invalid("the dialog isn't from the style dialog"))?;
        if style.kind == crate::api::services::StyleDialogType::Table
            && matches!(model.kind, DialogKind::BordersShading | DialogKind::TableProperties | DialogKind::Banding)
        {
            let start = self.table_style_format(model.kind, &style)?;
            return Ok(dialogs::style_model(self.editor.style_dialog_table_format(&style, &start, &model.values)?));
        }
        if let crate::api::services::DialogValues::BulletsNumbering(v) = &model.values {
            // The list is made now, as one undo step; the style takes it when the style dialog is applied.
            let num = match &v.entry {
                None => None,
                Some(id) => {
                    let (levels, continues) = self.editor.list_entry_definition(id).ok_or_else(|| Error::invalid("no such list entry"))?;
                    match continues {
                        Some(n) => Some(n),
                        None => {
                            self.refuse_edit(crate::api::command::CommandId::SetListDefinition)?;
                            let mut made = 0;
                            self.apply_with(crate::api::record::Origin::Api, |e, fonts| {
                                let (out, n) = e.add_list(&levels, fonts);
                                made = n;
                                out
                            })?;
                            Some(made)
                        }
                    }
                }
            };
            return Ok(dialogs::style_model(self.editor.style_dialog_numbering(&style, num)?));
        }
        let commands = if let crate::api::services::DialogValues::BordersShading(v) = &model.values {
            dialogs::borders_shading_commands(&self.borders_shading_for(Some(&style.pending), v.apply_to)?, v)
        } else if let crate::api::services::DialogValues::Frame(v) = &model.values {
            dialogs::frame_commands(&self.frame(Some(&style.pending))?, v)
        } else {
            let state = self.style_state(&style)?;
            let cx = self.dialog_context(model.kind);
            dialogs::commands(&model.values, &state, &cx)?
        };
        let fonts = self.fonts.fonts();
        Ok(dialogs::style_model(self.editor.style_dialog_format(&style, commands, &fonts)?))
    }

    /// Changes some of a dialog's values; dependent values and field states are recomputed. Fields the patch leaves
    /// out keep the model's values.
    pub fn dialog_update(&mut self, model: DialogModel, patch: DialogPatch) -> Result<DialogModel, Error> {
        let mut values = model.values.clone();
        if !patch.apply(&mut values) {
            return Err(Error::invalid("the patch is for another dialog"));
        }
        if let crate::api::services::DialogValues::ManageStyles(v) = values {
            return Ok(dialogs::manage_styles_model(self.manage_styles(v)));
        }
        // Borders and Shading on another target: that target's borders and shading.
        if let (crate::api::services::DialogValues::BordersShading(was), crate::api::services::DialogValues::BordersShading(now)) =
            (&model.values, &values)
            && was.apply_to != now.apply_to
        {
            let style = model.on_style.as_ref().map(|s| s.pending.clone());
            let mut v = self.borders_shading_for(style.as_deref(), now.apply_to)?;
            v.tab = now.tab;
            let mut m = dialogs::borders_shading_model(v, style.is_none() && self.editor.in_table());
            m.on_style = model.on_style.clone();
            return Ok(m);
        }
        if let (crate::api::services::DialogValues::Style(was), crate::api::services::DialogValues::Style(now)) = (&model.values, values.clone()) {
            let fonts = self.fonts.fonts();
            return Ok(dialogs::style_model(self.editor.style_dialog_update(was, now, &fonts)?));
        }
        let mut cx = self.dialog_context(model.kind);
        // A style's Borders and Shading isn't on the selection's table.
        cx.in_table &= model.on_style.is_none();
        dialogs::update(&model, values, &cx)
    }

    /// Validates a dialog's values and applies them as one transaction.
    pub fn dialog_apply(&mut self, model: DialogModel) -> Result<Commit, Error> {
        if model.on_style.is_some() {
            return Err(Error::invalid("a Format menu dialog applies to its style dialog (dialog_apply_to_style)"));
        }
        if let crate::api::services::DialogValues::ManageStyles(v) = &model.values {
            self.refuse_read_only()?;
            let now = self.styles(None);
            let mut commands: Vec<crate::api::command::Command> = v
                .styles
                .iter()
                .filter_map(|m| {
                    let s = now.iter().find(|s| s.id == m.id)?;
                    let change = crate::api::command::StyleMetaChange {
                        id: m.id.clone(),
                        visibility: (m.visibility != s.visibility).then_some(m.visibility),
                        priority: (m.priority != s.priority).then_some(m.priority),
                        quick: None,
                        locked: (m.locked != s.locked).then_some(m.locked),
                    };
                    (change.visibility.is_some() || change.priority.is_some() || change.locked.is_some())
                        .then_some(crate::api::command::Command::SetStyleMeta(change))
                })
                .collect();
            let defaults = defaults_changed(&v.defaults, &self.defaults_dialog_values());
            if defaults != crate::api::command::DocumentDefaults::default() {
                commands.push(crate::api::command::Command::SetDocumentDefaults(defaults));
            }
            return match commands.len() {
                0 => Ok(Commit { record: None, value: None }),
                1 => self.run(commands.remove(0)),
                _ => self.batch(commands, Some("Manage Styles".into())),
            };
        }
        let selection_commands = match &model.values {
            crate::api::services::DialogValues::BordersShading(v) => {
                let start = self.borders_shading_for(None, v.apply_to)?;
                Some(dialogs::borders_shading_commands(&start, v))
            }
            crate::api::services::DialogValues::Frame(v) => Some(dialogs::frame_commands(&self.frame(None)?, v)),
            v if model.kind.is_list() => Some(self.editor.list_dialog_command(v).into_iter().collect()),
            _ => None,
        };
        if let Some(mut commands) = selection_commands {
            return match commands.len() {
                0 => Ok(Commit { record: None, value: None }),
                1 => self.run(commands.remove(0)),
                _ => self.batch(commands, Some(dialogs::label(model.kind).to_string())),
            };
        }
        if let crate::api::services::DialogValues::Style(v) = model.values {
            // Style dialog
            self.refuse_edit(crate::api::command::CommandId::UpdateStyleToSelection)?;
            return self.apply_with(crate::api::record::Origin::Api, |e, fonts| e.style_dialog_apply(v, fonts));
        }
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
            (Some("float"), Some(f), None) => {
                let f = crate::layout::FloatId(f);
                let selected = crate::editing::selection::Selection::Object(crate::editing::selection::ObjectRef::Floating(f));
                return match self.apply_with(Origin::Api, |e, _| e.select_float(f)) {
                    Ok(_) if self.editor.selection() == selected => Ok(()),
                    _ => Err(Error::unavailable("the issue is gone")),
                };
            }
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

    /// `query` compiled, over the selection's ranges with `inSelection`: invalid when its text doesn't parse.
    fn search(&mut self, query: &FindQuery) -> Result<find::Search, Error> {
        let mut spec = find::FindSpec::of_query(query);
        if query.in_selection == Some(true) {
            // On from the match a find within the selection selected, in the selection it began with
            let kept = self.services.find_within.as_ref().filter(|w| self.editor.selected_match() == Some(w.hit)).map(|w| w.ranges.clone());
            // With nothing selected, the whole document (as Word searches it).
            let ranges = kept.unwrap_or_else(|| self.editor.selected_ranges());
            spec.within = ranges.iter().any(|r| r.1 < r.2).then_some(ranges);
        }
        find::Search::new(spec).map_err(|why| Error::invalid(why.to_string()))
    }

    /// Finds and selects the next or previous match: invalid when the query doesn't parse.
    pub fn find(&mut self, query: &FindQuery, dir: FindDirection) -> Result<FindResult, Error> {
        let search = self.search(query)?;
        Ok(self.find_next(&search, dir == Order::Previous))
    }

    fn find_next(&mut self, search: &find::Search, back: bool) -> FindResult {
        let Some((hit, wrapped)) = search.next(&self.editor, back) else {
            return FindResult { truncated: search.truncated(), ..FindResult::default() };
        };
        let _ = self.apply_with(Origin::Api, |e, fonts| hit.select(e, fonts));
        self.services.find_within =
            search.spec().within.clone().and_then(|ranges| Some(crate::services::FindWithin { hit: self.editor.selected_match()?, ranges }));
        FindResult { found: true, wrapped, truncated: search.truncated() }
    }

    /// Every match, for a results list: invalid when the query doesn't parse.
    pub fn find_all(&mut self, query: &FindQuery) -> Result<FindMatches, Error> {
        let search = self.search(query)?;
        let hits = search.all(&self.editor);
        self.complete_layout();
        let pages = crate::services::para_pages(&mut self.editor);
        let paras: Vec<crate::layout::ParaId> = hits.iter().map(|h| h.para).collect();
        let headings = crate::services::headings_over(&self.editor, &paras);
        let matches = hits
            .into_iter()
            .zip(headings)
            .map(|(h, heading)| FindMatch { heading, ..find::describe(&self.editor, h, pages.get(&h.para).copied().unwrap_or(1)) })
            .collect();
        Ok(FindMatches { matches, truncated: search.truncated() })
    }

    /// Selects and reveals a match.
    pub fn select_match(&mut self, id: MatchId) -> Result<(), Error> {
        let hit = find::Hit::parse(&id).ok_or_else(|| Error::invalid("not a match id"))?;
        let doc = self.editor.document();
        if [hit.para, hit.last].iter().any(|&p| self.editor.index().para(doc, p).is_none()) {
            return Err(Error::unavailable("the match is gone"));
        }
        // A crafted id across stories or ending before it starts is refused by the engine as invalid.
        self.apply_with(Origin::Api, |e, fonts| hit.select(e, fonts)).map(|_| ())
    }

    /// Replaces the current match (with `^` codes, or a regular expression's groups) and gives it `format`'s
    /// formatting, then finds the next.
    pub fn replace(&mut self, query: &FindQuery, with: &str, format: Option<&FindFormat>) -> Result<FindResult, Error> {
        let search = self.search(query)?;
        if let Some(hit) = search.at_selection(&self.editor) {
            self.refuse_read_only()?;
            let format = format.map(find::FindFormat::of_api);
            self.apply_with(Origin::Api, |e, fonts| {
                find::replace_with(e, &search, &[hit], with, format.as_ref(), false, fonts).map_or_else(Outcome::Refused, |r| r.1)
            })?;
        }
        Ok(self.find_next(&search, false))
    }

    /// Replaces every match as one transaction (with `^` codes, or a regular expression's groups), giving each
    /// `format`'s formatting; how many. A match that document protection, a locked control or a sub-template's body
    /// keeps isn't replaced; when protection keeps every one, the replace is refused.
    pub fn replace_all(&mut self, query: &FindQuery, with: &str, format: Option<&FindFormat>) -> Result<FindCount, Error> {
        let search = self.search(query)?;
        let hits = search.all(&self.editor);
        let truncated = search.truncated();
        if hits.is_empty() {
            return Ok(FindCount { count: 0, truncated });
        }
        self.refuse_read_only()?;
        let format = format.map(find::FindFormat::of_api);
        let mut count = 0;
        self.apply_with(Origin::Api, |e, fonts| match find::replace_with(e, &search, &hits, with, format.as_ref(), true, fonts) {
            Ok((n, out)) => {
                count = n;
                out
            }
            Err(why) => Outcome::Refused(why),
        })?;
        Ok(FindCount { count: u32::try_from(count).unwrap_or(u32::MAX), truncated: truncated || search.truncated() })
    }

    /// Highlights every match on the pages (Highlight All) until the document changes or
    /// [`Session::clear_find_highlights`]: how many; none clears them. `state.view.findHighlights` says whether they
    /// show.
    pub fn highlight_all(&mut self, query: &FindQuery) -> Result<FindCount, Error> {
        let search = self.search(query)?;
        let mut count = 0;
        self.apply_with(Origin::Api, |e, _| {
            count = find::highlight_all(e, &search);
            Outcome::Moved
        })?;
        Ok(FindCount { count: u32::try_from(count).unwrap_or(u32::MAX), truncated: search.truncated() })
    }

    /// Takes Highlight All's highlights away.
    pub fn clear_find_highlights(&mut self) {
        if self.editor.find_highlights().is_some() {
            let _ = self.apply_with(Origin::Api, |e, _| {
                e.clear_find_highlights();
                Outcome::Moved
            });
        }
    }

    /// The items a cross-reference of `kind` can point to, as Word's Cross-reference dialog lists them (bookmarks by
    /// name, the rest in document order): `insertCrossReference` names one by its place here.
    pub fn cross_reference_targets(&self, kind: &crate::api::command::CrossReferenceKind) -> Vec<crate::api::services::CrossReferenceTarget> {
        use crate::editing::commands::references::{ref_kind, ref_target};
        self.editor.cross_reference_targets(&ref_kind(kind)).iter().map(ref_target).collect()
    }

    /// The labels a caption can take: Equation, Figure and Table, then the host's `custom` labels, then those the
    /// document's captions use, each once.
    pub fn caption_labels(&self, custom: &[String]) -> Vec<String> {
        self.editor.caption_labels(custom).into_iter().map(|l| l.replace('_', " ")).collect()
    }

    /// The document's tables of contents and of figures, and the one the selection is in.
    pub fn tables_of_contents(&self) -> crate::api::services::TablesOfContents {
        let e = &self.editor;
        let sep = e.list_separator();
        let names = std::cell::OnceCell::new();
        let para = |t: &crate::editing::toc::TocField, k: usize| {
            e.index().blocks(e.document(), t.container).and_then(|b| match b.get(k) {
                Some(crate::docx::Block::Para(p)) => Some(crate::api::value::ParagraphId(p.id.to_string())),
                _ => None,
            })
        };
        let tables = e
            .tables_of_contents()
            .iter()
            .enumerate()
            .map(|(k, t)| {
                let options = t.options(sep);
                crate::api::services::TableOfContentsInfo {
                    index: u32::try_from(k).unwrap_or(u32::MAX),
                    figures: options.captions.is_some(),
                    options: e.toc_options_api(&options, &names),
                    locked: e.toc_locked(t),
                    first_paragraph: para(t, t.first).unwrap_or_default(),
                    last_paragraph: para(t, t.last).unwrap_or_default(),
                }
            })
            .collect();
        let here = e.toc_at_selection().map(|(k, _)| u32::try_from(k).unwrap_or(u32::MAX));
        crate::api::services::TablesOfContents { tables, here }
    }

    /// The entries a table of contents with `options` would collect now (`{}`: Word's default table), for a
    /// preview: invalid levels or styles refused.
    pub fn toc_entries(&self, options: &crate::api::command::TableOfContentsOptions) -> Result<Vec<crate::api::services::TocEntryPreview>, Error> {
        let o = if *options == crate::api::command::TableOfContentsOptions::default() {
            crate::editing::toc::TocOptions::word_default()
        } else {
            self.editor.toc_options(options).map_err(|out| match out {
                Outcome::Refused(e) => e,
                _ => Error::invalid("not a table of contents"),
            })?
        };
        Ok(self
            .editor
            .toc_entries(&o)
            .into_iter()
            .map(|x| crate::api::services::TocEntryPreview { level: x.level, text: x.text, page_number: x.page })
            .collect())
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

    /// The heading the selection's focus is under, as a navigation pane highlights it (the nearest listed heading at
    /// or before it); `None` before the first heading or outside the body. Hosts ask on `selectionChanged`.
    pub fn current_heading(&self) -> Option<HeadingId> {
        crate::services::current_heading(&self.editor)
    }

    /// The document's properties (core, extended and custom) and variables.
    pub fn document_properties(&self) -> crate::api::services::DocumentProperties {
        self.editor.document_properties()
    }

    /// The document's bookmarks, hidden ones too, in document order.
    pub fn bookmarks(&self) -> Vec<crate::api::services::BookmarkInfo> {
        self.editor
            .bookmarks()
            .into_iter()
            .map(|b| crate::api::services::BookmarkInfo { hidden: b.name.starts_with('_'), empty: b.end.is_none_or(|e| e == b.start), name: b.name })
            .collect()
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
        let mut out = crate::services::styles::list(self.editor.document(), kind, self.editor.editing_sub_template().is_some());
        let uses = self.editor.style_use_counts();
        for s in &mut out {
            if matches!(s.kind, StyleType::Paragraph | StyleType::Character) {
                s.uses = uses.get(&s.id.0).map_or(0, |&n| u32::try_from(n).unwrap_or(u32::MAX));
            }
        }
        out
    }

    /// The document's accessibility tree.
    pub fn accessible_tree(&mut self) -> AccessibleNode {
        crate::services::a11y::tree_with(&self.editor, &self.services.spelling, &self.language, &mut self.services.tree_cache)
    }

    /// The kinds of announcements the editor makes; the others are neither said nor emitted. All by default.
    pub fn set_announcements(&mut self, kinds: &[crate::api::services::AnnouncementKind]) {
        self.services.silenced = crate::api::services::AnnouncementKind::ALL.into_iter().filter(|k| !kinds.contains(k)).collect();
    }

    /// Puts the caret at the start of a tree node's content and reveals it: a note reference's note, a comment's
    /// commented text. `invalid-argument` when no node has the id, `unavailable` when it holds no caret position (a
    /// region).
    pub fn accessible_go(&mut self, id: crate::api::services::AccessibleId) -> Result<(), Error> {
        let tree = self.accessible_tree();
        let para = crate::services::a11y::caret_para(&tree, id)
            .ok_or_else(|| Error::invalid("no accessibility node has this id"))?
            .ok_or_else(|| Error::unavailable("the node holds no caret position"))?;
        self.apply_with(Origin::Api, |e, fonts| e.select_range(para, Unit(0), Unit(0), fonts)).map(|_| ())
    }

    /// The document's counts, or the selection's, computed now: a count walks the whole document, so it is a
    /// query and never part of the state.
    pub fn word_count(&mut self, options: crate::api::services::WordCountOptions) -> WordCount {
        // Lines and pages count the whole layout.
        let fonts = self.fonts.fonts();
        self.editor.complete(&fonts);
        let scope = crate::services::WordCountScope { selection: options.selection, boxes_and_notes: options.include_textboxes_and_notes };
        crate::services::word_count_in(&mut self.editor, scope)
    }

    /// The undo and redo lists, at most `limit` labels each (default and most: 100, Word's list length).
    pub fn history_labels(&self, limit: Option<u32>) -> crate::api::services::HistoryLabels {
        let n = limit.unwrap_or(HISTORY_LIST).min(HISTORY_LIST) as usize;
        let h = self.editor.history();
        let list = |mut labels: Box<dyn Iterator<Item = &str> + '_>| {
            let shown: Vec<String> = labels.by_ref().take(n).map(str::to_string).collect();
            let more = labels.next().is_some();
            (shown, more)
        };
        let (undo, more_undo) = list(Box::new(h.undo_labels()));
        let (redo, more_redo) = list(Box::new(h.redo_labels()));
        crate::api::services::HistoryLabels { undo, redo, more_undo, more_redo }
    }

    /// A style as it is defined: the document's, or a built-in one it doesn't define (`defined: false`); none for
    /// an id that is neither.
    pub fn style_definition(&self, id: &crate::api::value::StyleId) -> Option<crate::api::styles::StyleDefinition> {
        self.editor.style_definition(id)
    }

    /// Every built-in style, defined by the document or not, with its list flags.
    pub fn built_in_styles(&self) -> Vec<crate::api::styles::BuiltInStyle> {
        self.editor.built_in_styles()
    }

    /// The document's default formatting; none without a styles part.
    pub fn document_defaults(&self) -> Option<crate::api::styles::DefaultsDefinition> {
        self.editor.document_defaults()
    }

    /// The theme colour grid Word's colour pickers show: a column for each of Background 1, Text 1, Background 2,
    /// Text 2 and Accent 1 to 6, each the colour then its five lighter and darker variants, named. A query, not state:
    /// the state's `themeColours` changes when the theme does.
    pub fn theme_colour_grid(&self) -> Vec<crate::api::state::ThemeColumn> {
        self.editor.theme_colour_grid()
    }

    /// The list of the paragraph the selection's focus is in, in full; none outside a list.
    pub fn list_definition(&self) -> Option<crate::api::styles::ListDefinition> {
        self.editor.list_definition()
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
        s.services.add_dictionary(&dictionary).map_err(Error::invalid)?;
        s.editor.set_hyphenation(s.services.hyphenation.clone());
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

/// The API's category for the catalogue's.
fn field_category(c: crate::services::fields::FieldCategory) -> crate::api::services::FieldCategory {
    use crate::api::services::FieldCategory as A;
    use crate::services::fields::FieldCategory as F;
    match c {
        F::DateAndTime => A::DateAndTime,
        F::DocumentAutomation => A::DocumentAutomation,
        F::DocumentInformation => A::DocumentInformation,
        F::EquationsAndFormulas => A::EquationsAndFormulas,
        F::IndexAndTables => A::IndexAndTables,
        F::LinksAndReferences => A::LinksAndReferences,
        F::MailMerge => A::MailMerge,
        F::Numbering => A::Numbering,
        F::UserInformation => A::UserInformation,
    }
}

/// A field from the catalogue as the API describes it.
fn field_info(f: &crate::services::fields::FieldSpec) -> crate::api::services::FieldDescription {
    use crate::api::services::{FieldArgInfo, FieldArgKind as K, FieldGeneralSwitches, FieldSwitchInfo, SwitchArgKind as S};
    use crate::services::fields::{ArgKind, SwitchArg};
    let arg = |k: ArgKind| match k {
        ArgKind::Text => K::Text,
        ArgKind::Bookmark => K::Bookmark,
        ArgKind::Style => K::Style,
        ArgKind::Property => K::Property,
        ArgKind::Variable => K::Variable,
        ArgKind::File => K::File,
        ArgKind::Name => K::Name,
        ArgKind::Expression => K::Expression,
        ArgKind::Number => K::Number,
    };
    let switch = |a: SwitchArg| match a {
        SwitchArg::None => S::None,
        SwitchArg::Text => S::Text,
        SwitchArg::Number => S::Number,
        SwitchArg::Bookmark => S::Bookmark,
        SwitchArg::Style => S::Style,
        SwitchArg::Name => S::Name,
        SwitchArg::Levels => S::Levels,
    };
    crate::api::services::FieldDescription {
        name: f.name.into(),
        categories: f.categories.iter().map(|&c| field_category(c)).collect(),
        description: f.description.into(),
        syntax: f.syntax.into(),
        args: f.args.iter().map(|a| FieldArgInfo { name: a.name.into(), kind: arg(a.kind), required: a.required }).collect(),
        switches: f.switches.iter().map(|s| FieldSwitchInfo { flag: s.flag.into(), arg: switch(s.arg), description: s.description.into() }).collect(),
        formats: FieldGeneralSwitches { text: f.formats.text, numeric: f.formats.numeric, date: f.formats.date },
        evaluated: f.evaluated(),
    }
}

/// The engine's choices for the API's.
fn engine_choice(c: &crate::api::services::FieldChoice) -> crate::services::fields::FieldChoice {
    crate::services::fields::FieldChoice {
        name: c.name.clone(),
        args: c.args.clone(),
        switches: c.switches.iter().map(|s| (s.flag.clone(), s.value.clone())).collect(),
        formats: c.formats.clone(),
        numeric: c.numeric_picture.clone(),
        date: c.date_picture.clone(),
        preserve_formatting: c.preserve_formatting,
    }
}

/// The API's choices for the engine's.
fn api_choice(c: crate::services::fields::FieldChoice) -> crate::api::services::FieldChoice {
    crate::api::services::FieldChoice {
        name: c.name,
        args: c.args,
        switches: c.switches.into_iter().map(|(flag, value)| crate::api::services::FieldSwitchChoice { flag, value }).collect(),
        formats: c.formats,
        numeric_picture: c.numeric,
        date_picture: c.date,
        preserve_formatting: c.preserve_formatting,
    }
}

/// Insert Field's problems as the API names them.
fn field_problem(p: crate::services::fields::FieldProblem) -> crate::api::services::FieldProblem {
    use crate::api::services::FieldProblem as A;
    use crate::services::fields::FieldProblem as F;
    match p {
        F::Empty => A::Empty,
        F::UnknownField(name) => A::UnknownField { name },
        F::MissingArgument(name) => A::MissingArgument { name: name.into() },
        F::UnknownSwitch(flag) => A::UnknownSwitch { flag },
        F::MissingSwitch(flag) => A::MissingSwitch { flag: flag.into() },
        F::MissingSwitchValue(flag) => A::MissingSwitchValue { flag },
        F::NotANumber(flag) => A::NotANumber { flag },
        F::FormatNotTaken(flag) => A::FormatNotTaken { flag: flag.to_string() },
    }
}

impl Session {
    /// The footnotes, then the endnotes, each in their references' order (those nothing refers to after, by id), for a
    /// notes list. Separator notes are left out.
    pub fn notes(&mut self) -> Vec<crate::api::services::NoteInfo> {
        use crate::api::services::NOTE_TEXT_LIMIT;
        self.editor
            .notes()
            .into_iter()
            .map(|n| {
                let mut text = n.text;
                if let Some((cut, _)) = text.char_indices().nth(NOTE_TEXT_LIMIT) {
                    text.truncate(cut);
                    text.push('…');
                }
                crate::api::services::NoteInfo {
                    kind: if n.kind == crate::docx::NoteKind::Foot {
                        crate::api::command::NoteType::Footnote
                    } else {
                        crate::api::command::NoteType::Endnote
                    },
                    id: n.id,
                    mark: n.mark,
                    custom_mark: n.custom,
                    text,
                    page: n.page.map(|p| p as u32 + 1),
                }
            })
            .collect()
    }

    /// The separators of footnotes or endnotes: each one's text, and whether it is the built-in line.
    pub fn note_separators(&self, kind: crate::api::command::NoteType) -> crate::api::services::NoteSeparators {
        use crate::api::services::NoteSeparatorState;
        use crate::docx::NoteSeparator as S;
        let kind = if kind == crate::api::command::NoteType::Footnote { crate::docx::NoteKind::Foot } else { crate::docx::NoteKind::End };
        let state = |i: crate::editing::NoteSeparatorInfo| NoteSeparatorState { text: i.text, builtin: i.builtin };
        // A separator the document doesn't define is Word's built-in line.
        let line = || NoteSeparatorState { text: String::new(), builtin: true };
        crate::api::services::NoteSeparators {
            separator: self.editor.note_separator(kind, S::Separator).map_or_else(line, state),
            continuation_separator: self.editor.note_separator(kind, S::Continuation).map_or_else(line, state),
            continuation_notice: self.editor.note_separator(kind, S::Notice).map(state),
        }
    }

    /// The columns one of Word's column presets gives the selection's section, for `setColumns`.
    pub fn column_preset(&self, preset: crate::api::value::ColumnPreset) -> crate::api::command::Columns {
        use crate::api::value::ColumnPreset as A;
        use crate::editing::section::ColumnPreset as P;
        self.editor.column_preset(match preset {
            A::One => P::One,
            A::Two => P::Two,
            A::Three => P::Three,
            A::Left => P::Left,
            A::Right => P::Right,
        })
    }

    /// The example Format Page Numbers shows for `format`, with chapter numbers when `chapter` gives their separator.
    pub fn page_number_sample(&self, format: crate::api::command::NumberFormat, chapter: Option<crate::api::command::ChapterSeparator>) -> String {
        crate::editing::commands::page_number_sample(format, chapter)
    }

    /// The styles Formatting Restrictions' Recommended Minimum leaves allowed.
    pub fn recommended_minimum_styles(&self) -> Vec<crate::api::value::StyleId> {
        self.editor.recommended_minimum_styles().into_iter().map(crate::api::value::StyleId).collect()
    }

    /// The editable ranges, in document order: each stretch's permissions, editors and page.
    pub fn editable_ranges(&mut self) -> Vec<crate::api::services::EditableRangeInfo> {
        let list = self.editor.editable_range_list();
        list.into_iter()
            .map(|r| crate::api::services::EditableRangeInfo {
                ids: r.ids,
                editors: r.editors.into_iter().map(crate::editing::protection::RangeEditor::to_api).collect(),
                page: self.editor.page_showing(r.start).map(|p| p as u32 + 1),
            })
            .collect()
    }

    /// Insert Field's fields, of one category or all, in its order.
    pub fn field_catalogue(&self, category: Option<crate::api::services::FieldCategory>) -> Vec<crate::api::services::FieldDescription> {
        let category = category.and_then(|c| crate::services::fields::FieldCategory::ALL.into_iter().find(|&f| field_category(f) == c));
        crate::services::fields::fields_in(category).map(field_info).collect()
    }

    /// The formats and pictures Insert Field offers.
    pub fn field_formats(&self) -> crate::api::services::FieldFormats {
        use crate::api::services::FieldFormatOption;
        use crate::services::fields::{DATE_PICTURES, NUMBER_FORMATS, NUMERIC_PICTURES, TEXT_FORMATS};
        let options = |list: &[(&str, &str)]| list.iter().map(|&(v, l)| FieldFormatOption { value: v.into(), label: l.into() }).collect();
        crate::api::services::FieldFormats {
            text_formats: options(TEXT_FORMATS),
            number_formats: options(NUMBER_FORMATS),
            numeric_pictures: NUMERIC_PICTURES.iter().map(|p| p.to_string()).collect(),
            date_pictures: DATE_PICTURES.iter().map(|p| p.to_string()).collect(),
        }
    }

    /// The field code Insert Field's choices make, and its problems. Pure: no document change.
    pub fn compose_field(&self, choice: &crate::api::services::FieldChoice) -> crate::api::services::ComposedField {
        let code = engine_choice(choice).code();
        let problems = crate::services::fields::check(&code).into_iter().map(field_problem).collect();
        crate::api::services::ComposedField { code, problems }
    }

    /// The choices a field code is made of, as Insert Field opens on it.
    pub fn parse_field(&self, code: &str) -> crate::api::services::FieldChoice {
        api_choice(crate::services::fields::FieldChoice::of_code(code))
    }

    /// The field the selection is in, with its code's choices.
    pub fn field_here(&self) -> Option<crate::api::services::FieldHere> {
        let (id, choice) = crate::services::fields::choice_here(&self.editor)?;
        Some(crate::api::services::FieldHere { id: crate::api::value::ElementId(crate::editing::elements::id_str(id)), choice: api_choice(choice) })
    }

    /// The display formats a date control offers for `locale` (BCP 47).
    pub fn date_formats(&self, locale: &str) -> Vec<String> {
        crate::editing::date_formats(locale).iter().map(|f| f.to_string()).collect()
    }

    /// One version per laid-out page, from page 0: a page keeps its version while layout keeps it, and takes a new
    /// one (never 0, never reused) when laid out again. Hosts key cached thumbnails by it.
    pub fn page_versions(&mut self) -> Vec<u64> {
        let snapshot = self.editor.snapshot();
        self.services.page_versions.update(snapshot.pages()).to_vec()
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
        let mut engine = Engine::new(FontLibrary::system_recommended());
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
            assert!(s.find(&query("apple"), Order::Next).expect("finds").found);
        }
        let again = s.find(&query("apple"), Order::Next).expect("finds");
        assert!(again.found && again.wrapped, "past the last, it wraps");
        let case = FindQuery { match_case: Some(true), ..query("aPPLE") };
        assert!(!s.find(&case, Order::Next).expect("finds").found);
        let all = s.find_all(&query("apple")).expect("finds").matches;
        assert_eq!(all.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(), ["Apple", "apple", "APPLE"]);
        assert_eq!((all[1].before.as_str(), all[1].page), ("Apple, ", 1));
        s.select_match(all[2].id.clone()).expect("selects");
        let revision = s.state().revision;
        s.replace(&query("apple"), "pear", None).expect("replaces");
        assert_eq!(s.state().revision, revision + 1);
        assert_eq!(s.replace_all(&query("apple"), "plum", None).map(|c| c.count).expect("replaces"), 2);
        assert_eq!(s.state().revision, revision + 2, "one transaction");
        assert!(s.find_all(&query("apple")).expect("finds").matches.is_empty());
        let whole = FindQuery { whole_words: Some(true), ..query("plum") };
        assert_eq!(s.find_all(&whole).expect("finds").matches.len(), 2);
    }

    #[test]
    fn a_dictionarys_hyphenation_patterns_reach_every_document() {
        let (mut engine, id, _) = open("editor/FX-services.docx");
        let s = engine.session(id).expect("a session");
        let patterns = Bytes(b"UTF-8\nhy3ph\n".to_vec());
        s.spelling()
            .add_dictionary(Dictionary { language: "en-US".into(), aff: Bytes(Vec::new()), dic: Bytes(Vec::new()), hyphenation: Some(patterns) })
            .expect("loads");
        let has = |s: &mut crate::api::session::Session| s.with_editor(|e| e.hyphenation().is_some_and(|h| h.for_language("en-US").is_some()));
        assert!(has(s));
        s.new_document();
        assert!(has(s), "a new document keeps them");
    }

    #[test]
    fn spelling_checks_ignores_adds_and_menus() {
        let (mut engine, id, events) = open("editor/FX-services.docx");
        let s = engine.session(id).expect("a session");
        let (aff, dic) = (fixture("dictionaries/en_US.aff"), fixture("dictionaries/en_US.dic"));
        s.spelling().load_user_words("en-US", &["brwon".to_string()]);
        s.spelling().add_dictionary(Dictionary { language: "en-US".into(), aff: Bytes(aff), dic: Bytes(dic), hyphenation: None }).expect("loads");
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
    fn the_misspelling_count_follows_edits_as_a_recount_would() {
        use crate::api::command::Command;
        let (mut engine, id, _) = open("editor/FX-services.docx");
        let s = engine.session(id).expect("a session");
        let (aff, dic) = (fixture("dictionaries/en_US.aff"), fixture("dictionaries/en_US.dic"));
        s.spelling().add_dictionary(Dictionary { language: "en-US".into(), aff: Bytes(aff), dic: Bytes(dic), hyphenation: None }).expect("loads");
        let recount = |s: &mut crate::api::session::Session| {
            s.with_editor(|_| ());
            s.state().spelling.misspellings
        };
        let start = s.state().spelling.misspellings;
        let steps = [
            Command::InsertText("zorblyx ".into()),
            Command::InsertParagraph,
            Command::InsertText("quuxle the the".into()),
            Command::Undo(Default::default()),
            Command::SelectAll,
            Command::InsertText("fine words".into()),
            Command::Undo(Default::default()),
        ];
        for c in steps {
            s.run(c.clone()).expect("runs");
            let counted = s.state().spelling.misspellings;
            assert_eq!(counted, recount(s), "after {c:?}");
        }
        assert_eq!(s.state().spelling.misspellings, start + 1, "the undo brought the first text back");
    }

    #[test]
    fn going_to_an_accessibility_node_moves_the_caret_there() {
        use crate::api::services::{AccessibleId, AccessibleNode, AccessibleRole};
        fn find(n: &AccessibleNode, role: AccessibleRole) -> Option<&AccessibleNode> {
            if n.role == role { Some(n) } else { n.children.iter().find_map(|c| find(c, role)) }
        }
        let (mut engine, id, _) = open("editor/FX-A11Y-STRUCTURE.docx");
        let s = engine.session(id).expect("a session");
        let tree = s.accessible_tree();
        let table = find(&tree, AccessibleRole::Table).expect("a table");
        s.accessible_go(table.id).expect("goes to the table");
        assert!(s.state().selection.table.is_some(), "the caret is in the table: {:?}", s.state().selection);
        let note_ref = find(&tree, AccessibleRole::NoteRef).expect("a note reference");
        s.accessible_go(note_ref.id).expect("goes to the note");
        assert_eq!(s.state().selection.story, crate::api::value::Story::Footnote, "{:?}", s.state().selection);
        let region = find(&tree, AccessibleRole::Region).expect("a region");
        assert_eq!(s.accessible_go(region.id).map_err(|e| e.code), Err(crate::api::error::ErrorCode::Unavailable));
        assert_eq!(s.accessible_go(AccessibleId((1 << 53) - 1)).map_err(|e| e.code), Err(crate::api::error::ErrorCode::InvalidArgument));
    }

    #[test]
    fn the_menu_key_emits_a_menu_and_the_tree_reads_the_document() {
        let (mut engine, id, events) = open("editor/FX-services.docx");
        let s = engine.session(id).expect("a session");
        assert_eq!(s.key(Key::ContextMenu, Modifiers::default()), KeyOutcome::Handled);
        assert_eq!(s.key(Key::F10, Modifiers { shift: true, ..Modifiers::default() }), KeyOutcome::Handled);
        assert_eq!(events.borrow().iter().filter(|e| matches!(e, Event::ContextMenu(_))).count(), 2);
        let tree = s.accessible_tree();
        // A paragraph's text is in its text nodes.
        let text: Vec<String> = tree.children.iter().map(|n| n.children.iter().filter_map(|t| t.text.as_deref()).collect()).collect();
        assert!(text.iter().any(|t| t.starts_with("Teh quick brown fox")), "{text:?}");
        let count = s.word_count(Default::default());
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
    fn a_gone_floats_issue_is_unavailable() {
        let (mut engine, id, _) = open("editor/FX-OBJ.docx");
        let s = engine.session(id).expect("a session");
        let issues = s.with_editor(|e| crate::services::dialogs::issues(e.document()));
        let float = issues.iter().find(|i| i.id.starts_with("float:")).expect("a floating picture without alt text");
        s.select_accessibility_issue(&float.id).expect("selects it");
        assert!(s.select_accessibility_issue(&format!("float:{}", u64::MAX)).is_err(), "another float stays selected");
    }

    #[test]
    fn dialogs_open_on_the_selection() {
        let (mut engine, id, _) = open("editor/FX-services.docx");
        let s = engine.session(id).expect("a session");
        let check = s.dialog_open(DialogKind::Accessibility).expect("opens");
        let DialogValues::Accessibility(a) = &check.values else { panic!("{check:?}") };
        assert!(!a.issues.is_empty(), "the picture has no alt text");
        s.select_accessibility_issue(&a.issues[0].id).expect("selects it");
        assert!(s.find(&query("the example site"), Order::Next).expect("finds").found);
        let link = s.dialog_open(DialogKind::Hyperlink).expect("opens");
        let DialogValues::Hyperlink(h) = &link.values else { panic!() };
        assert_eq!(h.text, "the example site");
        assert!(s.dialog_open(DialogKind::TableProperties).is_err(), "not in a table");
        let patch: crate::api::services::DialogPatch =
            serde_json::from_str(r#"{"kind":"hyperlink","address":"https://example.org/"}"#).expect("a patch");
        let link = s.dialog_update(link, patch).expect("updates");
        let DialogValues::Hyperlink(h) = &link.values else { panic!() };
        assert_eq!((h.text.as_str(), h.address.as_str()), ("the example site", "https://example.org/"), "fields left out keep their values");
        // A link to a bookmark: the picker lists the shown ones.
        s.run(crate::api::command::Command::InsertBookmark("Terms".into())).expect("bookmarks");
        s.run(crate::api::command::Command::InsertBookmark("_Ref1".into())).expect("bookmarks");
        let link = s.dialog_open(DialogKind::Hyperlink).expect("opens");
        let options: Vec<&str> = link.fields["bookmark"].options.iter().flatten().map(|o| o.value.as_str()).collect();
        assert_eq!(options, ["_top", "Terms"], "hidden bookmarks aren't offered");
        let patch = serde_json::from_str(r#"{"kind":"hyperlink","address":"","bookmark":"Terms"}"#).expect("a patch");
        let link = s.dialog_update(link, patch).expect("updates");
        s.dialog_apply(link).expect("applies");
        assert_eq!(s.state().selection.link.as_ref().map(|l| l.address.as_str()), Some("#Terms"));
        let link = s.dialog_open(DialogKind::Hyperlink).expect("opens");
        let DialogValues::Hyperlink(h) = &link.values else { panic!() };
        assert_eq!((h.address.as_str(), h.bookmark.as_str()), ("", "Terms"));
    }
}
