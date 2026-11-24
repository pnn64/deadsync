use deadlib_present::actors::{Actor, TextContent};
use deadsync_assets::progress::{ContentReloadPhase, LibraryLoadView};
use deadsync_config::app_config::Config;
use deadsync_theme::Screen;
use deadsync_theme::app_requests::AppEffect;
use deadsync_theme::attract::AttractPresentation;
use deadsync_theme::startup::StartupPresentation;

use crate::StepManiaIntro;
use crate::presentation::Layout;
use crate::resources::ITEM_FONT;

#[derive(Default)]
pub struct StepManiaStartup {
    intro: StepManiaIntro,
}

impl StartupPresentation<Actor, LibraryLoadView, Config> for StepManiaStartup {
    fn reset_intro(&mut self, _color_index: i32) {
        self.intro.reset();
    }

    fn advance_intro(&mut self, delta_time: f32) -> AppEffect {
        if self.intro.advance(delta_time) {
            AppEffect::NavigateNoFade(Screen::Menu)
        } else {
            AppEffect::None
        }
    }

    fn push_loading(
        &self,
        actors: &mut Vec<Actor>,
        progress: Option<&LibraryLoadView>,
        _policy: &Config,
    ) {
        let layout = Layout::current();
        actors.push(layout.solid([320.0, 240.0], [640.0, 480.0], [0.0, 0.0, 0.0, 1.0], -100));
        // The original loader is a captionless dialog: a 468x62 splash followed
        // by three centered status lines, rather than a themed full-screen page.
        actors.push(layout.solid([320.0, 240.0], [474.0, 129.0], [0.25, 0.25, 0.25, 1.0], 0));
        actors.push(layout.solid([319.0, 239.0], [472.0, 127.0], [1.0; 4], 1));
        actors.push(layout.solid([320.0, 240.0], [468.0, 123.0], [0.83, 0.82, 0.78, 1.0], 2));
        actors.push(
            layout
                .sprite(
                    "stepmania-395/loading-splash",
                    [320.0, 209.5],
                    [468.0, 62.0],
                    3,
                )
                .build(0),
        );
        let phase = progress.map_or(ContentReloadPhase::Songs, |progress| progress.phase);
        let label = match phase {
            ContentReloadPhase::Songs => "Loading songs",
            ContentReloadPhase::Courses => "Loading courses",
            ContentReloadPhase::Artwork => "Caching artwork",
            ContentReloadPhase::Noteskins => "Loading noteskins",
            ContentReloadPhase::ReplayGain => "Analyzing loudness",
        };
        let heading = if let Some(progress) = progress.filter(|progress| progress.total > 0) {
            TextContent::inline_format(format_args!(
                "{label} ({} of {})...",
                progress.done, progress.total
            ))
            .unwrap_or_else(|| {
                format!("{label} ({} of {})...", progress.done, progress.total).into()
            })
        } else {
            format!("{label}...").into()
        };
        let detail = if progress.is_some_and(|progress| progress.can_skip_replaygain) {
            "Press Start or Back to skip".into()
        } else {
            progress.map_or_else(|| "".into(), |progress| (&progress.line3).into())
        };
        let lines = [
            heading,
            progress.map_or_else(|| "".into(), |progress| (&progress.line2).into()),
            detail,
        ];
        for (index, content) in lines.into_iter().enumerate() {
            let y = [247.5, 267.0, 283.5][index];
            let mut text = layout.text(ITEM_FONT, content, [320.0, y], 0.5);
            text.diffuse([0.0, 0.0, 0.0, 1.0]);
            text.shadowlength(0.0);
            text.maxwidth(458.0 * layout.scale);
            text.z(4);
            actors.push(text.build(0));
        }
    }

    fn push_intro(&self, actors: &mut Vec<Actor>, _policy: &Config) {
        self.intro.push_actors(actors, None);
    }
}
