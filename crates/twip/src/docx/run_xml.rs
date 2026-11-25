//! Run formatting through its property XML: the typed resolution of a run's authored properties, and the bridge for
//! code that edits a run's `w:rPr` as XML (written from the typed properties, edited, parsed back)

use std::borrow::Cow;
use std::rc::Rc;

use super::char_props::{RunDirect, from_run_props};
use super::model::{Run, RunStyle};
use super::props::RunProps;
use super::resolve::Resolver;
use super::review::{FormatChange, RunRevisions};
use super::write::props::Cx;
use crate::format::CharProps;
use crate::format::resolve::{self, RunEnv};

/// What resolving a run in `res`'s document needs.
pub fn env(res: &Resolver) -> RunEnv<'_> {
    RunEnv { colours: &res.styles.colours, fonts: &res.theme, categories: &res.categories, open_type: res.styles.open_type }
}

/// A paragraph's run base as typed properties.
pub fn base(base: &RunProps) -> CharProps {
    from_run_props(base)
}

/// Character style `id`'s chain, folded.
pub fn character_style(res: &Resolver, id: &str) -> CharProps {
    from_run_props(&res.styles.chain_rpr(Some(id)))
}

/// A run's levels folded: its own properties `own` over a paragraph's typed `base` through its character style; in a
/// table of contents' result (`toc`) without the Hyperlink character style.
pub fn folded(res: &Resolver, base: &CharProps, own: Option<&CharProps>, toc: bool) -> CharProps {
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
pub fn resolve(res: &Resolver, base: &CharProps, direct: RunDirect, toc: bool) -> RunStyle {
    let p = folded(res, base, direct.props().map(|p| &**p), toc);
    let mut s = resolve::run_style(&env(res), &p);
    s.direct = direct;
    s.toc = toc;
    s
}

/// A run's `w:rPr` to edit as XML in the part `cx` describes: as read, else written from its typed properties.
pub fn rpr<'r>(res: &Resolver, run: &'r RunStyle, cx: &Cx) -> Option<Cow<'r, str>> {
    run.direct.xml(&res.styles.colours, cx)
}

/// A run from its direct `w:rPr` as written (in the part `cx` describes) over `base`: what an edit of a run's direct
/// formatting gives.
pub fn run_from_xml(res: &Resolver, base: &RunProps, rpr: Option<&str>, cx: &Cx) -> Run {
    let direct = RunDirect::from_xml(rpr, cx);
    #[cfg(test)]
    written_as(res, None, &direct, rpr, cx);
    Rc::new(resolve(res, &self::base(base), direct, false))
}

/// `run` resolved again from its authored properties over `base`, where it is: in a table of contents' result
/// without the Hyperlink character style, as the reader resolved it, its review marks kept.
pub fn rerun(res: &Resolver, base: &RunProps, run: &RunStyle) -> Run {
    again(res, &self::base(base), run, run.direct.clone())
}

/// `rpr` resolved over `base` in `run`'s place, as reading gives a run with that `w:rPr` there: in a table of
/// contents' result without the Hyperlink character style, and with `run`'s revisions (a format change's previous
/// properties resolved again over `base`).
pub fn rerun_xml(res: &Resolver, base: &RunProps, run: &RunStyle, rpr: Option<&str>, cx: &Cx) -> Run {
    let direct = RunDirect::from_xml(rpr, cx);
    #[cfg(test)]
    written_as(res, Some(run), &direct, rpr, cx);
    again(res, &self::base(base), run, direct)
}

/// What an edit of a run's `w:rPr` as XML wrote (`rpr`, over `run`'s as read) is what writing its typed properties
/// over the same writes, byte for byte; `TWIP_RPR_BYTES_OUT` names a file to list each one that isn't in, runs made
/// from XML included, in place of failing.
#[cfg(test)]
fn written_as(res: &Resolver, run: Option<&RunStyle>, direct: &RunDirect, rpr: Option<&str>, cx: &Cx) {
    let origin = run.and_then(|r| r.direct.read_from()).map(|(p, x)| (&**p, &**x));
    // A run made from XML copied from elsewhere keeps that XML as written: compared on request only.
    let report = std::env::var_os("TWIP_RPR_BYTES_OUT");
    if run.is_none() && report.is_none() || run.is_some_and(|r| r.direct.props().is_some() && r.direct.as_read().is_none()) {
        return;
    }
    let got = direct.props().and_then(|p| super::char_props::written_over(p, &Default::default(), &res.styles.colours, cx, origin, &[]));
    if got.as_deref() == rpr {
        return;
    }
    let what = format!("over {:?}\n  wrote {rpr:?}\n  typed {got:?}\n", origin.map(|o| o.1));
    match report {
        Some(path) => {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("the report");
            let test = std::thread::current().name().unwrap_or_default().to_string();
            let _ = writeln!(f, "{} {test} {what}", if run.is_some() { "edit" } else { "new" });
        }
        None => panic!("an edit's w:rPr is not what its typed properties write: {what}"),
    }
}

/// Authored properties `direct` resolved over typed `base` in `run`'s place.
pub fn again(res: &Resolver, base: &CharProps, run: &RunStyle, direct: RunDirect) -> Run {
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
