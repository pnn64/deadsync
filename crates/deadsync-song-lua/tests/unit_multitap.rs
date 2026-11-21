//! Original multitap travel unit on the Rust VM.
use deadlib_lua::Value;
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaSpeedMod, {SongRuntime, test_support},
};
#[path = "support/baked_program.rs"]
mod api;
#[test]
fn multitap_travel_obeys_chart_timing_and_speedmod() {
    use deadsync_rules::timing::{
        ScrollSegment, SpeedSegment, SpeedUnit, TimingData, TimingSegments,
    };
    let mut context = SongLuaCompileContext::new(".", "Multitap timing");
    context.player_timing[0] = Some(TimingData::from_segments(
        0.0,
        0.0,
        &TimingSegments {
            bpms: vec![(0.0, 120.0), (8.0, 240.0)],
            scrolls: vec![
                ScrollSegment {
                    beat: 0.0,
                    ratio: 0.5,
                },
                ScrollSegment {
                    beat: 8.0,
                    ratio: 0.25,
                },
            ],
            speeds: vec![
                SpeedSegment {
                    beat: 0.0,
                    ratio: 0.5,
                    delay: 0.0,
                    unit: SpeedUnit::Beats,
                },
                SpeedSegment {
                    beat: 4.0,
                    ratio: 1.0,
                    delay: 4.0,
                    unit: SpeedUnit::Beats,
                },
            ],
            ..Default::default()
        },
        &[],
    ));
    context.players[0].display_bpms = [120.0, 240.0];
    for (speed, rate, expected) in [
        (SongLuaSpeedMod::X(1.0), 1.0, 72.0),
        (SongLuaSpeedMod::X(2.0), 1.5, 144.0),
        (SongLuaSpeedMod::M(480.0), 2.0, 72.0),
        (SongLuaSpeedMod::C(120.0), 1.0, 192.0),
        (SongLuaSpeedMod::C(120.0), 2.0, 96.0),
    ] {
        context.players[0].speedmod = speed;
        context.song_music_rate = rate;
        assert!((test_support::multitap_y_offset(&context, 0, 6.0, 4.0) - expected).abs() < 0.001);
        let mut host = SongRuntime::new(&context).unwrap();
        host.set_values(6.0, 3.0 / rate).unwrap();
        let (method, value) = match speed {
            SongLuaSpeedMod::X(value) => ("XMod", value),
            SongLuaSpeedMod::M(value) => ("MMod", value),
            SongLuaSpeedMod::C(value) => ("CMod", value),
            _ => unreachable!(),
        };
        let value_method = api::string(host.vm_mut(), method);
        host.vm_mut()
            .set_global(b"speed_method", value_method)
            .unwrap();
        host.vm_mut()
            .set_global(b"speed_value", Value::Number(value as f64))
            .unwrap();
        let actual = api::as_number(
            api::eval(
                host.vm_mut(),
                r#"
                local options = {__songlua_reference_bpm=240}
                options[speed_method] = function() return speed_value end
                local ps = {GetPlayerNumber=function() return "PlayerNumber_P1" end,
                    GetCurrentPlayerOptions=function() return options end}
                return ArrowEffects.GetYOffset(ps, 1, 10) - ArrowEffects.GetYOffset(ps, 1, 6)
            "#,
            )[0],
        ) as f32;
        assert!(
            (actual - expected).abs() < 0.001,
            "Lua ArrowEffects {speed:?}: actual {actual}, expected {expected}"
        );
    }
}

#[test]
fn multitap_travel_uses_real_player_speed_options() {
    use deadsync_rules::timing::{TimingData, TimingSegments};
    let timing = TimingData::from_segments(
        0.0,
        0.0,
        &TimingSegments {
            bpms: vec![(0.0, 120.0), (8.0, 240.0)],
            ..Default::default()
        },
        &[],
    );
    for live in [false, true] {
        for rate in [0.75_f32, 1.0, 1.1, 1.5, 2.0] {
            let mut context = SongLuaCompileContext::new(".", "Multitap player speed options");
            context.live = live;
            context.song_music_rate = rate;
            context.song_timing = Some(timing.clone());
            context.player_timing = [Some(timing.clone()), Some(timing.clone())];
            for (player, reference) in context.players.iter_mut().zip([240.0, 183.0]) {
                player.enabled = true;
                player.display_bpms = [120.0, reference];
                player.speedmod = SongLuaSpeedMod::M(480.0);
            }
            let mut host = SongRuntime::new(&context).unwrap();
            host.set_values(6.0, if live { 3.0 } else { 3.0 / rate })
                .unwrap();
            for (player, reference) in [(1, 240.0_f32), (2, 183.0)] {
                api::eval(
                    host.vm_mut(),
                    format!(
                        "ps=GAMESTATE:GetPlayerState(PLAYER_{player});po=ps:GetCurrentPlayerOptions();\
                         assert(po:XMod()==1 and po:MMod()==480)"
                    ),
                );
                // ITGmania ArrowEffects::GetYOffset gives MaxScrollBPM priority
                // over ScrollSpeed, even though XMod() is also non-nil.
                for (mode, multiplier) in [
                    ("", 480.0 / (reference * rate)),
                    ("po:XMod(2)", 2.0),
                    ("po:CMod(120)", 0.0),
                    ("po:MMod(673)", 673.0 / (reference * rate)),
                ] {
                    api::eval(host.vm_mut(), mode);
                    for (note, seconds) in [(4.0, -1.0), (6.0, 0.0), (10.0, 1.5)] {
                        let actual = api::as_number(
                            api::eval(
                                host.vm_mut(),
                                format!("return ArrowEffects.GetYOffset(ps, 1, {note})"),
                            )[0],
                        ) as f32;
                        let expected = if mode == "po:CMod(120)" {
                            seconds * 120.0 / 60.0 / rate * 64.0
                        } else {
                            (note - 6.0) * 64.0 * multiplier
                        };
                        if mode.is_empty() || mode.starts_with("po:MMod") {
                            assert_eq!(actual.to_bits(), expected.to_bits());
                        }
                        assert!(
                            (actual - expected).abs() < 0.0001,
                            "live={live} rate={rate} P{player} mode={mode:?} note={note}: {actual} != {expected}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn untimed_arrow_fallback_keeps_cmod_spacing() {
    let mut context = SongLuaCompileContext::new(".", "Untimed CMod fallback");
    context.song_music_rate = 1.5;
    context.players[0].display_bpms = [240.0; 2];
    context.players[0].speedmod = SongLuaSpeedMod::C(120.0);
    let mut host = SongRuntime::new(&context).unwrap();
    let actual = api::as_number(
        api::eval(
            host.vm_mut(),
            "return ArrowEffects.GetYOffset(GAMESTATE:GetPlayerState(PLAYER_1), 1, 4)",
        )[0],
    );
    assert_eq!(actual, 128.0);
}

#[test]
fn multitap_speed_matches_native_itgmania_captures() {
    use deadsync_rules::timing::{TimingData, TimingSegments};
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/itgmania-actors/multitap-speed-modifiers.json"
    ))
    .unwrap();
    assert_eq!(reference["provenance"]["execution"], "native_executable");
    assert_eq!(
        reference["provenance"]["native_source"]["modified_src"],
        false
    );
    let mut checked = 0;
    for capture in reference["captures"].as_array().unwrap() {
        assert_eq!(capture["validation"]["complete"], true);
        assert!(
            capture["validation"]["script_errors"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let timing = TimingData::from_segments(
            capture["chart_offset"].as_f64().unwrap() as f32,
            0.0,
            &TimingSegments {
                bpms: capture["bpms"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|entry| {
                        (
                            entry[0].as_f64().unwrap() as f32,
                            entry[1].as_f64().unwrap() as f32,
                        )
                    })
                    .collect(),
                ..Default::default()
            },
            &[],
        );
        let beat = capture["sample_clock"]["player_visible_beat"]
            .as_f64()
            .unwrap() as f32;
        let seconds = capture["sample_clock"]["player_visible_seconds"]
            .as_f64()
            .unwrap() as f32;
        let notes = capture["note_beats"].as_array().unwrap();
        for live in [false, true] {
            for case in capture["speed_samples"].as_array().unwrap() {
                // These native probes retain CMod controls with the cabinet's
                // timing offsets. Pin beat-based M/X selection here; the timed
                // and untimed CMod regressions above cover its unchanged paths.
                if case["mode"] == "CMod" {
                    continue;
                }
                let rate = case["rate"].as_f64().unwrap() as f32;
                let amount = case["amount"].as_f64().unwrap() as f32;
                let mut context = SongLuaCompileContext::new(".", "Native multitap speeds");
                context.live = live;
                context.song_music_rate = rate;
                context.song_timing = Some(timing.clone());
                context.player_timing[0] = Some(timing.clone());
                context.players[0].display_bpms =
                    [capture["display_bpm"].as_f64().unwrap() as f32; 2];
                context.players[0].speedmod = match case["mode"].as_str().unwrap() {
                    "MMod" => SongLuaSpeedMod::M(amount),
                    "XMod" => SongLuaSpeedMod::X(amount),
                    other => panic!("unhandled native speed mode {other}"),
                };
                let mut host = SongRuntime::new(&context).unwrap();
                host.set_values(beat, if live { seconds } else { seconds / rate })
                    .unwrap();
                api::eval(
                    host.vm_mut(),
                    format!(
                        "ps=GAMESTATE:GetPlayerState(PLAYER_1);po=ps:GetCurrentPlayerOptions();\
                     po:Wave({});po:WavePeriod({})",
                        case["wave"], case["wave_period"],
                    ),
                );
                let expected = case["y_offsets"].as_array().unwrap();
                assert_eq!(notes.len(), expected.len());
                for (note, expected) in notes.iter().zip(expected) {
                    let actual = api::as_number(
                        api::eval(
                            host.vm_mut(),
                            format!("return ArrowEffects.GetYOffset(ps, 1, {note})"),
                        )[0],
                    ) as f32;
                    let expected = expected.as_f64().unwrap() as f32;
                    assert_eq!(
                        actual.to_bits(),
                        expected.to_bits(),
                        "native display={} live={live} mode={} rate={rate} wave={} note={note}: {actual} != {expected}",
                        capture["display_bpm"],
                        case["mode"],
                        case["wave"]
                    );
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(
        checked, 294,
        "all native MMod/XMod offsets in preparation and live modes"
    );
}
