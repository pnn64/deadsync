use std::sync::Arc;

use deadlib_present::actors::{Actor, SpriteSource};
use deadlib_present::dsl::SpriteBuilder;
use deadsync_config::theme::GameFlag;
use deadsync_theme::attract::{
    AttractDemoChart, AttractDemoRequest, AttractMedia, AttractPresentation,
};
use deadsync_theme::views::AttractRecords;

use crate::presentation::Layout;

#[derive(Clone, Copy, Default)]
enum Phase {
    #[default]
    Company,
    Logo,
    Tutorial,
    Demonstration,
    Ranking,
    Warning,
}

#[derive(Default)]
pub struct StepManiaIntro {
    phase: Phase,
    elapsed: f32,
    idle: bool,
    records: AttractRecords,
}

impl StepManiaIntro {
    fn duration(&self) -> f32 {
        match self.phase {
            Phase::Company => 8.0,
            Phase::Logo => 10.0,
            Phase::Tutorial => 25.0,
            Phase::Demonstration => 30.0,
            Phase::Ranking => crate::ranking::PAGE_SECONDS * self.records.songs.len().max(1) as f32,
            Phase::Warning => 10.0,
        }
    }
}

impl AttractPresentation<Actor> for StepManiaIntro {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn record_limits(&self) -> (usize, usize) {
        (20, 0)
    }

    fn start_idle(&mut self, records: AttractRecords) {
        *self = Self {
            idle: true,
            records,
            ..Default::default()
        };
    }

    fn advance(&mut self, delta_time: f32) -> bool {
        if !delta_time.is_finite() || delta_time <= 0.0 {
            return false;
        }
        self.elapsed += delta_time;
        while self.elapsed >= self.duration() {
            self.elapsed -= self.duration();
            self.phase = match self.phase {
                Phase::Company => Phase::Logo,
                Phase::Logo if !self.idle => return true,
                Phase::Logo => Phase::Tutorial,
                Phase::Tutorial => Phase::Demonstration,
                Phase::Demonstration => Phase::Ranking,
                Phase::Ranking => Phase::Warning,
                Phase::Warning => Phase::Company,
            };
        }
        false
    }

    fn media(&self) -> AttractMedia {
        let (music, loop_music) = match self.phase {
            Phase::Company => (Some("assets/media/stepmania-395/company.ogg"), false),
            Phase::Tutorial => (Some("assets/media/stepmania-395/training.ogg"), true),
            Phase::Ranking => (Some("assets/media/stepmania-395/ranking.ogg"), true),
            Phase::Logo | Phase::Demonstration | Phase::Warning => (None, false),
        };
        AttractMedia {
            movie: None,
            music,
            loop_music,
        }
    }

    fn playback_time(&self) -> f32 {
        self.elapsed
    }
    fn demonstration(&self) -> Option<AttractDemoRequest> {
        matches!(self.phase, Phase::Demonstration).then_some(AttractDemoRequest {
            game: "dance",
            chart_type: "dance-single",
            max_difficulty: 2,
            force_uncentered_notefield: true,
            hide_gameplay_hud: false,
            two_players: true,
            play_music: false,
        })
    }
    fn prepare_demonstration(&mut self, _chart: Option<AttractDemoChart<'_>>) {}
    fn finish_demonstration(&mut self) {
        if matches!(self.phase, Phase::Demonstration) {
            self.phase = Phase::Ranking;
            self.elapsed = 0.0;
        }
    }

    fn push_actors(&self, actors: &mut Vec<Actor>, _video_key: Option<&Arc<str>>) {
        let layout = Layout::current();
        match self.phase {
            Phase::Company => {
                let mut white = SpriteBuilder::with_source(SpriteSource::Solid);
                let [x, y] = layout.position([320.0, 240.0]);
                white.xy(x, y);
                white.size(640.0 * layout.scale, 480.0 * layout.scale);
                white.z(-100);
                actors.push(white.build(0));
                let x = 340.0 - self.elapsed * 5.0;
                let mut shadow = layout.sprite(
                    "stepmania-395/company-shadow",
                    [x, 246.0],
                    [512.0, 284.0],
                    0,
                );
                shadow.diffuse([0.0, 0.0, 0.0, 1.0]);
                actors.push(shadow.build(0));
                actors.push(
                    layout
                        .sprite("stepmania-395/company", [320.0, 240.0], [480.0, 252.0], 1)
                        .build(0),
                );
            }
            Phase::Logo => {
                actors.push(
                    layout
                        .sprite(
                            "stepmania-395/title-background",
                            [320.0, 240.0],
                            [640.0, 480.0],
                            -100,
                        )
                        .build(0),
                );
                crate::menu::push_logo(actors, layout, GameFlag::Dance, self.elapsed, 1.0);
            }
            Phase::Tutorial => crate::tutorial::push_tutorial(actors, self.elapsed),
            Phase::Demonstration => self.push_demo_overlay(actors, self.elapsed),
            Phase::Ranking => crate::ranking::push_ranking(actors, &self.records, self.elapsed),
            Phase::Warning => actors.push(
                layout
                    .sprite(
                        "stepmania-395/attract-warning",
                        [320.0, 240.0],
                        [640.0, 480.0],
                        -100,
                    )
                    .build(0),
            ),
        }
    }

    fn push_demo_underlay(&self, _actors: &mut Vec<Actor>, _music_time: f32) {}
    fn push_demo_overlay(&self, actors: &mut Vec<Actor>, _music_time: f32) {
        let layout = Layout::current();
        let mut shade = SpriteBuilder::with_source(SpriteSource::Solid);
        let [x, y] = layout.position([320.0, 240.0]);
        shade.xy(x, y);
        shade.size(640.0 * layout.scale, 120.0 * layout.scale);
        shade.diffuse([0.0, 0.0, 0.0, 0.7]);
        shade.z(1000);
        actors.push(shade.build(0));
        if self.elapsed.rem_euclid(1.0) < 0.5 {
            actors.push(
                layout
                    .sprite(
                        "stepmania-395/attract-demo",
                        [320.0, 240.0],
                        [320.0, 34.0],
                        1001,
                    )
                    .build(0),
            );
        }
    }
}
