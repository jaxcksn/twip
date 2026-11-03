---
title: Shapes
sidebar_position: 10
---

# Shapes

Shapes are DrawingML objects: the 187 preset geometries of ECMA-376 and custom geometry, with fills, outlines, theme
styles, shadows, glow, soft edges and blur, and text inside. VML shapes in older documents are read into the same
model. Shapes are commands and state like everything else: insert one, read the selection, change it.

```ts file=../../examples/web/shapes.ts
```

```csharp file=../../examples/dotnet/Shapes.cs#shapes
```

## Inserting

`insertShape({ preset, width?, height? })` puts a floating shape at the caret's paragraph, in front of the text,
72 × 72 pt by default (a line or connector 72 × 0), in Word's default style: the theme's accent 1 through its style
references. The new shape is selected. `shapeGallery` lists the presets of Word's Insert > Shapes gallery in its eight
groups (Lines, Rectangles, Basic Shapes, Block Arrows, Equation Shapes, Flowchart, Stars and Banners, Callouts), for
an application's menu.

## The selection

`state.selection.object` describes the selected object: its kind (`shape`, `line`, `group`, `picture`, `textBox`),
size, wrapping, rotation and flips, the selected group child's path, and how many objects are selected. Its `shape`
gives the preset and adjust values, and the fill, outline and effects **as written**: `{ explicit: … }` for the
shape's own, `{ fromStyle: … }` for its style's. A theme colour stays a theme colour with its transforms (accent 1 at
75% luminance), so a colour picker can show the theme swatch it came from. `state.themeColours` gives the
document's 12 theme colours, in `ThemeColour` order.

## Formatting

| Command | Changes |
|---|---|
| `setShapePreset(preset)` | the geometry; text, fill, outline and effects stay, adjust values reset |
| `setShapeAdjust({ index, value })` | one adjust value |
| `setShapeFill(fill)` | `none`, `solid`, `gradient`, `pattern`, `picture`, or `fromStyle` to remove the explicit fill |
| `setShapeLine(change)` | width, fill, dash, compound, cap, join, arrowheads; what isn't given is kept |
| `setShapeEffects(change)` | outer and inner shadow, glow, soft edge, blur; `null` removes one |
| `setShapeText(change)` | insets, anchor, autofit, wrapping, direction of the text inside |
| `setPictureAdjust(change)` | a picture's transparency, greyscale, brightness, contrast |
| `setObjectRotation(degrees)`, `flipObject(axis)` | rotation and flips |
| `arrangeObject(how)` | forward, backward, to front, to back, in front of or behind the text |
| `groupObjects()`, `ungroupObjects()` | groups two or more selected floats, or ungroups the selected group |

Each is one undo step; `selectGroupChild('next')` moves the selection through a group's children without one. With
several objects selected (Shift or Ctrl and a click), a command applies to each of them.

## In the view

A press selects a shape on its ink: inside a filled shape or on its outline, wherever its rotation puts it. A second
press on a selected group selects the child under the pointer, and Esc goes back to the group, then to the text. The
selected shape shows a rotate handle above it (Shift snaps to 15°) and yellow adjust handles, and typing into a shape
gives it text. Dragging a handle lays the document out live.

## Saving

Untouched drawings are saved byte for byte. An edited shape has only the properties that changed written again, in
Word's form; 3D, reflection and other effects the engine doesn't draw are kept. New shapes are written as Word writes
them.
