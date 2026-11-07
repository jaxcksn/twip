//! Host input, notices and requests

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Down,
    Move,
    Up,
    Cancel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Cursor {
    #[default]
    Default,
    Text,
    Pointer,
    Move,
    ResizeHorizontal,
    ResizeVertical,
    ResizeNorthWestSouthEast,
    ResizeNorthEastSouthWest,
    Crosshair,
    NotAllowed,
    Copy,
    CellSelect,
    ColumnSelect,
    RowSelect,
}

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

#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    Activate(crate::docx::ElementId),
    FieldUpdate { token: u64, fields: Vec<(crate::docx::ElementId, String)> },
    Link { address: String, ctrl: bool },
    PictureControl { control: crate::docx::ControlId },
    Refused { sub_template: Option<String>, element: Option<crate::docx::ElementId> },
    Warning(crate::docx::diagnostics::WarningCode, String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    ScrollIntoView { page: usize, rect: [f64; 4] },
    Autoscroll { vx: f64, vy: f64 },
    CapturePointer,
    ReleasePointer,
    Clipboard { text: String, html: Option<String> },
}
