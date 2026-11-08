//! The font library sessions share: added fonts, a resolver's requests, and the application's font settings.

use std::cell::{Ref, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use super::error::Error;
use super::state::{FontsState, MissingFont, StandIn};
pub use crate::docx::font_table::{FontClass, FontDescription, FontPitch, GenericFamily};
use crate::font::{FontSettings, Fonts, Resolution};

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

/// A face layout needs and the library doesn't have: what a resolver is asked for, once per family and style.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontRequest {
    /// The family, as the document names it.
    pub family: String,
    /// A bold face is needed.
    pub bold: bool,
    /// An italic face is needed.
    pub italic: bool,
    /// The document's description of the family (its font table), when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<FontDescription>,
    /// The generic category that description gives, whether or not classification is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed: Option<GenericFamily>,
}

/// A resolver's answer to a [`FontRequest`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum FontAnswer {
    /// The font was added (with [`FontLibrary::add`]); the family is missing still if the font doesn't have it.
    Font,
    /// Draw the family with another, as an alias for every style would, until aliases or replacements change.
    Family {
        /// The family that draws it.
        family: String,
    },
    /// Draw the family with the default of a generic category, as an alias to it would.
    Category {
        /// The category.
        category: GenericFamily,
    },
    /// Nothing provides it.
    Missing,
    /// The resolver failed (it threw, a download failed, or the bytes weren't a font).
    Failed {
        /// What failed, as the report's `error` says it.
        message: String,
    },
}

/// A draw-time replacement: `from` (optionally only its bold or italic faces) is drawn with `to`, even when `from`
/// is in the library. The document keeps naming `from`: saved files and the formatting a session reports are
/// unchanged. When `to` isn't in the library, `from` resolves as if there were no rule.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontRule {
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

/// An alias: when `from` (optionally only its bold or italic faces) isn't in the library, it is drawn with `to`.
/// Aliases chain (`A → B → C`) up to 8 hops. The document keeps naming `from`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Alias {
    /// The family the document names, case-insensitive.
    pub from: String,
    /// Only its bold (`true`) or regular-weight (`false`) faces; `None` for both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    /// Only its italic (`true`) or upright (`false`) faces; `None` for both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    /// The family, or the generic category, that draws instead.
    pub to: AliasTarget,
}

/// What an [`Alias`] draws with: another family, or the default family of a generic category
/// ([`GenericDefaults`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AliasTarget {
    /// Another family; its own aliases apply when it is missing too.
    Family(String),
    /// The default family of a generic category. When that default is unset or missing, the general fallback draws.
    Category {
        /// The category.
        category: GenericFamily,
    },
}

/// The family that draws a missing font of each generic category. Opt-in: every category is unset by default, and
/// with none set no missing font is drawn through a category. A missing family's category comes from the document's
/// description of it (its font table), from an [`Alias`] to a category, or from a resolver's answer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenericDefaults {
    /// The family for missing serif fonts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serif: Option<String>,
    /// The family for missing sans-serif fonts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sans_serif: Option<String>,
    /// The family for missing fixed-pitch fonts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monospace: Option<String>,
    /// The family for missing script fonts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    /// The family for missing decorative fonts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decorative: Option<String>,
}

impl GenericDefaults {
    /// The family set for `category`.
    pub fn get(&self, category: GenericFamily) -> Option<&str> {
        match category {
            GenericFamily::Serif => self.serif.as_deref(),
            GenericFamily::SansSerif => self.sans_serif.as_deref(),
            GenericFamily::Monospace => self.monospace.as_deref(),
            GenericFamily::Script => self.script.as_deref(),
            GenericFamily::Decorative => self.decorative.as_deref(),
        }
    }
}

/// The face that draws characters no other font has, after the character-fallback families. It is never a family a
/// document can name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SymbolFallback {
    /// The bundled symbol font: bullets, checkboxes, arrows, check marks, card suits and common math signs.
    #[default]
    Bundled,
    /// The first face of this font file (TrueType, OpenType, a collection, WOFF or WOFF2).
    #[serde(skip)]
    Font(std::sync::Arc<Vec<u8>>),
    /// None: a character no font has draws as its font's missing-glyph box.
    Off,
}

/// Receives each font request once.
type Requester = Box<dyn FnMut(&FontRequest)>;

/// What the library holds besides its faces.
#[derive(Default)]
struct Library {
    fonts: Fonts,
    /// Faces no font had when a document needed them, by lowercase family, bold and italic, and where their request
    /// stands. A face added for the family resolves it, whatever its entry says.
    asked: HashMap<(String, bool, bool), Asked>,
    requester: Option<Requester>,
    generation: u64,
}

/// Where a face's request stands.
#[derive(Clone, Debug, PartialEq)]
enum Asked {
    Pending,
    Answered,
    Failed(String),
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

    /// The system's fonts with the settings the fonts guide recommends.
    #[cfg(test)]
    pub(crate) fn system_recommended() -> FontLibrary {
        FontLibrary::with(Fonts::system_recommended())
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

    /// The draw-time replacements, replacing earlier ones. A replacement is the first step of resolution: it applies
    /// even when the replaced family is in the library.
    pub fn set_replacements(&self, rules: Vec<FontRule>) {
        self.remap(|s| s.replacements = rules);
    }

    /// The aliases, replacing earlier ones. An alias applies only when its family isn't in the library; aliases chain
    /// up to 8 hops, and matching is case-insensitive. Families answered by a resolver, or found missing, are
    /// requested again.
    pub fn set_aliases(&self, aliases: Vec<Alias>) {
        self.remap(|s| s.aliases = aliases);
    }

    /// The default family of each generic category, for a missing family of that category. Opt-in:
    /// `GenericDefaults::default()` turns the feature off, and with no default set no family draws through a category.
    pub fn set_generic_defaults(&self, defaults: GenericDefaults) {
        self.change(|s| s.generic = defaults);
    }

    /// Whether a document's font table gives a missing family its generic category. On by default; it has no effect
    /// until a generic default is set. Off, only aliases to a category and resolver answers give one.
    pub fn set_classification(&self, from_document: bool) {
        self.change(|s| s.classify_from_document = from_document);
    }

    /// The face for characters no other font has, after the character-fallback families: the bundled symbol font
    /// (the default), a font of the application's, or none. A font that holds no face fails with `InvalidArgument`
    /// and leaves the setting as it was.
    pub fn set_symbol_fallback(&self, fallback: SymbolFallback) -> Result<(), Error> {
        if let SymbolFallback::Font(bytes) = &fallback {
            Fonts::default().add_bytes(bytes.as_ref().clone())?;
        }
        self.change(|s| s.symbol_fallback = fallback);
        Ok(())
    }

    /// The families searched, in order, for a character a run's font lacks, before the symbol fallback.
    pub fn set_character_fallback(&self, families: Vec<String>) {
        self.change(|s| s.character_fallback = families);
    }

    /// A change to how names map to families: what resolvers answered no longer applies, so every family is asked again.
    fn remap(&self, f: impl FnOnce(&mut FontSettings)) {
        self.change(|s| {
            f(s);
            s.learned.clear();
        });
        self.inner.borrow_mut().asked.clear();
    }

    /// The general fallback: the family that draws a font nothing else resolves (after replacements, aliases,
    /// resolver answers and generic defaults). With none, or when it isn't in the library, the first face added
    /// draws.
    pub fn set_fallback(&self, family: Option<&str>) {
        let family = family.map(str::to_string);
        self.change(|s| s.fallback = family);
    }

    fn change(&self, f: impl FnOnce(&mut FontSettings)) {
        let mut lib = self.inner.borrow_mut();
        lib.fonts.set_settings(f);
        lib.generation += 1;
    }

    /// Ends a pending request with the resolver's answer. A family or category answer applies to every style of
    /// the family, until aliases or replacements change. An answer to a request that isn't pending does nothing.
    pub fn answer(&self, request: &FontRequest, answer: FontAnswer) {
        let key = (request.family.to_lowercase(), request.bold, request.italic);
        let to = match &answer {
            FontAnswer::Family { family } => Some(AliasTarget::Family(family.clone())),
            FontAnswer::Category { category } => Some(AliasTarget::Category { category: *category }),
            FontAnswer::Font | FontAnswer::Missing | FontAnswer::Failed { .. } => None,
        };
        let mut lib = self.inner.borrow_mut();
        if lib.asked.get(&key) != Some(&Asked::Pending) {
            return;
        }
        lib.asked.insert(key, if let FontAnswer::Failed { message } = answer { Asked::Failed(message) } else { Asked::Answered });
        if let Some(to) = to
            && !lib.fonts.settings().learned.iter().any(|a| a.from.eq_ignore_ascii_case(&request.family))
        {
            let alias = Alias { from: request.family.clone(), bold: None, italic: None, to };
            lib.fonts.set_settings(|s| s.learned.push(alias));
        }
        lib.generation += 1;
    }

    /// Where requests for faces the library lacks go, replacing an earlier receiver. Each family and style is
    /// requested once; answer with [`FontLibrary::answer`] (after [`FontLibrary::add`] for a font). With no receiver, a
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

    /// The faces of `faces` (family, its generic category, bold, italic, as a document names them) that draw with a
    /// stand-in: those being fetched and those nothing provided. A face the library can't draw is requested the
    /// first time it is seen. The characters list is left empty: the session fills it.
    pub(crate) fn check<'a>(
        &self,
        faces: impl IntoIterator<Item = (&'a str, Option<GenericFamily>, bool, bool)>,
        descriptions: &HashMap<String, FontDescription>,
    ) -> FontsState {
        // Faces by lowercase family and style: the first spelling seen is the one reported.
        let (mut pending, mut missing, mut requests) = (BTreeMap::new(), BTreeMap::new(), Vec::new());
        {
            let mut lib = self.inner.borrow_mut();
            let Library { fonts, asked, requester, .. } = &mut *lib;
            for (family, category, bold, italic) in faces {
                let (original, entry) = entries(fonts, family, category, bold, italic);
                if let Some(o) = original {
                    missing.entry((o.family.to_lowercase(), bold, italic)).or_insert(o);
                }
                let Some(mut entry) = entry else { continue };
                let lower = family.to_lowercase();
                let key = (lower.clone(), bold, italic);
                let pending_now = match asked.get(&key) {
                    Some(Asked::Pending) => true,
                    Some(Asked::Answered) => false,
                    Some(Asked::Failed(message)) => {
                        entry.error = Some(message.clone());
                        false
                    }
                    // Missing for now, and not remembered: a receiver set later is asked.
                    None if requester.is_none() => false,
                    None => {
                        asked.insert(key.clone(), Asked::Pending);
                        let description = descriptions.get(&lower).cloned();
                        let proposed = description.as_ref().and_then(crate::docx::font_table::classify);
                        requests.push(FontRequest { family: family.to_string(), bold, italic, description, proposed });
                        true
                    }
                };
                if pending_now { &mut pending } else { &mut missing }.entry(key).or_insert(entry);
            }
        }
        // Called with the library released, so a receiver that reads it doesn't fail.
        let requester = if requests.is_empty() { None } else { self.inner.borrow_mut().requester.take() };
        if let Some(mut f) = requester {
            requests.iter().for_each(&mut f);
            self.inner.borrow_mut().requester.get_or_insert(f);
        }
        FontsState { pending: pending.into_values().collect(), missing: missing.into_values().collect(), characters: Vec::new() }
    }
}

/// The faces of `faces` that draw with a stand-in in `fonts`, for a document no library requests fonts for (a PDF
/// conversion): everything missing is missing, nothing is pending.
pub(crate) fn report<'a>(fonts: &Fonts, faces: impl IntoIterator<Item = (&'a str, Option<GenericFamily>, bool, bool)>) -> FontsState {
    let mut missing = BTreeMap::new();
    for (family, category, bold, italic) in faces {
        let (original, entry) = entries(fonts, family, category, bold, italic);
        for m in original.into_iter().chain(entry) {
            missing.entry((m.family.to_lowercase(), m.bold, m.italic)).or_insert(m);
        }
    }
    FontsState { missing: missing.into_values().collect(), ..FontsState::default() }
}

/// What one face reports: the replacement target, when a replacement matched and its target is missing, and the face
/// itself, when it draws with a stand-in.
fn entries(fonts: &Fonts, family: &str, category: Option<GenericFamily>, bold: bool, italic: bool) -> (Option<MissingFont>, Option<MissingFont>) {
    let resolved = fonts.resolve(family, category, bold, italic);
    let original = resolved.replacement.as_deref().filter(|_| resolved.by != Resolution::Replacement).map(|target| {
        let drawn_with = if resolved.by.found() { resolved.family.clone() } else { Some(family.to_string()) };
        MissingFont { family: target.to_string(), bold, italic, tried: Vec::new(), category: None, drawn_with, by: StandIn::Original, error: None }
    });
    let entry = stand_in(resolved.by).map(|by| MissingFont {
        family: family.to_string(),
        bold,
        italic,
        tried: resolved.tried,
        category: resolved.category,
        drawn_with: resolved.family,
        by,
        error: None,
    });
    (original, entry)
}

/// The stand-in a resolution step reports, or `None` when the family drew as the document asked.
fn stand_in(by: Resolution) -> Option<StandIn> {
    match by {
        Resolution::Replacement | Resolution::Itself | Resolution::Alias | Resolution::Learned => None,
        Resolution::Generic(_) => Some(StandIn::Generic),
        Resolution::Fallback => Some(StandIn::Fallback),
        Resolution::LastResort => Some(StandIn::LastResort),
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
    use crate::api::state::MissingFont;

    pub(crate) fn families(list: &[MissingFont]) -> Vec<&str> {
        list.iter().map(|m| m.family.as_str()).collect()
    }

    #[test]
    fn a_face_missing_before_there_is_a_receiver_is_requested_once_there_is_one() {
        let lib = FontLibrary::empty();
        assert_eq!(families(&lib.check([("Carlito", None, false, false)], &Default::default()).missing), ["Carlito"]);
        let asked: Rc<RefCell<Vec<String>>> = Rc::default();
        let sink = asked.clone();
        let generation = lib.generation();
        lib.on_request(move |r| sink.borrow_mut().push(r.family.clone()));
        assert_ne!(lib.generation(), generation, "the sessions check again");
        let state = lib.check([("Carlito", None, false, false)], &Default::default());
        assert_eq!((families(&state.pending), families(&state.missing)), (vec!["Carlito"], vec![]));
        assert_eq!(*asked.borrow(), ["Carlito"]);
        // Asked once.
        lib.check([("Carlito", None, false, false)], &Default::default());
        assert_eq!(asked.borrow().len(), 1);
    }

    fn requests(lib: &FontLibrary) -> Rc<RefCell<Vec<super::FontRequest>>> {
        let asked: Rc<RefCell<Vec<super::FontRequest>>> = Rc::default();
        let sink = asked.clone();
        lib.on_request(move |r| sink.borrow_mut().push(r.clone()));
        asked
    }

    #[test]
    fn a_request_carries_the_documents_description_and_the_category_it_gives() {
        use crate::docx::font_table::{FontClass, FontDescription};
        let lib = FontLibrary::empty();
        let asked = requests(&lib);
        let described = FontDescription { class: Some(FontClass::Roman), ..Default::default() };
        let descriptions = [("garamond".to_string(), described.clone())].into_iter().collect();
        lib.check([("Garamond", None, false, false), ("Zzyzx", None, false, false)], &descriptions);
        let asked = asked.borrow();
        assert_eq!((asked[0].description.as_ref(), asked[0].proposed), (Some(&described), Some(super::GenericFamily::Serif)));
        assert_eq!((asked[1].description.as_ref(), asked[1].proposed), (None, None));
    }

    #[test]
    fn answers_end_requests_as_the_state_diagram_says() {
        use super::{FontAnswer, GenericDefaults, GenericFamily};
        let lib = FontLibrary::empty();
        lib.add(include_bytes!("../../tests/fixtures/editor/fonts/stand-in-serif.ttf")).unwrap();
        lib.add(include_bytes!("../../tests/fixtures/editor/fonts/stand-in-sans.ttf")).unwrap();
        lib.set_generic_defaults(GenericDefaults { serif: Some("Stand-in Serif".into()), ..Default::default() });
        let asked = requests(&lib);
        let faces =
            [("A", None, false, false), ("B", None, false, false), ("C", None, false, false), ("D", None, false, false), ("E", None, false, false)];
        let none = Default::default();
        let state = lib.check(faces, &none);
        assert_eq!(families(&state.pending), ["A", "B", "C", "D", "E"]);
        let request = |n: usize| asked.borrow()[n].clone();
        lib.answer(&request(0), FontAnswer::Family { family: "Stand-in Sans".into() });
        lib.answer(&request(1), FontAnswer::Category { category: GenericFamily::Serif });
        lib.answer(&request(2), FontAnswer::Missing);
        lib.answer(&request(3), FontAnswer::Failed { message: "offline".into() });
        lib.answer(&request(3), FontAnswer::Missing);
        let state = lib.check(faces, &none);
        assert_eq!(families(&state.pending), ["E"]);
        // A family answer resolves; a category answer draws with that category's default.
        assert_eq!(families(&state.missing), ["B", "C", "D"]);
        let b = &state.missing[0];
        assert_eq!(
            (b.by, b.drawn_with.as_deref(), b.category),
            (crate::api::state::StandIn::Generic, Some("Stand-in Serif"), Some(GenericFamily::Serif))
        );
        assert_eq!(state.missing[2].error.as_deref(), Some("offline"), "an answer to a request that isn't pending does nothing");
        assert_eq!(asked.borrow().len(), 5, "each face is asked once");
        // New aliases: what was answered is asked again.
        lib.set_aliases(Vec::new());
        let state = lib.check(faces, &none);
        assert_eq!(families(&state.pending), ["A", "B", "C", "D", "E"]);
        assert_eq!(asked.borrow().len(), 10);
    }

    #[test]
    fn a_symbol_fallback_that_isnt_a_font_is_refused_and_changes_nothing() {
        let lib = FontLibrary::empty();
        let generation = lib.generation();
        let err = lib.set_symbol_fallback(super::SymbolFallback::Font(std::sync::Arc::new(b"not a font".to_vec()))).expect_err("refused");
        assert_eq!(err.code, crate::api::error::ErrorCode::InvalidArgument);
        assert_eq!((lib.generation(), lib.fonts().settings().symbol_fallback.clone()), (generation, super::SymbolFallback::Bundled));
    }
}
