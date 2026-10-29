import { FontLibrary, HeadlessEditor } from '@twip/web';

// One library per page, shared by every editor. The browser gives the engine no system fonts.
const fonts = FontLibrary.default();

// Add files you ship: TTF, OTF, TTC, WOFF or WOFF2, as bytes or a URL.
await fonts.add('/fonts/Carlito-Regular.ttf');
await fonts.add(new URL('/fonts/Carlito-Bold.ttf', location.href));

// Draw a family the document names with one you have. Metric-compatible stand-ins keep the line breaks.
fonts.substitute({ Calibri: 'Carlito', Cambria: 'Caladea', 'Times New Roman': 'Tinos' });
fonts.setDefault('Carlito'); // for a family nothing else resolves

// Fetch a family the first time a document needs it. Return null when you don't have it.
fonts.setResolver(async (request, signal) => {
  const style = `${request.bold ? 'Bold' : ''}${request.italic ? 'Italic' : ''}` || 'Regular';
  const response = await fetch(`/fonts/${encodeURIComponent(request.family)}-${style}.woff2`, { signal });
  return response.ok ? new Uint8Array(await response.arrayBuffer()) : null;
});

// Or take what the resolver doesn't have from Google Fonts.
fonts.useGoogleFonts();

const editor = await HeadlessEditor.create({ fonts });
editor.on('fontsChanged', ({ pending, unresolved }) => console.log({ pending, unresolved }));
console.log(await fonts.families()); // for a font box
