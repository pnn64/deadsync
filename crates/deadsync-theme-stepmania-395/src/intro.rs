use std::io;
use std::path::Path;
use std::sync::Arc;

use deadlib_assets::TextureDecodeJob;
use deadlib_present::actors::Actor;
use deadsync_theme::attract::{
    AttractDemoChart, AttractDemoRequest, AttractMedia, AttractPresentation,
};
use deadsync_theme::views::AttractRecords;

use crate::scene::{SceneTextures, frame};
use crate::{BackgroundPlayback, ThemePackage};

/// The authored company and logo backgrounds played after library loading.
pub struct PackageIntro {
    scenes: Vec<BackgroundPlayback>,
    index: usize,
    current: BackgroundPlayback,
}

impl PackageIntro {
    /// Prepares playback and the texture uploads needed by the renderer.
    pub fn open(root: &Path, language: Option<&str>) -> io::Result<(Self, Vec<TextureDecodeJob>)> {
        let package = ThemePackage::open([root], language)?;
        let mut scenes = Vec::new();
        let mut textures = SceneTextures::new("intro");
        for section in ["ScreenCompany", "ScreenLogo"] {
            if let Some(scene) = textures.background(&package, section)? {
                scenes.push(scene);
            }
        }
        let current = scenes.first().cloned().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "theme has no company or logo background",
            )
        })?;
        Ok((
            Self {
                scenes,
                index: 0,
                current,
            },
            textures.into_jobs(),
        ))
    }
}

impl AttractPresentation<Actor> for PackageIntro {
    fn reset(&mut self) {
        self.index = 0;
        self.current = self.scenes[0].clone();
    }

    fn record_limits(&self) -> (usize, usize) {
        (0, 0)
    }

    fn start_idle(&mut self, _records: AttractRecords) {
        self.reset();
    }

    fn advance(&mut self, delta_time: f32) -> bool {
        self.current.advance(delta_time);
        if !self.current.is_finished() {
            return false;
        }
        if let Some(next) = self.scenes.get(self.index + 1) {
            self.index += 1;
            self.current = next.clone();
            return false;
        }
        true
    }

    fn media(&self) -> AttractMedia {
        AttractMedia {
            movie: None,
            music: None,
            loop_music: false,
        }
    }

    fn playback_time(&self) -> f32 {
        self.current.elapsed_seconds()
    }

    fn demonstration(&self) -> Option<AttractDemoRequest> {
        None
    }

    fn prepare_demonstration(&mut self, _chart: Option<AttractDemoChart<'_>>) {}

    fn finish_demonstration(&mut self) {}

    fn push_actors(&self, actors: &mut Vec<Actor>, _video_key: Option<&Arc<str>>) {
        let mut children = Vec::new();
        self.current.push_actors(&mut children, 0);
        actors.push(frame(children));
    }

    fn push_demo_underlay(&self, _actors: &mut Vec<Actor>, _music_time: f32) {}

    fn push_demo_overlay(&self, _actors: &mut Vec<Actor>, _music_time: f32) {}
}
