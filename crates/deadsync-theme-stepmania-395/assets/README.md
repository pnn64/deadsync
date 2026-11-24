The title, logo, company graphics, fonts, and sounds come from the supplied StepMania 3.95 `Themes/default` folder.

Original graphics used here are `BGAnimations/ScreenLogo background/1.png`, `Graphics/ScreenLogo dance (dither).png`, `Graphics/ScreenLogo pump.png`, and the company background's `stepmania (dither).png` and `shadow (32bpp).png`. Stage graphics come from `BGAnimations/ScreenStage overlay`.

Fonts are stored in `assets/fonts/stepmania-395`. Audio is stored in `assets/media/stepmania-395`. The Rust modules define the presentation and animation directly.

The song selection assets come from the default theme's `MusicWheel`, `MusicWheelItem`, `ScreenSelectMusic`, and `DifficultyMeter` graphics, the shared background, the `_shared2` font, and the wheel change sound.

Options use the original `ScreenOptions` page, cursor, and underline artwork, with the `_Arial 14 shadow` bitmap font. The radar artwork comes from `GrooveRadar`. Row, value, difficulty, and options sounds and the selection menu loop are also copied from the supplied theme.

The bundled bitmap fonts also include the original `Common default` imports for button symbols, Japanese, Korean, and miscellaneous glyphs.

Select this installed native theme through System Options or `[Options] Theme=StepMania 3.95` in `deadsync.ini`. The reference theme folder is used during development to inspect and copy assets. The game uses the bundled assets and Rust presentation modules.
