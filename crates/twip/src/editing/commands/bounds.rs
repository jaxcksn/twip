//! Argument ranges of the formatting commands

use crate::api::command::{Border, Command, CustomPropertyKind, Indents, ParagraphSpacing};
use crate::api::error::Error;
use crate::api::value::{AutoOr, LineSpacing, Points};

/// The longest length most settings take, 22 in.
pub(crate) const MAX: f64 = 1584.0;

/// `v` within `lo..=hi`, else an error naming `what`.
pub(crate) fn within(what: &str, v: f64, lo: f64, hi: f64) -> Result<(), Error> {
    if v.is_finite() && (lo..=hi).contains(&v) { Ok(()) } else { Err(Error::invalid(format!("{what} must be from {lo} to {hi}"))) }
}

pub(crate) fn length(what: &str, v: Option<Points>, lo: f64, hi: f64) -> Result<(), Error> {
    v.map_or(Ok(()), |v| within(what, v.0, lo, hi))
}

/// A font size: 1…1638 pt in 0.5 pt steps.
pub(crate) fn font_size(size: f64) -> Result<(), Error> {
    within("the size", size, 1.0, 1638.0)?;
    if (size * 2.0).fract() != 0.0 { Err(Error::invalid("the size goes in 0.5 pt steps")) } else { Ok(()) }
}

/// A border line: 0.25…6 pt.
pub(crate) fn border(b: Option<&Border>) -> Result<(), Error> {
    b.map_or(Ok(()), |b| within("the width", b.width.0, 0.25, 6.0))
}

fn spacing(s: &ParagraphSpacing) -> Result<(), Error> {
    for v in [&s.before, &s.after] {
        if let Some(AutoOr::Value(p)) = v {
            within("the spacing", p.0, 0.0, MAX)?;
        }
    }
    Ok(())
}

fn line_spacing(s: &LineSpacing) -> Result<(), Error> {
    match s {
        LineSpacing::Multiple { lines } => within("the lines", *lines, 0.06, 132.0),
        LineSpacing::Exact { size } | LineSpacing::AtLeast { size } => within("the size", size.0, 0.7, MAX),
    }
}

fn indents(i: &Indents) -> Result<(), Error> {
    length("the left indent", i.left, -MAX, MAX)?;
    length("the right indent", i.right, -MAX, MAX)?;
    length("the first line indent", i.first_line, 0.0, MAX)?;
    length("the hanging indent", i.hanging, 0.0, MAX)?;
    if i.first_line.is_some() && i.hanging.is_some() {
        return Err(Error::invalid("firstLine and hanging exclude each other"));
    }
    Ok(())
}

/// The sizes, lengths, line widths and numbers `command` sets, in the ranges Word takes; every command runs past this check,
/// however it is built (a dialog, a style edit, the document defaults).
pub(crate) fn bounds(command: &Command) -> Result<(), Error> {
    use Command as C;
    match command {
        C::SetTypography(t) => {
            if let Some(s) = t.scale {
                within("the scale", s, 1.0, 600.0)?;
            }
            length("the kerning size", t.kerning, 0.0, 1638.0)?;
            match t.stylistic_set {
                Some(n) if n > 20 => Err(Error::invalid("the stylistic set is 0…20")),
                _ => Ok(()),
            }
        }
        C::SetFontSize(s) => font_size(s.0),
        C::SetCharacterSpacing(s) | C::SetCharacterPosition(s) | C::SetTableIndent(s) => within("the length", s.0, -MAX, MAX),
        C::SetLineSpacing(s) => line_spacing(s),
        C::SetParagraphSpacing(s) => spacing(s),
        C::SetIndents(i) => indents(i),
        C::SetDocumentDefaults(d) => {
            d.size.map_or(Ok(()), |s| font_size(s.0))?;
            d.spacing.as_ref().map_or(Ok(()), spacing)?;
            d.line_spacing.as_ref().map_or(Ok(()), line_spacing)?;
            d.indents.as_ref().map_or(Ok(()), indents)
        }
        C::SetParagraphBorderLines(b) => {
            [&b.top, &b.bottom, &b.left, &b.right, &b.between].into_iter().try_for_each(|e| border(e.as_ref().and_then(Option::as_ref)))
        }
        C::SetPageBorders(b) => [&b.top, &b.bottom, &b.left, &b.right].into_iter().try_for_each(|e| border(e.as_ref().and_then(Option::as_ref))),
        C::SetTextBorder(b) => border(b.border.as_ref()),
        C::SetCellBorders(b) => border(b.border.as_ref()),
        C::SetDocumentProperties(p) => p.custom.iter().flatten().filter(|c| c.kind == CustomPropertyKind::Number).try_for_each(|c| {
            if c.value.trim().parse::<f64>().is_ok_and(f64::is_finite) { Ok(()) } else { Err(Error::invalid(format!("{} isn't a number", c.value))) }
        }),
        _ => Ok(()),
    }
}
