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
