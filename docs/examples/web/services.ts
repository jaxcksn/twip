import type { Editor, MenuItem } from '@twip/web';

export function wireServices(editor: Editor, showMenu: (items: MenuItem[], x: number, y: number) => void) {
  // While this listener is registered, the browser's own menu never opens over the document.
  editor.on('contextMenu', ({ menu, at }) => showMenu(menu.items, at.clientX, at.clientY));
  // ...and when the user picks an item: editor.menus.invoke(item.action)

  // A key or gesture asked for a dialog: draw it from its model, then apply it as one undo step.
  editor.on('dialogRequested', async ({ kind }) => {
    if (kind !== 'paragraph') return;
    const dialog = await editor.dialogs.open('paragraph');
    const next = await dialog.update({ keepWithNext: true });
    console.log(next.values.left, next.fields.left?.label);
    await next.apply();
  });

  // An application shortcut. Registering a key the editor already uses fails, naming the collision.
  const offShortcut = editor.shortcuts.register({ id: 'insertSignature', keys: 'Ctrl+Alt+S' });
  editor.on('shortcut', ({ id }) => {
    if (id === 'insertSignature') void editor.commands.insertText('Kind regards,');
  });
  return offShortcut;
}

export async function findAndReplace(editor: Editor) {
  const query = { text: 'colour', matchCase: false, wholeWords: true };
  const { found, wrapped } = await editor.find.next(query); // selects and reveals the match
  console.log(found, wrapped);
  for (const match of await editor.find.all(query)) console.log(match.page, `${match.before}[${match.text}]${match.after}`);
  const { count } = await editor.find.replaceAll(query, 'color'); // one transaction
  console.log(`${count} replaced`);
}

export async function spelling(editor: Editor, aff: Uint8Array, dic: Uint8Array, saved: string[]) {
  await editor.spelling.addDictionary({ language: 'en-US', aff, dic });
  await editor.spelling.loadUserWords('en-US', saved);
  editor.on('wordAdded', ({ word }) => saved.push(word)); // persist the user's dictionary
  const next = await editor.spelling.next(); // selects the next misspelling
  if (next) console.log(next.word, next.suggestions);
}
