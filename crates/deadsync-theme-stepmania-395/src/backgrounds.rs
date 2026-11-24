use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use deadlib_present::actors::{Actor, SpriteSource};
use deadlib_present::anim::{TweenSeq, TweenState};
use deadlib_present::dsl::SpriteBuilder;
use deadsync_config::ini::{IniSection, SimpleIni};

use crate::{ActorAnimation, AnimationError};

/// A numbered sprite-layer background on a logical canvas.
#[derive(Debug)]
pub struct BackgroundDefinition {
    canvas: [f32; 2],
    minimum_seconds: f32,
    layers: Vec<Layer>,
}

#[derive(Debug)]
struct Layer {
    path: PathBuf,
    stretch: bool,
    scroll: [f32; 2],
    on: ActorAnimation,
}

/// The host's texture binding and the source dimensions used by layer tweens.
#[derive(Clone)]
pub struct BackgroundTexture {
    pub source: SpriteSource,
    pub dimensions: [f32; 2],
}

#[derive(Clone)]
pub struct BackgroundPlayback {
    minimum_seconds: f32,
    elapsed_seconds: f32,
    layers: Vec<PlayingLayer>,
}

#[derive(Clone)]
struct PlayingLayer {
    source: SpriteSource,
    scroll: [f32; 2],
    tween: TweenSeq,
}

impl BackgroundDefinition {
    pub(crate) fn read(directory: &Path, canvas: [f32; 2]) -> io::Result<Self> {
        if canvas
            .iter()
            .any(|dimension| !dimension.is_finite() || *dimension <= 0.0)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid background canvas",
            ));
        }
        if !directory.is_dir() {
            return Err(unsupported(
                directory,
                "expected an INI background directory",
            ));
        }
        let root = directory
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| invalid(directory, "background directory has no package root"))?;
        let mut ancestry = vec![directory.canonicalize()?];
        let (minimum_seconds, layers) = Self::read_layers(directory, root, canvas, &mut ancestry)?;
        Ok(Self {
            canvas,
            minimum_seconds,
            layers,
        })
    }

    fn read_layers(
        directory: &Path,
        root: &Path,
        canvas: [f32; 2],
        ancestry: &mut Vec<PathBuf>,
    ) -> io::Result<(f32, Vec<Layer>)> {
        let path = directory.join("BGAnimation.ini");
        let text = fs::read_to_string(&path).map_err(|error| {
            io::Error::new(error.kind(), format!("reading {}: {error}", path.display()))
        })?;
        let mut ini = SimpleIni::new();
        ini.load_str(text.trim_start_matches('\u{feff}'));

        if ini.get_section("Scroller").is_some() {
            return Err(unsupported(&path, "background scrollers"));
        }
        let mut minimum_seconds = 0.0;
        if let Some(settings) = ini.get_section("BGAnimation") {
            check_supported(
                &path,
                "BGAnimation",
                settings,
                &["Condition", "Cond", "InitCommand", "OnCommand"],
            )?;
            minimum_seconds =
                number(&path, "BGAnimation", settings, "LengthSeconds")?.unwrap_or(0.0);
            if minimum_seconds < 0.0 {
                return Err(invalid(
                    &path,
                    "[BGAnimation] LengthSeconds must be nonnegative",
                ));
            }
        }

        let mut numbered = ini
            .sections()
            .iter()
            .filter_map(|(section, fields)| {
                section
                    .strip_prefix("Layer")
                    .map(|suffix| (suffix, section, fields))
            })
            .map(|(suffix, section, fields)| {
                let number = suffix
                    .parse::<u32>()
                    .map_err(|_| invalid(&path, format!("invalid layer section [{section}]")))?;
                Ok((number, section, fields))
            })
            .collect::<io::Result<Vec<_>>>()?;
        numbered.sort_unstable_by_key(|(number, _, _)| *number);
        for pair in numbered.windows(2) {
            if pair[0].0 == pair[1].0 {
                return Err(invalid(
                    &path,
                    format!("duplicate layer number {}", pair[0].0),
                ));
            }
        }
        let mut layers = Vec::with_capacity(numbered.len());
        for (_, section, fields) in numbered {
            let Some(import) = fields.get("Import") else {
                layers.push(Layer::read(
                    &path, root, directory, section, fields, canvas,
                )?);
                continue;
            };
            check_supported(
                &path,
                section,
                fields,
                &[
                    "Condition",
                    "Cond",
                    "DrawCond",
                    "InitCommand",
                    "File",
                    "Command",
                    "OnCommand",
                ],
            )?;
            let import = scalar(import);
            let imported_directory =
                package_relative_path(root, directory, import).ok_or_else(|| {
                    invalid(
                        &path,
                        format!("[{section}] Import must be relative to the theme package"),
                    )
                })?;
            let imported_directory = imported_directory.canonicalize().map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!("{} [{section}] Import {import:?}: {error}", path.display()),
                )
            })?;
            if !imported_directory.starts_with(root) || !imported_directory.is_dir() {
                return Err(invalid(
                    &path,
                    format!(
                        "[{section}] Import must resolve to a directory within the theme package"
                    ),
                ));
            }
            if ancestry.contains(&imported_directory) {
                return Err(invalid(
                    &path,
                    format!("[{section}] recursive Import {import:?}"),
                ));
            }
            ancestry.push(imported_directory.clone());
            let (_, imported_layers) =
                Self::read_layers(&imported_directory, root, canvas, ancestry)?;
            ancestry.pop();
            layers.extend(imported_layers);
        }
        Ok((minimum_seconds, layers))
    }

    /// Binds layer textures and starts their entry commands. The second
    /// argument to the binding callback selects repeat sampling.
    pub fn start(
        &self,
        mut bind_texture: impl FnMut(&Path, bool) -> Option<BackgroundTexture>,
    ) -> io::Result<BackgroundPlayback> {
        self.try_start(|path, repeat| {
            bind_texture(path, repeat).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("background texture {} is unavailable", path.display()),
                )
            })
        })
    }

    pub(crate) fn try_start(
        &self,
        mut bind_texture: impl FnMut(&Path, bool) -> io::Result<BackgroundTexture>,
    ) -> io::Result<BackgroundPlayback> {
        let mut layers = Vec::with_capacity(self.layers.len());
        for layer in &self.layers {
            let repeat = layer.scroll != [0.0, 0.0] || layer.on.repeat_texture().unwrap_or(false);
            let texture = bind_texture(&layer.path, repeat)?;
            let [width, height] = texture.dimensions;
            if texture
                .dimensions
                .iter()
                .any(|size| !size.is_finite() || *size <= 0.0)
            {
                return Err(invalid(
                    &layer.path,
                    "invalid background texture dimensions",
                ));
            }
            let initial = TweenState {
                x: (self.canvas[0] * 0.5).trunc(),
                y: (self.canvas[1] * 0.5).trunc(),
                w: width,
                h: height,
                scale: if layer.stretch {
                    [self.canvas[0] / width, self.canvas[1] / height]
                } else {
                    [1.0, 1.0]
                },
                ..TweenState::default()
            };
            layers.push(PlayingLayer {
                source: texture.source,
                scroll: layer.scroll,
                tween: layer.on.start(initial),
            });
        }
        Ok(BackgroundPlayback {
            minimum_seconds: self.minimum_seconds,
            elapsed_seconds: 0.0,
            layers,
        })
    }
}

impl Layer {
    fn read(
        path: &Path,
        root: &Path,
        directory: &Path,
        section: &str,
        fields: &IniSection,
        canvas: [f32; 2],
    ) -> io::Result<Self> {
        check_supported(
            path,
            section,
            fields,
            &["Condition", "Cond", "DrawCond", "InitCommand"],
        )?;
        let kind = fields.get("Type").map_or("sprite", |value| scalar(value));
        let stretch = match kind.to_ascii_lowercase().as_str() {
            "0" | "sprite" => number(path, section, fields, "Stretch")?.unwrap_or(0.0) != 0.0,
            "1" => true,
            _ => {
                return Err(unsupported(
                    path,
                    format!("[{section}] layer type {kind:?}"),
                ));
            }
        };
        let file = fields
            .get("File")
            .map(|value| scalar(value))
            .filter(|name| !name.is_empty())
            .ok_or_else(|| invalid(path, format!("[{section}] File is required")))?;
        let texture_path = sprite_path(path, root, directory, section, file)?;
        let velocity = |key, legacy| -> io::Result<f32> {
            if let Some(value) = number(path, section, fields, legacy)? {
                return Ok(value);
            }
            Ok(number(path, section, fields, key)?.unwrap_or(0.0))
        };
        let scroll = [
            velocity("TexCoordVelocityX", "StretchTexCoordVelocityX")?,
            velocity("TexCoordVelocityY", "StretchTexCoordVelocityY")?,
        ];
        let commands = fields.get("OnCommand").or_else(|| fields.get("Command"));
        let on = ActorAnimation::parse(commands.map_or("", String::as_str), canvas).map_err(
            |error| {
                let message = format!("[{section}] {error}");
                match error {
                    AnimationError::UnsupportedCommand { .. } => unsupported(path, message),
                    _ => invalid(path, message),
                }
            },
        )?;
        Ok(Self {
            path: texture_path,
            stretch,
            scroll,
            on,
        })
    }
}

fn sprite_path(
    path: &Path,
    root: &Path,
    directory: &Path,
    section: &str,
    file: &str,
) -> io::Result<PathBuf> {
    let relative_to_package = || {
        invalid(
            path,
            format!("[{section}] File must be relative to the theme package"),
        )
    };
    let mut resolved =
        package_relative_path(root, directory, file).ok_or_else(relative_to_package)?;
    let file_error = |error: io::Error| {
        io::Error::new(
            error.kind(),
            format!("reading {} [{section}] File: {error}", path.display()),
        )
    };
    if resolved.extension().is_none() {
        let parent = resolved.parent().ok_or_else(relative_to_package)?;
        let parent = parent.canonicalize().map_err(file_error)?;
        if !parent.starts_with(root) {
            return Err(relative_to_package());
        }
        let name = resolved
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(relative_to_package)?;
        resolved = implicit_sprite(&parent, name).map_err(file_error)?;
    }
    let texture_path = resolved.canonicalize().map_err(file_error)?;
    if !texture_path.starts_with(root) || !texture_path.is_file() {
        return Err(invalid(
            path,
            format!("[{section}] File must resolve to a file within the theme package"),
        ));
    }
    if !sprite_extension(&texture_path) {
        return Err(unsupported(
            path,
            format!("[{section}] sprite file {}", texture_path.display()),
        ));
    }
    Ok(texture_path)
}

fn package_relative_path(root: &Path, directory: &Path, value: &str) -> Option<PathBuf> {
    let value = value.replace('\\', "/");
    let components = Path::new(&value).components();
    if value.contains(':') || !matches!(components.clone().next_back(), Some(Component::Normal(_)))
    {
        return None;
    }
    let mut resolved = directory.to_path_buf();
    for component in components {
        match component {
            Component::Normal(name) => resolved.push(name),
            Component::CurDir => {}
            Component::ParentDir if resolved != root => {
                resolved.pop();
            }
            _ => return None,
        }
    }
    Some(resolved)
}

fn implicit_sprite(parent: &Path, name: &str) -> io::Result<PathBuf> {
    let prefix = name.to_ascii_lowercase();
    let mut matched = None;
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let candidate = entry.path();
        if !entry.file_type()?.is_file() || !sprite_extension(&candidate) {
            continue;
        }
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.to_ascii_lowercase().starts_with(&prefix))
        {
            if matched.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("ambiguous sprite name {name:?}"),
                ));
            }
            matched = Some(candidate);
        }
    }
    matched.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("sprite {name:?} is missing"),
        )
    })
}

fn sprite_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "bmp"
            )
        })
}

impl BackgroundPlayback {
    pub(crate) fn elapsed_seconds(&self) -> f32 {
        self.elapsed_seconds
    }

    pub fn advance(&mut self, delta_seconds: f32) {
        if delta_seconds.is_finite() && delta_seconds > 0.0 {
            self.elapsed_seconds += delta_seconds;
            for layer in &mut self.layers {
                layer.tween.update(delta_seconds);
            }
        }
    }

    /// Whether the declared hold time and the entry animations have elapsed.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.elapsed_seconds >= self.minimum_seconds
            && self.layers.iter().all(|layer| layer.tween.is_empty())
    }

    pub fn push_actors(&self, actors: &mut Vec<Actor>, first_z: i16) {
        actors.reserve(self.layers.len());
        for (index, layer) in self.layers.iter().enumerate() {
            let state = layer.tween.state();
            let mut sprite = SpriteBuilder::with_source(layer.source.clone());
            sprite.apply_tween_state(state);
            sprite.z(first_z.saturating_add(i16::try_from(index).unwrap_or(i16::MAX)));
            if layer.scroll != [0.0, 0.0] {
                let [u, v] = layer.scroll.map(|velocity| velocity * self.elapsed_seconds);
                sprite.customtexturerect([u, v, u + 1.0, v + 1.0]);
            }
            let mut actor = sprite.build(0);
            if let Actor::Sprite { world_z, .. } = &mut actor {
                *world_z = state.z;
            }
            actors.push(actor);
        }
    }
}

fn check_supported(
    path: &Path,
    section: &str,
    fields: &IniSection,
    unsupported_fields: &[&str],
) -> io::Result<()> {
    for field in unsupported_fields {
        if fields
            .get(*field)
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(unsupported(path, format!("[{section}] {field}")));
        }
    }
    Ok(())
}

fn scalar(value: &str) -> &str {
    value
        .split_once("//")
        .map_or(value, |(value, _)| value)
        .trim()
}

fn number(path: &Path, section: &str, fields: &IniSection, key: &str) -> io::Result<Option<f32>> {
    fields
        .get(key)
        .map(|value| {
            scalar(value)
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .ok_or_else(|| invalid(path, format!("[{section}] {key} requires a finite number")))
        })
        .transpose()
}

fn invalid(path: &Path, message: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{}: {message}", path.display()),
    )
}

fn unsupported(path: &Path, message: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{}: {message}", path.display()),
    )
}
