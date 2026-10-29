// Keyboard input as the engine takes it: a key number from KeyboardEvent.code and a modifier bit set.

/**
 * The key table of the C ABI (`TwipKey` in `twip.h`), by `KeyboardEvent.code`, so the physical key is what counts and
 * the layout is the engine's business. 0 is no key. The numbers must match the engine's table exactly.
 */
const numbers: Readonly<Record<string, number>> = {
  KeyA: 1, KeyB: 2, KeyC: 3, KeyD: 4, KeyE: 5, KeyF: 6, KeyG: 7, KeyH: 8, KeyI: 9, KeyJ: 10, KeyK: 11, KeyL: 12,
  KeyM: 13, KeyN: 14, KeyO: 15, KeyP: 16, KeyQ: 17, KeyR: 18, KeyS: 19, KeyT: 20, KeyU: 21, KeyV: 22, KeyW: 23,
  KeyX: 24, KeyY: 25, KeyZ: 26, Digit0: 27, Digit1: 28, Digit2: 29, Digit3: 30, Digit4: 31, Digit5: 32, Digit6: 33,
  Digit7: 34, Digit8: 35, Digit9: 36, F1: 37, F2: 38, F3: 39, F4: 40, F5: 41, F6: 42, F7: 43, F8: 44, F9: 45,
  F10: 46, F11: 47, F12: 48, Enter: 61, Tab: 62, Backspace: 63, Delete: 64, Escape: 65, Space: 66, Insert: 67,
  Home: 68, End: 69, PageUp: 70, PageDown: 71, ArrowLeft: 72, ArrowRight: 73, ArrowUp: 74, ArrowDown: 75,
  ContextMenu: 76, Minus: 77, Equal: 78, BracketLeft: 79, BracketRight: 80, Backslash: 81, Semicolon: 82, Quote: 83,
  Backquote: 84, Comma: 85, Period: 86, Slash: 87, IntlBackslash: 88, NumpadEnter: 89, NumpadAdd: 90,
  NumpadSubtract: 91, NumpadMultiply: 92, NumpadDivide: 93, NumpadDecimal: 94, Numpad0: 95, Numpad1: 96, Numpad2: 97,
  Numpad3: 98, Numpad4: 99, Numpad5: 100, Numpad6: 101, Numpad7: 102, Numpad8: 103, Numpad9: 104,
};

/** A key's number for the engine, or 0 when the engine doesn't know it. */
export function keyNumber(code: string): number {
  return Object.hasOwn(numbers, code) ? numbers[code]! : 0;
}

/** Modifier bits of the C ABI. */
export const Mods = { ctrl: 1, shift: 2, alt: 4, meta: 8 } as const;

/** A keyboard event's modifier bits. */
export function modsOf(e: { shiftKey: boolean; ctrlKey: boolean; altKey: boolean; metaKey: boolean }): number {
  return (e.shiftKey ? Mods.shift : 0) | (e.ctrlKey ? Mods.ctrl : 0) | (e.altKey ? Mods.alt : 0) | (e.metaKey ? Mods.meta : 0);
}
