//! Check actual production preparation and reversal tracks without a window or gameplay Lua VM.
use deadsync_gameplay::{SongLuaRuntimeBuilder, SongLuaRuntimeWindowBuild};
use std::{path::PathBuf, sync::Arc, time::Instant};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.len() == 2 || args.len() == 4,
        "seventh_compiled <7th Gear.ssc> <result.json> [m-mod mini-percent]"
    );
    let simfile = PathBuf::from(&args[0]).canonicalize().unwrap();
    let bundle = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let data = bundle.join("target/seventh-compiled/data");
    let dirs = deadsync_config::dirs::AppDirs {
        cache_dir: data.join("cache"),
        data_dir: data,
        exe_dir: bundle,
        portable: false,
    };
    dirs.ensure_dirs_exist();
    deadsync_shell::app::init_paths(&dirs).unwrap();
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    deadlib_present::space::set_current_window_px(1280, 720);
    let song = deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0).unwrap();
    let index = song
        .charts
        .iter()
        .position(|chart| chart.difficulty.eq_ignore_ascii_case("challenge"))
        .unwrap();
    let charts = [
        Arc::new(song.charts[index].clone()),
        Arc::new(song.charts[index].clone()),
    ];
    let payloads =
        deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[index, index], 0.0).unwrap();
    let mut profiles = [
        deadsync_profile::Profile::default(),
        deadsync_profile::Profile::default(),
    ];
    profiles[0].noteskin = deadsync_profile::NoteSkin::new("cel");
    profiles[0].scroll_speed = deadsync_rules::scroll::ScrollSpeedSetting::XMod(2.0);
    if args.len() == 4 {
        profiles[0].scroll_speed = deadsync_rules::scroll::ScrollSpeedSetting::MMod(args[2].parse().unwrap());
        profiles[0].mini_percent = args[3].parse().unwrap();
    }
    let scroll = [profiles[0].scroll_speed, profiles[1].scroll_speed];
    let start = Instant::now();
    let mut prepared = deadsync_shell::gameplay_entry::prepare_song_lua(
        &song,
        &charts,
        [&payloads[0].timing, &payloads[1].timing],
        &profiles,
        &scroll,
        1.0,
        deadsync_gameplay::GameplayViewport::design(),
        (1280, 720),
        16.0 / 9.0,
        1.0,
        &deadsync_gameplay::GameplaySession::default(),
        &deadsync_gameplay::GameplayConfig::default(),
        "OpenGL",
        1,
    );
    assert!(
        prepared.take_live_scene().unwrap().is_none(),
        "production retained a Lua VM"
    );
    let compile_ms = start.elapsed().as_secs_f64() * 1000.0;
    let (constants, eases, visuals) = prepared.build_song_lua_runtime(SongLuaRuntimeWindowBuild {
        song_title: &song.title,
        timing_players: [&payloads[0].timing, &payloads[1].timing],
        num_players: 1,
        machine_global_offset_seconds: 0.0,
        player_global_offset_shift_seconds: &[0.0; 2],
        screen_width: 854.0,
        screen_height: 480.0,
        player_actor_defaults: [deadsync_gameplay::SongLuaPlayerActorDefault { x: 0.0, y: 0.0 }; 2],
    });
    let tracks = &visuals.column_splines[0];
    let afts = visuals
        .overlays
        .iter()
        .filter(|actor| {
            matches!(
                actor.kind,
                deadsync_song_lua::SongLuaOverlayKind::ActorFrameTexture { .. }
            )
        })
        .count();
    assert_eq!(
        afts, 3,
        "the compiled check must retain the actual AFT branch"
    );
    assert_eq!(
        tracks.len(),
        4,
        "selected Single chart must carry all four lanes"
    );
    let mut observations = Vec::new();
    let mini_scale = 1.0 - profiles[0].mini_percent as f32 / 200.0;
    for (beat, direction) in [
        (112.0, Some(1.0)),
        (115.0, Some(-1.0)),
        (123.0, Some(1.0)),
        (175.0, None),
    ] {
        let second = payloads[0].timing.get_time_for_beat_exact(beat);
        for track in tracks {
            let frame = track.at_second(second).unwrap();
            if let Some(direction) = direction {
                let position = frame
                    .position
                    .as_ref()
                    .expect("authored reversal position spline");
                let y0 = position.coefficients[0][1][0];
                let y1 = position.coefficients[1][1][0];
                assert!(
                    (y1 - y0) * direction > 0.0,
                    "wrong reversal direction at beat {beat}, column {}",
                    track.column
                );
                assert!(frame.zoom.is_some(), "lost authored zoom spline");
                assert!((y0 * mini_scale + 135.0 * direction).abs() < 0.001,
                    "constructor Mini disagrees with the gameplay profile at beat {beat}: y0={y0}, Mini={}",
                    profiles[0].mini_percent);
                let samples = [0.0, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0].map(|ahead| {
                    let (position, _) = position.view().sample(beat, beat + ahead);
                    serde_json::json!({"ahead":ahead,"position":position})
                });
                observations.push(serde_json::json!({"beat":beat,"column":track.column,"y0":y0,"y1":y1,"position":true,"zoom":true,
                    "points":position.coefficients.len(),"last_point":position.coefficients.last().map(|axes| axes.map(|axis| axis[0])),"samples":samples,
                    "receptor_y":240.0+y0*mini_scale,"endpoint_y":240.0+position.view().sample(beat,beat+8.0).0[1]*mini_scale}));
            } else {
                assert!(
                    frame.position.is_none() && frame.zoom.is_none(),
                    "splines must reset after the reversal section"
                );
                observations.push(serde_json::json!({"beat":beat,"column":track.column,"position":false,"zoom":false}));
            }
        }
    }
    // The later transition reverses lanes separately, then hands them back to
    // the ordinary Reverse modifier at beat 208.
    for beat in [206.75, 207.5, 208.5] {
        let second = payloads[0].timing.get_time_for_beat_exact(beat);
        for track in tracks {
            let frame = track.at_second(second).unwrap();
            if beat < 208.0 {
                let position = frame.position.as_ref().expect("late lane reversal spline");
                let slope = position.coefficients[1][1][0] - position.coefficients[0][1][0];
                if beat == 207.5 || track.column == 2 {
                    assert!(slope < 0.0);
                }
                if beat == 206.75 && track.column == 3 {
                    assert!(slope > 0.0);
                }
                assert!(frame.zoom.is_none());
                observations.push(serde_json::json!({"beat":beat,"column":track.column,"position":true,"zoom":false,"slope_y":slope}));
            } else {
                assert!(frame.position.is_none() && frame.zoom.is_none());
                observations.push(serde_json::json!({"beat":beat,"column":track.column,"position":false,"zoom":false}));
            }
        }
    }
    let mut attacks = deadsync_gameplay::GameplayAttackRuntimeState::new(constants, eases);
    let mut transform = deadsync_gameplay::SongLuaPlayerTransform::default();
    let mut previous = payloads[0].timing.get_time_for_beat_exact(0.0);
    let mut reverse_samples = Vec::new();
    // Song-level targets approach the previous frame's value. Coarse beat
    // checkpoints under-sample the authored ease, so advance with frame-sized
    // steps before inspecting the handoff.
    for tick in 0..=(209 * 120) {
        let beat = tick as f32 / 120.0;
        let second = payloads[0].timing.get_time_for_beat_exact(beat);
        if let Some(next) = attacks.refresh_player(
            0,
            second,
            (second - previous).max(0.0),
            deadsync_gameplay::AppearanceEffects::default(),
            deadsync_gameplay::AttackBaseEffects::default,
            transform,
        ) {
            transform = next;
        }
        previous = second;
        let expected = match tick {
            24780 => Some(0.0),
            24900 => Some(0.75),
            25020 => Some(1.0),
            _ => None,
        };
        if let Some(expected) = expected {
            let actual = attacks.scroll[0].reverse.unwrap_or(0.0);
            println!("Reverse at beat {beat}: {actual}, expected {expected}");
            assert!(
                (actual - expected).abs() < 0.05,
                "Reverse at beat {beat}: {actual}, expected {expected}"
            );
            reverse_samples.push(serde_json::json!({"beat":beat,"reverse":actual}));
        }
    }
    let checks = observations.len() + reverse_samples.len();
    let result = serde_json::json!({"song":song.title,"chart_hash":charts[0].short_hash,"retained_lua_vm":false,"aft_stages":afts,
        "mini_percent":profiles[0].mini_percent,"scroll_speed":format!("{:?}", profiles[0].scroll_speed),
        "compile_ms":compile_ms,"reversal_checks_passed":checks,"observations":observations,"reverse_modifier_samples":reverse_samples,
        "whole_song_parity_proven":false,"pixel_parity_proven":false});
    std::fs::write(&args[1], serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    println!(
        "PASS production_retained_lua_vm=false reversal_checks={} compile_ms={compile_ms}",
        checks
    );
}
