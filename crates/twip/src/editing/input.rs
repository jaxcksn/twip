//! What the editor takes from a host and tells it besides its state: pointer kinds, the viewport, notices and
//! requests. The `api` session turns them into its typed calls and events.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Down,
    Move,
    Up,
    Cancel,
}

/// The pointer's shape over the document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Cursor {
    /// The arrow.
    #[default]
    Default,
    /// The I-beam, over text.
    Text,
    /// The hand, over links.
    Pointer,
    /// Moving an object, or over a table's move handle.
    Move,
    /// Resizing a column, or an object from its left or right side.
    ResizeHorizontal,
    /// Resizing a row, or an object from its top or bottom.
    ResizeVertical,
    /// Resizing from the top-left or bottom-right corner.
    ResizeNorthWestSouthEast,
    /// Resizing from the top-right or bottom-left corner.
    ResizeNorthEastSouthWest,
    /// A crosshair.
    Crosshair,
    /// Nothing can happen here.
    NotAllowed,
    /// A dragged selection is copied.
    Copy,
    /// Selecting a cell: the black arrow pointing up-right.
    CellSelect,
    /// Selecting a column: the black arrow pointing down.
    ColumnSelect,
    /// Selecting rows, or lines from the margin: the white arrow pointing right.
    RowSelect,
}

/// The visible part of the document, in document points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub zoom: f64,
    pub dpr: f64,
}

impl Default for Viewport {
    fn default() -> Viewport {
        Viewport { left: 0.0, top: 0.0, width: 816.0, height: 1056.0, zoom: 1.0, dpr: 1.0 }
    }
}

/// Something the editor tells the application, besides requests.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    /// Design mode: an element was activated (a double-click, Enter).
    Activate(crate::docx::ElementId),
    /// Fields the application computes need results: their ids and codes, answered with the token.
    FieldUpdate { token: u64, fields: Vec<(crate::docx::ElementId, String)> },
    /// A link was Ctrl+clicked (or clicked, read only).
    Link { address: String, ctrl: bool },
    /// An unfilled picture placeholder was activated: its key.
    Placeholder(String),
}

/// A request for the host.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    ScrollIntoView {
        page: usize,
        rect: [f64; 4],
    },
    Autoscroll {
        vx: f64,
        vy: f64,
    },
    CapturePointer,
    ReleasePointer,
    Clipboard {
        text: String,
        html: Option<String>,
    },
    /// An unfilled placeholder was activated: the host supplies its content.
    Placeholder(super::placeholders::PlaceholderInfo),
}
