//! Run resolution: a run's typed authored properties over its paragraph's run base

use std::borrow::Cow;
use std::rc::Rc;

use super::char_props::RunDirect;
use super::model::{Run, RunStyle};
use super::resolvers::Resolvers;
use super::review::{FormatChange, RunRevisions};
use crate::format::CharProps;
use crate::format::cascade::Cascade;
use crate::format::resolve;

/// Character style `id`'s chain, folded.
pub fn character_style(res: &Resolvers, id: &str) -> CharProps {
    res.cascade().chain_char(Some(id))
}

/// A run's levels folded: its own properties `own` over a paragraph's typed `base` through its character style; in a
/// table of contents' result (`toc`) without the Hyperlink character style.
pub fn folded(res: &Resolvers, base: &CharProps, own: Option<&CharProps>, toc: bool) -> CharProps {
    let id = own.and_then(|p| p.style.as_deref()).filter(|id| !(toc && *id == "Hyperlink"));
    let style = id.map(|id| character_style(res, id));
    let own = match own {
        Some(p) if toc && p.style.as_deref() == Some("Hyperlink") => Cow::Owned(CharProps { style: None, ..p.clone() }),
        Some(p) => Cow::Borrowed(p),
        None => Cow::Owned(CharProps::default()),
    };
    resolve::run_props(base, style.as_ref(), &own)
}

/// A run with authored properties `direct` over a paragraph's typed `base`: in a table of contents' result (`toc`)
/// without the Hyperlink character style. No revisions.
pub fn resolve(res: &Resolvers, base: &CharProps, direct: RunDirect, toc: bool) -> RunStyle {
    let p = folded(res, base, direct.props().map(|p| &**p), toc);
    let mut s = resolve::run_style(&res.cascade().env(), &p);
    s.direct = direct;
    s.toc = toc;
    s
}

/// A list number's run: its level's formatting `level` over `under`, what its paragraph mark's levels fold to.
pub fn label(res: &Resolvers, under: &CharProps, level: Option<&CharProps>) -> Run {
    let mut p = under.clone();
    if let Some(l) = level {
        p.merge(l);
    }
    Rc::new(resolve::run_style(&res.cascade().env(), &p))
}

/// A run with no authored properties of its own over `base`.
pub fn plain(res: &Resolvers, base: &CharProps) -> Run {
    Rc::new(resolve(res, base, RunDirect::default(), false))
}

/// A run with authored properties `direct` over `base`.
pub fn run_of(res: &Resolvers, base: &CharProps, direct: RunDirect) -> Run {
    Rc::new(resolve(res, base, direct, false))
}

/// `run`'s authored properties, to edit.
pub fn edit_of(run: &RunStyle) -> crate::format::CharEdit {
    crate::format::CharEdit::new(run.direct.props().map_or_else(CharProps::default, |p| (**p).clone()))
}

/// `run` resolved again from its authored properties over `base`, where it is: in a table of contents' result
/// without the Hyperlink character style, as the reader resolved it, its review marks kept.
pub fn rerun(res: &Resolvers, base: &CharProps, run: &RunStyle) -> Run {
    again(res, base, run, run.direct.clone())
}

/// Authored properties `direct` resolved over typed `base` in `run`'s place.
pub fn again(res: &Resolvers, base: &CharProps, run: &RunStyle, direct: RunDirect) -> Run {
    let mut s = resolve(res, base, direct, run.toc);
    s.rev = RunRevisions {
        ins: run.rev.ins.clone(),
        del: run.rev.del.clone(),
        fmt: run.rev.fmt.as_ref().map(|f| {
            let previous = resolve(res, base, f.previous.direct.clone(), false);
            if previous.layout_eq(&f.previous) { f.clone() } else { Rc::new(FormatChange { previous: Rc::new(previous), ..(**f).clone() }) }
        }),
    };
    Rc::new(s)
}

// On typed definitions

/// [`folded`] on the typed cascade `c`.
pub fn folded_typed(c: &Cascade, base: &CharProps, own: Option<&CharProps>, toc: bool) -> CharProps {
    let id = own.and_then(|p| p.style.as_deref()).filter(|id| !(toc && *id == "Hyperlink"));
    let style = id.map(|id| c.chain_char(Some(id)));
    let own = match own {
        Some(p) if toc && p.style.as_deref() == Some("Hyperlink") => Cow::Owned(CharProps { style: None, ..p.clone() }),
        Some(p) => Cow::Borrowed(p),
        None => Cow::Owned(CharProps::default()),
    };
    resolve::run_props(base, style.as_ref(), &own)
}

/// [`resolve`] on the typed cascade `c`.
pub fn resolve_typed(c: &Cascade, base: &CharProps, direct: RunDirect, toc: bool) -> RunStyle {
    let p = folded_typed(c, base, direct.props().map(|p| &**p), toc);
    let mut s = c.run_style(&p);
    s.direct = direct;
    s.toc = toc;
    s
}

/// [`again`] on the typed cascade `c`.
pub fn again_typed(c: &Cascade, base: &CharProps, run: &RunStyle, direct: RunDirect) -> Run {
    let mut s = resolve_typed(c, base, direct, run.toc);
    s.rev = RunRevisions {
        ins: run.rev.ins.clone(),
        del: run.rev.del.clone(),
        fmt: run.rev.fmt.as_ref().map(|f| {
            let previous = resolve_typed(c, base, f.previous.direct.clone(), false);
            if previous.layout_eq(&f.previous) { f.clone() } else { Rc::new(FormatChange { previous: Rc::new(previous), ..(**f).clone() }) }
        }),
    };
    Rc::new(s)
}

/// [`rerun`] on the typed cascade `c`, over a typed `base`.
pub fn rerun_typed(c: &Cascade, base: &CharProps, run: &RunStyle) -> Run {
    again_typed(c, base, run, run.direct.clone())
}
