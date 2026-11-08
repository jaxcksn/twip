import { Editor } from '@twip/web';

// 'auto' (the default) runs the engine in a worker where the browser supports it, so layout never blocks input.
const editor = await Editor.mount(document.querySelector<HTMLElement>('#editor')!, { execution: 'auto' });
console.log(editor.executionMode); // 'worker' or 'main'

// The API is the same in both modes: operations are promises, and getState() is synchronous.
await editor.commands.insertText('Same code, either thread.');
console.log(editor.getState().revision);
