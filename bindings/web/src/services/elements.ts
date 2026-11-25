import type { Op } from '../commands.js';
import type { Session } from '../editor.js';
import type { SelectionSummary } from '../state.js';
import { Method } from '../transport/protocol.js';
import type { ClientPoint, ClientRect, ElementId, Points } from '../types.js';

/** A content control's kind. */
export type ControlKind =
  | 'richText'
  | 'plainText'
  | 'picture'
  | 'checkbox'
  | 'date'
  | 'dropDown'
  | 'comboBox'
  | 'group'
  | 'repeatingSection'
  | 'buildingBlockGallery'
  | 'buildingBlock'
  | 'citation'
  | 'bibliography'
  | 'equation';

/** A date control's calendar. */
export type CalendarType =
  | 'gregorian'
  | 'hijri'
  | 'hebrew'
  | 'taiwan'
  | 'japan'
  | 'thai'
  | 'korea'
  | 'saka'
  | 'gregorianXlitEnglish'
  | 'gregorianXlitFrench'
  | 'gregorianUs'
  | 'gregorianMeFrench'
  | 'gregorianArabic';

/** What a bound date control stores in its data: the text it shows, the date, or the date and time. */
export type DateStorage = 'text' | 'date' | 'dateTime';

/** The merge designer's elements: fields and content controls. Changing them is done with the commands. */
export interface Elements {
  /** The elements, in document order. */
  list(filter?: {
    kind?: 'field' | ControlKind;
    match?: { in: 'code' | 'tag' | 'title'; text: string };
    story?: SelectionSummary['story'];
    /** Only the elements in this sub-template's body. */
    subTemplate?: string;
  }): Promise<ElementInfo[]>;
  /** One element in detail. */
  get(id: ElementId): Promise<ElementDetail>;
  /**
   * The body (or an element) as blocks. Rejects with `invalid-argument` for a root that is not an element id and
   * `unavailable` for one the document doesn't have.
   */
  structure(options?: { root?: ElementId; depth?: number }): Promise<Block[]>;
  /** Selects an element; enters a header or footer when needed. */
  select(id: ElementId, options?: { reveal?: boolean }): Promise<void>;
  /**
   * An element's boxes on screen, one per line it spans, for anchoring your own chrome to it: none while it is out of
   * view. Rejects with `unavailable` for an element the document doesn't have.
   */
  boxes(id: ElementId): Promise<ClientRect[]>;
  /** Insert Field's fields, of one category or all, in its order. */
  fieldCatalogue(category?: FieldCategory): Promise<FieldDescription[]>;
  /** The formats and pictures Insert Field offers. */
  fieldFormats(): Promise<FieldFormats>;
  /** The field code Insert Field's choices make, and its problems (none when it takes it as it is). Cheap: call it per keystroke. */
  composeField(choice: FieldChoice): Promise<{ code: string; problems: FieldProblem[] }>;
  /** The choices a field code is made of, as Insert Field opens on it. */
  parseField(code: string): Promise<FieldChoice>;
  /** The field the selection is in, with its code's choices: Insert Field opened on it changes it (`setField`). */
  fieldHere(): Promise<{ id: ElementId; choice: FieldChoice } | null>;
  /** The display formats a date control offers for a locale (BCP 47). */
  dateFormats(locale: string): Promise<string[]>;
}

/** Insert Field's categories. */
export type FieldCategory =
  | 'dateAndTime'
  | 'documentAutomation'
  | 'documentInformation'
  | 'equationsAndFormulas'
  | 'indexAndTables'
  | 'linksAndReferences'
  | 'mailMerge'
  | 'numbering'
  | 'userInformation';

/** A field as Insert Field describes it. */
export interface FieldDescription {
  /** Its name as a code writes it (`PAGE`, `=`). */
  name: string;
  /** The categories it is listed under. */
  categories: FieldCategory[];
  /** What it does. */
  description: string;
  /** Its syntax, as Insert Field shows it under Field codes. */
  syntax: string;
  /** Its arguments, in order. */
  args: {
    name: string;
    kind: 'text' | 'bookmark' | 'style' | 'property' | 'variable' | 'file' | 'name' | 'expression' | 'number';
    required: boolean;
  }[];
  /** Its own switches: the flag after the backslash, what it takes, and what it does. */
  switches: { flag: string; arg: 'none' | 'text' | 'number' | 'bookmark' | 'style' | 'name' | 'levels'; description: string }[];
  /** The general switches it takes: `\*` text and number formats, `\#` a numeric picture, `\@` a date picture. */
  formats: { text: boolean; numeric: boolean; date: boolean };
  /** The engine computes its result (the others keep theirs, or the application's `setFieldResults` gives it). */
  evaluated: boolean;
}

/** The formats Insert Field offers: `\*` text and number formats (value, then name), and pictures. */
export interface FieldFormats {
  /** `\*` text formats: as the code writes them, and their names. */
  textFormats: { value: string; label: string }[];
  /** `\*` number formats: as the code writes them, and their names. */
  numberFormats: { value: string; label: string }[];
  /** `\#` numeric pictures. */
  numericPictures: string[];
  /** `\@` date-time pictures. */
  datePictures: string[];
}

/** What a field code is made of: Insert Field's choices. */
export interface FieldChoice {
  /** The field's name. */
  name: string;
  /** Its arguments, in order. */
  args?: string[];
  /** Its own switches, with what follows each. */
  switches?: { flag: string; value?: string }[];
  /** `\*` formats other than MERGEFORMAT. */
  formats?: string[];
  /** `\#`: a numeric picture. */
  numericPicture?: string;
  /** `\@`: a date-time picture. */
  datePicture?: string;
  /** `\* MERGEFORMAT`: the result keeps its formatting through updates. */
  preserveFormatting?: boolean;
}

/** What is wrong with a field code, as Insert Field flags it. */
export type FieldProblem =
  | { kind: 'empty' }
  | { kind: 'unknownField'; name: string }
  | { kind: 'missingArgument'; name: string }
  | { kind: 'unknownSwitch'; flag: string }
  | { kind: 'missingSwitch'; flag: string }
  | { kind: 'missingSwitchValue'; flag: string }
  | { kind: 'notANumber'; flag: string }
  | { kind: 'formatNotTaken'; flag: string };

/** {@link Elements} on a mounted editor: adds drop targets and hit testing. */
export interface MountedElements extends Elements {
  /** What is under a point, without moving the selection. */
  hitTest(at: ClientPoint): Promise<{ element: ElementId | null; editable: boolean }>;
  /** An external drag (a field list beside the editor): shows the drop caret. */
  dragOver(at: ClientPoint): void;
  /** The external drag left the editor. */
  dragLeave(): void;
  /** Drops at the point: runs `op` there, as one transaction. */
  drop(at: ClientPoint, op: Op): Promise<void>;
}

/** An element. */
export interface ElementInfo {
  /** The element. */
  id: ElementId;
  /** Its kind. */
  kind: 'field' | ControlKind;
  /** A field's code. */
  code?: string;
  /** A control's tag. */
  tag?: string;
  /** A control's title. */
  title?: string;
  /** The result or content text, up to 200 characters. */
  display: string;
  /** The story it is in. */
  story: SelectionSummary['story'];
  /** The sub-template whose body it is in, when `story` is `'subTemplate'`. */
  subTemplate?: string;
  /** The element it is in. */
  parent: ElementId | null;
  /** Locked against editing. */
  locked: boolean;
  /** A field shows its code rather than its result. */
  showingCode: boolean;
  /** A field's result is out of date: the next field update refreshes it. */
  dirty: boolean;
  /** A date control's date (`YYYY-MM-DDTHH:MM:SSZ`); absent while it shows its placeholder or no date. */
  date?: string;
  /** A sub-template reference field's sub-template. */
  reference?: SubTemplateReference;
  /** A picture control's picture size (inline or floating). */
  size?: { width: Points; height: Points };
  /** Whether a content control shows its placeholder. */
  showingPlaceholder?: boolean;
}

/**
 * One sub-template reference rule (`setSubTemplateRules`): a field of type `fieldType` names, in argument `argument`,
 * the building block in `category` (and `gallery`) whose body it shows.
 */
export interface ReferenceRule {
  /** The field type (`SUBTEMPLATE`), compared without case. */
  fieldType: string;
  /** Which of the field's arguments is the sub-template's name, from 0. Default: 0. */
  argument?: number;
  /** The building block category the name is looked up in. */
  category: string;
  /** The building block gallery (`custom1`), when it matters. */
  gallery?: string;
}

/** Whether a reference's sub-template resolves, or why its body can't show. */
export type ReferenceResolution = 'resolved' | 'missing' | 'nearMatch' | 'cycle' | 'tooDeep' | 'notAlone' | 'notInBody';

/** What a sub-template reference field names and whether its body shows. */
export interface SubTemplateReference {
  /** The sub-template's name, as the field gives it. */
  name: string;
  /** Whether it names a sub-template whose body can show in its place. */
  resolution: ReferenceResolution;
  /** The sub-template's name when only a name in another case matches (`'nearMatch'`). */
  nearMatch?: string;
  /** Its body shows in its place. */
  expanded: boolean;
}

/** An element in detail. */
export interface ElementDetail extends ElementInfo {
  /** A control's lock. */
  lock: 'none' | 'control' | 'content' | 'both';
  /** A control's placeholder text. */
  placeholder?: string;
  /** A control's data binding. */
  binding?: DataBinding;
  /** A control's options. */
  options?: ControlOptions;
  /** The elements inside it. */
  children: ElementId[];
}

/** A content control's binding to a custom XML part. */
export interface DataBinding {
  /** The XPath of the bound node. */
  xpath: string;
  /** The data part's item id. */
  storeItemId?: string;
  /** The XPath's namespace prefixes. */
  prefixMappings?: string;
}

/** A picture control's placeholder picture: PNG, JPEG, GIF, BMP or SVG (`image/svg+xml`). */
export interface PlaceholderImage {
  /** The picture's bytes. */
  data: Uint8Array;
  /** Its MIME type; the data must be one. */
  mime: string;
}

/**
 * A content control's options, by kind: checkbox, date, list, plain text, or picture. A picture's size (1…1584 pt) is
 * its placeholder's, and the picture's once filled (fitted to it); `setControl` keeps a side not given.
 */
export type ControlOptions =
  | { checked?: boolean; checkedSymbol?: CheckboxSymbol; uncheckedSymbol?: CheckboxSymbol }
  | { sectionTitle?: string; allowInsertDelete?: boolean }
  | { format?: string; locale?: string; calendar?: CalendarType; storeAs?: DateStorage }
  | { gallery?: string; category?: string }
  | { items: { text: string; value: string }[] }
  | { multiline?: boolean }
  | { width?: Points; height?: Points };

/** A check box control's checked or unchecked symbol: one character in a font. */
export interface CheckboxSymbol {
  /** Its font. */
  font: string;
  /** The character. */
  character: string;
}

/** A block of the document structure. */
export type Block =
  | {
      kind: 'paragraph';
      textLength: number;
      elements: ElementId[];
      /**
       * Its hidden paragraph marks: where each paragraph joined into it by one ends, and its own mark when that is hidden
       * (it runs into the next paragraph). `at` is in characters; absent when there are none.
       */
      hiddenMarks?: { at: number; styleSeparator: boolean }[];
    }
  | {
      kind: 'table';
      rows: {
        cells: { span: number; verticalMerge: 'none' | 'start' | 'continue'; empty: boolean; blocks: Block[] }[];
      }[];
    };

/** The document's custom XML data parts. Writing and removing them is done with commands. */
export interface DataParts {
  /** Every data part. */
  list(): Promise<{ namespace: string | null; itemId: string }[]>;
  /** The data parts with a namespace or an item id. */
  get(options: { namespace?: string; itemId?: string }): Promise<{ namespace: string | null; itemId: string; xml: string }[]>;
}

/** A sub-template: a building block in a category the rules declare. */
export interface SubTemplateInfo {
  /** Its name. */
  name: string;
  /** Its building block category. */
  category: string;
  /** The reference fields naming it in the template's body, headers, footers and notes. */
  references: number;
  /** The reference fields naming it in other sub-templates' bodies. */
  nestedReferences: number;
  /** The fields and controls in its body. */
  elements: ElementInfo[];
}

/**
 * The template's sub-templates, and editing one in this editor. While one is edited, `state.editing` names it and
 * every command, query and event concerns its body; `editingTargetChanged` fires on each switch.
 */
export interface SubTemplates {
  /** The sub-templates the rules declare, in glossary order. */
  list(): Promise<SubTemplateInfo[]>;
  /** Edits a sub-template's body on its own pages (`category`: when several rules name categories). */
  edit(name: string, category?: string): Promise<void>;
  /** Back to the template, at its selection from before. */
  close(): Promise<void>;
  /**
   * A new sub-template: empty, with the body of the DOCX `from`, or a copy of the sub-template `copyOf` in the same
   * category (its formatting, fields, controls, pictures and references). One undo step.
   */
  create(name: string, options?: { category?: string; from?: Uint8Array; copyOf?: string }): Promise<void>;
  /**
   * A standalone DOCX of a sub-template's body, with the document's styles, numbering, theme and fonts and without
   * its headers, footers or other sub-templates; `create(name, { from })` reads it back unchanged.
   */
  export(name: string, options?: { category?: string }): Promise<Uint8Array>;
  /** Renames a sub-template; `updateReferences` renames every reference to it too. One undo step. */
  rename(name: string, to: string, options?: { category?: string; updateReferences?: boolean }): Promise<void>;
  /** Removes a sub-template; its references no longer resolve. One undo step. */
  remove(name: string, options?: { category?: string }): Promise<void>;
}

/** The sub-templates over a session. */
export function createSubTemplates(s: Session): SubTemplates {
  return {
    list: () => s.call(Method.SubTemplates),
    edit: async (name, category) => {
      await s.run({ type: 'editSubTemplate', name, ...(category === undefined ? {} : { category }) } as Op);
    },
    close: async () => {
      await s.run({ type: 'closeSubTemplate' } as Op);
    },
    create: async (name, options) => {
      await s.run({ type: 'createSubTemplate', name, ...options } as Op);
    },
    rename: async (name, to, options) => {
      await s.run({ type: 'renameSubTemplate', name, to, ...options } as Op);
    },
    remove: async (name, options) => {
      await s.run({ type: 'removeSubTemplate', name, ...options } as Op);
    },
    export: async (name, options) => {
      const docx = await s.call<string>(Method.SubTemplateExport, { name, ...options });
      return Uint8Array.from(atob(docx), (c) => c.charCodeAt(0));
    },
  };
}

/** The elements over a session. */
export function createElements(s: Session): Elements {
  return {
    list: (filter) => s.call(Method.Elements, filter ?? null),
    get: (id) => s.call(Method.Element, id),
    structure: (options) => s.call(Method.Structure, options ?? null),
    select: (id, options) => s.call(Method.SelectElement, { id, reveal: options?.reveal ?? false }),
    boxes: async (id) => {
      const boxes = await s.call<ClientRect[]>(Method.ElementBoxes, id);
      return s.clientRect ? boxes.map(s.clientRect) : boxes;
    },
    fieldCatalogue: (category) => s.call(Method.FieldCatalogue, category ?? null),
    fieldFormats: () => s.call(Method.FieldFormats),
    composeField: (choice) => s.call(Method.ComposeField, choice),
    parseField: (code) => s.call(Method.ParseField, code),
    fieldHere: () => s.call(Method.FieldHere),
    dateFormats: (locale) => s.call(Method.DateFormats, locale),
  };
}

/** The data parts over a session. */
export function createDataParts(s: Session): DataParts {
  return {
    list: async () => (await s.call<{ namespace: string | null; itemId: string }[]>(Method.DataParts)).map(({ namespace, itemId }) => ({ namespace, itemId })),
    get: (options) => s.call(Method.DataParts, options),
  };
}
