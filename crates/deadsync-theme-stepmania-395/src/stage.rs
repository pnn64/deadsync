use std::time::Duration;

use deadlib_present::actors::{Actor, SpriteSource};
use deadlib_present::dsl::SpriteBuilder;
use deadsync_theme::stage::{StageCardMode, StageCardPresentation, StageCardTiming, StageCardView};

use crate::presentation::Layout;

pub struct StepManiaStageCard;

impl StageCardPresentation<Actor> for StepManiaStageCard {
    fn timing(&self) -> StageCardTiming {
        StageCardTiming {
            hold: Duration::from_secs(3),
            exit: Duration::from_millis(300),
        }
    }

    fn push_actors(&self, actors: &mut Vec<Actor>, view: StageCardView) {
        let layout = Layout::current();
        let mut background = SpriteBuilder::with_source(SpriteSource::Solid);
        let [x, y] = layout.position([320.0, 240.0]);
        background.xy(x, y);
        background.size(640.0 * layout.scale, 480.0 * layout.scale);
        background.diffuse([0.0, 0.0, 0.0, 1.0]);
        background.z(-100);
        actors.push(background.build(0));

        let (key, size) = match view.mode {
            StageCardMode::Regular => match view.stage_number {
                1 => ("stepmania-395/stage-1", [324.0, 70.0]),
                2 => ("stepmania-395/stage-2", [342.0, 72.0]),
                3 => ("stepmania-395/stage-3", [342.0, 72.0]),
                4 => ("stepmania-395/stage-4", [342.0, 72.0]),
                5 => ("stepmania-395/stage-5", [342.0, 72.0]),
                _ => ("stepmania-395/stage-6", [342.0, 72.0]),
            },
            StageCardMode::Final => ("stepmania-395/stage-final", [400.0, 70.0]),
            StageCardMode::Event => ("stepmania-395/stage-event", [390.0, 58.0]),
            StageCardMode::Nonstop => ("stepmania-395/stage-nonstop", [512.0, 74.0]),
            StageCardMode::Oni => ("stepmania-395/stage-oni", [512.0, 64.0]),
            StageCardMode::Endless | StageCardMode::Survival => {
                ("stepmania-395/stage-endless", [480.0, 74.0])
            }
        };
        let remaining = 1.0 - ((view.elapsed - 0.1) / 0.6).clamp(0.0, 1.0);
        let alpha = 1.0 - view.exit_elapsed.unwrap_or_default() / 0.3;
        let mut label = layout.sprite(key, [320.0, 240.0 + 90.0 * remaining * remaining], size, 0);
        label.alpha(alpha.clamp(0.0, 1.0));
        actors.push(label.build(0));

        let mut mask = SpriteBuilder::with_source(SpriteSource::Solid);
        let [x, y] = layout.position([320.0, 330.0]);
        mask.xy(x, y);
        mask.size(640.0 * layout.scale, 80.0 * layout.scale);
        mask.diffuse([0.0, 0.0, 0.0, 1.0]);
        mask.z(1);
        actors.push(mask.build(0));

        let grow = (view.elapsed / 0.1).clamp(0.0, 1.0);
        let shrink = 1.0 - ((view.elapsed - 0.2) / 0.2).clamp(0.0, 1.0).powi(2);
        let mut rule = layout.sprite(
            "stepmania-395/stage-underline",
            [320.0, 290.0],
            [386.0, 10.0],
            2,
        );
        rule.zoomx((0.5 + 0.5 * grow) * shrink);
        rule.alpha((0.5 + 0.5 * grow) * shrink * alpha.clamp(0.0, 1.0));
        actors.push(rule.build(0));
    }
}
