// Drives the engine through the generated bindings in Node; fails loudly if the binding layer is broken.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { Editor, Fonts, attachWebGpu, paintWebGpu, webGlTarget, paintWebGl } from './dist/index.mjs';

const fonts = new Fonts();
// Node has no system fonts for the engine: give it a font, if this machine has one.
for (const f of ['C:/Windows/Fonts/calibri.ttf', '/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf']) {
  try { fonts.addFont(readFileSync(f)); break; } catch {}
}
const ctrl = { ctrl: true, shift: false, alt: false, meta: false };
const editor = new Editor(fonts);
editor.open(readFileSync(new URL('../hello.docx', import.meta.url)));
assert.equal(editor.pageCount, 1);

editor.key('End', ctrl);
editor.text(' Typed through the SDK.');
editor.selectAll();
editor.bold();
editor.key('End', ctrl);
editor.insertTable(2, 3);
const docx = Uint8Array.from(editor.save().asSlice());
assert.ok(docx.length > 100 && docx[0] === 0x50, `save: ${docx.length} bytes`);

const requests = editor.takeRequests();
assert.ok(editor.caret().height > 0, 'no caret');
editor.setViewport(0, 0, 200, 150, 1);
const frame = editor.paint(200, 150, 0, 0, 1);
assert.ok(frame && frame.len === 200 * 150 * 4, 'paint');
assert.ok(new Set(frame.asSlice()).size > 2, 'painted a blank frame');

// Queries: status, a selection round trip through selectRange, the reading tree, elements.
const status = editor.status();
assert.ok(status.page === 1 && status.pages === 1 && status.words >= 3, JSON.stringify(status));
editor.selectAll();
let sel = editor.selection();
assert.equal(sel.kind.value, "Text");
editor.selectRange(sel.anchorParagraph, 0, 5);
sel = editor.selection();
assert.deepEqual([sel.anchorOffset, sel.focusOffset], [0, 5]);
const tree = editor.accessibleTree();
assert.ok(tree.role() === "document" && tree.childCount > 0, "empty reading tree");
assert.equal(editor.elements("").len, 0);

const reopened = new Editor(fonts);
reopened.open(docx);
const pdf = reopened.exportPdf('Smoke');
assert.equal(String.fromCharCode(...pdf.asSlice().slice(0, 5)), '%PDF-');

assert.throws(() => editor.open(new Uint8Array([])), /OpenError.Empty/);
assert.throws(() => editor.open(new Uint8Array([1, 2, 3])), /OpenError.Invalid/);
console.log(`${editor.describe()}, saved ${docx.length} bytes, ${requests.len} request(s), PDF ${pdf.len} bytes`);
// The browser GPU paths need a browser; here they only have to load and link.
for (const f of [attachWebGpu, paintWebGpu, webGlTarget, paintWebGl]) assert.equal(typeof f, 'function');
assert.equal(typeof Editor.webgpuInstance(), 'number');
console.log('ok');
