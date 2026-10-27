//! Find and Replace for the host's find card: through the editor's queries and commands.

use super::Engine;

impl Engine {
    /// Selects the next match (the previous one, `backwards`), wrapping; false when there is none.
    pub fn find_next(&mut self, text: &str, match_case: bool, backwards: bool) -> bool {
        let Some(hit) = self.editor.as_ref().and_then(|e| e.find(text, match_case, backwards)) else { return false };
        let (p, a, b) = hit;
        self.command("edit.select-range", Some(&format!("{},{},{}", p.0, a.0, b.0)));
        true
    }

    /// Replaces the selection when it is a match, then selects the next one; false when there is none left.
    pub fn replace_next(&mut self, find: &str, replacement: &str, match_case: bool) -> bool {
        if self.editor.as_ref().is_some_and(|e| e.selection_matches(find, match_case)) {
            self.command("edit.insert_text", Some(replacement));
        }
        self.find_next(find, match_case, false)
    }

    /// Replaces every match (one undo step); how many.
    pub fn replace_all(&mut self, find: &str, replacement: &str, match_case: bool) -> usize {
        let n = self.editor.as_ref().map_or(0, |e| e.count_matches(find, match_case));
        if n > 0 {
            let arg = serde_json::json!({ "find": find, "with": replacement, "case": match_case }).to_string();
            self.command("edit.replace-all", Some(&arg));
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::Fonts;

    #[test]
    fn find_and_replace() {
        let mut e = Engine::with_fonts(Fonts::system());
        e.new_document();
        e.text("one two One three one");
        e.key("Home", true, false, false);
        assert!(e.find_next("one", false, false));
        assert_eq!(e.editor.as_mut().unwrap().selected_text(), "one");
        // Replace: the selected match, then the next is selected.
        assert!(e.replace_next("one", "1", false));
        assert_eq!(e.editor.as_mut().unwrap().selected_text(), "One");
        assert_eq!(e.replace_all("one", "1", true), 1);
        let text = crate::editing::text::UnitText::of(match &e.editor.as_ref().unwrap().document().sections[0].blocks[0] {
            crate::docx::Block::Para(p) => p,
            _ => panic!(),
        })
        .text;
        assert_eq!(text, "1 two One three 1");
        // One undo takes Replace All back.
        e.command("edit.undo", None);
        assert!(e.find_next("one", true, false));
        assert!(!e.find_next("zebra", false, false));
    }
}
