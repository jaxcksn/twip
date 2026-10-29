import { type HeadlessEditor, inches, type Op } from '@twip/web';

export async function format(editor: HeadlessEditor) {
  // One method per command. Each resolves after its change commits.
  await editor.commands.setFontSize(14);
  await editor.commands.setIndents({ left: inches(0.5), firstLine: inches(0.25) });
  await editor.commands.insertTable({ rows: 3, columns: 2 });

  // A batch is atomic: one transaction and one undo step, or nothing when an op fails.
  const ops: Op[] = [
    { type: 'insertText', value: 'Total' },
    { type: 'setBold', value: true },
    { type: 'setAlignment', value: 'right' },
  ];
  await editor.batch(ops, { label: 'Add total' });
  await editor.commands.undo(); // undoes the whole batch

  // The catalog describes every command, for a command palette or a shortcut list.
  for (const command of editor.catalog.filter((c) => c.group === 'table')) {
    console.log(command.id, command.label, command.shortcuts.windows.join(', '));
  }
}
