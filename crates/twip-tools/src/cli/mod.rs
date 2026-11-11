//! Command-line tools for checking the engine: dumps and their comparison, round trips, unit limits.

pub mod autocorrect;
pub mod dump;
pub mod dumpdiff;
pub mod fonts;
pub mod pdf;
pub mod presets;
pub mod roundtrip;
pub mod units;

use twip::internal::font::Fonts;
use twip::internal::layout::hyphenate::{Hyphenation, Patterns};

/// The families searched for a character a run's font lacks, as Word on Windows draws symbols and emoji; the bundled
/// symbol face comes after them.
pub const CHARACTER_FALLBACK: [&str; 2] = ["Segoe UI Symbol", "Segoe UI Emoji"];

/// The system's fonts with [`CHARACTER_FALLBACK`]: what the layout commands lay out with.
pub fn system_fonts() -> Fonts {
    let mut fonts = Fonts::system();
    fonts.set_character_fallback(CHARACTER_FALLBACK.map(String::from).to_vec());
    fonts
}

/// Takes every `--hyphenation <lang>=<patterns file>` out of `args`: the host's hyphenation patterns (a `hyph_*.dic`
/// file per BCP 47 language, as a session's dictionaries give them), or None when there are none.
pub fn hyphenation(args: &mut Vec<String>) -> Result<Option<Hyphenation>, String> {
    let mut out: Option<Hyphenation> = None;
    while let Some(i) = args.iter().position(|a| a == "--hyphenation") {
        let spec = args.get(i + 1).cloned().ok_or("--hyphenation needs <lang>=<patterns file>")?;
        args.drain(i..i + 2);
        let (lang, path) = spec.split_once('=').ok_or_else(|| format!("--hyphenation {spec}: not <lang>=<patterns file>"))?;
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        out = Some(out.unwrap_or_default().with(lang, Patterns::parse(&String::from_utf8_lossy(&bytes))));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn symbols_fall_back_to_segoe_ui_symbol() {
        // Where it is installed (Windows): a check mark the run's font lacks draws in it, not in the bundled face.
        let fonts = super::system_fonts();
        if fonts.is_installed("Segoe UI Symbol") {
            assert_eq!(fonts.fallback('\u{2713}', false, false).expect("a face").family, "Segoe UI Symbol");
        }
    }

    #[test]
    fn hyphenation_takes_its_options_out() {
        let file = std::env::temp_dir().join("twip-tools-hyph-test.dic");
        std::fs::write(&file, "UTF-8\nhy3ph\nhe2n\nhena4\nhen5at\n1na\nn2at\n1tio\n2io\no2n").unwrap();
        let mut args: Vec<String> = ["out", "--hyphenation", &format!("en-US={}", file.display()), "a.docx"].map(String::from).to_vec();
        let h = super::hyphenation(&mut args).unwrap().expect("patterns");
        assert_eq!(args, ["out", "a.docx"]);
        assert_eq!(h.for_language("en-US").expect("en-US patterns").points("hyphenation", 3), vec![2, 6]);
        // None without the option, and the arguments as they were.
        let mut args = vec!["out".to_string()];
        assert!(super::hyphenation(&mut args).unwrap().is_none() && args == ["out"]);
        assert!(super::hyphenation(&mut vec!["--hyphenation".into(), "en-US".into()]).is_err());
    }
}
