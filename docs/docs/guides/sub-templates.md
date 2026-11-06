---
title: Sub-templates
sidebar_position: 6
---

# Sub-templates

A sub-template is a body of content kept once and referenced from a template: a notice, a fee row, a signature
block. Its body is a glossary building block (`w:docPart` in the glossary part), and each place that uses it is a
field naming it, such as `{ SUBTEMPLATE Violation }`. Twip reads, shows, edits and writes the building blocks; merging
them with data stays your application's.

```ts file=../../examples/web/sub-templates.ts
```

## Declaring references

`setSubTemplateRules` says which fields are references: a field type, which argument holds the name (the first by
default), and the building block category (and, optionally, gallery) the name is looked up in. With no rules, nothing
here changes how a document behaves. Element info reports a reference's `name` and `resolution`: `resolved`,
`missing`, `nearMatch` (only a name in another case matches), `cycle`, `tooDeep` (more than 16 levels), `notAlone`
(the field shares its paragraph) or `notInBody` (it is in a header, footer, note or text box). A declared field shows
its stored result and is never read as a bookmark reference.

## The inline view

A resolved reference alone in its paragraph shows its sub-template's body in its place, laid out at the paragraph's
width; one alone in a table row whose body is one table row shows as that row. Bodies nest up to 16 levels. The view
is the editor's only: saving, the PDF and copying see the field. Typing into a shown body is refused with
`editRefused`. `setSubTemplateView` shows references' fields instead, all or by id.

## Editing a sub-template

A double-click on a shown body, `editor.subTemplates.edit(name)` or the `editSubTemplate` command switches the
editor to that sub-template: its body on its own pages, sized as the template's section. Every command, query and
event then concerns the body; page setup, headers and footers and new comments are unavailable. `state.editing` says
where the editor is and `editingTargetChanged` fires once per switch, including when undo or redo goes into or out of a
sub-template. Twip draws nothing for the switch: show it in your own UI, with a way back (`subTemplates.close()`).
An export (DOCX or PDF) writes the whole document, template and every sub-template, whether one is open or not: there
is no need to close it first.

## Authoring

`subTemplates.list()` gives each sub-template with its reference counts and the fields and controls in its body.
`create` makes one, empty, from a DOCX's body, or (`copyOf`) as a copy of another in its category with its formatting,
fields, controls, pictures and references; `rename` renames one and, with `updateReferences`, every reference to it;
`remove` takes one out, leaving its references `missing`. Each is one undo step. `subTemplates.export(name)` gives one
body as a standalone DOCX (the document's styles, numbering, theme and fonts, without its headers, footers or other
sub-templates) that `create(name, { from })` reads back unchanged. The `replaceBody({ from })` command replaces the
template's own body with a DOCX's, on its last section's page, keeping its sub-templates, custom XML data parts,
settings, styles, headers and footers, in one undo step. Styles in a body resolve as a
merge does: a style the template defines by name is the template's; one only the glossary defines is the glossary's.
