The title, logo, company graphics, fonts, and sounds come from the supplied StepMania 3.95 `Themes/default` folder.

Original graphics used here are `BGAnimations/ScreenLogo background/1.png`, `Graphics/ScreenLogo dance (dither).png`, `Graphics/ScreenLogo pump.png`, and the company background's `stepmania (dither).png` and `shadow (32bpp).png`. Stage graphics come from `BGAnimations/ScreenStage overlay`.

Fonts are stored in `assets/fonts/stepmania-395`. Audio is stored in `assets/media/stepmania-395`. The Rust modules define the presentation and animation directly.

Startup loading copies `Data/splash.png` and reproduces the original captionless loading dialog with its three live status lines. It consumes the shared library progress view and keeps the existing Start/Back shortcut for skipping loudness analysis.

The song selection assets come from the default theme's `MusicWheel`, `MusicWheelItem`, `ScreenSelectMusic`, and `DifficultyMeter` graphics, the shared background, the `_shared2` font, and the wheel change sound.

Options use the original `ScreenOptions` page, cursor, and underline artwork, with the `_Arial 14 shadow` bitmap font. The radar artwork comes from `GrooveRadar`. Row, value, difficulty, and options sounds and the selection menu loop are also copied from the supplied theme.

The song-selection radar draws the original half-transparent player polygons and two-pixel outlines using the source colors, radius, and rotations. Values come from the shared chart catalog's computed Stream, Voltage, Air, Freeze, and Chaos data, normalized by music length. Chart analysis and persistence remain in the game layer.

The current sort uses the original `MusicSortDisplay` artwork for title, artist, BPM, group, popularity, and top grades. Extra catalog sorts and player filters use the bundled normal font with their actual names. The shared wheel view supplies the active ordering; the theme owns its label and animation.

The bundled bitmap fonts also include the original `Common default` imports for button symbols, Japanese, Korean, and miscellaneous glyphs.

Gameplay uses the original `ScreenGameplay` life/score frames and difficulty sheet, `LifeMeterBar` textures, `Judgment` and `HoldJudgment` sheets, `Combo` artwork, and the ready/go, cleared/failed, and entry/exit animation graphics. Rust code reproduces their authored layout and animation. The failed sound is copied from `BGAnimations/ScreenGameplay failed/failed.ogg`.

Evaluation uses the original stage/summary headers, banner and grade frames, grade and judgment sheets, bonus frames, score label, record badges, and `_moveon.png`. Numeric fonts preserve the original `Numbers/_numbers2`, `_numbers3`, `Combo numbers`, and `ScreenEvaluation percent text` bitmap sheets; their local descriptors map the original 5-by-3 character layout.

Style selection uses the original five `ScreenSelectStyle` icons, illustrations, information panels, header, explanation, and warning. Difficulty/mode selection uses the eight `ScreenSelectDifficulty` cards, explanation, paging arrows, player cursors, shadows, and OK graphics. The shared game controller accepts the four regular difficulties and Nonstop; unsupported Challenge, Endless, and Magic Dance outcomes are visibly disabled. No unsupported game mode is substituted for those labels.

Course selection uses `ScreenSelectCourse` artwork and the original `CourseEntryDisplay` bar, sharing this theme's wheel drawing with song selection. The source has no local-profile catalog browser; the host's existing profile picker is adapted with the original options page/cursors and blank `ScreenSelectPlayer` header.

Select this installed native theme through System Options or `[Options] Theme=StepMania 3.95` in `deadsync.ini`. The reference theme folder is used during development to inspect and copy assets. The game uses the bundled assets and Rust presentation modules.

Idle attract scenes use the original tutorial background, hand/message graphics, default dance note/receptor sheets, demonstration overlay, ranking background, ranking bullets, `_ScreenRanking common` font and imports, and tutorial/ranking music. Tutorial chart beats, stops, prompts, and scene transitions are expressed as native Rust data and actors.

`training-pad.bin` contains the original static `Characters/DancePad-DDR.txt` geometry and material colors, converted by `tools/convert_training_pad.py`. Its three textures are copied from `Characters/MetalPanel.png`, `Arrow-Blue.png`, and `Arrow-Red.png`; the white texture comes from the company background. The conversion tool is for asset authoring only. The supplied default character has no model, so the tutorial does not invent a replacement dancer.

The ranking table displays real, recent per-chart cabinet leaderboards and percentages. The host does not store the original category-wide numeric score histories, so this adaptation uses song titles rather than invented category totals. Unlock-point and warning pages are not part of the current native attract sequence.

The ignored `attract_render` integration test renders the native scenes into a hidden window and writes PNG captures under `target/native-395-attract-captures` for visual inspection. It uses only the bundled native assets.
