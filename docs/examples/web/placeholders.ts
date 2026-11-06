import type { Editor } from '@twip/web';

export async function placeholders(editor: Editor, icon: Uint8Array, pick: () => Promise<Uint8Array | null>) {
  // A picture control showing the application's placeholder picture: an SVG with percentage geometry lays itself
  // out at whatever size the control has.
  await editor.commands.insertControl({
    kind: 'picture',
    tag: 'logo',
    options: { width: 144, height: 72 },
    placeholderImage: { data: icon, mime: 'image/svg+xml' },
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
