import { EditorError, HeadlessEditor } from '@twip/web';

// No DOM: Node, a worker, a test or a server.
const editor = await HeadlessEditor.create({ author: { name: 'Report bot' } });
try {
  const { warnings } = await editor.open(new Uint8Array(await (await fetch('/template.docx')).arrayBuffer()));
  for (const warning of warnings) console.warn(warning.code, warning.message);

  await editor.commands.moveSelection({ by: 'document', direction: 'forward' });
  await editor.commands.insertText('Generated on the server.');
  const pdf = await editor.export({ format: 'pdf' });
  console.log(`${pdf.bytes.length} bytes at revision ${pdf.revision}`);
} catch (e) {
  if (e instanceof EditorError && e.code === 'import-failed') console.error('not a document Twip can read');
  else throw e;
} finally {
  await editor.destroy();
}
