---
title: Accessibility
sidebar_position: 6
---

# Accessibility

The mounted web editor draws the document on a canvas, which assistive technology can't read. So it also puts the
document in the page in two forms screen readers can use, and says what changes as the user works. This guide says
what the editor does, what your application does, and how each WCAG 2.1 A and AA success criterion is covered.

## Three words

- The **reading structure** is the document as data: `editor.accessibility.tree()` returns it, and the
  `AccessibleNode` types describe it. Every node has an `id` that stays the same while the thing it names exists.
- The **mirror** is that structure as ARIA elements in the editor's container, out of sight but not hidden from
  assistive technology. Screen reader users read and navigate it in browse mode: by heading, list, table, link,
  graphic and region.
- The **editing element** is the element that has the keyboard focus while the editor does. It holds the text around
  the caret (the caret's paragraph and its neighbours, in their laid-out lines) and the selection, so the screen
  reader reads characters, words, lines and paragraphs as the caret moves, and echoes typing and deleting.

## What a screen reader user gets

- Tab into the editor: the screen reader says its name (the mount's `label`, default "Document"), that it is an
  editable multi-line text, and how to leave it. **F6** (Shift+F6 for the stop before), as Word's F6 goes to the next
  pane, or **Escape, then Tab** (or Shift+Tab), moves the focus out; Tab alone stays the editor's (a tab, an indent, the
  next cell). Screen readers keep the first Escape for themselves (NVDA's turns focus mode off), so with one, F6 is the
  way out, or Escape twice, then Tab.
- In browse mode, the mirror has headings with levels, lists with their numbers and positions, tables with column
  headers, spans and nesting, links, pictures named by their alternative text (decorative ones are left out), text
  boxes after the paragraph they are anchored in, and, after the body, regions for headers and footers, footnotes,
  endnotes and comments. Commented text points at its comment, insertions and deletions are marked with their author,
  misspelled words are marked, and text in another language says so. Enter on a node puts the caret there.
- While editing, the editor announces, in a polite live region: entering, leaving and moving through tables; entering
  and leaving lists; another page; entering a header, footer, footnote, endnote or text box; entering read-only
  content and an edit refused there; insertions and deletions with their author; comments with their author and text;
  a misspelled word; formatting turned on or off; select all; and what undo, redo, cut, copy and paste did.
- **Shift+F1** says the formatting at the caret: paragraph style, font and size, emphasis, colour and highlight.

## The API

```ts file=../../examples/web/accessibility.ts
```

- `MountOptions.label`: the editor's accessible name.
- `accessibility.tree()`: the reading structure; `accessibility.go(id)`: the caret to a node's start (a note
  reference goes to its note, a comment to its commented text).
- `accessibility.setAnnouncements(kinds)`: the kinds the editor announces (`table`, `list`, `page`, `review`,
  `comment`, `spelling`, `formatting`, `mode`, `readOnly`, `command`, `selection`); the others are neither spoken
  nor emitted. Default: all.
- The `announcement` event: `{ text, kind }` for each announcement. When one change says several things, they are one
  announcement and `kind` is the first one's.
- `commands.describeFormatting()`: what Shift+F1 does.

The .NET SDK has the same tree, announcements and methods (`Accessibility.GetTreeAsync`, `GoAsync`,
`SetAnnouncementsAsync`, `Commands.DescribeFormattingAsync`, the `AccessibilityAnnouncement` event).

## What the editor adds to its container

```text
container
└── div[data-twip="editor"]
    ├── canvas[data-twip="canvas"]       aria-hidden="true"
    └── div[data-twip="content"] > div[data-twip="layer"]
        ├── div[data-twip="input"]       the editing element: contenteditable, role="textbox", aria-multiline,
        │                                aria-label, aria-description (the way out), lang, aria-readonly
        ├── div[data-twip="a11y"]        the mirror: role="document" with the structure inside
        └── div[data-twip="live"]        role="status", aria-live="polite"
```

Mirror elements carry `data-twip-id` (the node's id). The editor creates and removes all of it; don't change it.

## What your application does

- Make your own chrome accessible: toolbars, menus, dialogs, comment cards, the navigation pane. The editor's
  services give you their models; drawing them, and their keyboard and screen reader behaviour, is yours.
- Give the editor a visible label near it, and pass the same words as `label`.
- Don't hide the editor's container from assistive technology (`aria-hidden`, `inert`), and don't trap the focus
  around it.
- Give the page a title and a language, and a way to skip to the editor.

## WCAG 2.1 A and AA

Status: **Twip** (the editor meets it), **later** (Twip's, not met yet), **application** (your page and UI),
**not applicable** (the editor has no such content). Twip's are checked by engine tests (the reading structure,
caret window and announcements), browser tests (the DOM above) and a suite outside this repository that drives NVDA
and checks axe-core's rules.

| Criterion | Level | Status | How |
|---|---|---|---|
| 1.1.1 Non-text Content | A | Twip | Pictures and shapes are named by their alternative text; decorative ones and the canvas are hidden. The text is the document author's. |
| 1.2.1 Audio-only and Video-only (Prerecorded) | A | not applicable | No time-based media. |
| 1.2.2 Captions (Prerecorded) | A | not applicable | No time-based media. |
| 1.2.3 Audio Description or Media Alternative (Prerecorded) | A | not applicable | No time-based media. |
| 1.2.4 Captions (Live) | AA | not applicable | No time-based media. |
| 1.2.5 Audio Description (Prerecorded) | AA | not applicable | No time-based media. |
| 1.3.1 Info and Relationships | A | Twip | Headings, lists, tables with header cells, links, notes, comments and regions in the mirror. |
| 1.3.2 Meaningful Sequence | A | Twip | The body in order, floating content at its anchor, the regions after the body. |
| 1.3.3 Sensory Characteristics | A | application | The editor's own instructions name keys, not shapes or places. |
| 1.3.4 Orientation | AA | application | The editor works in either orientation. |
| 1.3.5 Identify Input Purpose | AA | application | Document text has no input purpose; your forms do. |
| 1.4.1 Use of Color | A | Twip | Insertions, deletions, comments and misspellings are marked and announced, not only coloured. |
| 1.4.2 Audio Control | A | not applicable | No audio. |
| 1.4.3 Contrast (Minimum) | AA | later | The colours the editor draws (selection, marks, caret) aren't checked yet; forced-colours mode is followed. The document's colours are its author's. |
| 1.4.4 Resize Text | AA | later | Zoom enlarges the document; the editor doesn't follow the browser's text size setting yet. |
| 1.4.5 Images of Text | AA | later | The canvas draws text as pixels; the editing element and the mirror carry it as text, but it can't be restyled. |
| 1.4.10 Reflow | AA | later | Pages keep their width; zoom to page width is available. |
| 1.4.11 Non-text Contrast | AA | later | The caret, selection and handles aren't checked yet. |
| 1.4.12 Text Spacing | AA | later | Canvas text doesn't take user style sheets. |
| 1.4.13 Content on Hover or Focus | AA | application | The editor emits `tooltip` events; your application draws the tooltip. |
| 2.1.1 Keyboard | A | Twip | Every editing operation has a key or a command; your chrome is yours. |
| 2.1.2 No Keyboard Trap | A | Twip | F6, or Escape then Tab, leaves the editor; the editing element's description says so. |
| 2.1.4 Character Key Shortcuts | A | Twip | The editor's shortcuts use a modifier or a function key; shortcuts you register are yours. |
| 2.2.1 Timing Adjustable | A | not applicable | The editor has no time limits. |
| 2.2.2 Pause, Stop, Hide | A | Twip | The caret's blink stops under reduced motion, and `caretBlinkStopAfter` stops it after a while. |
| 2.3.1 Three Flashes or Below Threshold | A | Twip | The caret blinks at under two flashes a second. |
| 2.4.1 Bypass Blocks | A | application | A skip link to the editor is your page's. |
| 2.4.2 Page Titled | A | application | Your page's title. |
| 2.4.3 Focus Order | A | Twip | The editor is one tab stop; your page's order is yours. |
| 2.4.4 Link Purpose (In Context) | A | Twip | Links are links with their text; the text is the author's. |
| 2.4.5 Multiple Ways | AA | application | Your application's navigation. |
| 2.4.6 Headings and Labels | AA | Twip | The editor's label (`label`) and the document's headings. |
| 2.4.7 Focus Visible | AA | later | The caret shows where the focus is; the editor draws no focus outline around itself yet. |
| 2.5.1 Pointer Gestures | A | later | Dragging objects and borders has commands, but not every drag has a key yet. |
| 2.5.2 Pointer Cancellation | A | later | Not checked yet. |
| 2.5.3 Label in Name | A | application | Keep `label` the same as the visible label. |
| 2.5.4 Motion Actuation | A | not applicable | No motion input. |
| 3.1.1 Language of Page | A | Twip | The mirror has the document's language; the page's is yours. |
| 3.1.2 Language of Parts | AA | Twip | Text in another language is marked with it, in the mirror and the editing element. |
| 3.2.1 On Focus | A | Twip | Focusing the editor changes nothing. |
| 3.2.2 On Input | A | Twip | Typing changes only the document. |
| 3.2.3 Consistent Navigation | AA | application | Your application's navigation. |
| 3.2.4 Consistent Identification | AA | application | Your application's controls. |
| 3.3.1 Error Identification | A | Twip | Misspellings are marked and announced; an edit refused in read-only content says why. |
| 3.3.2 Labels or Instructions | A | Twip | The editor's label and the way out; your forms are yours. |
| 3.3.3 Error Suggestion | AA | Twip | The spelling service gives suggestions; your application shows them. |
| 3.3.4 Error Prevention (Legal, Financial, Data) | AA | application | Undo is always there; confirming a submission is yours. |
| 4.1.1 Parsing | A | Twip | The editor builds its DOM through DOM calls. |
| 4.1.2 Name, Role, Value | A | Twip | The editing element is a named multi-line text box with its text and selection; mirror elements have their roles and names. |
| 4.1.3 Status Messages | AA | Twip | Announcements go to a polite live region and never move the focus. |
