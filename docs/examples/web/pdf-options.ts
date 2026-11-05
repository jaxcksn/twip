import { HeadlessEditor } from '@twip/web';

const editor = await HeadlessEditor.create();
await editor.open(new Uint8Array(await (await fetch('/notice.docx')).arrayBuffer()));
// #region options
// An open password encrypts the PDF (AES-256); keywords go to its metadata.
const pdf = await editor.export({ format: 'pdf', openPassword: 's3cret', keywords: ['2026-CV-1042', 'notice'] });
// A protected PDF can't claim PDF/A conformance, so the export says it left the claim out.
console.log(pdf.warnings.some((w) => w.code === 'pdf-archival-claim-dropped')); // true
// #endregion
await editor.destroy();
