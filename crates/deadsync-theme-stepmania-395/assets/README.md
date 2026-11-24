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

The full-screen danger background uses the original `ScreenGameplay danger all` tile sheet and centered glow/text graphics. Its 14-frame tile sequence, 20 fps timing, two-second color effects and half-second visibility blink are authored in Rust. The mounted HUD requests an all-player warning scope; the common gameplay composer checks the joined players and warning preferences. This theme's individual-player danger and dead animations are empty in the reference.

Evaluation uses the original stage/summary headers, banner and grade frames, grade and judgment sheets, bonus frames, score label, record badges, and `_moveon.png`. Numeric fonts preserve the original `Numbers/_numbers2`, `_numbers3`, `Combo numbers`, and `ScreenEvaluation percent text` bitmap sheets; their local descriptors map the original 5-by-3 character layout.

Stage evaluation also uses the original possible/actual radar bars, their opposing player anchors, and staggered width animations. The score layer prepares achievement ratios from judged chart notes, the recorded combo runs, and earned points. Nonstop replaces the bonus area and maximum combo with the original survived frames and `ScreenEvaluation stage` digits, using the number of stages each player passed. The course header follows the original redirection to the stage header; the session-summary header is a different screen.

Style selection uses the original five `ScreenSelectStyle` icons, illustrations, information panels, header, explanation, and warning. Difficulty/mode selection uses the eight `ScreenSelectDifficulty` cards, explanation, paging arrows, player cursors, shadows, and OK graphics. The shared game controller accepts the four regular difficulties and Nonstop; unsupported Challenge, Endless, and Magic Dance outcomes are visibly disabled. No unsupported game mode is substituted for those labels.

Course selection uses `ScreenSelectCourse` artwork and the original `CourseEntryDisplay` bar, sharing this theme's wheel drawing with song selection. Its row numbers, titles, subtitles, artists and meters draw above the bars, following the original two/three-line text-banner layout. The shared course view retains both the compact full title and separate song metadata. The source has no local-profile catalog browser; the host's existing profile picker is adapted with the original options page body, three-piece cursors, footer and blank `ScreenSelectPlayer` header.

Select this installed native theme through System Options or `[Options] Theme=StepMania 3.95` in `deadsync.ini`. The reference theme folder is used during development to inspect and copy assets. The game uses the bundled assets and Rust presentation modules.

Screen transitions use the source `_fade in normal`, `_fade out normal`, `_menu in`, `_menu out`, and `_options to options` sequences expressed as Rust tweens. The original `_moveon.png` supplies the menu transition graphic, while `_swoosh.ogg` and `_swoosh and more.ogg` supply menu and evaluation effects. The executable mounts these transition handlers directly. The shared shell forwards optional fixed entry/exit effects and keeps screen opacity separate from overlays.

Idle attract scenes use the original tutorial background, hand/message graphics, default dance note/receptor sheets, demonstration overlay, ranking background, ranking bullets, `_ScreenRanking common` font and imports, and tutorial/ranking music. Tutorial chart beats, stops, prompts, and scene transitions are expressed as native Rust data and actors.

`training-pad.bin` contains the original static `Characters/DancePad-DDR.txt` geometry and material colors, converted by `tools/convert_training_pad.py`. Its three textures are copied from `Characters/MetalPanel.png`, `Arrow-Blue.png`, and `Arrow-Red.png`; the white texture comes from the company background. The conversion tool is for asset authoring only. The supplied default character has no model, so the tutorial does not invent a replacement dancer.

The ranking table displays real, recent per-chart cabinet leaderboards and percentages. The host does not store the original category-wide numeric score histories, so this adaptation uses song titles rather than invented category totals. The original warning artwork follows rankings for ten seconds before returning to Company. The unlock-points page is omitted because the host does not store that cabinet unlock system.

The ignored `attract_render` integration test renders the native scenes into a hidden window and writes PNG captures under `target/native-395-attract-captures` for visual inspection. It uses only the bundled native assets.

Session summary reuses the evaluation panels with the original summary header and half-size banner stack. The score layer selects the three most recent stages where a player passed, sums the judgments, scores and per-stage best combos, and averages the five radar axes. The source's extra-stage category records are unavailable in the host and are not fabricated.

Name entry uses the original `ScreenNameEntry background`, `ScreenNameEntry letters`, `MenuTimer numbers`, category font and looping music. Its four timed letter columns run at the original 2.5 characters per second, retain previous initials, accept each pad direction independently, and finish on Start or the 24-second timer. The host forwards real machine/personal song record ranks and commits names through its shared profile request. The supplied reference has only dance noteskins; other game families use the bundled gray receptors as an adaptation.

Game Over uses the original centered image on black, fading to 75% opacity over 0.3 seconds and holding for five seconds before returning to attract. The shared session-ending mount owns no artwork or concrete theme names; game results and the timed name editor live in the score layer.
