//! Style descriptions

use crate::docx::props::{Border, FontRef, Jc, LineRule, ParaProps, RunProps, Tab, TabKind, toggle};
use crate::docx::styles::{Style, StyleKind, Styles};

/// A line of a description's group: written under it, on its own line.
const SUB: &str = "\r    ";

fn number(v: f64) -> String {
    let s = format!("{:.2}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn inches(pt: f64) -> String {
    format!("{}\"", number(pt / 72.0))
}

fn points(pt: f64) -> String {
    format!("{} pt", number(pt))
}

/// A colour by its name in the colour picker, or its RGB.
fn colour(rgb: u32, theme: Option<&str>) -> String {
    if let Some(t) = theme {
        let digits: String = t.chars().filter(char::is_ascii_digit).collect();
        let name = match t.trim_end_matches(|c: char| c.is_ascii_digit()) {
            "accent" => "Accent",
            "text" | "dark" => "Text",
            "background" | "light" => "Background",
            "hyperlink" => return "Hyperlink".into(),
            "followedHyperlink" => return "Followed Hyperlink".into(),
            _ => "",
        };
        if !name.is_empty() {
            return format!("{name} {digits}");
        }
    }
    let named = match rgb {
        0x000000 => "Black",
        0xFFFFFF => "White",
        0x404040 => "Gray-75%",
        0x808080 => "Gray-50%",
        0xC0C0C0 => "Gray-25%",
        0xC00000 => "Dark Red",
        0xFF0000 => "Red",
        0xFFC000 => "Orange",
        0xFFFF00 => "Yellow",
        0x92D050 => "Light Green",
        0x00B050 => "Green",
        0x00B0F0 => "Light Blue",
        0x0070C0 => "Blue",
        0x002060 => "Dark Blue",
        0x7030A0 => "Purple",
        _ => "",
    };
    if named.is_empty() { format!("Custom Color(RGB({},{},{}))", rgb >> 16, (rgb >> 8) & 0xFF, rgb & 0xFF) } else { named.to_string() }
}

/// The `w:themeColor` of `raw`'s `w:rPr` `w:color`, if any.
fn theme_colour(raw: &str) -> Option<String> {
    let rpr = raw.split("rPr>").nth(1)?;
    let at = rpr.find("color ")?;
    let tail = &rpr[at..];
    let attr = tail.split("themeColor=\"").nth(1)?;
    Some(attr.split('"').next()?.to_string())
}

fn font_name(f: &FontRef) -> String {
    match f {
        FontRef::Name(n) => n.clone(),
        FontRef::Theme(t) if t.starts_with("major") => "+Headings".into(),
        FontRef::Theme(_) => "+Body".into(),
    }
}

fn border_line(b: &Border) -> String {
    let style = match b.style.as_str() {
        "single" => "Single solid line",
        "double" => "Double solid lines",
        "dotted" => "Dotted line",
        "dashed" => "Dashed line",
        "thick" => "Single solid line",
        "wave" => "Wave line",
        other => other,
    };
    let colour = b.color.map_or("Auto".to_string(), |c| colour(c, None));
    format!("({style}, {colour},  {} Line width)", points(b.width))
}

fn tab(t: &Tab) -> String {
    let kind = match t.kind {
        TabKind::Left => "Left",
        TabKind::Center => "Centered",
        TabKind::Right => "Right",
        TabKind::Decimal => "Decimal",
        TabKind::Bar => "Bar",
        TabKind::Num => "List",
        TabKind::Clear => "Clear",
    };
    format!("{}, {kind}", inches(t.pos))
}

/// The character formatting items: the font group, then the effects.
fn character(r: &RunProps, raw: &str, default_font: Option<&str>, out: &mut Vec<String>) {
    let mut font: Vec<String> = Vec::new();
    match (&r.ascii, default_font) {
        (Some(f), _) => font.push(font_name(f)),
        (None, Some(d)) => font.push(format!("(Default) {d}")),
        (None, None) => {}
    }
    if let Some(sz) = r.size {
        font.push(points(sz));
    }
    let on = |bit: u16| (r.toggles_set & bit != 0).then_some(r.toggles & bit != 0);
    match on(toggle::B) {
        Some(true) => font.push("Bold".into()),
        Some(false) => font.push("Not Bold".into()),
        None => {}
    }
    match on(toggle::I) {
        Some(true) => font.push("Italic".into()),
        Some(false) => font.push("Not Italic".into()),
        None => {}
    }
    if let Some(first) = font.first_mut() {
        *first = format!("Font: {first}");
    }
    out.extend(font);
    if let Some(Some(c)) = r.color {
        out.push(format!("Font color: {}", colour(c, theme_colour(raw).as_deref())));
    }
    if let Some(u) = r.underline.as_deref().filter(|u| *u != "none") {
        out.push(if u == "single" { "Underline".into() } else { format!("Underline: {u}") });
    }
    for (bit, name) in [(toggle::CAPS, "All caps"), (toggle::SMALL_CAPS, "Small caps"), (toggle::STRIKE, "Strikethrough"), (toggle::VANISH, "Hidden")]
    {
        if on(bit) == Some(true) {
            out.push(name.into());
        }
    }
}

/// The paragraph formatting items.
fn paragraph(p: &ParaProps, root: bool, out: &mut Vec<String>) {
    let mut indent = Vec::new();
    if let Some(l) = p.ind_left.filter(|l| *l != 0.0) {
        indent.push(format!("{SUB}Left:  {}", inches(l)));
    }
    if let Some(r) = p.ind_right.filter(|r| *r != 0.0) {
        indent.push(format!("{SUB}Right:  {}", inches(r)));
    }
    match p.ind_first {
        Some(f) if f > 0.0 => indent.push(format!("{SUB}First line:  {}", inches(f))),
        Some(f) if f < 0.0 => indent.push(format!("{SUB}Hanging:  {}", inches(-f))),
        _ => {}
    }
    if !indent.is_empty() {
        out.push(format!("Indent:{}", indent.concat()));
    }
    match p.jc.or(root.then_some(Jc::Left)) {
        Some(Jc::Left) => out.push("Left".into()),
        Some(Jc::Center) => out.push("Centered".into()),
        Some(Jc::Right) => out.push("Right".into()),
        Some(Jc::Both) => out.push("Justified".into()),
        Some(Jc::Distribute) => out.push("Distributed".into()),
        None => {}
    }
    if let Some(line) = p.line {
        let text = match p.line_rule.unwrap_or(LineRule::Auto) {
            LineRule::Auto if (line - 240.0).abs() < 0.5 => "single".to_string(),
            LineRule::Auto if (line - 360.0).abs() < 0.5 => "1.5 lines".to_string(),
            LineRule::Auto if (line - 480.0).abs() < 0.5 => "double".to_string(),
            LineRule::Auto => format!("Multiple {} li", number(line / 240.0)),
            LineRule::Exact => format!("Exactly {}", points(line / 20.0)),
            LineRule::AtLeast => format!("At least {}", points(line / 20.0)),
        };
        out.push(format!("{SUB}Line spacing:  {text}"));
    }
    let mut space = String::new();
    if let Some(b) = p.space_before {
        space.push_str(&format!("{SUB}Before:  {}", points(b)));
    }
    if let Some(a) = p.space_after {
        space.push_str(&format!("{SUB}After:  {}", points(a)));
    }
    if !space.is_empty() {
        out.push(format!("Space{space}"));
    }
    let sides =
        [(&p.border_top, "Top"), (&p.border_bottom, "Bottom"), (&p.border_left, "Left"), (&p.border_right, "Right"), (&p.border_between, "Between")];
    let boxed = p.border_top.is_some() && [&p.border_bottom, &p.border_left, &p.border_right].iter().all(|b| **b == p.border_top);
    if boxed {
        if let Some(b) = &p.border_top {
            out.push(format!("Border:{SUB}Box: {}", border_line(b)));
        }
    } else {
        let lines: String = sides.iter().filter_map(|(b, name)| b.as_ref().map(|b| format!("{SUB}{name}: {}", border_line(b)))).collect();
        if !lines.is_empty() {
            out.push(format!("Border:{lines}"));
        }
    }
    for (v, name) in [(p.keep_next, "Keep with next"), (p.keep_lines, "Keep lines together"), (p.page_break_before, "Page break before")] {
        if v == Some(true) {
            out.push(name.into());
        }
    }
    if p.widow_control.unwrap_or(root) && (p.widow_control.is_some() || root) {
        out.push("Widow/Orphan control".into());
    }
    if let Some(l) = p.outline_lvl.filter(|l| (0..9).contains(l)) {
        out.push(format!("Level {}", l + 1));
    }
    if p.contextual == Some(true) {
        out.push("Don't add space between paragraphs of the same style".into());
    }
    let tabs: Vec<String> = p.tabs.iter().filter(|t| t.kind != TabKind::Clear).map(tab).collect();
    if !tabs.is_empty() {
        out.push(format!("{SUB}Tab stops:  {}", tabs.join(" +  ")));
    }
}

/// What `own` sets that `base` doesn't already have.
fn differs<T: PartialEq + Clone>(own: &Option<T>, base: &Option<T>) -> Option<T> {
    own.as_ref().filter(|v| base.as_ref() != Some(*v)).cloned()
}

/// The paragraph properties a style sets over its base's, as text in the base resolves.
fn own_ppr(style: &Style, styles: &Styles) -> ParaProps {
    let Some(b) = style.based_on.as_deref() else { return style.base.ppr.clone() };
    let mut base = styles.doc_ppr.clone();
    base.merge(&styles.chain_ppr(Some(b)));
    let o = &style.base.ppr;
    let mut p = o.clone();
    p.jc = differs(&o.jc, &base.jc);
    p.ind_left = differs(&o.ind_left, &base.ind_left);
    p.ind_right = differs(&o.ind_right, &base.ind_right);
    p.ind_first = differs(&o.ind_first, &base.ind_first);
    p.space_before = differs(&o.space_before, &base.space_before);
    p.space_after = differs(&o.space_after, &base.space_after);
    if (o.line, o.line_rule) == (base.line, base.line_rule) {
        p.line = None;
    }
    p.keep_next = differs(&o.keep_next, &base.keep_next);
    p.keep_lines = differs(&o.keep_lines, &base.keep_lines);
    p.page_break_before = differs(&o.page_break_before, &base.page_break_before);
    p.widow_control = differs(&o.widow_control, &base.widow_control);
    p.outline_lvl = differs(&o.outline_lvl, &base.outline_lvl);
    p.contextual = differs(&o.contextual, &base.contextual);
    p
}

/// The character properties a style sets over its base's.
fn own_rpr(style: &Style, styles: &Styles) -> RunProps {
    let Some(b) = style.based_on.as_deref() else { return style.base.rpr.clone() };
    let mut base = styles.doc_rpr.clone();
    base.merge_level(&styles.chain_rpr(Some(b)));
    let o = &style.base.rpr;
    let mut r = o.clone();
    r.ascii = differs(&o.ascii, &base.ascii);
    r.size = differs(&o.size, &base.size);
    r.color = differs(&o.color, &base.color);
    r.underline = differs(&o.underline, &base.underline);
    let same = o.toggles_set & base.toggles_set & !(o.toggles ^ base.toggles);
    r.toggles_set = o.toggles_set & !same;
    r
}

/// A style's own formatting in words, as the style dialog shows it: what it sets over the style it is based on, then
/// its list flags, its base and the style that follows it. A style based on none also shows the defaults it starts
/// from.
pub fn describe(style: &Style, styles: &Styles) -> String {
    let root = style.based_on.is_none() && style.kind == StyleKind::Paragraph;
    let mut items: Vec<String> = Vec::new();
    let mut rpr = own_rpr(style, styles);
    let default_font = if root && rpr.ascii.is_none() { styles.doc_rpr.ascii.as_ref().map(font_name) } else { None };
    if root {
        rpr.size = rpr.size.filter(|s| Some(*s) != styles.doc_rpr.size);
    }
    character(&rpr, &style.raw, default_font.as_deref(), &mut items);
    if style.kind != StyleKind::Character {
        let mut ppr = own_ppr(style, styles);
        if root {
            let mut merged = styles.doc_ppr.clone();
            merged.merge(&ppr);
            ppr = merged;
        }
        paragraph(&ppr, root, &mut items);
    }
    let mut flags: Vec<&str> = Vec::new();
    if style.link.is_some() {
        flags.push("Linked");
    }
    if style.meta.auto_redefine {
        flags.push("Automatically update");
    }
    if style.meta.unhide_when_used {
        flags.push("Hide until used");
    }
    if style.quick {
        flags.push("Show in the Styles gallery");
    }
    if style.meta.locked {
        flags.push("Locked");
    }
    let priority = style.priority.filter(|p| *p != 0).map(|p| format!("Priority: {}", p + 1));
    match (flags.is_empty(), priority) {
        (false, Some(p)) => items.push(format!("Style: {}, {p}", flags.join(", "))),
        (false, None) => items.push(format!("Style: {}", flags.join(", "))),
        (true, Some(p)) => items.push(p),
        (true, None) => {}
    }
    let name = |id: &str| styles.get(id).map_or(id.to_string(), display_name);
    if let Some(b) = &style.based_on {
        items.push(format!("{SUB}Based on: {}", name(b)));
    }
    if let Some(n) = style.next.as_deref().filter(|n| *n != style.id) {
        items.push(format!("{SUB}Following style: {}", name(n)));
    }
    let mut out = String::new();
    for item in items {
        if !out.is_empty() && !item.starts_with('\r') {
            out.push_str(", ");
        }
        out.push_str(&item);
    }
    out
}

/// A style's name as lists show it: a built-in's capitalised ("heading 1" is "Heading 1").
pub fn display_name(s: &Style) -> String {
    if !s.custom && !s.name.chars().any(char::is_uppercase) {
        s.name
            .split(' ')
            .map(|w| w.chars().take(1).flat_map(char::to_uppercase).chain(w.chars().skip(1)).collect::<String>())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        s.name.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::describe;

    fn styles() -> crate::docx::styles::Styles {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-STYLES.docx");
        let doc = crate::docx::read_editable(&std::fs::read(path).expect("FX-STYLES")).expect("reads");
        doc.resolver.expect("a resolver").styles.clone()
    }

    #[test]
    fn descriptions_read_as_the_style_dialog_writes_them() {
        let s = styles();
        let d = |id: &str| describe(s.get(id).expect(id), &s);
        assert_eq!(
            d("Heading1"),
            "Font: 16 pt, Bold, Space\r    Before:  12 pt\r    After:  0 pt, Keep with next, Keep lines together, Level 1, Style: Show in the Styles gallery\r    Based on: Normal\r    Following style: Normal"
        );
        assert_eq!(
            d("Heading2"),
            "Font: 13 pt, Space\r    Before:  2 pt, Level 2, Style: Hide until used, Show in the Styles gallery, Priority: 10\r    Based on: Heading 1\r    Following style: Normal"
        );
        assert_eq!(
            d("Quote"),
            "Font: Italic, Font color: Gray-75%, Indent:\r    Left:  0.6\"\r    Right:  0.6\", Centered, Space\r    Before:  10 pt, Style: Linked, Show in the Styles gallery, Priority: 30\r    Based on: Normal\r    Following style: Normal"
        );
        assert_eq!(
            d("Emph"),
            "Font: Italic, Font color: Dark Red, Style: Show in the Styles gallery, Priority: 21\r    Based on: Default Paragraph Font"
        );
        assert_eq!(d("MemoBody"), "\r    Line spacing:  1.5 lines\r    Based on: Normal");
        assert_eq!(d("Locked"), "All caps, Style: Locked\r    Based on: Normal");
        assert_eq!(d("Draft"), "Font color: Custom Color(RGB(127,127,127)), Style: Hide until used, Priority: 51\r    Based on: Normal");
        assert_eq!(
            d("Normal"),
            "Font: (Default) Calibri, Left\r    Line spacing:  Multiple 1.08 li, Space\r    After:  8 pt, Widow/Orphan control, Style: Show in the Styles gallery"
        );
    }
}
