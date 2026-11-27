use crate::{allocations, thread_cycles::cycles};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaOverlayKind, SongLuaOverlayState, SongLuaSpeedMod,
};
use std::{path::Path, sync::Arc, time::Instant};

// The default runs the entire original on the OpenGL AFT path. Use duration
// and renderer overrides to isolate work. Count allocations in a separate run.
// Fingerprints are computed after the timed bake.
pub(super) fn run() {
    type Kind = SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;
    let fixture = std::env::var_os("DEADSYNC_PEACH_FIXTURE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/itgmania-song-lua-micro/peach-summer-splash")
        })
        // Compiled overlay data includes paths. Give every spelling of the
        // same fixture directory the same input to the bake and fingerprints.
        .canonicalize()
        .expect("resolve Peach benchmark fixture");
    let chart = std::env::var("DEADSYNC_PEACH_CHART").unwrap_or_else(|_| "original".into());
    let dir = fixture.join("inputs").join(&chart);
    let duration = deadlib_audio_decode::file_length_seconds(
        &dir.join("Peach Summer Splash.ogg"),
        Default::default(),
    )
    .unwrap();
    let metadata = deadsync_simfile::song::parse_song_meta_file(
        &dir.join("Peach Summer Splash.ssc"),
        &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        -0.008,
        |_| duration,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&dir, "Peach Summer Splash");
    context.players[1].enabled = false;
    context.players[0].speedmod = SongLuaSpeedMod::X(2.0);
    context.players[0].noteskin_name = "cel".into();
    context.screen_width = 854.0;
    context.display_width = 854.0;
    context.song_timing_bpms = vec![(0.0, 125.0)];
    context.song_display_bpms = [125.0; 2];
    context.song_timing = metadata.song_timing;
    context.global_offset_seconds = -0.008;
    context.music_length_seconds =
        std::env::var("DEADSYNC_PEACH_DURATION").map_or(duration, |value| value.parse().unwrap());
    context.random_seed = 1;
    context.video_renderers =
        std::env::var("DEADSYNC_PEACH_RENDERER").unwrap_or_else(|_| "opengl".into());
    context.song_steps = metadata
        .charts
        .iter()
        .map(deadsync_song_lua::SongLuaStep::from)
        .collect();
    context.player_step_indices[0] = context
        .song_steps
        .iter()
        .position(|step| step.difficulty == deadsync_song_lua::SongLuaDifficulty::Challenge);
    assert!(context.player_step_indices[0].is_some());

    let path = dir.join("lua/default.lua");
    let bake = || {
        deadsync_song_lua::compile_prepared_song_lua_layers(
            &[&path],
            0,
            &context,
            Default::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None,
            |_, _| None::<(Kind, SongLuaOverlayState)>,
        )
        .unwrap()
    };
    let started = Instant::now();
    let start_cycles = cycles();
    let compiled = if std::env::var_os("DEADSYNC_PEACH_ALLOCATIONS").is_some() {
        let (compiled, churn) = allocations::measure(bake);
        println!(
            "peach_alloc,{chart},{},{},{}",
            churn.allocs, churn.reallocs, churn.allocated_bytes
        );
        compiled
    } else {
        bake()
    };
    let cpu = cycles() - start_cycles;
    let seconds = started.elapsed().as_secs_f64();
    assert_eq!(compiled.len(), 1);
    let compiled = &compiled[0];
    let spline_frames: usize = compiled
        .column_splines
        .iter()
        .map(|track| track.frames.len())
        .sum();
    let draw_ops: usize = compiled
        .draw_frames
        .iter()
        .map(|frame| frame.ops.len())
        .sum();
    println!(
        "peach_bake,{chart},{},{},{cpu},{seconds:.6},{spline_frames},{draw_ops}",
        context.video_renderers, context.music_length_seconds
    );
    assert!(spline_frames > 0);
    if std::env::var_os("DEADSYNC_PEACH_FINGERPRINT").is_some() {
        use std::hash::{Hash, Hasher};
        let mut splines = twox_hash::XxHash64::with_seed(0);
        for track in &compiled.column_splines {
            track.player.hash(&mut splines);
            track.column.hash(&mut splines);
            track.time_offset.to_bits().hash(&mut splines);
            track.frames.len().hash(&mut splines);
            for frame in track.frames.iter() {
                frame.second.to_bits().hash(&mut splines);
                for curve in [&frame.position, &frame.rotation, &frame.zoom] {
                    curve.is_some().hash(&mut splines);
                    if let Some(curve) = curve {
                        curve.absolute.hash(&mut splines);
                        curve.constant.hash(&mut splines);
                        curve.looping.hash(&mut splines);
                        curve.beats_per_t.to_bits().hash(&mut splines);
                        curve.receptor_t.to_bits().hash(&mut splines);
                        curve.subtract_song_beat.hash(&mut splines);
                        curve.coefficients.len().hash(&mut splines);
                        for point in curve.coefficients.iter() {
                            point.map(|axis| axis.map(f32::to_bits)).hash(&mut splines);
                        }
                    }
                }
            }
        }
        println!(
            "peach_fingerprint,{:016x},{:016x}",
            splines.finish(),
            debug_fingerprint(format_args!("{:?}", compiled.draw_frames))
        );
        let overlays = debug_fingerprint(format_args!(
            "{:?}",
            (
                &compiled.overlays,
                &compiled.overlay_eases,
                &compiled.overlay_updates,
                &compiled.stateful_message_captures,
                &compiled.player_actors,
                &compiled.song_foreground,
                &compiled.screen_layers,
            )
        ));
        let gameplay = debug_fingerprint(format_args!(
            "{:?}{:?}",
            (
                &compiled.startup,
                &compiled.beat_mods,
                &compiled.time_mods,
                &compiled.eases,
                &compiled.messages,
                &compiled.hidden_players,
                &compiled.note_hides,
                &compiled.column_offsets,
                &compiled.loaded_steps,
            ),
            (
                &compiled.loaded_players,
                &compiled.loaded_style,
                &compiled.foreground_beats,
                compiled.startup_reloads,
                compiled.screen_overlay_index,
                compiled.overlay_update_unit,
                compiled.overlay_capture_calibration_seconds,
                compiled.column_spline_origin,
                &compiled.info,
            )
        ));
        println!("peach_output_fingerprint,{overlays:016x},{gameplay:016x}");
    }
}

fn debug_fingerprint(args: std::fmt::Arguments<'_>) -> u64 {
    use std::{fmt::Write, hash::Hasher};

    struct Fingerprint(twox_hash::XxHash64);

    impl Write for Fingerprint {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            self.0.write(text.as_bytes());
            Ok(())
        }
    }

    let mut hash = Fingerprint(twox_hash::XxHash64::with_seed(0));
    hash.write_fmt(args).unwrap();
    hash.0.finish()
}
