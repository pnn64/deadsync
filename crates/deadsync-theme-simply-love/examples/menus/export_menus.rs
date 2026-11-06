//! Export the Simply Love menu screens for the available display presets.
mod export_menus_viewer;
#[path = "../export_support.rs"]
mod export_support;

use deadlib_assets::AssetManager;
use deadlib_present::{
    actors::Actor,
    compose,
    space::{self, Metrics},
};
use deadlib_render::{Backend, RenderFrame};
use deadsync_theme_simply_love::{self as theme, screens, views};
use export_support::{PRESETS, Size};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

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
const CAPTURE_TIME: f32 = 2.0;

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
    let song_pack = sample_song(isolated.path())?;
    let song = song_pack.songs[0].clone();

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
    export(&options, song, song_pack, window)?;
    println!(
        "Menus export completed in {:.1}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn export(
    options: &Options,
    song: Arc<deadsync_chart::SongData>,
    song_pack: deadsync_chart::SongPack,
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
        let resolution_started = Instant::now();
        let mut resize_elapsed = Duration::ZERO;
        let mut prepare_elapsed = Duration::ZERO;
        let mut render_readback_elapsed = Duration::ZERO;
        let mut png_write_elapsed = Duration::ZERO;
        let logical_width = 480.0 * size.aspect.0 as f32 / size.aspect.1 as f32;
        let metrics = Metrics::centered(logical_width, 480.0);
        space::set_current_metrics(metrics);
        space::set_current_window_px(size.width, size.height);
        let resize_started = Instant::now();
        backend.resize(size.width, size.height);
        resize_elapsed += resize_started.elapsed();
        backend.set_default_projection(metrics.projection());
        let mut pending_capture: Option<PathBuf> = None;

        for &(name, _) in MENUS {
            let folder = output.join(size.name());
            std::fs::create_dir_all(&folder)?;
            let path = folder.join(format!("{name}.png"));
            if name == "content-browser" && size.aspect != (16, 9) {
                let prepare_started = Instant::now();
                let image = image::RgbaImage::from_pixel(
                    size.width,
                    size.height,
                    image::Rgba([0, 0, 0, 255]),
                );
                prepare_elapsed += prepare_started.elapsed();
                let save_started = Instant::now();
                image.save(&path)?;
                png_write_elapsed += save_started.elapsed();
                println!("{} (disabled: requires 16:9)", path.display());
                continue;
            }

            let prepare_started = Instant::now();
            let mut actors = menu_actors(name, &assets, song.clone(), song_pack.clone())?;
            let capture = export_support::CaptureTextures::new(&assets);
            let _ = compose::build_screen_with_texture_context(
                &actors,
                [0.0, 0.0, 0.0, 1.0],
                &metrics,
                assets.fonts(),
                CAPTURE_TIME,
                &capture,
            );
            for key in capture.missing_keys() {
                export_support::load_texture(&mut assets, &mut backend, &key, false)?;
            }
            let frame = compose::build_screen_with_texture_context(
                &actors,
                [0.0, 0.0, 0.0, 1.0],
                &metrics,
                assets.fonts(),
                CAPTURE_TIME,
                assets.texture_context(),
            );
            prepare_elapsed += prepare_started.elapsed();
            let (render_readback, png_write) = draw_capture(
                &mut backend,
                &assets,
                frame,
                size,
                &path,
                &mut pending_capture,
            )?;
            render_readback_elapsed += render_readback;
            png_write_elapsed += png_write;
            actors.clear();
        }
        if let Some(path) = pending_capture.take() {
            let (readback_elapsed, write_elapsed) = save_capture(&mut backend, size, &path)?;
            render_readback_elapsed += readback_elapsed;
            png_write_elapsed += write_elapsed;
            println!("{}", path.display());
        }
        eprintln!(
            "menu resolution={} elapsed={:.2}s resize={:.2}s prepare={:.2}s render_readback={:.2}s png_write={:.2}s other={:.2}s",
            size.name(),
            resolution_started.elapsed().as_secs_f64(),
            resize_elapsed.as_secs_f64(),
            prepare_elapsed.as_secs_f64(),
            render_readback_elapsed.as_secs_f64(),
            png_write_elapsed.as_secs_f64(),
            resolution_started
                .elapsed()
                .saturating_sub(
                    resize_elapsed + prepare_elapsed + render_readback_elapsed + png_write_elapsed,
                )
                .as_secs_f64(),
        );
    }

    backend.dispose_textures(&mut assets.take_textures());
    backend.cleanup();
    println!(
        "Exported {} PNGs. Gallery: {}",
        options.sizes.len() * MENUS.len(),
        output.join("index.html").display()
    );
    Ok(())
}

fn menu_actors(
    name: &str,
    assets: &AssetManager,
    song: Arc<deadsync_chart::SongData>,
    song_pack: deadsync_chart::SongPack,
) -> Result<Vec<Actor>> {
    let actors = match name {
        "title" => screens::menu::get_actors(&screens::menu::init(), None, 1.0),
        "profile-select" => screens::select_profile::get_actors(
            &screens::select_profile::init(views::ProfilePickerView::default()),
            assets,
            1.0,
        ),
        "style-select" => screens::select_style::get_actors(&screens::select_style::init(
            views::SelectFlowRuntimeView::default(),
        )),
        "mode-select" => screens::select_mode::get_actors(
            &screens::select_mode::init(views::SelectFlowRuntimeView::default()),
            assets,
        ),
        "song-select" => {
            let mut init = views::SelectMusicInitView::default();
            init.song_packs.push(song_pack);
            let mut state = screens::select_music::init(init);
            state.selection_animation_timer = CAPTURE_TIME;
            screens::select_music::get_actors(&state, assets, 1)
        }
        "course-select" => screens::select_course::get_actors(
            &screens::select_course::init(views::SelectCourseInitView::default()),
            assets,
        ),
        "key-bindings" => screens::mappings::get_actors(
            &screens::mappings::init(views::MappingsRuntimeView::default()),
            assets,
            1.0,
        ),
        "options" => {
            let config = deadsync_config::app_config::Config::default();
            let mut graphics = deadsync_theme::views::GraphicsOptionsView::default();
            graphics.aspect_ratio = 16.0 / 9.0;
            screens::options::get_actors(
                &screens::options::init(views::OptionsInitView {
                    config,
                    judgment_palettes:
                        deadsync_config::judgment_palettes::JudgmentPaletteCatalog::new(
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
                }),
                assets,
                1.0,
            )
        }
        "player-options" => {
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
        "content-browser" => {
            screens::content_browser::get_actors(&screens::content_browser::init(), assets)
        }
        _ => return Err(format!("unknown menu {name}").into()),
    };
    Ok(actors)
}

fn draw_capture(
    backend: &mut Backend,
    assets: &AssetManager,
    frame: RenderFrame,
    size: &Size,
    path: &PathBuf,
    pending_capture: &mut Option<PathBuf>,
) -> Result<(Duration, Duration)> {
    backend.request_screenshot();
    let render_started = Instant::now();
    backend.draw(&frame, assets.textures(), false)?;
    let render_readback = render_started.elapsed();
    let mut png_write = Duration::ZERO;
    if let Some(previous_path) = pending_capture.replace(path.clone()) {
        let (readback_elapsed, write_elapsed) = save_capture(backend, size, &previous_path)?;
        let render_readback = render_readback + readback_elapsed;
        png_write += write_elapsed;
        println!("{}", previous_path.display());
        return Ok((render_readback, png_write));
    }
    Ok((render_readback, png_write))
}

fn save_capture(backend: &mut Backend, size: &Size, path: &Path) -> Result<(Duration, Duration)> {
    let readback_started = Instant::now();
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
    let readback_elapsed = readback_started.elapsed();
    let save_started = Instant::now();
    image.save(path)?;
    Ok((readback_elapsed, save_started.elapsed()))
}

fn sample_song(directory: &Path) -> Result<deadsync_chart::SongPack> {
    let pack_name = "Simply Love Menu Preview".to_owned();
    let pack_directory = directory.join(&pack_name);
    let mut songs = Vec::with_capacity(10);
    let mut first_banner = None;
    for index in 0..10 {
        let title = format!("Preview Track {:02}", index + 1);
        let song_directory = pack_directory.join(format!("{:02} - {title}", index + 1));
        std::fs::create_dir_all(&song_directory)?;
        let banner_path = song_directory.join("banner.png");
        image::RgbaImage::from_fn(512, 160, |x, y| {
            let hue = (index as u8).wrapping_mul(23);
            image::Rgba([
                35_u8.saturating_add((x / 5) as u8).wrapping_add(hue),
                35_u8.saturating_add((y / 2) as u8).wrapping_add(hue / 2),
                110_u8.saturating_add((x / 8) as u8).wrapping_add(hue),
                255,
            ])
        })
        .save(&banner_path)?;
        if first_banner.is_none() {
            first_banner = Some(banner_path);
        }

        let bpm = 110 + index * 8;
        let mut simfile = format!(
            "#TITLE:{title};#SUBTITLE:Exporter Sample;#ARTIST:DeadSync;#BPMS:0={bpm};#OFFSET:0;#BANNER:banner.png;\n#NOTES:dance-single:Menu Preview:Challenge:{}:0,0,0,0,0:\n",
            8 + index
        );
        for row in 0..64 {
            let mut notes = *b"0000";
            if row % 4 == 0 {
                notes[(row / 4 + index) % notes.len()] = b'1';
            }
            if row % 8 == 4 {
                notes[(row / 8 + index + 2) % notes.len()] = b'1';
            }
            simfile.push_str(std::str::from_utf8(&notes)?);
            simfile.push('\n');
        }
        simfile.push_str(";\n");
        let path = song_directory.join("preview.sm");
        std::fs::write(&path, simfile)?;
        songs.push(Arc::new(
            deadsync_simfile::app_runtime::parse_song_for_test(&path, 0.0)?,
        ));
    }
    let pack = deadsync_chart::SongPack {
        group_name: pack_name.clone(),
        name: pack_name,
        sort_title: "Menu Preview Pack".into(),
        translit_title: String::new(),
        series: String::new(),
        folder_series: String::new(),
        year: 2026,
        sync_pref: deadsync_chart::SyncPref::Default,
        directory: pack_directory,
        banner_path: first_banner,
        songs,
    };
    Ok(pack)
}
