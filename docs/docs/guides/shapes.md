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
| `setShapeFill(fill)` | `none`, `solid`, `gradient`, `pattern`, `picture`, or `fromStyle` to remove the explicit fill; a picture is stretched (`stretch`: insets, fractions of the shape) or tiled (`tile`: offset, scale, alignment, mirroring), and turns with the shape or not (`rotateWithShape`) |
| `setShapeLine(change)` | width, fill, dash, compound, cap, join, arrowheads; what isn't given is kept |
| `setShapeEffects(change)` | outer and inner shadow (with its scale, skew, alignment), glow, soft edge, blur, reflection; `null` removes one |
| `setShapeText(change)` | insets, anchor, autofit, wrapping, direction of the text inside; `upright` keeps the text level when the shape turns |
| `setPictureAdjust(change)` | a picture's transparency, greyscale, brightness, contrast, transparent colour |
| `setPictureRecolor(recolor)` | `none`, `grayscale`, `blackAndWhite` with its threshold (0 to 1), or `duotone` with a dark and a light colour |
| `resetPicture({ size })` | Reset Picture: the picture's adjustments, effects and outline removed, its crop kept; with `size` (Reset Picture & Size) the crop goes too and the picture takes its image's own size |
| `setLockAspectRatio(lock)` | whether resizing keeps the object's proportions |
| `setObjectRotation(degrees)`, `flipObject(axis)` | rotation and flips |
| `arrangeObject(how)` | forward, backward, to front, to back, in front of or behind the text |
| `groupObjects()`, `ungroupObjects()` | groups two or more selected floats, or ungroups the selected group |

Each is one undo step; `selectGroupChild('next')` moves the selection through a group's children without one. With
several objects selected (Shift or Ctrl and a click), a command applies to each of them.

## Names, hiding and stacking

Word's Selection Pane is an application's own panel built on these. `objectList(page)` (.NET `GetObjectListAsync`)
lists the floating objects on a page (from 0) that the story being edited reaches, front first, hidden ones too, each
group's objects inside it: `{ object, name, kind, hidden, children }`. `object` is an `ObjectId`, `{ float, path }`:
the float and, for a group's object, its place in the group. Ids last while the document is open, not across opens.

- `setObjectName(name)` renames the selected object (blank fails `unavailable`; 255 characters are kept), and
  `setObjectTitle(title)` sets its alternative text's title.
- `setObjectHidden({ object, hidden })` hides or shows one; a hidden object isn't drawn, and hiding the selected one puts
  the caret at its anchor.
- `restackObject({ object, inFrontOf })` puts one just in front of another, or behind every other without `inFrontOf`.
- `selectObject({ object, extend })` selects one, or adds it to the selection.

`state.selection.object` reads them back: `id`, `name`, `title`, `hidden`, `lockAspect`, `stackIndex` (0 is the front
of its page), a picture's `originalSize` and `pictureAdjust` (brightness, contrast, transparency and recolour).

## Wrap points

A floating object with tight or through wrapping wraps text around a polygon. `state.selection.object.wrapPolygon`
gives its points, as fractions of the object's box, and whether they were `edited`. `setWrapPoints(points)` sets
3 to 1024 of them (and makes a square or other wrap tight); `resetWrapPoints()` puts back the object's outline.
`editWrapPoints()` lets the user drag them on the canvas.

## Linked text boxes

Text that doesn't fit in a text box can go on in another: `linkTextBox(object)` (Word's Create Link) links the selected
box to an empty one in no chain, and `unlinkTextBox()` (Break Link) ends the chain after the selected box.
`beginTextBoxLink()` starts Word's pitcher pointer instead: the next click on an empty box links it, a click elsewhere
or Esc cancels, and `state.view.linkingTextBox` is true meanwhile. `state.selection.object.textBoxLink` gives the
box's `index` in its chain, the chain's `count`, and the boxes before and after it (`previous`, `next`).

## Selecting objects

`setSelectObjects(true)` is Word's Select Objects: a drag from outside every object draws a rectangle
(`state.view.objectMarquee`, page points) and selects the floating objects wholly inside it, and a click on the text
places no caret. Esc or `setSelectObjects(false)` ends it; `state.view.selectingObjects` says whether it is on.

## Frames

A frame (a paragraph placed like a floating object) selected by its border reads `kind: frame`, with `frame` giving
its placement (`properties`) and `allowOverlap`. `setFrame({ …, allowOverlap })` changes them; with `allowOverlap`
false the frame moves clear of other frames and floating objects.

## Images

`objectImage()` gives the selected picture's image bytes as the document holds them, `shapeFillImage()` the selected
shape's picture fill, `watermarkImage(section)` a section's picture watermark, and `pageFillImage()` the page colour's
picture or texture; each is `null` without one (.NET `byte[]?`). The state carries no image bytes.

## In the view

A press selects a shape on its ink: inside a filled shape or on its outline, wherever its rotation puts it. A second
press on a selected group selects the child under the pointer, and Esc goes back to the group, then to the text. The
selected shape shows a rotate handle above it (Shift snaps to 15°) and yellow adjust handles, and typing into a shape
gives it text. Dragging a handle lays the document out live.

## Watermarks and page colour

`setWatermark` puts a text or picture watermark in every header of every section, written as Word writes it, so Word's
Watermark dialog edits it; `removeWatermark` takes it out. `setPageColor` sets the page colour: a colour, a theme
colour with its tint or shade, or a gradient, pattern, texture or picture fill. They are the Printed Watermark and Fill
Effects dialogs (see [Services](services#dialogs)). A page colour isn't drawn in a PDF export, as in Word's. A picture fill or watermark can be any format the document draws, including JPEG XR and EMF+ metafiles.

## Saving

Untouched drawings are saved byte for byte. An edited shape has only the properties that changed written again, in
Word's form; 3D and other effects the engine doesn't draw are kept. Reflection, glow, soft edges and shadows are drawn. New shapes are written as Word writes
them.
