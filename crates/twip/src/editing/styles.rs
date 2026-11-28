//! Style edits

use std::rc::Rc;

use crate::api::command::Command;
use crate::api::error::Error;
use crate::docx::props::{Tab, TabKind};
use crate::docx::resolvers::{Resolvers, TypedDefs};
use crate::docx::style_xml::StyleEditError;
use crate::docx::style_xml::{DefaultsDef, StyleDef};
use crate::docx::write::props::Cx;
use crate::docx::{Paragraph, RunStyle};
use crate::font::Fonts;
use crate::format::StyleKind;
use crate::format::{CellEdit, DefaultsSet, ParaEdit, ParaProps, Region, RegionKind, RowEdit, Style, StyleFormat, StyleProp, StyleSet, TableEdit};

use super::Editor;
use super::commands::Outcome;
use super::step::Step;
use crate::docx::char_props::RunDirect;
use crate::docx::run_xml;
use crate::format::{CharEdit, CharProps, Prop};

/// The text's shading set to `sh` over its own and `run`'s, its formatting; `none` takes its own away.
fn text_shading(e: &mut CharEdit, sh: &super::table_format::Shading, none: bool, run: &RunStyle) {
    use crate::docx::props::ShadingPattern as P;
    let was = e.shading;
    e.shading = (!none).then(|| {
        // A pattern the change keeps: the text's own, else its style's
        let pattern = sh
            .pattern
            .map(crate::docx::line_style::shading_pattern)
            .or_else(|| was.map(|s| s.pattern))
            .or_else(|| run.shd.as_ref().map(|s| s.pattern).filter(|p| *p != P::Nil))
            .unwrap_or(P::Clear);
        let c = |c: Option<super::theme::WordColour>| Some(c.map_or(crate::format::Color::Auto, super::format::colour));
        crate::format::Shading { pattern, color: c(sh.color), fill: c(sh.fill) }
    });
    e.whole(Prop::Shading);
}

/// The style the document defaults are formatted as: a paragraph style that sets nothing.
fn defaults_style() -> Style {
    Style { id: "DocumentDefaults".into(), kind: crate::format::StyleKind::Paragraph, name: Some("Document Defaults".into()), ..Style::default() }
}

/// Refuses a command [`Editor::format_style`] doesn't run on a style.
pub(super) fn formats_style(commands: &[Command]) -> Result<(), Error> {
    match commands.iter().find(|c| !formats_a_style(c)) {
        Some(c) => Err(Error::invalid(format!("{:?} doesn't format a style", c.id()))),
        None => Ok(()),
    }
}

/// The commands [`Editor::format_style`] runs on a style: character and paragraph formatting.
pub fn formats_a_style(c: &Command) -> bool {
    formats_a_style_id(c.id())
}

/// Whether [`formats_a_style`] takes the commands `id` names.
pub fn formats_a_style_id(id: crate::api::command::CommandId) -> bool {
    use crate::api::command::CommandId as C;
    matches!(
        id,
        C::SetBold
            | C::SetItalic
            | C::SetUnderline
            | C::SetStrikethrough
            | C::SetDoubleStrikethrough
            | C::SetSuperscript
            | C::SetSubscript
            | C::SetSmallCaps
            | C::SetAllCaps
            | C::SetHidden
            | C::SetFont
            | C::SetFontSize
            | C::GrowFont
            | C::ShrinkFont
            | C::SetFontColor
            | C::SetHighlight
            | C::SetCharacterSpacing
            | C::SetCharacterPosition
            | C::SetTypography
            | C::SetLanguage
            | C::SetCharacterEffects
            | C::SetTextEffects
            | C::SetUnderlineColor
            | C::SetAlignment
            | C::SetLineSpacing
            | C::SetParagraphSpacing
            | C::SetIndents
            | C::IncreaseIndent
            | C::DecreaseIndent
            | C::SetParagraphFlow
            | C::SetOutline
            | C::SetParagraphTextDirection
            | C::SetParagraphBorders
            | C::SetParagraphShading
            | C::SetParagraphBorderLines
            | C::SetShading
            | C::SetTextBorder
            | C::SetFrame
            | C::RemoveFrame
            | C::AddTabStop
            | C::DeleteTabStop
            | C::ClearTabStops
    )
}

// Typed edits of a style

/// A style being formatted: the holder it started from, its definition now, and what the formatting set, in order.
#[derive(Clone, Debug)]
pub(crate) struct StyleTarget {
    start: StyleDef,
    now: Style,
    set: Vec<StyleSet>,
}

impl StyleTarget {
    /// `start`, to format.
    pub(super) fn of(start: StyleDef) -> StyleTarget {
        StyleTarget { now: (**start.get()).clone(), start, set: Vec::new() }
    }

    /// Its holder: the one it started from as the formatting leaves it.
    pub(super) fn def(&self) -> StyleDef {
        if self.set.is_empty() && self.now == **self.start.get() {
            return self.start.clone();
        }
        self.start.edited(self.now.clone(), &self.set)
    }

    /// Its definition now.
    pub(super) fn style(&self) -> &Style {
        &self.now
    }

    /// What the formatting set, in order.
    pub(super) fn set(&self) -> &[StyleSet] {
        &self.set
    }

    /// Its definition as `edit` changes it, what it sets noted.
    pub(super) fn edit(&mut self, edit: impl FnOnce(&mut Style, &mut Vec<StyleSet>)) {
        edit(&mut self.now, &mut self.set);
    }
}

/// `def` as `edit` changes its definition, what it sets noted.
pub(super) fn edit_style(def: &StyleDef, edit: impl FnOnce(&mut Style, &mut Vec<StyleSet>)) -> StyleDef {
    let mut t = StyleTarget::of(def.clone());
    t.edit(edit);
    t.def()
}

/// The formatting `s` sets at its base (`region` none) or in its region `region`, the region made when it has none.
pub(super) fn level_mut(s: &mut Style, region: Option<RegionKind>) -> &mut StyleFormat {
    let Some(kind) = region else { return &mut s.format };
    let at = match s.regions.iter().position(|r| r.kind == kind) {
        Some(i) => i,
        None => {
            s.regions.push(Region { kind, format: StyleFormat::default(), foreign: None });
            s.regions.len() - 1
        }
    };
    &mut s.regions[at].format
}

/// The character formatting of `s` at its base or in `region` edited by `edit`, what it sets noted.
pub(super) fn edit_chars(s: &mut Style, set: &mut Vec<StyleSet>, region: Option<RegionKind>, edit: impl FnOnce(&mut CharEdit)) {
    let f = level_mut(s, region);
    let mut e = CharEdit::new(std::mem::take(&mut f.char));
    edit(&mut e);
    f.char = e.props;
    set.extend(e.set.into_iter().map(|x| StyleSet::Char(region, x)));
}

/// The paragraph formatting of `s` at its base or in `region` edited by `edit`, what it sets noted.
pub(super) fn edit_paras(s: &mut Style, set: &mut Vec<StyleSet>, region: Option<RegionKind>, edit: impl FnOnce(&mut ParaEdit)) {
    let f = level_mut(s, region);
    let mut e = ParaEdit::new(std::mem::take(&mut f.para));
    edit(&mut e);
    f.para = e.props;
    set.extend(e.set.into_iter().map(|x| StyleSet::Para(region, x)));
}

/// The table properties of `s` at its base or in `region` edited by `edit`, what it sets noted.
pub(super) fn edit_tables(s: &mut Style, set: &mut Vec<StyleSet>, region: Option<RegionKind>, edit: impl FnOnce(&mut TableEdit)) {
    let f = level_mut(s, region);
    let mut e = TableEdit::new(std::mem::take(&mut f.table));
    edit(&mut e);
    f.table = e.props;
    set.extend(e.set.into_iter().map(|x| StyleSet::Table(region, x)));
}

/// The row properties of `s` at its base or in `region` edited by `edit`, what it sets noted.
pub(super) fn edit_rows(s: &mut Style, set: &mut Vec<StyleSet>, region: Option<RegionKind>, edit: impl FnOnce(&mut RowEdit)) {
    let f = level_mut(s, region);
    let mut e = RowEdit::new(std::mem::take(&mut f.row));
    edit(&mut e);
    f.row = e.props;
    set.extend(e.set.into_iter().map(|x| StyleSet::Row(region, x)));
}

/// The cell properties of `s` at its base or in `region` edited by `edit`, what it sets noted.
pub(super) fn edit_cells(s: &mut Style, set: &mut Vec<StyleSet>, region: Option<RegionKind>, edit: impl FnOnce(&mut CellEdit)) {
    let f = level_mut(s, region);
    let mut e = CellEdit::new(std::mem::take(&mut f.cell));
    edit(&mut e);
    f.cell = e.props;
    set.extend(e.set.into_iter().map(|x| StyleSet::Cell(region, x)));
}

/// Style `id`'s holder: a document style's, else a glossary style's from its XML as the glossary holds it.
pub(super) fn style_holder(typed: &TypedDefs, id: &str) -> Option<StyleDef> {
    match typed.styles.styles.get(id) {
        Some(d) => Some(d.clone()),
        None => typed.style_xml(id).map(|x| StyleDef::from_xml(&x, &typed.styles.cx())),
    }
}

/// Whether `a` writes as `b` does in the styles part.
pub(super) fn written_alike(typed: &TypedDefs, a: &StyleDef, b: &StyleDef) -> bool {
    let (colours, cx) = (&typed.cascade.colours, typed.styles.cx());
    a.xml(colours, &cx) == b.xml(colours, &cx)
}

/// A style's on/off flag `p`.
pub(super) fn flag_mut(s: &mut Style, p: StyleProp) -> &mut Option<bool> {
    match p {
        StyleProp::Quick => &mut s.quick,
        StyleProp::SemiHidden => &mut s.meta.semi_hidden,
        StyleProp::Hidden => &mut s.meta.hidden,
        StyleProp::UnhideWhenUsed => &mut s.meta.unhide_when_used,
        StyleProp::Locked => &mut s.meta.locked,
        _ => &mut s.meta.auto_redefine,
    }
}

/// `s`'s flag `p` on (written alone), or not written; noted.
pub(super) fn set_flag(s: &mut Style, set: &mut Vec<StyleSet>, p: StyleProp, on: bool) {
    *flag_mut(s, p) = on.then_some(true);
    set.push(StyleSet::Style(p));
}

/// `s`'s names from `names` as a style's name is written: its name, then aliases, comma separated; noted.
pub(super) fn set_names(s: &mut Style, set: &mut Vec<StyleSet>, names: &str) {
    let mut names = names.split(',').map(str::trim);
    s.name = names.next().map(Rc::from);
    s.aliases = names.filter(|a| !a.is_empty()).map(Rc::from).collect();
    set.push(StyleSet::Style(StyleProp::Name));
}

/// The on/off paragraph properties, by bit, as their children are named.
const PARA_FLAGS: [&str; 17] = [
    "keepNext",
    "keepLines",
    "pageBreakBefore",
    "widowControl",
    "suppressLineNumbers",
    "suppressAutoHyphens",
    "kinsoku",
    "wordWrap",
    "overflowPunct",
    "topLinePunct",
    "autoSpaceDE",
    "autoSpaceDN",
    "bidi",
    "snapToGrid",
    "contextualSpacing",
    "mirrorIndents",
    "suppressOverlap",
];

/// `p` over `q` a child at a time, as a paragraph's direct formatting goes into a style: each child `p` writes in
/// place of `q`'s, with what `p` keeps beside it; `q`'s others as they are.
pub(super) fn para_over(p: &ParaProps, q: &ParaProps) -> ParaProps {
    let mut r = q.clone();
    let foreign = p.foreign.as_deref();
    let beside = |key: &str| {
        foreign.is_some_and(|f| {
            f.attributes.iter().any(|a| a.on.as_ref().is_some_and(|n| &*n.local == key)) || f.kept.iter().any(|c| &*c.name.local == key)
        })
    };
    let mut written: Vec<&str> = Vec::new();
    let mut take = |key: &'static str, sets: bool, copy: &mut dyn FnMut(&mut ParaProps)| {
        if sets || beside(key) {
            copy(&mut r);
            written.push(key);
        }
    };
    take("jc", p.align.is_some(), &mut |r| r.align = p.align);
    take("ind", p.indent_left.is_some() || p.indent_right.is_some() || p.indent_first.is_some(), &mut |r| {
        (r.indent_left, r.indent_right, r.indent_first) = (p.indent_left, p.indent_right, p.indent_first);
    });
    let spacing = [p.space_before, p.space_after, p.line].iter().any(Option::is_some)
        || p.space_before_auto.is_some()
        || p.space_after_auto.is_some()
        || p.line_rule.is_some();
    take("spacing", spacing, &mut |r| {
        (r.space_before, r.space_before_auto, r.space_after, r.space_after_auto, r.line, r.line_rule) =
            (p.space_before, p.space_before_auto, p.space_after, p.space_after_auto, p.line, p.line_rule);
    });
    for (bit, key) in PARA_FLAGS.iter().enumerate() {
        let f = crate::format::ParaFlag(1 << bit);
        take(key, p.flags.get(f).is_some(), &mut |r| match p.flags.get(f) {
            Some(on) => r.flags.put(f, on),
            None => r.flags.clear(f),
        });
    }
    take("textboxTightWrap", p.tight_wrap.is_some(), &mut |r| r.tight_wrap = p.tight_wrap);
    take("divId", p.division.is_some(), &mut |r| r.division = p.division);
    take("tabs", !p.tabs.is_empty(), &mut |r| r.tabs = p.tabs.clone());
    take("pBdr", !p.borders.is_empty(), &mut |r| r.borders = p.borders);
    take("shd", p.shading.is_some(), &mut |r| r.shading = p.shading);
    take("framePr", p.frame.is_some() || p.frame_locked.is_some(), &mut |r| (r.frame, r.frame_locked) = (p.frame.clone(), p.frame_locked));
    take("textDirection", p.text_flow.is_some(), &mut |r| r.text_flow = p.text_flow);
    take("textAlignment", p.text_alignment.is_some(), &mut |r| r.text_alignment = p.text_alignment);
    take("outlineLvl", p.outline_level.is_some(), &mut |r| r.outline_level = p.outline_level);
    take("collapsed", p.collapsed.is_some(), &mut |r| r.collapsed = p.collapsed);
    // What each keeps beside its children: the written ones' `p`'s
    let Some(theirs) = foreign else { return r };
    let mut f = q.foreign.as_deref().cloned().unwrap_or_default();
    let of = |n: &crate::format::QName| written.contains(&&*n.local);
    f.attributes.retain(|a| !a.on.as_ref().is_some_and(of));
    f.attributes.extend(theirs.attributes.iter().filter(|a| a.on.as_ref().is_some_and(of)).cloned());
    f.kept.retain(|c| !of(&c.name));
    f.kept.extend(theirs.kept.iter().filter(|c| of(&c.name)).cloned());
    for c in &theirs.children {
        match f.children.iter_mut().find(|x| x.name == c.name) {
            Some(x) => *x = c.clone(),
            None => f.children.push(c.clone()),
        }
    }
    r.foreign = (f != crate::format::ForeignProps::default()).then(|| Rc::new(f));
    r
}

/// A paragraph's and a run's authored properties (`ppr`, `rpr`) as a style takes them: without what isn't formatting
/// (the style, list, section break, tracked changes); none for those that set nothing then.
pub(super) fn selection_props(ppr: Option<&ParaProps>, rpr: Option<&CharProps>) -> (Option<ParaProps>, Option<CharProps>) {
    let ppr = ppr.map(|p| {
        let mut q = p.clone();
        (q.style, q.list, q.list_level) = (None, None, None);
        if let Some(f) = q.foreign.as_mut() {
            let f = Rc::make_mut(f);
            let not_ours = |n: &crate::format::QName| &*n.ns == crate::docx::xml::W && matches!(&*n.local, "pStyle" | "numPr");
            f.attributes.retain(|a| !a.on.as_ref().is_some_and(not_ours));
            f.kept.retain(|c| !not_ours(&c.name));
            if *f == crate::format::ForeignProps::default() {
                q.foreign = None;
            }
        }
        q
    });
    let rpr = rpr.map(|r| CharProps { style: None, ..r.clone() });
    (ppr.filter(|q| *q != ParaProps::default()), rpr.filter(|q| *q != CharProps::default()))
}

/// `was` with the formatting `ppr` and `rpr` set (a character style's paragraph formatting aside): each property
/// element child they write in place of its own.
pub(super) fn updated_to(was: &StyleDef, ppr: Option<ParaProps>, rpr: Option<CharProps>, character: bool) -> StyleDef {
    edit_style(was, |s, _| {
        if let Some(p) = ppr.filter(|_| !character) {
            s.format.para = para_over(&p, &s.format.para);
        }
        if let Some(r) = rpr {
            let own = s.format.char.style.clone();
            s.format.char = crate::docx::char_props::over(&r, &s.format.char);
            s.format.char.style = own;
        }
    })
}

/// List Bullet `s` in its own bullet list `num`, its paragraphs without space between them.
pub(super) fn bulleted(s: &mut Style, num: i64) {
    let p = &mut s.format.para;
    p.list = Some(crate::format::ListChoice::In(crate::document::paragraph::ListId(num)));
    p.flags.put(crate::format::ParaFlag::CONTEXTUAL, true);
}

/// `was` with the list flags `m` sets.
pub(super) fn meta_edited(was: &StyleDef, m: &crate::api::command::StyleMetaChange) -> StyleDef {
    use crate::api::services::StyleVisibility;
    edit_style(was, |s, set| {
        if let Some(v) = m.visibility {
            let (semi, unhide) = match v {
                StyleVisibility::Show => (false, false),
                StyleVisibility::HideUntilUsed => (true, true),
                StyleVisibility::Hide => (true, false),
            };
            set_flag(s, set, StyleProp::SemiHidden, semi);
            set_flag(s, set, StyleProp::UnhideWhenUsed, unhide);
        }
        if let Some(p) = m.priority {
            s.priority = Some(p);
            set.push(StyleSet::Style(StyleProp::Priority));
        }
        if let Some(q) = m.quick {
            set_flag(s, set, StyleProp::Quick, q);
        }
        if let Some(l) = m.locked {
            set_flag(s, set, StyleProp::Locked, l);
        }
    })
}

/// The document defaults `was` (none: none written) with a formatting of their stand-in style, `t`: a level read
/// empty stays (none means the application's defaults). `None` when they write as they did.
pub(super) fn defaults_edited(typed: &TypedDefs, was: Option<&DefaultsDef>, t: &StyleTarget) -> Option<DefaultsDef> {
    let d = was.map(|d| (**d.get()).clone()).unwrap_or_default();
    let (para, char) = (t.style().format.para.clone(), t.style().format.char.clone());
    let now = crate::format::Defaults {
        char: (d.char.is_some() || char != CharProps::default()).then_some(char),
        para: (d.para.is_some() || para != ParaProps::default()).then_some(para),
        foreign: d.foreign.clone(),
    };
    let set: Vec<DefaultsSet> = t
        .set()
        .iter()
        .filter_map(|s| match *s {
            StyleSet::Char(None, x) => Some(DefaultsSet::Char(x)),
            StyleSet::Para(None, x) => Some(DefaultsSet::Para(x)),
            _ => None,
        })
        .collect();
    let (colours, cx) = (&typed.cascade.colours, typed.styles.cx());
    match was {
        Some(w) => {
            let def = w.edited(now, &set);
            (def.xml(colours, &cx) != w.xml(colours, &cx)).then_some(def)
        }
        None => (now.char.is_some() || now.para.is_some()).then(|| DefaultsDef::typed(Rc::new(now))),
    }
}

impl Editor {
    /// Runs formatting commands on a style instead of the selection, and returns it after them. Only character and
    /// paragraph formatting is taken; the document doesn't change.
    pub fn format_style(&mut self, style: &StyleDef, commands: Vec<Command>, fonts: &Fonts) -> Result<StyleDef, Error> {
        self.format_target(StyleTarget::of(style.clone()), commands, fonts).map(|t| t.def())
    }

    /// [`Self::format_style`] on a style's XML, its XML after.
    #[cfg(test)]
    pub(crate) fn format_style_xml(&mut self, raw: &str, commands: Vec<Command>, fonts: &Fonts) -> Result<String, Error> {
        formats_style(&commands)?;
        let def = self.style_of_xml(raw).ok_or_else(|| Error::invalid("not a style"))?;
        self.format_style(&def, commands, fonts).map(|d| self.style_xml_of(&d))
    }

    /// [`Self::format_style`] on a style being formatted already, with what its formatting set.
    pub(super) fn format_target(&mut self, target: StyleTarget, commands: Vec<Command>, fonts: &Fonts) -> Result<StyleTarget, Error> {
        formats_style(&commands)?;
        self.style_target = Some(target);
        for c in commands {
            if let Outcome::Refused(e) = self.execute(c, fonts) {
                self.style_target = None;
                return Err(e);
            }
        }
        self.style_target.take().ok_or_else(|| Error::invalid("not a style"))
    }

    /// The target style as text in it resolves: a paragraph in it (or, for a character style, in the default
    /// paragraph style) and a run of that paragraph, from the definitions with the target among them.
    fn style_target_formatting(&self) -> Option<(Paragraph, crate::docx::Run, Cx)> {
        self.style_text(&self.style_target.as_ref()?.def())
    }

    /// The style a `w:style` holds, as the API carries a style being edited, its XML beside it: `None` when it isn't
    /// one, or would be based on itself.
    pub(crate) fn style_of_xml(&self, raw: &str) -> Option<StyleDef> {
        let s = self.doc.resolver.as_ref()?.typed().styles.style_from_xml(raw)?;
        Some(StyleDef::read(s, raw.into()))
    }

    /// A style's `w:style`, as the API carries a style being edited.
    pub(crate) fn style_xml_of(&self, def: &StyleDef) -> String {
        match self.doc.resolver.as_ref() {
            Some(r) => def.xml(&r.colours(), &r.cx()).into_owned(),
            None => String::new(),
        }
    }

    /// [`Self::style_text`] for a style's XML, as the API carries a style being edited.
    pub(super) fn style_formatting(&self, raw: &str) -> Option<(Paragraph, crate::docx::Run, Cx)> {
        self.style_text(&self.style_of_xml(raw)?)
    }

    /// A style as text in it resolves: a paragraph in it (or, for a character style, in the default paragraph style)
    /// and a run of that paragraph, from the definitions with it among them.
    pub(super) fn style_text(&self, def: &StyleDef) -> Option<(Paragraph, crate::docx::Run, Cx)> {
        let res = self.doc.resolver.as_ref()?;
        let mut next = (**res).clone();
        // The defaults formatted as a style: alone, not over themselves.
        if self.defaults_target {
            next.without_defaults();
        }
        next.set_style(def.clone()).ok()?;
        let id = def.get().id.clone();
        let kind = next.style_kind(&id)?;
        let cx = next.typed().styles.cx();
        let (ppr, direct) = match kind {
            crate::format::StyleKind::Character => {
                (None, RunDirect::typed(Some(Rc::new(CharProps { style: Some(id.clone()), ..Default::default() }))))
            }
            crate::format::StyleKind::Table => (None, RunDirect::default()),
            _ => (Some(&*id), RunDirect::default()),
        };
        // A table style's text: in the part being formatted, else the whole table.
        let parts: Vec<RegionKind> = self.style_part.into_iter().collect();
        let region = (kind == crate::format::StyleKind::Table).then(|| next.cascade().table_region(Some(&id), &parts));
        let ppr = crate::format::ParaProps { style: ppr.map(Into::into), ..Default::default() };
        let para = crate::docx::para_xml::resolve(&next, &ppr, None, region.as_ref());
        let run = Rc::new(run_xml::resolve(&next, &para.base, direct, false));
        let p = Paragraph {
            style: Rc::new(para.style),
            mark: run.clone(),
            list: None,
            inlines: Vec::new(),
            origin: crate::docx::Origin::NONE,
            id: 0,
            div: None,
            controls: Vec::new(),
        };
        Some((p, run, cx))
    }

    /// Runs `edit` on the target style, when its text resolves.
    fn format_target_with(&mut self, edit: impl FnOnce(&mut Style, &mut Vec<StyleSet>, &Paragraph, &RunStyle)) -> Outcome {
        let Some((para, run, _)) = self.style_target_formatting() else { return Outcome::Unchanged("no style") };
        let Some(t) = self.style_target.as_mut() else { return Outcome::Unchanged("no style") };
        t.edit(|s, set| edit(s, set, &para, &run));
        Outcome::Moved
    }

    /// Runs a character formatting command's typed edit on the target style's character formatting: the style's own,
    /// or its table part's (`style_part`).
    pub(super) fn format_style_rpr(&mut self, edit: &dyn Fn(&mut CharEdit, &RunStyle)) -> Outcome {
        let region = self.style_part;
        self.format_target_with(|s, set, _, run| edit_chars(s, set, region, |e| edit(e, run)))
    }

    /// Runs a paragraph formatting command's typed edit on the target style's paragraph formatting: the style's own,
    /// or its table part's (`style_part`).
    pub(super) fn format_style_ppr(&mut self, edit: &dyn Fn(&mut ParaEdit, &Paragraph)) -> Outcome {
        let region = self.style_part;
        self.format_target_with(|s, set, para, _| edit_paras(s, set, region, |e| edit(e, para)))
    }

    /// Paragraph formatting as a typed edit on paragraphs in an automatically updating style: the style changes
    /// instead, as one undo step.
    pub(super) fn format_auto_redefined_typed(&mut self, id: &str, edit: &dyn Fn(&mut ParaEdit, &Paragraph), fonts: &Fonts) -> Outcome {
        let Some(typed) = self.doc.resolver.as_ref().map(|r| r.typed()) else { return Outcome::Unchanged("no style") };
        let Some(was) = style_holder(&typed, id) else { return Outcome::Unchanged("no style") };
        self.style_target = Some(StyleTarget::of(was.clone()));
        let _ = self.format_style_ppr(edit);
        let Some(now) = self.style_target.take().map(|t| t.def()) else { return Outcome::Unchanged("no style") };
        // The formatting it has already, as for paragraphs formatted directly: no undo step.
        if written_alike(&typed, &now, &was) || self.style_text(&now).zip(self.style_text(&was)).is_some_and(|(a, b)| a.0.style.layout_eq(&b.0.style))
        {
            return Outcome::Unchanged("the style already has it");
        }
        self.edit_styles("Modify Style", |s| s.set(now), fonts)
    }

    /// The target style's tab stops, and those it inherits from its base, when commands edit a style: the one
    /// formatting edits, or the one the selected paragraphs are in when it updates automatically.
    pub(super) fn style_tabs(&mut self) -> Option<(Vec<Tab>, Vec<Tab>)> {
        let def = match &self.style_target {
            Some(t) => t.def(),
            None => {
                let id = self.auto_redefined()?;
                style_holder(&self.doc.resolver.as_ref()?.typed(), &id)?
            }
        };
        let (para, _, _) = self.style_text(&def)?;
        let res = self.doc.resolver.as_ref()?;
        let typed = res.typed();
        // Its base while it is the document's style as it is
        let base = typed
            .cascade
            .styles
            .styles
            .get(&def.get().id)
            .filter(|e| style_holder(&typed, &e.style.id).is_some_and(|d| written_alike(&typed, &d, &def)))
            .filter(|e| !e.style.word_default_table())
            .and_then(|e| e.style.based_on.clone());
        let inherited = crate::docx::para_xml::resolve(res, &crate::format::ParaProps { style: base, ..Default::default() }, None, None)
            .style
            .tabs
            .iter()
            .filter(|t| t.kind != TabKind::Clear)
            .copied()
            .collect();
        Some((para.style.tabs.clone(), inherited))
    }

    /// The selection's direct formatting, as a style takes it: its paragraph's and its first run's authored
    /// properties, without what isn't formatting (the style, list, section break, tracked changes).
    pub(super) fn selection_formatting(&self) -> (Option<ParaProps>, Option<CharProps>) {
        if self.doc.resolver.is_none() {
            return (None, None);
        }
        let para = self.selection.focus().and_then(|p| p.para_offset()).and_then(|(p, _)| self.index.para(&self.doc, p));
        let run = self.caret().and_then(|c| self.run_at(&c));
        selection_props(para.map(|p| &**p.style.direct.get()), run.as_ref().and_then(|r| r.direct.props()).map(|r| &**r))
    }

    /// The built-in style `cmd` applies (or updates) when the document doesn't define it, and it is of the kind `cmd`
    /// takes.
    pub(super) fn built_in_named(&self, cmd: &Command) -> Option<&'static crate::services::styles::builtin::BuiltIn> {
        let (id, kind) = match cmd {
            Command::SetParagraphStyle(id) => (id, Some(StyleKind::Paragraph)),
            Command::ApplyCharacterStyle(Some(id)) => (id, Some(StyleKind::Character)),
            Command::SetTableStyle(id) => (id, Some(StyleKind::Table)),
            Command::UpdateStyleToSelection(id) => (id, None),
            _ => return None,
        };
        if self.doc.resolver.as_ref()?.has_style(&id.0) {
            return None;
        }
        crate::services::styles::builtin::BUILT_IN.iter().find(|b| b.id == id.0 && kind.is_none_or(|k| k == b.kind))
    }

    /// Whether `cmd` names a built-in style the document will take on its first use.
    pub fn built_in_for(&self, cmd: &Command) -> bool {
        self.built_in_named(cmd).is_some()
    }

    /// Adds built-in style `b` from its definition, as one undo step.
    pub(super) fn add_built_in(&mut self, b: &crate::services::styles::builtin::BuiltIn, fonts: &Fonts) -> Outcome {
        if self.doc.resolver.is_none() {
            return Outcome::Unchanged("no styles");
        }
        let mut style = crate::services::styles::builtin::definition(b);
        // List Bullet comes with its own bullet list.
        if b.id == "ListBullet" {
            let mut tx = self.begin("New Style");
            let num = self.new_style_bullets(&mut tx, b.id);
            if num == 0 {
                self.abort(tx);
                return Outcome::Unchanged("the list couldn't be added");
            }
            self.commit(tx, None, fonts);
            bulleted(&mut style, num);
        }
        let def = StyleDef::typed(Rc::new(style));
        self.edit_styles("New Style", |s| s.set(def), fonts)
    }

    /// Changes a style to the selection's formatting.
    pub(super) fn update_style_to_selection(&mut self, id: &str, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let typed = res.typed();
        let Some(was) = style_holder(&typed, id) else { return Outcome::Refused(Error::unavailable("no such style")) };
        let character = res.style_kind(id) == Some(crate::format::StyleKind::Character);
        let (ppr, rpr) = self.selection_formatting();
        let def = updated_to(&was, ppr, rpr, character);
        if written_alike(&typed, &def, &was) {
            return Outcome::Unchanged("the style already matches");
        }
        self.edit_styles("Update Style", |s| s.set(def), fonts)
    }

    /// Sets the selected paragraphs' border lines.
    pub(super) fn set_paragraph_border_lines(&mut self, b: crate::api::command::ParagraphBorderLines, fonts: &Fonts) -> Outcome {
        let shadow = b.shadow.unwrap_or(false);
        let line = |v: Option<Option<crate::api::command::Border>>, far: bool| {
            v.map(|v| v.map(|v| super::readback::BorderLine::of_api(&v, 1.0, shadow && far)))
        };
        let edges = super::readback::ParagraphBorderEdges {
            top: line(b.top, false),
            left: line(b.left, false),
            bottom: line(b.bottom, true),
            right: line(b.right, true),
            between: line(b.between, false),
        };
        self.set_paragraph_border_edges(&edges, fonts)
    }

    /// Sets the selected paragraphs' border edges, each set, removed or kept: its line, colour (RGB or a theme colour),
    /// distance from the text and shadow.
    pub fn set_paragraph_border_edges(&mut self, b: &super::readback::ParagraphBorderEdges, fonts: &Fonts) -> Outcome {
        let edges = [("top", &b.top), ("left", &b.left), ("bottom", &b.bottom), ("right", &b.right), ("between", &b.between)];
        let mut colours = [None; 5];
        for (k, (_, e)) in edges.iter().enumerate() {
            if let Some(Some(line)) = e {
                match super::readback::checked(self, line) {
                    Ok(c) => colours[k] = c,
                    Err(why) => return Outcome::invalid(&why),
                }
            }
        }
        use crate::format::{BorderSide, Color, ParaProp, TextBorder};
        let sides = [BorderSide::Top, BorderSide::Left, BorderSide::Bottom, BorderSide::Right, BorderSide::Between];
        self.format_paras_typed(
            "Borders",
            &|e, _| {
                for (k, (_, v)) in edges.iter().enumerate() {
                    let Some(v) = v else { continue };
                    *e.borders.side_mut(sides[k]) = v.as_ref().map(|line| TextBorder {
                        style: super::readback::border_style(line.style),
                        width: (line.width.0 * 8.0).round() / 8.0,
                        space: line.space.0.round(),
                        shadow: line.shadow,
                        color: Some(colours[k].map_or(Color::Auto, super::format::colour)),
                    });
                    e.part(ParaProp::Border(sides[k]));
                }
            },
            fonts,
        )
    }

    /// Sets or removes the selected text's border (`w:bdr`).
    pub(super) fn set_text_border(&mut self, b: crate::api::command::TextBorderChange, fonts: &Fonts) -> Outcome {
        let line = b.border.map(|l| super::readback::BorderLine::of_api(&l, 0.0, b.shadow));
        self.set_text_border_line(line.as_ref(), fonts)
    }

    /// Sets the selected text's border: one box around it, its line, colour (RGB or a theme colour), distance from the
    /// text and shadow; none removes it.
    pub fn set_text_border_line(&mut self, b: Option<&super::readback::BorderLine>, fonts: &Fonts) -> Outcome {
        let colour = match b.map(|b| super::readback::checked(self, b)).transpose() {
            Ok(c) => c.flatten(),
            Err(why) => return Outcome::invalid(&why),
        };
        // Whole eighths of a point wide, whole points from the text
        let border = b.map(|b| crate::format::TextBorder {
            style: super::readback::border_style(b.style),
            width: (b.width.0 * 8.0).round() / 8.0,
            space: b.space.0.round(),
            shadow: b.shadow,
            color: Some(colour.map_or(crate::format::Color::Auto, super::format::colour)),
        });
        self.format_runs_typed(
            "Borders",
            &|e, _| {
                e.border = border;
                e.whole(Prop::Border);
            },
            fonts,
        )
    }

    /// Sets the shading of the selected paragraphs, text, cells or tables.
    pub(super) fn set_shading(&mut self, s: crate::api::command::ShadingChange, fonts: &Fonts) -> Outcome {
        let v = super::readback::ShadingValue { fill: s.fill, pattern: s.pattern, color: s.color };
        self.set_shading_value(s.target, &v, fonts)
    }

    /// Sets the shading of the selected paragraphs, text, cells or tables: its fill, pattern and pattern colour, each
    /// an RGB or a theme colour with a tint or shade.
    pub fn set_shading_value(&mut self, target: crate::api::command::ShadingTarget, s: &super::readback::ShadingValue, fonts: &Fonts) -> Outcome {
        use super::table_format::{Shading, TableFormat};
        use crate::api::command::{ShadingPattern, ShadingTarget};
        use crate::api::value::{AutoOr, NoneOr};
        // The command's gates: a call without it is refused alike
        let gate = Command::SetShading(crate::api::command::ShadingChange {
            target,
            fill: NoneOr::None,
            pattern: ShadingPattern::Clear,
            color: AutoOr::Auto,
        });
        if let Some(out) = self.refuse_like(&gate) {
            return out;
        }
        let colour = |c: &crate::api::drawing::ColourValue| self.word_colour(c);
        let fill = match &s.fill {
            NoneOr::Value(c) => match colour(c) {
                Ok(c) => Some(c),
                Err(why) => return Outcome::invalid(&why),
            },
            NoneOr::None => None,
        };
        let color = match &s.color {
            AutoOr::Auto => None,
            AutoOr::Value(c) => match colour(c) {
                Ok(c) => Some(c),
                Err(why) => return Outcome::invalid(&why),
            },
        };
        let sh = Shading { pattern: s.pattern.val(), color, fill };
        // No fill and a clear pattern: no shading of their own.
        let none = fill.is_none() && s.pattern == ShadingPattern::Clear;
        // A table style's shading goes in its cells' or its table's properties; no other style has them, and the
        // document's tables stay as they are.
        if let Some(t) = &self.style_target
            && matches!(target, ShadingTarget::Cell | ShadingTarget::Table)
        {
            if self.doc.resolver.is_none() || t.style().kind != crate::format::StyleKind::Table {
                return Outcome::Refused(Error::invalid("only a table style has cell or table shading"));
            }
            let shading = |was: Option<crate::format::Shading>| {
                (!none).then(|| super::table_format::authored_shading(&sh.shd_over(was.map(|s| s.pattern), None)))
            };
            let region = self.style_part;
            return self.format_target_with(|s, set, _, _| match target {
                ShadingTarget::Cell => edit_cells(s, set, region, |e| {
                    e.shading = shading(e.shading);
                    e.whole(crate::format::CellProp::Shading);
                }),
                _ => edit_tables(s, set, region, |e| {
                    e.shading = shading(e.shading);
                    e.whole(crate::format::TableProp::Shading);
                }),
            });
        }
        match target {
            ShadingTarget::Paragraph => self.format_paras_typed(
                "Shading",
                &|e, para| {
                    use crate::format::{Color, ParaProp};
                    e.shading = (!none).then(|| {
                        let inherited = para.style.shd.as_ref().map(|s| s.pattern).filter(|p| *p != crate::docx::props::ShadingPattern::Nil);
                        let pattern = sh.pattern.map(crate::docx::line_style::shading_pattern).or(e.shading.map(|s| s.pattern)).or(inherited);
                        let c = |c: Option<crate::editing::theme::WordColour>| Some(c.map_or(Color::Auto, super::format::colour));
                        crate::format::Shading {
                            pattern: pattern.unwrap_or(crate::docx::props::ShadingPattern::Clear),
                            color: c(sh.color),
                            fill: c(sh.fill),
                        }
                    });
                    e.whole(ParaProp::Shading);
                },
                fonts,
            ),
            ShadingTarget::Text => self.format_runs_typed("Shading", &|e, run| text_shading(e, &sh, none, run), fonts),
            ShadingTarget::Cell => self.table_format(&TableFormat::CellShading(sh.clone()), fonts),
            ShadingTarget::Table => self.table_format(&TableFormat::TableShading(sh.clone()), fonts),
        }
    }

    /// Sets the selected cells' fill (clear pattern), or removes their shading: an RGB or a theme colour with a tint
    /// or shade.
    pub fn set_cell_shading(&mut self, c: Option<crate::api::drawing::ColourValue>, fonts: &Fonts) -> Outcome {
        if let Some(out) = self.refuse_like(&Command::SetCellShading(None)) {
            return out;
        }
        let fill = match c.map(|c| self.word_colour(&c)).transpose() {
            Ok(f) => f,
            Err(why) => return Outcome::invalid(&why),
        };
        self.table_format(&super::table_format::TableFormat::CellShading(super::table_format::Shading::fill(fill)), fonts)
    }

    /// Deletes a style the document made.
    pub(super) fn delete_style(&mut self, id: &str, fonts: &Fonts) -> Outcome {
        let Some(c) = self.doc.resolver.as_ref().map(|r| r.cascade()) else { return Outcome::Refused(Error::unavailable("no such style")) };
        let Some(style) = c.styles.get(id) else { return Outcome::Refused(Error::unavailable("no such style")) };
        if !style.custom.unwrap_or(false) || c.styles.default_paragraph.as_deref() == Some(id) {
            return Outcome::Refused(Error::new(crate::api::error::ErrorCode::NotAllowed, "built-in styles can't be deleted"));
        }
        self.edit_styles("Delete Style", |s| s.delete(id), fonts)
    }

    /// The paragraphs, marks, runs (a ruby's too) and tables whose direct formatting names style `id`, with the name
    /// taken out, as stale.
    fn drop_style_refs(&mut self, tx: &mut super::Tx, id: &str, res: &Resolvers) {
        use crate::docx::Inline;
        let cx = self.cx();
        let named = |r: &crate::docx::Run| r.style_id.as_deref() == Some(id);
        // The properties without the character style when they name it
        let unstyle = |e: &mut CharEdit| {
            let ours = e.style.as_deref() == Some(id);
            if ours {
                e.style = None;
                e.whole(Prop::Style);
            }
            ours
        };
        let unstyled = |r: &crate::docx::Run| -> Option<crate::docx::Run> {
            let mut e = CharEdit::new((**r.direct.props()?).clone());
            unstyle(&mut e).then(|| Rc::new(RunStyle { direct: r.direct.edit(e), ..(**r).clone() }))
        };
        let in_ruby = |i: &Inline| matches!(i, Inline::Ruby(r, _) if r.base.iter().chain(&r.text).any(|x| named(&x.1)));
        let ruby_rpr = super::format::ruby_edit(|e, _| unstyle(e));
        let paras: Vec<_> = self.index.para_ids().collect();
        for para in paras {
            let Some(p) = self.index.para(&self.doc, para) else { continue };
            let in_style = p.style.style_id.as_deref() == Some(id);
            if !in_style && !named(&p.mark) && !p.inlines.iter().any(|i| i.run().is_some_and(named) || in_ruby(i)) {
                continue;
            }
            let d = &p.style.direct;
            let style = (in_style && d.get().style.as_deref() == Some(id)).then(|| {
                let mut e = ParaEdit::new((**d.get()).clone());
                e.style = None;
                e.whole(crate::format::ParaProp::Style);
                Rc::new(crate::docx::ParaStyle { direct: d.edit(e), ..(*p.style).clone() })
            });
            let mark = if named(&p.mark) { unstyled(&p.mark) } else { None };
            let mut inlines: Option<Vec<Inline>> = None;
            let mut at = 0;
            for (k, i) in p.inlines.iter().enumerate() {
                let unit = crate::layout::Unit(at);
                at += i.units() as u32;
                let run = i.run().filter(|r| named(r)).and_then(unstyled);
                let mut edited = run.is_some();
                let mut with = run.map_or_else(|| i.clone(), |r| super::format::with_run(i, &r));
                if in_ruby(i)
                    && let Inline::Ruby(r, _) = &mut with
                    && let Some(base) = self.run_base(para, unit, res, &cx)
                    && let Some(n) = super::format::ruby_again(r, &base, Some((&ruby_rpr, true)), res, &cx)
                {
                    (*r, edited) = (n, true);
                }
                if edited {
                    inlines.get_or_insert_with(|| p.inlines.clone())[k] = with;
                }
            }
            if style.is_some() || mark.is_some() {
                let (style, mark, list) = (style.unwrap_or_else(|| p.style.clone()), mark.unwrap_or_else(|| p.mark.clone()), p.list.clone());
                let _ = self.step(tx, Step::Para { para, style, mark, list });
            }
            if let Some(inlines) = inlines {
                let _ = self.step(tx, Step::Restyle { para, inlines });
            }
            tx.stale_paras.push(para);
        }
        let tables: Vec<_> = self.index.table_ids().collect();
        for table in tables {
            let Some(t) = self.index.table(&self.doc, table).filter(|t| t.style_id.as_deref() == Some(id)) else { continue };
            if t.direct.get().style.as_deref() == Some(id) {
                let mut props = super::step::TableProps::of(t);
                super::table_format::edit_table(&mut props, |e| {
                    e.style = None;
                    e.whole(crate::format::TableProp::Style);
                });
                let _ = self.step(tx, Step::TableProps { table, props });
            }
            tx.stale_tables.push(table);
        }
    }

    /// Sets a style's list flags.
    pub(super) fn set_style_meta(&mut self, m: crate::api::command::StyleMetaChange, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let typed = res.typed();
        let Some(was) = style_holder(&typed, &m.id.0) else { return self.set_latent_meta(&m, fonts) };
        let def = meta_edited(&was, &m);
        if written_alike(&typed, &def, &was) {
            return Outcome::Unchanged("already so");
        }
        self.edit_styles("Style Options", |s| s.set(def), fonts)
    }

    /// The document defaults as a paragraph style: their paragraph and character properties its own.
    fn defaults_stand_in(&self) -> Option<Style> {
        let typed = self.doc.resolver.as_ref()?.typed();
        let d = typed.styles.defaults.as_ref().map(|d| (**d.get()).clone()).unwrap_or_default();
        let mut s = defaults_style();
        (s.format.para, s.format.char) = (d.para.unwrap_or_default(), d.char.unwrap_or_default());
        Some(s)
    }

    /// The document's default formatting, without a dialog: text before any style's (resolved), and what the
    /// defaults set themselves (the rest unavailable: the application's apply).
    pub fn document_defaults(&self) -> Option<super::style_defs::DefaultsDefinition> {
        let own = self.defaults_stand_in()?;
        let colours = self.doc.resolver.as_ref()?.colours();
        // Text in a style that sets nothing: the defaults alone.
        let (character, paragraph, tabs) = self.formatting_state_of(&StyleDef::typed(Rc::new(defaults_style())))?;
        Some(super::style_defs::DefaultsDefinition {
            own_character: super::style_defs::own_character(&crate::docx::char_props::to_run_props(&own.format.char, &colours), &character),
            own_paragraph: super::style_defs::own_paragraph(&crate::docx::para_props::to_para_props(&own.format.para, &colours), &paragraph),
            character,
            paragraph,
            tabs,
        })
    }

    /// Sets the document's default formatting: the formatting commands run on the defaults as if they were a style.
    pub(super) fn set_document_defaults(&mut self, d: crate::api::command::DocumentDefaults, fonts: &Fonts) -> Outcome {
        let mut commands = Vec::new();
        if let Some(f) = d.font.filter(|f| !matches!(f, crate::api::command::FontChoice::Name(n) if n.trim().is_empty())) {
            commands.push(Command::SetFont(f));
        }
        if let Some(s) = d.size {
            commands.push(Command::SetFontSize(s));
        }
        if let Some(c) = d.color {
            commands.push(Command::SetFontColor(c));
        }
        if let Some(s) = d.spacing {
            commands.push(Command::SetParagraphSpacing(s));
        }
        if let Some(l) = d.line_spacing {
            commands.push(Command::SetLineSpacing(l));
        }
        if let Some(i) = d.indents {
            commands.push(Command::SetIndents(i));
        }
        self.format_defaults(commands, fonts)
    }

    /// Sets the document's default formatting with any character and paragraph formatting commands but frames, run
    /// on the defaults as on a style, as one undo step. Every style's text lays out again.
    pub fn set_default_formatting(&mut self, commands: Vec<Command>, fonts: &Fonts) -> Outcome {
        use crate::api::command::CommandId as C;
        if let Some(c) = commands.iter().find(|c| matches!(c.id(), C::SetFrame | C::RemoveFrame)) {
            return Outcome::invalid(&format!("{:?} doesn't set a default", c.id()));
        }
        self.format_defaults(commands, fonts)
    }

    /// Formats the defaults with `commands`, as one undo step.
    fn format_defaults(&mut self, commands: Vec<Command>, fonts: &Fonts) -> Outcome {
        if let Some(out) = self.refuse_like(&Command::SetDocumentDefaults(Default::default())) {
            return out;
        }
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let Some(stand_in) = self.defaults_stand_in() else { return Outcome::Unchanged("no styles") };
        self.defaults_target = true;
        let edited = self.format_target(StyleTarget::of(StyleDef::typed(Rc::new(stand_in))), commands, fonts);
        self.defaults_target = false;
        let edited = match edited {
            Ok(t) => t,
            Err(e) => return Outcome::Refused(e),
        };
        let Some(def) = defaults_edited(&res.typed(), res.defaults_def().as_ref(), &edited) else { return Outcome::Unchanged("already so") };
        self.edit_styles("Set Defaults", |s| s.set_defaults(def), fonts)
    }

    /// The paragraph style every selected paragraph is in, when it updates automatically (`w:autoRedefine`).
    pub(super) fn auto_redefined(&mut self) -> Option<String> {
        let res = self.doc.resolver.as_ref()?.cascade();
        let paras = self.selected_paras();
        let mut ids = paras.iter().map(|&p| {
            self.index
                .para(&self.doc, p)
                .and_then(|q| q.style.style_id.clone())
                .or_else(|| res.styles.default_paragraph.as_deref().map(str::to_string))
        });
        let first = ids.next()??;
        if !ids.all(|i| i.as_deref() == Some(first.as_str())) {
            return None;
        }
        res.styles.get(&first).filter(|s| s.meta.auto_redefine.unwrap_or(false)).map(|s| s.id.to_string())
    }

    /// The paragraphs in one of the styles `changed`, or with a run in one: its mark, its list label, a ruby's text
    /// too.
    pub(super) fn styled_paras(&self, changed: &[String]) -> Vec<crate::layout::ParaId> {
        let default = self.doc.resolver.as_ref().and_then(|r| r.cascade().styles.default_paragraph.clone());
        let changed: std::collections::HashSet<&str> = changed.iter().map(String::as_str).collect();
        let uses = |id: Option<&str>| id.or(default.as_deref()).is_some_and(|id| changed.contains(id));
        let styled = |r: &crate::docx::Run| r.style_id.as_deref().is_some_and(|s| changed.contains(s));
        self.index
            .para_ids()
            .filter(|&p| {
                self.index.para(&self.doc, p).is_some_and(|q| {
                    uses(q.style.style_id.as_deref())
                        || styled(&q.mark)
                        || q.list.as_ref().is_some_and(|l| styled(&l.run))
                        || q.inlines.iter().any(|i| {
                            i.run().is_some_and(styled)
                                || matches!(i, crate::docx::Inline::Ruby(r, _) if r.base.iter().chain(&r.text).any(|x| styled(&x.1)))
                        })
                })
            })
            .collect()
    }

    /// Runs `edit` on the styles table as one undo step named `label`. Every paragraph, run and table whose style
    /// changed (the edit's answer) is formatted again from the new table; those naming a style it deleted name it no
    /// more.
    pub(crate) fn edit_styles(
        &mut self,
        label: &'static str,
        edit: impl FnOnce(&mut crate::docx::resolvers::StyleEdits) -> Result<Vec<String>, StyleEditError>,
        fonts: &Fonts,
    ) -> Outcome {
        self.edit_styles_relabel(label, edit, false, fonts)
    }

    /// [`Self::edit_styles`], and with `relabel` every list paragraph is numbered again: an edit that changes the
    /// lists styles number with.
    pub(crate) fn edit_styles_relabel(
        &mut self,
        label: &'static str,
        edit: impl FnOnce(&mut crate::docx::resolvers::StyleEdits) -> Result<Vec<String>, StyleEditError>,
        relabel: bool,
        fonts: &Fonts,
    ) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let mut next = (*res).clone();
        let changed = match edit(&mut next.style_edits()) {
            Ok(c) => c,
            Err(StyleEditError::Missing) => return Outcome::Refused(Error::unavailable("no such style")),
            Err(StyleEditError::Loop) => return Outcome::Refused(Error::invalid("a style can't be based on itself")),
            Err(StyleEditError::Malformed) => return Outcome::Refused(Error::invalid("not a style")),
        };
        let mut tx = self.begin(label);
        let added = self.doc.added_lists.clone();
        if self.step(&mut tx, Step::Resolver { resolver: Some(Rc::new(next)), added }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("the styles didn't change");
        }
        // The styles it deleted: the document's references to them go, untracked
        let before = res.cascade();
        let gone: Vec<&str> =
            before.styles.styles.keys().filter(|id| !self.doc.resolver.as_ref().is_some_and(|r| r.has_style(id))).map(|id| &**id).collect();
        let tracked = std::mem::replace(&mut tx.tracked, false);
        for id in gone {
            self.drop_style_refs(&mut tx, id, &res);
        }
        tx.tracked = tracked;
        let paras = self.styled_paras(&changed);
        let tables: Vec<_> = self
            .index
            .table_ids()
            .filter(|&t| self.index.table(&self.doc, t).and_then(|t| t.style_id.as_deref()).is_some_and(|s| changed.iter().any(|c| c == s)))
            .collect();
        tx.stale_paras.extend(paras);
        tx.stale_tables.extend(tables);
        if relabel {
            let lists: Vec<crate::layout::ParaId> = self.list_paras().into_iter().map(|(p, _, _)| p).collect();
            tx.stale_paras.extend(lists);
            tx.relabel = true;
        }
        self.commit(tx, None, fonts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_style_part_is_edited_in_place_however_its_type_is_quoted() {
        let cx = Cx::of_xml(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#);
        let raw = r#"<w:style w:type="table" w:styleId="T"><w:name w:val="T"/><w:tblStylePr w:type='firstRow'><w:tcPr><w:noWrap/></w:tcPr></w:tblStylePr></w:style>"#;
        let def = edit_style(&StyleDef::from_xml(raw, &cx), |s, set| {
            edit_cells(s, set, Some(RegionKind::FirstRow), |e| {
                e.v_align = Some(crate::docx::VerticalAlignment::Center);
                e.whole(crate::format::CellProp::VAlign);
            })
        });
        let xml = def.xml(&Default::default(), &cx);
        assert_eq!(xml.matches("<w:tblStylePr").count(), 1, "{xml}");
        assert!(xml.contains(r#"<w:tcPr><w:noWrap/><w:vAlign w:val="center"/></w:tcPr>"#), "{xml}");
    }
}
