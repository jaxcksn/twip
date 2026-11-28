//! Legacy form fields' settings and values

/// A legacy form field's settings and value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormData {
    pub name: String,
    /// `w:label` and `w:tabIndex`, which the engine keeps but doesn't use.
    pub label: Option<i64>,
    pub tab_index: Option<i64>,
    pub enabled: bool,
    pub calc_on_exit: bool,
    pub entry_macro: String,
    pub exit_macro: String,
    /// The help shown on F1 (`w:helpText`) and in the status bar (`w:statusText`): own text, or an AutoText entry's.
    pub help: (bool, String),
    pub status: (bool, String),
    pub kind: FormKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FormKind {
    /// FORMTEXT: `regular`, `number`, `date`, `currentDate`, `currentTime` or `calculated`; its default text, at most
    /// `max_length` characters (0: any), and its format.
    Text { input: String, default: String, max_length: u32, format: String },
    /// FORMCHECKBOX: its size in points (`None`: its text's), checked by default, checked now (`None`: as by default).
    CheckBox { size: Option<f64>, default: bool, checked: Option<bool> },
    /// FORMDROPDOWN: its entries, the one by default and the one shown (`None`: the default).
    DropDown { entries: Vec<String>, default: u32, result: Option<u32> },
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
