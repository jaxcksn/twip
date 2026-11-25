//! Clipboard pictures: copied objects drawn as PNG

use crate::editing::Editor;
use crate::editing::clipboard::{ClipPicture, Fragment};
use crate::editing::paste::MAX_DRAWN_PIXELS;
use crate::font::Fonts;

/// The picture a copy of an object puts on the clipboard: the selected object (or objects) drawn as on the page, at
/// 96 dpi, with transparency, cut to what it draws (its effects included). `None` when the copy isn't of an object.
pub fn image(f: &Fragment, fonts: &Fonts) -> Option<ClipPicture> {
    let items = f.image.as_ref()?;
    picture(items, fonts, &f.images(), u64::MAX).map(|(p, _)| p)
}

/// The copy's HTML, its objects drawn as PNG pictures in it, at most [`MAX_DRAWN_PIXELS`] in all (a copy of an object
/// whatever its size): the objects past that are their alternative text. Objects not drawn when copied
/// ([`Editor::copy_drawn`]) are drawn now, by `editor`, the one they were copied from.
pub fn html(editor: &Editor, f: &Fragment, fonts: &Fonts) -> String {
    let later;
    let drawn = if f.drawn.is_empty() {
        later = editor.drawings(f, fonts);
        &later[..]
    } else {
        &f.drawn[..]
    };
    if drawn.iter().all(Option::is_none) {
        return f.html();
    }
    let images = f.images();
    // A copy of one object embeds it whatever its size, as its picture on the clipboard is.
    let mut budget = if f.image.is_some() { u64::MAX } else { MAX_DRAWN_PIXELS };
    let pictures: Vec<Option<ClipPicture>> = drawn
        .iter()
        .map(|d| {
            let (p, px) = picture(d.as_ref()?, fonts, &images, budget)?;
            budget -= px;
            Some(p)
        })
        .collect();
    f.html_with(&pictures)
}

/// Items drawn as a clipboard picture of at most `max_pixels`, with the pixels it takes.
fn picture(items: &[crate::layout::fragment::Clipped], fonts: &Fonts, images: &[Vec<u8>], max_pixels: u64) -> Option<(ClipPicture, u64)> {
    let (png, [w, h], size) = crate::render::preview::object_png(items, fonts, images, max_pixels)?;
    Some((ClipPicture { png, size }, u64::from(w) * u64::from(h)))
}
