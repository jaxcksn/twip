//! The context menu: what the selection is on, and the items for it, each with its state from the command states.
//! Spelling suggestions come first on a misspelling; then the clipboard, then the table, object, link, review and
//! header and footer actions.

use crate::api::catalog;
use crate::api::command::{Command, CommentResolve, HorizontalSide, InsertColumns, InsertRows, PasteOptions, VerticalSide};
use crate::api::services::{ContextMenu, MenuAction, MenuItem, MenuTarget};
use crate::api::state::{ObjectKind, State};
use crate::api::value::Story;

/// A misspelling under the selection, with its suggestions.
pub struct Misspelled {
    pub word: String,
    pub language: String,
    pub repeated: bool,
    pub suggestions: Vec<String>,
}

/// What the menu is for: the misspelling, revision, comment, element, link, object, table or header and footer the
/// selection is in, else text.
fn target(state: &State, misspelled: Option<&Misspelled>) -> MenuTarget {
    let s = &state.selection;
    if let Some(m) = misspelled {
        return MenuTarget::Misspelling { word: m.word.clone(), language: m.language.clone() };
    }
    if let Some(id) = &s.revision {
        return MenuTarget::Revision { id: id.clone() };
    }
    if let Some(id) = &s.comment {
        return MenuTarget::Comment { id: id.clone() };
    }
    if let Some(e) = s.context.first() {
        return MenuTarget::Element { id: e.id.clone() };
    }
    if let Some(l) = &s.link {
        return MenuTarget::Link { address: l.address.clone() };
    }
    match s.object.as_ref().map(|o| o.kind) {
        Some(ObjectKind::Picture) => return MenuTarget::Picture,
        Some(ObjectKind::TextBox) => return MenuTarget::TextBox,
        None => {}
    }
    if s.table.is_some() {
        return MenuTarget::Table;
    }
    if matches!(s.story, Story::Header | Story::Footer) {
        return MenuTarget::HeaderFooter;
    }
    MenuTarget::Text
}

/// A command item, enabled and checked as the command's state says. A command the state doesn't list yet is enabled
/// when the session can run it (`needs_selection`: only with something selected).
fn item(state: &State, label: &str, command: Command, needs_selection: bool) -> MenuItem {
    let id = command.id();
    let fallback = (!state.read_only || !command.mutates()) && (!needs_selection || !state.selection.empty);
    let (enabled, checked) =
        state.commands.get(&id).map_or((fallback, None), |c| (c.enabled && (!needs_selection || !state.selection.empty), c.checked));
    let shortcut = catalog::info(id).and_then(|c| c.shortcuts.windows.first().cloned());
    MenuItem::Command { command: id, label: label.into(), enabled, checked, shortcut, action: MenuAction::Command { command } }
}

/// The context menu for the selection.
pub fn build(state: &State, misspelled: Option<&Misspelled>) -> ContextMenu {
    let target = target(state, misspelled);
    let mut items = Vec::new();
    if let Some(m) = misspelled {
        if m.repeated {
            items.push(MenuItem::Suggestion { label: "Delete Repeated Word".into(), action: MenuAction::ReplaceWord { with: String::new() } });
        } else {
            for s in m.suggestions.iter().take(5) {
                items.push(MenuItem::Suggestion { label: s.clone(), action: MenuAction::ReplaceWord { with: s.clone() } });
            }
            items.push(MenuItem::Suggestion { label: "Ignore".into(), action: MenuAction::IgnoreWord { word: m.word.clone() } });
            items.push(MenuItem::Suggestion {
                label: "Add to Dictionary".into(),
                action: MenuAction::AddWord { word: m.word.clone(), language: m.language.clone() },
            });
        }
        items.push(MenuItem::Separator);
    }
    items.push(item(state, "Cut", Command::Cut, true));
    items.push(item(state, "Copy", Command::Copy, true));
    items.push(item(state, "Paste", Command::Paste(PasteOptions::default()), false));
    let s = &state.selection;
    let group = |list: Vec<MenuItem>, items: &mut Vec<MenuItem>| {
        if !list.is_empty() {
            items.push(MenuItem::Separator);
            items.extend(list);
        }
    };
    if let Some(t) = &s.table {
        let rows = |side, label| item(state, label, Command::InsertRows(InsertRows { side, count: None }), false);
        let cols = |side, label| item(state, label, Command::InsertColumns(InsertColumns { side, count: None }), false);
        let mut merge = item(state, "Merge Cells", Command::MergeCells, false);
        if let MenuItem::Command { enabled, .. } = &mut merge {
            *enabled &= t.can_merge;
        }
        group(
            vec![
                rows(VerticalSide::Above, "Insert Rows Above"),
                rows(VerticalSide::Below, "Insert Rows Below"),
                cols(HorizontalSide::Left, "Insert Columns to the Left"),
                cols(HorizontalSide::Right, "Insert Columns to the Right"),
                item(state, "Delete Rows", Command::DeleteRows, false),
                item(state, "Delete Columns", Command::DeleteColumns, false),
                item(state, "Delete Table", Command::DeleteTable, false),
                merge,
            ],
            &mut items,
        );
    }
    if let Some(o) = &s.object {
        let mut list = Vec::new();
        if o.kind == ObjectKind::Picture {
            list.push(item(state, "Reset Size", Command::ResetObjectSize, false));
        }
        list.push(item(state, "Delete", Command::DeleteObject, false));
        group(list, &mut items);
    }
    if s.link.is_some() {
        group(vec![item(state, "Remove Hyperlink", Command::RemoveHyperlink, false)], &mut items);
    }
    if let Some(id) = &s.revision {
        group(
            vec![
                item(state, "Accept Change", Command::AcceptRevision(id.clone()), false),
                item(state, "Reject Change", Command::RejectRevision(id.clone()), false),
            ],
            &mut items,
        );
    }
    if let Some(id) = &s.comment {
        group(
            vec![
                item(state, "Delete Comment", Command::DeleteComment(id.clone()), false),
                item(state, "Resolve Comment", Command::ResolveComment(CommentResolve { id: id.clone(), done: true }), false),
            ],
            &mut items,
        );
    }
    if s.header_footer.is_some() {
        group(vec![item(state, "Close Header and Footer", Command::CloseHeaderFooter, false)], &mut items);
    }
    ContextMenu { target, items }
}

/// The command ids a menu's items run, nested items included (for checking menus against the catalog).
#[cfg(test)]
fn commands(items: &[MenuItem]) -> Vec<crate::api::command::CommandId> {
    items
        .iter()
        .flat_map(|i| match i {
            MenuItem::Command { command, .. } => vec![*command],
            MenuItem::Submenu { items, .. } => commands(items),
            _ => Vec::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::command::CommandId;
    use crate::api::state::{CommandState, TableSelection};

    #[test]
    fn a_misspelling_comes_first_then_the_clipboard() {
        let state = State::default();
        let m = Misspelled { word: "quikc".into(), language: "en-US".into(), repeated: false, suggestions: vec!["quick".into()] };
        let menu = build(&state, Some(&m));
        assert_eq!(menu.target, MenuTarget::Misspelling { word: "quikc".into(), language: "en-US".into() });
        let labels: Vec<&str> = menu
            .items
            .iter()
            .filter_map(|i| match i {
                MenuItem::Suggestion { label, .. } | MenuItem::Command { label, .. } => Some(label.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(labels[..4], ["quick", "Ignore", "Add to Dictionary", "Cut"]);
    }

    #[test]
    fn a_table_adds_its_actions_with_their_states() {
        let mut state = State::default();
        state.selection.empty = true;
        state.selection.table = Some(TableSelection { rows: 2, columns: 2, ..TableSelection::default() });
        state.commands.insert(CommandId::DeleteTable, CommandState { enabled: false, ..CommandState::default() });
        let menu = build(&state, None);
        assert_eq!(menu.target, MenuTarget::Table);
        let ids = commands(&menu.items);
        for id in [CommandId::InsertRows, CommandId::DeleteRows, CommandId::InsertColumns, CommandId::DeleteColumns, CommandId::MergeCells] {
            assert!(ids.contains(&id), "{id:?}");
        }
        let state_of = |want: CommandId| {
            menu.items.iter().find_map(|i| match i {
                MenuItem::Command { command, enabled, .. } if *command == want => Some(*enabled),
                _ => None,
            })
        };
        assert_eq!(state_of(CommandId::DeleteTable), Some(false), "from the command state");
        assert_eq!(state_of(CommandId::Cut), Some(false), "nothing selected");
        assert_eq!(state_of(CommandId::MergeCells), Some(false), "one cell");
        assert_eq!(state_of(CommandId::Paste), Some(true));
    }
}
