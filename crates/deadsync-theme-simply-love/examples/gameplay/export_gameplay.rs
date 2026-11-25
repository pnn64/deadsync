//! Export the real Simply Love gameplay presentation without changing player settings.
mod export_gameplay_viewer;
mod export_worker;
use crate::export_support;
pub use export_support::Size;

use deadlib_present::{
    actors::Actor,
    compose::{self, ComposeScratch, TextLayoutCache},
    space::{self, Metrics},
    texture::TextureContext,
};
use deadlib_render::RenderFrame;
use deadsync_gameplay::{
    GameplayInputPlayStyle, GameplayInputPlayerSide, GameplaySession, GameplayViewport,
};
use deadsync_profile as profile;
use deadsync_song_lua::gameplay_presentation::{FramePresentationContext, SongFrameContent};
use deadsync_theme_simply_love::{self as theme, screens::gameplay, views::GameplayInitView};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use export_support::{CaptureTextures, PRESETS, RenderAssets, ScreenshotTimer};
use export_worker::{CaptureSpec, ExportProfiling, ProfilePhase};

pub(super) type Result<T> = export_support::Result<T>;
const LAYOUTS: &[&str] = &["p1", "p2", "versus", "double"];
const HELP: &str = "\
Export Simply Love gameplay PNGs (requires a desktop and Vulkan-capable GPU).

cargo run -p deadsync-theme-simply-love --example export_screenshot -- gameplay [OPTIONS]

  --preset NAME   Select a preset; repeat to select several (default: all)
  --reverse       Use reverse scrolling
    --images-only   Export images without writing the gallery index
    --index-only    Write the gallery index without exporting images
  --help          Show this help

Available presets: vga, svga, xga, 5-4, square, 16-10, 720p, ultrawide,
uwqhd, portrait-9-16, portrait-10-16.";

struct Options {
    sizes: Vec<Size>,
    reverse: bool,
    images_only: bool,
    index_only: bool,
}

fn options(args: impl IntoIterator<Item = String>) -> Result<Option<Options>> {
    let mut args = args.into_iter();
    let mut sizes = Vec::new();
    let mut reverse = false;
    let mut images_only = false;
    let mut index_only = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{HELP}");
                return Ok(None);
            }
            "--reverse" => reverse = true,
            "--images-only" => images_only = true,
            "--index-only" => index_only = true,
            "--preset" => {
                let name = args.next().ok_or("--preset requires a name")?;
                let &(_, width, height) = PRESETS
                    .iter()
                    .find(|preset| preset.0 == name)
                    .ok_or_else(|| format!("unknown preset '{name}'; use --help"))?;
                let size = Size::from_preset(width, height);
                if !sizes.contains(&size) {
                    sizes.push(size);
                }
            }
            _ => return Err(format!("unknown option '{arg}'; use --help").into()),
        }
    }
    if images_only && index_only {
        return Err("--images-only and --index-only cannot be combined".into());
    }
    if sizes.is_empty() {
        for &(_, width, height) in PRESETS {
            let size = Size::from_preset(width, height);
            if !sizes.contains(&size) {
                sizes.push(size);
            }
        }
    }
    Ok(Some(Options {
        sizes,
        reverse,
        images_only,
        index_only,
    }))
}

pub(super) fn run(args: Vec<String>) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        return std::thread::Builder::new()
            .name("gameplay-export".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || run_with_args(args).map_err(|error| error.to_string()))?
            .join()
            .map_err(|_| "gameplay export thread panicked")?
            .map_err(Into::into);
    }
    #[cfg(not(target_os = "windows"))]
    run_with_args(args)
}

fn run_with_args(args: impl IntoIterator<Item = String>) -> Result<()> {
    let Some(options) = options(args)? else {
        return Ok(());
    };
    let output = export_support::workspace_root()
        .join("target")
        .join("gameplay-exports");
    if options.index_only {
        std::fs::create_dir_all(&output)?;
        export_gameplay_viewer::write_index(&output, &options.sizes, options.reverse)?;
        return Ok(());
    }
    let started = Instant::now();
    export_support::init_logger()?;
    let isolated = tempfile::tempdir()?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let dirs = deadsync_config::dirs::AppDirs {
        data_dir: isolated.path().to_path_buf(),
        cache_dir: isolated.path().join("cache"),
        exe_dir: root.to_path_buf(),
        portable: false,
    };
    export_support::initialize_runtime(&dirs)?;
    theme::i18n::init_for_tests();
    let (simfile, pack_directory, pack_name) = sample_song(isolated.path())?;
    let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(
        &simfile, 0.0,
    )?);
    let charts = [
        Arc::new(song.charts[0].clone()),
        Arc::new(song.charts[1].clone()),
    ];
    let cfg = deadsync_config::runtime::get();
    let gameplay_charts = [
        Arc::new(
            deadsync_simfile::app_runtime::load_gameplay_charts(
                &song,
                &[0],
                deadsync_simfile::runtime::RuntimeSongConfig::new(
                    cfg.fastload,
                    cfg.cachesongs,
                    0.0,
                ),
                deadsync_config::runtime::group_is_never_cached,
            )?
            .remove(0),
        ),
        Arc::new(
            deadsync_simfile::app_runtime::load_gameplay_charts(
                &song,
                &[1],
                deadsync_simfile::runtime::RuntimeSongConfig::new(
                    cfg.fastload,
                    cfg.cachesongs,
                    0.0,
                ),
                deadsync_config::runtime::group_is_never_cached,
            )?
            .remove(0),
        ),
    ];
    deadsync_simfile::runtime_cache::set_song_cache(vec![deadsync_chart::SongPack {
        group_name: pack_name.clone(),
        name: pack_name,
        sort_title: "Layout Preview Pack".into(),
        translit_title: String::new(),
        series: String::new(),
        folder_series: String::new(),
        year: 2026,
        sync_pref: deadsync_chart::SyncPref::Default,
        directory: pack_directory.clone(),
        banner_path: Some(pack_directory.join("banner.png")),
        songs: vec![song.clone()],
    }]);
    let mut event_loop_builder = winit::event_loop::EventLoop::builder();
    #[cfg(target_os = "windows")]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        event_loop_builder.with_any_thread(true);
    }
    let event_loop = event_loop_builder.build()?;
    #[expect(
        deprecated,
        reason = "batch screenshots use a hidden window without an interactive loop"
    )]
    let window = Arc::new(
        event_loop.create_window(
            winit::window::Window::default_attributes()
                .with_title("Simply Love gameplay exporter")
                .with_visible(false)
                .with_inner_size(winit::dpi::PhysicalSize::new(640, 480)),
        )?,
    );
    export(&options, song, charts, gameplay_charts, window)?;
    println!(
        "Gameplay export completed in {:.1}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn export(
    options: &Options,
    song: Arc<deadsync_chart::SongData>,
    charts: [Arc<deadsync_chart::ChartData>; 2],
    gameplay_charts: [Arc<deadsync_chart::GameplayChartData>; 2],
    window: Arc<winit::window::Window>,
) -> Result<()> {
    let mut backend = export_support::create_backend(window)?;
    let mut assets = RenderAssets::new();
    export_support::load_initial_assets(&mut assets, &mut backend)?;
    let mut pipeline = export_support::CapturePipeline::new(backend, assets);
    let output = export_support::workspace_root()
        .join("target")
        .join("gameplay-exports");
    std::fs::create_dir_all(&output)?;
    if !options.images_only {
        export_gameplay_viewer::write_index(&output, &options.sizes, options.reverse)?;
    }
    let mut reports = Vec::with_capacity(options.sizes.len());
    for size in &options.sizes {
        let mut timer = ScreenshotTimer::new();
        let logical_width = 480.0 * size.aspect.0 as f32 / size.aspect.1 as f32;
        let logical_height = 480.0;
        let metrics = Metrics::centered(logical_width, logical_height);
        timer.record_resize(pipeline.begin_resolution(size, metrics.projection())?);
        let report_size = size.clone();
        let resolution_size = size.clone();
        let output = output.clone();
        let song = Arc::clone(&song);
        let charts = charts.clone();
        let gameplay_charts = gameplay_charts.clone();
        let reverse = options.reverse;
        let backend_timings = pipeline.run_producer(&mut timer, move |producer| {
            space::set_current_metrics(metrics);
            space::set_current_window_px(resolution_size.width, resolution_size.height);
            let mut captures = Vec::with_capacity(12);
            for &layout in LAYOUTS {
                let mut variants = vec![(format!("{layout}"), false)];
                if matches!(layout, "p1" | "p2") {
                    variants.push((format!("{layout}-centered"), true));
                }
                for (export_layout, center_single_notefield) in variants {
                    let folder = output.join(resolution_size.name()).join(&export_layout);
                    std::fs::create_dir_all(&folder)?;
                    for (show_scorebox, stem) in [(true, "gameplay"), (false, "gameplay-hmr")] {
                        let filename =
                            format!("{stem}{}.png", if reverse { "-reverse" } else { "" });
                        captures.push(CaptureSpec {
                            layout,
                            show_scorebox,
                            center_single_notefield,
                            path: folder.join(filename),
                        });
                    }
                }
            }

            let worker_count = std::thread::available_parallelism()
                .map_or(1, std::num::NonZeroUsize::get)
                .min(4)
                .min(captures.len());
            let profiling = ExportProfiling::default();
            for batch in captures.chunks(worker_count) {
                let prepared_batch = profiling.measure(ProfilePhase::StateBatchWall, || {
                    export_worker::prepare_batch(
                        batch,
                        &song,
                        &charts,
                        &gameplay_charts,
                        logical_width,
                        logical_height,
                        reverse,
                        metrics,
                        resolution_size.width,
                        resolution_size.height,
                        &profiling,
                    )
                })?;
                for (capture, mut prepared) in batch.iter().zip(prepared_batch) {
                    let frame_started = Instant::now();
                    let texture_keys = profiling.measure(ProfilePhase::TextureKeys, || {
                        noteskin_texture_keys(&prepared.state)
                    });
                    profiling.measure(ProfilePhase::TextureUpload, || {
                        producer.load_textures(texture_keys)
                    })?;
                    let frame = profiling.measure(ProfilePhase::Compose, || {
                        prepare_capture_frame(
                            &mut prepared.state,
                            producer,
                            &metrics,
                            capture.center_single_notefield,
                        )
                    })?;
                    producer.submit(
                        frame,
                        capture.path.clone(),
                        prepared.elapsed + frame_started.elapsed(),
                    )?;
                }
            }
            profiling.report(&resolution_size.name(), worker_count);
            Ok(())
        })?;
        reports.push((timer, report_size, backend_timings));
    }
    pipeline.finish()?;
    for (timer, size, backend_timings) in reports {
        timer.report("gameplay", &size, Some(backend_timings))?;
    }
    println!(
        "Exported {} PNGs. Gallery: {}",
        options.sizes.len()
            * LAYOUTS
                .iter()
                .map(|layout| if matches!(*layout, "p1" | "p2") { 4 } else { 2 })
                .sum::<usize>(),
        output.join("index.html").display()
    );
    Ok(())
}

fn sample_song(directory: &Path) -> Result<(PathBuf, PathBuf, String)> {
    let pack_name = "Simply Love Preview Pack".to_owned();
    let pack_directory = directory.join(&pack_name);
    let song_directory = pack_directory.join("Layout Preview");
    std::fs::create_dir_all(&song_directory)?;
    let banner = image::RgbaImage::from_fn(512, 160, |x, y| {
        image::Rgba([40 + (x / 4) as u8, 40 + (y / 2) as u8, 150, 255])
    });
    banner.save(song_directory.join("banner.png"))?;
    pack_banner().save(pack_directory.join("banner.png"))?;
    let mut sm = String::from(
        "#TITLE:Simply Love Layout Preview;#SUBTITLE:All HUD widgets;#ARTIST:DeadSync;\
        #BPMS:0=150;#OFFSET:0;#BANNER:banner.png;\n",
    );
    for (style, columns) in [("dance-single", 4), ("dance-double", 8)] {
        sm.push_str(&format!(
            "#NOTES:{style}:Layout Preview:Challenge:14:0,0,0,0,0:\n"
        ));
        for measure in 0..96 {
            if measure != 0 {
                sm.push_str(",\n");
            }
            for row in 0..16 {
                let mut notes = vec![b'0'; columns];
                if measure % 12 < 8 || row % 4 == 0 {
                    notes[(row + measure) % columns] = b'1';
                    if row % 8 == 0 {
                        notes[(row + measure + 2) % columns] = b'1';
                    }
                }
                match (measure, row) {
                    (0, 4) => notes[0] = b'2',
                    (0, 8) => notes[0] = b'3',
                    (0, 12) => notes[1] = b'M',
                    (1, 4) => notes[0] = b'4',
                    (1, 6) => notes[0] = b'3',
                    _ => {}
                }
                sm.push_str(std::str::from_utf8(&notes)?);
                sm.push('\n');
            }
        }
        sm.push_str(";\n");
    }
    let path = song_directory.join("preview.sm");
    std::fs::write(&path, sm)?;
    Ok((path, pack_directory, pack_name))
}

fn pack_banner() -> image::RgbaImage {
    image::RgbaImage::from_fn(512, 160, |x, y| {
        let diagonal = (x + 2 * y) % 144;
        let glow = (110_i32 - (x as i32 - 370).abs() / 2 - (y as i32 - 76).abs()).max(0) as u8;
        let stripe = if diagonal < 18 { 30 } else { 0 };
        image::Rgba([
            28_u8
                .saturating_add((x / 3) as u8)
                .saturating_add(glow / 2)
                .saturating_add(stripe),
            24_u8.saturating_add((y / 2) as u8).saturating_add(glow / 3),
            76_u8.saturating_add((x / 5) as u8).saturating_add(glow),
            255,
        ])
    })
}

fn sample_profile(reverse: bool, show_scorebox: bool) -> profile::Profile {
    profile::Profile {
        step_statistics: profile::StepStatisticsMask::all_widgets(),
        step_stats_extra: profile::StepStatsExtra::ErrorStats,
        show_fa_plus_window: true,
        show_ex_score: true,
        show_fa_plus_pane: true,
        error_bar_active_mask: profile::ErrorBarMask::all(),
        error_bar: profile::ErrorBarStyle::Colorful,
        error_bar_text: true,
        error_ms_display: true,
        center_tick: true,
        error_bar_multi_tick: true,
        short_average_error_bar_enabled: true,
        long_error_bar_enabled: true,
        live_timing_stats: true,
        live_timing_stats_mask: profile::LiveTimingStatsMask::all(),
        show_life_percent: true,
        display_scorebox: show_scorebox,
        column_cues: true,
        measure_cues: true,
        column_countdown: true,
        broken_run: true,
        run_timer: true,
        mini_indicator: profile::MiniIndicator::SubtractiveScoring,
        measure_counter: profile::MeasureCounter::Sixteenth,
        measure_lines: profile::MeasureLines::Measure,
        reverse_scroll: reverse,
        ..Default::default()
    }
}

fn sample_state(
    song: Arc<deadsync_chart::SongData>,
    charts: &[Arc<deadsync_chart::ChartData>; 2],
    gameplay_charts: &[Arc<deadsync_chart::GameplayChartData>; 2],
    width: f32,
    height: f32,
    layout: &str,
    reverse: bool,
    show_scorebox: bool,
    center_single_notefield: bool,
    profiling: &ExportProfiling,
) -> Result<gameplay::State> {
    let setup_scope = profiling.scope(ProfilePhase::StateSetup);
    let (play_style, side) = match layout {
        "p1" => (GameplayInputPlayStyle::Single, GameplayInputPlayerSide::P1),
        "p2" => (GameplayInputPlayStyle::Single, GameplayInputPlayerSide::P2),
        "versus" => (GameplayInputPlayStyle::Versus, GameplayInputPlayerSide::P1),
        "double" => (GameplayInputPlayStyle::Double, GameplayInputPlayerSide::P1),
        _ => return Err(format!("invalid layout {layout}").into()),
    };
    let chart_index = usize::from(play_style == GameplayInputPlayStyle::Double);
    let payload = Arc::clone(&gameplay_charts[chart_index]);
    let chart = Arc::clone(&charts[chart_index]);
    let joined = match layout {
        "p2" => [false, true],
        "versus" => [true, true],
        _ => [true, false],
    };
    let session = GameplaySession {
        play_style,
        player_side: side,
        joined_sides: joined,
        ..Default::default()
    };
    let profiles = [
        sample_profile(reverse, show_scorebox),
        sample_profile(reverse, show_scorebox),
    ];
    let scroll = profiles.each_ref().map(|p| p.scroll_speed);
    let theme_style = match layout {
        "versus" => profile::PlayStyle::Versus,
        "double" => profile::PlayStyle::Double,
        _ => profile::PlayStyle::Single,
    };
    let theme_side = if layout == "p2" {
        profile::PlayerSide::P2
    } else {
        profile::PlayerSide::P1
    };
    let mut init = GameplayInitView::default();
    init.runtime.play_style = theme_style;
    init.runtime.player_side = theme_side;
    init.runtime.joined = joined;
    init.runtime.policy.center_single_notefield = center_single_notefield;
    init.hud.play_style = theme_style;
    init.hud.player_side = theme_side;
    init.hud.p1.joined = joined[0];
    init.hud.p2.joined = joined[1];
    init.scores.scorebox_visible = joined;
    let scorebox_snapshot =
        deadsync_score::CachedPlayerLeaderboardData::ready(deadsync_score::PlayerLeaderboardData {
            panes: vec![deadsync_score::LeaderboardPane {
                name: "GrooveStats".into(),
                entries: [
                    ("1", 9998.0, false),
                    ("DeadSync", 9994.0, true),
                    ("StepStar", 9982.0, false),
                    ("Arrow", 9965.0, false),
                    ("Rhythm", 9942.0, false),
                ]
                .into_iter()
                .enumerate()
                .map(
                    |(index, (name, score, is_self))| deadsync_score::LeaderboardEntry {
                        rank: index as u32 + 1,
                        name: name.into(),
                        machine_tag: None,
                        score,
                        date: String::new(),
                        is_rival: false,
                        is_self,
                        is_fail: false,
                    },
                )
                .collect(),
                is_ex: true,
                disabled: false,
                personalized: true,
                arrowcloud_kind: None,
            }],
            srpg_self_score: None,
            itl_self_score: None,
            itl_self_rank: None,
        });
    init.scores.scorebox_snapshots = [Some(scorebox_snapshot.clone()), Some(scorebox_snapshot)];
    drop(setup_scope);
    let mut state = profiling.measure(ProfilePhase::StateInit, || {
        gameplay::init(
            song,
            [chart.clone(), chart],
            [payload.clone(), payload],
            GameplayViewport::new(width, height),
            session,
            deadsync_gameplay::GameplayConfig {
                center_1player_notefield: center_single_notefield,
                ..Default::default()
            },
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
            Default::default(),
            init,
        )
    });
    let data_scope = profiling.scope(ProfilePhase::StateData);
    state.boundary.total_elapsed_in_screen = 32.05;
    state.set_current_music_time_ns(32_050_000_000);
    for player in 0..state.num_players() {
        let runtime = &mut state.players_runtime.players[player];
        runtime.combo = 234 + player as u32 * 17;
        runtime.judgment_counts = [216, 12, 4, 1, 0, 1];
        runtime.scoring_counts = runtime.judgment_counts;
        runtime.earned_grade_points = 450;
        runtime.current_possible_points = 468;
        runtime.life = 0.87 - player as f32 * 0.08;
        runtime.calories_burned = 42.5;
        for sample in 0..234 {
            let offset = [-7.0, 19.0, 31.0, 58.0, 110.0, 150.0, 0.0][sample % 7];
            let (grade, window) = match sample % 7 {
                0 | 1 => (
                    deadsync_rules::judgment::JudgeGrade::Fantastic,
                    Some(deadsync_rules::judgment::TimingWindow::W1),
                ),
                2 => (
                    deadsync_rules::judgment::JudgeGrade::Excellent,
                    Some(deadsync_rules::judgment::TimingWindow::W2),
                ),
                3 => (
                    deadsync_rules::judgment::JudgeGrade::Great,
                    Some(deadsync_rules::judgment::TimingWindow::W3),
                ),
                4 => (
                    deadsync_rules::judgment::JudgeGrade::Decent,
                    Some(deadsync_rules::judgment::TimingWindow::W4),
                ),
                5 => (
                    deadsync_rules::judgment::JudgeGrade::WayOff,
                    Some(deadsync_rules::judgment::TimingWindow::W5),
                ),
                _ => (deadsync_rules::judgment::JudgeGrade::Miss, None),
            };
            let judgment = deadsync_rules::judgment::Judgment {
                time_error_ms: offset,
                time_error_music_ns: (offset * 1_000_000.0) as i64,
                grade,
                window,
                miss_because_held: false,
            };
            state
                .progress
                .window_counts
                .record_judgment(player, &judgment, 15.0);
            deadsync_rules::timing::record_live_timing_stats(
                &mut state.players_runtime.players[player].live_timing_stats,
                &judgment,
            );
        }
        for offset in [-12.0, 7.0, -3.0, 19.0, -7.0] {
            let judgment = deadsync_rules::judgment::Judgment {
                time_error_ms: offset,
                time_error_music_ns: (offset * 1_000_000.0) as i64,
                grade: deadsync_rules::judgment::JudgeGrade::Fantastic,
                window: Some(deadsync_rules::judgment::TimingWindow::W1),
                miss_because_held: false,
            };
            state.set_last_judgment(player, judgment);
            state.error_bar_register_tap(player, &judgment, 32.05);
            state.trigger_tap_judgment_explosion(player, 1, &judgment);
        }
    }
    gameplay::set_heart_rate_view(
        &mut state,
        (1, 1),
        gameplay::HeartRateView {
            players: std::array::from_fn(|p| gameplay::HeartRatePlayerView {
                configured: true,
                connected: true,
                bpm: Some(156 + p as u16 * 8),
                max_heart_rate: 190,
            }),
        },
    );
    drop(data_scope);
    Ok(state)
}

fn noteskin_texture_keys(state: &gameplay::State) -> Vec<(String, bool)> {
    let mut texture_keys = BTreeSet::new();
    for skin in [
        &state.noteskin_assets.noteskin,
        &state.noteskin_assets.mine_noteskin,
        &state.noteskin_assets.receptor_noteskin,
        &state.noteskin_assets.tap_explosion_noteskin,
    ]
    .into_iter()
    .flat_map(|set| set.iter().flatten())
    {
        skin.for_each_slot(|slot| {
            for slot in std::iter::once(slot).chain(slot.model_additive.as_deref()) {
                texture_keys.insert((slot.texture_key().to_owned(), slot.model.is_some()));
                for key in slot.model_texture_keys.iter() {
                    texture_keys.insert((key.to_string(), true));
                }
            }
        });
    }
    texture_keys.into_iter().collect()
}

fn prepare_capture_frame(
    state: &mut gameplay::State,
    producer: &mut export_support::CaptureProducer,
    metrics: &Metrics,
    center_single_notefield: bool,
) -> Result<RenderFrame> {
    let (missing, frame) = producer.with_assets(|assets| {
        let context = CaptureTextures::new(assets);
        let frame = compose(state, assets, metrics, &context, center_single_notefield);
        let missing = context.missing_keys();
        Ok((missing, frame))
    })?;
    if !missing.is_empty() {
        producer.load_textures(missing.into_iter().map(|key| (key, false)).collect())?;
        return producer.with_assets(|assets| {
            Ok(compose(
                state,
                assets,
                metrics,
                assets.texture_context(),
                center_single_notefield,
            ))
        });
    }
    Ok(frame)
}

fn compose(
    state: &mut gameplay::State,
    assets: &RenderAssets,
    metrics: &Metrics,
    textures: &impl TextureContext,
    center_single_notefield: bool,
) -> RenderFrame {
    let mut actors: Vec<Actor> = Vec::new();
    let mut view = gameplay::ActorViewOverride::default();
    view.notefield.force_center_1player = center_single_notefield;
    let segments = gameplay::compose_frame(
        state,
        &mut actors,
        FramePresentationContext {
            assets,
            view,
            uptime_seconds: 32.05,
            policy: Default::default(),
        },
        SongFrameContent::PLAYBACK,
    );
    compose::build_passes(
        segments.segments(state.song_frame(), &actors),
        state.render_targets(),
        [0.0, 0.0, 0.0, 1.0],
        metrics,
        assets.fonts(),
        32.05,
        &mut TextLayoutCache::default(),
        &mut ComposeScratch::default(),
        textures,
        Some(state.actor_resources()),
    )
}
