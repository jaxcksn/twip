//! Authored character formatting, independent of any file format: what one level (a run, a style, a list level, the
//! document defaults) sets, with unset, explicitly off and automatic kept apart and theme references kept as
//! references. [`resolve`] folds levels into the effective [`crate::docx::RunStyle`].

pub mod cascade;
pub mod counters;
mod list;
mod para;
pub mod resolve;
mod section;
mod style;
mod table;

pub use list::*;
pub use para::*;
pub use section::*;
pub use style::*;
pub use table::*;

use std::rc::Rc;

use crate::document::format::{Emphasis, ShadingPattern, VertAlign};
use crate::document::line::{BorderStyle, Underline};
use crate::docx::model::{TextEffects, ThemeColourRef};

/// Which part of the source a definition was read from: the document's, or its glossary's (a style or list only a
/// building block uses, kept beside the document's under an id that doesn't collide).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Origin {
    /// The document's.
    #[default]
    Document,
    /// The glossary's.
    Glossary,
}

/// Authored character properties at one level. `None` means "not set here". Its `==` is authored equality.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CharProps {
    /// The character style.
    pub style: Option<Rc<str>>,
    /// The font of each script class.
    pub fonts: ScriptFonts<Option<FontRef>>,
    /// The named East Asian font a theme reference at a later level replaced: Word uses it when that theme slot is
    /// empty. Only folding levels sets it.
    pub east_asian_below: Option<FontRef>,
    /// The East Asian theme reference a named font at a later level replaced. Only folding levels sets it.
    pub east_asian_theme: Option<FontRef>,
    /// Which script class ambiguous characters take.
    pub hint: Option<FontHint>,
    /// The size in points.
    pub size: Option<f64>,
    /// The complex-script size in points.
    pub complex_size: Option<f64>,
    /// The run is laid out in its complex-script properties.
    pub complex: Option<bool>,
    /// The on/off properties this level sets, and their values.
    pub toggles: Toggles,
    /// Double strikethrough.
    pub double_strike: Option<bool>,
    /// No proofing, and no automatic hyphenation.
    pub no_proof: Option<bool>,
    /// The underline and its colour.
    pub underline: Option<UnderlineProps>,
    /// The text colour.
    pub color: Option<Color>,
    /// The highlight: `Some(None)` is an explicit "no highlight".
    pub highlight: Option<Option<Highlight>>,
    /// The text's shading.
    pub shading: Option<Shading>,
    /// The text's border.
    pub border: Option<TextBorder>,
    /// Superscript, subscript or the baseline.
    pub vertical: Option<VertAlign>,
    /// Raised (+) or lowered (−), points.
    pub position: Option<f64>,
    /// Extra character spacing, points.
    pub spacing: Option<f64>,
    /// Horizontal scale, percent.
    pub scale: Option<f64>,
    /// Kern at or above this size (points); 0 is off.
    pub kern: Option<f64>,
    /// OpenType settings.
    pub typography: TypographyProps,
    /// Text effects: fill, outline, shadow, glow and reflection, each set or not on its own.
    pub effects: Option<Rc<TextEffects>>,
    /// The emphasis mark.
    pub emphasis: Option<Emphasis>,
    /// The languages of each script class.
    pub lang: LangProps,
    /// Right-to-left text.
    pub rtl: Option<bool>,
    /// A paragraph mark's: the mark, hidden, joins its paragraph to the next as a style separator.
    pub spec_vanish: Option<bool>,
    /// The width its region of text is fitted to (points) and the region's id.
    pub fit_text: Option<(f64, i64)>,
    /// What the source format wrote at this level that this struct does not model. Owned by the adapter that read
    /// it; never read by formatting or layout.
    pub foreign: Option<Rc<ForeignProps>>,
}

/// A property an edit sets, as a source format writes it: one written element or attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    /// An on/off property.
    Toggle(Toggle),
    /// Double strikethrough.
    DoubleStrike,
    /// No proofing.
    NoProof,
    /// Laid out as complex script.
    Complex,
    /// Right-to-left.
    Rtl,
    /// The size.
    Size,
    /// The complex-script size.
    ComplexSize,
    /// The underline, its line and colour.
    Underline,
    /// The underline's colour alone.
    UnderlineColor,
    /// The text colour.
    Color,
    /// The highlight.
    Highlight,
    /// The shading.
    Shading,
    /// The border.
    Border,
    /// Superscript or subscript.
    Vertical,
    /// The raise or lowering.
    Position,
    /// The character spacing.
    Spacing,
    /// The horizontal scale.
    Scale,
    /// Kerning.
    Kern,
    /// The font of a script class.
    Font(ScriptClass),
    /// The script class ambiguous characters take.
    Hint,
    /// The character style.
    Style,
    /// The language of a script class.
    Lang(ScriptClass),
    /// Fit Text.
    FitText,
    /// The emphasis mark.
    Emphasis,
    /// Ligatures.
    Ligatures,
    /// Contextual alternates.
    ContextualAlternates,
    /// The figures' form.
    NumberForm,
    /// The figures' spacing.
    NumberSpacing,
    /// Stylistic sets.
    StylisticSets,
    /// A text effect.
    Effect(Effect),
    /// A paragraph mark's style separator flag.
    SpecVanish,
}

/// One of the text effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// The letters' fill.
    Fill,
    /// Their outline.
    Outline,
    /// Their shadow.
    Shadow,
    /// Their glow.
    Glow,
    /// Their reflection.
    Reflection,
}

/// A property an edit set: whole (written anew, whatever it was), or in part (among what is written with it, which
/// stays as written).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Set {
    /// The property.
    pub prop: Prop,
    /// Set whole.
    pub whole: bool,
}

/// Authored properties being edited, with what the edit set, in the order it set it: a source format writes what was
/// set again, the rest as it was written.
#[derive(Clone, Debug, Default)]
pub struct CharEdit {
    /// The properties.
    pub props: CharProps,
    /// What was set, in order.
    pub set: Vec<Set>,
}

impl CharEdit {
    /// An edit of `props`.
    pub fn new(props: CharProps) -> CharEdit {
        CharEdit { props, set: Vec::new() }
    }

    /// Notes `prop` set whole.
    pub fn whole(&mut self, prop: Prop) {
        self.set.push(Set { prop, whole: true });
    }

    /// Notes `prop` set in part.
    pub fn part(&mut self, prop: Prop) {
        self.set.push(Set { prop, whole: false });
    }

    /// Sets on/off property `t` whole.
    pub fn toggle(&mut self, t: Toggle, on: bool) {
        self.props.toggles.put(t, on);
        self.whole(Prop::Toggle(t));
    }
}

impl std::ops::Deref for CharEdit {
    type Target = CharProps;
    fn deref(&self) -> &CharProps {
        &self.props
    }
}

impl std::ops::DerefMut for CharEdit {
    fn deref_mut(&mut self) -> &mut CharProps {
        &mut self.props
    }
}

/// One value per script class. `latin_extended` is the slot for Latin text beyond ASCII.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScriptFonts<T> {
    /// Latin text in the ASCII range.
    pub latin: T,
    /// Other Latin text.
    pub latin_extended: T,
    /// East Asian text.
    pub east_asian: T,
    /// Complex-script text.
    pub complex: T,
}

/// A script class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptClass {
    /// Latin text in the ASCII range.
    Latin,
    /// Other Latin text.
    LatinExtended,
    /// East Asian text.
    EastAsian,
    /// Complex-script text.
    Complex,
}

/// A theme's two font sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeFace {
    /// Headings.
    Major,
    /// Body text.
    Minor,
    /// A set the file named that is neither; resolves as the body set, and is no theme font.
    Unrecognised,
}

/// A font: a family by name, or a theme font, which follows the theme.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontRef {
    /// A family by name.
    Family(Rc<str>),
    /// The theme's font of `face` for `script`.
    Theme {
        /// Which set.
        face: ThemeFace,
        /// Which script's font of the set.
        script: ScriptClass,
    },
}

/// Which script class characters that could be either take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontHint {
    /// By their own script.
    Default,
    /// East Asian.
    EastAsia,
    /// Complex script.
    Complex,
}

/// The on/off properties a level sets (`set`) and their values (`on`), as bits of [`Toggle`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Toggles {
    /// The toggles this level sets.
    pub set: u16,
    /// Their values; a bit outside `set` is off.
    pub on: u16,
}

/// One on/off property's bit in [`Toggles`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Toggle(pub u16);

impl Toggle {
    /// Bold.
    pub const BOLD: Toggle = Toggle(1);
    /// Italic.
    pub const ITALIC: Toggle = Toggle(1 << 1);
    /// All capitals.
    pub const CAPS: Toggle = Toggle(1 << 2);
    /// Small capitals.
    pub const SMALL_CAPS: Toggle = Toggle(1 << 3);
    /// Strikethrough.
    pub const STRIKE: Toggle = Toggle(1 << 4);
    /// Hidden.
    pub const HIDDEN: Toggle = Toggle(1 << 5);
    /// Complex-script bold.
    pub const BOLD_COMPLEX: Toggle = Toggle(1 << 6);
    /// Complex-script italic.
    pub const ITALIC_COMPLEX: Toggle = Toggle(1 << 7);
    /// Embossed.
    pub const EMBOSS: Toggle = Toggle(1 << 8);
    /// Engraved.
    pub const IMPRINT: Toggle = Toggle(1 << 9);
    /// Outlined.
    pub const OUTLINE: Toggle = Toggle(1 << 10);
    /// Shadowed.
    pub const SHADOW: Toggle = Toggle(1 << 11);
    /// Every toggle.
    pub const ALL: [Toggle; 12] = [
        Toggle::BOLD,
        Toggle::ITALIC,
        Toggle::CAPS,
        Toggle::SMALL_CAPS,
        Toggle::STRIKE,
        Toggle::HIDDEN,
        Toggle::BOLD_COMPLEX,
        Toggle::ITALIC_COMPLEX,
        Toggle::EMBOSS,
        Toggle::IMPRINT,
        Toggle::OUTLINE,
        Toggle::SHADOW,
    ];
}

impl Toggles {
    /// What this level sets `t` to.
    pub fn get(&self, t: Toggle) -> Option<bool> {
        (self.set & t.0 != 0).then_some(self.on & t.0 != 0)
    }

    /// Whether `t` is on.
    pub fn is_on(&self, t: Toggle) -> bool {
        self.on & t.0 != 0
    }

    /// Sets `t` to `on` at this level.
    pub fn put(&mut self, t: Toggle, on: bool) {
        self.set |= t.0;
        if on {
            self.on |= t.0;
        } else {
            self.on &= !t.0;
        }
    }
}

/// An authored colour keeps how it was chosen; the effective colour is always RGB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    /// Automatic.
    Auto,
    /// `0xRRGGBB`.
    Rgb(u32),
    /// A theme colour, with its tint or shade.
    Theme(ThemeColourRef),
}

/// An underline: its line and colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnderlineProps {
    /// The line; [`Underline::None`] turns an inherited underline off.
    pub style: Underline,
    /// Its own colour; `None`: the text's.
    pub color: Option<Color>,
}

/// One of the highlight colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Highlight {
    /// Yellow.
    Yellow,
    /// Green.
    Green,
    /// Cyan.
    Cyan,
    /// Magenta.
    Magenta,
    /// Blue.
    Blue,
    /// Red.
    Red,
    /// Dark blue.
    DarkBlue,
    /// Dark cyan.
    DarkCyan,
    /// Dark green.
    DarkGreen,
    /// Dark magenta.
    DarkMagenta,
    /// Dark red.
    DarkRed,
    /// Dark yellow.
    DarkYellow,
    /// Dark gray.
    DarkGray,
    /// Light gray.
    LightGray,
    /// Black.
    Black,
    /// White.
    White,
}

impl Highlight {
    /// Every highlight colour.
    pub const ALL: [Highlight; 16] = {
        use Highlight::*;
        [Yellow, Green, Cyan, Magenta, Blue, Red, DarkBlue, DarkCyan, DarkGreen, DarkMagenta, DarkRed, DarkYellow, DarkGray, LightGray, Black, White]
    };

    /// Its colour, `0xRRGGBB`.
    pub fn rgb(self) -> u32 {
        use Highlight::*;
        match self {
            Yellow => 0xFFFF00,
            Green => 0x00FF00,
            Cyan => 0x00FFFF,
            Magenta => 0xFF00FF,
            Blue => 0x0000FF,
            Red => 0xFF0000,
            DarkBlue => 0x00008B,
            DarkCyan => 0x008B8B,
            DarkGreen => 0x006400,
            DarkMagenta => 0x800080,
            DarkRed => 0x8B0000,
            DarkYellow => 0x808000,
            DarkGray => 0xA9A9A9,
            LightGray => 0xD3D3D3,
            Black => 0x000000,
            White => 0xFFFFFF,
        }
    }
}

/// Shading: a pattern in a colour over a fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shading {
    /// The pattern.
    pub pattern: ShadingPattern,
    /// The pattern's colour; `None`: automatic.
    pub color: Option<Color>,
    /// The fill; `None`: automatic.
    pub fill: Option<Color>,
}

/// A border around text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextBorder {
    /// The line.
    pub style: BorderStyle,
    /// The line's width, points.
    pub width: f64,
    /// The space between the line and the text, points.
    pub space: f64,
    /// Drawn with a shadow.
    pub shadow: bool,
    /// Its colour; `None`: automatic.
    pub color: Option<Color>,
}

/// OpenType settings, each set or not on its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TypographyProps {
    /// Ligature classes: 1 standard, 2 contextual, 4 historical, 8 discretionary.
    pub ligatures: Option<u8>,
    /// Contextual alternates.
    pub contextual_alternates: Option<bool>,
    /// Lining or old-style figures.
    pub number_form: Option<NumberForm>,
    /// Proportional or tabular figures.
    pub number_spacing: Option<NumberSpacing>,
    /// Stylistic sets: bit n − 1 for set n.
    pub stylistic_sets: Option<u32>,
}

/// The figures' form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberForm {
    /// The font's default.
    Default,
    /// Lining figures.
    Lining,
    /// Old-style figures.
    OldStyle,
}

impl NumberForm {
    /// Its OpenType feature bit: 1 `lnum`, 2 `onum`, 0 none.
    pub fn bits(self) -> u32 {
        match self {
            NumberForm::Default => 0,
            NumberForm::Lining => 1,
            NumberForm::OldStyle => 2,
        }
    }
}

/// The figures' spacing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberSpacing {
    /// The font's default.
    Default,
    /// Proportional figures.
    Proportional,
    /// Tabular figures.
    Tabular,
}

impl NumberSpacing {
    /// Its OpenType feature bit: 4 `pnum`, 8 `tnum`, 0 none.
    pub fn bits(self) -> u32 {
        match self {
            NumberSpacing::Default => 0,
            NumberSpacing::Proportional => 4,
            NumberSpacing::Tabular => 8,
        }
    }
}

/// The language of each script class's text, as BCP 47 tags.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LangProps {
    /// Latin text.
    pub latin: Option<Rc<str>>,
    /// East Asian text.
    pub east_asian: Option<Rc<str>>,
    /// Complex-script text.
    pub complex: Option<Rc<str>>,
}

/// A name in a namespace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QName {
    /// The namespace; empty for none.
    pub ns: Rc<str>,
    /// The local name.
    pub local: Rc<str>,
}

/// What a properties element held that the typed struct does not model, in source order. Its content is the source
/// format's own, read and written only by the adapter that defines it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ForeignProps {
    /// Whole unmodelled child elements.
    pub children: Vec<ForeignChild>,
    /// Unmodelled attributes of modelled children, and of the element itself, and modelled attributes as written
    /// where that differs from how their value is written.
    pub attributes: Vec<ForeignAttr>,
    /// Modelled children as written, beside their typed value, where the typed value may not hold all they hold.
    pub kept: Vec<ForeignChild>,
}

/// An unmodelled child element, as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignChild {
    /// Its expanded name.
    pub name: QName,
    /// The element as written.
    pub payload: Rc<str>,
    /// Prefix (empty: the default namespace) and namespace of every binding the payload's names use.
    pub bindings: Rc<[(Rc<str>, Rc<str>)]>,
}

/// An unmodelled attribute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignAttr {
    /// The modelled child it is on; `None`: the properties element itself.
    pub on: Option<QName>,
    /// Its expanded name.
    pub name: QName,
    /// The prefix it was written with.
    pub prefix: Rc<str>,
    /// Its value, unescaped.
    pub value: Rc<str>,
    /// The value, as the adapter writes it, that this text was read as: the text is written in its place while
    /// that is still the value. `None` for an unmodelled attribute.
    pub stands_for: Option<Rc<str>>,
    /// The modelled attribute beside it that decided the value in its place: it is written while that one is.
    pub beside: Option<Rc<str>>,
}

impl CharProps {
    /// Overrides everything `o` sets; toggles are absolute (direct formatting, or within one style chain).
    /// `foreign` stays this level's.
    pub fn merge(&mut self, o: &CharProps) {
        let family = |f: &Option<FontRef>| matches!(f, Some(FontRef::Family(_)));
        let theme = |f: &Option<FontRef>| matches!(f, Some(FontRef::Theme { .. }));
        let (mine, theirs) = (&self.fonts.east_asian, &o.fonts.east_asian);
        let named = family(mine) || self.east_asian_below.is_some();
        if theme(mine) && family(theirs) && self.east_asian_below.is_none() {
            self.east_asian_theme = mine.clone();
        } else if theirs.is_some() {
            self.east_asian_theme = None;
        }
        if o.east_asian_theme.is_some() && !named {
            self.east_asian_theme = o.east_asian_theme.clone();
        }
        let mine = &self.fonts.east_asian;
        if family(mine) && theme(theirs) {
            self.east_asian_below = mine.clone();
        } else if family(theirs) {
            self.east_asian_below = None;
        }
        if o.east_asian_below.is_some() {
            self.east_asian_below = o.east_asian_below.clone();
        }
        macro_rules! take {
            ($($f:ident).+) => { if o.$($f).+.is_some() { self.$($f).+ = o.$($f).+.clone(); } };
        }
        take!(fonts.latin);
        take!(fonts.latin_extended);
        take!(fonts.east_asian);
        take!(fonts.complex);
        take!(hint);
        take!(size);
        take!(complex_size);
        take!(complex);
        take!(double_strike);
        take!(no_proof);
        take!(underline);
        take!(color);
        take!(highlight);
        take!(shading);
        take!(border);
        take!(vertical);
        take!(position);
        take!(spacing);
        take!(scale);
        take!(kern);
        take!(typography.ligatures);
        take!(typography.contextual_alternates);
        take!(typography.number_form);
        take!(typography.number_spacing);
        take!(typography.stylistic_sets);
        take!(lang.latin);
        take!(lang.east_asian);
        take!(lang.complex);
        take!(style);
        take!(rtl);
        take!(spec_vanish);
        take!(emphasis);
        take!(fit_text);
        match (&mut self.effects, &o.effects) {
            (_, None) => {}
            (None, Some(e)) => self.effects = Some(e.clone()),
            (Some(mine), Some(e)) => Rc::make_mut(mine).merge(e),
        }
        let t = &mut self.toggles;
        t.on = (t.on & !o.toggles.set) | (o.toggles.on & o.toggles.set);
        t.set |= o.toggles.set;
    }

    /// Adds a style chain's level: its values override, and each toggle it turns on flips the one below.
    pub fn merge_level(&mut self, level: &CharProps) {
        let flip = level.toggles.on & level.toggles.set;
        let toggles = self.toggles;
        self.merge(level);
        self.toggles.on = toggles.on ^ flip;
        self.toggles.set = toggles.set | flip;
    }

    /// One style chain's levels, root first, folded: the nearest level decides each toggle, and only toggles that
    /// end on count.
    pub fn fold_chain<'a>(chain: impl IntoIterator<Item = &'a CharProps>) -> CharProps {
        let mut acc = CharProps::default();
        for level in chain {
            acc.merge(level);
        }
        let on = acc.toggles.on & acc.toggles.set;
        acc.toggles.on = on;
        acc.toggles.set = on;
        acc
    }
}
