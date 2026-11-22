use super::*;
use crate::noteskin::{SpriteSlot, TapExplosion};
use deadsync_core::input::MAX_COLS;
use deadsync_noteskin::runtime::{SkinPart, TapExplosionLayer};
use deadsync_noteskin::{ExplosionAnimation, ReceptorStepBehaviors};
use std::cell::RefCell;

fn slot(marker: f32) -> SpriteSlot {
    let mut slot = noteskin::test_model_slot();
    slot.beat_receptor_start = Some(marker);
    slot
}

fn skin(marker: f32) -> Noteskin {
    Noteskin {
        notes: vec![slot(marker)],
        note_layers: Vec::new(),
        lift_note_layers: Vec::new(),
        receptor_off: Vec::new(),
        receptor_glow: Vec::new(),
        receptor_idle_glow_layers: Vec::new(),
        receptor_overlays: Vec::new(),
        receptor_off_reverse: Vec::new(),
        receptor_glow_reverse: Vec::new(),
        receptor_idle_glow_reverse: Vec::new(),
        receptor_step_behaviors: Vec::new(),
        mines: vec![Some(slot(marker))],
        mine_fill_slots: Vec::new(),
        mine_layers: Vec::new(),
        column_xs: vec![-96, -32, 32, 96],
        tap_explosions: Default::default(),
        tap_explosions_by_col: Vec::new(),
        mine_hit_explosion: None,
        receptor_glow_behavior: Default::default(),
        receptor_idle_glow: Default::default(),
        receptor_pulse: Default::default(),
        hold_let_go_gray_percent: 0.5,
        hold_columns: Vec::new(),
        roll_columns: Vec::new(),
        hold: Default::default(),
        roll: Default::default(),
        custom_parts: Default::default(),
        part_animation_is_beat_based: [false; deadsync_noteskin::NOTE_ANIM_PART_COUNT],
        note_display_metrics: Default::default(),
    }
}

fn profiles() -> [profile_data::Profile; MAX_PLAYERS] {
    std::array::from_fn(|_| profile_data::Profile::default())
}

fn assert_shared_components(assets: &GameplayNoteskinAssets, player: usize) {
    let base = assets.noteskin[player]
        .as_ref()
        .expect("active player's skin");
    for component in [
        &assets.mine_noteskin[player],
        &assets.receptor_noteskin[player],
        &assets.tap_explosion_noteskin[player],
    ] {
        assert!(Arc::ptr_eq(
            base,
            component.as_ref().expect("visible component")
        ));
    }
}

#[test]
fn chart_override_precedes_profile_parts_and_retimes_only_the_play_copy() {
    let mut profiles = profiles();
    profiles[0].noteskin = profile_data::NoteSkin::new("unused-base");
    profiles[0].arrow_noteskin = Some(profile_data::NoteSkin::new("unused-arrows"));
    let expected_options = profiles[0].scroll_option.to_string();
    let mut source = skin(3.0);
    source.receptor_off = vec![slot(-4.0), noteskin::test_model_slot(), slot(2.0)];
    let source = Arc::new(source);
    let assets = gameplay_noteskin_assets_with_loaders(
        4,
        1,
        &profiles,
        Path::new("songs/chart-a"),
        &[Some("chart-owned".to_owned()), Some("inactive".to_owned())],
        20.0,
        |_, skin| panic!("a successful chart override must skip profile skin {skin}"),
        |style, song_dir, skin, options| {
            assert_eq!((style.num_cols, style.num_players), (4, 1));
            assert_eq!(song_dir, Path::new("songs/chart-a"));
            assert_eq!(skin, "chart-owned");
            assert_eq!(options, expected_options);
            Ok(source.clone())
        },
    );
    let selected = assets.noteskin[0].as_ref().unwrap();
    assert!(!Arc::ptr_eq(selected, &source));
    assert_eq!(
        selected
            .receptor_off
            .iter()
            .map(|slot| slot.beat_receptor_start)
            .collect::<Vec<_>>(),
        [Some(12.0), None, Some(12.0)],
    );
    assert_eq!(
        source
            .receptor_off
            .iter()
            .map(|slot| slot.beat_receptor_start)
            .collect::<Vec<_>>(),
        [Some(-4.0), None, Some(2.0)],
    );
    assert_eq!(selected.notes[0].beat_receptor_start, Some(3.0));
    assert_shared_components(&assets, 0);
    assert!(assets.noteskin[1].is_none());
    assert!(assets.mine_noteskin[1].is_none());
    assert!(assets.receptor_noteskin[1].is_none());
    assert!(assets.tap_explosion_noteskin[1].is_none());

    let cloned = assets.clone();
    assert!(Arc::ptr_eq(selected, cloned.noteskin[0].as_ref().unwrap(),));
}

#[test]
fn failed_chart_and_profile_skins_keep_fallback_order_and_skip_missing_parts() {
    let mut profiles = profiles();
    profiles[0].noteskin = profile_data::NoteSkin::new("missing-base");
    profiles[0].arrow_noteskin = Some(profile_data::NoteSkin::new("missing-arrows"));
    profiles[0].receptor_noteskin = Some(profile_data::NoteSkin::none_choice());
    let fallback = Arc::new(skin(7.0));
    for fallback_available in [true, false] {
        let calls = RefCell::new(Vec::new());
        let assets = gameplay_noteskin_assets_with_loaders(
            8,
            1,
            &profiles,
            Path::new("songs/chart-b"),
            &[Some("missing-chart".to_owned()), None],
            0.0,
            |style, skin| {
                assert_eq!((style.num_cols, style.num_players), (8, 1));
                calls.borrow_mut().push(format!("skin:{skin}"));
                if skin == profile_data::NoteSkin::DEFAULT_NAME && fallback_available {
                    Ok(fallback.clone())
                } else {
                    Err("missing fixture".to_owned())
                }
            },
            |_, _, skin, _| {
                calls.borrow_mut().push(format!("chart:{skin}"));
                Err("missing fixture".to_owned())
            },
        );
        let mut expected = vec![
            "chart:missing-chart".to_owned(),
            "skin:missing-base".to_owned(),
            format!("skin:{}", profile_data::NoteSkin::DEFAULT_NAME),
        ];
        if fallback_available {
            expected.push("skin:missing-arrows".to_owned());
            assert!(Arc::ptr_eq(assets.noteskin[0].as_ref().unwrap(), &fallback));
            assert_shared_components(&assets, 0);
            assert!(
                !assets.noteskin[0]
                    .as_ref()
                    .unwrap()
                    .custom_parts
                    .contains(SkinPart::Arrows)
            );
        } else {
            assert!(assets.noteskin[0].is_none());
            assert!(assets.receptor_noteskin[0].is_none());
            assert!(assets.mine_noteskin[0].is_none());
            assert!(assets.tap_explosion_noteskin[0].is_none());
        }
        assert_eq!(*calls.borrow(), expected);
        assert!(assets.noteskin[1].is_none());
    }
}

#[test]
fn legacy_component_migration_respects_explicit_choices_and_base_geometry() {
    let mut profiles = profiles();
    let legacy = "workshop?arrows=old&receptors=old&hold_active=a&hold_inactive=b&roll_active=c&roll_inactive=d&tap_explosions=e&hold_explosions=f&mines=blue&mine_size=0.5&lifts=g";
    profiles[0].noteskin = profile_data::NoteSkin::new(legacy);
    profiles[0].arrow_noteskin = Some(profile_data::NoteSkin::new("explicit-arrows"));
    profiles[0].receptor_noteskin = Some(profile_data::NoteSkin::none_choice());
    profiles[0].tap_explosion_noteskin = Some(profile_data::NoteSkin::none_choice());
    let base = Arc::new(skin(1.0));
    let mut component = skin(9.0);
    component.column_xs = vec![-200, 200];
    let component = Arc::new(component);
    let mut calls = Vec::new();
    let assets = gameplay_noteskin_assets_with_loaders(
        4,
        1,
        &profiles,
        Path::new("."),
        &[None, None],
        0.0,
        |_, skin| {
            calls.push(skin.to_owned());
            Ok(if skin == "workshop" {
                base.clone()
            } else {
                component.clone()
            })
        },
        |_, _, _, _| panic!("no chart override"),
    );
    assert_eq!(
        calls,
        [
            "workshop",
            "explicit-arrows",
            "workshop?hold_active=a",
            "workshop?hold_inactive=b",
            "workshop?roll_active=c",
            "workshop?roll_inactive=d",
            "workshop?hold_explosions=f",
            "workshop?mines=blue&mine_size=0.5",
            "workshop?lifts=g",
        ]
    );
    let selected = assets.noteskin[0].as_ref().unwrap();
    assert_eq!(selected.notes[0].beat_receptor_start, Some(9.0));
    assert_eq!(
        selected.mines[0].as_ref().unwrap().beat_receptor_start,
        Some(9.0)
    );
    assert_eq!(selected.column_xs, base.column_xs);
    assert!(!Arc::ptr_eq(selected, &base));
    assert_eq!(base.notes[0].beat_receptor_start, Some(1.0));
    for part in [
        SkinPart::Arrows,
        SkinPart::HoldActive,
        SkinPart::HoldInactive,
        SkinPart::RollActive,
        SkinPart::RollInactive,
        SkinPart::HoldExplosions,
        SkinPart::Mines,
        SkinPart::Lifts,
    ] {
        assert!(selected.custom_parts.contains(part), "{part:?}");
        assert!(!base.custom_parts.contains(part), "{part:?}");
    }
    assert!(!selected.custom_parts.contains(SkinPart::Receptors));
    assert!(!selected.custom_parts.contains(SkinPart::TapExplosions));
    assert!(assets.tap_explosion_noteskin[0].is_none());
    assert_eq!(profiles[0].noteskin.as_str(), legacy);
    assert_eq!(
        profiles[0].arrow_noteskin.as_ref().unwrap().as_str(),
        "explicit-arrows"
    );
}

#[test]
fn active_player_limits_do_not_load_or_copy_inactive_skins() {
    let mut profiles = profiles();
    profiles[0].noteskin = profile_data::NoteSkin::new("p1");
    profiles[1].noteskin = profile_data::NoteSkin::new("p2");
    let first = Arc::new(skin(1.0));
    let second = Arc::new(skin(2.0));
    for player_count in [0, 1, MAX_PLAYERS, usize::MAX] {
        let mut calls = Vec::new();
        let assets = gameplay_noteskin_assets_with_loaders(
            5,
            player_count,
            &profiles,
            Path::new("."),
            &[None, None],
            0.0,
            |style, skin| {
                assert_eq!((style.num_cols, style.num_players), (5, 1));
                calls.push(skin.to_owned());
                Ok(if skin == "p1" {
                    first.clone()
                } else {
                    second.clone()
                })
            },
            |_, _, _, _| panic!("no chart override"),
        );
        assert_eq!(calls.len(), player_count.min(MAX_PLAYERS));
        for player in 0..MAX_PLAYERS {
            if player < player_count {
                let expected = if player == 0 { &first } else { &second };
                assert!(Arc::ptr_eq(
                    assets.noteskin[player].as_ref().unwrap(),
                    expected
                ));
                assert_shared_components(&assets, player);
            } else {
                assert!(assets.noteskin[player].is_none());
                assert!(assets.mine_noteskin[player].is_none());
                assert!(assets.receptor_noteskin[player].is_none());
                assert!(assets.tap_explosion_noteskin[player].is_none());
            }
        }
    }
}

fn animation(duration: f32) -> ExplosionAnimation {
    let mut animation = ExplosionAnimation::default();
    animation.segments[0].duration = duration;
    animation
}

fn explosion(durations: &[f32]) -> TapExplosion {
    let first = durations.first().copied().unwrap();
    TapExplosion {
        slot: noteskin::test_model_slot(),
        animation: animation(first),
        layers: durations
            .iter()
            .map(|&duration| TapExplosionLayer {
                slot: noteskin::test_model_slot(),
                animation: animation(duration),
            })
            .collect::<Vec<_>>()
            .into(),
    }
}

#[test]
fn feedback_uses_component_sources_column_overrides_and_longest_explosion_layer() {
    let mut profiles = profiles();
    profiles[1].tap_explosion_noteskin = Some(profile_data::NoteSkin::none_choice());
    let mut base = skin(1.0);
    base.tap_explosions.insert_window("W1", explosion(&[8.0]));
    base.mine_hit_explosion = Some(explosion(&[0.125]));
    let base = Arc::new(base);

    let mut tap = skin(2.0);
    tap.tap_explosions
        .insert_window("W1", explosion(&[0.25, 0.5]));
    tap.tap_explosions
        .insert_window("W1Bright", explosion(&[0.75]));
    tap.tap_explosions.insert_window("W2", explosion(&[0.625]));
    tap.tap_explosions
        .insert_window("Held", explosion(&[0.375]));
    tap.tap_explosions_by_col = vec![Default::default(), Default::default()];
    tap.tap_explosions_by_col[1].insert_window("W1", explosion(&[0.875]));
    let mut mine = skin(3.0);
    mine.mine_hit_explosion = Some(explosion(&[0.25, 1.25, 0.5]));
    let assets = GameplayNoteskinAssets {
        noteskin: [Some(base.clone()), Some(base.clone())],
        tap_explosion_noteskin: [Some(Arc::new(tap)), Some(base)],
        mine_noteskin: [Some(Arc::new(mine)), None],
        ..Default::default()
    };
    let effects = assets.gameplay_data(2, MAX_PLAYERS, &profiles).effects;
    assert_eq!(effects.tap_explosion_duration(0, 0, "W1", false), Some(0.5));
    assert_eq!(
        effects.tap_explosion_duration(0, 1, "W1", false),
        Some(0.875)
    );
    assert_eq!(effects.tap_explosion_duration(0, 1, "W1", true), Some(0.75));
    assert_eq!(
        effects.tap_explosion_duration(0, 0, "W2", true),
        Some(0.625)
    );
    assert_eq!(
        effects.tap_explosion_duration(0, 1, "Held", true),
        Some(0.375)
    );
    assert_eq!(effects.tap_explosion_duration(0, 1, "Miss", false), None);
    assert_eq!(effects.tap_explosion_duration(0, 1, "W3", true), None);
    assert_eq!(effects.tap_explosion_duration(0, 2, "W1", false), None);
    for window in TAP_EXPLOSION_WINDOWS {
        for bright in [false, true] {
            assert_eq!(effects.tap_explosion_duration(1, 0, window, bright), None);
        }
    }
    assert_eq!(effects.mine_explosion_duration(0), 1.25);
    assert_eq!(effects.mine_explosion_duration(1), 0.125);

    profiles[1].tap_explosion_noteskin = None;
    let fallback_assets = GameplayNoteskinAssets {
        noteskin: assets.noteskin.clone(),
        ..Default::default()
    };
    let fallback = fallback_assets
        .gameplay_data(1, MAX_PLAYERS, &profiles)
        .effects;
    assert_eq!(
        fallback.tap_explosion_duration(0, 0, "W1", false),
        Some(8.0)
    );
    assert_eq!(fallback.tap_explosion_duration(1, 0, "W1", true), Some(8.0));
    assert_eq!(fallback.mine_explosion_duration(0), 0.125);
}

#[test]
fn receptor_feedback_preserves_press_release_and_window_animation_for_every_tween() {
    let profiles = profiles();
    let base = Arc::new(skin(0.0));
    for (index, tween) in [
        TweenType::Linear,
        TweenType::Accelerate,
        TweenType::Decelerate,
        TweenType::Smooth,
    ]
    .into_iter()
    .enumerate()
    {
        let mut receptor = skin(1.0);
        receptor.receptor_glow_behavior = ReceptorGlowBehavior {
            draw_before_target: true,
            press_duration: 0.5,
            press_alpha_start: 0.25,
            press_alpha_end: 0.875,
            press_zoom_start: 0.75,
            press_zoom_end: 1.25,
            press_tween: tween,
            lift_interrupts_press: index % 2 == 0,
            lift_finishes_press: index % 2 != 0,
            lift_alpha_start: Some(0.375),
            lift_zoom_start: Some(1.125),
            duration: 0.75,
            alpha_start: 0.625,
            alpha_end: 0.125,
            zoom_start: 1.5,
            zoom_end: 0.5,
            tween,
            blend_add: index % 2 == 0,
        };
        let step = |duration: f32| ReceptorStepBehavior {
            duration,
            zoom_start: 0.375,
            zoom_end: 1.625,
            tween,
            interrupts: index % 2 == 0,
        };
        receptor.receptor_step_behaviors = vec![
            ReceptorStepBehaviors::new(
                step(0.5),
                step(0.75),
                std::array::from_fn(|i| step((i + 1) as f32)),
            ),
            ReceptorStepBehaviors::new(
                step(1.5),
                step(1.75),
                std::array::from_fn(|i| step((i + 2) as f32)),
            ),
        ];
        let receptor = Arc::new(receptor);
        let assets = GameplayNoteskinAssets {
            noteskin: [Some(base.clone()), None],
            receptor_noteskin: [Some(receptor.clone()), None],
            ..Default::default()
        };
        let effects = assets.gameplay_data(2, 1, &profiles).effects;
        let source = receptor.receptor_glow_behavior;
        let prepared = effects.receptor_glow_behavior_for_player(0);
        assert_eq!(prepared.lift_interrupts_press, source.lift_interrupts_press);
        assert_eq!(prepared.lift_finishes_press, source.lift_finishes_press);
        assert_eq!(prepared.lift_alpha_start, source.lift_alpha_start);
        assert_eq!(prepared.lift_zoom_start, source.lift_zoom_start);
        assert_eq!(prepared.alpha_start, source.alpha_start);
        assert_eq!(prepared.zoom_start, source.zoom_start);
        assert_eq!(prepared.blend_add, source.blend_add);
        for remaining in [-1.0, 0.0, 0.125, 0.25, 0.5, 0.75, 2.0] {
            assert_eq!(
                prepared.sample_press(remaining),
                source.sample_press(remaining)
            );
            assert_eq!(
                prepared.sample_lift(remaining, 0.375, 1.125),
                source.sample_lift(remaining, 0.375, 1.125),
            );
        }
        for col in 0..2 {
            for window in RECEPTOR_STEP_WINDOWS {
                let source = receptor.receptor_step_behavior_for_col(col, window);
                let prepared = effects.receptor_step_behavior_for_col(0, col, window);
                assert_eq!(prepared.duration, source.duration);
                assert_eq!(prepared.interrupts, source.interrupts);
                for fraction in [-0.25, 0.0, 0.25, 0.5, 1.0, 2.0] {
                    let remaining = source.duration * fraction;
                    assert_eq!(
                        prepared.sample_zoom(remaining),
                        source.sample_zoom(remaining)
                    );
                }
            }
        }
        let default = GameplayNoteskinEffects::default();
        assert_eq!(
            effects.receptor_step_behavior_for_col(0, 2, Some("W1")),
            default.receptor_step_behavior_for_col(0, 2, Some("W1")),
        );
        assert_eq!(
            effects.receptor_step_behavior_for_col(1, 0, Some("W1")),
            default.receptor_step_behavior_for_col(1, 0, Some("W1")),
        );
    }
}

#[test]
fn missing_assets_and_oversized_counts_keep_gameplay_defaults() {
    let profiles = profiles();
    let defaults = GameplayNoteskinAssets::default()
        .gameplay_data(usize::MAX, usize::MAX, &profiles)
        .effects;
    for player in 0..MAX_PLAYERS {
        assert_eq!(
            defaults.mine_explosion_duration(player),
            MINE_EXPLOSION_DURATION
        );
        assert_eq!(
            defaults.receptor_glow_behavior_for_player(player).duration,
            0.2
        );
        assert_eq!(
            defaults.tap_explosion_duration(player, MAX_COLS - 1, "W1", true),
            None
        );
    }

    let mut base = skin(1.0);
    base.tap_explosions.insert_window("W1", explosion(&[0.375]));
    let base = Arc::new(base);
    let assets = GameplayNoteskinAssets {
        noteskin: [Some(base.clone()), Some(base)],
        ..Default::default()
    };
    let effects = assets
        .gameplay_data(usize::MAX, usize::MAX, &profiles)
        .effects;
    for player in 0..MAX_PLAYERS {
        assert_eq!(
            effects.tap_explosion_duration(player, MAX_COLS - 1, "W1", true),
            Some(0.375)
        );
        assert_eq!(
            effects.mine_explosion_duration(player),
            MINE_EXPLOSION_DURATION
        );
    }
    let inactive = assets.gameplay_data(usize::MAX, 0, &profiles).effects;
    assert_eq!(inactive.tap_explosion_duration(0, 0, "W1", false), None);
}
