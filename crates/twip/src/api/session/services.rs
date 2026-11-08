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
            MenuAction::Command { command } => self.run(*command),
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
            (Some(r), Some(id)) => r.styles.get(&id.0).map(|s| crate::services::styles::describe::describe(s, &r.styles)).unwrap_or_default(),
            _ => String::new(),
        };
        v
    }

    /// The document defaults as Set Defaults shows them.
    fn document_defaults(&self) -> crate::api::command::DocumentDefaults {
        use crate::api::command::{DocumentDefaults, Indents, ParagraphSpacing};
        use crate::api::value::{AutoOr, Color, LineSpacing, Points};
        let Some(r) = self.editor.document().resolver.as_ref() else { return DocumentDefaults::default() };
        let (rpr, ppr) = (&r.styles.doc_rpr, &r.styles.doc_ppr);
        let font = rpr.ascii.as_ref().map(|f| match f {
            crate::docx::props::FontRef::Name(n) => n.clone(),
            crate::docx::props::FontRef::Theme(t) => r.theme.resolve(t),
        });
        DocumentDefaults {
            font,
            size: rpr.size.map(Points),
            color: Some(rpr.color.flatten().map_or(AutoOr::Auto, |c| AutoOr::Value(Color(c)))),
            spacing: Some(ParagraphSpacing {
                before: Some(AutoOr::Value(Points(ppr.space_before.unwrap_or(0.0)))),
                after: Some(AutoOr::Value(Points(ppr.space_after.unwrap_or(0.0)))),
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
        self.borders_shading_for(style, text)
    }

    /// Borders and Shading's values for the paragraphs, or the text.
    fn borders_shading_for(&self, style: Option<&str>, text: bool) -> Result<crate::api::services::BordersShadingDialog, Error> {
        self.editor.borders_shading_values(style, text).ok_or_else(|| Error::unavailable("the selection has no paragraph"))
    }

    /// The Frame dialog's properties: the selection's frame's, or a style's.
    fn frame(&self, style: Option<&str>) -> Result<crate::api::command::FrameProperties, Error> {
        self.editor.frame_properties(style).ok_or_else(|| Error::unavailable("the selection isn't in a frame"))
    }

    /// Opens a dialog on the selection's values.
    pub fn dialog_open(&mut self, kind: DialogKind) -> Result<DialogModel, Error> {
        if kind == DialogKind::BordersShading {
            return Ok(dialogs::borders_shading_model(self.borders_shading(None)?));
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
            let v = crate::api::services::ManageStylesDialog { selected: first, defaults: self.document_defaults(), ..Default::default() };
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
        let (height, children) =
            crate::layout::expansion::lay(self.editor.document(), &blocks, if fit { f64::MAX / 4.0 } else { width }, &fonts, None);
        let items = crate::layout::traverse::items_of(&children);
        let appearance = self.presentation.appearance();
        let (png, w, h) =
            crate::render::preview::png(&items, height, &self.editor.document().images, (size.width, size.height, size.scale), fit, appearance)
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
            .and_then(|r| r.styles.get(&id.0))
            .map(|s| s.raw.clone())
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
            dialogs::borders_shading_model(self.borders_shading(Some(&style.pending))?)
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
                DialogValues::BordersShading(self.editor.table_part_borders(&style.pending, style.table_part.region()).ok_or_else(not)?)
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
                            self.refuse_read_only()?;
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
            dialogs::borders_shading_commands(
                &self.borders_shading_for(Some(&style.pending), v.apply_to == crate::api::command::ShadingTarget::Text)?,
                v,
            )
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
            let mut v = self.borders_shading_for(style.as_deref(), now.apply_to == crate::api::command::ShadingTarget::Text)?;
            v.tab = now.tab;
            let mut m = dialogs::borders_shading_model(v);
            m.on_style = model.on_style.clone();
            return Ok(m);
        }
        if let (crate::api::services::DialogValues::Style(was), crate::api::services::DialogValues::Style(now)) = (&model.values, values.clone()) {
            let fonts = self.fonts.fonts();
            return Ok(dialogs::style_model(self.editor.style_dialog_update(was, now, &fonts)?));
        }
        let cx = self.dialog_context(model.kind);
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
            if v.defaults != self.document_defaults() {
                commands.push(crate::api::command::Command::SetDocumentDefaults(v.defaults.clone()));
            }
            return match commands.len() {
                0 => Ok(Commit { record: None, value: None }),
                1 => self.run(commands.remove(0)),
                _ => self.batch(commands, Some("Manage Styles".into())),
            };
        }
        let selection_commands = match &model.values {
            crate::api::services::DialogValues::BordersShading(v) => {
                let start = self.borders_shading_for(None, v.apply_to == crate::api::command::ShadingTarget::Text)?;
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
            self.refuse_read_only()?;
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
        crate::services::styles::list(self.editor.document(), kind, self.editor.editing_sub_template().is_some())
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
