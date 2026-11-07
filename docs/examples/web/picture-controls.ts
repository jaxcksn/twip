import type { Editor } from '@twip/web';

export async function pictureControls(editor: Editor, icon: Uint8Array, pick: () => Promise<Uint8Array | null>) {
  // A picture control showing the application's placeholder picture: an SVG with percentage geometry lays itself
  // out at whatever size the control has.
  await editor.commands.insertControl({
    kind: 'picture',
    tag: 'signature',
    options: { width: 144, height: 36 },
    placeholderImage: { data: icon, mime: 'image/svg+xml' },
  });

  // A floating one keeps its place on the page whatever the text around it does.
  await editor.commands.insertControl({
    kind: 'picture',
    tag: 'seal',
    placeholderImage: { data: icon, mime: 'image/svg+xml' },
    float: { wrap: 'square', horizontal: { relativeTo: 'margin', offset: 360 }, vertical: { relativeTo: 'paragraph', offset: 0 } },
  });

  // These pictures change only through fillControl and setControl.
  await editor.commands.setProtectedPictureControls({ tagPrefixes: ['signature', 'seal'] });

  // A click on an unfilled picture control, a double-click or Enter activates it: supply its picture.
  editor.on('pictureControlActivated', async ({ element }) => {
    const image = await pick();
    if (image) await editor.commands.fillControl({ id: element, image });
  });
}

// The picture controls still showing their placeholder, to fill with no click.
export const unfilled = async (editor: Editor) =>
  (await editor.elements.list()).filter((e) => e.size && e.showingPlaceholder);
