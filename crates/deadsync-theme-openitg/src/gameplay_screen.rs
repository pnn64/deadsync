//! OpenITG's artwork and labels for the shared gameplay presenter.

use std::sync::Arc;

use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{screen_height, screen_width};
use deadsync_assets::gameplay_presentation::{
    GameplayPresentationInit,
    standard::{GameplayHudLabel, StandardGameplay, StandardGameplaySettings, StandardGameplayStyle},
};
use deadsync_assets::gameplay_stage::{ActiveGameplayStage, GameplayStageAssets};
use deadsync_gameplay::HoldToExitKey;
use deadsync_theme::views::{GameplayStageIntro, GameplayStageView};

use crate::gameplay::GAMEPLAY_HUD;
use crate::presentation::Layout;
use crate::resources::MENU_NORMAL_FONT;

impl crate::OpenItgTheme {
    pub fn mount_gameplay<L>(
        stage: &ActiveGameplayStage,
        mut assets: GameplayStageAssets,
        view: &GameplayStageView,
        settings: StandardGameplaySettings,
        init: Option<GameplayPresentationInit>,
        lobby_code: fn(&L) -> Option<&str>,
    ) -> StandardGameplay<L> {
        let (round, badge) = match view.intro {
            GameplayStageIntro::Song { round, .. } => (round, Arc::from("EVENT")),
            GameplayStageIntro::Course { number, .. } => {
                (Some(number), Arc::from(format!("STAGE {number}")))
            }
            GameplayStageIntro::Restart(count) => (None, Arc::from(format!("RESTART {count}"))),
            GameplayStageIntro::Demonstration | GameplayStageIntro::Practice => {
                (None, Arc::from(""))
            }
        };
        let replay_status = view.replay.as_ref().map(|replay| {
            Arc::from(format!(
                "Autoplay - {} {:.2}%",
                replay.name,
                replay.score_10000 / 100.0
            ))
        });
        if let Some(course) = &view.course {
            assets.pack_banner_path.clone_from(&course.banner_path);
        }
        StandardGameplay::new(
            stage,
            assets,
            settings,
            init,
            StandardGameplayStyle {
                hud: GAMEPLAY_HUD,
                badge,
                round,
                replay_status,
                push_label,
                lobby_code,
            },
        )
    }
}

fn push_label(actors: &mut Vec<Actor>, label: GameplayHudLabel<'_>) {
    let (at, text) = match label {
        GameplayHudLabel::ExitPrompt(key) => (
            [320.0, 364.0],
            TextContent::Static(match key {
                HoldToExitKey::Start => "Continue holding START to give up",
                HoldToExitKey::Back => "Continue holding BACK to give up",
            }),
        ),
        GameplayHudLabel::Replay(status) => {
            ([320.0, 447.0], TextContent::Shared(Arc::clone(status)))
        }
        GameplayHudLabel::Profile { index, name } => {
            let Some(text) = TextContent::inline_str(name) else {
                return;
            };
            ([160.0 + 320.0 * index as f32, 465.0], text)
        }
        GameplayHudLabel::LobbyCode(code) => {
            let Some(text) = TextContent::inline_str(code) else {
                return;
            };
            ([320.0, 50.0], text)
        }
    };
    let layout = Layout::centered(screen_width(), screen_height());
    let mut label = layout.text(at, 0.5, text, 0.7, [1.0; 4]);
    label.font(MENU_NORMAL_FONT);
    label.z(1000);
    actors.push(label.build(0));
}
