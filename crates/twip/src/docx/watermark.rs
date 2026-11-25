//! Watermarks
//!
//! Word's text and picture watermarks: a VML shape in a header, behind the text and centred on the margins, that
//! Word's Watermark dialog finds by its id's prefix.

use super::model::Float;

/// A text watermark's shape id prefix.
pub const TEXT_ID: &str = "PowerPlusWaterMarkObject";
/// A picture watermark's shape id prefix.
pub const PICTURE_ID: &str = "WordPictureWatermark";
/// The most characters a text watermark has (what a text path reads).
pub const MAX_TEXT: usize = 1024;
/// Word's turn for a diagonal text watermark, degrees.
pub const DIAGONAL: f64 = 315.0;
/// Word's default colour, silver.
pub const SILVER: u32 = 0xC0_C0C0;

/// What a watermark shape shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Watermark {
    Text(TextMark),
    Picture(PictureMark),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextMark {
    pub text: String,
    pub family: String,
    /// The font size; `None` when the text is fitted to the shape (Word's Auto, written as 1pt).
    pub size: Option<f64>,
    pub colour: u32,
    /// Percent, 0 opaque.
    pub transparency: f64,
    pub diagonal: bool,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PictureMark {
    pub image: Option<usize>,
    pub washout: bool,
}

/// A watermark shape's content: a text watermark's shape and text path, or a picture watermark's picture.
enum Kind<'a> {
    Text(&'a crate::drawing::SpShape, &'a crate::drawing::TextPath),
    Picture(&'a crate::drawing::PicShape),
}

/// What watermark `f` is: its id's prefix and the content that prefix names.
fn kind(f: &Float) -> Option<(&crate::drawing::Shape, Kind<'_>)> {
    let shape = f.shape.as_deref()?;
    let name = shape.nv.name.as_str();
    let kind = if name.starts_with(TEXT_ID) {
        let sp = shape.sp()?;
        Kind::Text(sp, sp.text_path.as_deref()?)
    } else if name.starts_with(PICTURE_ID) {
        Kind::Picture(shape.pic()?)
    } else {
        return None;
    };
    Some((shape, kind))
}

/// The watermark `f` is, when it is one.
pub fn of(f: &Float) -> Option<Watermark> {
    let (shape, kind) = kind(f)?;
    Some(match kind {
        Kind::Text(sp, tp) => {
            let palette = crate::drawing::theme::Theme::default();
            let map = crate::drawing::theme::ColourMap::default();
            let fill = match &sp.fill {
                Some(crate::drawing::paint::Fill::Solid(c)) => Some(c.resolve(&palette, &map, None)),
                _ => None,
            };
            let rot = f64::from(shape.xfrm.rot.0) / 60000.0;
            Watermark::Text(TextMark {
                text: tp.string.clone(),
                family: tp.family.clone(),
                size: (tp.size > 1.0).then_some(tp.size),
                colour: fill.map_or(SILVER, |c| c.to_rgb()),
                transparency: transparency(sp.fill.as_ref()),
                diagonal: (rot - DIAGONAL).abs() < 0.5,
                bold: tp.bold,
                italic: tp.italic,
            })
        }
        Kind::Picture(pic) => {
            let washout = pic
                .blip
                .adjust
                .iter()
                .any(|a| matches!(a, crate::drawing::paint::BlipAdjust::Lum { bright, contrast } if bright.0 > 0 && contrast.0 < 0));
            Watermark::Picture(PictureMark { image: pic.blip.image, washout })
        }
    })
}

/// A text watermark's transparency, percent, from its fill's opacity (to the opacity's own precision).
fn transparency(fill: Option<&crate::drawing::paint::Fill>) -> f64 {
    let Some(crate::drawing::paint::Fill::Solid(c)) = fill else { return 0.0 };
    let alpha = c.mods.iter().rev().find_map(|m| if let crate::drawing::colour::ColourMod::Alpha(p) = m { Some(p.0) } else { None });
    alpha.map_or(0.0, |a| f64::from(100_000 - a.clamp(0, 100_000)) / 1000.0)
}

/// A transparency's `v:fill` opacity, written as Word writes one (`.5`); none when opaque.
fn opacity_xml(transparency: f64) -> String {
    let t = if transparency.is_finite() { transparency.clamp(0.0, 100.0) } else { 0.0 };
    if t <= 0.0 {
        return String::new();
    }
    let o = format!("{:.5}", 1.0 - t / 100.0);
    let o = o.trim_end_matches('0').trim_end_matches('.');
    let o = o.strip_prefix('0').filter(|r| !r.is_empty()).unwrap_or(o);
    format!(r#"<v:fill opacity="{o}"/>"#)
}

/// Whether `f` is a watermark ([`of`] reads it).
pub fn is(f: &Float) -> bool {
    kind(f).is_some()
}

/// Whether a font name survives a text watermark's VML: a quoted CSS `font-family` without its quotes, escapes or
/// separators, nothing the reader trims.
pub fn writable_family(name: &str) -> bool {
    name == name.trim()
        && !name.starts_with('\'')
        && !name.ends_with('\'')
        && !name.chars().any(|c| matches!(c, '"' | ';' | ',' | '\\') || c.is_control())
}

const V: &str = "urn:schemas-microsoft-com:vml";
const O: &str = "urn:schemas-microsoft-com:office:office";
const W10: &str = "urn:schemas-microsoft-com:office:word";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

const TEXT_TYPE: &str = concat!(
    r#"<v:shapetype id="_x0000_t136" coordsize="21600,21600" o:spt="136" adj="10800" path="m@7,l@8,m@5,21600l@6,21600e"><v:formulas>"#,
    r#"<v:f eqn="sum #0 0 10800"/><v:f eqn="prod #0 2 1"/><v:f eqn="sum 21600 0 @1"/><v:f eqn="sum 0 0 @2"/><v:f eqn="sum 21600 0 @3"/>"#,
    r#"<v:f eqn="if @0 @3 0"/><v:f eqn="if @0 21600 @1"/><v:f eqn="if @0 0 @2"/><v:f eqn="if @0 @4 21600"/><v:f eqn="mid @5 @6"/>"#,
    r#"<v:f eqn="mid @8 @5"/><v:f eqn="mid @7 @8"/><v:f eqn="mid @6 @7"/><v:f eqn="sum @6 0 @5"/></v:formulas><v:path textpathok="t" "#,
    r#"o:connecttype="custom" o:connectlocs="@9,0;@10,10800;@11,21600;@12,10800" o:connectangles="270,180,90,0"/><v:textpath on="t" "#,
    r##"fitshape="t"/><v:handles><v:h position="#0,bottomRight" xrange="6629,14971"/></v:handles><o:lock v:ext="edit" text="t" "##,
    r#"shapetype="t"/></v:shapetype>"#
);

const PICTURE_TYPE: &str = concat!(
    r#"<v:shapetype id="_x0000_t75" coordsize="21600,21600" o:spt="75" o:preferrelative="t" path="m@4@5l@4@11@9@11@9@5xe" filled="f" "#,
    r#"stroked="f"><v:stroke joinstyle="miter"/><v:formulas><v:f eqn="if lineDrawn pixelLineWidth 0"/><v:f eqn="sum @0 1 0"/>"#,
    r#"<v:f eqn="sum 0 0 @1"/><v:f eqn="prod @2 1 2"/><v:f eqn="prod @3 21600 pixelWidth"/><v:f eqn="prod @3 21600 pixelHeight"/>"#,
    r#"<v:f eqn="sum @0 0 1"/><v:f eqn="prod @6 1 2"/><v:f eqn="prod @7 21600 pixelWidth"/><v:f eqn="sum @8 21600 0"/>"#,
    r#"<v:f eqn="prod @7 21600 pixelHeight"/><v:f eqn="sum @10 21600 0"/></v:formulas><v:path o:extrusionok="f" gradientshapeok="t" "#,
    r#"o:connecttype="rect"/><o:lock v:ext="edit" aspectratio="t"/></v:shapetype>"#
);

/// Where Word's placement puts a watermark: centred on the margins, behind the text.
const PLACE: &str = "z-index:-251658752;mso-position-horizontal:center;mso-position-horizontal-relative:margin;mso-position-vertical:center;mso-position-vertical-relative:margin";

/// Points as VML writes them: at most two decimals.
fn pt(v: f64) -> String {
    let s = format!("{:.2}", v.max(0.0));
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s}pt")
}

/// The run content Word writes for a watermark `w` × `h` points (`w` the WordprocessingML prefix with its colon):
/// `w:rPr` and the `w:pict` with its shapetype and shape, `spid` its shape number and `image` the picture's
/// relationship id, `lang` the text's language (none when empty). The VML namespaces are declared on the `w:pict`.
pub fn vml(m: &Watermark, w: &str, size: [f64; 2], spid: u32, image: &str, lang: &str) -> String {
    let esc = super::write::props::esc;
    let (width, height) = (pt(size[0]), pt(size[1]));
    let (id, ty, body) = match m {
        Watermark::Text(t) => {
            let fill = if t.colour == SILVER { "silver".to_string() } else { format!("#{:06x}", t.colour & 0xFF_FFFF) };
            let opacity = opacity_xml(t.transparency);
            let rot = if t.diagonal { ";rotation:315" } else { "" };
            // The family is a CSS string: its quotes, escapes and separators can't be in it.
            let family: String = t.family.chars().filter(|c| !matches!(c, '"' | ';' | ',' | '\\') && !c.is_control()).collect();
            let size = t.size.map_or("1pt".to_string(), pt);
            let weight = if t.bold { ";font-weight:bold" } else { "" };
            let style = if t.italic { ";font-style:italic" } else { "" };
            let text: String = t.text.chars().map(|c| if c.is_control() { ' ' } else { c }).take(MAX_TEXT).collect();
            let body = format!(
                concat!(
                    r#"style="position:absolute;margin-left:0;margin-top:0;width:{width};height:{height}{rot};{place}" o:allowincell="f" "#,
                    r#"fillcolor="{fill}" stroked="f">{opacity}<v:textpath style="font-family:&quot;{family}&quot;;font-size:{size}{weight}{style}" "#,
                    r#"string="{text}"/>"#
                ),
                width = width,
                height = height,
                rot = rot,
                place = PLACE,
                fill = fill,
                opacity = opacity,
                family = esc(&family),
                size = size,
                weight = weight,
                style = style,
                text = esc(&text),
            );
            (TEXT_ID, TEXT_TYPE, (body, "_x0000_t136"))
        }
        Watermark::Picture(p) => {
            let wash = if p.washout { r#" gain="19661f" blacklevel="22938f""# } else { "" };
            let body = format!(
                r#"style="position:absolute;margin-left:0;margin-top:0;width:{width};height:{height};{PLACE}" o:allowincell="f"><v:imagedata r:id="{}" o:title=""{wash}/>"#,
                esc(image)
            );
            (PICTURE_ID, PICTURE_TYPE, (body, "_x0000_t75"))
        }
    };
    let (body, kind) = body;
    let number = 357_476_640 + u64::from(spid);
    let lang = if lang.is_empty() { String::new() } else { format!(r#"<{w}lang {w}val="{}"/>"#, esc(lang)) };
    format!(
        concat!(
            r#"<{w}rPr><{w}noProof/>{lang}</{w}rPr><{w}pict xmlns:v="{V}" xmlns:o="{O}" xmlns:w10="{W10}" xmlns:r="{R}">{ty}"#,
            r##"<v:shape id="{id}{number}" o:spid="_x0000_s{spid}" type="#{kind}" alt="" {body}<w10:wrap anchorx="margin" anchory="margin"/>"##,
            r#"</v:shape></{w}pict>"#
        ),
        w = w,
        lang = lang,
        V = V,
        O = O,
        W10 = W10,
        R = R,
        ty = ty,
        id = id,
        number = number,
        spid = spid,
        kind = kind,
        body = body,
    )
}

/// A watermark's shape as a float, as the reader makes it of [`vml`]'s markup (`image` the picture, which `vml` names
/// by `rid`): centred on the margins, behind the text, wrapping nothing.
pub fn float(m: &Watermark, size: [f64; 2], spid: u32, image: Option<usize>) -> Option<Float> {
    const WML: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let rid = "rIdWatermark";
    let xml = format!(r#"<w:r xmlns:w="{WML}">{}</w:r>"#, vml(m, "w:", size, spid, rid, ""));
    let dom = roxmltree::Document::parse(&xml).ok()?;
    let pict = dom.root_element().children().find(|c| c.tag_name().name() == "pict")?;
    struct One(&'static str, Option<usize>);
    impl crate::drawing::read::Source for One {
        fn image(&mut self, rid: &str) -> Option<usize> {
            if rid == self.0 { self.1 } else { None }
        }
        fn diagram(&mut self, _: &str) -> Option<crate::drawing::Shape> {
            None
        }
    }
    let mut src = One(rid, image);
    let mut ctx = crate::drawing::read::Ctx { source: &mut src, unknown_presets: Vec::new() };
    let v = crate::drawing::vml::read(pict, &mut ctx)?;
    let mut shape = v.shape;
    let emu = |p: f64| crate::drawing::units::Emu((p * 12700.0).round() as i64);
    shape.xfrm.ext = [emu(v.style.w), emu(v.style.h)];
    shape.nv.id = spid;
    let twips = |p: f64| (p * 20.0 + 0.5 + 1e-7).floor() / 20.0;
    Some(Float {
        id: super::next_para_id(),
        w: twips(v.style.w),
        auto_w: false,
        h: twips(v.style.h),
        image: shape.pic().and_then(|p| p.blip.image),
        alt: "".into(),
        lock_aspect: false,
        text_box: None,
        h_relative: crate::docx::HorizontalRelation::Margin,
        h_align: Some(crate::docx::HorizontalAlign::Center),
        h_offset: 0.0,
        v_relative: crate::docx::VerticalRelation::Margin,
        v_align: Some(crate::docx::VerticalAlign::Center),
        v_offset: 0.0,
        wrap: crate::docx::FloatWrap::None,
        wrap_text: crate::docx::WrapText::BothSides,
        dist: [0.0, 0.0, 9.0, 9.0],
        effect: [0.0; 4],
        poly: None,
        poly_edited: false,
        behind: true,
        allow_overlap: v.allow_overlap,
        layout_in_cell: v.allow_in_cell,
        order: usize::MAX / 2,
        z: v.style.z.unsigned_abs().min(u64::from(u32::MAX)) as u32,
        rot_swap: false,
        vml: true,
        relative: Default::default(),
        src: None,
        locked: false,
        frame_src: None,
        in_note: false,
        parts: Vec::new(),
        shape: Some(Box::new(shape)),
    })
}
