//! The context menu: what the selection is on, and the items for it, each with its state from the command states.
//! Spelling suggestions come first on a misspelling; then the clipboard and paste options, then the field, link,
//! list, table, object, review and header and footer actions, Synonyms, then New Comment.

use crate::api::catalog;
use crate::api::command::{
    AutoFit, Command, CommentResolve, ElementIds, HorizontalSide, InsertColumns, InsertRows, ObjectPosition, PasteAs, PasteOptions, ShowFieldCodes,
    VerticalSide,
};
use crate::api::drawing::Arrange;
use crate::api::review::Body;
use crate::api::services::{ContextMenu, MenuAction, MenuItem, MenuTarget};
use crate::api::state::{ElementKind, ListState, ObjectKind, ObjectSelection, State, Tri};
use crate::api::value::{ObjectWrap, Story, VerticalPosition, VerticalReference};

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
        // A shape, line or group takes the object menu a picture does.
        Some(ObjectKind::Picture | ObjectKind::Shape | ObjectKind::Line | ObjectKind::Group) => return MenuTarget::Picture,
        Some(ObjectKind::TextBox) => return MenuTarget::TextBox,
        // A frame holds text: the text's menu.
        Some(ObjectKind::Frame) | None => {}
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
    MenuItem::Command { command: id, label: label.into(), enabled, checked, shortcut, action: MenuAction::Command { command: Box::new(command) } }
}

/// What the menu needs beyond the state: the Toggle Field Codes command for the fields at the selection, and the
/// synonyms the host gave for the word at the selection.
#[derive(Clone, Debug, Default)]
pub struct MenuFacts {
    /// What Toggle Field Codes sets, `None` without a field at the selection.
    pub field_codes: Option<ShowFieldCodes>,
    /// The Synonyms submenu's words, as the host's thesaurus gives them; `None` shows no Synonyms submenu.
    pub synonyms: Option<Vec<String>>,
}

impl MenuFacts {
    /// The facts for `editor`'s selection, without synonyms.
    pub fn of(editor: &crate::editing::Editor) -> MenuFacts {
        MenuFacts { field_codes: editor.field_codes_toggle(false), synonyms: None }
    }
}

fn submenu(label: &str, items: Vec<MenuItem>) -> MenuItem {
    MenuItem::Submenu { label: label.into(), items, inline: false }
}

fn action(label: &str, enabled: bool, action: MenuAction) -> MenuItem {
    MenuItem::Action { label: label.into(), enabled, checked: None, action }
}

fn checked(mut i: MenuItem, on: bool) -> MenuItem {
    if let MenuItem::Command { checked, .. } = &mut i {
        *checked = Some(on);
    }
    i
}

fn enabled_if(mut i: MenuItem, on: bool) -> MenuItem {
    if let MenuItem::Command { enabled, .. } = &mut i {
        *enabled &= on;
    }
    i
}

/// The context menu for the selection.
pub fn build(state: &State, misspelled: Option<&Misspelled>) -> ContextMenu {
    build_with(state, misspelled, &MenuFacts::default())
}

/// The context menu for the selection, in Word's order: spelling, the clipboard and paste options, the target's
/// own items (field, link, list, table, object, revision, comment, header and footer), Synonyms, then New Comment.
pub fn build_with(state: &State, misspelled: Option<&Misspelled>, facts: &MenuFacts) -> ContextMenu {
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
    let paste = |label, as_| item(state, label, Command::Paste(PasteOptions { as_: Some(as_) }), false);
    items.push(MenuItem::Submenu {
        label: "Paste Options".into(),
        items: vec![paste("Keep Source Formatting", PasteAs::Formatted), paste("Keep Text Only", PasteAs::Text)],
        inline: true,
    });
    let s = &state.selection;
    let group = |list: Vec<MenuItem>, items: &mut Vec<MenuItem>| {
        if !list.is_empty() {
            items.push(MenuItem::Separator);
            items.extend(list);
        }
    };
    if let Some(e) = s.context.first().filter(|e| e.kind == ElementKind::Field) {
        let ids = ElementIds { ids: Some(vec![e.id.clone()]) };
        let mut list = vec![item(state, "Update Field", Command::UpdateFields(ids), false)];
        if let Some(t) = &facts.field_codes {
            list.push(item(state, "Toggle Field Codes", Command::ShowFieldCodes(t.clone()), false));
        }
        group(list, &mut items);
    }
    if let Some(l) = &s.link {
        let address = l.address.clone();
        group(
            vec![
                action("Open Hyperlink", true, MenuAction::OpenLink { address: address.clone() }),
                action("Copy Hyperlink", true, MenuAction::CopyLink { address }),
                item(state, "Remove Hyperlink", Command::RemoveHyperlink, false),
            ],
            &mut items,
        );
    }
    if s.object.is_none() && state.paragraph.list == Tri::Uniform(ListState::Numbered) {
        group(
            vec![item(state, "Restart at 1", Command::RestartNumbering, false), item(state, "Continue Numbering", Command::ContinueNumbering, false)],
            &mut items,
        );
    }
    if let Some(t) = &s.table
        && s.object.is_none()
    {
        group(table(state, t.can_merge), &mut items);
    }
    if let Some(o) = &s.object {
        group(object(state, o), &mut items);
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
    if let Some(words) = facts.synonyms.as_ref().filter(|_| s.object.is_none()) {
        let list = words.iter().map(|w| MenuItem::Suggestion { label: w.clone(), action: MenuAction::ReplaceWord { with: w.clone() } }).collect();
        group(vec![submenu("Synonyms", list)], &mut items);
    }
    if s.header_footer.is_none() {
        group(vec![item(state, "New Comment", Command::AddComment(Body::Text(String::new())), false)], &mut items);
    }
    ContextMenu { target, items }
}

/// A table's Insert and Delete submenus, for several cells Merge Cells and Distribute Rows and Columns Evenly, then the
/// AutoFit submenu.
fn table(state: &State, can_merge: bool) -> Vec<MenuItem> {
    let rows = |side, label| item(state, label, Command::InsertRows(InsertRows { side, count: None }), false);
    let cols = |side, label| item(state, label, Command::InsertColumns(InsertColumns { side, count: None }), false);
    let mut items = vec![
        submenu(
            "Insert",
            vec![
                cols(HorizontalSide::Left, "Insert Columns to the Left"),
                cols(HorizontalSide::Right, "Insert Columns to the Right"),
                rows(VerticalSide::Above, "Insert Rows Above"),
                rows(VerticalSide::Below, "Insert Rows Below"),
            ],
        ),
        submenu(
            "Delete",
            vec![
                item(state, "Delete Columns", Command::DeleteColumns, false),
                item(state, "Delete Rows", Command::DeleteRows, false),
                item(state, "Delete Table", Command::DeleteTable, false),
            ],
        ),
    ];
    if can_merge {
        items.push(item(state, "Merge Cells", Command::MergeCells, false));
        items.push(item(state, "Distribute Rows Evenly", Command::DistributeRows, false));
        items.push(item(state, "Distribute Columns Evenly", Command::DistributeColumns, false));
    }
    items.push(submenu("Cell Alignment", cell_alignment(state)));
    let fit = |label, a| item(state, label, Command::AutoFitColumns(a), false);
    items.push(submenu(
        "AutoFit",
        vec![fit("AutoFit to Contents", AutoFit::Contents), fit("AutoFit to Window", AutoFit::Window), fit("Fixed Column Width", AutoFit::Fixed)],
    ));
    items
}

/// Word's 3×3 Cell Alignment: each sets the paragraphs' alignment and the cells' vertical alignment, as one step.
fn cell_alignment(state: &State) -> Vec<MenuItem> {
    use crate::api::value::{Alignment as A, CellAlignment as V};
    let enabled = state.commands.get(&crate::api::command::CommandId::SetCellAlignment).is_none_or(|c| c.enabled) && !state.read_only;
    let now = (state.paragraph.alignment, state.selection.table.as_ref().map(|t| t.cell_alignment));
    let mut out = Vec::with_capacity(9);
    for (v, row) in [(V::Top, "Top"), (V::Center, "Center"), (V::Bottom, "Bottom")] {
        for (a, column) in [(A::Left, "Left"), (A::Center, "Center"), (A::Right, "Right")] {
            let label = match (row, column) {
                ("Center", "Center") => "Align Center".to_string(),
                _ => format!("Align {row} {column}"),
            };
            let on = now.0 == Tri::Uniform(a) && now.1 == Some(Tri::Uniform(v));
            out.push(MenuItem::Action {
                label,
                enabled,
                checked: Some(on),
                action: MenuAction::Commands {
                    commands: vec![Command::SetAlignment(a), Command::SetCellAlignment(v)],
                    label: "Cell Alignment".into(),
                },
            });
        }
    }
    out
}

/// The menu a right-button drag of text opens where it dropped: Move Here, Copy Here, Link Here (with a file name to
/// link to), Create Hyperlink Here, Cancel.
pub fn drop_menu(link: bool) -> ContextMenu {
    use crate::api::services::DropChoice as D;
    let choice = |label, enabled, what| action(label, enabled, MenuAction::Drop { what });
    let items = vec![
        choice("Move Here", true, D::Move),
        choice("Copy Here", true, D::Copy),
        choice("Link Here", link, D::Link),
        choice("Create Hyperlink Here", true, D::Hyperlink),
        MenuItem::Separator,
        choice("Cancel", true, D::Cancel),
    ];
    ContextMenu { target: MenuTarget::Drop, items }
}

/// A picture's or shape's Group, Bring to Front, Send to Back and Wrap Text submenus.
fn object(state: &State, o: &ObjectSelection) -> Vec<MenuItem> {
    let arrange = |label, a| item(state, label, Command::ArrangeObject(a), false);
    let floating = o.wrap != ObjectWrap::Inline;
    let mut group = vec![enabled_if(item(state, "Group", Command::GroupObjects, false), o.count >= 2)];
    if o.kind == ObjectKind::Group {
        group.push(item(state, "Ungroup", Command::UngroupObjects, false));
    }
    group.push(item(state, "Regroup", Command::RegroupObjects, false));
    let wrap = |label, w| checked(item(state, label, Command::SetObjectWrap(w), false), o.wrap == w);
    let vertical = |relative_to| {
        let v = VerticalPosition { relative_to, align: None, offset: None, percent: None };
        Command::SetObjectPosition(ObjectPosition { horizontal: None, vertical: Some(v), allow_overlap: None, layout_in_cell: None })
    };
    let moves = o.placement.as_ref().is_some_and(|p| matches!(p.vertical.relative_to, VerticalReference::Paragraph | VerticalReference::Line));
    let mut wraps = vec![
        wrap("In Line with Text", ObjectWrap::Inline),
        wrap("Square", ObjectWrap::Square),
        wrap("Tight", ObjectWrap::Tight),
        wrap("Through", ObjectWrap::Through),
        wrap("Top and Bottom", ObjectWrap::TopAndBottom),
        wrap("Behind Text", ObjectWrap::BehindText),
        wrap("In Front of Text", ObjectWrap::InFrontOfText),
    ];
    if floating {
        wraps.push(MenuItem::Separator);
        wraps.push(checked(item(state, "Move with Text", vertical(VerticalReference::Paragraph), false), moves));
        wraps.push(checked(item(state, "Fix Position on Page", vertical(VerticalReference::Page), false), !moves));
        wraps.push(item(state, "Edit Wrap Points", Command::EditWrapPoints, false));
    }
    wraps.push(MenuItem::Separator);
    wraps.push(action("Set as Default Layout", true, MenuAction::SetDefaultPictureWrap { wrap: o.wrap }));
    let front = vec![
        arrange("Bring to Front", Arrange::ToFront),
        arrange("Bring Forward", Arrange::Forward),
        arrange("Bring in Front of Text", Arrange::InFrontOfText),
    ];
    let back =
        vec![arrange("Send to Back", Arrange::ToBack), arrange("Send Backward", Arrange::Backward), arrange("Send Behind Text", Arrange::BehindText)];
    vec![submenu("Group", group), submenu("Bring to Front", front), submenu("Send to Back", back), submenu("Wrap Text", wraps)]
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
        for id in [CommandId::InsertRows, CommandId::DeleteRows, CommandId::InsertColumns, CommandId::DeleteColumns] {
            assert!(ids.contains(&id), "{id:?}");
        }
        fn state_of(items: &[MenuItem], want: CommandId) -> Option<bool> {
            items.iter().find_map(|i| match i {
                MenuItem::Command { command, enabled, .. } if *command == want => Some(*enabled),
                MenuItem::Submenu { items, label, .. } if label != "Paste Options" => state_of(items, want),
                _ => None,
            })
        }
        let state_of = |want| state_of(&menu.items, want);
        assert_eq!(state_of(CommandId::DeleteTable), Some(false), "from the command state");
        assert_eq!(state_of(CommandId::Cut), Some(false), "nothing selected");
        assert_eq!(state_of(CommandId::MergeCells), None, "one cell");
        assert_eq!(state_of(CommandId::Paste), None, "in Paste Options");
        assert!(menu.items.iter().any(|i| matches!(i, MenuItem::Submenu { label, items, .. } if label == "Paste Options" && items.len() == 2)));
    }

    #[test]
    fn several_cells_distribute_and_every_table_has_autofit() {
        let mut state = State::default();
        state.selection.table = Some(TableSelection { rows: 2, columns: 2, ..TableSelection::default() });
        let one = commands(&build(&state, None).items);
        assert!(one.contains(&CommandId::AutoFitColumns) && !one.contains(&CommandId::DistributeRows), "{one:?}");
        state.selection.table = Some(TableSelection { rows: 2, columns: 2, can_merge: true, ..TableSelection::default() });
        let menu = build(&state, None);
        let ids = commands(&menu.items);
        let at = |id| ids.iter().position(|i| *i == id).unwrap_or_else(|| panic!("{id:?}"));
        assert!(at(CommandId::MergeCells) < at(CommandId::DistributeRows) && at(CommandId::DistributeRows) < at(CommandId::DistributeColumns));
        assert!(at(CommandId::DistributeColumns) < at(CommandId::AutoFitColumns));
        let fit = menu.items.iter().find_map(|i| match i {
            MenuItem::Submenu { label, items, .. } if label == "AutoFit" => Some(items.len()),
            _ => None,
        });
        assert_eq!(fit, Some(3), "contents, window, fixed");
    }

    #[test]
    fn synonyms_come_before_new_comment_when_the_host_gives_them() {
        let state = State::default();
        let facts = MenuFacts { synonyms: Some(vec!["fast".into(), "rapid".into()]), ..MenuFacts::default() };
        let menu = build_with(&state, None, &facts);
        let n = menu.items.len();
        let MenuItem::Submenu { label, items, .. } = &menu.items[n - 3] else { panic!("{:?}", menu.items) };
        assert_eq!(label, "Synonyms");
        assert_eq!(items[1], MenuItem::Suggestion { label: "rapid".into(), action: MenuAction::ReplaceWord { with: "rapid".into() } });
        assert!(matches!(menu.items[n - 1], MenuItem::Command { command: CommandId::AddComment, .. }));
        assert!(!build(&state, None).items.iter().any(|i| matches!(i, MenuItem::Submenu { label, .. } if label == "Synonyms")), "no synonyms given");
    }

    #[test]
    fn an_object_has_word_arrange_and_wrap_submenus() {
        use crate::api::state::ObjectPlacement;
        let mut state = State::default();
        let placement = ObjectPlacement {
            vertical: VerticalPosition { relative_to: VerticalReference::Page, ..VerticalPosition::default() },
            ..ObjectPlacement::default()
        };
        state.selection.object = Some(ObjectSelection {
            kind: ObjectKind::Picture,
            wrap: ObjectWrap::Square,
            count: 1,
            placement: Some(placement),
            ..ObjectSelection::default()
        });
        let menu = build(&state, None);
        assert_eq!(menu.target, MenuTarget::Picture);
        let sub = |label: &str| {
            menu.items.iter().find_map(|i| match i {
                MenuItem::Submenu { label: l, items, .. } if l == label => Some(items.clone()),
                _ => None,
            })
        };
        let labels = |items: Vec<MenuItem>| {
            items
                .into_iter()
                .filter_map(|i| match i {
                    MenuItem::Command { label, checked, enabled, .. } => Some((label, checked, enabled)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let wrap = labels(sub("Wrap Text").expect("wrap text"));
        assert_eq!(wrap.len(), 10, "seven wraps, Move with Text, Fix Position on Page, Edit Wrap Points");
        assert_eq!(wrap.iter().filter(|w| w.1 == Some(true)).map(|w| w.0.as_str()).collect::<Vec<_>>(), ["Square", "Fix Position on Page"]);
        assert_eq!(labels(sub("Bring to Front").expect("front"))[2].0, "Bring in Front of Text");
        assert_eq!(labels(sub("Send to Back").expect("back"))[2].0, "Send Behind Text");
        let group = labels(sub("Group").expect("group"));
        assert_eq!((group[0].0.as_str(), group[0].2), ("Group", false), "one object");
        assert_eq!(group[1].0, "Regroup");
        assert!(sub("Wrap Text").expect("wrap text").iter().any(|i| matches!(i, MenuItem::Action { label, .. } if label == "Set as Default Layout")));
        assert!(matches!(menu.items.last(), Some(MenuItem::Command { command: CommandId::AddComment, .. })), "New Comment last on an object");
    }

    #[test]
    fn a_numbered_field_paragraph_has_list_and_field_items_and_new_comment() {
        use crate::api::state::ElementContext;
        let mut state = State::default();
        state.paragraph.list = Tri::Uniform(ListState::Numbered);
        state.selection.context = vec![ElementContext { kind: ElementKind::Field, ..ElementContext::default() }];
        let facts = MenuFacts { field_codes: Some(ShowFieldCodes { show: true, ids: None }), ..MenuFacts::default() };
        let ids = commands(&build_with(&state, None, &facts).items);
        for id in
            [CommandId::UpdateFields, CommandId::ShowFieldCodes, CommandId::RestartNumbering, CommandId::ContinueNumbering, CommandId::AddComment]
        {
            assert!(ids.contains(&id), "{id:?}");
        }
        assert!(!commands(&build(&state, None).items).contains(&CommandId::ShowFieldCodes), "needs the editor's field view");
    }
}
