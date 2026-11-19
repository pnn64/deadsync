#![cfg(feature = "test-support")]

use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::{TimingData, TimingSegments};
use deadsync_song_lua::*;

type Compiled = CompiledSongLua<
    SongLuaOverlayActor<SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>>,
>;

fn compile(speed: SongLuaSpeedMod, command: &str, frames: &[f64]) -> Compiled {
    compile_with_update(speed, command, "", frames)
}

fn compile_with_update(
    speed: SongLuaSpeedMod,
    command: &str,
    update: &str,
    frames: &[f64],
) -> Compiled {
    compile_with_option_clocks(speed, command, update, frames, &[])
}

fn compile_with_option_clocks(
    speed: SongLuaSpeedMod,
    command: &str,
    update: &str,
    frames: &[f64],
    option_deltas: &[[f32; 3]],
) -> Compiled {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("default.lua");
    std::fs::write(
        &path,
        format!(
            r#"
            local po=GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
            return Def.ActorFrame {{
                OnCommand=function(self)
                    {command}
                    self:SetUpdateFunction(function() {update} end)
                end,
            }}
            "#
        ),
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(directory.path(), "Current speed capture");
    context.players[0].speedmod = speed;
    context.players[1].enabled = !option_deltas.is_empty();
    context.compile_option_clock_deltas = option_deltas.to_vec();
    context.music_length_seconds = *frames.last().unwrap() as f32;
    context.song_display_bpms = [60.; 2];
    context.song_timing_bpms = vec![(0., 60.)];
    context.compile_clock_inputs = frames
        .iter()
        .enumerate()
        .map(|(index, &second)| {
            [
                second,
                second,
                if index == 0 {
                    0.
                } else {
                    second - frames[index - 1]
                },
            ]
        })
        .collect();
    let compiled = compile_song_lua::<(), (), _>(
        &path,
        &context,
        Default::default(),
        |_, _, _| Ok(Default::default()),
        |_| None,
        |_, _| None,
    )
    .unwrap();
    assert!(
        compiled.live_scene.is_none(),
        "exercise ordinary baked output"
    );
    compiled
}

fn gameplay_runtime(compiled: &Compiled) -> deadsync_gameplay::GameplayAttackRuntimeState {
    let timing = TimingData::from_segments(
        0.,
        0.,
        &TimingSegments {
            bpms: vec![(0., 60.)],
            ..Default::default()
        },
        &[],
    );
    // Exercise the production tail extension, target enum and application
    // order; choosing a "latest" source window here would bypass those paths.
    let windows = std::array::from_fn(|player| {
        let (windows, unsupported) =
            gameplay::build_song_lua_ease_windows_for_player(compiled, &timing, player, 0., &[]);
        assert_eq!(unsupported, 0);
        for window in &windows {
            if matches!(
                window.target,
                deadsync_gameplay::SongLuaEaseMaskTarget::ScrollSpeedX
                    | deadsync_gameplay::SongLuaEaseMaskTarget::ScrollSpeedC
                    | deadsync_gameplay::SongLuaEaseMaskTarget::ScrollSpeedM
            ) {
                assert_eq!(
                    window.approach_speed, None,
                    "Current must not approach a second time"
                );
            }
        }
        windows
    });
    deadsync_gameplay::GameplayAttackRuntimeState::new([Vec::new(), Vec::new()], windows)
}

fn runtime_speed_at(
    runtime: &mut deadsync_gameplay::GameplayAttackRuntimeState,
    second: f32,
) -> (&'static str, f32) {
    let _ = runtime.refresh_player(
        0,
        second,
        1. / 60.,
        deadsync_gameplay::AppearanceEffects::default(),
        deadsync_gameplay::AttackBaseEffects::default,
        deadsync_gameplay::SongLuaPlayerTransform::default(),
    );
    match runtime.scroll_speed[0].expect("captured Current speed reaches gameplay") {
        ScrollSpeedSetting::XMod(value) => ("xmod", value),
        ScrollSpeedSetting::CMod(value) => ("cmod", value),
        ScrollSpeedSetting::MMod(value) => ("mmod", value),
    }
}

fn assert_speed(compiled: &Compiled, second: f32, key: &str, expected: f32) {
    let (actual_key, actual) = runtime_speed_at(&mut gameplay_runtime(compiled), second);
    assert_eq!(actual_key, key, "effective speed mode at {second}s");
    assert!(
        (actual - expected).abs() < 0.0001,
        "speed at {second}s: {actual} instead of expected {expected}"
    );
}

#[test]
fn visual_only_baked_updates_leave_profile_speed_unoverridden() {
    for speed in [SongLuaSpeedMod::M(690.), SongLuaSpeedMod::X(1.7)] {
        let compiled = compile_with_update(
            speed,
            "",
            "SCREENMAN:GetTopScreen():GetChild('PlayerP1'):x(GAMESTATE:GetSongBeat())",
            &[0., 0.25, 0.5],
        );
        let mut runtime = gameplay_runtime(&compiled);
        for second in [0.25, 0.5, 1.] {
            let transform = runtime
                .refresh_player(
                    0,
                    second,
                    1. / 60.,
                    deadsync_gameplay::AppearanceEffects::default(),
                    deadsync_gameplay::AttackBaseEffects::default,
                    deadsync_gameplay::SongLuaPlayerTransform::default(),
                )
                .expect("the visual update still reaches gameplay");
            assert_eq!(transform.x, Some(second.min(0.5)));
            assert_eq!(
                runtime.scroll_speed[0], None,
                "baking a visual callback must not replace the player's profile speed"
            );
        }
    }
}

#[test]
fn captured_speed_does_not_override_an_untouched_second_player() {
    let compiled = compile_with_option_clocks(
        SongLuaSpeedMod::X(1.),
        "po:XMod(2, 10000)",
        "SCREENMAN:GetTopScreen():GetChild('PlayerP2'):x(GAMESTATE:GetSongBeat())",
        &[0., 0.25, 0.5],
        &[[0.; 3], [0.25; 3], [0.25; 3]],
    );
    let mut runtime = gameplay_runtime(&compiled);
    assert_eq!(runtime_speed_at(&mut runtime, 0.5), ("xmod", 2.));
    let transform = runtime
        .refresh_player(
            1,
            0.5,
            1. / 60.,
            deadsync_gameplay::AppearanceEffects::default(),
            deadsync_gameplay::AttackBaseEffects::default,
            deadsync_gameplay::SongLuaPlayerTransform::default(),
        )
        .expect("the untouched player's visual callback still reaches gameplay");
    assert_eq!(transform.x, Some(0.5));
    assert_eq!(runtime.scroll_speed[1], None);
}

#[test]
fn baked_speed_keeps_approaching_mmod_after_a_text_xmod_request() {
    // Independent native control: speed-mode-approach-input.json and its
    // captured result in tests/fixtures/itgmania-song-lua-micro. FromString
    // changes ScrollSpeed's approach but leaves MaxScrollBPM's approach at1.
    let compiled = compile(
        SongLuaSpeedMod::M(690.),
        "po:FromString('clearall'); po:FromString('*2 2x')",
        &[0., 0.125, 0.5, 10.],
    );
    for (second, expected) in [(0., 690.), (0.125, 689.875), (0.5, 689.5), (10., 680.)] {
        assert_speed(&compiled, second, "mmod", expected);
    }
}

#[test]
fn baked_speed_switches_mode_only_after_current_max_scroll_bpm_reaches_zero() {
    let compiled = compile(
        SongLuaSpeedMod::M(690.),
        "po:XMod(1.3, 10000)",
        &[0., 0.0625, 0.125, 0.25],
    );
    assert_speed(&compiled, 0., "mmod", 690.);
    assert_speed(&compiled, 0.0625, "mmod", 65.);
    // Between captured frames, retain the sampled mode until the next input.
    assert_speed(&compiled, 0.1, "mmod", 65.);
    assert_speed(&compiled, 0.125, "xmod", 1.3);
    assert_speed(&compiled, 0.25, "xmod", 1.3);
}

#[test]
fn baked_speed_preserves_continuous_and_frozen_native_approaches() {
    let frames = [0., 0.125, 0.5, 10.];
    let continuous = compile(
        SongLuaSpeedMod::X(1.),
        "po:FromString('clearall'); po:FromString('*2 2x')",
        &frames,
    );
    for (second, expected) in [(0., 1.), (0.125, 1.25), (0.5, 2.), (10., 2.)] {
        assert_speed(&continuous, second, "xmod", expected);
    }
    let frozen = compile(SongLuaSpeedMod::X(1.), "po:XMod(3, 0)", &frames);
    for second in frames {
        assert_speed(&frozen, second as f32, "xmod", 1.);
    }
}

#[test]
fn baked_speed_mode_reentry_wins_over_prior_mode_tails() {
    let compiled = compile_with_update(
        SongLuaSpeedMod::M(690.),
        "po:XMod(1.3, 10000); local restored=false",
        "if not restored and GAMESTATE:GetSongBeat() >= .25 then
            restored=true; po:MMod(690, 10000)
        end",
        &[0., 0.0625, 0.125, 0.25, 0.3125, 0.375, 0.5],
    );
    let mut runtime = gameplay_runtime(&compiled);
    for (second, expected_key, expected) in [
        (0., "mmod", 690.),
        (0.0625, "mmod", 65.),
        (0.125, "xmod", 1.3),
        (0.25, "xmod", 1.3),
        (0.3, "xmod", 1.3),
        (0.3125, "mmod", 625.),
        (0.375, "mmod", 690.),
        (0.5, "mmod", 690.),
        (1., "mmod", 690.),
    ] {
        let (key, value) = runtime_speed_at(&mut runtime, second);
        assert_eq!(key, expected_key, "effective mode at {second}s");
        assert!(
            (value - expected).abs() < 0.0001,
            "effective speed at {second}s: {value} instead of {expected}"
        );
    }
}

#[test]
fn captured_speed_uses_independent_song_and_player_option_clocks() {
    let compiled = compile_with_option_clocks(
        SongLuaSpeedMod::X(1.),
        "PREFSMAN:SetPreference('RateModsAffectTweens', false)
         po:XMod(3, 1)
         GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions('ModsLevel_Song'):XMod(3, 1)
         GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate(1)
         GAMESTATE:GetSongOptionsObject('ModsLevel_Song'):MusicRate(2, 1)",
        "SCREENMAN:GetTopScreen():GetChild('PlayerP1')
            :x(GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate())",
        &[0., 1., 2.],
        &[[0.; 3], [0.03125, 0.0625, 0.125], [0.25, 0.125, 0.]],
    );
    let mut runtime = gameplay_runtime(&compiled);
    for (second, song_rate, speeds) in [
        (1., 1.03125, [1.0625, 1.125]),
        (2., 1.28125, [1.1875, 1.125]),
    ] {
        for (player, speed) in speeds.into_iter().enumerate() {
            let transform = runtime
                .refresh_player(
                    player,
                    second,
                    1.,
                    deadsync_gameplay::AppearanceEffects::default(),
                    deadsync_gameplay::AttackBaseEffects::default,
                    deadsync_gameplay::SongLuaPlayerTransform::default(),
                )
                .expect("captured option sample reaches gameplay");
            assert_eq!(
                runtime.scroll_speed[player],
                Some(ScrollSpeedSetting::XMod(speed)),
                "player {} option clock at {second}s",
                player + 1
            );
            if player == 0 {
                assert_eq!(
                    transform.x,
                    Some(song_rate),
                    "SongOptions uses its own timer before the callback"
                );
            }
        }
    }
}
