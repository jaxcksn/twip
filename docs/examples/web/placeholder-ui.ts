import type { Editor, PlaceholderTarget } from '@twip/web';

export async function placeholderUi(
  editor: Editor,
  popover: (target: PlaceholderTarget | null) => void,
  sign: (key: string) => Promise<Uint8Array | null>,
) {
  // Placeholders drawn as editor UI: field marks, repeating sections, sub-template cards and input blocks.
  await editor.commands.setMode('design');
  await editor.view.setPlaceholderUi(true);
  await editor.commands.setFieldTypes({ types: { 'MERGEFIELD ApplicantName': 'Text' } });

  // Your popover, at the selected field's anchor; null when the selection leaves it.
  editor.on('placeholderSelected', (target) => popover(target));

  // Signature blocks: their colour, label, hint and buttons.
  await editor.commands.setPlaceholderLooks({
    looks: [
      {
        match: { prefix: 'Signature' },
        look: { fill: '#2E7D32', label: 'Signature' },
        hint: 'Click to sign',
        actions: [{ id: 'sign', label: 'Sign', primary: true }, { id: 'upload', label: 'Upload image' }],
      },
    ],
  });
  editor.on('placeholderActivated', async ({ key, action }) => {
    if (action !== 'sign' && action !== 'replace') return;
    const image = await sign(key);
    if (image) await editor.commands.fillPlaceholder({ key, image });
  });

  // Fields the data source lacks, and the sub-templates' versions.
  await editor.commands.setUnmappedFields({ names: ['Inspector_Name'] });
  await editor.commands.setSubTemplateVersions({ versions: [{ name: 'Appeals notice', version: 'v4', newer: 'v5 available' }] });
  editor.on('subTemplateUpdateRequested', ({ name, category }) => console.log(`Update ${name} in ${category}`));
}
