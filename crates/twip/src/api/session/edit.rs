//! Running commands and batches through the transaction pipeline, and the session's settings for who edits and how.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use super::Session;
use super::commands::id_name;
use crate::api::command::{Command, CommandResult};
use crate::api::error::Error;
use crate::api::event::Event;
use crate::api::record::{Author, Origin, TransactionRecord};
use crate::api::services::AnnouncementKind;
use crate::api::state::Mode;
use crate::editing::commands::Outcome;

/// What a command did: the transaction it committed (none for selection and view commands, or when nothing changed),
/// and what it made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Commit {
    /// The committed transaction, as `documentChanged` reported it.
    pub record: Option<TransactionRecord>,
    /// The command's result, for the commands that create something.
    pub value: Option<CommandResult>,
}

impl Session {
    /// Runs one command as one transaction.
    pub fn run(&mut self, command: Command) -> Result<Commit, Error> {
        self.check(&command)?;
        if let Some(done) = self.run_view(&command) {
            return done;
        }
        self.apply(command, Origin::Api)
    }

    /// Runs `commands` as one transaction and one undo step, labelled `label` (else its first command's): all of
    /// them, or none when one fails. Every command is checked before any runs.
    pub fn batch(&mut self, commands: Vec<Command>, label: Option<String>) -> Result<Commit, Error> {
        for c in &commands {
            // A batch is edits: undo, redo and view settings aren't part of a transaction.
            if matches!(
                c,
                Command::Undo
                    | Command::Redo
                    | Command::SetZoom(_)
                    | Command::SetViewOptions(_)
                    | Command::SetMode(_)
                    | Command::SetRulerTabType(_)
                    | Command::ShowTabTypes
                    | Command::ShowTabStop
                    | Command::SetAutoCorrect(_)
                    | Command::RevertAutoCorrect
                    | Command::NeverAutoCorrect
                    | Command::ShowAutoCorrectOptions
                    | Command::ClosePopover
            ) {
                return Err(Error::invalid(format!("a batch can't hold {}", id_name(c.id()))));
            }
            self.check(c)?;
        }
        self.apply_with(Origin::Api, |e, fonts| {
            e.begin_batch(label.map(Cow::Owned));
            for c in commands {
                if let refused @ Outcome::Refused(_) = e.execute(c, fonts) {
                    e.end_batch(false, fonts);
                    return refused;
                }
            }
            e.end_batch(true, fonts)
        })
    }

    /// Runs a command, then rebuilds the state and reports the change. Input runs here with [`Origin::User`].
    pub(super) fn apply(&mut self, command: Command, origin: Origin) -> Result<Commit, Error> {
        self.services.said = self.command_said(&command);
        // A copy and Describe Formatting never change anything, and still say their part; a paste that pasted nothing
        // (an empty clipboard) says nothing.
        let says_unchanged = matches!(command, Command::Copy | Command::DescribeFormatting);
        let done = self.apply_with(origin, |e, fonts| e.execute(command, fonts));
        // What wasn't said with a change is said now, if it may be; an edit refused where text can't go says why.
        let mut said = std::mem::take(&mut self.services.said);
        if !says_unchanged {
            said.clear();
        } else if !said.is_empty() {
            // Through a change, so the state goes first, as with every event.
            self.services.said = std::mem::take(&mut said);
            self.changed(Vec::new());
        }
        if let Err(e) = &done
            && (e.code == crate::api::error::ErrorCode::ReadOnly || e.message == "locked")
        {
            said = vec![(AnnouncementKind::ReadOnly, "Read only".into())];
        }
        self.announce(said);
        done
    }

    /// What a command says as it runs: undo and redo name what they take back or do again, the clipboard commands say
    /// what they did, a character format says whether it went on or off.
    pub(super) fn command_said(&self, c: &Command) -> Vec<(AnnouncementKind, String)> {
        let h = self.editor.history();
        let on = |name: &str, on: bool| (AnnouncementKind::Formatting, format!("{name} {}", if on { "on" } else { "off" }));
        match c {
            Command::Undo => h.next_undo().map(|l| (AnnouncementKind::Command, format!("Undo {l}"))).into_iter().collect(),
            Command::Redo => h.next_redo().map(|l| (AnnouncementKind::Command, format!("Redo {l}"))).into_iter().collect(),
            Command::Cut => vec![(AnnouncementKind::Command, "Cut".into())],
            Command::Copy => vec![(AnnouncementKind::Command, "Copied".into())],
            Command::Paste(_) | Command::PasteText(_) => vec![(AnnouncementKind::Command, "Pasted".into())],
            Command::SetBold(v) => vec![on("Bold", *v)],
            Command::SetItalic(v) => vec![on("Italic", *v)],
            Command::SetUnderline(s) => vec![on("Underline", *s != crate::api::value::UnderlineStyle::None)],
            Command::SetStrikethrough(v) => vec![on("Strikethrough", *v)],
            Command::DescribeFormatting => vec![(AnnouncementKind::Formatting, self.describe_formatting())],
            _ => Vec::new(),
        }
    }

    /// The formatting at the caret, as said: "Heading 1, Calibri 16 point, bold, highlighted yellow".
    fn describe_formatting(&self) -> String {
        use crate::api::state::Tri::Uniform;
        use crate::api::value::{AutoOr, NoneOr, UnderlineStyle};
        let (f, p) = (&self.state.formatting, &self.state.paragraph);
        let mut parts = Vec::new();
        if let Uniform(id) = &p.style {
            parts.push(self.styles(None).into_iter().find(|s| &s.id == id).map_or_else(|| id.0.clone(), |s| s.name));
        }
        match (&f.font, &f.size) {
            (Uniform(font), Uniform(size)) => parts.push(format!("{font} {} point", size.0)),
            (Uniform(font), _) => parts.push(font.clone()),
            _ => {}
        }
        for (on, name) in [(&f.bold, "bold"), (&f.italic, "italic"), (&f.strikethrough, "strikethrough")] {
            if *on == Uniform(true) {
                parts.push(name.to_string());
            }
        }
        if matches!(&f.underline, Uniform(u) if *u != UnderlineStyle::None) {
            parts.push("underline".into());
        }
        if matches!(&f.color, Uniform(AutoOr::Value(_))) {
            parts.push("coloured".into());
        }
        if let Uniform(NoneOr::Value(h)) = &f.highlight {
            // `BrightGreen` as `bright green`.
            let name = format!("{h:?}").chars().fold(String::new(), |mut s, c| {
                if c.is_uppercase() && !s.is_empty() {
                    s.push(' ');
                }
                s.extend(c.to_lowercase());
                s
            });
            parts.push(format!("highlighted {name}"));
        }
        if parts.is_empty() { "No formatting".into() } else { parts.join(", ") }
    }

    /// Runs an editor operation from `origin` (a command, or what the services drive: selecting a match, replacing
    /// matches), then reports it: a record for each transaction it committed, the state rebuilt, the listener told.
    pub(super) fn apply_with(
        &mut self,
        origin: Origin,
        op: impl FnOnce(&mut crate::editing::Editor, &crate::font::Fonts) -> Outcome,
    ) -> Result<Commit, Error> {
        let before = self.editor.selection();
        let out = {
            let fonts = self.fonts.fonts();
            self.editor.with_origin(origin, |e| op(e, &fonts))
        };
        let value = self.editor.take_made();
        let commits = self.editor.take_commits();
        let paras: Vec<_> = commits.iter().flat_map(|c| c.targets.paras.iter().copied()).collect();
        let tables = commits.iter().any(|c| !c.targets.tables.is_empty());
        let records: Vec<TransactionRecord> = commits
            .into_iter()
            .map(|c| {
                self.records += 1;
                TransactionRecord::of(c, self.records, &self.author)
            })
            .collect();
        let record = records.last().cloned();
        let edited = !records.is_empty();
        let mut events: Vec<Event> = records.into_iter().map(Event::DocumentChanged).collect();
        if edited {
            // An edit can name a family or face the document didn't have (a font applied, a paste).
            let (fonts, warning) = self.edited_fonts(paras, tables);
            events.extend(warning.map(Event::Warning).into_iter().chain(fonts));
        }
        events.extend(self.notices());
        // A move or a view change (track changes, the markup view) moves the state sequence, not the revision.
        let observable = matches!(out, Outcome::Changed(_) | Outcome::Moved);
        if observable || !events.is_empty() || self.editor.selection() != before {
            self.changed(events);
        }
        match out {
            Outcome::Refused(e) => Err(e),
            _ => Ok(Commit { record, value }),
        }
    }

    /// Who edits from now on: transactions, revisions and comments are recorded under this author.
    pub fn set_author(&mut self, author: Author) {
        self.editor.set_author(&author.name, author.identity.clone());
        self.author = author;
    }

    /// Read only on or off, with no reopen: a read-only session refuses every command that changes the document.
    pub fn set_read_only(&mut self, on: bool) {
        if self.read_only != on {
            self.read_only = on;
            self.configure_mode();
        }
    }

    /// Editing or design mode, with no reopen.
    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.configure_mode();
        }
    }

    /// The editor in the session's mode: a view change, so the state sequence moves and the revision doesn't.
    fn configure_mode(&mut self) {
        let mode = self.editor_mode();
        self.editor.set_mode(mode);
        self.changed(Vec::new());
    }
}
