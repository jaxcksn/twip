//! Paragraph resolution: a paragraph's typed authored properties over its style chain, its list level, its table
//! style region and the document defaults

use std::rc::Rc;

use super::model::{ListId, ListJc, ListRef, ParaStyle};
use super::para_props::{ParaDirect, from_para_props};
use super::resolve::{ParaResolved, Resolver, div_chain};
use super::review::ParaChange;
use super::styles::Region;
use super::write::props::Cx;
use crate::format::resolve::para_style;
use crate::format::{ListChoice, ParaProps};

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

/// A paragraph with authored properties `direct`, its mark's size `mark_size` (points, when its mark sets one), in
/// `region` when it is in a cell; its run base still from the style chain's XML-read properties. The effective
/// format's authored properties are left default.
pub fn resolve(res: &Resolver, direct: &ParaProps, mark_size: Option<f64>, region: Option<&Region>) -> ParaResolved {
    let style_id = res.styles.paragraph_style(direct.style.as_deref()).map(str::to_string);
    let style_ppr = from_para_props(&res.styles.chain_ppr(style_id.as_deref()));
    let direct_list = direct.list.map(list_id);
    let (num_id, ilvl, from_style) = match direct_list {
        Some(id) => (Some(id), direct.list_level.or(style_ppr.list_level).unwrap_or(0), false),
        None => match style_ppr.list.map(list_id) {
            Some(id) => (Some(id), direct.list_level.or(style_ppr.list_level).unwrap_or(0), true),
            None => (None, 0, false),
        },
    };
    let num_id = num_id.filter(|&id| id != 0);
    let level = num_id.and_then(|id| res.numbering.level(id, ilvl, &res.styles)).cloned();

    let mut ppr = from_para_props(&res.styles.doc_ppr);
    if let Some(t) = region {
        ppr.merge(&from_para_props(&t.ppr));
    }
    match (&level, from_style) {
        (Some(l), true) => {
            ppr.merge(&from_para_props(&l.ppr));
            ppr.merge(&style_ppr);
        }
        _ => {
            ppr.merge(&style_ppr);
            if let Some(l) = &level {
                ppr.merge(&from_para_props(&l.ppr));
            } else if direct_list == Some(0) {
                // No list over a style's still sets a level's indents: start 0, first line 0 over the style chain's
                // (the end indent stays).
                (ppr.indent_left, ppr.indent_first) = (Some(0.0), Some(0.0));
            }
        }
    }
    ppr.merge(direct);
    ppr.style = style_id.as_deref().map(Into::into);
    let mut style = para_style(&res.styles.colours, &ppr);
    if let Some(l) = &level {
        // A legacy level places its number itself, in the label.
        style.list_jc = if l.legacy.is_some() { ListJc::Left } else { super::paragraph::list_jc(&l.jc) };
        style.num = num_id.map(|id| ListRef { list: ListId(id), level: ilvl });
        style.list_fmt = Some(l.fmt.clone());
        style.list = num_id.and_then(|id| res.numbering.abstract_of(id, &res.styles));
    }

    let mut base = res.styles.doc_rpr.clone();
    if let Some(t) = region {
        base.merge_level(&t.rpr);
    }
    let style_rpr = res.styles.chain_rpr(style_id.as_deref());
    base.merge_level(&style_rpr);
    if let Some(size) = region.and_then(|t| t.size_over_normal) {
        let sizer = res.styles.chain(style_id.as_deref()).into_iter().rev().find(|s| s.base.rpr.size.is_some());
        if sizer.is_some_and(|s| Some(&s.id) == res.styles.default_paragraph.as_ref() && s.base.rpr.size == Some(12.0)) {
            base.size = Some(size);
        }
    }
    if region.is_some_and(|t| t.mark_at_style) && mark_size == Some(11.0) {
        style.mark_size = base.size;
    }
    // The margins of the paragraph's division and its ancestors add to its indents; in a table cell only its own
    // division's.
    let mut chain = div_chain(&res.divisions, style.div);
    if region.is_some() {
        chain.truncate(1);
    }
    if !chain.is_empty() {
        let sum = |i: usize| chain.iter().map(|id| res.divisions[id].margins[i]).sum::<f64>();
        style.ind_left += sum(0);
        style.ind_right += sum(1);
    }
    ParaResolved { style, base, level, num_id, ilvl, from_style, style_id }
}

/// A paragraph with authored properties `direct` (its mark's size `mark_size`), in `region` when it is in a cell.
pub fn with(res: &Resolver, direct: ParaDirect, mark_size: Option<f64>, region: Option<&Region>) -> ParaResolved {
    let mut r = resolve(res, direct.get(), mark_size, region);
    r.style.direct = direct;
    r
}

/// Authored paragraph properties `d` as the `w:pPr` written in the part `cx` describes, a theme colour's value as
/// `res`'s theme resolves it.
pub fn ppr<'d>(res: Option<&Resolver>, d: &'d ParaDirect, cx: &Cx) -> Option<std::borrow::Cow<'d, str>> {
    d.xml(&res.map_or_else(Default::default, |r| r.styles.colours), cx)
}

/// The mark size a paragraph's `w:pPr` as written (in the part `cx` describes) sets.
fn mark_size_of(ppr: Option<&str>, cx: &Cx) -> Option<f64> {
    ppr.and_then(|x| cx.parse(x, |_, el| mark_size(el))).flatten()
}

/// A paragraph resolved from its `w:pPr` as written (`ppr`, in the part `cx` describes), in `region` when it is in
/// a cell: its properties parsed, the XML kept beside them.
pub fn from_xml(res: &Resolver, ppr: Option<&str>, region: Option<&Region>, cx: &Cx) -> ParaResolved {
    with(res, ParaDirect::from_xml(ppr, cx), mark_size_of(ppr, cx), region)
}

/// `again`, a paragraph's properties resolved again in `region`, with what reading gave `was` beyond them: its
/// tracked property change (the earlier properties resolved again too) and its place in a frame.
pub fn again(res: &Resolver, was: &ParaStyle, mut again: ParaStyle, region: Option<&Region>, cx: &Cx) -> ParaStyle {
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
pub fn of(res: &Resolver, d: &ParaDirect, region: Option<&Region>, cx: &Cx) -> ParaResolved {
    let size = mark_size_of(d.origin_xml().map(|x| &**x), cx);
    with(res, d.clone(), size, region)
}
