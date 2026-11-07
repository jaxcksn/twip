# Changelog

## Unreleased

### Features

* **web:** `configure({ assets })` sets where the engine's files (`dist/`) are served from, for applications whose bundler doesn't follow `new URL(..., import.meta.url)`; the folder may be on another origin with CORS (README: "Serving the engine's files yourself")
* **editing:** `setApplicationFieldTypes({ types })` declares the application's own field types: their fields keep their results and show their design labels instead of "Error! Bookmark not defined."
* **editing:** picture content controls take a size: `insertControl({ kind: 'picture', options: { width, height } })` (points) sizes the placeholder, `setControl` resizes it (or the filled picture), and `elements.get` reports it
* **editing:** `subTemplates.create(name, { copyOf })` copies a sub-template's body; `subTemplates.export(name)` gives one body as a standalone DOCX that `create(name, { from })` reads back unchanged
* **editing:** `replaceBody({ from })` replaces the template's body with a DOCX's, keeping its sub-templates, custom XML data parts and settings, in one undo step
* **editing:** `insertControl({ kind: 'picture', placeholderImage: { data, mime } })` sets a picture control's placeholder picture (PNG, JPEG, GIF, BMP or SVG), shown in the editor, the saved DOCX and the PDF; `setControl` replaces it while the control shows it
* **docx:** SVG pictures (`asvg:svgBlip`) are read (the SVG over its PNG fallback), drawn on the canvas and in PDFs with the picture's box as the viewport, and written with a PNG fallback as Word does
* **editing:** picture controls report their size (`elements.list`, the selection's context); a corner handle keeps a picture control's aspect; sizing and placeholder pictures work for floating pictures in controls too, which the mouse moves as it does plain floats

### ⚠ BREAKING CHANGES

* **editing:** `setPlaceholderLooks` and `PlaceholderLook` are removed (web, .NET, wire); a placeholder draws as its own picture. Set it with `placeholderImage`

### Bug Fixes

* **docx:** content controls in blocks written afresh (a new sub-template, header, footer or note) are no longer dropped on save

## [0.1.0](https://github.com/jaxcksn/twip/compare/v0.1.0-alpha.1...v0.1.0) (2026-11-07)


### ⚠ BREAKING CHANGES

* **editing:** picture controls move, resize and float; activation is its own gesture ([#35](https://github.com/jaxcksn/twip/issues/35))
* **editing:** application-controlled fonts ([#34](https://github.com/jaxcksn/twip/issues/34))
* **editing:** application placeholder pictures, SVG pictures and picture control sizing ([#32](https://github.com/jaxcksn/twip/issues/32))

### Features

* Add public API surface ([#8](https://github.com/jaxcksn/twip/issues/8)) ([24dcfa9](https://github.com/jaxcksn/twip/commit/24dcfa95cc9e86cd5ced73c47dddb6d08034763a))
* **drawing:** DrawingML and VML shapes ([#20](https://github.com/jaxcksn/twip/issues/20)) ([8e34765](https://github.com/jaxcksn/twip/commit/8e3476536387b530a9978a526b4d8b06354f0aef))
* **editing:** application placeholder pictures, SVG pictures and picture control sizing ([#32](https://github.com/jaxcksn/twip/issues/32)) ([8a13ac8](https://github.com/jaxcksn/twip/commit/8a13ac8fdacb158f6ef9ca9ac21aedc3a2bdc052))
* **editing:** application-controlled fonts ([#34](https://github.com/jaxcksn/twip/issues/34)) ([895b381](https://github.com/jaxcksn/twip/commit/895b381931e6b15d71c0364e556a3facb866ed9c))
* **editing:** float editing like Word, with position and wrap APIs ([#18](https://github.com/jaxcksn/twip/issues/18)) ([5a4f96e](https://github.com/jaxcksn/twip/commit/5a4f96e665dbc967e2744d497bbe319935d494b4))
* **editing:** picture control size, sub-template copy and export, replaceBody ([#31](https://github.com/jaxcksn/twip/issues/31)) ([84c6597](https://github.com/jaxcksn/twip/commit/84c6597da628939219e05e1154ddc0fb7c3c2177))
* **editing:** picture controls move, resize and float; activation is its own gesture ([#35](https://github.com/jaxcksn/twip/issues/35)) ([f0e2801](https://github.com/jaxcksn/twip/commit/f0e28019eb852d2432213e43e72d7504d6995074))
* **editing:** placeholder affordances ([#25](https://github.com/jaxcksn/twip/issues/25)) ([6f0bf0b](https://github.com/jaxcksn/twip/commit/6f0bf0b86ed541f7011017f4e051195081a1b054))
* **editing:** pointer shapes, section navigation, symbol fallback and editor fixes ([e558d8c](https://github.com/jaxcksn/twip/commit/e558d8c1da020fe651d1939054b99d50ede040aa))
* **editing:** setApplicationFieldTypes for the application's own field types ([#29](https://github.com/jaxcksn/twip/issues/29)) ([1644e0a](https://github.com/jaxcksn/twip/commit/1644e0a789d8a2986b7f5f062f061426ff446440))
* **editing:** sub-templates: inline view, editing in place, and create, rename and remove ([#22](https://github.com/jaxcksn/twip/issues/22)) ([4d6177a](https://github.com/jaxcksn/twip/commit/4d6177a1ea3a3058751584abcf9816cdffb93c78))
* **editing:** track changes and comments ([#6](https://github.com/jaxcksn/twip/issues/6)) ([303bdac](https://github.com/jaxcksn/twip/commit/303bdacc7684f6c641fe873d9d8239940454d56d))
* **layout:** lay out documents as Word does ([#5](https://github.com/jaxcksn/twip/issues/5)) ([9d9f513](https://github.com/jaxcksn/twip/commit/9d9f513292e1d440b00e7c8ec6946ccfa7aba768))
* **layout:** variable fonts' named instances ([#11](https://github.com/jaxcksn/twip/issues/11)) ([621659a](https://github.com/jaxcksn/twip/commit/621659adb3a9bb01baa24adfd1cf471c0dc81b66))
* **pdf:** headless PDF conversion, open passwords, keywords and Alpine builds ([#26](https://github.com/jaxcksn/twip/issues/26)) ([8cb8181](https://github.com/jaxcksn/twip/commit/8cb81814dfb343947feedfebf47fa1d8f5ad245e))
* pre render overhaul work ([#9](https://github.com/jaxcksn/twip/issues/9)) ([f2ce2be](https://github.com/jaxcksn/twip/commit/f2ce2be147fa93023ca016f0d36d24fb6cbbd6b9))
* **render:** dark mode, chrome themes and modern canvas affordances ([#21](https://github.com/jaxcksn/twip/issues/21)) ([092e560](https://github.com/jaxcksn/twip/commit/092e56049575833fefac97c6935ce786a358098f))
* **render:** draw with Metal on macOS through native Dawn ([#6](https://github.com/jaxcksn/twip/issues/6)) ([24119bc](https://github.com/jaxcksn/twip/commit/24119bc6472ac77d8ab3098723afdc3178757a40))
* **render:** retained pages and engine-drawn overlays ([#10](https://github.com/jaxcksn/twip/issues/10)) ([8466cad](https://github.com/jaxcksn/twip/commit/8466cad6a42180a660d04588ceebac394d3f219c))
* the twip engine, its .NET and web SDKs, the rulebook and the docs site ([5c90784](https://github.com/jaxcksn/twip/commit/5c9078448d619eb454d50a8db84dbec0ca55287b))
* **web:** configure({ assets }) for where the engine's files are served from ([#28](https://github.com/jaxcksn/twip/issues/28)) ([7dfb59b](https://github.com/jaxcksn/twip/commit/7dfb59b4308032268773cc31fc40bca2e67e82e4))
* **web:** screen reader support for the web editor ([#23](https://github.com/jaxcksn/twip/issues/23)) ([abcb2d5](https://github.com/jaxcksn/twip/commit/abcb2d523d81dc6f922a488e7f6500ee3bfaebdf))


### Bug Fixes

* **editing:** a protected floating picture control deletes with its box ([#37](https://github.com/jaxcksn/twip/issues/37)) ([64f5795](https://github.com/jaxcksn/twip/commit/64f57953bed0b308af4eb64499a219163cee9ff1))
* **editing:** delete section breaks ([#16](https://github.com/jaxcksn/twip/issues/16)) ([3e48fc9](https://github.com/jaxcksn/twip/commit/3e48fc9570c1de3bcfe56e32b28075bb132df2e7))
* **editing:** follow links to bookmarks, like a table of contents' entries ([#13](https://github.com/jaxcksn/twip/issues/13)) ([432a482](https://github.com/jaxcksn/twip/commit/432a482094689a711916f6dc25a045c929ebc2b2))
* **editing:** input blocks, fields in section rows, section bars and host tooltips ([#27](https://github.com/jaxcksn/twip/issues/27)) ([05bfdb9](https://github.com/jaxcksn/twip/commit/05bfdb9db4545d021a4efa60d3f91ed609913655))
* **editing:** keep header and footer floats out of body clicks ([#3](https://github.com/jaxcksn/twip/issues/3)) ([3dd45a8](https://github.com/jaxcksn/twip/commit/3dd45a8baeeb33ebfa27e96c59b8bb2c155b3fb3))
* **editing:** placeholderActivated names the placeholder's control ([#33](https://github.com/jaxcksn/twip/issues/33)) ([dd1d090](https://github.com/jaxcksn/twip/commit/dd1d090e7b304b5c612500d8dccee6cb8e54e254))
* **editing:** show the caret on the next page after a page break ([#11](https://github.com/jaxcksn/twip/issues/11)) ([882a6b6](https://github.com/jaxcksn/twip/commit/882a6b66aa22d7558ce400eacf2f30bc74e38502))
* **editing:** Tab changes list levels in cells ([#14](https://github.com/jaxcksn/twip/issues/14)) ([5bb77f5](https://github.com/jaxcksn/twip/commit/5bb77f5d8c720c8e91b06e701fa1533413b698ab))
* **layout:** layout fidelity pass ([#19](https://github.com/jaxcksn/twip/issues/19)) ([4e9c983](https://github.com/jaxcksn/twip/commit/4e9c983121937ada6bc6a960750ce201461bafc7))
* **layout:** more layout fidelity ([#15](https://github.com/jaxcksn/twip/issues/15)) ([8ca1b23](https://github.com/jaxcksn/twip/commit/8ca1b2338863c7191d3314ea5ae45fc57cb199de))
* **layout:** showcase document fixes ([#4](https://github.com/jaxcksn/twip/issues/4)) ([4131d3b](https://github.com/jaxcksn/twip/commit/4131d3b283a660c636d3cba17309f3ee865b45fd))
* **layout:** Word fidelity rounds 2 and 3 ([ee3c09f](https://github.com/jaxcksn/twip/commit/ee3c09f573c9fb44a788c486069eae5f8e2f6633))
* margin-relative frames, rotate and adjust feedback, turned selection handles ([#36](https://github.com/jaxcksn/twip/issues/36)) ([599b742](https://github.com/jaxcksn/twip/commit/599b7425b748e0281815ab8c2cb0bb96ee300eaf))
* **release:** run pnpm from the checkout when publishing to npm ([f16fa12](https://github.com/jaxcksn/twip/commit/f16fa12e2aa342e04e892c1eba7de62b9d7d4f2b))
* **web:** give the package its repository for npm provenance ([28c03cb](https://github.com/jaxcksn/twip/commit/28c03cbe38b2f517e85b0eff18bcc42d25052111))


### Performance Improvements

* faster border drags, release and typing ([#17](https://github.com/jaxcksn/twip/issues/17)) ([2ad9422](https://github.com/jaxcksn/twip/commit/2ad94226705035958558a988a313256dc8acd662))


### Reverts

* **editing:** placeholder affordances ([#25](https://github.com/jaxcksn/twip/issues/25), [#27](https://github.com/jaxcksn/twip/issues/27)), keeping the fixes ([#30](https://github.com/jaxcksn/twip/issues/30)) ([d060d80](https://github.com/jaxcksn/twip/commit/d060d80d979be9347e19f32b7f8b4f60ef72eddc))

## 0.1.0-alpha.1 (2026-10-27)


### Features

* the twip engine, its .NET and web SDKs and the docs site ([244f232](https://github.com/jaxcksn/twip/commit/244f2325747d9fbc2275269d2030ff0af8d080b4))
