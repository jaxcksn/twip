import { Editor } from '@twip/web';

// The container needs a size: the editor fills it and scrolls inside it.
const container = document.querySelector<HTMLElement>('#editor')!;
const response = await fetch('/report.docx');

const editor = await Editor.mount(container, {
  document: new Uint8Array(await response.arrayBuffer()),
  author: { name: 'Ada Lovelace', initials: 'AL' },
  rulers: true,
  zoom: 'pageWidth',
});
editor.focus();

await editor.commands.selectAll();
await editor.commands.setBold(true);
console.log(editor.getState().formatting.bold); // true: the state is already at the command's revision

const { bytes } = await editor.export({ format: 'docx' });
const download = document.createElement('a');
download.href = URL.createObjectURL(new Blob([bytes.slice()]));
download.download = 'report.docx';
download.click(); // or send the bytes to your server

await editor.destroy(); // releases the session and everything the editor added to the container
