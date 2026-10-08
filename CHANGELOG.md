# Changelog

## [0.3.2](https://github.com/felixpy/envark/compare/v0.3.1...v0.3.2) (2026-10-08)


### Bug Fixes

* **desktop:** repair macOS bundles and require explicit scan folders ([#29](https://github.com/felixpy/envark/issues/29)) ([6395a90](https://github.com/felixpy/envark/commit/6395a90afc7944a7d0b2e8705e85ac5a7ac83247))

## [0.3.1](https://github.com/felixpy/envark/compare/v0.3.0...v0.3.1) (2026-10-07)


### Bug Fixes

* **desktop:** improve startup defaults and inventory usability ([#27](https://github.com/felixpy/envark/issues/27)) ([98f1124](https://github.com/felixpy/envark/commit/98f11249deedafad4b194603f8a043b2165345d9))

## [0.3.0](https://github.com/felixpy/envark/compare/v0.2.1...v0.3.0) (2026-10-06)


### Features

* **projects:** measure and safely remove linked worktrees ([#25](https://github.com/felixpy/envark/issues/25)) ([74ac1e0](https://github.com/felixpy/envark/commit/74ac1e0a2e7ef4eeb497a6dcf0673e85baabdf74))
* **updater:** download and install signed application updates ([#24](https://github.com/felixpy/envark/issues/24)) ([3a28c13](https://github.com/felixpy/envark/commit/3a28c13a118296d6c3aaa8573778b56536b5bdff))


### Bug Fixes

* **ui:** format Windows verbatim paths for display ([#23](https://github.com/felixpy/envark/issues/23)) ([b1fc232](https://github.com/felixpy/envark/commit/b1fc232da068a5ff9b37e69b8bb56da8f11cd35c))

## [0.2.1](https://github.com/felixpy/envark/compare/v0.2.0...v0.2.1) (2026-10-05)


### Bug Fixes

* **rust:** display compiler versions for installed toolchains ([#19](https://github.com/felixpy/envark/issues/19)) ([e1242b9](https://github.com/felixpy/envark/commit/e1242b9b520167633b786fd6087519e358c19173))

## [0.2.0](https://github.com/felixpy/envark/compare/v0.1.0...v0.2.0) (2026-10-04)


### Features

* improve repository workflows and desktop controls ([ef5b900](https://github.com/felixpy/envark/commit/ef5b90020889cacf4a2defdd962753b2a38e2bac))

## 0.1.0 (2026-10-04)


### Features

* **app:** bootstrap the Envark desktop application ([261cb62](https://github.com/felixpy/envark/commit/261cb62208b6fa2032f07fd6a30ca82a3e7fa973))
* **environments:** review batch tool updates and resource removal ([bfcc782](https://github.com/felixpy/envark/commit/bfcc782b85635252008a448543090d4e20a4167e))
* **providers:** support nvm and SDKMAN shell-managed runtimes ([e1a0012](https://github.com/felixpy/envark/commit/e1a0012a700a7816371f535b7911efc18f2dc742))


### Bug Fixes

* **activity:** preserve operation results when logging fails ([aea8a42](https://github.com/felixpy/envark/commit/aea8a42d6cb40df999e0cd3650aa50b7c899fc82))
* **cleanup:** distinguish logical removal from reclaimed disk space ([09c8a0e](https://github.com/felixpy/envark/commit/09c8a0ee1e02ee4c8aef7afaa4881f86b8b8828b))
* **cleanup:** honor changed scan scope and project protection ([92e8f87](https://github.com/felixpy/envark/commit/92e8f871d6264f21a1a6998ccf9c0efffbdb8a8e))
* **cleanup:** revalidate artifact contents and cache destinations ([09786b1](https://github.com/felixpy/envark/commit/09786b1f8fdc2f529fb6ce161db4c34888fc2126))
* **cleanup:** verify artifact ownership and excluded descendants ([5f09d31](https://github.com/felixpy/envark/commit/5f09d31a70dcc36e00fc37d2a977ed1d0f9faaa9))
* **config:** validate structured files before saving ([c62665a](https://github.com/felixpy/envark/commit/c62665a9755ea907439ad06c0fa8cf175057778b))
* **discovery:** prevent automatic toolchain installation ([96364df](https://github.com/felixpy/envark/commit/96364df9b75b4bb5edaad2ad92d5cba20cd02c9f))
* **ollama:** bind mutations to the reviewed local service ([330c4a2](https://github.com/felixpy/envark/commit/330c4a23cc9dbab43298d2b3bdd53e1ee27662da))
* **process:** terminate descendants when commands are cancelled ([3b30803](https://github.com/felixpy/envark/commit/3b30803d49b9e4b96c49be2f7c08a5351edc16ce))
* **providers:** bind tool actions to their owning installations ([ffa6f45](https://github.com/felixpy/envark/commit/ffa6f45f33c346f86c69920d0c1d60f049ed9644))
* **python:** target exact installations and protect active interpreters ([d9cbe79](https://github.com/felixpy/envark/commit/d9cbe79e92b98e6fcf1a349e11bb5731314a80e2))
* **release:** bootstrap the first release at 0.1.0 ([a09fa21](https://github.com/felixpy/envark/commit/a09fa21cb7abfcfaee67f29ea5a590c36947056a))
* **runtimes:** distinguish inherited state from manager defaults ([16ae751](https://github.com/felixpy/envark/commit/16ae75185626f4f8a2ea37555a1c898601cd767d))
* **settings:** label preference selectors for assistive technology ([3eb32bb](https://github.com/felixpy/envark/commit/3eb32bb909aa987c0630275a51e82f4d2eb260be))
* **storage:** recover damaged state without losing original data ([64e9939](https://github.com/felixpy/envark/commit/64e9939c663a8c795cbfa8840a0c4705dd9639ca))
* **updates:** bound streamed release metadata ([c32319d](https://github.com/felixpy/envark/commit/c32319dabf310fe8c75ebeabc33a0927c715d95b))
* **updates:** reject downgrades using ecosystem version ordering ([01c0878](https://github.com/felixpy/envark/commit/01c087897379c48f2574c2d659a59da281192990))


### Performance Improvements

* **scanner:** reuse unchanged roots with filesystem notifications ([0c507de](https://github.com/felixpy/envark/commit/0c507de90f83fab0d6aca6168db1fa031cc8b139))
* **ui:** lazy-load application views with error recovery ([42abce5](https://github.com/felixpy/envark/commit/42abce50be29271b99fd3dcabb725f2576856e90))
