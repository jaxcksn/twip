//! Commands: the catalog, each command's availability in the current state, the checks a command passes before any
//! step runs (read only, then availability, then its arguments), and the view commands the session keeps.

use super::{Commit, Session};
use crate::api::catalog::{CommandInfo, info};
use crate::api::command::{Command, CommandId, Zoom, ZoomFit};
use crate::api::error::{Error, ErrorCode};
use crate::api::record::Origin;
use crate::api::services::Popover;
use crate::api::state::CommandState;
use crate::api::value::{CommentId, ElementId, Points, TableWidth};
use crate::editing::commands::Outcome;

use crate::editing::commands::bounds::{MAX, length, within};

/// An object's or a table's place on each axis (aligned, and its offset): an offset within ±`MAX`, and not with an
/// alignment.
fn placed(axes: [Option<(bool, Option<Points>)>; 2]) -> Result<(), Error> {
    for (aligned, offset) in axes.into_iter().flatten() {
        if aligned && offset.is_some() {
            return Err(Error::invalid("align and offset exclude each other"));
        }
        length("the offset", offset, -MAX, MAX)?;
    }
    Ok(())
}

/// A separator's character: one, not a paragraph or line break, given with `other` only.
fn separator(kind: crate::api::command::TextSeparator, character: Option<&str>) -> Result<(), Error> {
    use crate::api::command::TextSeparator;
    let mut chars = character.map(str::chars);
    let one = chars.as_mut().and_then(|c| c.next().filter(|_| c.next().is_none()));
    match (kind, character, one) {
        (TextSeparator::Other, _, Some(c)) if !c.is_control() && !matches!(c, '\u{2028}' | '\u{2029}') => Ok(()),
        (TextSeparator::Other, ..) => Err(Error::invalid("the separator must be one character")),
        (_, Some(_), _) => Err(Error::invalid("a character goes with the other separator only")),
        _ => Ok(()),
    }
}

fn count(what: &str, v: Option<u32>, hi: u32) -> Result<(), Error> {
    v.map_or(Ok(()), |v| within(what, f64::from(v), 1.0, f64::from(hi)))
}

fn text(what: &str, v: &str) -> Result<(), Error> {
    if v.is_empty() { Err(Error::invalid(format!("{what} must not be empty"))) } else { Ok(()) }
}

fn picture(data: &[u8]) -> Result<(), Error> {
    match crate::docx::image::image_type(data) {
        Some(_) => Ok(()),
        None => Err(Error::invalid("the data isn't a PNG, JPEG, GIF, BMP, TIFF, JPEG XR, SVG, EMF or WMF picture")),
    }
}

/// A command id's wire name (`setBold`).
pub(super) fn id_name(id: CommandId) -> String {
    serde_json::to_value(id).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

impl Session {
    /// Every command the engine runs.
    pub fn catalog() -> &'static [CommandInfo] {
        crate::api::catalog::catalog()
    }

    /// The shape gallery: Word's Insert > Shapes presets in its groups.
    pub fn shape_gallery() -> Vec<crate::api::drawing::ShapeGalleryGroup> {
        crate::api::drawing::shape_gallery()
    }

    /// Whether `id` can run now, and its state value.
    pub fn command_state(&self, id: CommandId) -> Option<&CommandState> {
        self.state.commands.get(&id)
    }

    /// The checks before any step runs, in order: what the engine doesn't do yet, read only, document protection, the
    /// arguments, then availability in the current state.
    pub(super) fn check(&self, command: &Command) -> Result<(), Error> {
        self.check_where(command, true)
    }

    /// [`Session::check`]; `here`: also what depends on where the command runs (document protection, availability),
    /// for a command that runs at the selection as it is now. The editor checks the protection again as each command
    /// runs.
    pub(super) fn check_where(&self, command: &Command, here: bool) -> Result<(), Error> {
        let id = command.id();
        if crate::editing::commands::UNSUPPORTED.contains(&id) {
            return Err(Error::unsupported(&id_name(id)));
        }
        if self.read_only && info(id).is_some_and(|i| i.mutates) {
            return Err(Error::new(ErrorCode::ReadOnly, "the session is read only"));
        }
        if here
            && command.mutates()
            && let Some(e) = self.editor.protection_refusal(command)
        {
            return Err(e);
        }
        self.validate(command)?;
        if here {
            // Paste: this editor's copy or another session's
            let enabled = match command {
                Command::Paste(_) => self.editor.clipboard().is_some() || self.shared.clipboard.borrow().is_some(),
                // Space or X on a selected legacy check box fills it in where typing isn't allowed
                Command::InsertText(t) if matches!(t.as_str(), " " | "x" | "X") && self.editor.selected_form_check_box().is_some() => true,
                _ => self.state.commands.get(&id).is_none_or(|s| s.enabled),
            };
            if !enabled {
                return Err(Error::unavailable(format!("{} isn't available here", id_name(id))));
            }
        }
        Ok(())
    }

    /// Bad ranges, counts, sizes, empty text and ids that don't exist.
    fn validate(&self, command: &Command) -> Result<(), Error> {
        use Command as C;
        crate::editing::commands::bounds::bounds(command)?;
        match command {
            C::InsertText(t) => text("the text", t),
            // Until layout is complete the editor finds whether the page exists.
            C::GoToPage(p) => {
                let last = if self.state.pages.complete { f64::from(self.state.pages.count.max(1)) } else { f64::MAX };
                within("the page", f64::from(*p), 1.0, last)
            }
            C::SetFont(crate::api::command::FontChoice::Name(f)) => text("the font", f),
            C::SetLanguage(l) => match &l.language {
                Some(tag) if tag.is_empty() || tag.len() > 35 || !tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') => {
                    Err(Error::invalid("the language is a BCP 47 tag"))
                }
                _ => Ok(()),
            },
            C::SetStyleMeta(m) => {
                let built_in = crate::services::styles::builtin::BUILT_IN.iter().any(|b| b.id == m.id.0);
                if built_in || self.editor.document().resolver.as_ref().is_some_and(|r| r.has_style(&m.id.0)) {
                    Ok(())
                } else {
                    Err(Error::invalid(format!("no style {}", m.id.0)))
                }
            }
            C::SetDefaultTableStyle(Some(s)) => match self.editor.document().resolver.as_ref().and_then(|r| r.style_kind(&s.0)) {
                Some(crate::format::StyleKind::Table) => Ok(()),
                _ => Err(Error::invalid(format!("no table style {}", s.0))),
            },
            C::SetParagraphStyle(s) | C::SetTableStyle(s) => {
                use crate::format::StyleKind as K;
                let want = if matches!(command, C::SetTableStyle(_)) { K::Table } else { K::Paragraph };
                match self.editor.document().resolver.as_ref().and_then(|r| r.style_kind(&s.0)) {
                    Some(k) if k == want => Ok(()),
                    Some(_) => Err(Error::invalid(format!("{} isn't a {} style", s.0, if want == K::Table { "table" } else { "paragraph" }))),
                    None if self.editor.built_in_for(command) => Ok(()),
                    None => Err(Error::invalid(format!("no style {}", s.0))),
                }
            }
            C::UpdateStyleToSelection(s) | C::DeleteStyle(s) => {
                let built_in = !matches!(command, C::DeleteStyle(_)) && self.editor.built_in_for(command);
                if built_in || self.editor.document().resolver.as_ref().is_some_and(|r| r.has_style(&s.0)) {
                    Ok(())
                } else {
                    Err(Error::invalid(format!("no style {}", s.0)))
                }
            }
            C::SetMargins(m) => {
                for v in [m.top, m.bottom, m.left, m.right, m.gutter] {
                    length("the margin", v, 0.0, MAX)?;
                }
                Ok(())
            }
            C::SetPaperSize(s) => within("the width", s.width.0, 36.0, MAX).and(within("the height", s.height.0, 36.0, MAX)),
            C::SetColumns(c) => {
                count("the columns", Some(c.count), 45)?;
                length("the spacing", c.spacing, 0.0, MAX)?;
                let Some(widths) = &c.widths else { return Ok(()) };
                if widths.len() != c.count as usize {
                    return Err(Error::invalid("there is a width for each column"));
                }
                for w in widths {
                    if !(w.width.0.is_finite() && w.width.0 > 0.0) {
                        return Err(Error::invalid("a column's width must be more than 0"));
                    }
                    within("the space after a column", w.spacing.0, 0.0, MAX)?;
                }
                Ok(())
            }
            C::SetHeaderFooterDistance(d) => length("the header distance", d.header, 0.0, MAX).and(length("the footer distance", d.footer, 0.0, MAX)),
            C::InsertTable(t) => count("the rows", Some(t.rows), 32767).and(count("the columns", Some(t.columns), 63)),
            C::InsertPicture(p) => picture(&p.data.0),
            C::InsertHyperlink(h) => text("the address", &h.address),
            C::SetDocumentVariable(d) => text("the variable's name", &d.name),
            C::SetFormField(f) => {
                if let Some(n) = &f.name
                    && !crate::editing::bookmarks::valid_name(n)
                {
                    return Err(Error::invalid("a form field's name is a bookmark name"));
                }
                if let Some(entries) = f.drop_down.as_ref().and_then(|d| d.entries.as_ref())
                    && (entries.len() > 25 || entries.iter().any(|e| e.is_empty() || e.chars().count() > 50))
                {
                    return Err(Error::invalid("a drop-down has at most 25 entries of 1 to 50 characters"));
                }
                if let Some(crate::api::value::AutoOr::Value(p)) = f.check_box.and_then(|c| c.size) {
                    within("the size", p.0, 1.0, MAX)?;
                }
                Ok(())
            }
            C::SetDocumentProperties(p) => {
                let custom = p.custom.as_deref().unwrap_or_default();
                for (k, c) in custom.iter().enumerate() {
                    text("a custom property's name", &c.name)?;
                    if custom[..k].iter().any(|d| d.name.eq_ignore_ascii_case(&c.name)) {
                        return Err(Error::invalid(format!("two custom properties are named {}", c.name)));
                    }
                }
                Ok(())
            }
            C::InsertBookmark(n) if !crate::editing::bookmarks::valid_name(n) => {
                Err(Error::invalid("a bookmark name starts with a letter and holds letters, digits and underscores, 40 at most"))
            }
            C::GoToBookmark(n) if !n.eq_ignore_ascii_case("_top") && self.editor.bookmark(n).is_none() => Err(Error::invalid("no such bookmark")),
            C::GoTo(g) => {
                use crate::api::command::{GoToTarget as T, GoToWhat as W};
                match (g.what, g.target, g.name.as_deref().map(str::trim)) {
                    (_, T::Absolute(0), _) => Err(Error::invalid("numbers count from 1")),
                    (_, T::Relative(0), _) => Err(Error::invalid("no move")),
                    (W::Bookmark, T::Absolute(_), Some(n))
                        if !n.is_empty() && !n.eq_ignore_ascii_case("_top") && self.editor.bookmark(n).is_none() =>
                    {
                        Err(Error::invalid("no such bookmark"))
                    }
                    _ => Ok(()),
                }
            }
            C::DeleteBookmark(n) if self.editor.bookmark(n).is_none() => Err(Error::invalid("no such bookmark")),
            C::EditHyperlink(h) => h.address.as_deref().map_or(Ok(()), |a| text("the address", a)),
            C::InsertRows(r) => count("the rows", r.count, 32767),
            C::InsertColumns(c) => count("the columns", c.count, 63),
            C::SplitCells(s) => count("the columns", Some(s.columns), 63).and(count("the rows", s.rows, 32767)),
            C::ConvertTextToTable(t) => separator(t.separator, t.character.as_deref()).and(count("the columns", t.columns, 63)),
            C::ConvertTableToText(t) => separator(t.separator, t.character.as_deref()),
            C::SetCellMargins(m) => {
                for v in [m.top, m.bottom, m.left, m.right] {
                    length("the margin", v, 0.0, MAX)?;
                }
                Ok(())
            }
            C::MoveColumnBoundary(m) => within("the position", m.to.0, -MAX, MAX),
            C::MoveTableColumnBoundary(b) | C::MoveTableRowBoundary(b) => within("the position", b.to.0, -MAX, MAX),
            C::SetRowHeight(crate::api::value::AutoOr::Value(h)) => within("the height", h.height.0, 0.0, MAX),
            C::SetColumnWidth(w) => within("the width", w.0, 1.0, MAX),
            C::SetTableWidth(TableWidth::Points { value }) => within("the width", value.0, 0.0, MAX),
            C::SetTableWidth(TableWidth::Percent { value }) => within("the width", *value, 1.0, 600.0),
            C::SetCellProperties(p) => {
                match p.width {
                    Some(TableWidth::Points { value }) => within("the width", value.0, 0.0, MAX)?,
                    Some(TableWidth::Percent { value }) => within("the width", value, 0.0, 100.0)?,
                    _ => {}
                }
                Ok(())
            }
            C::SetTableCellSpacing(s) => length("the spacing", *s, 0.0, MAX),
            C::SetRowProperties(p) => length("the spacing", p.exceptions.as_ref().and_then(|x| x.cell_spacing), 0.0, MAX),
            C::SetObjectSize(s) => {
                length("the width", s.width, 1.0, MAX)?;
                length("the height", s.height, 1.0, MAX)?;
                for r in [s.relative_width, s.relative_height] {
                    if let Some(crate::api::value::NoneOr::Value(r)) = r {
                        within("the relative size", r.percent, 1.0, 1000.0)?;
                    }
                }
                Ok(())
            }
            C::InsertShape(i) => length("the width", i.width, 0.0, MAX).and(length("the height", i.height, 0.0, MAX)),
            C::SetShapeFill(f) => f.to_model(&mut |data| crate::docx::image::image_info(data).map(|_| 0)).map(|_| ()).map_err(Error::invalid),
            C::SetShapeLine(l) => l.apply(None, &mut |_| Some(0)).map(|_| ()).map_err(Error::invalid),
            C::SetShapeEffects(e) => e.apply(None).map(|_| ()).map_err(Error::invalid),
            C::SetShapeText(t) => t.apply(&crate::drawing::text::BodyProps::default()).map(|_| ()).map_err(Error::invalid),
            C::SetPictureAdjust(a) => a.apply(&[]).map(|_| ()).map_err(Error::invalid),
            C::SetObjectRotation(d) => within("the rotation", *d, 0.0, 360.0),
            C::SetPictureCrop(c) => {
                for v in [c.left, c.top, c.right, c.bottom].into_iter().flatten() {
                    within("the crop", v, -1.0, 1.0)?;
                }
                Ok(())
            }
            C::ChangePicture(data) => picture(&data.0),
            C::SetShapeAdjust(a) => {
                let count = self.editor.shape_info().and_then(|s| s.state).map_or(0, |s| s.adjust.len());
                if usize::from(a.index) < count { Ok(()) } else { Err(Error::invalid(format!("the shape has {count} adjust values"))) }
            }
            C::SetObjectPosition(p) => {
                placed([p.horizontal.map(|h| (h.align.is_some(), h.offset)), p.vertical.map(|v| (v.align.is_some(), v.offset))])?;
                for (aligned, percent) in
                    [p.horizontal.map(|h| (h.align.is_some(), h.percent)), p.vertical.map(|v| (v.align.is_some(), v.percent))].into_iter().flatten()
                {
                    if aligned && percent.is_some() {
                        return Err(Error::invalid("align and percent exclude each other"));
                    }
                    percent.map_or(Ok(()), |v| within("the percent", v, -1000.0, 1000.0))?;
                }
                Ok(())
            }
            C::SetTablePosition(Some(p)) => {
                placed([p.horizontal.map(|h| (h.align.is_some(), h.offset)), p.vertical.map(|v| (v.align.is_some(), v.offset))])?;
                let d = p.wrap_distance.map(|d| [d.top, d.bottom, d.left, d.right]).unwrap_or_default();
                d.into_iter().try_for_each(|v| within("the distance", v.0, 0.0, MAX))
            }
            C::SetFrame(f) => {
                use crate::api::command::{FrameHeightRule, FrameWidthRule};
                if f.width_rule == FrameWidthRule::Exactly {
                    within("the width", f.width.0, 0.0, MAX)?;
                }
                if f.height_rule != FrameHeightRule::Auto {
                    within("the height", f.height.0, 0.0, MAX)?;
                }
                within("the distance", f.horizontal_distance.0, 0.0, MAX)?;
                within("the distance", f.vertical_distance.0, 0.0, MAX)?;
                if f.horizontal_align.is_none() {
                    within("the position", f.horizontal.0, -MAX, MAX)?;
                }
                if f.vertical_align.is_none() {
                    within("the position", f.vertical.0, -MAX, MAX)?;
                }
                Ok(())
            }
            C::SetObjectWrapOptions(w) => {
                for v in [w.top, w.bottom, w.left, w.right] {
                    length("the distance", v, 0.0, MAX)?;
                }
                Ok(())
            }
            C::SetField(f) => self.known_element(&f.id),
            C::SetFieldResults(r) => r.results.iter().try_for_each(|r| self.known_element(&r.id)),
            C::UpdateFields(i) | C::UnlinkFields(i) => self.known_elements(i.ids.as_deref()),
            C::SetFieldsLocked(l) => self.known_elements(l.ids.as_deref()),
            C::ShowFieldCodes(s) => self.known_elements(s.ids.as_deref()),
            C::SetControl(c) => self.known_element(&c.id),
            C::RemoveControl(c) => self.known_element(&c.id),
            C::SetCheckbox(c) => self.known_element(&c.id),
            C::FillControl(f) => {
                self.known_element(&f.id)?;
                let given = [f.text.is_some(), f.image.is_some(), f.docx.is_some(), f.building_block.is_some(), f.date.is_some()];
                if given.iter().filter(|x| **x).count() != 1 {
                    return Err(Error::invalid("give exactly one of text, image, docx, buildingBlock and date"));
                }
                f.image.as_ref().map_or(Ok(()), |i| picture(&i.0))
            }
            C::RemoveElement(id) => self.known_element(id),
            C::ReplaceInElements(r) => text("find", &r.find).and(self.known_elements(r.ids.as_deref())),
            C::ReplaceFont(r) => {
                text("from", &r.from)?;
                text("to", &r.to)?;
                if r.from.eq_ignore_ascii_case(&r.to) {
                    return Err(Error::invalid("from and to must name different families"));
                }
                Ok(())
            }
            C::MarkElements(m) => m.marks.iter().try_for_each(|m| self.known_element(&m.id)),
            C::ActivatePictureControl(a) => a.element.as_ref().map_or(Ok(()), |id| self.known_element(id)),
            C::WriteDataPart(w) => text("the namespace", &w.namespace),
            C::RemoveDataPart(r) => text("the namespace", &r.namespace),
            C::AcceptRevision(id) | C::RejectRevision(id) => match crate::api::record::parse_revision_id(id) {
                Some(_) => Ok(()),
                None => Err(Error::invalid(format!("no revision {}", id.0))),
            },
            C::ReplyToComment(r) => self.known_comment(&r.to),
            C::EditComment(e) => self.known_comment(&e.id),
            C::DeleteComment(id) | C::SelectComment(id) => self.known_comment(id),
            C::ResolveComment(r) => self.known_comment(&r.id),
            C::SetActiveComment(Some(id)) => self.known_comment(id),
            C::SetZoom(Zoom::Percent(z)) => within("the zoom", *z, 10.0, 500.0),
            _ => Ok(()),
        }
    }

    /// An element id that exists.
    fn known_element(&self, id: &ElementId) -> Result<(), Error> {
        match crate::editing::elements::parse_id(&id.0) {
            Some(e) if self.editor.index().element(e).is_some() => Ok(()),
            _ => Err(Error::invalid(format!("no element {}", id.0))),
        }
    }

    fn known_elements(&self, ids: Option<&[ElementId]>) -> Result<(), Error> {
        ids.into_iter().flatten().try_for_each(|id| self.known_element(id))
    }

    /// A comment id that exists.
    fn known_comment(&self, id: &CommentId) -> Result<(), Error> {
        let found = crate::api::record::parse_comment_id(id).is_some_and(|c| self.editor.document().comments.by_id.contains_key(&c));
        if found { Ok(()) } else { Err(Error::invalid(format!("no comment {}", id.0))) }
    }

    /// The commands the session keeps, not the document: the zoom, the view options and the mode. `None` for the
    /// others.
    /// Applies `setAutoCorrect`'s change to the session's AutoCorrect settings.
    fn set_auto_correct(&mut self, c: &crate::api::services::AutoCorrectChange) {
        let a = &mut self.autocorrect;
        let set = |to: &mut bool, from: Option<bool>| {
            if let Some(v) = from {
                *to = v;
            }
        };
        set(&mut a.two_initial_capitals, c.two_initial_capitals);
        set(&mut a.sentence_capitals, c.sentence_capitals);
        set(&mut a.cell_capitals, c.cell_capitals);
        set(&mut a.day_names, c.day_names);
        set(&mut a.replace_text, c.replace_text);
        set(&mut a.smart_quotes, c.smart_quotes);
        set(&mut a.ordinals, c.ordinals);
        set(&mut a.fractions, c.fractions);
        set(&mut a.dashes, c.dashes);
        set(&mut a.numbered_lists, c.numbered_lists);
        set(&mut a.bulleted_lists, c.bulleted_lists);
        set(&mut a.border_lines, c.border_lines);
        set(&mut a.internet_paths, c.internet_paths);
        if let Some(on) = c.tab_indents {
            (self.tab_indents, self.editor.indent_keys) = (on, on);
        }
        if let Some(e) = &c.entries {
            a.entries = Some(e.iter().map(|e| (e.from.clone(), e.to.clone())).collect());
        }
        if let Some(e) = &c.first_letter_exceptions {
            a.first_letter_exceptions = Some(e.clone());
        }
        if let Some(e) = &c.initial_caps_exceptions {
            a.initial_caps_exceptions = Some(e.clone());
        }
        if let Some(n) = &c.never {
            a.never = n.clone();
        }
    }

    pub(super) fn run_view(&mut self, command: &Command) -> Option<Result<Commit, Error>> {
        match command {
            Command::SetZoom(z) => {
                let percent = match z {
                    Zoom::Percent(p) => *p,
                    Zoom::Fit(fit) => {
                        let page = self.editor.snapshot().pages().first().map_or((612.0, 792.0), |p| (p.w, p.h));
                        let (w, h) = self.presentation.viewport();
                        let px = 96.0 / 72.0;
                        let by_width = w / ((page.0 + 2.0 * crate::editing::view::PAGE_GAP) * px);
                        let factor = match fit {
                            ZoomFit::PageWidth => by_width,
                            ZoomFit::WholePage => by_width.min(h / ((page.1 + 2.0 * crate::editing::view::PAGE_GAP) * px)),
                        };
                        (factor * 100.0).clamp(10.0, 500.0)
                    }
                };
                self.set_zoom(percent / 100.0);
            }
            Command::SetViewOptions(v) => {
                if let Some(g) = v.gridlines {
                    self.gridlines = g;
                }
                let m = &mut self.marks;
                for (to, from) in [
                    (&mut m.all, v.formatting_marks),
                    (&mut m.tabs, v.tab_characters),
                    (&mut m.spaces, v.spaces),
                    (&mut m.paragraphs, v.paragraph_marks),
                    (&mut m.hidden, v.hidden_text),
                    (&mut m.optional_hyphens, v.optional_hyphens),
                    (&mut m.anchors, v.object_anchors),
                ] {
                    if let Some(on) = from {
                        *to = on;
                    }
                }
                let fonts = self.fonts.fonts();
                if matches!(self.editor.set_mark_options(self.marks), Outcome::Moved) {
                    // Hidden text and optional hyphens lay the document out again.
                    self.editor.ensure_current(&fonts);
                    self.presentation.want_frame();
                }
                drop(fonts);
                if let Some(c) = v.click_and_type {
                    self.click_and_type = c;
                    self.editor.set_click_and_type(c);
                }
                if let Some(d) = v.text_drag_and_drop {
                    self.text_drags = d;
                    self.editor.set_text_drag_and_drop(d);
                }
                if let Some(h) = v.hide_white_space
                    && matches!(self.editor.set_white_space_hidden(h), Outcome::Moved)
                {
                    self.presentation.want_frame();
                }
                if let Some(b) = v.text_boundaries {
                    self.text_boundaries = b;
                }
                if let Some(f) = v.field_shading {
                    self.field_shading = f;
                }
                if let Some(b) = v.bookmarks {
                    self.show_bookmarks = b;
                }
                if let Some(r) = v.show_editable_regions {
                    self.show_editable_regions = r;
                    self.presentation.want_frame();
                }
                if let Some(o) = &v.cursor_overrides {
                    self.cursor_overrides.clone_from(o);
                }
                if let Some(l) = v.live_layout {
                    (self.live_layout, self.editor.live_layout) = (l, l);
                }
                if let Some(c) = v.control_labels {
                    (self.control_labels, self.editor.control_labels) = (c, c);
                }
                let p = &mut self.presentation;
                if let Some(t) = v.theme {
                    (p.chrome, p.chrome_set) = (t, true);
                }
                if let Some(c) = v.theme_colors {
                    p.theme_colors.light = c.light.or(p.theme_colors.light);
                    p.theme_colors.dark = c.dark.or(p.theme_colors.dark);
                }
                if let Some(a) = v.accent {
                    p.accent = Some(a);
                }
                if let Some(a) = v.appearance {
                    p.appearance = a;
                }
                if let Some(d) = v.detail {
                    p.detail = d;
                }
                if let Some(g) = v.object_grid {
                    p.object_grid = g;
                }
                if v.theme.is_some() || v.theme_colors.is_some() || v.accent.is_some() || v.appearance.is_some() {
                    p.apply_look();
                }
            }
            Command::SetMode(m) => self.set_mode(*m),
            Command::SetRulerTabType(t) => {
                self.presentation.tab_selector = *t;
                // The picker's answer closes it.
                if matches!(self.presentation.popover, Some(Popover::TabType { .. })) {
                    self.close_popover();
                    return Some(Ok(Commit { record: None, value: None }));
                }
            }
            Command::ClosePopover => {
                self.close_popover();
                return Some(Ok(Commit { record: None, value: None }));
            }
            Command::ShowTabTypes => {
                self.show_popover(self.tab_types_popover());
                return Some(Ok(Commit { record: None, value: None }));
            }
            Command::ShowTabStop => {
                let caret = self.host().caret_client_point();
                if let Some(p) = self.nearest_tab_stop(caret).and_then(|at| self.tab_stop_popover(at)) {
                    self.show_popover(p);
                }
                return Some(Ok(Commit { record: None, value: None }));
            }
            Command::SetAutoCorrect(c) => self.set_auto_correct(c),
            Command::SetPasteOptions(c) => {
                if let Outcome::Refused(e) = self.editor.set_paste_options(c) {
                    return Some(Err(e));
                }
                self.paste_defaults = self.editor.paste_defaults();
            }
            Command::SetAutoCaption(c) => {
                if let Outcome::Refused(e) = self.editor.set_auto_caption_api(c.as_ref()) {
                    return Some(Err(e));
                }
                self.auto_caption = self.editor.auto_caption().cloned();
            }
            Command::DismissPasteOptions => {
                self.editor.dismiss_paste_options();
                self.close_popover();
            }
            Command::ShowAutoCorrectOptions => {
                if let Some(p) = self.auto_correct_popover() {
                    self.show_popover(p);
                }
                return Some(Ok(Commit { record: None, value: None }));
            }
            Command::NeverAutoCorrect if self.editor.auto_format_change().is_some() => {
                use crate::editing::CorrectionKind as K;
                let said = match self.editor.auto_format_change().map(|c| &c.kind) {
                    Some(K::Hyperlink) => "Won\u{2019}t create hyperlinks automatically",
                    Some(K::BorderLine { .. }) => "Won\u{2019}t create border lines automatically",
                    _ => "Won\u{2019}t set indents with tabs and backspaces",
                };
                self.close_popover();
                let mut ac = std::mem::take(&mut self.autocorrect);
                let done = self.apply_with(Origin::Api, |e, fonts| e.stop_auto_format(&mut ac, fonts));
                self.autocorrect = ac;
                self.tab_indents = self.editor.indent_keys;
                self.services.said.push((crate::api::services::AnnouncementKind::Command, said.into()));
                self.changed(Vec::new());
                return Some(done);
            }
            Command::RevertAutoCorrect | Command::NeverAutoCorrect => {
                let never = matches!(command, Command::NeverAutoCorrect);
                let from = self.editor.correction().map(|c| c.from.clone());
                self.close_popover();
                let done = self.apply_with(Origin::Api, |e, fonts| e.revert_correction(fonts));
                if never && let Some(from) = from {
                    let text = format!("Won\u{2019}t autocorrect \u{201C}{from}\u{201D} again");
                    self.autocorrect.never.push(from);
                    self.services.said.push((crate::api::services::AnnouncementKind::Command, text));
                    self.changed(Vec::new());
                }
                return Some(done);
            }
            _ => return None,
        }
        self.changed(Vec::new());
        Some(Ok(Commit { record: None, value: None }))
    }
}
