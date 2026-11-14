//! A session through `twip::api`: open a document, run commands, read the state and events, export.
//!
//! `cargo run -p twip --example session`

use twip::api::command::Command;
use twip::api::engine::{Engine, SessionOptions};
use twip::api::event::Event;
use twip::api::fonts::FontLibrary;
use twip::api::session::ExportOptions;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = Engine::new(FontLibrary::system());
    // Events arrive synchronously, in commit order, after the session's state reflects them.
    engine.set_listener(|_, event| match event {
        Event::StateChanged(state) => println!("event: stateChanged, revision {}", state.revision),
        event => println!("event: {}", twip_wire::to_string(event)),
    });
    let id = engine.create_session(SessionOptions::default());
    let session = engine.session(id).expect("the session was just created");

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/text-basics.docx");
    let warnings = session.open(&std::fs::read(path)?)?;
    println!("opened {path}, {} warning(s)", warnings.len());

    for command in [Command::SelectAll, Command::SetBold(true), Command::Undo] {
        session.run(command)?;
    }

    let state = session.state();
    println!(
        "state: revision {}, modified {}, formatting {}, history {}",
        state.revision,
        state.modified,
        twip_wire::to_string(&state.formatting),
        twip_wire::to_string(&state.history)
    );

    let export = session.export(&ExportOptions::Docx)?;
    println!("exported {} bytes at revision {}", export.bytes.0.len(), export.revision);
    Ok(())
}
