//! The font library sessions share: added fonts, a resolver's requests, substitution rules and the fallback family.

use std::cell::{Ref, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use super::error::Error;
use super::state::FontsState;
use crate::font::Fonts;

/// A face a font file added.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FaceInfo {
    /// The family name the font file gives.
    pub family: String,
    /// The face is bold.
    pub bold: bool,
    /// The face is italic.
    pub italic: bool,
}

/// A face layout needs and the library doesn't have: what a resolver is asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontRequest {
    /// The family, as the document names it.
    pub family: String,
    /// A bold face is needed.
    pub bold: bool,
    /// An italic face is needed.
    pub italic: bool,
}

/// Draw `from` (optionally only its bold or italic faces) with `to`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubstitutionRule {
    /// The family to replace, case-insensitive.
    pub from: String,
    /// Only its bold (`true`) or regular-weight (`false`) faces; `None` for both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    /// Only its italic (`true`) or upright (`false`) faces; `None` for both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    /// The family that draws instead.
    pub to: String,
}

/// Receives each font request once.
type Requester = Box<dyn FnMut(&FontRequest)>;

/// What the library holds besides its faces.
#[derive(Default)]
struct Library {
    fonts: Fonts,
    /// Faces no font had when a document needed them, by lowercase family, bold and italic: `false` while the
    /// request is unanswered, `true` once it was answered with nothing. A face added for the family resolves it,
    /// whatever its entry says.
    asked: HashMap<(String, bool, bool), bool>,
    requester: Option<Requester>,
    generation: u64,
}

/// The fonts every session of an engine lays out with. Cloning shares it.
#[derive(Clone)]
pub struct FontLibrary {
    // Shared and mutable: fonts arrive after sessions exist (a resolver's answer, a late `add`), and every session
    // that holds the library must lay out with them. A copy-on-write catalog would fork instead, leaving earlier
    // sessions with the old fonts.
    inner: Rc<RefCell<Library>>,
}

impl FontLibrary {
    /// The fonts installed on this system (none in the browser).
    pub fn system() -> FontLibrary {
        FontLibrary::with(Fonts::system())
    }

    /// No fonts: add them with [`FontLibrary::add`].
    pub fn empty() -> FontLibrary {
        FontLibrary::with(Fonts::default())
    }

    fn with(fonts: Fonts) -> FontLibrary {
        FontLibrary { inner: Rc::new(RefCell::new(Library { fonts, ..Library::default() })) }
    }

    /// Adds a font file (TrueType, OpenType, a collection, WOFF or WOFF2) and returns its faces.
    pub fn add(&self, bytes: &[u8]) -> Result<Vec<FaceInfo>, Error> {
        let mut lib = self.inner.borrow_mut();
        let faces = lib.fonts.add_bytes(bytes.to_vec())?;
        lib.generation += 1;
        Ok(faces)
    }

    /// The substitution rules, replacing earlier ones.
    pub fn set_substitutions(&self, rules: Vec<SubstitutionRule>) {
        let mut lib = self.inner.borrow_mut();
        lib.fonts.set_rules(rules);
        lib.generation += 1;
    }

    /// The family text falls back to when nothing else draws it. Until one is set it is the first font in the
    /// library (Calibri or a metric-compatible family first, when the library has one).
    pub fn set_default(&self, family: &str) {
        let mut lib = self.inner.borrow_mut();
        lib.fonts.set_default(family);
        lib.generation += 1;
    }

    /// Records that no font could be found for `request` (a resolver answered nothing).
    pub fn mark_unresolved(&self, request: &FontRequest) {
        let mut lib = self.inner.borrow_mut();
        let answered = lib.asked.insert((request.family.to_lowercase(), request.bold, request.italic), true);
        if answered != Some(true) {
            lib.generation += 1;
        }
    }

    /// Where requests for faces the library lacks go, replacing an earlier receiver. Each family and style is
    /// requested once; answer with [`FontLibrary::add`] or [`FontLibrary::mark_unresolved`]. With no receiver, a
    /// missing face is unresolved at once, and requested once there is one: the sessions ask at their next
    /// [`Engine::sync_fonts`](crate::api::engine::Engine::sync_fonts). The receiver must not call the library: it
    /// queues the work instead.
    pub fn on_request(&self, f: impl FnMut(&FontRequest) + 'static) {
        let mut lib = self.inner.borrow_mut();
        lib.requester = Some(Box::new(f));
        lib.generation += 1;
    }

    /// No receiver from now on: a missing face is unresolved at once.
    pub fn stop_requests(&self) {
        self.inner.borrow_mut().requester = None;
    }

    /// Every family the library can draw now, sorted.
    pub fn families(&self) -> Vec<String> {
        self.inner.borrow().fonts.families()
    }

    /// Changes whenever the library does: sessions lay out again when it moves.
    pub fn generation(&self) -> u64 {
        self.inner.borrow().generation
    }

    /// The faces, for layout and drawing.
    pub(crate) fn fonts(&self) -> Ref<'_, Fonts> {
        Ref::map(self.inner.borrow(), |l| &l.fonts)
    }

    /// The families of `faces` (family, bold, italic, as a document names them) that are being fetched or that no
    /// font was found for. A face the library can't draw is requested the first time it is seen.
    pub(crate) fn check<'a>(&self, faces: impl IntoIterator<Item = (&'a str, bool, bool)>) -> FontsState {
        // Families by their lowercase name: the first spelling seen is the one reported.
        let (mut pending, mut unresolved, mut requests) = (BTreeMap::new(), BTreeMap::new(), Vec::new());
        {
            let mut lib = self.inner.borrow_mut();
            let Library { fonts, asked, requester, .. } = &mut *lib;
            for (family, bold, italic) in faces {
                let family = fonts.render_family(family, bold, italic);
                if fonts.resolves(family, bold, italic) {
                    continue;
                }
                let lower = family.to_lowercase();
                let key = (lower.clone(), bold, italic);
                let answered = match asked.get(&key) {
                    Some(&answered) => answered,
                    // Unresolved for now, and not remembered: a receiver set later is asked.
                    None if requester.is_none() => true,
                    None => {
                        asked.insert(key, false);
                        requests.push(FontRequest { family: family.to_string(), bold, italic });
                        false
                    }
                };
                if answered { &mut unresolved } else { &mut pending }.entry(lower).or_insert_with(|| family.to_string());
            }
        }
        // Called with the library released, so a receiver that reads it doesn't fail.
        let requester = if requests.is_empty() { None } else { self.inner.borrow_mut().requester.take() };
        if let Some(mut f) = requester {
            requests.iter().for_each(&mut f);
            self.inner.borrow_mut().requester.get_or_insert(f);
        }
        // A family with one style still being fetched is pending, not unresolved.
        let unresolved = unresolved.into_iter().filter(|(k, _)| !pending.contains_key(k)).map(|(_, f)| f).collect();
        FontsState { pending: pending.into_values().collect(), unresolved }
    }
}

impl std::fmt::Debug for FontLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontLibrary").field("generation", &self.generation()).finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::FontLibrary;

    #[test]
    fn a_face_missing_before_there_is_a_receiver_is_requested_once_there_is_one() {
        let lib = FontLibrary::empty();
        assert_eq!(lib.check([("Carlito", false, false)]).unresolved, ["Carlito"]);
        let asked: Rc<RefCell<Vec<String>>> = Rc::default();
        let sink = asked.clone();
        let generation = lib.generation();
        lib.on_request(move |r| sink.borrow_mut().push(r.family.clone()));
        assert_ne!(lib.generation(), generation, "the sessions check again");
        let state = lib.check([("Carlito", false, false)]);
        assert_eq!((state.pending, state.unresolved), (vec!["Carlito".to_string()], vec![]));
        assert_eq!(*asked.borrow(), ["Carlito"]);
        // Asked once.
        lib.check([("Carlito", false, false)]);
        assert_eq!(asked.borrow().len(), 1);
    }
}
