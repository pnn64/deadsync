use super::*;
use std::hint::black_box;

pub fn original_load_visual_player_options<F>(options: &mut PlayerOptionsData, mut get: F)
where
    F: FnMut(&str) -> Option<String>,
{
    options.background_filter = get("BackgroundFilter")
        .and_then(|s| BackgroundFilter::from_str(&s).ok())
        .unwrap_or(options.background_filter);
    options.hold_judgment_graphic = get("HoldJudgmentGraphic")
        .and_then(|s| HoldJudgmentGraphic::from_str(&s).ok())
        .unwrap_or_else(|| options.hold_judgment_graphic.clone());
    options.held_miss_graphic = get("HeldGraphic")
        .or_else(|| get("HeldMissGraphic"))
        .and_then(|s| HeldMissGraphic::from_str(&s).ok())
        .unwrap_or_else(|| options.held_miss_graphic.clone());
    options.judgment_graphic = get("JudgmentGraphic")
        .and_then(|s| JudgmentGraphic::from_str(&s).ok())
        .unwrap_or_else(|| options.judgment_graphic.clone());
    options.combo_font = get("ComboFont")
        .and_then(|s| ComboFont::from_str(&s).ok())
        .unwrap_or(options.combo_font);
    options.combo_colors = get("ComboColors")
        .and_then(|s| ComboColors::from_str(&s).ok())
        .unwrap_or(options.combo_colors);
    options.combo_mode = get("ComboMode")
        .and_then(|s| ComboMode::from_str(&s).ok())
        .unwrap_or(options.combo_mode);
    options.carry_combo_between_songs = get("CarryComboBetweenSongs")
        .or_else(|| get("ComboContinuesBetweenSongs"))
        .and_then(|s| s.parse::<u8>().ok())
        .map_or(options.carry_combo_between_songs, |v| v != 0);
    options.noteskin = get("NoteSkin")
        .and_then(|s| NoteSkin::from_str(&s).ok())
        .unwrap_or_else(|| options.noteskin.clone());
    options.mine_noteskin = get("MineSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    options.receptor_noteskin = get("ReceptorSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    options.tap_explosion_noteskin =
        get("TapExplosionSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    options.arrow_noteskin = get("ArrowSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    options.hold_active_noteskin = get("HoldActiveSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    options.hold_inactive_noteskin =
        get("HoldInactiveSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    options.roll_active_noteskin = get("RollActiveSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    options.roll_inactive_noteskin =
        get("RollInactiveSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    options.hold_explosion_noteskin =
        get("HoldExplosionSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    options.lift_noteskin = get("LiftSkin").and_then(|s| NoteSkin::from_str(&s).ok());
    options.mine_size_percent = get("MineSizePercent")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(100)
        .clamp(10, 200);
    migrate_noteskin_parts(options);
    let tap_explosion_mask_version = get("TapExplosionMaskVersion")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(1);
    options.tap_explosion_active_mask = get("TapExplosionMask")
        .and_then(|s| s.parse::<u8>().ok())
        .map(|bits| normalize_tap_explosion_mask(bits, tap_explosion_mask_version))
        .unwrap_or(options.tap_explosion_active_mask);
    options.mini_percent = get("MiniPercent")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.mini_percent);
    options.spacing_percent = get("Spacing")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.spacing_percent);
    options.perspective = get("Perspective")
        .and_then(|s| Perspective::from_str(&s).ok())
        .unwrap_or(options.perspective);
    options.note_field_offset_x = get("NoteFieldOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.note_field_offset_x);
    options.note_field_offset_y = get("NoteFieldOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.note_field_offset_y);
    options.judgment_offset_x = get("JudgmentOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.judgment_offset_x);
    options.judgment_offset_y = get("JudgmentOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.judgment_offset_y);
    options.combo_offset_x = get("ComboOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.combo_offset_x);
    options.combo_offset_y = get("ComboOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.combo_offset_y);
    options.error_bar_offset_x = get("ErrorBarOffsetX")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.error_bar_offset_x);
    options.error_bar_offset_y = get("ErrorBarOffsetY")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(options.error_bar_offset_y);
    options.visual_delay_ms = get("VisualDelayMs")
        .or_else(|| get("VisualDelay"))
        .and_then(|s| s.trim_end_matches("ms").parse::<i32>().ok())
        .unwrap_or(options.visual_delay_ms);
    options.global_offset_shift_ms = get("GlobalOffsetShiftMs")
        .and_then(|s| s.trim_end_matches("ms").parse::<i32>().ok())
        .unwrap_or(options.global_offset_shift_ms);
}

#[test]
fn visual_options_preserve_values_aliases_and_getter_order() {
    let cases: &[&[(&str, &str)]] = &[
        &[],
        &[
            ("HoldJudgmentGraphic", ""),
            ("HeldGraphic", ""),
            ("HeldMissGraphic", "ITG2"),
            ("JudgmentGraphic", ""),
            ("NoteSkin", ""),
        ],
        &[
            ("HoldJudgmentGraphic", "ITG2"),
            ("HeldMissGraphic", "None"),
            ("JudgmentGraphic", "Love"),
            ("NoteSkin", "cel"),
        ],
        &[
            ("HeldGraphic", "Love"),
            ("HeldMissGraphic", "None"),
            ("NoteSkin", "default?arrows=blue&mines=red"),
            ("MiniPercent", "150"),
            ("MineSizePercent", "250"),
        ],
        &[
            ("HoldJudgmentGraphic", "custom/雪.png"),
            ("HeldGraphic", "custom/held.png"),
            ("JudgmentGraphic", "custom/judge.png"),
            ("NoteSkin", "雪"),
            ("TapExplosionMaskVersion", "1"),
            ("TapExplosionMask", "255"),
        ],
    ];
    for case in cases {
        let mut before = PlayerOptionsData::default();
        before.noteskin = NoteSkin::new("custom");
        before.hold_judgment_graphic = HoldJudgmentGraphic::new("mute");
        before.held_miss_graphic = HeldMissGraphic::new("None");
        before.judgment_graphic = JudgmentGraphic::new("ITG2");
        before.mine_noteskin = Some(NoteSkin::new("prior"));
        let mut after = before.clone();
        let mut old_calls = Vec::new();
        let mut new_calls = Vec::new();
        original_load_visual_player_options(&mut before, |key| {
            old_calls.push(key.to_owned());
            case.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_owned())
        });
        load_visual_player_options(&mut after, |key| {
            new_calls.push(key.to_owned());
            case.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_owned())
        });
        assert_eq!(before, after, "{case:?}");
        assert_eq!(old_calls, new_calls, "{case:?}");
    }
}

#[test]
fn missing_visual_options_retain_owned_default_buffers() {
    let mut options = PlayerOptionsData::default();
    let ptrs = [
        options.hold_judgment_graphic.as_str().as_ptr(),
        options.held_miss_graphic.as_str().as_ptr(),
        options.judgment_graphic.as_str().as_ptr(),
        options.noteskin.as_str().as_ptr(),
    ];
    let (_, churn) = crate::perf::measure(|| load_visual_player_options(&mut options, |_| None));
    assert_eq!(churn.allocs, 0);
    assert_eq!(
        ptrs,
        [
            options.hold_judgment_graphic.as_str().as_ptr(),
            options.held_miss_graphic.as_str().as_ptr(),
            options.judgment_graphic.as_str().as_ptr(),
            options.noteskin.as_str().as_ptr()
        ]
    );
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_profile_dataflow_visual_defaults() {
    let cases: &[(&str, &[(&str, &str)])] = &[
        ("visual/missing", &[]),
        (
            "visual/partial",
            &[("NoteSkin", "cel"), ("MiniPercent", "25")],
        ),
        (
            "visual/invalid",
            &[
                ("HoldJudgmentGraphic", ""),
                ("HeldGraphic", ""),
                ("JudgmentGraphic", ""),
                ("NoteSkin", ""),
            ],
        ),
        (
            "visual/complete",
            &[
                ("HoldJudgmentGraphic", "ITG2"),
                ("HeldGraphic", "None"),
                ("JudgmentGraphic", "Love"),
                ("NoteSkin", "cel"),
            ],
        ),
    ];
    for &(label, fields) in cases {
        let mut old = PlayerOptionsData::default();
        let mut new = old.clone();
        crate::dataflow_perf::compare(
            label,
            || {
                original_load_visual_player_options(black_box(&mut old), |key| {
                    black_box(fields)
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, v)| (*v).to_owned())
                });
                black_box(&old);
            },
            || {
                load_visual_player_options(black_box(&mut new), |key| {
                    black_box(fields)
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, v)| (*v).to_owned())
                });
                black_box(&new);
            },
        );
        assert_eq!(old, new);
    }
}
