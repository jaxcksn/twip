import type { ChromeTheme, Editor, HeadlessEditor, Popover } from '@twip/web';

// #region appearance
export async function appearance(editor: Editor) {
  // Dark chrome with the application's accent; the dark scheme lightens the accent so it holds on dark surfaces.
  await editor.view.setTheme('dark', '#7A3EE0');
  // A dark page, the document's colours adjusted to it. 'darkWithLightPage' keeps the page in its own colours.
  await editor.view.setAppearance('dark');
  // Everything measured while dragging: the change from the start, table sizes and status lines.
  await editor.view.setDetail('full');
  // The same settings in one command, with the object grid shown always.
  await editor.commands.setViewOptions({ theme: 'light', appearance: 'light', detail: 'standard', objectGrid: 'always' });

  // The state reports the accent as drawn and the text colour on it, for chrome that matches the editor's.
  const { theme, accent, accentOn } = editor.getState().view;
  document.documentElement.style.setProperty('--accent', accent);
  document.documentElement.style.setProperty('--on-accent', accentOn);
  console.log(typeof theme === 'object' ? 'high contrast' : theme);
}

// Following the platform's dark mode. Under forced colours the mounted editor sets the high-contrast theme itself.
export function followDarkMode(editor: Editor): () => void {
  const dark = matchMedia('(prefers-color-scheme: dark)');
  const follow = () => {
    const theme: ChromeTheme = dark.matches ? 'dark' : 'light';
    void editor.view.setTheme(theme);
    void editor.view.setAppearance(dark.matches ? 'dark' : 'light');
  };
  follow();
  dark.addEventListener('change', follow);
  return () => dark.removeEventListener('change', follow);
}

// The application's own popovers: listening for popoverRequested turns the editor's off.
export function ownPopovers(editor: Editor, show: (p: Popover, pick: (choice: string) => Promise<void>) => void, hide: () => void) {
  editor.on('popoverRequested', (p) => {
    switch (p.kind) {
      case 'tabType':
        // A choice's value; setRulerTabType closes the engine's popover.
        show(p, (value) => editor.commands.setRulerTabType(p.choices.find((c) => c.value === value)?.value ?? p.current));
        break;
      case 'tabStop':
        // addTabStop at its position changes the alignment; deleteTabStop clears it.
        show(p, (choice) =>
          choice === 'clear' ? editor.commands.deleteTabStop(p.position) : editor.commands.addTabStop({ position: p.position, alignment: p.alignment, leader: choice === 'dots' ? 'dots' : 'none' }),
        );
        break;
      case 'autoCorrect':
        show(p, (choice) => (choice === 'never' ? editor.commands.neverAutoCorrect() : editor.commands.revertAutoCorrect()));
        break;
    }
  });
  // The engine dismissed it: Esc, a press elsewhere, or its target gone.
  editor.on('popoverClosed', hide);
}

// #endregion

// #region autocorrect
export async function autoCorrect(editor: HeadlessEditor) {
  // Typing corrects as it goes. Switch rules off, replace the list, and never correct some texts.
  await editor.commands.setAutoCorrect({
    smartQuotes: false,
    entries: [
      { from: 'teh', to: 'the' },
      { from: '(c)', to: '©' },
    ],
    never: ['adn'],
  });
  console.log(editor.getState().autoCorrect.never);

  // The correction just made: put back what was typed, or that and never again.
  if (editor.getState().commands.revertAutoCorrect.enabled) await editor.commands.revertAutoCorrect();
  if (editor.getState().commands.neverAutoCorrect.enabled) await editor.commands.neverAutoCorrect();

  // "AutoCorrect rules…" asks for the application's own dialog.
  editor.on('dialogRequested', ({ kind }) => {
    if (kind === 'autoCorrect') console.log('open the AutoCorrect rules dialog');
  });
}
// #endregion
