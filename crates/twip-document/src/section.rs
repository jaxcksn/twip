//! Section and table values

/// Where content sits between a page's or a cell's top and bottom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VerticalAlignment {
    /// At the top.
    #[default]
    Top,
    /// Centred.
    Center,
    /// Spread from top to bottom.
    Both,
    /// At the bottom.
    Bottom,
}

/// The pages of a section that show its page borders.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PageBorderDisplay {
    /// Every page.
    #[default]
    AllPages,
    /// The first page only.
    FirstPage,
    /// Every page but the first.
    NotFirstPage,
}

/// How a table row's height is measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HeightRule {
    /// The content's height; the given height is ignored.
    Auto,
    /// Exactly the given height.
    Exact,
    /// At least the given height.
    AtLeast,
}
