use std::io;
use std::path::Path;
use std::sync::Arc;

use deadlib_assets::{
    TextureDecodeJob, parse_texture_hints, texture_key_sampler, texture_source_size,
};
use deadlib_present::actors::{Actor, SizeSpec, SpriteSource};
use deadlib_present::space::{screen_center_x, screen_center_y};
use deadsync_theme::attract::{
    AttractDemoChart, AttractDemoRequest, AttractMedia, AttractPresentation,
};
use deadsync_theme::views::AttractRecords;

use crate::{BackgroundPlayback, BackgroundTexture, ThemePackage};

const CANVAS: [f32; 2] = [640.0, 480.0];

/// The authored company and logo backgrounds played after library loading.
pub struct PackageIntro {
    scenes: Vec<BackgroundPlayback>,
    index: usize,
    current: BackgroundPlayback,
}

struct IntroTexture {
    job: TextureDecodeJob,
    binding: BackgroundTexture,
}

impl PackageIntro {
    /// Prepares playback and the texture uploads needed by the renderer.
    pub fn open(root: &Path, language: Option<&str>) -> io::Result<(Self, Vec<TextureDecodeJob>)> {
        let package = ThemePackage::open([root], language)?;
        let mut scenes = Vec::new();
        let mut textures: Vec<IntroTexture> = Vec::new();
        for section in ["ScreenCompany", "ScreenLogo"] {
            let Some(background) = package.background(section, "background", CANVAS)? else {
                continue;
            };
            let scene = background.try_start(|path, repeat| {
                let hints = parse_texture_hints(
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .as_ref(),
                );
                let sampler = texture_key_sampler(&hints, repeat);
                if let Some(texture) = textures
                    .iter()
                    .find(|texture| texture.job.path == path && texture.job.sampler == sampler)
                {
                    return Ok(texture.binding.clone());
                }
                let (width, height) = texture_source_size(path).map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("reading dimensions of {}: {error}", path.display()),
                    )
                })?;
                let key = format!("local-theme/intro/{}", textures.len());
                let binding = BackgroundTexture {
                    source: SpriteSource::Texture(Arc::from(key.as_str())),
                    dimensions: [width as f32, height as f32],
                };
                textures.push(IntroTexture {
                    job: TextureDecodeJob {
                        key,
                        path: path.to_owned(),
                        sampler,
                        hints,
                    },
                    binding: binding.clone(),
                });
                Ok(binding)
            })?;
            scenes.push(scene);
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
            textures.into_iter().map(|texture| texture.job).collect(),
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
        actors.push(Actor::Frame {
            align: [0.5, 0.5],
            offset: [screen_center_x(), screen_center_y()],
            size: CANVAS.map(SizeSpec::Px),
            children,
            background: None,
            z: 0,
        });
    }

    fn push_demo_underlay(&self, _actors: &mut Vec<Actor>, _music_time: f32) {}

    fn push_demo_overlay(&self, _actors: &mut Vec<Actor>, _music_time: f32) {}
}
