import { Editor, preload } from '@twip/web';

// On a page shown before the editor (a sign-in screen, a document list): start the engine now, in the background.
void preload();

// Later, when the user opens a document, the editor doesn't wait for the download or the compile.
const editor = await Editor.mount(document.querySelector<HTMLElement>('#editor')!);
await editor.open(await (await fetch('/documents/letter.docx')).bytes());
