import type { Editor } from '@twip/web';

export async function placeholders(editor: Editor, logo: Uint8Array, pick: () => Promise<Uint8Array | null>) {
  // How unfilled picture placeholders look: by element, by tag, by tag prefix, or the default. A view setting.
  await editor.commands.setPlaceholderLooks({
    looks: [
      { match: { tag: 'logo' }, look: { image: logo, mime: 'image/png' } },
      { match: { prefix: 'photo.' }, look: { fill: '#EEF3FA', label: 'Click to add a photo' } },
      { match: 'default', look: { label: 'Picture' } },
    ],
  });

  // A click on an unfilled placeholder selects it and emits this: supply its picture.
  editor.on('placeholderActivated', async ({ key }) => {
    const image = await pick();
    if (image) await editor.commands.fillPlaceholder({ image });
    console.log(`filled ${key}`);
  });
}

// Fill every unfilled placeholder with one key at once, with no click.
export const fillAll = (editor: Editor, key: string, image: Uint8Array) => editor.commands.fillPlaceholder({ key, image });
