//! Running commands and batches through the transaction pipeline, and the session's settings for who edits and how.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use super::Session;
use super::commands::id_name;
use crate::api::command::{Command, CommandResult};
use crate::api::error::Error;
use crate::api::event::Event;
use crate::api::record::{Author, Origin, TransactionRecord};
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
            if matches!(c, Command::Undo | Command::Redo | Command::SetZoom(_) | Command::SetViewOptions(_) | Command::SetMode(_)) {
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
        self.apply_with(origin, |e, fonts| e.execute(command, fonts))
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
        let records: Vec<TransactionRecord> = self
            .editor
            .take_commits()
            .into_iter()
            .map(|c| {
                self.records += 1;
                TransactionRecord::of(c, self.records, &self.author)
            })
            .collect();
        let record = records.last().cloned();
        let mut events: Vec<Event> = records.into_iter().map(Event::DocumentChanged).collect();
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
