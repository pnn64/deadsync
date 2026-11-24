use std::{path::Path, time::Instant};

use deadlib_present::actors::Actor;
use deadsync_config::{
    coin::CoinMode,
    theme::{GameFlag, ThemeFlag},
};
use deadsync_core::input::InputSource;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_theme::attract::AttractPresentation;
use deadsync_theme::main_menu::{MainMenuFrame, MainMenuPhase, MainMenuRuntimeView, ThemeMainMenu};
use deadsync_theme::stage::{StageCardMode, StageCardPresentation, StageCardView};
use deadsync_theme::{Screen, ThemeEffect, ThemeResources, TransitionState};
use deadsync_theme_stepmania_395::{StepManiaIntro, StepManiaStageCard, StepManiaTheme};

#[test]
fn installed_native_title_uses_bundled_resources_and_shared_navigation() {
    assert_eq!(
        "StepMania 3.95".parse::<ThemeFlag>(),
        Ok(ThemeFlag::StepMania395)
    );
    let manifest = StepManiaTheme::asset_manifest();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for png in manifest.embedded_pngs {
        image::load_from_memory_with_format(png.bytes, image::ImageFormat::Png).unwrap();
    }
    for spec in manifest.fonts {
        let font =
            deadlib_present::font::parse(root.join(spec.ini_path).to_str().unwrap()).unwrap();
        assert!(font.font.glyph_map.contains_key(&'A'));
        assert!(font.font.line_spacing > 0);
    }
    let mut state = StepManiaTheme::init_menu(2);
    StepManiaTheme::sync_menu_runtime(
        &mut state,
        MainMenuRuntimeView {
            game: GameFlag::Pump,
            song_count: 37,
            ..Default::default()
        },
    );
    StepManiaTheme::update_menu::<()>(&mut state, 1.0, MainMenuPhase::Active);
    let mut actors = Vec::new();
    StepManiaTheme::push_menu_actors(
        &mut actors,
        &state,
        MainMenuFrame {
            update_banner_tag: None,
            background_key: None,
            alpha_multiplier: 1.0,
            transition: &TransitionState::Idle,
            visual_policy: (),
        },
    );
    assert!(
        actors
            .iter()
            .any(|actor| matches!(actor, Actor::Sprite { source, .. }
        if source.texture_key() == Some("stepmania-395/title-pump")))
    );
    assert_eq!(
        actors
            .iter()
            .filter(|actor| matches!(
                actor,
                Actor::Text {
                    font: "stepmania-395/title",
                    ..
                }
            ))
            .count(),
        7
    );
    let now = Instant::now();
    let event = |action| InputEvent::new(action, 0, true, InputSource::Keyboard, now, 0, now, now);
    let mut effects = Vec::new();
    StepManiaTheme::handle_menu_input::<()>(&mut state, &event(VirtualAction::p2_start))
        .append_to(&mut effects);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ThemeEffect::Navigate(Screen::SelectProfile)))
    );
    StepManiaTheme::handle_menu_input::<()>(&mut state, &event(VirtualAction::p2_menu_down));
    effects.clear();
    StepManiaTheme::handle_menu_input::<()>(&mut state, &event(VirtualAction::p2_start))
        .append_to(&mut effects);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ThemeEffect::Navigate(Screen::Options)))
    );
    StepManiaTheme::reset_menu(&mut state);
    StepManiaTheme::sync_menu_runtime(
        &mut state,
        MainMenuRuntimeView {
            coin_mode: CoinMode::Pay,
            ..Default::default()
        },
    );
    effects.clear();
    StepManiaTheme::handle_menu_input::<()>(&mut state, &event(VirtualAction::p2_menu_down));
    StepManiaTheme::handle_menu_input::<()>(&mut state, &event(VirtualAction::p2_start))
        .append_to(&mut effects);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, ThemeEffect::Navigate(Screen::SelectProfile)))
    );
}

#[test]
fn native_intro_advances_from_company_to_logo_then_title() {
    let mut intro = StepManiaIntro::default();
    assert_eq!(
        intro.media().music,
        Some("assets/media/stepmania-395/company.ogg")
    );
    assert!(!intro.advance(8.0));
    assert_eq!(intro.media().music, None);
    let mut actors = Vec::new();
    intro.push_actors(&mut actors, None);
    assert!(matches!(&actors[0], Actor::Sprite { source, .. }
        if source.texture_key() == Some("stepmania-395/title-background")));
    assert!(intro.advance(10.0));
    intro.reset();
    assert_eq!(intro.playback_time(), 0.0);
}

#[test]
fn stage_card_uses_the_session_mode_and_native_exit_clock() {
    let card = StepManiaStageCard;
    assert_eq!(card.timing().hold.as_secs_f32(), 3.0);
    let mut actors = Vec::new();
    card.push_actors(
        &mut actors,
        StageCardView {
            stage_number: 2,
            mode: StageCardMode::Event,
            elapsed: 1.0,
            exit_elapsed: Some(0.15),
        },
    );
    assert!(actors.iter().any(|actor| matches!(actor, Actor::Sprite { source, tint, .. }
        if source.texture_key() == Some("stepmania-395/stage-event") && (tint[3] - 0.5).abs() < 1e-6)));
    assert!(
        !actors
            .iter()
            .any(|actor| matches!(actor, Actor::Sprite { source, .. }
        if source.texture_key() == Some("stepmania-395/stage-2")))
    );
}
