use std::{io, path::Path, sync::Arc};

use deadlib_assets::{
    TextureDecodeJob, parse_texture_hints, texture_key_sampler, texture_source_size,
};
use deadlib_present::actors::{Actor, SizeSpec, SpriteSource};
use deadlib_present::space::{screen_center_x, screen_center_y};

use crate::{
    ActorAnimation, BackgroundPlayback, BackgroundTexture, ResourceCategory, ThemePackage,
};

pub(crate) const CANVAS: [f32; 2] = [640.0, 480.0];

pub(crate) fn frame(children: Vec<Actor>) -> Actor {
    Actor::Frame {
        align: [0.5, 0.5],
        offset: [screen_center_x(), screen_center_y()],
        size: CANVAS.map(SizeSpec::Px),
        children,
        background: None,
        z: 0,
    }
}

pub(crate) struct SceneTextures {
    namespace: &'static str,
    entries: Vec<(TextureDecodeJob, BackgroundTexture)>,
}

impl SceneTextures {
    pub(crate) const fn new(namespace: &'static str) -> Self {
        Self {
            namespace,
            entries: Vec::new(),
        }
    }

    pub(crate) fn background(
        &mut self,
        package: &ThemePackage,
        section: &str,
    ) -> io::Result<Option<BackgroundPlayback>> {
        self.animation(package, section, "background")
    }

    pub(crate) fn animation(
        &mut self,
        package: &ThemePackage,
        section: &str,
        name: &str,
    ) -> io::Result<Option<BackgroundPlayback>> {
        package
            .background(section, name, CANVAS)?
            .map(|background| background.try_start(|path, repeat| self.bind(path, repeat)))
            .transpose()
    }

    pub(crate) fn into_jobs(self) -> Vec<TextureDecodeJob> {
        self.entries.into_iter().map(|(job, _)| job).collect()
    }

    pub(crate) fn graphic(
        &mut self,
        package: &ThemePackage,
        section: &str,
        name: &str,
    ) -> io::Result<Option<BackgroundTexture>> {
        package
            .screen_resource(ResourceCategory::Graphics, section, name)
            .map_err(io::Error::other)?
            .map(|path| self.bind(path, false))
            .transpose()
    }

    pub(crate) fn bind(&mut self, path: &Path, repeat: bool) -> io::Result<BackgroundTexture> {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let hints = parse_texture_hints(&stem);
        let sampler = texture_key_sampler(&hints, repeat);
        if let Some((_, binding)) = self
            .entries
            .iter()
            .find(|(job, _)| job.path == path && job.sampler == sampler)
        {
            return Ok(binding.clone());
        }
        let (width, height) = texture_source_size(path).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("reading dimensions of {}: {error}", path.display()),
            )
        })?;
        let key = format!("local-theme/{}/{}", self.namespace, self.entries.len());
        let binding = BackgroundTexture {
            source: SpriteSource::Texture(Arc::from(key.as_str())),
            dimensions: [width as f32, height as f32],
        };
        self.entries.push((
            TextureDecodeJob {
                key,
                path: path.to_owned(),
                sampler,
                hints,
            },
            binding.clone(),
        ));
        Ok(binding)
    }
}

pub(crate) fn coordinate(value: &str) -> Result<f32, crate::AnimationError> {
    let animation = ActorAnimation::parse(&format!("x,{value}"), CANVAS)?;
    Ok(animation.start(Default::default()).state().x)
}
