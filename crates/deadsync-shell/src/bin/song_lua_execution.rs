//! Headless smoke check of the application build's Lua scene ownership and updates.
//!
//! Run as a shell binary, not a Cargo test/example: those unify dev-dependency
//! `test-support` features and can hide differences in the shipped gameplay path.
use deadsync_gameplay::{GameplayAudioSnapshot, GameplayStreamClockSnapshot};
use deadsync_song_lua::SongLuaOverlayKind;
use deadsync_theme_simply_love::screens::gameplay;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Instant};

#[path = "../../../../tests/support/song_lua_metrics.rs"]
mod metrics;

use sha2::{Digest, Sha256};
use std::fmt::Write as _;

struct Fingerprint(Sha256);
impl std::fmt::Write for Fingerprint {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.0.update(text.as_bytes());
        Ok(())
    }
}

#[cfg(windows)]
fn pin_benchmark_thread() -> impl Drop {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessAffinityMask(process: *mut std::ffi::c_void, process_mask: *mut usize, system_mask: *mut usize) -> i32;
        fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
    }
    struct Restore(usize);
    impl Drop for Restore {
        fn drop(&mut self) {
            // SAFETY: restore the mask returned for this live thread.
            unsafe { SetThreadAffinityMask(GetCurrentThread(), self.0); }
        }
    }
    let (mut allowed, mut system) = (0, 0);
    // SAFETY: valid process pseudo-handle and synchronous output pointers.
    assert_ne!(unsafe { GetProcessAffinityMask(GetCurrentProcess(), &mut allowed, &mut system) }, 0);
    // Prefer the second permitted logical CPU; respect process affinity.
    let rest = allowed & (allowed - 1);
    let mask = 1usize << if rest == 0 { allowed.trailing_zeros() } else { rest.trailing_zeros() };
    // SAFETY: mask selects one CPU allowed by the process affinity.
    let previous = unsafe { SetThreadAffinityMask(GetCurrentThread(), mask) };
    assert_ne!(previous, 0);
    Restore(previous)
}

#[cfg(not(windows))]
fn pin_benchmark_thread() -> impl Drop { Box::new(()) }

// Hash observations outside the timed section. Include every pose property and
// mesh/text/texture payload, rather than just checking the final actor position.
fn fingerprint(state: &gameplay::State, hash: &mut Fingerprint) {
    let visuals = &state.gameplay.mods.song_lua_visuals;
    for actor in visuals.overlays.iter()
        .chain(visuals.background_visual_layers.iter().flat_map(|layer| &layer.overlays))
        .chain(visuals.foreground_visual_layers.iter().flat_map(|layer| &layer.overlays))
    {
        write!(hash, "{:?}{:?}{:?}", actor.name, actor.initial_state, actor.kind).unwrap();
    }
    write!(hash, "{:?}{:?}{:?}", visuals.player_actors,
        visuals.song_foreground, visuals.screen_layers).unwrap();
}

fn snapshot(state: &gameplay::State) -> Value {
    let visuals = &state.gameplay.mods.song_lua_visuals;
    let actors = visuals
        .overlays
        .iter()
        .chain(
            visuals
                .background_visual_layers
                .iter()
                .flat_map(|layer| &layer.overlays),
        )
        .chain(
            visuals
                .foreground_visual_layers
                .iter()
                .flat_map(|layer| &layer.overlays),
        );
    Value::Array(
        actors
            .map(|actor| {
                let state = &actor.initial_state;
                let vertices = match &actor.kind {
                    SongLuaOverlayKind::ActorMultiVertex { vertices, .. } => vertices
                        .iter()
                        .map(|vertex| json!([vertex.pos[0], vertex.pos[1], vertex.z]))
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                json!({"name":actor.name,"x":state.x,"y":state.y,"z":state.z,
            "visible":state.visible,"vertices":vertices})
            })
            .collect(),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 && !(args.len() == 4 && args[2] == "--benchmark") {
        return Err("song_lua_execution <song.ssc> <result.json> [--benchmark FRAMES]".into());
    }
    let benchmark = args.len() == 4;
    let _affinity = benchmark.then(pin_benchmark_thread);
    let frame_count = if benchmark { args[3].parse::<usize>()? } else { 241 };
    if frame_count <= 60 { return Err("benchmark requires more than 60 warmup frames".into()); }
    let simfile = PathBuf::from(&args[0]).canonicalize()?;
    let bundle = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let data = bundle.join("target/song-lua-execution/data");
    let dirs = deadsync_config::dirs::AppDirs {
        cache_dir: data.join("cache"),
        data_dir: data,
        exe_dir: bundle,
        portable: false,
    };
    dirs.ensure_dirs_exist();
    deadsync_shell::app::init_paths(&dirs)?;
    deadsync_theme_simply_love::i18n::init(deadsync_assets::language::load("en"));
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    deadlib_present::space::set_current_window_px(1280, 720);
    let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(
        &simfile, 0.0,
    )?);
    let index = song
        .charts
        .iter()
        .position(|chart| chart.difficulty.eq_ignore_ascii_case("challenge"))
        .unwrap_or(0);
    let chart = Arc::new(song.charts[index].clone());
    let mut charts = [chart.clone(), chart];
    let mut loaded = deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[index], 0.0)?;
    let payload = Arc::new(loaded.remove(0));
    let mut payloads = [payload.clone(), payload];
    let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
    let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
    let viewport = deadsync_gameplay::GameplayViewport::design();
    let mut session = deadsync_gameplay::GameplaySession::default();
    let config = deadsync_gameplay::GameplayConfig {
        global_offset_seconds: 0.0,
        ..Default::default()
    };
    let start = Instant::now();
    let prepared = deadsync_shell::gameplay_entry::prepare_song_lua(
        &song,
        &charts,
        [&payloads[0].timing, &payloads[1].timing],
        &profiles,
        &scroll,
        1.0,
        viewport,
        (1280, 720),
        16.0 / 9.0,
        0.0,
        &session,
        &config,
        "Vulkan",
        1,
    );
    deadsync_shell::gameplay_entry::apply_song_lua_reload(
        &song,
        &mut charts,
        &mut payloads,
        &prepared,
        &mut session,
        config.global_offset_seconds,
    )?;
    // Transfer the prepared scene through the real screen initializer. Merely
    // calling take_live_scene here would miss a rejection at screen attachment.
    let mut state = gameplay::init(
        song.clone(),
        charts,
        payloads,
        viewport,
        session,
        config,
        5,
        1.0,
        scroll,
        profiles,
        None,
        None,
        None,
        Arc::from("EVENT"),
        None,
        None,
        None,
        None,
        None,
        [deadsync_gameplay::CourseLifeConfig::Bar; 2],
        false,
        None,
        None,
        [0; 2],
        prepared,
        Default::default(),
    );
    let prepare_ms = start.elapsed().as_secs_f64() * 1000.0;
    let retained_lua_vm = state.song_lua.is_some();
    let initial = snapshot(&state);
    let mut previous = initial.clone();
    let mut changed_frames = 0;
    let mut frames = 0;
    let mut actor_sound_operations = 0;
    let mut errors = Vec::new();
    let mut samples = Vec::new();
    let mut hash = Fingerprint(Sha256::new());
    if retained_lua_vm {
        for frame in 0..frame_count {
            let seconds = frame as f64 / 60.0;
            let host = 1_000_000_000 + (seconds * 1e9).round() as u64;
            let measurement = (benchmark && frame >= 60).then(metrics::Measurement::start);
            gameplay::update(
                &mut state,
                if frame == 0 { 0.0 } else { 1.0 / 60.0 },
                GameplayAudioSnapshot {
                    stream_clock: GameplayStreamClockSnapshot {
                        music_nanos: (seconds * 1e9).round() as i64,
                        has_music_mapping: true,
                        valid_at_host_nanos: host,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                || host,
                &mut Vec::new(),
            );
            if let Some(measurement) = measurement { samples.push(measurement.finish()); }
            let output = state.take_song_lua_output();
            errors.extend(output.errors);
            actor_sound_operations += output.actor_sounds.len();
            if benchmark { fingerprint(&state, &mut hash); }
            let current = snapshot(&state);
            changed_frames += usize::from(current != previous);
            previous = current;
            frames += 1;
        }
    }
    let passed = retained_lua_vm && errors.is_empty() && frames == frame_count && changed_frames > 1;
    let mut fingerprint = String::with_capacity(64);
    for byte in hash.0.finalize() { write!(&mut fingerprint, "{byte:02x}")?; }
    let total_seconds: f64 = samples.iter().map(|sample| sample["wall_seconds"].as_f64().unwrap()).sum();
    let total_cycles: u64 = samples.iter().filter_map(|sample| sample["thread_cpu_cycles"].as_u64()).sum();
    let total_allocations: u64 = samples.iter().map(|sample| sample["allocations"].as_u64().unwrap()).sum();
    let total_reallocations: u64 = samples.iter().map(|sample| sample["reallocations"].as_u64().unwrap()).sum();
    let total_bytes: u64 = samples.iter().map(|sample| sample["allocated_bytes"].as_u64().unwrap()).sum();
    let result = json!({"song":song.title,"source_simfile":simfile,"retained_lua_vm":retained_lua_vm,
        "execution_path":"application-gameplay-init-and-update","prepare_ms":prepare_ms,
        "frames":frames,"changed_visual_frames":changed_frames,"actor_sound_operations":actor_sound_operations,
        "errors":errors,"initial_actors":initial,"final_actors":previous,
        "startup_smoke_passed":passed,"native_parity_checked":false,"pixel_parity_proven":false,
        "benchmark": benchmark.then(|| json!({"warmup_frames":60,"samples":samples,
            "updates_per_second":samples.len() as f64 / total_seconds,
            "cpu_cycles_per_update":cfg!(windows).then_some(total_cycles as f64 / samples.len() as f64),
            "cpu_cycle_source":if cfg!(windows) { "QueryThreadCycleTime, pinned update thread" } else { "unavailable" },
            "debug_assertions":cfg!(debug_assertions),
            "allocations_per_update":total_allocations as f64 / samples.len() as f64,
            "reallocations_per_update":total_reallocations as f64 / samples.len() as f64,
            "allocated_bytes_per_update":total_bytes as f64 / samples.len() as f64,
            "visual_sha256":fingerprint,"measurement":"gameplay::update including live Lua and visual publication; no GPU/audio device; observation hashing excluded"}))});
    std::fs::write(&args[1], serde_json::to_vec_pretty(&result)?)?;
    println!(
        "retained_lua_vm={retained_lua_vm}, frames={frames}, changed_visual_frames={changed_frames}, startup_smoke_passed={passed}"
    );
    if !passed {
        return Err(format!("gameplay Lua startup failed; see {}", args[1]).into());
    }
    Ok(())
}
