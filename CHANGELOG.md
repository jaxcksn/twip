# Changelog

## [0.3.0](https://github.com/jaxcksn/twip/compare/v0.2.1...v0.3.0) (2026-11-25)


### Layout changes

* rendering fidelity ([#35](https://github.com/jaxcksn/twip/pull/35))


### ⚠ BREAKING CHANGES

* **editing:** public API for phase 4 editing ([#62](https://github.com/jaxcksn/twip/issues/62))

### Features

* add full DOCX roundtrip capabilities ([#30](https://github.com/jaxcksn/twip/issues/30)) ([706af62](https://github.com/jaxcksn/twip/commit/706af629ea1f8bdf8dc2613d0b4d76e4f56b0dbb))
* **editing:** find, paste and tables of contents ([#61](https://github.com/jaxcksn/twip/issues/61)) ([7d45348](https://github.com/jaxcksn/twip/commit/7d4534861dc75d7fec7fcaf4b2a2c8db863ba5d6))
* **editing:** Keyboard Shortcuts, Canvas Menus, Hover, Pointers ([#59](https://github.com/jaxcksn/twip/issues/59)) ([4f4e508](https://github.com/jaxcksn/twip/commit/4f4e5088a82ec5fca0bfef5575cd9673e4447792))
* **editing:** public API for phase 4 editing ([#62](https://github.com/jaxcksn/twip/issues/62)) ([db966d6](https://github.com/jaxcksn/twip/commit/db966d6a0a6e7fba7a37f349310ac6e2c4db6855))
* **editing:** selection, styles, objects and review ([#60](https://github.com/jaxcksn/twip/issues/60)) ([496da68](https://github.com/jaxcksn/twip/commit/496da6826f34e598aca715ce7af937887b616bc9))
* rendering fidelity ([#35](https://github.com/jaxcksn/twip/issues/35)) ([a9e3f13](https://github.com/jaxcksn/twip/commit/a9e3f13a0e1fb74091e08dc4cc7256dfa93c8666))


### Bug Fixes

* **dotnet:** copy the native library to smoke on macOS ([#37](https://github.com/jaxcksn/twip/issues/37)) ([7288476](https://github.com/jaxcksn/twip/commit/7288476784b2696e1bce649ad26eb677aac4c65c))
* **editing:** harden the editing subsystem ([#58](https://github.com/jaxcksn/twip/issues/58)) ([b21666c](https://github.com/jaxcksn/twip/commit/b21666cc3abf8eeabe41d1b4486ae9c4bdf7744e))
* **layout:** close additional layout gaps ([#34](https://github.com/jaxcksn/twip/issues/34)) ([b24e8f2](https://github.com/jaxcksn/twip/commit/b24e8f2a097eb6539402e0239965b159c5fb3d0c))
* **layout:** draw a cell's bottom border over columns the prior leaves out ([#31](https://github.com/jaxcksn/twip/issues/31)) ([cc73823](https://github.com/jaxcksn/twip/commit/cc738238e6cdc011cb1997f1192bdb15be2a4431))
* **layout:** merge grid lines no cell ends on ([#33](https://github.com/jaxcksn/twip/issues/33)) ([70afc6e](https://github.com/jaxcksn/twip/commit/70afc6ed2d201b9204f36566bb0c5145d54885bd))
* **render:** drop caches when resources change ([#38](https://github.com/jaxcksn/twip/issues/38)) ([dc8068d](https://github.com/jaxcksn/twip/commit/dc8068d236ccd873fac6ad883ea2b4ee6a272a50))


### Performance Improvements

* improve load times of larger documents ([#40](https://github.com/jaxcksn/twip/issues/40)) ([bbc8b85](https://github.com/jaxcksn/twip/commit/bbc8b85475e4849d856f8e4702ccffbd8f4ebf3e))

## [0.2.1](https://github.com/jaxcksn/twip/compare/v0.2.0...v0.2.1) (2026-11-16)


### Bug Fixes

* **pdf:** keep kerned pairs' text in the PDF ([#28](https://github.com/jaxcksn/twip/issues/28)) ([ba54e1f](https://github.com/jaxcksn/twip/commit/ba54e1ff1d1a74f5fc1e8672bfbba00e4e624c2f))

## [0.2.0](https://github.com/jaxcksn/twip/compare/v0.1.1...v0.2.0) (2026-11-15)


### Layout changes

* **layout:** expand layout stability gate ([#17](https://github.com/jaxcksn/twip/pull/17))
* **layout:** various layout stability and rendering fidelity enhancements ([#19](https://github.com/jaxcksn/twip/pull/19))
* **layout:** vertical merged cells now layout correctly ([#20](https://github.com/jaxcksn/twip/pull/20))
* **editing:** cross-page object drags, shape clicks and layout fidelity ([#21](https://github.com/jaxcksn/twip/pull/21))
* **layout:** merged regions after every row grows ([#22](https://github.com/jaxcksn/twip/pull/22))
* **layout:** close more layout gaps and findings ([#24](https://github.com/jaxcksn/twip/pull/24))
* hardening for 0.2.0 ([#26](https://github.com/jaxcksn/twip/pull/26))


### ⚠ BREAKING CHANGES

* **render:** replace skia with custom rendering engine ([#18](https://github.com/jaxcksn/twip/issues/18))

### Features

* **editing:** control labels, host ruler tooltips, row marks ([#14](https://github.com/jaxcksn/twip/issues/14)) ([0ae52ef](https://github.com/jaxcksn/twip/commit/0ae52ef6f94181e7ef862e2679947e204f72e34b))
* **layout:** close more layout gaps and findings ([#24](https://github.com/jaxcksn/twip/issues/24)) ([3767152](https://github.com/jaxcksn/twip/commit/3767152bfd15603a7cc224199d6fead80f3cab99))
* **layout:** various layout stability and rendering fidelity enhancements ([#19](https://github.com/jaxcksn/twip/issues/19)) ([4370a14](https://github.com/jaxcksn/twip/commit/4370a14fa5fe735262a72852b4c331837c6b209c))
* **render:** replace skia with custom rendering engine ([#18](https://github.com/jaxcksn/twip/issues/18)) ([7faa0fe](https://github.com/jaxcksn/twip/commit/7faa0fe0a3977a5cae331ea5b545952b0e984083))


### Bug Fixes

* **editing:** cross-page object drags, shape clicks and layout fidelity ([#21](https://github.com/jaxcksn/twip/issues/21)) ([7c478e0](https://github.com/jaxcksn/twip/commit/7c478e000277021eb8ccacf71593ac86212f14f8))
* **editing:** new lists drop the old direct indent ([#16](https://github.com/jaxcksn/twip/issues/16)) ([66515eb](https://github.com/jaxcksn/twip/commit/66515eb89bfdeac2d9e5d06310409b1aa9d69580))
* **editing:** select outlined shapes and drag floats across sections ([#23](https://github.com/jaxcksn/twip/issues/23)) ([341a9d5](https://github.com/jaxcksn/twip/commit/341a9d56e463e36a67992e47a37440dc90014844))
* hardening for 0.2.0 ([#26](https://github.com/jaxcksn/twip/issues/26)) ([a884c52](https://github.com/jaxcksn/twip/commit/a884c52ba989e8cf99073fa7a753b904195f89b7))
* **layout:** merged regions after every row grows ([#22](https://github.com/jaxcksn/twip/issues/22)) ([5428606](https://github.com/jaxcksn/twip/commit/5428606bc0c02f14c1c9e949bfe811f1e41f7ee5))
* **layout:** vertical merged cells now layout correctly ([#20](https://github.com/jaxcksn/twip/issues/20)) ([1bf8e35](https://github.com/jaxcksn/twip/commit/1bf8e358816d9eae16966721314f644d6c2667b2))
* remove unused test helpers ([9983ad5](https://github.com/jaxcksn/twip/commit/9983ad58ecd051626baac3bc9c4ea7a970207389))

## [0.1.1](https://github.com/jaxcksn/twip/compare/v0.1.0...v0.1.1) (2026-11-09)


### Features

* **render:** draw with Metal directly on macOS ([#7](https://github.com/jaxcksn/twip/issues/7)) ([10cc9ab](https://github.com/jaxcksn/twip/commit/10cc9ab247c68784d9a305b2900cbfa0158e6f9f))


### Bug Fixes

* bound nesting, sizes and entities ([#11](https://github.com/jaxcksn/twip/issues/11)) ([338abca](https://github.com/jaxcksn/twip/commit/338abcaa776fac902fb74db57ca958b08df3ef24))
* **layout:** resolve small issues with field labels ([#13](https://github.com/jaxcksn/twip/issues/13)) ([10ab00b](https://github.com/jaxcksn/twip/commit/10ab00bd0dd3972ad88c2063dde49ec953dc2870))


### Performance Improvements

* **editing:** memoize page geometry per line ([#10](https://github.com/jaxcksn/twip/issues/10)) ([ac6dae8](https://github.com/jaxcksn/twip/commit/ac6dae85a55869510ae42d7689060712cb6b51dd))
* **render:** one font manager for the process ([#12](https://github.com/jaxcksn/twip/issues/12)) ([41d5e7d](https://github.com/jaxcksn/twip/commit/41d5e7d0dae27ae73c2a4c82f14e443c887aa539))

## [0.1.0](https://github.com/jaxcksn/twip/compare/v0.1.0-alpha.1...v0.1.0) (2026-11-08)


### Features

* the twip engine, its .NET and web SDKs and the docs site ([d9e4a6a](https://github.com/jaxcksn/twip/commit/d9e4a6a5783045118f3606e06c22d4479b3f41d0))


### Bug Fixes

* **editing:** self-closing numbering root ([#2](https://github.com/jaxcksn/twip/issues/2)) ([efc977a](https://github.com/jaxcksn/twip/commit/efc977a8d751385787514ee4b6e436e079343082))

## Changelog
