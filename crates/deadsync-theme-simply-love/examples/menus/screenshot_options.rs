use super::{Result, Size, export_support, screens, theme, views};
use deadlib_assets::AssetManager;
use deadlib_present::{actors::Actor, space::Metrics};
use deadlib_render::Backend;
use std::{path::Path, time::Instant};

use deadsync_theme_simply_love::screens::options::SubmenuKind;

pub(super) const PAGES: &[(&str, &str, Option<SubmenuKind>)] = &[
    ("main", "Options Landing", None),
    ("coin", "Coin", Some(SubmenuKind::Coin)),
    ("bookkeeping", "Bookkeeping", Some(SubmenuKind::Bookkeeping)),
    ("system", "System", Some(SubmenuKind::System)),
    ("graphics", "Graphics", Some(SubmenuKind::Graphics)),
    ("input", "Input", Some(SubmenuKind::Input)),
    (
        "input-backend",
        "Input Backend",
        Some(SubmenuKind::InputBackend),
    ),
    (
        "smx-config",
        "StepManiaX Config",
        Some(SubmenuKind::SmxConfig),
    ),
    ("lights", "Lights", Some(SubmenuKind::Lights)),
    (
        "online-scoring",
        "Online Scoring",
        Some(SubmenuKind::OnlineScoring),
    ),
    ("streaming", "Streaming", Some(SubmenuKind::Streaming)),
    ("null-or-die", "Null or Die", Some(SubmenuKind::NullOrDie)),
    (
        "null-or-die-options",
        "Null or Die Options",
        Some(SubmenuKind::NullOrDieOptions),
    ),
    ("sync-packs", "Sync Packs", Some(SubmenuKind::SyncPacks)),
    ("machine", "Machine", Some(SubmenuKind::Machine)),
    ("advanced", "Advanced", Some(SubmenuKind::Advanced)),
    ("course", "Course", Some(SubmenuKind::Course)),
    ("gameplay", "Gameplay", Some(SubmenuKind::Gameplay)),
    ("tournament", "Tournament", Some(SubmenuKind::Tournament)),
    ("sound", "Sound", Some(SubmenuKind::Sound)),
    (
        "select-music",
        "Select Music",
        Some(SubmenuKind::SelectMusic),
    ),
    ("groovestats", "GrooveStats", Some(SubmenuKind::GrooveStats)),
    ("arrowcloud", "ArrowCloud", Some(SubmenuKind::ArrowCloud)),
    (
        "score-import",
        "Score Import",
        Some(SubmenuKind::ScoreImport),
    ),
    ("folders", "Folders", Some(SubmenuKind::Folders)),
    ("downloads", "Downloads", Some(SubmenuKind::Downloads)),
];

pub(super) fn export_resolution(
    backend: &mut Backend,
    assets: &mut AssetManager,
    size: &Size,
    output: &Path,
    metrics: Metrics,
    captures: &mut export_support::CaptureBatch,
    timer: &mut export_support::ScreenshotTimer,
) -> Result<()> {
    let folder = output.join("options").join(size.name());
    std::fs::create_dir_all(&folder)?;
    for &(page, _, submenu) in PAGES {
        let prepare_started = Instant::now();
        let actors = page_actors(assets, submenu);
        let frame = super::prepare_frame(&actors, backend, assets, metrics)?;
        captures.draw(
            backend,
            &frame,
            assets.textures(),
            folder.join(format!("{page}.png")),
        )?;
        timer.record_prepare(prepare_started.elapsed());
    }
    Ok(())
}

fn page_actors(assets: &AssetManager, submenu: Option<SubmenuKind>) -> Vec<Actor> {
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
    let mut state = screens::options::init(init);
    if let Some(submenu) = submenu {
        screens::options::open_submenu(&mut state, submenu);
    }
    screens::options::get_actors(&state, assets, 1.0)
}
