import type { Editor } from '@twip/web';

// Ctrl+C, Ctrl+X and Ctrl+V need nothing: a mounted editor owns the clipboard. Buttons call the commands.
export function clipboardButtons(editor: Editor, cut: HTMLButtonElement, copy: HTMLButtonElement, paste: HTMLButtonElement) {
  editor.subscribe((state) => {
    cut.disabled = !state.commands.cut.enabled;
    copy.disabled = !state.commands.copy.enabled; // enabled for a selection, also when read only
    paste.disabled = !state.commands.paste.enabled;
  });
  cut.onclick = () => editor.commands.cut();
  copy.onclick = () => editor.commands.copy();
  paste.onclick = async () => {
    try {
      await editor.commands.paste(); // reads the system clipboard
    } catch (e) {
      // The browser may refuse a button's clipboard read ('not-allowed'). Ctrl+V always works.
      console.warn('Press Ctrl+V to paste', e);
    }
  };
}

// Text from elsewhere (a snippet library, a chat message) goes in at the selection, as one undo step.
export const insertSnippet = (editor: Editor, text: string) => editor.commands.pasteText(text);
