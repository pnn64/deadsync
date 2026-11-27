//! Export the Simply Love menu screens for the available display presets.
mod export_menus_viewer;
mod preview_content;
mod screenshot_options;
use crate::export_support;

use deadlib_present::{
    actors::Actor,
    compose,
    space::{self, Metrics},
};
use deadlib_render::RenderFrame;
use deadsync_theme_simply_love::{self as theme, screens, views};
use export_support::{PRESETS, RenderAssets, ScreenshotTimer, Size};
use preview_content::MenuPreviewContent;
use std::{path::Path, sync::Arc, time::Instant};

const MENUS: &[(&str, &str)] = &[
    ("title", "Title Menu"),
    ("profile-select", "Profile Select"),
    ("style-select", "Style Select"),
    ("mode-select", "Play Mode Select"),
    ("song-select", "Song Select"),
    ("course-select", "Course Select"),
    ("key-bindings", "Key Bindings"),
    ("options", "Options"),
    ("player-options", "Player Options"),
    ("content-browser", "Content Browser"),
];
const PLAYER_VARIANT_MENUS: &[&str] = &["song-select"];
const PLAYER_OPTIONS_PANES: &[(&str, &str, screens::player_options::OptionsPane)] = &[
    ("main", "Main", screens::player_options::OptionsPane::Main),
    (
        "display",
        "Display",
        screens::player_options::OptionsPane::Display,
    ),
    (
        "advanced",
        "Advanced",
        screens::player_options::OptionsPane::Advanced,
    ),
    (
        "uncommon",
        "Uncommon",
        screens::player_options::OptionsPane::Uncommon,
    ),
];
const PLAYER_VARIANTS: &[PlayerVariant] = &[
    PlayerVariant {
        suffix: "-1p",
        label: "1P",
        joined: [true, false],
    },
    PlayerVariant {
        suffix: "-2p",
        label: "2P",
        joined: [false, true],
    },
    PlayerVariant {
        suffix: "-1p-2p",
        label: "1P + 2P",
        joined: [true, true],
    },
];
const PLAYER_OPTIONS_VARIANTS: &[(&str, &str, [bool; 2])] = &[
    ("p1-solo", "P1 Solo", [true, false]),
    ("p2-solo", "P2 Solo", [false, true]),
    ("2p", "2P", [true, true]),
];
const CAPTURE_TIME: f32 = 2.0;

#[derive(Clone, Copy)]
struct PlayerVariant {
    suffix: &'static str,
    label: &'static str,
    joined: [bool; 2],
}

const HELP: &str = "\
Export Simply Love menu PNGs (requires a desktop and Vulkan-capable GPU).

cargo run -p deadsync-theme-simply-love --example export_screenshot -- menus [OPTIONS]

  --preset NAME   Select a preset; repeat to select several (default: all)
    --images-only   Export images without writing the gallery index
    --index-only    Write the gallery index without exporting images
  --help          Show this help

Available presets: vga, svga, xga, 5-4, square, 16-10, 720p, ultrawide,
uwqhd, portrait-9-16, portrait-10-16.";

struct Options {
    sizes: Vec<Size>,
    images_only: bool,
    index_only: bool,
}

fn options(args: impl IntoIterator<Item = String>) -> Result<Option<Options>> {
    let mut args = args.into_iter();
    let mut sizes = Vec::new();
    let mut images_only = false;
    let mut index_only = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{HELP}");
                return Ok(None);
            }
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
        images_only,
        index_only,
    }))
}

pub(super) type Result<T> = export_support::Result<T>;

pub(super) fn run(args: Vec<String>) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        return std::thread::Builder::new()
            .name("menus-export".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || run_with_args(args).map_err(|error| error.to_string()))?
            .join()
            .map_err(|_| "menu export thread panicked")?
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
        .join("menu-exports");
    if options.index_only {
        std::fs::create_dir_all(&output)?;
        export_menus_viewer::write_index(&output, &options.sizes)?;
        return Ok(());
    }
    let started = Instant::now();
    export_support::init_logger()?;
    let isolated = tempfile::tempdir()?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let dirs = deadsync_config::dirs::AppDirs {
        data_dir: isolated.path().to_path_buf(),
        cache_dir: isolated.path().join("cache"),
        exe_dir: root,
        portable: false,
    };
    export_support::initialize_runtime(&dirs)?;
    theme::i18n::init_for_tests();
    let preview = MenuPreviewContent::create(isolated.path())?;
    let song = preview.first_song();

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
                .with_title("Simply Love menu exporter")
                .with_visible(false)
                .with_inner_size(winit::dpi::PhysicalSize::new(640, 480)),
        )?,
    );
    export(&options, song, preview, window)?;
    println!(
        "Menus export completed in {:.1}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn export(
    options: &Options,
    song: Arc<deadsync_chart::SongData>,
    preview: MenuPreviewContent,
    window: Arc<winit::window::Window>,
) -> Result<()> {
    let mut backend = export_support::create_backend(window)?;
    let mut assets = RenderAssets::new();
    export_support::load_initial_assets(&mut assets, &mut backend)?;
    let mut pipeline = export_support::CapturePipeline::new(backend, assets);
    let preview = Arc::new(preview);
    let output = export_support::workspace_root()
        .join("target")
        .join("menu-exports");
    std::fs::create_dir_all(&output)?;
    if !options.images_only {
        export_menus_viewer::write_index(&output, &options.sizes)?;
    }
    let mut reports = Vec::with_capacity(options.sizes.len());
    for size in &options.sizes {
        reports.push(export_resolution(
            &mut pipeline,
            size,
            &output,
            song.clone(),
            &preview,
        )?);
    }

    pipeline.finish()?;
    for (timer, size) in reports {
        timer.report("menu", &size, None)?;
    }
    println!(
        "Exported {} PNGs. Gallery: {}",
        options.sizes.len()
            * (MENUS.len() - 3
                + PLAYER_VARIANT_MENUS.len() * (PLAYER_VARIANTS.len() + 2)
                + PLAYER_OPTIONS_VARIANTS.len() * PLAYER_OPTIONS_PANES.len()
                + screenshot_options::PAGES.len()),
        output.join("index.html").display()
    );
    Ok(())
}

fn export_resolution(
    pipeline: &mut export_support::CapturePipeline,
    size: &Size,
    output: &Path,
    song: Arc<deadsync_chart::SongData>,
    preview: &Arc<MenuPreviewContent>,
) -> Result<(ScreenshotTimer, Size)> {
    let mut timer = ScreenshotTimer::new();
    let default_song_select = output.join(size.name()).join("song-select.png");
    if default_song_select.is_file() {
        std::fs::remove_file(default_song_select)?;
    }
    let metrics = Metrics::centered(480.0 * size.aspect.0 as f32 / size.aspect.1 as f32, 480.0);
    timer.record_resize(pipeline.begin_resolution(size, metrics.projection())?);
    let report_size = size.clone();
    let size = size.clone();
    let output = output.to_path_buf();
    let preview = Arc::clone(preview);
    pipeline.run_producer(&mut timer, move |producer| {
        space::set_current_metrics(metrics);
        space::set_current_window_px(size.width, size.height);
        for &(name, _) in MENUS {
            if name != "song-select" && name != "options" && name != "player-options" {
                export_menu(
                    name,
                    None,
                    producer,
                    &size,
                    &output,
                    song.clone(),
                    &preview,
                    metrics,
                    false,
                )?;
            }
            if PLAYER_VARIANT_MENUS.contains(&name) {
                for variant in PLAYER_VARIANTS {
                    export_menu(
                        name,
                        Some(*variant),
                        producer,
                        &size,
                        &output,
                        song.clone(),
                        &preview,
                        metrics,
                        false,
                    )?;
                }
                for variant in &PLAYER_VARIANTS[..2] {
                    export_menu(
                        name,
                        Some(*variant),
                        producer,
                        &size,
                        &output,
                        song.clone(),
                        &preview,
                        metrics,
                        true,
                    )?;
                }
            }
        }
        export_player_options(producer, &size, &output, song, metrics)?;
        screenshot_options::export_resolution(producer, &size, &output, metrics)
    })?;
    Ok((timer, report_size))
}

fn export_player_options(
    producer: &mut export_support::CaptureProducer,
    size: &Size,
    output: &Path,
    song: Arc<deadsync_chart::SongData>,
    metrics: Metrics,
) -> Result<()> {
    let folder = output.join(size.name());
    std::fs::create_dir_all(&folder)?;
    for &(suffix, _, joined) in PLAYER_OPTIONS_VARIANTS {
        let mut state = player_options_state(song.clone(), joined);
        for &(name, _, pane) in PLAYER_OPTIONS_PANES {
            state.current_pane = pane;
            producer.with_assets(|assets| {
                screens::player_options::prepare_presentation(&mut state, assets);
                Ok(())
            })?;
            let prepare_started = Instant::now();
            let actors = producer
                .with_assets(|assets| Ok(screens::player_options::get_actors(&state, assets)))?;
            let frame = prepare_frame(&actors, producer, metrics)?;
            producer.submit(
                frame,
                folder.join(format!("player-options-{suffix}-{name}.png")),
                prepare_started.elapsed(),
            )?;
        }
    }
    Ok(())
}

fn export_menu(
    name: &str,
    player_variant: Option<PlayerVariant>,
    producer: &mut export_support::CaptureProducer,
    size: &Size,
    output: &Path,
    song: Arc<deadsync_chart::SongData>,
    preview: &MenuPreviewContent,
    metrics: Metrics,
    expanded_step_artist: bool,
) -> Result<()> {
    let folder = output.join(size.name());
    std::fs::create_dir_all(&folder)?;
    let suffix = if expanded_step_artist {
        format!(
            "{}-expanded",
            player_variant.map_or("", |variant| variant.suffix)
        )
    } else {
        player_variant.map_or(String::new(), |variant| variant.suffix.to_owned())
    };
    let path = folder.join(format!("{name}{suffix}.png"));
    if name == "content-browser" && size.aspect != (16, 9) {
        return save_disabled_content_browser(size, &path, producer);
    }
    let prepare_started = Instant::now();
    let actors = producer.with_assets(|assets| {
        menu_actors(
            name,
            player_variant,
            expanded_step_artist,
            assets,
            song,
            preview,
        )
    })?;
    let frame = prepare_frame(&actors, producer, metrics)?;
    producer.submit(frame, path, prepare_started.elapsed())?;
    Ok(())
}

fn prepare_frame(
    actors: &[Actor],
    producer: &mut export_support::CaptureProducer,
    metrics: Metrics,
) -> Result<RenderFrame> {
    let (missing, frame) = producer.with_assets(|assets| {
        let capture = export_support::CaptureTextures::new(assets);
        let frame = compose::build_screen_with_texture_context(
            actors,
            [0.0, 0.0, 0.0, 1.0],
            &metrics,
            assets.fonts(),
            CAPTURE_TIME,
            &capture,
        );
        let missing = capture.missing_keys();
        Ok((missing, frame))
    })?;
    if !missing.is_empty() {
        producer.load_textures(missing.into_iter().map(|key| (key, false)).collect())?;
        return producer.with_assets(|assets| {
            Ok(compose::build_screen_with_texture_context(
                actors,
                [0.0, 0.0, 0.0, 1.0],
                &metrics,
                assets.fonts(),
                CAPTURE_TIME,
                assets.texture_context(),
            ))
        });
    }
    Ok(frame)
}

fn save_disabled_content_browser(
    size: &Size,
    path: &Path,
    producer: &mut export_support::CaptureProducer,
) -> Result<()> {
    let prepare_started = Instant::now();
    let image = image::RgbaImage::from_pixel(size.width, size.height, image::Rgba([0, 0, 0, 255]));
    producer.write_png(path.to_path_buf(), image, prepare_started.elapsed())?;
    println!("{} (disabled: requires 16:9)", path.display());
    Ok(())
}

fn menu_actors(
    name: &str,
    player_variant: Option<PlayerVariant>,
    expanded_step_artist: bool,
    assets: &RenderAssets,
    song: Arc<deadsync_chart::SongData>,
    preview: &MenuPreviewContent,
) -> Result<Vec<Actor>> {
    match name {
        "title" => Ok(title_actors()),
        "profile-select" => Ok(profile_select_actors(assets)),
        "style-select" => Ok(style_select_actors(player_variant)),
        "mode-select" => Ok(mode_select_actors(assets, player_variant)),
        "song-select" => Ok(preview.song_select_actors(
            assets,
            CAPTURE_TIME,
            player_variant.map(|variant| variant.joined),
            expanded_step_artist,
        )),
        "course-select" => Ok(preview.course_select_actors(
            assets,
            CAPTURE_TIME,
            player_variant.map(|variant| variant.joined),
        )),
        "key-bindings" => Ok(key_bindings_actors(assets)),
        "player-options" => Ok(player_options_actors(song, assets)),
        "content-browser" => Ok(content_browser_actors(assets)),
        _ => Err(format!("unknown menu {name}").into()),
    }
}

fn title_actors() -> Vec<Actor> {
    screens::menu::get_actors(&screens::menu::init(), None, 1.0)
}

fn profile_select_actors(assets: &RenderAssets) -> Vec<Actor> {
    let mut view = views::ProfilePickerView::default();
    let mut preview_profile = view.guest.clone();
    preview_profile.id = "menu-preview".to_owned();
    preview_profile.display_name = "Dummy Profile".to_owned();
    preview_profile.total_songs_played = 42;
    view.profiles.push(preview_profile);
    view.default_profiles = [
        deadsync_profile::ActiveProfile::Local {
            id: "menu-preview".to_owned(),
        },
        deadsync_profile::ActiveProfile::Guest,
    ];
    let mut state = screens::select_profile::init(view);
    screens::select_profile::set_joined(&mut state, true, true);
    let _ = screens::select_profile::get_actors(&state, assets, 1.0);
    deadlib_present::runtime::tick(CAPTURE_TIME);
    screens::select_profile::get_actors(&state, assets, 1.0)
}

fn style_select_actors(player_variant: Option<PlayerVariant>) -> Vec<Actor> {
    let mut runtime = views::SelectFlowRuntimeView::default();
    if let Some(variant) = player_variant {
        runtime.players = preview_players(variant.joined);
    }
    screens::select_style::get_actors(&screens::select_style::init(runtime))
}

fn mode_select_actors(assets: &RenderAssets, player_variant: Option<PlayerVariant>) -> Vec<Actor> {
    let mut runtime = views::SelectFlowRuntimeView::default();
    if let Some(variant) = player_variant {
        runtime.players = preview_players(variant.joined);
    }
    let state = screens::select_mode::init(runtime);
    screens::select_mode::get_actors(&state, assets)
}

fn preview_players(joined: [bool; 2]) -> [views::SelectFlowPlayerView; 2] {
    std::array::from_fn(|index| views::SelectFlowPlayerView {
        joined: joined[index],
        guest: true,
        display_name: format!("Menu Preview P{}", index + 1),
        ..Default::default()
    })
}

fn key_bindings_actors(assets: &RenderAssets) -> Vec<Actor> {
    let state = screens::mappings::init(views::MappingsRuntimeView::default());
    screens::mappings::get_actors(&state, assets, 1.0)
}

fn content_browser_actors(assets: &RenderAssets) -> Vec<Actor> {
    screens::content_browser::get_actors(&screens::content_browser::init(), assets)
}

fn player_options_actors(song: Arc<deadsync_chart::SongData>, assets: &RenderAssets) -> Vec<Actor> {
    let mut state = player_options_state(song, [true, true]);
    screens::player_options::prepare_presentation(&mut state, assets);
    screens::player_options::get_actors(&state, assets)
}

fn player_options_state(
    song: Arc<deadsync_chart::SongData>,
    joined: [bool; 2],
) -> screens::player_options::State {
    let chart_index = song
        .charts
        .iter()
        .position(|chart| chart.chart_type.eq_ignore_ascii_case("dance-single"))
        .unwrap_or(0);
    let init_view = views::PlayerOptionsInitView {
        play_style: if joined == [true, true] {
            deadsync_profile::PlayStyle::Versus
        } else {
            deadsync_profile::PlayStyle::Single
        },
        player_side: if joined == [false, true] {
            deadsync_profile::PlayerSide::P2
        } else {
            deadsync_profile::PlayerSide::P1
        },
        joined,
        ..Default::default()
    };
    let state = screens::player_options::init(
        song,
        [chart_index, chart_index],
        [2, 2],
        0,
        deadsync_theme::Screen::SelectMusic,
        None,
        Default::default(),
        Default::default(),
        Default::default(),
        init_view,
    );
    state
}
