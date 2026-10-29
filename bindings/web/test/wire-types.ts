// Writes wire.generated.ts: every example of the engine's wire fixtures as `(example) satisfies Wire<PublicType>`,
// so `tsc -p test` fails when a public type doesn't accept the JSON the engine sends or takes, extra fields included.
// Run with `node test/wire-types.ts` before the tests' type check.
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import type * as T from '../dist/index.js';
import type * as P from '../dist/transport/protocol.js';

type Id = T.ElementId | T.CommentId | T.RevisionId | T.HeadingId | T.MatchId | T.StyleId;

/** A public type as JSON: ids are plain strings, bytes are base64, and a menu action is whatever the engine sent. */
export type Wire<V> = V extends Id
  ? string
  : V extends T.MenuAction
    ? object
    : V extends Uint8Array
      ? string
      : V extends readonly (infer E)[]
        ? Wire<E>[]
        : V extends object
          ? { [K in keyof V]: Wire<V[K]> }
          : V;

/** A command's argument. */
export type Arg<K extends T.CommandId> = Exclude<Parameters<T.Commands[K]>[0], undefined>;
/** A value over the selection, where it doesn't differ. */
export type Known<V> = Exclude<V, 'mixed' | null>;
/** What a query resolves with. */
export type Result<F extends (...args: never[]) => unknown> = Awaited<ReturnType<F>>;

/** The engine's error shape. */
export type WireError = Pick<T.EditorError, 'code' | 'message' | 'details'>;

/** A dialog's values, tagged with its kind as they travel. */
export type WireDialogValues = { [K in T.DialogKind]: { kind: K } & T.DialogValues[K] }[T.DialogKind];

/** An open dialog's data, without the session's methods. */
export type WireDialogModel = {
  [K in T.DialogKind]: Pick<T.DialogSession<K>, 'kind' | 'fields'> & { values: { kind: K } & T.DialogValues[K] };
}[T.DialogKind];

/**
 * An event as it travels: `{ type, ...payload }`, a payload that isn't an object in `value`, `error` as the error
 * shape plus `fatal`, and the state, which the SDK delivers as its snapshot rather than as an event.
 */
export type WireEvent =
  | ({ type: 'stateChanged' } & WireState)
  | ({ type: 'error'; fatal: boolean } & WireError)
  | {
      [K in Exclude<keyof T.TwipEvents, 'error'>]: T.TwipEvents[K] extends object
        ? T.TwipEvents[K] extends readonly unknown[]
          ? { type: K; value: T.TwipEvents[K] }
          : { type: K } & T.TwipEvents[K]
        : { type: K; value: T.TwipEvents[K] };
    }[Exclude<keyof T.TwipEvents, 'error'>];

/**
 * The state as it travels. The engine fills `commands` from the whole catalog, so the public type keeps every key;
 * the fixture is a sample of two.
 */
export type WireState = Omit<T.EditorState, 'commands'> & { commands: Partial<T.EditorState['commands']> };

/** Fixtures whose public type isn't the PascalCase of the file name. */
const types: Record<string, string> = {
  'accessible-mark': "NonNullable<T.AccessibleNode['marks']>[number]",
  'accessible-role': "T.AccessibleNode['role']",
  alignment: "Arg<'setAlignment'>",
  'apply-to': "T.DialogValues['pageSetup']['applyTo']",
  'area-kind': "Exclude<T.GestureEvent['target'], T.MenuTarget>['kind']",
  'auto-fit': "Arg<'autoFitColumns'>",
  'auto-or': "Arg<'setFontColor'>",
  body: "Arg<'addComment'>",
  'border-edges': "Arg<'setCellBorders'>['edges']",
  'border-preset': "Arg<'setParagraphBorders'>",
  'border-state': "Known<T.ParagraphFormatting['borders']>",
  'border-style': "NonNullable<Arg<'setCellBorders'>['border']>['style']",
  bytes: 'Uint8Array',
  'case-change': "Arg<'changeCase'>",
  'cell-alignment': "Arg<'setCellAlignment'>",
  'cell-shift': "Arg<'deleteCells'>",
  'change-summary': "T.TransactionRecord['summary']",
  'command-group': "T.CommandInfo['group']",
  'command-result': 'Exclude<Result<T.Commands[T.CommandId]>, void>',
  'command-value': 'T.CommandValues[keyof T.CommandValues]',
  command: 'T.Op',
  'control-lock': "T.ElementDetail['lock']",
  // The C ABI's own shapes, in the transport.
  cursor: 'P.Cursor',
  'host-request': 'P.HostRequest',
  'data-part-info': "Result<T.DataParts['list']>[number]",
  'data-part': "Result<T.DataParts['get']>[number]",
  'default-keyword': "Extract<T.PlaceholderLook['match'], string>",
  'delete-unit': "NonNullable<Arg<'delete'>['unit']>",
  'dialog-model': 'WireDialogModel',
  'dialog-values': 'WireDialogValues',
  direction: "Arg<'delete'>['direction']",
  'element-class': "NonNullable<Arg<'replaceInElements'>['kind']>",
  'element-field': "Arg<'replaceInElements'>['in']",
  'element-filter': "NonNullable<Parameters<T.Elements['list']>[0]>",
  'element-kind': "T.ElementContext['kind']",
  error: 'WireError',
  event: 'WireEvent',
  export: 'T.ExportResult',
  'export-format': "Parameters<T.HeadlessEditor['export']>[0]['format']",
  'export-options': "Parameters<T.HeadlessEditor['export']>[0]",
  'field-kind': "T.FieldInfo['kind']",
  'font-style': "Known<T.DialogValues['font']['style']>",
  'gesture-target': "T.GestureEvent['target']",
  'header-footer-kind': "NonNullable<T.SelectionSummary['headerFooter']>['kind']",
  'header-footer-type': "NonNullable<T.SelectionSummary['headerFooter']>['type']",
  'height-rule': "Extract<T.RowHeight, object>['rule']",
  'hit-result': "Result<T.MountedElements['hitTest']>",
  'horizontal-alignment': "Arg<'setTableAlignment'>",
  'horizontal-side': "Arg<'insertColumns'>['where']",
  identity: "NonNullable<T.Author['identity']>",
  'key-binding': "NonNullable<ReturnType<T.Shortcuts['lookup']>>",
  'level-change': "Arg<'setListLevel'>",
  'link-via': "T.TwipEvents['linkActivated']['via']",
  'list-kind': "NonNullable<Arg<'setList'>>",
  'list-state': "Known<T.ParagraphFormatting['list']>",
  'load-state': "T.EditorState['load']",
  location: "NonNullable<T.Warning['location']>",
  'menu-via': "T.ContextMenuEvent['via']",
  misspelling: "NonNullable<Result<T.Spelling['next']>>",
  mode: "T.EditorState['mode']",
  'move-unit': "Arg<'moveSelection'>['by']",
  'none-or': "Known<T.ParagraphFormatting['shading']>",
  'object-kind': "T.ObjectSelection['kind']",
  order: "Arg<'moveToCell'>",
  orientation: "Arg<'setOrientation'>",
  origin: "T.TransactionRecord['origin']",
  'outline-level': "Known<T.DialogValues['paragraph']['outlineLevel']>",
  'page-number-format': "Arg<'insertPageNumber'>['format']",
  'page-number-place': "NonNullable<Arg<'insertPageNumber'>['where']>",
  'page-rect': "ReturnType<T.View['visiblePages']>[number]",
  'paste-as': "NonNullable<Arg<'paste'>['as']>",
  'placeholder-info': 'T.PlaceholderInfo',
  'placeholder-look': "T.PlaceholderLook['look']",
  'placeholder-match': "T.PlaceholderLook['match']",
  'revision-kind': "T.RevisionInfo['kind']",
  'section-break-kind': "Arg<'insertSectionBreak'>",
  'select-unit': "Arg<'select'>",
  'selection-kind': "T.SelectionSummary['kind']",
  'selection-mode': "T.SelectionSummary['mode']",
  severity: "Arg<'markElements'>['marks'][number]['severity']",
  shortcut: "Parameters<T.Shortcuts['register']>[0]",
  'shortcut-keys': "Parameters<T.Shortcuts['register']>[0]['keys']",
  'special-indent': "Known<T.DialogValues['paragraph']['special']>",
  state: 'WireState',
  story: "T.SelectionSummary['story']",
  'style-type': "T.StyleInfo['type']",
  'tab-stop-info': "T.RulerModel['horizontal']['tabs'][number]",
  'tabs-action': "T.DialogValues['tabs']['action']",
  timestamp: "T.TransactionRecord['time']",
  'tooltip-kind': "T.Tooltip['kind']",
  'undo-grouping': "T.CommandInfo['undo']",
  'vertical-direction': "Arg<'moveRows'>",
  'vertical-merge': "Extract<T.Block, { kind: 'table' }>['rows'][number]['cells'][number]['verticalMerge']",
  'vertical-side': "Arg<'insertRows'>['where']",
  'warning-code': "T.Warning['code']",
  'zoom-fit': "Exclude<Arg<'setZoom'>, number>",
  zoom: "Arg<'setZoom'>",
};

/** Fixtures with no public type: the C ABI's own shapes, and input the SDK forwards. */
const skip: Record<string, string> = {
  balloons: 'not a type: the cases review.stackBalloons must give (anchors, heights, gap, tops)',
  'clipboard-data': 'the C ABI copy and paste payload; the SDK owns the system clipboard',
  commit: "the C ABI's command result; the SDK resolves the promise and emits documentChanged",
  'composition-phase': 'IME input the SDK forwards',
  'frame-info': 'the C ABI frame the SDK draws',
  key: 'keyboard input the SDK forwards',
  'key-outcome': "the C ABI's answer to a forwarded key",
  'menu-action': 'opaque in the public API: only passed back to menus.invoke',
  modifiers: 'keyboard and pointer input the SDK forwards',
  point: 'pointer input the SDK forwards, in viewport pixels',
  'pointer-button': 'pointer input the SDK forwards',
  'pointer-kind': 'pointer input the SDK forwards',
  'session-id': 'the C ABI session handle',
};

const wire = new URL('../../../crates/twip/tests/wire/', import.meta.url);
const stems = readdirSync(wire)
  .filter((f) => f.endsWith('.json'))
  .map((f) => f.slice(0, -'.json'.length))
  .sort();
const stale = [...Object.keys(types), ...Object.keys(skip)].filter((s) => !stems.includes(s));
if (stale.length) throw new Error(`no fixture for ${stale.join(', ')}`);

const pascal = (stem: string) => 'T.' + stem.replace(/(^|-)(\w)/g, (_, _dash, c: string) => c.toUpperCase());
const lines = [
  '// Generated by wire-types.ts from the engine wire fixtures: do not edit.',
  "import type * as T from '../dist/index.js';",
  "import type * as P from '../dist/transport/protocol.js';",
  "import type { Arg, Known, Result, Wire, WireDialogModel, WireDialogValues, WireError, WireEvent, WireState } from './wire-types.ts';",
];
let checked = 0;
for (const stem of stems) {
  if (stem in skip) continue;
  const type = types[stem] ?? pascal(stem);
  const examples = JSON.parse(readFileSync(new URL(`${stem}.json`, wire), 'utf8')) as unknown[];
  lines.push('', `// ${stem}.json`);
  for (const example of examples) lines.push(`(${JSON.stringify(example, null, 2)}) satisfies Wire<${type}>;`);
  checked++;
}
writeFileSync(new URL('wire.generated.ts', import.meta.url), lines.join('\n') + '\n');
console.log(`wire.generated.ts: ${checked} fixtures checked, ${Object.keys(skip).length} skipped`);
