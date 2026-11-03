import type { ColourValue, Editor, FillValue, ThemeColour } from '@twip/web';

export async function shapes(editor: Editor) {
  // A shape at the caret's paragraph, in front of the text, in the theme's default style. It is selected.
  await editor.commands.insertShape({ preset: 'roundRect', width: 144, height: 72 });

  // The selection describes it as written: a theme colour stays a theme colour.
  const shape = editor.getState().selection.object?.shape;
  if (shape && 'fromStyle' in shape.fill) console.log('styled fill', shape.fill.fromStyle);

  // Accent 1 at 75% luminance, a 2 pt dashed outline, a soft shadow, a quarter turn.
  const accent: ColourValue = { theme: 'accent1', transforms: [{ kind: 'lumMod', value: 0.75 }] };
  await editor.commands.setShapeFill({ type: 'solid', colour: accent });
  await editor.commands.setShapeLine({ width: 2, dash: 'dash', fill: { type: 'solid', colour: { theme: 'text1' } } });
  await editor.commands.setShapeEffects({
    outerShadow: { colour: { rgb: '#000000', transforms: [{ kind: 'alpha', value: 0.4 }] }, blur: 4, distance: 3, direction: 45 },
  });
  await editor.commands.setObjectRotation(90);

  // `fromStyle` takes the explicit fill away again.
  await editor.commands.setShapeFill({ type: 'fromStyle' });
}

// A colour picker from the document's theme colours (state.themeColours, in ThemeColour order).
export function themePicker(editor: Editor): { colour: ThemeColour; rgb: string; fill: FillValue }[] {
  const names: ThemeColour[] = [
    'text1', 'background1', 'text2', 'background2', 'accent1', 'accent2',
    'accent3', 'accent4', 'accent5', 'accent6', 'hyperlink', 'followedHyperlink',
  ];
  return names.map((colour, i) => ({
    colour,
    rgb: editor.getState().themeColours[i] ?? '#000000',
    fill: { type: 'solid', colour: { theme: colour } },
  }));
}

// An "Insert shape" menu from Word's gallery groups.
export function galleryMenu(editor: Editor) {
  return editor.shapeGallery.map((group) => ({
    label: group.label,
    items: group.presets.map((preset) => ({ label: preset, run: () => editor.commands.insertShape({ preset }) })),
  }));
}

// Grouping: select two floating shapes (Shift+click in the view), then group, and ungroup again.
export async function group(editor: Editor) {
  if ((editor.getState().selection.object?.count ?? 0) >= 2) {
    await editor.commands.groupObjects();
    await editor.commands.ungroupObjects();
  }
}
