//! Export the real Simply Love gameplay presentation without changing player settings.
mod export_gameplay_viewer;

use deadlib_assets::AssetManager;
use deadlib_present::{
    actors::Actor,
    compose::{self, ComposeScratch, TextLayoutCache},
    space::{self, Metrics},
    texture::{TextureContext, TextureMeta},
};
use deadlib_render::{Backend, BackendType, PresentModePolicy, RenderFrame, TextureHandle};
use deadsync_gameplay::{
    GameplayInputPlayStyle, GameplayInputPlayerSide, GameplaySession, GameplayViewport,
};
use deadsync_profile as profile;
use deadsync_theme_simply_love::{self as theme, screens::gameplay, views::GameplayInitView};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    error::Error,
    path::{Path, PathBuf},
    sync::Arc,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const PRESETS: &[(&str, u32, u32)] = &[
    ("vga", 640, 480),
    ("svga", 800, 600),
    ("xga", 1024, 768),
    ("5-4", 1280, 1024),
    ("720p", 1280, 720),
    ("square", 640, 640),
    ("16-10", 1280, 800),
    ("ultrawide", 1280, 540),
    ("uwqhd", 1720, 720),
    ("portrait-9-16", 540, 960),
    ("portrait-10-16", 540, 960),
];
const LAYOUTS: &[&str] = &["p1", "p2", "versus", "double"];
const HELP: &str = "\
Export Simply Love gameplay PNGs (requires a desktop and Vulkan-capable GPU).

cargo test -p deadsync-theme-simply-love --example export_gameplay -- [OPTIONS]

  --preset NAME   Select a preset; repeat to select several (default: all)
  --reverse       Use reverse scrolling
  --help          Show this help

Available presets: vga, svga, xga, 5-4, square, 16-10, 720p, ultrawide,
uwqhd, portrait-9-16, portrait-10-16.";

#[derive(Clone, Debug, PartialEq)]
struct Size {
    width: u32,
    height: u32,
    aspect: (u32, u32),
}

impl Size {
    fn from_preset(width: u32, height: u32) -> Self {
        let divisor = gcd(width, height);
        Self {
            width,
            height,
            aspect: (width / divisor, height / divisor),
        }
    }

    fn name(&self) -> String {
        format!(
            "{}x{}-{}x{}",
            self.width, self.height, self.aspect.0, self.aspect.1
        )
    }
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

struct Options {
    sizes: Vec<Size>,
    reverse: bool,
}

fn options(args: impl IntoIterator<Item = String>) -> Result<Option<Options>> {
    let mut args = args.into_iter();
    let mut sizes = Vec::new();
    let mut reverse = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{HELP}");
                return Ok(None);
            }
            "--reverse" => reverse = true,
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
    if sizes.is_empty() {
        for &(_, width, height) in PRESETS {
            let size = Size::from_preset(width, height);
            if !sizes.contains(&size) {
                sizes.push(size);
            }
        }
    }
    Ok(Some(Options { sizes, reverse }))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("Simply Love crate is under the workspace crates directory")
        .to_path_buf()
}

struct Logger;
impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{}: {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

fn main() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        return std::thread::Builder::new()
            .name("gameplay-export".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(|| run().map_err(|error| error.to_string()))?
            .join()
            .map_err(|_| "gameplay export thread panicked")?
            .map_err(Into::into);
    }
    #[cfg(not(target_os = "windows"))]
    run()
}

fn run() -> Result<()> {
    let Some(options) = options(std::env::args().skip(1))? else {
        return Ok(());
    };
    log::set_logger(&Logger).map_err(|e| e.to_string())?;
    log::set_max_level(log::LevelFilter::Warn);
    let isolated = tempfile::tempdir()?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let dirs = deadsync_config::dirs::AppDirs {
        data_dir: isolated.path().to_path_buf(),
        cache_dir: isolated.path().join("cache"),
        exe_dir: root.to_path_buf(),
        portable: false,
    };
    dirs.ensure_dirs_exist();
    deadsync_assets::init_paths(dirs.asset_paths(None))?;
    theme::i18n::init_for_tests();
    deadsync_config::runtime::init_paths(dirs.config_path(), dirs.judgment_palettes_path())?;
    deadsync_profile::app_runtime::init_paths(
        dirs.profiles_root(),
        dirs.default_player_options_path(),
        dirs.pad_config_path(),
    )?;
    deadsync_simfile::app_runtime::init_paths(deadsync_simfile::app_runtime::ScanPaths {
        song_cache: dirs.song_cache_dir(),
        extra_songs: Vec::new(),
        extra_courses: Vec::new(),
        autogen_courses: dirs.courses_dir(),
        song_movies: Vec::new(),
        random_movies: Vec::new(),
        bg_animations: Vec::new(),
    })?;
    let (simfile, pack_directory, pack_name) = sample_song(isolated.path())?;
    let song = Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(
        &simfile, 0.0,
    )?);
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
    export(&options, song, window)
}

fn export(
    options: &Options,
    song: Arc<deadsync_chart::SongData>,
    window: Arc<winit::window::Window>,
) -> Result<()> {
    let mut backend = deadlib_render::create_backend(
        BackendType::VulkanWgpu,
        window,
        Metrics::centered(640.0, 480.0).projection(),
        false,
        PresentModePolicy::Immediate,
        false,
        true,
    )?;
    let mut assets = AssetManager::new();
    deadsync_assets::load_initial_assets(&mut assets, &mut backend, theme::asset_manifest())?;
    let output = workspace_root().join("target").join("gameplay-exports");
    std::fs::create_dir_all(&output)?;
    export_gameplay_viewer::write_index(&output, &options.sizes, options.reverse)?;
    for size in &options.sizes {
        let logical_width = 480.0 * size.aspect.0 as f32 / size.aspect.1 as f32;
        let logical_height = 480.0;
        let metrics = Metrics::centered(logical_width, logical_height);
        space::set_current_metrics(metrics);
        space::set_current_window_px(size.width, size.height);
        backend.resize(size.width, size.height);
        backend.set_default_projection(metrics.projection());
        for &layout in LAYOUTS {
            let mut variants = vec![(layout.to_owned(), false)];
            if matches!(layout, "p1" | "p2") {
                variants.push((format!("{layout}-centered"), true));
            }
            for (export_layout, center_single_notefield) in variants {
                let folder = output.join(size.name()).join(&export_layout);
                std::fs::create_dir_all(&folder)?;
                for (show_scorebox, stem) in [(true, "gameplay"), (false, "gameplay-hmr")] {
                    let mut state = sample_state(
                        song.clone(),
                        logical_width,
                        logical_height,
                        layout,
                        options.reverse,
                        show_scorebox,
                        center_single_notefield,
                    )?;
                    preload_noteskins(&state, &mut assets, &mut backend)?;
                    let context = CaptureTextures {
                        assets: &assets,
                        keys: RefCell::default(),
                    };
                    drop(compose(
                        &mut state,
                        &assets,
                        &metrics,
                        &context,
                        center_single_notefield,
                    ));
                    let missing: Vec<_> = context
                        .keys
                        .into_inner()
                        .into_iter()
                        .filter(|key| !assets.has_uploaded_texture_key(key))
                        .collect();
                    for key in missing {
                        load_texture(&mut assets, &mut backend, &key, false)?;
                    }
                    let frame = compose(
                        &mut state,
                        &assets,
                        &metrics,
                        assets.texture_context(),
                        center_single_notefield,
                    );
                    backend.request_screenshot();
                    backend.draw(&frame, assets.textures(), false)?;
                    let image = backend.capture_frame()?;
                    if image.dimensions() != (size.width, size.height) {
                        return Err(format!(
                            "capture returned {:?}, expected {}x{}",
                            image.dimensions(),
                            size.width,
                            size.height
                        )
                        .into());
                    }
                    let filename = format!(
                        "{stem}{}.png",
                        if options.reverse { "-reverse" } else { "" }
                    );
                    let output = folder.join(&filename);
                    image.save(&output)?;
                    println!("{}", output.display());
                }
            }
        }
    }
    backend.dispose_textures(&mut assets.take_textures());
    backend.cleanup();
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
    width: f32,
    height: f32,
    layout: &str,
    reverse: bool,
    show_scorebox: bool,
    center_single_notefield: bool,
) -> Result<gameplay::State> {
    let (play_style, side) = match layout {
        "p1" => (GameplayInputPlayStyle::Single, GameplayInputPlayerSide::P1),
        "p2" => (GameplayInputPlayStyle::Single, GameplayInputPlayerSide::P2),
        "versus" => (GameplayInputPlayStyle::Versus, GameplayInputPlayerSide::P1),
        "double" => (GameplayInputPlayStyle::Double, GameplayInputPlayerSide::P1),
        _ => return Err(format!("invalid layout {layout}").into()),
    };
    let chart_index = usize::from(play_style == GameplayInputPlayStyle::Double);
    let payload = Arc::new(
        deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[chart_index], 0.0)?.remove(0),
    );
    let chart = Arc::new(song.charts[chart_index].clone());
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
    let scorebox_profile = deadsync_score::GameplayScoreboxProfileSnapshot::new(
        true,
        true,
        true,
        String::new(),
        String::new(),
        false,
        "DeadSync".into(),
        None,
        None,
        false,
    );
    init.scores.scorebox_profiles = [scorebox_profile.clone(), scorebox_profile];
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
    let mut state = gameplay::init(
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
    );
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
    Ok(state)
}

fn load_texture(
    assets: &mut AssetManager,
    backend: &mut Backend,
    key: &str,
    model: bool,
) -> Result<()> {
    if assets.has_uploaded_texture_key(key) || assets.load_generated_texture(backend, key, None)? {
        return Ok(());
    }
    assets
        .load_texture(
            backend,
            &deadsync_assets::textures::texture_decode_job(key, model),
        )
        .map_err(|e| format!("cannot load capture texture '{key}': {e}"))?;
    Ok(())
}

fn preload_noteskins(
    state: &gameplay::State,
    assets: &mut AssetManager,
    backend: &mut Backend,
) -> Result<()> {
    let mut keys = BTreeSet::new();
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
                keys.insert((slot.texture_key().to_owned(), slot.model.is_some()));
                for key in slot.model_texture_keys.iter() {
                    keys.insert((key.to_string(), true));
                }
            }
        });
    }
    for (key, model) in keys {
        load_texture(assets, backend, &key, model)?;
    }
    Ok(())
}

struct CaptureTextures<'a> {
    assets: &'a AssetManager,
    keys: RefCell<BTreeSet<String>>,
}
impl TextureContext for CaptureTextures<'_> {
    fn texture_registry_generation(&self) -> u64 {
        self.assets.texture_context().revision()
    }
    fn texture_dims(&self, key: &str) -> Option<TextureMeta> {
        self.assets.texture_context().texture_dims(key)
    }
    fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
        self.assets.texture_context().sprite_sheet_dims(key)
    }
    fn texture_handle(&self, key: &str) -> TextureHandle {
        self.keys.borrow_mut().insert(key.to_owned());
        self.assets.texture_context().texture_handle(key)
    }
}

fn compose(
    state: &mut gameplay::State,
    assets: &AssetManager,
    metrics: &Metrics,
    textures: &impl TextureContext,
    center_single_notefield: bool,
) -> RenderFrame {
    let mut actors: Vec<Actor> = Vec::new();
    let mut song = state.song_scratch.take().expect("song scratch");
    let mut hud = state.frame_scratch.take().expect("HUD scratch");
    let mut view = gameplay::ActorViewOverride::default();
    view.notefield.force_center_1player = center_single_notefield;
    let visual = Default::default();
    let segments = deadsync_song_lua::playback::compose_frame(
        &mut actors,
        &state.gameplay,
        &state.song_media,
        &mut song,
        assets,
        deadsync_song_lua::playback::FrameOptions {
            play_style: state.gameplay.setup.session.play_style,
            player_side: state.gameplay.setup.session.player_side,
            hide_song_bg: [false; 2],
            notefield: view.notefield,
            hide_gameplay_hud: false,
            apply_attacks: true,
            show_song_visuals: true,
        },
        gameplay::frame_layers(state, &mut hud, assets, view, visual),
        |request, cameras, field, flat, hud, hud_flat| {
            gameplay::draw_field(
                state, assets, view, 32.05, visual, request, cameras, field, flat, hud, hud_flat,
            )
        },
    );
    state.song_scratch = Some(song);
    state.frame_scratch = Some(hud);
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
