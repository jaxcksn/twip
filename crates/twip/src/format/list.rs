//! Authored list definitions, independent of any file format: the definitions a document's lists number with, the
//! lists themselves with what each overrides, and the pictures bullets draw. `None` means "not set".

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use super::{CharProps, ForeignProps, Origin, ParaProps, ParaSet, Set};
use crate::document::paragraph::{ListId, ListJc, ListSuffix};

/// A list definition, by the id its source format gives it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ListDefId(pub i64);

/// A value a source format names: one this model knows, or a name it doesn't, kept as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Literal<T> {
    /// A value this model knows.
    Known(T),
    /// A name this model doesn't know, as written; it means what the format's default for unknown names means.
    Other(Rc<str>),
}

impl<T: Copy> Literal<T> {
    /// The known value; `None` for a name this model doesn't know.
    pub fn known(&self) -> Option<T> {
        match self {
            Literal::Known(v) => Some(*v),
            Literal::Other(_) => None,
        }
    }
}

macro_rules! number_formats {
    ($($(#[doc = $doc:literal])* $name:ident,)*) => {
        /// How a list level writes its number.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum NumberFormat {
            $($(#[doc = $doc])* $name,)*
        }

        impl NumberFormat {
            /// Every number format.
            pub const ALL: &'static [NumberFormat] = &[$(NumberFormat::$name,)*];
        }
    };
}

number_formats! {
    /// 1, 2, 3.
    Decimal,
    /// I, II, III.
    UpperRoman,
    /// i, ii, iii.
    LowerRoman,
    /// A, B, C.
    UpperLetter,
    /// a, b, c.
    LowerLetter,
    /// 1st, 2nd, 3rd.
    Ordinal,
    /// One, Two, Three.
    CardinalText,
    /// First, Second, Third.
    OrdinalText,
    /// Hexadecimal.
    Hex,
    /// *, †, ‡, §, then doubled.
    Chicago,
    /// Ideographic digits.
    IdeographDigital,
    /// Japanese counting.
    JapaneseCounting,
    /// Japanese aiueo order.
    Aiueo,
    /// Japanese iroha order.
    Iroha,
    /// Full-width decimal.
    DecimalFullWidth,
    /// Half-width decimal.
    DecimalHalfWidth,
    /// Japanese legal.
    JapaneseLegal,
    /// Japanese digital ten thousand.
    JapaneseDigitalTenThousand,
    /// Decimal in a circle.
    DecimalEnclosedCircle,
    /// Full-width decimal, alternate.
    DecimalFullWidth2,
    /// Full-width aiueo order.
    AiueoFullWidth,
    /// Full-width iroha order.
    IrohaFullWidth,
    /// 01, 02, 03.
    DecimalZero,
    /// A bullet: the level's text in its font, or its picture.
    Bullet,
    /// Korean ganada order.
    Ganada,
    /// Korean chosung order.
    Chosung,
    /// Decimal with a full stop.
    DecimalEnclosedFullstop,
    /// Decimal in parentheses.
    DecimalEnclosedParen,
    /// Decimal in a circle, Chinese.
    DecimalEnclosedCircleChinese,
    /// Ideographs in a circle.
    IdeographEnclosedCircle,
    /// Traditional ideographs.
    IdeographTraditional,
    /// Zodiac ideographs.
    IdeographZodiac,
    /// Traditional zodiac ideographs.
    IdeographZodiacTraditional,
    /// Taiwanese counting.
    TaiwaneseCounting,
    /// Traditional legal ideographs.
    IdeographLegalTraditional,
    /// Taiwanese counting thousand.
    TaiwaneseCountingThousand,
    /// Taiwanese digital.
    TaiwaneseDigital,
    /// Chinese counting.
    ChineseCounting,
    /// Simplified Chinese legal.
    ChineseLegalSimplified,
    /// Chinese counting thousand.
    ChineseCountingThousand,
    /// Korean digital.
    KoreanDigital,
    /// Korean counting.
    KoreanCounting,
    /// Korean legal.
    KoreanLegal,
    /// Korean digital, alternate.
    KoreanDigital2,
    /// Vietnamese counting.
    VietnameseCounting,
    /// а, б, в.
    RussianLower,
    /// А, Б, В.
    RussianUpper,
    /// No number.
    None,
    /// - 1 -, - 2 -, - 3 -.
    NumberInDash,
    /// Hebrew letters.
    Hebrew1,
    /// Hebrew alphabet.
    Hebrew2,
    /// Arabic alphabet.
    ArabicAlpha,
    /// Arabic abjad.
    ArabicAbjad,
    /// क, ख, ग.
    HindiVowels,
    /// अ, आ, इ.
    HindiConsonants,
    /// १, २, ३.
    HindiNumbers,
    /// Hindi counting.
    HindiCounting,
    /// ก, ข, ค.
    ThaiLetters,
    /// ๑, ๒, ๓.
    ThaiNumbers,
    /// Thai counting.
    ThaiCounting,
    /// Thai baht text.
    BahtText,
    /// Dollar text.
    DollarText,
    /// A custom format, which [`ListLevel::format_code`] gives.
    Custom,
}

/// How a definition's levels relate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiLevelType {
    /// One level.
    SingleLevel,
    /// Several levels.
    Multilevel,
    /// Several levels, each its own single-level list.
    HybridMultilevel,
}

/// A level's label as earlier word processors placed it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LegacyLabel {
    /// Whether the earlier placement applies.
    pub on: Option<bool>,
    /// The space after the label, points.
    pub space: Option<f64>,
    /// The label's indent, points.
    pub indent: Option<f64>,
}

/// One level of a list definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListLevel {
    /// The number the level starts at.
    pub start: Option<i64>,
    /// How the number is written.
    pub format: Option<Literal<NumberFormat>>,
    /// A custom format's code.
    pub format_code: Option<Rc<str>>,
    /// The label text, with `%1` to `%9` for the levels' numbers.
    pub text: Option<Rc<str>>,
    /// The label's alignment at the number position.
    pub jc: Option<Literal<ListJc>>,
    /// What follows the label.
    pub suffix: Option<Literal<ListSuffix>>,
    /// Every level's number in the label is written in Arabic numerals.
    pub legal: Option<bool>,
    /// The level (1-based) after whose items this one restarts; 0: never.
    pub restart: Option<i64>,
    /// The paragraph style that puts paragraphs at this level.
    pub style: Option<Rc<str>>,
    /// The picture bullet the label draws.
    pub pic_bullet: Option<i64>,
    /// The label as earlier word processors placed it.
    pub legacy: Option<LegacyLabel>,
    /// The template code the user interface identifies the level with.
    pub template: Option<u32>,
    /// The level is not yet shown in the user interface.
    pub tentative: Option<bool>,
    /// The paragraph properties of the level's paragraphs.
    pub para: ParaProps,
    /// The label's character properties.
    pub char: CharProps,
    /// What the source format wrote that this struct does not model. Owned by the adapter that read it.
    pub foreign: Option<Rc<ForeignProps>>,
}

/// A list definition: its levels, and how it relates to numbering styles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListDef {
    /// Its id.
    pub id: ListDefId,
    /// The id that identifies it across documents.
    pub nsid: Option<u32>,
    /// How its levels relate.
    pub multi_level_type: Option<Literal<MultiLevelType>>,
    /// The template code the user interface identifies it with.
    pub template: Option<u32>,
    /// Its name.
    pub name: Option<Rc<str>>,
    /// The numbering style this definition is.
    pub style_link: Option<Rc<str>>,
    /// The numbering style whose list this definition numbers with in its place.
    pub num_style_link: Option<Rc<str>>,
    /// Its levels, by index; `None`: no such level.
    pub levels: [Option<Rc<ListLevel>>; 9],
    /// The part it was read from.
    pub origin: Origin,
    /// What the source format wrote that this struct does not model. Owned by the adapter that read it.
    pub foreign: Option<Rc<ForeignProps>>,
}

/// What a list overrides of one level of its definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LevelOverride {
    /// The level, as written: a level past the definition's is kept, and never numbers.
    pub level: i64,
    /// The number the level starts at the first time the list reaches it.
    pub start: Option<i64>,
    /// The level the list numbers with in place of its definition's.
    pub def: Option<Rc<ListLevel>>,
    /// What the source format wrote that this struct does not model. Owned by the adapter that read it.
    pub foreign: Option<Rc<ForeignProps>>,
}

/// A list: the definition it numbers with, and what it overrides of it.
#[derive(Clone, Debug, PartialEq)]
pub struct List {
    /// Its id.
    pub id: ListId,
    /// The definition it numbers with.
    pub def: Option<ListDefId>,
    /// What it overrides, one per level, in level order.
    pub overrides: Vec<LevelOverride>,
    /// The part it was read from.
    pub origin: Origin,
    /// What the source format wrote that this struct does not model. Owned by the adapter that read it.
    pub foreign: Option<Rc<ForeignProps>>,
}

impl List {
    /// What it overrides of level `level`.
    pub fn override_of(&self, level: i64) -> Option<&LevelOverride> {
        self.overrides.binary_search_by_key(&level, |o| o.level).ok().map(|k| &self.overrides[k])
    }
}

/// A picture a bullet level draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PictureBullet {
    /// Its id.
    pub id: i64,
    /// The relationship its image is read through, as its picture names it.
    pub rel: Option<Rc<str>>,
    /// Its image's index in the document's images, once read.
    pub image: Option<usize>,
    /// The picture, as the source format wrote it. Owned by the adapter that read it.
    pub foreign: Option<Rc<ForeignProps>>,
}

/// A document's list definitions, lists and picture bullets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lists {
    /// The definitions, by id.
    pub defs: HashMap<ListDefId, Rc<ListDef>>,
    /// The lists, by id.
    pub lists: HashMap<ListId, Rc<List>>,
    /// The picture bullets, by id.
    pub pictures: BTreeMap<i64, Rc<PictureBullet>>,
}

impl Lists {
    /// The definition `list` numbers with. A definition linked to a numbering style numbers with the definition of
    /// that style's list in its place, once: `linked` gives the list a numbering style puts paragraphs in.
    pub fn def_of(&self, list: ListId, linked: impl Fn(&str) -> Option<ListId>) -> Option<ListDefId> {
        let def = self.lists.get(&list)?.def?;
        if let Some(link) = self.defs.get(&def).and_then(|d| d.num_style_link.as_deref()) {
            let other = linked(link)?;
            if other != list {
                return self.lists.get(&other)?.def;
            }
        }
        Some(def)
    }

    /// The level `level` (clamped to 0 to 8) of `list` numbers with: the list's override, else its definition's.
    pub fn level(&self, list: ListId, level: i64, linked: impl Fn(&str) -> Option<ListId>) -> Option<&Rc<ListLevel>> {
        let level = level.clamp(0, 8);
        if let Some(l) = self.lists.get(&list).and_then(|n| n.override_of(level)).and_then(|o| o.def.as_ref()) {
            return Some(l);
        }
        let def = self.def_of(list, linked)?;
        self.defs.get(&def)?.levels[level as usize].as_ref()
    }

    /// The nine levels `list` numbers with: its definition's, with the list's overrides.
    pub fn levels(&self, list: ListId, linked: impl Fn(&str) -> Option<ListId>) -> Option<[Option<&Rc<ListLevel>>; 9]> {
        let def = self.defs.get(&self.def_of(list, linked)?)?;
        let own = self.lists.get(&list);
        Some(std::array::from_fn(|k| own.and_then(|n| n.override_of(k as i64)).and_then(|o| o.def.as_ref()).or(def.levels[k].as_ref())))
    }

    /// The image a bullet level draws: its picture bullet's.
    pub fn bullet_image(&self, level: &ListLevel) -> Option<usize> {
        (level.format.as_ref().and_then(Literal::known) == Some(NumberFormat::Bullet)).then_some(())?;
        self.pictures.get(&level.pic_bullet?)?.image
    }

    /// A free definition id and list id.
    pub fn free_ids(&self) -> (ListDefId, ListId) {
        let def = self.defs.keys().max().map_or(0, |m| m.0 + 1);
        let list = self.lists.keys().max().map_or(1, |m| m.0 + 1).max(1);
        (ListDefId(def), ListId(list))
    }
}

/// A definition property an edit sets, written again over the definition as read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListDefProp {
    /// The cross-document id.
    Nsid,
    /// How the levels relate.
    MultiLevelType,
    /// The template code.
    Template,
    /// The name.
    Name,
    /// The numbering style it is.
    StyleLink,
    /// The numbering style it numbers with.
    NumStyleLink,
    /// A property of a level, by index.
    Level(u8, LevelProp),
}

/// A level property an edit sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelProp {
    /// The whole level, written anew.
    Whole,
    /// The start.
    Start,
    /// The number format and its code.
    Format,
    /// The label text.
    Text,
    /// The label's alignment.
    Jc,
    /// What follows the label.
    Suffix,
    /// Legal numbering.
    Legal,
    /// The restart.
    Restart,
    /// The paragraph style.
    Style,
    /// The picture bullet.
    PicBullet,
    /// The earlier placement.
    Legacy,
    /// The template code.
    Template,
    /// The tentative flag.
    Tentative,
    /// A paragraph property.
    Para(ParaSet),
    /// A character property of the label.
    Char(Set),
}

/// A list property an edit sets, written again over the list as read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListProp {
    /// The definition it numbers with.
    Def,
    /// A property of what it overrides of a level.
    Override(i64, OverrideProp),
}

/// A level override's property an edit sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverrideProp {
    /// The whole override, written anew.
    Whole,
    /// The start.
    Start,
    /// A property of the level it defines.
    Level(LevelProp),
}
