use super::*;

pub(super) fn build_advanced_rows(return_screen: Screen) -> RowMap {
    let mut gameplay_extras_choices = vec![
        tr("PlayerOptions", "GameplayExtrasFlashColumnForMiss").to_string(),
        tr("PlayerOptions", "GameplayExtrasDensityGraphAtTop").to_string(),
        tr("PlayerOptions", "GameplayExtrasColumnCues").to_string(),
    ];
    if crate::game::scores::is_gs_get_scores_service_allowed() {
        gameplay_extras_choices
            .push(tr("PlayerOptions", "GameplayExtrasDisplayScorebox").to_string());
    }

    let mut b = RowBuilder::new();
    b.push(Row {
        id: RowId::Turn,
        name: lookup_key("PlayerOptions", "Turn"),
        choices: vec![
            tr("PlayerOptions", "TurnNone").to_string(),
            tr("PlayerOptions", "TurnMirror").to_string(),
            tr("PlayerOptions", "TurnLeft").to_string(),
            tr("PlayerOptions", "TurnRight").to_string(),
            tr("PlayerOptions", "TurnLRMirror").to_string(),
            tr("PlayerOptions", "TurnUDMirror").to_string(),
            tr("PlayerOptions", "TurnShuffle").to_string(),
            tr("PlayerOptions", "TurnBlender").to_string(),
            tr("PlayerOptions", "TurnRandom").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "TurnHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::Scroll,
        name: lookup_key("PlayerOptions", "Scroll"),
        choices: vec![
            tr("PlayerOptions", "ScrollReverse").to_string(),
            tr("PlayerOptions", "ScrollSplit").to_string(),
            tr("PlayerOptions", "ScrollAlternate").to_string(),
            tr("PlayerOptions", "ScrollCross").to_string(),
            tr("PlayerOptions", "ScrollCentered").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ScrollHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::Hide,
        name: lookup_key("PlayerOptions", "Hide"),
        choices: vec![
            tr("PlayerOptions", "HideTargets").to_string(),
            tr("PlayerOptions", "HideBackground").to_string(),
            tr("PlayerOptions", "HideCombo").to_string(),
            tr("PlayerOptions", "HideLife").to_string(),
            tr("PlayerOptions", "HideScore").to_string(),
            tr("PlayerOptions", "HideDanger").to_string(),
            tr("PlayerOptions", "HideComboExplosions").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "HideHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::LifeMeterType,
        name: lookup_key("PlayerOptions", "LifeMeterType"),
        choices: vec![
            tr("PlayerOptions", "LifeMeterTypeStandard").to_string(),
            tr("PlayerOptions", "LifeMeterTypeSurround").to_string(),
            tr("PlayerOptions", "LifeMeterTypeVertical").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "LifeMeterTypeHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::LifeBarOptions,
        name: lookup_key("PlayerOptions", "LifeBarOptions"),
        choices: vec![
            tr("PlayerOptions", "LifeBarOptionsRainbowMax").to_string(),
            tr("PlayerOptions", "LifeBarOptionsResponsiveColors").to_string(),
            tr("PlayerOptions", "LifeBarOptionsShowLifePercentage").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "LifeBarOptionsHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::DataVisualizations,
        name: lookup_key("PlayerOptions", "DataVisualizations"),
        choices: vec![
            tr("PlayerOptions", "DataVisualizationsNone").to_string(),
            tr("PlayerOptions", "DataVisualizationsTargetScoreGraph").to_string(),
            tr("PlayerOptions", "DataVisualizationsStepStatistics").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "DataVisualizationsHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::DensityGraphBackground,
        name: lookup_key("PlayerOptions", "DensityGraphBackground"),
        choices: vec![
            tr("PlayerOptions", "DensityGraphBackgroundSolid").to_string(),
            tr("PlayerOptions", "DensityGraphBackgroundTransparent").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "DensityGraphBackgroundHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::TargetScore,
        name: lookup_key("PlayerOptions", "TargetScore"),
        choices: vec![
            tr("PlayerOptions", "TargetScoreCMinus").to_string(),
            tr("PlayerOptions", "TargetScoreC").to_string(),
            tr("PlayerOptions", "TargetScoreCPlus").to_string(),
            tr("PlayerOptions", "TargetScoreBMinus").to_string(),
            tr("PlayerOptions", "TargetScoreB").to_string(),
            tr("PlayerOptions", "TargetScoreBPlus").to_string(),
            tr("PlayerOptions", "TargetScoreAMinus").to_string(),
            tr("PlayerOptions", "TargetScoreA").to_string(),
            tr("PlayerOptions", "TargetScoreAPlus").to_string(),
            tr("PlayerOptions", "TargetScoreSMinus").to_string(),
            tr("PlayerOptions", "TargetScoreS").to_string(),
            tr("PlayerOptions", "TargetScoreSPlus").to_string(),
            tr("PlayerOptions", "TargetScoreMachineBest").to_string(),
            tr("PlayerOptions", "TargetScorePersonalBest").to_string(),
        ],
        selected_choice_index: [10; PLAYER_SLOTS], // S by default
        help: vec![tr("PlayerOptionsHelp", "TargetScoreHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::ActionOnMissedTarget,
        name: lookup_key("PlayerOptions", "TargetScoreMissPolicy"),
        choices: vec![
            tr("PlayerOptions", "TargetScoreMissPolicyNothing").to_string(),
            tr("PlayerOptions", "TargetScoreMissPolicyFail").to_string(),
            tr("PlayerOptions", "TargetScoreMissPolicyRestartSong").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "TargetScoreMissPolicyHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::MiniIndicator,
        name: lookup_key("PlayerOptions", "MiniIndicator"),
        choices: vec![
            tr("PlayerOptions", "MiniIndicatorNone").to_string(),
            tr("PlayerOptions", "MiniIndicatorSubtractiveScoring").to_string(),
            tr("PlayerOptions", "MiniIndicatorPredictiveScoring").to_string(),
            tr("PlayerOptions", "MiniIndicatorPaceScoring").to_string(),
            tr("PlayerOptions", "MiniIndicatorRivalScoring").to_string(),
            tr("PlayerOptions", "MiniIndicatorPacemaker").to_string(),
            tr("PlayerOptions", "MiniIndicatorStreamProg").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "MiniIndicatorHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::IndicatorScoreType,
        name: lookup_key("PlayerOptions", "IndicatorScoreType"),
        choices: vec![
            tr("PlayerOptions", "IndicatorScoreTypeITG").to_string(),
            tr("PlayerOptions", "IndicatorScoreTypeEX").to_string(),
            tr("PlayerOptions", "IndicatorScoreTypeHEX").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "IndicatorScoreTypeHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::GameplayExtras,
        name: lookup_key("PlayerOptions", "GameplayExtras"),
        choices: gameplay_extras_choices,
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "GameplayExtrasHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::ComboColors,
        name: lookup_key("PlayerOptions", "ComboColors"),
        choices: vec![
            tr("PlayerOptions", "ComboColorsGlow").to_string(),
            tr("PlayerOptions", "ComboColorsSolid").to_string(),
            tr("PlayerOptions", "ComboColorsRainbow").to_string(),
            tr("PlayerOptions", "ComboColorsRainbowScroll").to_string(),
            tr("PlayerOptions", "ComboColorsNone").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ComboColorsHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::ComboColorMode,
        name: lookup_key("PlayerOptions", "ComboColorMode"),
        choices: vec![
            tr("PlayerOptions", "ComboColorModeFullCombo").to_string(),
            tr("PlayerOptions", "ComboColorModeCurrentCombo").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ComboColorModeHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::CarryCombo,
        name: lookup_key("PlayerOptions", "CarryCombo"),
        choices: vec![
            tr("PlayerOptions", "CarryComboNo").to_string(),
            tr("PlayerOptions", "CarryComboYes").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "CarryComboHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::JudgmentTilt,
        name: lookup_key("PlayerOptions", "JudgmentTilt"),
        choices: vec![
            tr("PlayerOptions", "JudgmentTiltNo").to_string(),
            tr("PlayerOptions", "JudgmentTiltYes").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "JudgmentTiltHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::JudgmentTiltIntensity,
        name: lookup_key("PlayerOptions", "JudgmentTiltIntensity"),
        choices: tilt_intensity_choices(),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "JudgmentTiltIntensityHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::JudgmentBehindArrows,
        name: lookup_key("PlayerOptions", "JudgmentBehindArrows"),
        choices: vec![
            tr("PlayerOptions", "JudgmentBehindArrowsOff").to_string(),
            tr("PlayerOptions", "JudgmentBehindArrowsOn").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "JudgmentBehindArrowsHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::OffsetIndicator,
        name: lookup_key("PlayerOptions", "OffsetIndicator"),
        choices: vec![
            tr("PlayerOptions", "OffsetIndicatorOff").to_string(),
            tr("PlayerOptions", "OffsetIndicatorOn").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "OffsetIndicatorHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::ErrorBar,
        name: lookup_key("PlayerOptions", "ErrorBar"),
        choices: vec![
            tr("PlayerOptions", "ErrorBarColorful").to_string(),
            tr("PlayerOptions", "ErrorBarMonochrome").to_string(),
            tr("PlayerOptions", "ErrorBarText").to_string(),
            tr("PlayerOptions", "ErrorBarHighlight").to_string(),
            tr("PlayerOptions", "ErrorBarAverage").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ErrorBarHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::ErrorBarTrim,
        name: lookup_key("PlayerOptions", "ErrorBarTrim"),
        choices: vec![
            tr("PlayerOptions", "ErrorBarTrimOff").to_string(),
            tr("PlayerOptions", "ErrorBarTrimFantastic").to_string(),
            tr("PlayerOptions", "ErrorBarTrimExcellent").to_string(),
            tr("PlayerOptions", "ErrorBarTrimGreat").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ErrorBarTrimHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::ErrorBarOptions,
        name: lookup_key("PlayerOptions", "ErrorBarOptions"),
        choices: vec![
            tr("PlayerOptions", "ErrorBarOptionsMoveUp").to_string(),
            tr("PlayerOptions", "ErrorBarOptionsMultiTick").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ErrorBarOptionsHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::ErrorBarOffsetX,
        name: lookup_key("PlayerOptions", "ErrorBarOffsetX"),
        choices: hud_offset_choices(),
        selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ErrorBarOffsetXHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::ErrorBarOffsetY,
        name: lookup_key("PlayerOptions", "ErrorBarOffsetY"),
        choices: hud_offset_choices(),
        selected_choice_index: [HUD_OFFSET_ZERO_INDEX; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ErrorBarOffsetYHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::MeasureCounter,
        name: lookup_key("PlayerOptions", "MeasureCounter"),
        choices: vec![
            tr("PlayerOptions", "MeasureCounterNone").to_string(),
            tr("PlayerOptions", "MeasureCounter8th").to_string(),
            tr("PlayerOptions", "MeasureCounter12th").to_string(),
            tr("PlayerOptions", "MeasureCounter16th").to_string(),
            tr("PlayerOptions", "MeasureCounter24th").to_string(),
            tr("PlayerOptions", "MeasureCounter32nd").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "MeasureCounterHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::MeasureCounterLookahead,
        name: lookup_key("PlayerOptions", "MeasureCounterLookahead"),
        choices: vec![
            tr("PlayerOptions", "MeasureCounterLookahead0").to_string(),
            tr("PlayerOptions", "MeasureCounterLookahead1").to_string(),
            tr("PlayerOptions", "MeasureCounterLookahead2").to_string(),
            tr("PlayerOptions", "MeasureCounterLookahead3").to_string(),
            tr("PlayerOptions", "MeasureCounterLookahead4").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "MeasureCounterLookaheadHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::MeasureCounterOptions,
        name: lookup_key("PlayerOptions", "MeasureCounterOptions"),
        choices: vec![
            tr("PlayerOptions", "MeasureCounterOptionsMoveLeft").to_string(),
            tr("PlayerOptions", "MeasureCounterOptionsMoveUp").to_string(),
            tr("PlayerOptions", "MeasureCounterOptionsVerticalLookahead").to_string(),
            tr("PlayerOptions", "MeasureCounterOptionsBrokenRunTotal").to_string(),
            tr("PlayerOptions", "MeasureCounterOptionsRunTimer").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "MeasureCounterOptionsHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::MeasureLines,
        name: lookup_key("PlayerOptions", "MeasureLines"),
        choices: vec![
            tr("PlayerOptions", "MeasureLinesOff").to_string(),
            tr("PlayerOptions", "MeasureLinesMeasure").to_string(),
            tr("PlayerOptions", "MeasureLinesQuarter").to_string(),
            tr("PlayerOptions", "MeasureLinesEighth").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "MeasureLinesHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::RescoreEarlyHits,
        name: lookup_key("PlayerOptions", "RescoreEarlyHits"),
        choices: vec![
            tr("PlayerOptions", "RescoreEarlyHitsNo").to_string(),
            tr("PlayerOptions", "RescoreEarlyHitsYes").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "RescoreEarlyHitsHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::EarlyDecentWayOffOptions,
        name: lookup_key("PlayerOptions", "EarlyDecentWayOffOptions"),
        choices: vec![
            tr("PlayerOptions", "EarlyDecentWayOffOptionsHideJudgments").to_string(),
            tr(
                "PlayerOptions",
                "EarlyDecentWayOffOptionsHideNoteFieldFlash",
            )
            .to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "EarlyDecentWayOffOptionsHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::ResultsExtras,
        name: lookup_key("PlayerOptions", "ResultsExtras"),
        choices: vec![tr("PlayerOptions", "ResultsExtrasTrackEarlyJudgments").to_string()],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "ResultsExtrasHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::TimingWindows,
        name: lookup_key("PlayerOptions", "TimingWindows"),
        choices: vec![
            tr("PlayerOptions", "TimingWindowsNone").to_string(),
            tr("PlayerOptions", "TimingWindowsWayOffs").to_string(),
            tr("PlayerOptions", "TimingWindowsDecentsAndWayOffs").to_string(),
            tr("PlayerOptions", "TimingWindowsFantasticsAndExcellents").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "TimingWindowsHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::FAPlusOptions,
        name: lookup_key("PlayerOptions", "FAPlusOptions"),
        choices: vec![
            tr("PlayerOptions", "FAPlusOptionsDisplayFAPlusWindow").to_string(),
            tr("PlayerOptions", "FAPlusOptionsDisplayEXScore").to_string(),
            tr("PlayerOptions", "FAPlusOptionsDisplayHEXScore").to_string(),
            tr("PlayerOptions", "FAPlusOptionsDisplayFAPlusPane").to_string(),
            tr("PlayerOptions", "FAPlusOptions10msBlueWindow").to_string(),
            tr("PlayerOptions", "FAPlusOptions1510msSplit").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "FAPlusOptionsHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::CustomBlueFantasticWindow,
        name: lookup_key("PlayerOptions", "CustomBlueFantasticWindow"),
        choices: vec![
            tr("PlayerOptions", "CustomBlueFantasticWindowNo").to_string(),
            tr("PlayerOptions", "CustomBlueFantasticWindowYes").to_string(),
        ],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "CustomBlueFantasticWindowHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::CustomBlueFantasticWindowMs,
        name: lookup_key("PlayerOptions", "CustomBlueFantasticWindowMs"),
        choices: custom_fantastic_window_choices(),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "CustomBlueFantasticWindowMsHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::WhatComesNext,
        name: lookup_key("PlayerOptions", "WhatComesNext"),
        choices: what_comes_next_choices(OptionsPane::Advanced, return_screen),
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![tr("PlayerOptionsHelp", "WhatComesNextAdvancedHelp").to_string()],
        choice_difficulty_indices: None,
    });
    b.push(Row {
        id: RowId::Exit,
        name: lookup_key("Common", "Exit"),
        choices: vec![tr("Common", "Exit").to_string()],
        selected_choice_index: [0; PLAYER_SLOTS],
        help: vec![String::new()],
        choice_difficulty_indices: None,
    });
    b.finish()
}
