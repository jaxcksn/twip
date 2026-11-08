import { Editor } from '@twip/web';

const container = document.querySelector<HTMLElement>('#editor')!;

// The name screen readers say when the editor gets the focus. Put a visible label near the container too.
const editor = await Editor.mount(container, { label: 'Letter body' });

// What the editor says to screen readers, for a log or your own live region.
editor.on('announcement', ({ text, kind }) => console.log(`${kind}: ${text}`));

// Only some kinds: the others are neither spoken nor emitted.
await editor.accessibility.setAnnouncements(['table', 'list', 'review', 'comment', 'spelling', 'mode', 'readOnly', 'command', 'selection']);

// A navigation pane of your own: go to the first heading, as a screen reader's Enter on it does.
const tree = await editor.accessibility.tree();
const heading = tree.children.find((n) => n.role === 'heading');
if (heading) await editor.accessibility.go(heading.id);

// Shift+F1 says the formatting at the caret; a toolbar button can do the same.
await editor.commands.describeFormatting();
