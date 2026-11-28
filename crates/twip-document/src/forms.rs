//! Legacy form fields' settings and values

/// A legacy form field's settings and value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormData {
    /// The field's name, its bookmark's.
    pub name: String,
    /// `w:label` and `w:tabIndex`, which the engine keeps but doesn't use.
    pub label: Option<i64>,
    /// Its place in the tab order.
    pub tab_index: Option<i64>,
    /// It can be filled in.
    pub enabled: bool,
    /// Fields are calculated again on leaving it.
    pub calc_on_exit: bool,
    /// The macro run on entering it.
    pub entry_macro: String,
    /// The macro run on leaving it.
    pub exit_macro: String,
    /// The help shown on F1 (`w:helpText`) and in the status bar (`w:statusText`): own text, or an AutoText entry's.
    pub help: (bool, String),
    /// The status bar text, as [`Self::help`].
    pub status: (bool, String),
    /// What kind of field it is, with its kind's settings.
    pub kind: FormKind,
}

/// A legacy form field's kind, with its kind's settings.
#[derive(Clone, Debug, PartialEq)]
pub enum FormKind {
    /// FORMTEXT: `regular`, `number`, `date`, `currentDate`, `currentTime` or `calculated`; its default text, at most
    /// `max_length` characters (0: any), and its format.
    Text {
        /// The input type.
        input: String,
        /// The default text.
        default: String,
        /// The most characters it takes; 0 for any.
        max_length: u32,
        /// The format.
        format: String,
    },
    /// FORMCHECKBOX: its size in points (`None`: its text's), checked by default, checked now (`None`: as by default).
    CheckBox {
        /// The size in points; `None` for its text's.
        size: Option<f64>,
        /// Checked by default.
        default: bool,
        /// Checked now; `None` for as by default.
        checked: Option<bool>,
    },
    /// FORMDROPDOWN: its entries, the one by default and the one shown (`None`: the default).
    DropDown {
        /// The entries.
        entries: Vec<String>,
        /// The entry by default.
        default: u32,
        /// The entry shown; `None` for the default.
        result: Option<u32>,
    },
}

impl Default for FormKind {
    fn default() -> FormKind {
        FormKind::Text { input: "regular".into(), default: String::new(), max_length: 0, format: String::new() }
    }
}

impl FormData {
    /// A drop-down's shown entry.
    pub fn shown(&self) -> Option<String> {
        let FormKind::DropDown { entries, default, result } = &self.kind else { return None };
        let k = result.unwrap_or(*default) as usize;
        entries.get(k).or(entries.first()).cloned()
    }

    /// A check box shows checked.
    pub fn checked(&self) -> Option<bool> {
        match &self.kind {
            FormKind::CheckBox { default, checked, .. } => Some(checked.unwrap_or(*default)),
            _ => None,
        }
    }
}
