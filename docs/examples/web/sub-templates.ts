import type { Editor, SubTemplateInfo } from '@twip/web';

/** Declares which fields are sub-template references: `SUBTEMPLATE <name>` names a building block in "Sub-templates". */
export async function declareSubTemplates(editor: Editor) {
  await editor.commands.setSubTemplateRules({ rules: [{ fieldType: 'SUBTEMPLATE', category: 'Sub-templates' }] });
  // Each reference shows its sub-template's body in its place; show a reference's field instead:
  // await editor.commands.setSubTemplateView({ expanded: false });
}

/** A panel listing the sub-templates; a click edits one in the same editor, and a banner says which. */
export async function subTemplatePanel(
  editor: Editor,
  render: (list: SubTemplateInfo[], onPick: (name: string) => void) => void,
  banner: (text: string | null, back: () => void) => void,
) {
  render(await editor.subTemplates.list(), (name) => void editor.subTemplates.edit(name));
  // Twip draws nothing for the switch: the application shows it in its own UI.
  editor.on('editingTargetChanged', (target) => {
    if (target.kind === 'subTemplate') banner(`Editing sub-template: ${target.name}`, () => void editor.subTemplates.close());
    else banner(null, () => {});
  });
  // Typing into a sub-template's body shown in place of its reference is refused: say where to edit it.
  editor.on('editRefused', ({ reason }) => {
    if (reason.kind === 'subTemplate') console.log(`Edit ${reason.name} to change this text.`);
  });
}

/** Authoring: a new sub-template from a DOCX the application converted, a rename that follows the references, and
 * a removal. Each is one undo step. */
export async function manageSubTemplates(editor: Editor, docx: Uint8Array) {
  await editor.subTemplates.create('Payment instructions', { from: docx });
  await editor.subTemplates.rename('Violation', 'Citation', { updateReferences: true });
  await editor.subTemplates.remove('Old notice');
}
