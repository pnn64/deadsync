use std::{io, path::Path, time::Duration};

use deadlib_assets::TextureDecodeJob;
use deadlib_present::actors::Actor;
use deadsync_theme::stage::{StageCardMode, StageCardPresentation, StageCardTiming, StageCardView};

use crate::scene::{SceneTextures, coordinate, frame};
use crate::{BackgroundPlayback, ThemePackage};

const SCREEN: &str = "ScreenStage";

pub struct PackageStageCard {
    background: Option<BackgroundPlayback>,
    overlay: Option<BackgroundPlayback>,
    entry: Option<BackgroundPlayback>,
    exit: Option<BackgroundPlayback>,
    timing: StageCardTiming,
}

impl PackageStageCard {
    pub fn open(root: &Path, language: Option<&str>) -> io::Result<(Self, Vec<TextureDecodeJob>)> {
        let package = ThemePackage::open([root], language)?;
        let mut textures = SceneTextures::new("stage");
        let background = textures.background(&package, SCREEN)?;
        let overlay = textures.animation(&package, SCREEN, "overlay")?;
        if background.is_none() && overlay.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "theme has no stage background or overlay",
            ));
        }
        let entry = textures.animation(&package, SCREEN, "in")?;
        let exit = textures.animation(&package, SCREEN, "out")?;
        let delay = package
            .metric(SCREEN, "MinimumDelay")
            .map_err(io::Error::other)?
            .map(coordinate)
            .transpose()
            .map_err(io::Error::other)?
            .unwrap_or_default();
        let hold_seconds = entry
            .as_ref()
            .map_or(0.0, BackgroundPlayback::duration_seconds);
        let hold = duration(delay, "MinimumDelay")?
            .checked_add(duration(hold_seconds, "entry animation")?)
            .ok_or_else(|| invalid_duration("total stage hold"))?;
        let exit_seconds = exit
            .as_ref()
            .map_or(0.0, BackgroundPlayback::duration_seconds);
        let timing = StageCardTiming {
            hold,
            exit: duration(exit_seconds, "exit animation")?,
        };
        Ok((
            Self {
                background,
                overlay,
                entry,
                exit,
                timing,
            },
            textures.into_jobs(),
        ))
    }
}

impl StageCardPresentation<Actor> for PackageStageCard {
    fn timing(&self) -> StageCardTiming {
        self.timing
    }

    fn push_actors(&self, actors: &mut Vec<Actor>, view: StageCardView) {
        let numbered = view.stage_number.to_string();
        let name = match view.mode {
            StageCardMode::Regular => numbered.as_str(),
            StageCardMode::Final => "final",
            StageCardMode::Event => "event",
            StageCardMode::Nonstop => "nonstop",
            StageCardMode::Oni => "oni",
            StageCardMode::Endless => "endless",
            StageCardMode::Survival => "survival",
        };
        let mut children = Vec::new();
        let entry = self
            .entry
            .as_ref()
            .filter(|scene| view.elapsed < scene.duration_seconds());
        let exit = self.exit.as_ref().zip(view.exit_elapsed);
        for (scene, elapsed, z) in [
            (self.background.as_ref(), view.elapsed, i16::MIN),
            (self.overlay.as_ref(), view.elapsed, -10_000),
            (entry, view.elapsed, 0),
            (
                exit.map(|(scene, _)| scene),
                view.exit_elapsed.unwrap_or_default(),
                10_000,
            ),
        ] {
            let Some(initial) = scene else { continue };
            let stage = if initial.has_stage(name) {
                name
            } else {
                &numbered
            };
            let mut playback = initial.clone();
            playback.advance(elapsed);
            playback.push_layers(&mut children, z, Some(stage));
        }
        actors.push(frame(children));
    }
}

fn duration(seconds: f32, name: &str) -> io::Result<Duration> {
    Duration::try_from_secs_f32(seconds).map_err(|_| invalid_duration(name))
}

fn invalid_duration(name: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("[{SCREEN}] invalid {name} duration"),
    )
}
