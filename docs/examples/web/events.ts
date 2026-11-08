import type { Editor } from '@twip/web';

export function listen(editor: Editor, save: () => void) {
  const off = [
    // Exactly one per commit, in order.
    editor.on('documentChanged', (record) => console.log(record.revision, record.origin, record.label)),
    // Keys the application owns: the browser's Save, Print and Find never open.
    editor.on('hostCommand', ({ command }) => {
      if (command === 'save') save();
    }),
    editor.on('linkActivated', ({ address }) => window.open(address, '_blank', 'noopener')),
    editor.on('warning', (warning) => console.warn(warning.code, warning.message)),
    // A listener's exception lands here instead of breaking the editor. A fault has `fatal: true`.
    editor.on('error', (error) => {
      console.error(error.code, error.message);
      if (error.fatal) void editor.destroy();
    }),
  ];
  return () => off.forEach((unsubscribe) => unsubscribe());
}
