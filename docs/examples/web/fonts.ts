import { Editor, FontLibrary, HeadlessEditor, type FontRequest } from '@twip/web';

// #region adding
// One library per page, shared by every editor. The browser gives the engine no system fonts.
const fonts = FontLibrary.default();

// Files you ship: TTF, OTF, TTC, WOFF or WOFF2, as bytes or a URL. Each returns the faces it added.
await fonts.add('/fonts/Carlito-Regular.ttf');
await fonts.add(new URL('/fonts/Carlito-Bold.ttf', location.href));
// #endregion

// #region aliases
// When a document asks for Arial and the library doesn't have it, draw Arimo. A family the library has is never
// aliased, the document keeps naming Arial, and chains (A → B → C) are followed.
fonts.setAliases({ Arial: 'Arimo', 'Times New Roman': 'Tinos' });

// Per style, or to a generic category's default.
fonts.setAliases([
  { from: 'Arial', to: 'Arimo' },
  { from: 'Garamond', bold: true, to: 'EB Garamond SemiBold' },
  { from: 'Brand Script', to: { category: 'script' } },
]);
// #endregion

// #region replacements
// Always draw Comic Sans MS with the house font, even where Comic Sans MS is installed. The document is unchanged.
fonts.setReplacements({ 'Comic Sans MS': 'Inter' });
// #endregion

// #region genericDefaults
// Opt-in: a missing family draws with the default of its category, which the document's font table gives it.
fonts.setGenericDefaults({ serif: 'Tinos', sansSerif: 'Arimo', monospace: 'Cousine', script: 'Dancing Script', decorative: 'Lobster' });

// Classify only through aliases to a category and resolver answers, not the font table.
fonts.setClassification({ fromDocument: false });

// {} turns generic defaults off again.
fonts.setGenericDefaults({});
// #endregion

// #region fallback
// The family for anything nothing else resolves. With null, the first face added draws.
fonts.setFallback('Arimo');
// #endregion

// #region characterFallback
// Searched in order for a character the run's font doesn't have, then the symbol fallback.
fonts.setCharacterFallback(['Noto Sans Symbols 2', 'Noto Sans CJK JP', 'Noto Color Emoji']);
// #endregion

// #region symbolFallback
// The bundled symbol font draws bullets and checkboxes no other font has. Replace it, or turn it off.
await fonts.setSymbolFallback('/fonts/MySymbols.ttf');
await fonts.setSymbolFallback('off');
await fonts.setSymbolFallback('bundled');
// #endregion

// #region resolver
// Asked once per family and style the document uses and the library lacks. Return bytes, a URL, or null.
fonts.setResolver(async (request, signal) => {
  const style = `${request.bold ? 'Bold' : ''}${request.italic ? 'Italic' : ''}` || 'Regular';
  const response = await fetch(`/fonts/${encodeURIComponent(request.family)}-${style}.woff2`, { signal });
  return response.ok ? new Uint8Array(await response.arrayBuffer()) : null;
});
// #endregion

// #region resolverCategory
// Or decide how to draw it: another family, or a category's default. The request says what the document's font
// table states about the family (`description`) and the category that gives (`proposed`).
fonts.setResolver(async (request: FontRequest) => {
  if (request.family === 'Corporate Sans') return { family: 'Inter' };
  if (request.proposed) return { category: request.proposed };
  return request.description?.pitch === 'fixed' ? { category: 'monospace' } : null;
});
// #endregion

// #region reports
const editor = await HeadlessEditor.create({ fonts });
editor.on('fontsChanged', ({ pending, missing, characters }) => {
  for (const m of missing) {
    const style = [m.bold && 'bold', m.italic && 'italic'].filter(Boolean).join(' ');
    console.log(`${m.family} ${style}: drawn with ${m.drawnWith ?? 'nothing'} (${m.by})${m.error ? `, ${m.error}` : ''}`);
  }
  for (const c of characters) console.log(`no font has ${c.characters} (${c.family})`);
  console.log(`${pending.length} still loading`);
});
console.log(editor.getState().fonts, await fonts.families());
// #endregion

// #region replaceFont
// Rewrite the document itself: every Calibri becomes Aptos, in the text, styles, theme, numbering and font table.
const { changed, skipped } = await editor.commands.replaceFont({ from: 'Calibri', to: 'Aptos' });
console.log(`${changed} places changed, ${skipped} in locked content controls`);

// Only the selected text, or only bold text.
await editor.commands.replaceFont({ from: 'Calibri', to: 'Aptos', scope: 'selection' });
await editor.commands.replaceFont({ from: 'Calibri', to: 'Aptos Display', bold: true });
// #endregion

// #region recommended
// Metric-compatible open families for common office fonts: the same widths, so the same line and page breaks.
fonts.setAliases({
  Calibri: 'Carlito',
  'Calibri Light': 'Carlito',
  Cambria: 'Caladea',
  Arial: 'Arimo',
  Helvetica: 'Arimo',
  'Times New Roman': 'Tinos',
  Times: 'Tinos',
  'Courier New': 'Cousine',
  Courier: 'Cousine',
});
fonts.setGenericDefaults({ serif: 'Tinos', sansSerif: 'Arimo', monospace: 'Cousine' });
fonts.setFallback('Arimo');
// #endregion

// #region googleFonts
// Google Fonts through a resolver: the css2 stylesheet for a family and style, then the font file it names.
fonts.setResolver(async ({ family, bold, italic }, signal) => {
  const name = encodeURIComponent(family).replace(/%20/g, '+');
  // Google answers a family it doesn't have with a 400 that has no CORS header, so the fetch rejects: not found.
  const css = await fetch(`https://fonts.googleapis.com/css2?family=${name}:ital,wght@${italic ? 1 : 0},${bold ? 700 : 400}`, { signal }).catch(() => null);
  if (!css?.ok) return null;
  const url = /src:\s*url\(([^)]+)\)/.exec(await css.text())?.[1];
  return url ?? null;
});
// #endregion

void Editor;
