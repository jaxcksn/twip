//! Commands: the catalog, each command's availability in the current state, the checks a command passes before any
//! step runs (read only, then availability, then its arguments), and the view commands the session keeps.

use super::{Commit, Session};
use crate::api::catalog::{CommandInfo, info};
use crate::api::command::{Command, CommandId, Zoom, ZoomFit};
use crate::api::error::{Error, ErrorCode};
use crate::api::event::Event;
use crate::api::record::Origin;
use crate::api::services::Popover;
use crate::api::state::CommandState;
use crate::api::value::{CommentId, ElementId, LineSpacing, Points, TableWidth};

/// The longest length most settings take, 22 in.
const MAX: f64 = 1584.0;

/// `v` within `lo..=hi`, else an error naming `what`.
fn within(what: &str, v: f64, lo: f64, hi: f64) -> Result<(), Error> {
    if v.is_finite() && (lo..=hi).contains(&v) { Ok(()) } else { Err(Error::invalid(format!("{what} must be from {lo} to {hi}"))) }
}

fn length(what: &str, v: Option<Points>, lo: f64, hi: f64) -> Result<(), Error> {
    v.map_or(Ok(()), |v| within(what, v.0, lo, hi))
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
        None => Err(Error::invalid("the data isn't a PNG, JPEG, GIF, BMP or TIFF picture")),
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

    /// The checks before any step runs, in order: what the engine doesn't do yet, read only, availability in the
    /// current state, then the arguments.
    pub(super) fn check(&self, command: &Command) -> Result<(), Error> {
        let id = command.id();
        if crate::editing::commands::UNSUPPORTED.contains(&id) {
            return Err(Error::unsupported(&id_name(id)));
        }
        if self.read_only && info(id).is_some_and(|i| i.mutates) {
            return Err(Error::new(ErrorCode::ReadOnly, "the session is read only"));
        }
        if self.state.commands.get(&id).is_some_and(|s| !s.enabled) {
            return Err(Error::unavailable(format!("{} isn't available here", id_name(id))));
        }
        self.validate(command)
    }

    /// Bad ranges, counts, sizes, empty text and ids that don't exist.
    fn validate(&self, command: &Command) -> Result<(), Error> {
        use Command as C;
        match command {
            C::InsertText(t) => text("the text", t),
            // Until layout is complete the editor finds whether the page exists.
            C::GoToPage(p) => {
                let last = if self.state.pages.complete { f64::from(self.state.pages.count.max(1)) } else { f64::MAX };
                within("the page", f64::from(*p), 1.0, last)
            }
            C::SetFont(f) => text("the font", f),
            C::SetFontSize(s) => {
                within("the size", s.0, 1.0, 1638.0)?;
                if (s.0 * 2.0).fract() != 0.0 { Err(Error::invalid("the size goes in 0.5 pt steps")) } else { Ok(()) }
            }
            C::SetCharacterSpacing(s) | C::SetCharacterPosition(s) | C::SetTableIndent(s) => within("the length", s.0, -MAX, MAX),
            C::SetLineSpacing(LineSpacing::Multiple { lines }) => within("the lines", *lines, 0.06, 132.0),
            C::SetLineSpacing(LineSpacing::Exact { size } | LineSpacing::AtLeast { size }) => within("the size", size.0, 0.7, MAX),
            C::SetParagraphSpacing(s) => {
                for v in [&s.before, &s.after] {
                    if let Some(crate::api::value::AutoOr::Value(p)) = v {
                        within("the spacing", p.0, 0.0, MAX)?;
                    }
                }
                Ok(())
            }
            C::SetIndents(i) => {
                length("the left indent", i.left, -MAX, MAX)?;
                length("the right indent", i.right, -MAX, MAX)?;
                length("the first line indent", i.first_line, 0.0, MAX)?;
                length("the hanging indent", i.hanging, 0.0, MAX)?;
                if i.first_line.is_some() && i.hanging.is_some() {
                    return Err(Error::invalid("firstLine and hanging exclude each other"));
                }
                Ok(())
            }
            C::SetParagraphStyle(s) | C::SetTableStyle(s) => {
                if self.editor.document().resolver.as_ref().is_some_and(|r| r.styles.by_id.contains_key(&s.0)) {
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
            C::SetColumns(c) => count("the columns", Some(c.count), 45).and(length("the spacing", c.spacing, 0.0, MAX)),
            C::SetHeaderFooterDistance(d) => length("the header distance", d.header, 0.0, MAX).and(length("the footer distance", d.footer, 0.0, MAX)),
            C::InsertTable(t) => count("the rows", Some(t.rows), 32767).and(count("the columns", Some(t.columns), 63)),
            C::InsertPicture(p) => picture(&p.data.0),
            C::InsertHyperlink(h) => text("the address", &h.address),
            C::EditHyperlink(h) => h.address.as_deref().map_or(Ok(()), |a| text("the address", a)),
            C::InsertRows(r) => count("the rows", r.count, 32767),
            C::InsertColumns(c) => count("the columns", c.count, 63),
            C::SplitCells(s) => count("the columns", Some(s.columns), 63).and(count("the rows", s.rows, 32767)),
            C::SetCellBorders(b) => b.border.as_ref().map_or(Ok(()), |b| within("the width", b.width.0, 0.25, 6.0)),
            C::SetCellMargins(m) => {
                for v in [m.top, m.bottom, m.left, m.right] {
                    length("the margin", v, 0.0, MAX)?;
                }
                Ok(())
            }
            C::SetRowHeight(crate::api::value::AutoOr::Value(h)) => within("the height", h.height.0, 0.0, MAX),
            C::SetColumnWidth(w) => within("the width", w.0, 1.0, MAX),
            C::SetTableWidth(TableWidth::Points { value }) => within("the width", value.0, 0.0, MAX),
            C::SetTableWidth(TableWidth::Percent { value }) => within("the width", *value, 1.0, 600.0),
            C::SetObjectSize(s) => length("the width", s.width, 1.0, MAX).and(length("the height", s.height, 1.0, MAX)),
            C::InsertShape(i) => length("the width", i.width, 0.0, MAX).and(length("the height", i.height, 0.0, MAX)),
            C::SetShapeFill(f) => f.to_model(&mut |data| crate::docx::image::image_info(data).map(|_| 0)).map(|_| ()).map_err(Error::invalid),
            C::SetShapeLine(l) => l.apply(None, &mut |_| Some(0)).map(|_| ()).map_err(Error::invalid),
            C::SetShapeEffects(e) => e.apply(None).map(|_| ()).map_err(Error::invalid),
            C::SetShapeText(t) => t.apply(&crate::drawing::text::BodyProps::default()).map(|_| ()).map_err(Error::invalid),
            C::SetPictureAdjust(a) => a.apply(&[]).map(|_| ()).map_err(Error::invalid),
            C::SetObjectRotation(d) => within("the rotation", *d, 0.0, 360.0),
            C::SetShapeAdjust(a) => {
                let count = self.editor.shape_info().and_then(|s| s.state).map_or(0, |s| s.adjust.len());
                if usize::from(a.index) < count { Ok(()) } else { Err(Error::invalid(format!("the shape has {count} adjust values"))) }
            }
            C::SetObjectPosition(p) => {
                let axes = [p.horizontal.map(|h| (h.align.is_some(), h.offset)), p.vertical.map(|v| (v.align.is_some(), v.offset))];
                for (aligned, offset) in axes.into_iter().flatten() {
                    if aligned && offset.is_some() {
                        return Err(Error::invalid("align and offset exclude each other"));
                    }
                    length("the offset", offset, -MAX, MAX)?;
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
                if [f.text.is_some(), f.image.is_some(), f.docx.is_some()].iter().filter(|x| **x).count() != 1 {
                    return Err(Error::invalid("give exactly one of text, image and docx"));
                }
                f.image.as_ref().map_or(Ok(()), |i| picture(&i.0))
            }
            C::RemoveElement(id) => self.known_element(id),
            C::ReplaceInElements(r) => text("find", &r.find).and(self.known_elements(r.ids.as_deref())),
            C::MarkElements(m) => m.marks.iter().try_for_each(|m| self.known_element(&m.id)),
            C::FillPlaceholder(f) => picture(&f.image.0),
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
                if let Some(m) = v.formatting_marks {
                    self.formatting_marks = m;
                }
                if let Some(b) = v.text_boundaries {
                    self.text_boundaries = b;
                }
                if let Some(l) = v.live_layout {
                    self.editor.live_layout = l;
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
                let x = self.host().caret_client_point().client_x;
                if let Some(p) = self.nearest_tab_stop(x).and_then(|at| self.tab_stop_popover(at)) {
                    self.show_popover(p);
                }
                return Some(Ok(Commit { record: None, value: None }));
            }
            Command::SetAutoCorrect(c) => self.set_auto_correct(c),
            Command::ShowAutoCorrectOptions => {
                if let Some(p) = self.auto_correct_popover() {
                    self.show_popover(p);
                }
                return Some(Ok(Commit { record: None, value: None }));
            }
            Command::RevertAutoCorrect | Command::NeverAutoCorrect => {
                let never = matches!(command, Command::NeverAutoCorrect);
                let from = self.editor.correction().map(|c| c.from.clone());
                self.close_popover();
                let done = self.apply_with(Origin::Api, |e, fonts| e.revert_correction(fonts));
                if never && let Some(from) = from {
                    let text = format!("Won\u{2019}t autocorrect \u{201C}{from}\u{201D} again");
                    self.autocorrect.never.push(from);
                    self.changed(vec![Event::Announcement { text }]);
                }
                return Some(done);
            }
            _ => return None,
        }
        self.changed(Vec::new());
        Some(Ok(Commit { record: None, value: None }))
    }
}
