//! `twip replay <doc.docx> <log.jsonl> [--fonts <dir>] [--dump] [--state] [--inspect <sections>]`: applies a host input log (one
//! message per line, as `Engine::input_log` returns it in test builds) through `Editor::apply` and prints the
//! inspection state and, with `--dump`, the document dump. `--fonts` uses only the fonts in a folder, as a browser
//! host has, so pointer coordinates hit the same geometry.

use twip::editing::{Editor, inspect};
use twip::font::Fonts;

pub fn run(args: &[String]) -> Result<(), String> {
    let usage = "usage: replay <doc.docx> <log.jsonl> [--fonts <dir>] [--dump] [--state] [--inspect <sections>]";
    let (Some(doc), Some(log)) = (args.first(), args.get(1)) else { return Err(usage.into()) };
    let fonts = match args.iter().position(|a| a == "--fonts") {
        Some(i) => {
            let mut f = Fonts::default();
            f.add_dir(args.get(i + 1).ok_or(usage)?, true);
            f
        }
        None => Fonts::system(),
    };
    let dump = args.iter().any(|a| a == "--dump");
    let state = args.iter().any(|a| a == "--state") || !dump;
    let bytes = std::fs::read(doc).map_err(|e| format!("{doc}: {e}"))?;
    let mut e = Editor::open(&bytes, &fonts)?;
    let text = std::fs::read_to_string(log).map_err(|e| format!("{log}: {e}"))?;
    for (n, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        let reply = e.apply_json(line, &fonts);
        // A bad input stops the replay; a command the editor refused (`answer.error`) is a reply like any.
        if serde_json::from_str::<serde_json::Value>(&reply).map_or(true, |r| r.get("error").is_some()) {
            return Err(format!("{log}:{}: {reply}", n + 1));
        }
        // Background layout between inputs, as a host that settles before each step.
        while e.idle(&fonts) {}
    }
    e.complete(&fonts);
    let mut sections = Vec::new();
    if let Some(i) = args.iter().position(|a| a == "--inspect") {
        sections.push(args.get(i + 1).map_or("", String::as_str));
    } else if state {
        sections.push("caret,selection,undo,integrity");
    }
    if dump {
        sections.push("dump");
    }
    println!("{}", serde_json::to_string_pretty(&inspect::inspect(&mut e, &sections.join(","), true)).unwrap_or_default());
    Ok(())
}
