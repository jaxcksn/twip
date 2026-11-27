//! Paragraph resolution: a paragraph's typed authored properties over its style chain, its list level, its table
//! style region and the document defaults

use std::rc::Rc;

use super::list_xml::{format_name, jc_name, legacy_of, suffix_name};
use super::model::ListLabel;
use super::model::{ListId, ListJc, ListRef, ParaStyle};
use super::para_props::ParaDirect;
use super::resolvers::Resolvers;
use super::review::ParaChange;
use super::write::props::Cx;
use crate::format::cascade::{Cascade, ParaCascade, TableRegion};
use crate::format::resolve::para_style;
use crate::format::{CharProps, ListChoice, ListLevel, ParaProps};

/// The size (points) a `w:pPr`'s mark sets, from its `w:rPr`.
pub fn mark_size(ppr: roxmltree::Node) -> Option<f64> {
    super::xml::child(ppr, "rPr").and_then(|r| super::char_props::parse(r).0.size)
}

/// A list choice as the list's id: 0 for none.
fn list_id(c: ListChoice) -> i64 {
    match c {
        ListChoice::None => 0,
        ListChoice::In(id) => id.0,
    }
}

/// A paragraph's resolved properties, and what its runs and label resolve over.
#[derive(Clone, Debug)]
pub struct Resolved {
    /// The effective format.
    pub style: ParaStyle,
    /// Its runs' levels below their own: the defaults, the region's, the paragraph style chain's.
    pub base: CharProps,
    /// The list level it is at.
    pub level: Option<Rc<ListLevel>>,
    /// Its list.
    pub num_id: Option<i64>,
    /// Its level in the list.
    pub ilvl: i64,
    /// The list comes from the paragraph style, not its own properties.
    pub from_style: bool,
    /// Its paragraph style.
    pub style_id: Option<String>,
}

/// A paragraph with authored properties `direct`, its mark's size `mark_size` (points, when its mark sets one), in
/// `region` when it is in a cell: its style chain and the defaults typed. The effective format's authored properties
/// are left default.
pub fn resolve(res: &Resolvers, direct: &ParaProps, mark_size: Option<f64>, region: Option<&TableRegion>) -> Resolved {
    let c = res.cascade();
    let style_id = c.paragraph_style(direct.style.as_deref()).map(str::to_string);
    let style_ppr = c.chain_para(style_id.as_deref());
    let direct_list = direct.list.map(list_id);
    let (num_id, ilvl, from_style) = match direct_list {
        Some(id) => (Some(id), direct.list_level.or(style_ppr.list_level).unwrap_or(0), false),
        None => match style_ppr.list.map(list_id) {
            Some(id) => (Some(id), direct.list_level.or(style_ppr.list_level).unwrap_or(0), true),
            None => (None, 0, false),
        },
    };
    let num_id = num_id.filter(|&id| id != 0);
    let level = num_id.and_then(|id| c.level(ListId(id), ilvl)).cloned();

    let mut ppr = c.doc_para().clone();
    if let Some(t) = region {
        ppr.merge(&t.format.para);
    }
    match (&level, from_style) {
        (Some(l), true) => {
            ppr.merge(&l.para);
            ppr.merge(&style_ppr);
        }
        _ => {
            ppr.merge(&style_ppr);
            if let Some(l) = &level {
                ppr.merge(&l.para);
            } else if direct_list == Some(0) {
                // No list over a style's still sets a level's indents: start 0, first line 0 over the style chain's
                // (the end indent stays).
                (ppr.indent_left, ppr.indent_first) = (Some(0.0), Some(0.0));
            }
        }
    }
    ppr.merge(direct);
    ppr.style = style_id.as_deref().map(Into::into);
    let mut style = para_style(&c.colours, &ppr);
    if let Some(l) = &level {
        // A legacy level places its number itself, in the label.
        style.list_jc = if legacy_of(l).is_some() { ListJc::Left } else { super::paragraph::list_jc(jc_name(l.jc.as_ref())) };
        style.num = num_id.map(|id| ListRef { list: ListId(id), level: ilvl });
        style.list_fmt = Some(format_name(l.format.as_ref()).to_string());
        style.list = num_id.and_then(|id| c.abstract_of(ListId(id))).map(|a| a.0);
    }

    let mut base = c.doc_char().clone();
    if let Some(t) = region {
        base.merge_level(&t.format.char);
    }
    base.merge_level(&c.chain_char(style_id.as_deref()));
    if let Some(size) = region.and_then(|t| t.size_over_normal) {
        let chain = c.chain(style_id.as_deref());
        let sizer = chain.into_iter().rev().find(|s| c.format_of(s).char.size.is_some());
        let normal =
            |s: &crate::format::StyleEntry| c.styles.default_paragraph.as_deref() == Some(&*s.style.id) && c.format_of(s).char.size == Some(12.0);
        if sizer.is_some_and(normal) {
            base.size = Some(size);
        }
    }
    if region.is_some_and(|t| t.mark_at_style) && mark_size == Some(11.0) {
        style.mark_size = base.size;
    }
    // The margins of the paragraph's division and its ancestors add to its indents; in a table cell only its own
    // division's.
    let mut chain = crate::format::cascade::div_chain(&c.divisions, style.div);
    if region.is_some() {
        chain.truncate(1);
    }
    if !chain.is_empty() {
        let sum = |i: usize| chain.iter().map(|id| c.divisions[id].margins[i]).sum::<f64>();
        style.ind_left += sum(0);
        style.ind_right += sum(1);
    }
    Resolved { style, base, level, num_id, ilvl, from_style, style_id }
}

/// A list label with text `text` at level `level`, its run the level's character properties over `under`, what its
/// paragraph mark's levels fold to.
pub fn label(res: &Resolvers, text: String, under: &CharProps, level: &ListLevel) -> ListLabel {
    ListLabel {
        text,
        run: super::run_xml::label(res, under, Some(&level.char)),
        jc: super::paragraph::list_jc(jc_name(level.jc.as_ref())),
        suff: super::paragraph::list_suffix(suffix_name(level.suffix.as_ref())),
        picture: res.cascade().bullet_image(level),
        legacy: legacy_of(level),
    }
}

/// A paragraph with authored properties `direct` (its mark's size `mark_size`), in `region` when it is in a cell.
pub fn with(res: &Resolvers, direct: ParaDirect, mark_size: Option<f64>, region: Option<&TableRegion>) -> Resolved {
    let mut r = resolve(res, direct.get(), mark_size, region);
    r.style.direct = direct;
    r
}

/// Authored paragraph properties `d` as the `w:pPr` written in the part `cx` describes, a theme colour's value as
/// `res`'s theme resolves it.
pub fn ppr<'d>(res: Option<&Resolvers>, d: &'d ParaDirect, cx: &Cx) -> Option<std::borrow::Cow<'d, str>> {
    d.xml(&res.map_or_else(Default::default, |r| r.cascade().colours), cx)
}

/// The mark size a paragraph's `w:pPr` as written (in the part `cx` describes) sets.
fn mark_size_of(ppr: Option<&str>, cx: &Cx) -> Option<f64> {
    ppr.and_then(|x| cx.parse(x, |_, el| mark_size(el))).flatten()
}

/// A paragraph resolved from its `w:pPr` as written (`ppr`, in the part `cx` describes), in `region` when it is in
/// a cell: its properties parsed, the XML kept beside them.
pub fn from_xml(res: &Resolvers, ppr: Option<&str>, region: Option<&TableRegion>, cx: &Cx) -> Resolved {
    with(res, ParaDirect::from_xml(ppr, cx), mark_size_of(ppr, cx), region)
}

/// `again`, a paragraph's properties resolved again in `region`, with what reading gave `was` beyond them: its
/// tracked property change (the earlier properties resolved again too) and its place in a frame.
pub fn again(res: &Resolvers, was: &ParaStyle, mut again: ParaStyle, region: Option<&TableRegion>, cx: &Cx) -> ParaStyle {
    if was.in_frame {
        (again.frame, again.in_frame) = (None, true);
    }
    again.change = was.change.as_ref().map(|c| {
        let mut previous = match (c.properties, &c.numbering_change.0) {
            (false, Some(_)) => ParaStyle { change: None, ..again.clone() },
            (false, None) => resolve(res, &ParaProps::default(), None, region).style,
            (true, _) => {
                let d = &c.previous.direct;
                with(res, d.clone(), mark_size_of(d.as_read().map(|x| &**x), cx), region).style
            }
        };
        if was.in_frame {
            (previous.frame, previous.in_frame) = (None, true);
        }
        if previous.layout_eq(&c.previous) { c.clone() } else { Rc::new(ParaChange { previous: Rc::new(previous), ..(**c).clone() }) }
    });
    again
}

/// A paragraph resolved again from its authored properties `d`, in `region` when it is in a cell, its mark's size from
/// the `w:rPr` the `w:pPr` they were read from holds.
pub fn of(res: &Resolvers, d: &ParaDirect, region: Option<&TableRegion>, cx: &Cx) -> Resolved {
    let size = mark_size_of(d.origin_xml().map(|x| &**x), cx);
    with(res, d.clone(), size, region)
}

// On typed definitions

/// A paragraph with authored properties `direct` (its mark's size `mark_size`), in `region` when it is in a cell,
/// resolved by the typed cascade `c`.
pub fn with_typed(c: &Cascade, direct: ParaDirect, mark_size: Option<f64>, region: Option<&TableRegion>) -> ParaCascade {
    let mut r = c.paragraph(direct.get(), mark_size, region);
    r.style.direct = direct;
    r
}

/// A paragraph resolved by the typed cascade `c` from its `w:pPr` as written (`ppr`, in the part `cx` describes), in
/// `region` when it is in a cell.
pub fn from_xml_typed(c: &Cascade, ppr: Option<&str>, region: Option<&TableRegion>, cx: &Cx) -> ParaCascade {
    with_typed(c, ParaDirect::from_xml(ppr, cx), mark_size_of(ppr, cx), region)
}

/// [`again`] on the typed cascade `c`.
pub fn again_typed(c: &Cascade, was: &ParaStyle, mut again: ParaStyle, region: Option<&TableRegion>, cx: &Cx) -> ParaStyle {
    if was.in_frame {
        (again.frame, again.in_frame) = (None, true);
    }
    again.change = was.change.as_ref().map(|ch| {
        let mut previous = match (ch.properties, &ch.numbering_change.0) {
            (false, Some(_)) => ParaStyle { change: None, ..again.clone() },
            (false, None) => c.paragraph(&ParaProps::default(), None, region).style,
            (true, _) => {
                let d = &ch.previous.direct;
                with_typed(c, d.clone(), mark_size_of(d.as_read().map(|x| &**x), cx), region).style
            }
        };
        if was.in_frame {
            (previous.frame, previous.in_frame) = (None, true);
        }
        if previous.layout_eq(&ch.previous) { ch.clone() } else { Rc::new(ParaChange { previous: Rc::new(previous), ..(**ch).clone() }) }
    });
    again
}

/// [`of`] on the typed cascade `c`.
pub fn of_typed(c: &Cascade, d: &ParaDirect, region: Option<&TableRegion>, cx: &Cx) -> ParaCascade {
    let size = mark_size_of(d.origin_xml().map(|x| &**x), cx);
    with_typed(c, d.clone(), size, region)
}
