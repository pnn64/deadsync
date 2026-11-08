//! Export the Simply Love menu screens for the available display presets.
mod export_menus_viewer;
mod preview_content;
use crate::export_support;

use deadlib_assets::AssetManager;
use deadlib_present::{
    actors::Actor,
    compose,
    space::{self, Metrics},
};
use deadlib_render::{Backend, RenderFrame};
use deadsync_theme_simply_love::{self as theme, screens, views};
use export_support::{PRESETS, ScreenshotTimer, Size};
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
    let mut assets = AssetManager::new();
    export_support::load_initial_assets(&mut assets, &mut backend)?;
    let output = export_support::workspace_root()
        .join("target")
        .join("menu-exports");
    std::fs::create_dir_all(&output)?;
    if !options.images_only {
        export_menus_viewer::write_index(&output, &options.sizes)?;
    }
    for size in &options.sizes {
        export_resolution(
            &mut backend,
            &mut assets,
            size,
            &output,
            song.clone(),
            &preview,
        )?;
    }

    backend.dispose_textures(&mut assets.take_textures());
    backend.cleanup();
    println!(
        "Exported {} PNGs. Gallery: {}",
        options.sizes.len()
            * (MENUS.len() - 1 + PLAYER_VARIANT_MENUS.len() * PLAYER_VARIANTS.len()),
        output.join("index.html").display()
    );
    Ok(())
}

fn export_resolution(
    backend: &mut Backend,
    assets: &mut AssetManager,
    size: &Size,
    output: &Path,
    song: Arc<deadsync_chart::SongData>,
    preview: &MenuPreviewContent,
) -> Result<()> {
    let mut timer = ScreenshotTimer::new();
    let default_song_select = output.join(size.name()).join("song-select.png");
    if default_song_select.is_file() {
        std::fs::remove_file(default_song_select)?;
    }
    let metrics = Metrics::centered(480.0 * size.aspect.0 as f32 / size.aspect.1 as f32, 480.0);
    space::set_current_metrics(metrics);
    space::set_current_window_px(size.width, size.height);
    let resize_started = Instant::now();
    backend.resize(size.width, size.height);
    timer.record_resize(resize_started.elapsed());
    backend.set_default_projection(metrics.projection());
    let mut captures = export_support::CaptureBatch::default();
    for &(name, _) in MENUS {
        if name != "song-select" {
            export_menu(
                name,
                None,
                backend,
                assets,
                size,
                output,
                song.clone(),
                preview,
                metrics,
                &mut captures,
                &mut timer,
            )?;
        }
        if PLAYER_VARIANT_MENUS.contains(&name) {
            for variant in PLAYER_VARIANTS {
                export_menu(
                    name,
                    Some(*variant),
                    backend,
                    assets,
                    size,
                    output,
                    song.clone(),
                    preview,
                    metrics,
                    &mut captures,
                    &mut timer,
                )?;
            }
        }
    }
    timer.record_captures(captures.finish(backend, size)?);
    timer.report("menu", size, None);
    Ok(())
}

fn export_menu(
    name: &str,
    player_variant: Option<PlayerVariant>,
    backend: &mut Backend,
    assets: &mut AssetManager,
    size: &Size,
    output: &Path,
    song: Arc<deadsync_chart::SongData>,
    preview: &MenuPreviewContent,
    metrics: Metrics,
    captures: &mut export_support::CaptureBatch,
    timer: &mut ScreenshotTimer,
) -> Result<()> {
    let folder = output.join(size.name());
    std::fs::create_dir_all(&folder)?;
    let path = folder.join(format!(
        "{}{suffix}.png",
        name,
        suffix = player_variant.map_or("", |variant| variant.suffix)
    ));
    if name == "content-browser" && size.aspect != (16, 9) {
        return save_disabled_content_browser(size, &path, timer);
    }
    let prepare_started = Instant::now();
    let mut actors = menu_actors(name, player_variant, assets, song, preview)?;
    let frame = prepare_frame(&actors, backend, assets, metrics)?;
    captures.draw(backend, &frame, assets.textures(), path)?;
    actors.clear();
    timer.record_prepare(prepare_started.elapsed());
    Ok(())
}

fn prepare_frame(
    actors: &[Actor],
    backend: &mut Backend,
    assets: &mut AssetManager,
    metrics: Metrics,
) -> Result<RenderFrame> {
    let capture = export_support::CaptureTextures::new(assets);
    let _ = compose::build_screen_with_texture_context(
        actors,
        [0.0, 0.0, 0.0, 1.0],
        &metrics,
        assets.fonts(),
        CAPTURE_TIME,
        &capture,
    );
    for key in capture.missing_keys() {
        export_support::load_texture(assets, backend, &key, false)?;
    }
    Ok(compose::build_screen_with_texture_context(
        actors,
        [0.0, 0.0, 0.0, 1.0],
        &metrics,
        assets.fonts(),
        CAPTURE_TIME,
        assets.texture_context(),
    ))
}

fn save_disabled_content_browser(
    size: &Size,
    path: &Path,
    timer: &mut ScreenshotTimer,
) -> Result<()> {
    let prepare_started = Instant::now();
    let image = image::RgbaImage::from_pixel(size.width, size.height, image::Rgba([0, 0, 0, 255]));
    timer.record_prepare(prepare_started.elapsed());
    let write_started = Instant::now();
    image.save(path)?;
    timer.record_png_write(write_started.elapsed());
    println!("{} (disabled: requires 16:9)", path.display());
    Ok(())
}

fn menu_actors(
    name: &str,
    player_variant: Option<PlayerVariant>,
    assets: &AssetManager,
    song: Arc<deadsync_chart::SongData>,
    preview: &MenuPreviewContent,
) -> Result<Vec<Actor>> {
    match name {
        "title" => Ok(title_actors()),
        "profile-select" => Ok(profile_select_actors(assets, player_variant)),
        "style-select" => Ok(style_select_actors(player_variant)),
        "mode-select" => Ok(mode_select_actors(assets, player_variant)),
        "song-select" => Ok(preview.song_select_actors(
            assets,
            CAPTURE_TIME,
            player_variant.map(|variant| variant.joined),
        )),
        "course-select" => Ok(preview.course_select_actors(
            assets,
            CAPTURE_TIME,
            player_variant.map(|variant| variant.joined),
        )),
        "key-bindings" => Ok(key_bindings_actors(assets)),
        "options" => Ok(options_actors(assets)),
        "player-options" => Ok(player_options_actors(song, assets)),
        "content-browser" => Ok(content_browser_actors(assets)),
        _ => Err(format!("unknown menu {name}").into()),
    }
}

fn title_actors() -> Vec<Actor> {
    screens::menu::get_actors(&screens::menu::init(), None, 1.0)
}

fn profile_select_actors(
    assets: &AssetManager,
    player_variant: Option<PlayerVariant>,
) -> Vec<Actor> {
    let mut state = screens::select_profile::init(views::ProfilePickerView::default());
    if let Some(variant) = player_variant {
        screens::select_profile::set_joined(&mut state, variant.joined[0], variant.joined[1]);
    }
    screens::select_profile::get_actors(&state, assets, 1.0)
}

fn style_select_actors(player_variant: Option<PlayerVariant>) -> Vec<Actor> {
    let mut runtime = views::SelectFlowRuntimeView::default();
    if let Some(variant) = player_variant {
        runtime.players = preview_players(variant.joined);
    }
    screens::select_style::get_actors(&screens::select_style::init(runtime))
}

fn mode_select_actors(assets: &AssetManager, player_variant: Option<PlayerVariant>) -> Vec<Actor> {
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

fn key_bindings_actors(assets: &AssetManager) -> Vec<Actor> {
    let state = screens::mappings::init(views::MappingsRuntimeView::default());
    screens::mappings::get_actors(&state, assets, 1.0)
}

fn content_browser_actors(assets: &AssetManager) -> Vec<Actor> {
    screens::content_browser::get_actors(&screens::content_browser::init(), assets)
}

fn options_actors(assets: &AssetManager) -> Vec<Actor> {
    let mut graphics = deadsync_theme::views::GraphicsOptionsView::default();
    graphics.aspect_ratio = 16.0 / 9.0;
    let init = views::OptionsInitView {
        config: deadsync_config::app_config::Config::default(),
        judgment_palettes: deadsync_config::judgment_palettes::JudgmentPaletteCatalog::new(
            theme::color::JUDGMENT_PRESET,
        ),
        updater_capabilities: Default::default(),
        app_paths: Default::default(),
        audio: Default::default(),
        graphics,
        song_packs: Vec::new(),
        pack_sync: Default::default(),
        noteskins: Default::default(),
        machine_player_options: Default::default(),
        smx_assignment: Default::default(),
        smx_gifs: Default::default(),
        score_import_profiles: Vec::new(),
        bookkeeping: Default::default(),
    };
    screens::options::get_actors(&screens::options::init(init), assets, 1.0)
}

fn player_options_actors(song: Arc<deadsync_chart::SongData>, assets: &AssetManager) -> Vec<Actor> {
    let chart_index = song
        .charts
        .iter()
        .position(|chart| chart.chart_type.eq_ignore_ascii_case("dance-single"))
        .unwrap_or(0);
    let mut state = screens::player_options::init(
        song,
        [chart_index, chart_index],
        [2, 2],
        0,
        screens::SimplyLoveScreen::SelectMusic,
        None,
        Default::default(),
        Default::default(),
        Default::default(),
        views::PlayerOptionsInitView::default(),
    );
    screens::player_options::prepare_presentation(&mut state, assets);
    screens::player_options::get_actors(&state, assets)
}
