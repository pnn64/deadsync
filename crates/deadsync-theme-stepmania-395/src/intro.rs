use std::sync::Arc;

use deadlib_present::actors::{Actor, SpriteSource};
use deadlib_present::dsl::SpriteBuilder;
use deadsync_config::theme::GameFlag;
use deadsync_theme::attract::{
    AttractDemoChart, AttractDemoRequest, AttractMedia, AttractPresentation,
};
use deadsync_theme::views::AttractRecords;

use crate::presentation::Layout;

#[derive(Default)]
pub struct StepManiaIntro {
    elapsed: f32,
}

impl AttractPresentation<Actor> for StepManiaIntro {
    fn reset(&mut self) {
        self.elapsed = 0.0;
    }

    fn record_limits(&self) -> (usize, usize) {
        (0, 0)
    }

    fn start_idle(&mut self, _records: AttractRecords) {
        self.reset();
    }

    fn advance(&mut self, delta_time: f32) -> bool {
        if delta_time.is_finite() && delta_time > 0.0 {
            self.elapsed += delta_time;
        }
        self.elapsed >= 18.0
    }

    fn media(&self) -> AttractMedia {
        AttractMedia {
            movie: None,
            music: (self.elapsed < 8.0).then_some("assets/media/stepmania-395/company.ogg"),
            loop_music: false,
        }
    }

    fn playback_time(&self) -> f32 {
        self.elapsed
    }
    fn demonstration(&self) -> Option<AttractDemoRequest> {
        None
    }
    fn prepare_demonstration(&mut self, _chart: Option<AttractDemoChart<'_>>) {}
    fn finish_demonstration(&mut self) {}

    fn push_actors(&self, actors: &mut Vec<Actor>, _video_key: Option<&Arc<str>>) {
        let layout = Layout::current();
        if self.elapsed < 8.0 {
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
        } else {
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
            crate::menu::push_logo(actors, layout, GameFlag::Dance, self.elapsed - 8.0, 1.0);
        }
    }

    fn push_demo_underlay(&self, _actors: &mut Vec<Actor>, _music_time: f32) {}
    fn push_demo_overlay(&self, _actors: &mut Vec<Actor>, _music_time: f32) {}
}
