//! Manual GPU captures of native presentation, without opening a visible window.
#![cfg(all(
    target_os = "windows",
    not(target_pointer_width = "32"),
    not(target_vendor = "win7")
))]

use std::{path::PathBuf, sync::Arc};
mod support;

use deadlib_assets::AssetManager;
use deadlib_present::{
    actors::Actor,
    compose,
    space::{self, Metrics},
};
use deadlib_render::{BackendType, PresentModePolicy, create_backend};
use deadsync_theme::startup::StartupPresentation;
use deadsync_theme::{
    ThemeResources,
    attract::AttractPresentation,
    views::{AttractRecord, AttractRecords},
};
use deadsync_theme_stepmania_395::StepManiaStartup;
use deadsync_theme_stepmania_395::{StepManiaIntro, StepManiaTheme};
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

#[test]
#[ignore = "requires a graphics device; writes reference captures under target"]
fn native_attract_captures() {
    let events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[expect(deprecated, reason = "hidden renderer fixture needs no event dispatch")]
    let window = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(640, 480)),
            )
            .unwrap(),
    );
    let metrics = Metrics::centered(640.0, 480.0);
    space::set_current_metrics(metrics);
    space::set_current_window_px(640, 480);
    let mut backend = create_backend(
        BackendType::VulkanWgpu,
        window,
        metrics.projection(),
        false,
        PresentModePolicy::Immediate,
        false,
        true,
    )
    .unwrap();
    let mut assets = AssetManager::new();
    assets.load_textures(&mut backend, Vec::new()).unwrap();
    let manifest = StepManiaTheme::asset_manifest();
    for &png in manifest.embedded_pngs {
        assets.load_embedded_png(&mut backend, png).unwrap();
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let roots = [root.join("assets")];
    for parsed in
        deadlib_assets::parse_font_asset_specs(manifest.fonts.iter().copied(), &roots, |path| {
            root.join(path)
        })
        .unwrap()
    {
        for texture in deadlib_assets::prepare_required_font_textures(
            &parsed.font,
            &parsed.required_textures,
            &roots,
            |key| assets.has_texture_key(key),
        )
        .unwrap()
        {
            assets
                .update_texture_for_key_with_sampler(
                    &mut backend,
                    &texture.key,
                    &texture.image,
                    texture.hints.sampler_desc(),
                )
                .unwrap();
        }
        assets.register_font(parsed.name, parsed.font);
    }
    let output = root.join("target/native-395-attract-captures");
    std::fs::create_dir_all(&output).unwrap();
    let progress = deadsync_assets::progress::LibraryLoadView {
        phase: deadsync_assets::progress::ContentReloadPhase::Songs,
        done: 47,
        total: 128,
        line2: "Dance Pack".into(),
        line3: "Quartz".into(),
        finished: false,
        can_skip_replaygain: false,
        started_at: std::time::Instant::now(),
        phase_started_at: std::time::Instant::now(),
    };
    let mut loading = Vec::new();
    StepManiaStartup::default().push_loading(&mut loading, Some(&progress), &Default::default());
    capture_actors(
        "loading",
        &loading,
        0.0,
        &output,
        &metrics,
        &mut backend,
        &assets,
    );
    let mut intro = StepManiaIntro::default();
    capture_actors(
        "select-music",
        &support::music_actors(),
        3.0,
        &output,
        &metrics,
        &mut backend,
        &assets,
    );
    for (name, delta) in [("company", 3.0), ("logo", 9.0)] {
        intro.advance(delta);
        capture(name, &intro, &output, &metrics, &mut backend, &assets);
    }
    for (name, course, p2_only, elapsed) in [
        ("evaluation-stage", false, false, 5.0),
        ("evaluation-stage-entering", false, false, 0.9),
        ("evaluation-solo-p2", false, true, 5.0),
        ("evaluation-nonstop", true, false, 5.0),
    ] {
        capture_actors(
            name,
            &support::evaluation_actors(course, p2_only, elapsed),
            elapsed,
            &output,
            &metrics,
            &mut backend,
            &assets,
        );
    }
    intro.start_idle(AttractRecords::default());
    intro.advance(18.0);
    for (name, delta) in [
        ("tutorial-title", 0.4),
        ("tutorial-pad", 6.6),
        ("tutorial-down", 3.0),
        ("tutorial-jump", 9.5),
        ("tutorial-miss", 4.0),
    ] {
        intro.advance(delta);
        capture(name, &intro, &output, &metrics, &mut backend, &assets);
    }
    intro.start_idle(records());
    intro.advance(43.0);
    capture(
        "demonstration-overlay",
        &intro,
        &output,
        &metrics,
        &mut backend,
        &assets,
    );
    intro.finish_demonstration();
    intro.advance(2.0);
    capture("ranking", &intro, &output, &metrics, &mut backend, &assets);
}

fn records() -> AttractRecords {
    AttractRecords {
        songs: vec![AttractRecord {
            title: "Quartz".into(),
            chart_type: "dance-single".into(),
            difficulty: "Hard".into(),
            initials: "AAA".into(),
            percent: 98.76,
            leaderboard: ["AAA", "BBB", "CCC"]
                .into_iter()
                .enumerate()
                .map(|(rank, name)| deadsync_score::LeaderboardEntry {
                    rank: rank as u32 + 1,
                    name: name.into(),
                    machine_tag: None,
                    score: 9876.0 - rank as f64 * 120.0,
                    date: String::new(),
                    is_rival: false,
                    is_self: false,
                    is_fail: false,
                })
                .collect(),
        }],
        ..Default::default()
    }
}

fn capture(
    name: &str,
    intro: &StepManiaIntro,
    output: &std::path::Path,
    metrics: &Metrics,
    backend: &mut deadlib_render::Backend,
    assets: &AssetManager,
) {
    let mut actors: Vec<Actor> = Vec::new();
    intro.push_actors(&mut actors, None);
    capture_actors(
        name,
        &actors,
        intro.playback_time(),
        output,
        metrics,
        backend,
        assets,
    );
}

fn capture_actors(
    name: &str,
    actors: &[Actor],
    elapsed: f32,
    output: &std::path::Path,
    metrics: &Metrics,
    backend: &mut deadlib_render::Backend,
    assets: &AssetManager,
) {
    let frame = compose::build_screen_with_texture_context(
        actors,
        [0.0, 0.0, 0.0, 1.0],
        metrics,
        assets.fonts(),
        elapsed,
        assets.texture_context(),
    );
    assert!(!frame.ops.is_empty(), "{name} has no rendered actors");
    backend.request_screenshot();
    backend.draw(&frame, assets.textures(), false).unwrap();
    let image = backend.capture_frame().unwrap();
    assert_eq!(image.dimensions(), (640, 480));
    image.save(output.join(format!("{name}.png"))).unwrap();
}
