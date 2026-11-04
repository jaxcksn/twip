---
title: Drawing shapes
sidebar_position: 22
---

A DrawingML object (`wps:wsp`, `pic:pic`, `wpg:wgp`) and a VML shape (`v:shape`, `v:rect`, `v:oval` and the other `v:` elements) are drawn from their geometry and their paint. The [FLOAT](float) rules place them and lay out their text; these rules say what they look like. Work in this order: evaluate the geometry on the shape's size, resolve its colours, draw its effects behind it, fill and stroke its paths, then draw its text over it. Pictures are shapes whose fill is their image. VML shapes are read into the same properties first.

## Geometry

### DRAW-1. Evaluating a geometry {#draw-1}

A shape's outline comes from its geometry: `a:prstGeom/@prst` names one of the standard's preset definitions, `a:custGeom` carries its own. Both have the same parts: adjust values (`a:avLst`), guides (`a:gdLst`), adjust handles (`a:ahLst`), connection sites (`a:cxnLst`), a text rectangle (`a:rect`) and paths (`a:pathLst`).

- Evaluate on the shape's size in EMU, after group scaling ([FLOAT-15](float#float-15)). A preset's adjust values are its defaults, replaced by name by the `a:gd` of the shape's `a:avLst` (`fmla="val n"`).
- Guides are evaluated in order; a guide may use adjust values, earlier guides, constants and the built-in names: `w`, `h`, `l` = `t` = 0, `r` = w, `b` = h, `hc` = w/2, `vc` = h/2, `ss` = min(w, h), `ls` = max(w, h), `wdN` = w/N, `hdN` = h/N, `ssdN` = ss/N, and the angles `cd2` = 10800000, `cd4` = 5400000, `cd8` = 2700000, `3cd4`, `3cd8`, `5cd8`, `7cd8`.
- The formulas: `*/ x y z` = x·y/z, `+- x y z` = x + y − z, `+/ x y z` = (x + y)/z, `?: x y z` = y if x > 0 else z, `abs x`, `at2 x y` = the angle of the point (x, y), `cat2 x y z` = x·cos(at2(y, z)), `cos x y` = x·cos(y), `max x y`, `min x y`, `mod x y z` = √(x² + y² + z²), `pin x y z` = y clamped to [x, z] (x when y < x, z when y > z), `sat2 x y z` = x·sin(at2(y, z)), `sin x y` = x·sin(y), `sqrt x`, `tan x y` = x·tan(y), `val x` = x. Angles are 60000ths of a degree. Compute in floating point, without rounding guides.
- A division by zero gives 0, and so does any result that is not a finite number.
- A path with `w` and `h` has its own coordinate space: scale its points by shape width / `w` and shape height / `h`. Without them, points are in the shape's EMU.
- The text rectangle is `a:rect` (`l`, `t`, `r`, `b`), evaluated like a point; without one, the whole shape.
- A preset name with no definition is drawn as `rect`.

### DRAW-2. Arcs {#draw-2}

`a:arcTo wR hR stAng swAng` continues the path from the current point along an ellipse of radii `wR` and `hR`. The current point lies on the ellipse at the start angle. `stAng` and `swAng` are angles as seen from the ellipse's centre, taken in the path's own coordinates (before a path's `w` and `h` scale it): turn each into the ellipse's parameter `t = atan2(wR·sin θ, hR·cos θ)`. The end is at the parameter of `stAng + swAng`; the sweep keeps the sign of `swAng` and its whole turns. Draw it in steps of at most 90°.

### DRAW-3. Filling and stroking paths {#draw-3}

Draw a geometry's paths in order, each filled and then stroked. A path with `fill="none"` is not filled; one with `stroke="0"` is not stroked. A path's fill mode changes the shape's fill colour for that path: `darken` multiplies each sRGB channel by 0.6, `darkenLess` by 0.8; `lighten` moves each channel 40 % of the way to 255, `lightenLess` 20 %. Paths fill with the nonzero winding rule. The shape's rotation (`a:xfrm/@rot`, clockwise) turns the drawing about the centre of its box; `flipH` and `flipV` mirror it there first.

### DRAW-4. Group transforms {#draw-4}

A group's children are placed as [FLOAT-15](float#float-15) says. A group's own `a:xfrm/@rot` turns each child's centre about the group's centre and adds to the child's rotation; `flipH` and `flipV` mirror each child's centre through the group's centre first and toggle the child's own flips. Under an odd number of mirrors the child's own rotation turns the other way.

### DRAW-5. Wedge callouts with the tail inside {#draw-5}

A wedge callout whose tail tip lies inside its body is drawn as the body alone: `wedgeRectCallout` as `rect` and `wedgeRoundRectCallout` as `roundRect` with its `adj3` corner when |adj1| < 50000 and |adj2| < 50000; `wedgeEllipseCallout` as `ellipse` when (adj1/50000)² + (adj2/50000)² < 1.

## Colours

### DRAW-6. Colour models and transforms {#draw-6}

A colour element is one of `a:srgbClr` (sRGB hex), `a:scrgbClr` (linear RGB, 100000ths), `a:hslClr` (hue in 60000ths of a degree, saturation and luminance in 100000ths), `a:sysClr` (its `lastClr`), `a:prstClr` (a named colour: the CSS colours, with `dk`, `lt` and `med` standing for dark, light and medium) and `a:schemeClr` ([DRAW-7](#draw-7)). Its children transform it in document order:

- `alpha` sets the opacity, `alphaOff` adds to it, `alphaMod` multiplies it.
- `hue`, `sat` and `lum` set, `hueOff`, `satOff` and `lumOff` add to, and `hueMod`, `satMod` and `lumMod` multiply the colour's HSL hue, saturation or luminance; `comp` turns the hue by 180°. Saturation and luminance are not clamped to 100 % before the colour is turned back into RGB; the RGB channels are.
- `tint` moves the colour toward white (each channel `1 − (1 − c)·tint`) and `shade` toward black (`c·shade`), both in linear RGB (the sRGB transfer function undone); `inv` inverts each channel in linear RGB; `red`, `green` and `blue` set, `…Off` add to and `…Mod` multiply a channel in linear RGB too.
- `gray` replaces the colour by its luminance 0.2126 R + 0.7152 G + 0.0722 B of the sRGB channels; `gamma` and `invGamma` apply and undo the sRGB transfer function.

Clamp each channel and the opacity to their range after every transform. `a:sysClr` uses its `lastClr`.

### DRAW-7. Theme colours {#draw-7}

`a:schemeClr` names a colour of the theme's `a:clrScheme`: `dk1`, `lt1`, `dk2`, `lt2`, `accent1`–`accent6`, `hlink`, `folHlink` directly; `bg1`, `tx1`, `bg2`, `tx2` (and the accents and links) through the settings' `w:clrSchemeMapping`, whose `w:bg1`, `w:t1`, `w:bg2`, `w:t2`, `w:accent1`… attributes name `light1`, `dark1`, `light2`, `dark2`, `accent1`… Without a mapping, `bg1` is `lt1`, `tx1` is `dk1`, `bg2` is `lt2` and `tx2` is `dk2`. Without a theme part, the scheme is dk1 000000, lt1 FFFFFF, dk2 0E2841, lt2 E8E8E8, accents 156082, E97132, 196B24, 0F9ED5, A02B93, 4EA72E, hlink 467886, folHlink 96607D.

### DRAW-27. Colours on a dark page {#draw-27}

On a dark page the document keeps its layout and its file; only the colours it's drawn in change, and only on screen (exported and printed output keep the document's colours). Each colour is resolved as on a light page first (automatic text white on a dark highlight or shading, a revision's colour, a theme colour), then drawn in its dark-page colour:

- **Text, highlights, paragraph and cell shading, borders, underlines and the page itself** (white, or the page colour of [PAGE-12](page#page-12)) take the colour the dark-page table gives: a table of 17 levels per sRGB channel (level *i* is round(*i* · 255 / 16)), read between its points by tetrahedral interpolation (the cube cell is split along its diagonal into the tetrahedron the colour's channel fractions order). The table is [`dark-page.txt`](https://github.com/jaxcksn/twip/blob/main/crates/twip/src/render/dark-page.txt): line 17 · *r* + *g* lists *b* = 0 … 16. White becomes `262626`, black `F6F6F6`; mid greys about `909090` stay as they are, and a colour whose relative luminance is about 0.26 keeps its colour whatever its hue. Highlights go through the table in the colours [FONT-15](font#font-15) draws them in (yellow `FFFF00` becomes `515100`).
- **Pure blues** (`0000xx`) step at `00008E`: from `000080` to `00008D` they run from `D0D0FF` to `CBCBFF`, from `00008E` to `000090` from `49D1FF` to `48D1FF` (dark blue highlight `00008B` becomes `CCCCFF`).
- **Shapes and pictures keep their colours**: a shape's fill, line, effects and gradients, and a picture's pixels. A shape with a fill keeps its text's colours and the shading in its text too; a shape without a fill (a text box over the page) has its text drawn as the page's.

A page shown in its own colours while the editor around it is dark draws every colour as it is.

## Paint

### DRAW-8. Style references {#draw-8}

A shape's fill, line and effects are its own `a:spPr` children when it has them. Otherwise `wps:style` decides: `a:fillRef idx` picks the theme format scheme's fill (`a:fillStyleLst`, 1-based; 1001 and up pick `a:bgFillStyleLst` from 1001), `a:lnRef idx` its line (`a:lnStyleLst`), `a:effectRef idx` its effects (`a:effectStyleLst`); `idx` 0 means none. In the entry, `phClr` is the reference's own colour. A shape's own `a:ln` replaces the referenced line property by property (width, fill, dash, compound, cap, join, alignment, ends), and a `phClr` in it is the reference's colour too. Without either, the shape has no fill, no line and no effects.

### DRAW-9. Line defaults {#draw-9}

A line with no fill, from the shape or its style reference, is not drawn. A line without `w` is 0.75 pt wide. Unless given: a solid dash, a single compound line, flat caps and round joins, the pen centred on the path, no ends.

### DRAW-10. Gradients {#draw-10}

`a:gradFill` blends its stops (`a:gs pos`, 100000ths) across the shape's box:

- **Linear** (`a:lin ang`, clockwise from the x axis): the stops run along the direction, from the box's first point on it to its last (the box projected onto the direction). With `scaled="1"` the direction is (h·cos ang, w·sin ang) normalized, so 45° runs corner to corner.
- **Circle** (`a:path path="circle"`): from the box's centre (stop 0) to its corners, half the box's diagonal; `a:fillToRect` doesn't move it.
- **Rect** (`path="rect"`): contours that are the box scaled from the focus rectangle `a:fillToRect` (insets of the box, 100000ths) out to the whole box.
- **Shape** (`path="shape"`): contours that are the shape's outline scaled about the box's centre; a shape whose outline is its box is filled as rect.

Two stops blend in linear light (the sRGB transfer function undone) along this curve, the share of the second stop at sixteenths of the way: 0, 0.018, 0.0472, 0.0843, 0.1404, 0.209, 0.2927, 0.3873, 0.4934, 0.5922, 0.6913, 0.778, 0.8485, 0.9054, 0.949, 0.9779, 1. Three or more stops blend linearly in sRGB between neighbours. Opacity blends linearly. Before the first stop and after the last, the end colours continue. With `rotWithShape="0"` the gradient doesn't turn with the shape. A linear gradient's `a:tileRect` (insets, negative outward) is the region it spans; past it the gradient repeats, mirrored when the gradient has `flip`.

### DRAW-11. Patterns {#draw-11}

`a:pattFill prst` fills with an 8 × 8 tile of 0.75 pt cells, each in `a:fgClr` or `a:bgClr`, on a grid that starts at the page's top left. The tiles, a row per byte from the top, the most significant bit at the left, set bits in the foreground colour: pct5 80 00 00 00 08 00 00 00; pct10 80 00 08 00 80 00 08 00; pct20 88 00 22 00 88 00 22 00; pct25 88 22 88 22 88 22 88 22; pct30 AA 44 AA 11 AA 44 AA 11; pct40 AA 55 AA 51 AA 55 AA 15; pct50 AA 55 AA 55 AA 55 AA 55; pct60 EE 55 BB 55 EE 55 BB 55; pct70 77 DD 77 DD 77 DD 77 DD; pct75 77 FF DD FF 77 FF DD FF; pct80 EF FF FE FF EF FF FE FF; pct90 FF FF FF F7 FF FF FF 7F; horz FF 00 00 00 00 00 00 00; vert 80 80 80 80 80 80 80 80; ltHorz FF 00 00 00 FF 00 00 00; ltVert 88 ×8; dkHorz FF FF 00 00 FF FF 00 00; dkVert CC ×8; narHorz FF 00 FF 00 FF 00 FF 00; narVert 55 ×8; dashHorz F0 00 00 00 0F 00 00 00; dashVert 80 80 80 80 08 08 08 08; cross FF 80 80 80 80 80 80 80; dnDiag 80 40 20 10 08 04 02 01; upDiag 01 02 04 08 10 20 40 80; ltDnDiag 88 44 22 11 88 44 22 11; ltUpDiag 11 22 44 88 11 22 44 88; dkDnDiag CC 66 33 99 CC 66 33 99; dkUpDiag 33 66 CC 99 33 66 CC 99; wdDnDiag C1 E0 70 38 1C 0E 07 83; wdUpDiag 83 07 0E 1C 38 70 E0 C1; dashDnDiag 00 00 88 44 22 11 00 00; dashUpDiag 00 00 11 22 44 88 00 00; diagCross 81 42 24 18 18 24 42 81; smCheck 99 66 66 99 99 66 66 99; lgCheck F0 F0 F0 F0 0F 0F 0F 0F; smGrid FF 88 88 88 FF 88 88 88; lgGrid FF 80 80 80 80 80 80 80; dotGrid AA 00 80 00 80 00 80 00; smConfetti 80 08 40 02 10 01 20 04; lgConfetti B1 30 03 1B D8 C0 0C 8D; horzBrick FF 80 80 80 FF 08 08 08; diagBrick 01 02 04 08 18 24 42 81; solidDmnd 10 38 7C FE 7C 38 10 00; openDmnd 82 44 28 10 28 44 82 01; dotDmnd 80 00 22 00 08 00 22 00; plaid AA 55 AA 55 F0 F0 F0 F0; sphere 77 89 8F 8F 77 98 F8 F8; weave 88 54 22 45 88 14 22 51; divot 00 10 08 10 00 80 01 80; shingle 03 84 48 30 0C 02 01 01; wave 00 18 25 C0 00 18 25 C0; trellis FF 66 FF 99 FF 66 FF 99; zigZag 81 42 24 18 81 42 24 18.

### DRAW-12. Picture fills {#draw-12}

`a:blipFill` uses the part of the image `a:srcRect` keeps (insets of the image, 100000ths; negative insets add empty margins). With `a:stretch` it fills the box inset by `a:fillRect` (100000ths of the box, negative outward). With `a:tile` it repeats at the image's size at 96 dpi scaled by `sx` and `sy`, one tile placed by `algn` in the box (`tl` at the top left, `ctr` centred, `br` at the bottom right, and so on) and moved by `tx` and `ty`; the tile's `flip` is not drawn. The picture is clipped to the geometry's path. An image that can't be decoded draws a light grey frame.

### DRAW-13. Group fill {#draw-13}

`a:grpFill` is the fill of the nearest enclosing group that has one (its `a:grpSpPr` fill); without one, nothing.

### DRAW-14. Lines {#draw-14}

A line is centred on the path, `w` wide (`algn="in"`: inside the path, from the outline in). Caps: `rnd` round, `sq` square, `flat` (the default) none. Joins: as `a:round`, `a:bevel` or `a:miter lim` (100000ths of the width) say, with these presets joining with miters (limit 8) when the line asks for round joins or none: bentUpArrow, chevron, cube, downArrow, ellipseRibbon, ellipseRibbon2, leftArrowCallout, leftRightArrowCallout, leftUpArrow, mathEqual, mathMinus, mathMultiply, mathNotEqual, mathPlus, moon, notchedRightArrow, plus, quadArrowCallout, rightArrow, star4, star5, star8, star10, star12, star16, star24, star32, stripedRightArrow, upDownArrow, upDownArrowCallout. A rectangle's outline starts at its bottom left corner and runs along its bottom first. Dashes, in line widths (dash, space, …): dot 1 3, dash 4 3, lgDash 8 3, dashDot 4 3 1 3, lgDashDot 8 3 1 3, lgDashDotDot 8 3 1 3 1 3, sysDash 3 1, sysDot 1 1, sysDashDot 3 1 1 1, sysDashDotDot 3 1 1 1 1 1; `a:custDash` gives them in 100000ths of the width. With round or square caps each dash is one width shorter and each space one width longer (the caps add the width back). A line filled with a gradient, pattern or picture paints the region it covers with that fill.

### DRAW-15. Compound lines {#draw-15}

`cmpd` divides the line's width into bands, from the outside of the shape in: `dbl` 1 : 1 : 1 (line, gap, line), `thickThin` 3 : 1 : 1, `thinThick` 1 : 1 : 3, `tri` 1 : 1 : 2 : 1 : 1. An open path has no outside: its bands are centred.

### DRAW-16. Line ends {#draw-16}

`a:headEnd` goes on the first point of an open path and `a:tailEnd` on its last; a closed path has none. Their size is in line widths, a line thinner than 2 pt counting as 2 pt: length (`len`) and width (`w`) `sm` 2, `med` 3, `lg` 5. Along the line's direction at the endpoint:

- `triangle`: the tip on the endpoint, the base at the length; the line stops half a (real) width short of the base.
- `stealth`: the same triangle notched on the line at 1, 2 or 3 widths from the tip (`len` sm, med, lg), where the line stops.
- `diamond` and `oval`: centred on the endpoint, as long and wide as their size; the line runs to the endpoint.
- `arrow`: an open V of the line's width, from the endpoint to the length and width; the line stops a width short.

The ends are filled with the line's fill.

### DRAW-17. Effects and their order {#draw-17}

A shape's effects come from its `a:effectLst`, else from its style's `a:effectRef` (the theme's `effectStyleLst` entry, its `phClr` the reference's colour). They draw in this order: the outer (or preset) shadow, the glow, the fill and line, the inner shadow; soft edges then fade what the shape drew (glow included, not its shadow), and blur blurs all of it. A blur radius is two sigmas of a Gaussian.

### DRAW-18. Outer and preset shadows {#draw-18}

`a:outerShdw` is the shape's silhouette (its filled paths if it has a fill, its outline if it has one) in the shadow colour, scaled by `sx`, `sy` and skewed by `kx`, `ky` about its alignment point (`algn`, on the shape's box), moved `dist` towards `dir` (clockwise from the right) and blurred by `blurRad`. With `rotWithShape="1"` it turns with the shape; otherwise its direction and its scale are the page's. A soft-edged shape casts the shadow of its faded silhouette. `a:prstShdw` is drawn as an outer shadow of its `dist`, `dir` and colour without blur.

### DRAW-19. Inner shadow {#draw-19}

`a:innerShdw` darkens the inside of the fill along the edges towards `dir`: the outside of the shape, moved `dist` away from `dir` and blurred by `blurRad`, drawn inside the fill only, in the shadow colour. It turns with the shape.

### DRAW-20. Glow {#draw-20}

`a:glow` is the silhouette grown by half of `rad` and blurred over half of it, in the glow colour, under the shape.

### DRAW-21. Soft edges and blur {#draw-21}

`a:softEdge` keeps what the shape drew where its silhouette, eroded by 0.85 `rad` and blurred with a sigma of 0.32 `rad`, covers it: the edge is at half opacity 0.85 `rad` inside and clear at the outline. `a:blur` blurs everything the shape drew by `rad`.

### DRAW-22. Picture cropping and clipping {#draw-22}

`a:srcRect` takes the part of the image inside its insets (fractions of the image; a negative inset adds an empty margin) and stretches it to the box (or `a:fillRect`). The picture is clipped to its geometry's path. A picture's outline (`pic:pic`, not a shape's picture fill) lies outside it: the path stroked twice as wide under the picture.

### DRAW-23. Picture adjustments {#draw-23}

In document order: `a:alphaModFix` multiplies the picture's opacity by `amt`; `a:grayscl` replaces each colour by its Rec. 709 luminance (0.2126 R + 0.7152 G + 0.0722 B); `a:lum` scales each channel about one half by `1 / (1 − contrast)` (positive contrast) or `1 + contrast`, then adds `bright`; `a:biLevel` makes a pixel white where its luminance is at least `thresh`, else black; `a:duotone` maps luminance 0 to 1 onto its first and second colours.

### DRAW-24. Effects not drawn {#draw-24}

3D (`a:scene3d`, `a:sp3d`), reflection, fill overlays, effect DAGs and artistic effects (`a14:imgEffect`) are kept on save and not drawn. They don't change layout: wrapping keeps using `wp:effectExtent` (FLOAT-5).

### DRAW-25. VML shapes {#draw-25}

A `w:pict` or `w:object` without a DrawingML branch is read into the same shape model. Its first shape element (a `v:shapetype` only defines a type) is placed by its CSS `style`: `position:absolute` makes it a float at `margin-left` and `margin-top` from `mso-position-horizontal-relative` and `mso-position-vertical-relative` (`text` is the column and the paragraph, `char` the character, `page`, `margin` and the margin areas as named), aligned by `mso-position-horizontal` and `mso-position-vertical` unless they are `absolute`; `width`, `height`, `rotation` (degrees, clockwise) and `flip` (`x`, `y`) as written; a negative `z-index` puts it behind the text; `w10:wrap/@type` its wrapping (none without one). Without `position:absolute` it is an inline object. Its geometry:

- `v:rect`, `v:oval`; `v:roundrect` with a corner radius of `arcsize` (20% by default) of the shorter side;
- `v:line`, `v:polyline`, `v:curve` from their points, `v:arc` from `startangle` to `endangle` (clockwise from 12 o'clock) on the ellipse in its box;
- `v:shape`: its own `path` or its type's, with the type's `v:formulas` and `adj` values (the shape's overriding them one by one), in its `coordsize` space from its `coordorigin`; the cube (`o:spt` 16) and bevel (84) types are drawn as their presets, the adjust value scaled from 21600ths;
- `v:group`: its children in its `coordorigin` and `coordsize` space at their `left`, `top`, `width` and `height`;
- `v:image`, and a `v:shape` with `v:imagedata`: a picture, cropped by `cropleft` to `cropbottom`.

`v:textbox` gives the shape its story, with `inset` (7.2, 3.6, 7.2, 3.6 pt by default) and `v-text-anchor`.

### DRAW-26. VML fill, stroke and shadow {#draw-26}

A VML shape is filled unless `filled="f"`, in `fillcolor` (white by default) or `v:fill/@color`, with `opacity`. `v:fill/@type`: `gradient` runs from `color` to `color2` along `angle` (screen degrees) with `focus` 100%, the other way with `focus` 0 and both ways at ±50%, or by `colors` stops; `gradientRadial` has `color2` at its centre; `tile` and `frame` show its picture tiled or stretched; `pattern` tiles its black and white bitmap in `fillcolor` and `color2` at a point per pixel. A shape is stroked unless `stroked="f"`, in `strokecolor` (black) at `strokeweight` (0.75 pt), with `v:stroke`'s `dashstyle` (named, or dash and space lengths in line widths), `linestyle`, `startarrow` and `endarrow` (`block` triangle, `classic` stealth, `diamond`, `oval`, `open` arrow, sized `narrow`/`medium`/`wide` and `short`/`medium`/`long`), `joinstyle` and `endcap`. `v:shadow` with `on="t"` is an outer shadow at `offset` (2 pt, 2 pt), in `color` (grey) with `opacity`, not blurred. Formulas, paths and features without a DrawingML counterpart (`v:textpath`, `o:extrusion`) are kept on save and not drawn. Saving an edited VML shape patches its `style`, `fillcolor`, `strokecolor` and `strokeweight`; the rest is copied.
