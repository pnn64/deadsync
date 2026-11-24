use std::{
    f32::consts::PI,
    io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use deadlib_assets::{TextureDecodeJob, present_dsl::cover_uv};
use deadlib_present::actors::{Actor, SpriteSource, TextContent};
use deadlib_present::anim::{TweenSeq, TweenState};
use deadlib_present::dsl::SpriteBuilder;
use deadsync_theme::select_music::{SELECT_MUSIC_WHEEL_ROWS, SelectMusicPrompt, SelectMusicView};
use deadsync_theme::selection::{MusicWheelPresentation, WheelMediaFrame};

use crate::bitmap_text::StyledPlayback;
use crate::scene::{CANVAS, SceneTextures, coordinate, frame};
use crate::{
    ActorAnimation, BackgroundPlayback, BackgroundTexture, ResourceCategory, ThemePackage,
};

const SCREEN: &str = "ScreenSelectMusic";
const WHEEL: &str = "MusicWheel";
const ROW: &str = "MusicWheelItem";
const FONT: &str = "local-theme-music-wheel";
const SECTION_FONT: &str = "local-theme-music-section";
const WHITE: [f32; 4] = [1.0; 4];

pub struct PreparedMusic {
    pub presentation: PackageMusic,
    pub fonts: Vec<(&'static str, PathBuf)>,
    pub textures: Vec<TextureDecodeJob>,
}

pub struct PackageMusic {
    wheel: Wheel,
    banner: Banner,
    seats: [Seat; 2],
    difficulty_icons: Option<BackgroundTexture>,
    meter: Option<Meter>,
    background: Option<BackgroundPlayback>,
    prompt: Option<PlacedSprite>,
    bpm: Option<Label>,
    entered: Option<Instant>,
    confirmed: Option<Instant>,
}

struct Wheel {
    pose: Pose,
    rows: usize,
    spacing: f32,
    curve: f32,
    song: BackgroundTexture,
    section: BackgroundTexture,
    expanded: Option<BackgroundTexture>,
    highlight: Option<BackgroundTexture>,
    song_name: Label,
    section_name: Label,
    show_random: bool,
}

struct Banner {
    pose: Pose,
    fallback: Option<BackgroundTexture>,
    frame: Option<PlacedSprite>,
    mask: Option<BackgroundTexture>,
    fade: TweenSeq,
}

struct Seat {
    difficulty_frame: Option<PlacedSprite>,
    icon: Option<Pose>,
    meter_frame: Option<PlacedSprite>,
    meter: Option<Pose>,
}

struct Meter {
    texture: BackgroundTexture,
    count: usize,
    max: usize,
}

struct PlacedSprite {
    texture: BackgroundTexture,
    pose: Pose,
}

struct Label {
    text: StyledPlayback,
    position: [f32; 2],
}

struct Pose {
    on: TweenSeq,
    off: TweenSeq,
}

#[derive(Clone, Copy)]
struct FrameTime {
    entered: f32,
    leaving: f32,
}

struct Metrics<'a> {
    package: &'a ThemePackage,
    section: &'static str,
}

impl PackageMusic {
    pub fn open(root: &Path, language: Option<&str>) -> io::Result<PreparedMusic> {
        let package = ThemePackage::open([root], language)?;
        let mut textures = SceneTextures::new("music");
        let wheel = Wheel::read(&package, &mut textures)?;
        let banner = Banner::read(&package, &mut textures)?;
        let difficulty_icons = textures.graphic(&package, SCREEN, "difficulty icons")?;
        let icon_size = difficulty_icons
            .as_ref()
            .map(|atlas| [atlas.dimensions[0], atlas.dimensions[1] / 6.0]);
        let seats = [
            Seat::read(&package, &mut textures, "p1", "P1", icon_size)?,
            Seat::read(&package, &mut textures, "p2", "P2", icon_size)?,
        ];
        let meter_metrics = Metrics::new(&package, "DifficultyMeter");
        let meter = if meter_metrics.number("ShowFeet")?.unwrap_or_default() != 0.0 {
            textures
                .graphic(&package, "DifficultyMeter", "bar 2x1.png")?
                .map(|texture| -> io::Result<Meter> {
                    Ok(Meter {
                        texture,
                        count: meter_metrics.count("NumFeetInMeter", 64)?,
                        max: meter_metrics.count("MaxFeetInMeter", 64)?,
                    })
                })
                .transpose()?
        } else {
            None
        };
        let background = textures.background(&package, SCREEN)?;
        let prompt = PlacedSprite::read(
            &package,
            &mut textures,
            "options message",
            "OptionsMessage",
            [CANVAS[0] * 0.5, CANVAS[1] * 0.5],
            2.0,
        )?;
        let bpm = Label::read(&Metrics::new(&package, SCREEN), "BPMDisplay")?;
        let mut fonts = Vec::new();
        let general = package
            .screen_resource(ResourceCategory::Fonts, ROW, "text")
            .map_err(io::Error::other)?
            .ok_or_else(|| invalid(ROW, "missing text font"))?;
        fonts.push((FONT, general.to_owned()));
        let section = package
            .screen_resource(ResourceCategory::Fonts, ROW, "section")
            .map_err(io::Error::other)?
            .unwrap_or(general);
        fonts.push((SECTION_FONT, section.to_owned()));
        Ok(PreparedMusic {
            presentation: Self {
                wheel,
                banner,
                seats,
                difficulty_icons,
                meter,
                background,
                prompt,
                bpm,
                entered: None,
                confirmed: None,
            },
            fonts,
            textures: textures.into_jobs(),
        })
    }
}

impl MusicWheelPresentation<Actor> for PackageMusic {
    fn show_random_entries(&self) -> bool {
        self.wheel.show_random
    }

    fn banner_crossfade(&self) -> Option<Duration> {
        let seconds = self.banner.fade.remaining_seconds();
        if seconds > 0.0 {
            Duration::try_from_secs_f32(seconds).ok()
        } else {
            None
        }
    }

    fn player_options_exit_seconds(&self) -> Option<f32> {
        let seconds = self
            .banner
            .pose
            .off
            .remaining_seconds()
            .max(self.wheel.pose.off.remaining_seconds());
        (seconds.is_finite() && seconds > 0.0).then_some(seconds)
    }

    fn enter_wheel(&mut self, now: Instant) {
        self.entered = Some(now);
        self.confirmed = None;
    }

    fn confirm_wheel(&mut self, now: Instant) {
        self.confirmed = Some(now);
    }

    fn cancel_wheel_confirmation(&mut self) {
        self.confirmed = None;
    }

    fn push_music(
        &self,
        actors: &mut Vec<Actor>,
        view: SelectMusicView<'_>,
        media: WheelMediaFrame<'_>,
    ) {
        let time = FrameTime {
            entered: self.entered.map_or(0.0, |at| {
                media.now.saturating_duration_since(at).as_secs_f32()
            }),
            leaving: media.exit_elapsed,
        };
        let mut children = Vec::new();
        if let Some(background) = &self.background {
            let mut playback = background.clone();
            playback.advance(time.entered);
            playback.push_actors(&mut children, -100);
        }
        self.wheel.push(&mut children, &view, time);
        self.banner.push(&mut children, &view, &media, time);
        if let Some((label, song)) = self.bpm.as_ref().zip(view.song) {
            label.push(
                &mut children,
                FONT,
                song.display_bpm,
                [0.0, 0.0],
                time.entered,
                6,
            );
        }
        for (seat, player) in self.seats.iter().zip(view.players) {
            let Some(chart) = player.chart.filter(|_| player.joined) else {
                continue;
            };
            if let Some(frame) = &seat.difficulty_frame {
                frame.push(&mut children, time, 6);
            }
            if let Some((pose, icons)) = seat.icon.as_ref().zip(self.difficulty_icons.as_ref()) {
                let index = difficulty_index(&chart.difficulty);
                let uv = [0.0, index as f32 / 6.0, 1.0, (index + 1) as f32 / 6.0];
                children.push(draw_sprite(
                    icons.source.clone(),
                    &pose.sample(time),
                    7,
                    Some(uv),
                ));
            }
            if let Some((pose, meter)) = seat.meter.as_ref().zip(self.meter.as_ref()) {
                if let Some(frame) = &seat.meter_frame {
                    frame.push(&mut children, time, 6);
                }
                meter.push(&mut children, chart.meter as usize, pose.sample(time));
            }
        }
        if view.prompt != SelectMusicPrompt::Hidden
            && let Some(prompt) = &self.prompt
        {
            let elapsed = self.confirmed.map_or(0.0, |at| {
                media.now.saturating_duration_since(at).as_secs_f32()
            });
            let pose = prompt.pose.sample(FrameTime {
                entered: elapsed,
                leaving: 0.0,
            });
            let top = if view.prompt == SelectMusicPrompt::EnteringOptions {
                0.5
            } else {
                0.0
            };
            children.push(draw_sprite(
                prompt.texture.source.clone(),
                &pose,
                20,
                Some([0.0, top, 1.0, top + 0.5]),
            ));
        }
        actors.push(frame(children));
    }
}

impl Wheel {
    fn read(package: &ThemePackage, textures: &mut SceneTextures) -> io::Result<Self> {
        let metrics = Metrics::new(package, WHEEL);
        if metrics.number("Use3D")?.unwrap_or_default() != 0.0 {
            return Err(invalid(WHEEL, "the preview supports a flat wheel"));
        }
        let geometry = Metrics::new(package, SCREEN);
        let position = geometry
            .position("Wheel")?
            .ok_or_else(|| invalid(SCREEN, "missing WheelX or WheelY"))?;
        let rows = metrics.count("NumWheelItems", SELECT_MUSIC_WHEEL_ROWS)?;
        let spacing = metrics.required("ItemSpacingY")?;
        if spacing <= 0.0 {
            return Err(invalid(WHEEL, "ItemSpacingY must be positive"));
        }
        let curve = if metrics.number("NoCurving")?.unwrap_or_default() != 0.0 {
            0.0
        } else {
            metrics.required("ItemCurveX")?
        };
        let song = textures
            .graphic(package, ROW, "song")?
            .ok_or_else(|| invalid(ROW, "missing song bar"))?;
        let section = textures
            .graphic(package, ROW, "section")?
            .ok_or_else(|| invalid(ROW, "missing section bar"))?;
        let expanded = textures.graphic(package, ROW, "expanded")?;
        let highlight = textures.graphic(package, WHEEL, "highlight")?;
        let labels = Metrics::new(package, ROW);
        let song_name = Label::read(&labels, "SongName")?
            .ok_or_else(|| invalid(ROW, "missing song name position"))?;
        let section_name = Label::read(&labels, "Section")?
            .ok_or_else(|| invalid(ROW, "missing section name position"))?;
        let show_random = metrics.number("ShowRandom")?.unwrap_or_default() != 0.0
            || metrics.number("ShowRoulette")?.unwrap_or_default() != 0.0;
        Ok(Self {
            pose: Pose::read(&geometry, "Wheel", position, [0.0; 2])?,
            rows,
            spacing,
            curve,
            song,
            section,
            expanded,
            highlight,
            song_name,
            section_name,
            show_random,
        })
    }

    fn push(&self, actors: &mut Vec<Actor>, view: &SelectMusicView<'_>, time: FrameTime) {
        let base = self.pose.sample(time);
        if !base.visible {
            return;
        }
        let middle = SELECT_MUSIC_WHEEL_ROWS / 2;
        for (index, row) in view.rows.iter().enumerate() {
            let Some(row) = row else {
                continue;
            };
            let relative = index as f32 - middle as f32 + view.wheel_offset;
            if relative.abs() > self.rows as f32 * 0.5 {
                continue;
            }
            let x = base.x + ((1.0 - (relative / PI).cos()) * self.curve).round();
            let y = base.y + (relative * self.spacing).round();
            let (bar, name, font) = if row.song_count.is_some() {
                let bar = if row.expanded {
                    self.expanded.as_ref().unwrap_or(&self.section)
                } else {
                    &self.section
                };
                (bar, &self.section_name, SECTION_FONT)
            } else {
                (&self.song, &self.song_name, FONT)
            };
            let state = TweenState {
                x,
                y,
                w: bar.dimensions[0],
                h: bar.dimensions[1],
                ..base
            };
            actors.push(draw_sprite(bar.source.clone(), &state, 0, None));
            name.push(actors, font, row.title, [x, y], time.entered, 1);
        }
        if let Some(highlight) = &self.highlight {
            let state = TweenState {
                w: highlight.dimensions[0],
                h: highlight.dimensions[1],
                ..base
            };
            actors.push(draw_sprite(highlight.source.clone(), &state, 2, None));
        }
    }
}

impl Banner {
    fn read(package: &ThemePackage, textures: &mut SceneTextures) -> io::Result<Self> {
        let metrics = Metrics::new(package, SCREEN);
        let position = metrics
            .position("Banner")?
            .ok_or_else(|| invalid(SCREEN, "missing BannerX or BannerY"))?;
        let size = [
            metrics.required("BannerWidth")?,
            metrics.required("BannerHeight")?,
        ];
        if size.iter().any(|part| *part <= 0.0) {
            return Err(invalid(SCREEN, "banner sizes must be positive"));
        }
        let banner_frame = PlacedSprite::read(
            package,
            textures,
            "banner frame",
            "BannerFrame",
            position,
            1.0,
        )?;
        let mask = textures.graphic(package, SCREEN, "banner mask")?;
        let fallback = textures.graphic(package, "Common", "fallback banner")?;
        let fade = Metrics::new(package, "FadingBanner")
            .motion("BannerFadeOffCommand", TweenState::default())?;
        Ok(Self {
            pose: Pose::read(&metrics, "Banner", position, size)?,
            fallback,
            frame: banner_frame,
            mask,
            fade,
        })
    }

    fn push(
        &self,
        actors: &mut Vec<Actor>,
        view: &SelectMusicView<'_>,
        media: &WheelMediaFrame<'_>,
        time: FrameTime,
    ) {
        let state = self.pose.sample(time);
        let current = view
            .banner_texture
            .map(|key| SpriteSource::Texture(key.clone()))
            .or_else(|| {
                self.fallback
                    .as_ref()
                    .map(|fallback| fallback.source.clone())
            });
        if let Some(source) = current {
            let masked = self.mask.is_some();
            if let Some(mask) = &self.mask {
                let mask_state = TweenState {
                    w: mask.dimensions[0],
                    h: mask.dimensions[1],
                    ..state
                };
                let mut sprite = SpriteBuilder::with_source(mask.source.clone());
                sprite.apply_tween_state(&mask_state);
                sprite.mask_source();
                sprite.z(2);
                actors.push(sprite.build(0));
            }
            let push_image =
                |actors: &mut Vec<Actor>, source: SpriteSource, state: TweenState, z| {
                    let uv = source.texture_key().and_then(|key| {
                        cover_uv(key, state.w * state.scale[0], state.h * state.scale[1])
                    });
                    let mut sprite = SpriteBuilder::with_source(source);
                    sprite.apply_tween_state(&state);
                    if masked {
                        sprite.mask_dest();
                    }
                    if let Some(uv) = uv {
                        sprite.customtexturerect(uv);
                    }
                    sprite.z(z);
                    actors.push(sprite.build(0));
                };
            push_image(actors, source, state, 3);
            for outgoing in media.outgoing.iter().flatten().rev() {
                let mut fade = self.fade.clone();
                fade.update(outgoing.elapsed_seconds);
                let mut state = state;
                state.tint[3] *= fade.state().tint[3];
                push_image(
                    actors,
                    SpriteSource::Texture(outgoing.texture.clone()),
                    state,
                    4,
                );
            }
        }
        if let Some(frame) = &self.frame {
            frame.push(actors, time, 5);
        }
    }
}

impl Seat {
    fn read(
        package: &ThemePackage,
        textures: &mut SceneTextures,
        file_suffix: &str,
        metric_suffix: &str,
        icon_size: Option<[f32; 2]>,
    ) -> io::Result<Self> {
        let metrics = Metrics::new(package, SCREEN);
        let icon_key = format!("DifficultyIcon{metric_suffix}");
        let icon = optional_pose(&metrics, &icon_key, icon_size)?;
        let meter = optional_pose(&metrics, &format!("Meter{metric_suffix}"), Some([0.0; 2]))?;
        let difficulty_frame = PlacedSprite::read(
            package,
            textures,
            &format!("difficulty frame {file_suffix}"),
            &format!("DifficultyFrame{metric_suffix}"),
            [0.0; 2],
            1.0,
        )?;
        let meter_frame = PlacedSprite::read(
            package,
            textures,
            &format!("meter frame {file_suffix}"),
            &format!("MeterFrame{metric_suffix}"),
            [0.0; 2],
            1.0,
        )?;
        Ok(Self {
            difficulty_frame,
            icon,
            meter_frame,
            meter,
        })
    }
}

impl Meter {
    fn push(&self, actors: &mut Vec<Actor>, points: usize, state: TweenState) {
        let visible = points.min(self.max);
        let count = self.count.max(visible);
        let w = self.texture.dimensions[0] * 0.5;
        let h = self.texture.dimensions[1];
        let step = w * state.scale[0];
        let start = state.x - step * (count as f32 - 1.0) * 0.5;
        for index in 0..count {
            let u = if index < visible { 0.0 } else { 0.5 };
            let state = TweenState {
                x: step.mul_add(index as f32, start),
                w,
                h,
                ..state
            };
            actors.push(draw_sprite(
                self.texture.source.clone(),
                &state,
                7,
                Some([u, 0.0, u + 0.5, 1.0]),
            ));
        }
    }
}

impl PlacedSprite {
    fn read(
        package: &ThemePackage,
        textures: &mut SceneTextures,
        file: &str,
        key: &str,
        fallback: [f32; 2],
        rows: f32,
    ) -> io::Result<Option<Self>> {
        textures
            .graphic(package, SCREEN, file)?
            .map(|texture| {
                let metrics = Metrics::new(package, SCREEN);
                let [width, height] = texture.dimensions;
                let pose = Pose::read(
                    &metrics,
                    key,
                    metrics.position(key)?.unwrap_or(fallback),
                    [width, height / rows],
                )?;
                Ok(Self { texture, pose })
            })
            .transpose()
    }

    fn push(&self, actors: &mut Vec<Actor>, time: FrameTime, z: i16) {
        actors.push(draw_sprite(
            self.texture.source.clone(),
            &self.pose.sample(time),
            z,
            None,
        ));
    }
}

impl Pose {
    fn read(
        metrics: &Metrics<'_>,
        key: &str,
        [x, y]: [f32; 2],
        [w, h]: [f32; 2],
    ) -> io::Result<Self> {
        let initial = TweenState {
            x,
            y,
            w,
            h,
            ..Default::default()
        };
        let on = metrics.motion(&format!("{key}OnCommand"), initial)?;
        let mut settled = on.clone();
        settled.update(on.remaining_seconds());
        Ok(Self {
            on,
            off: metrics.motion(&format!("{key}OffCommand"), *settled.state())?,
        })
    }

    fn sample(&self, time: FrameTime) -> TweenState {
        let (animation, elapsed) = if time.leaving > 0.0 {
            (&self.off, time.leaving)
        } else {
            (&self.on, time.entered)
        };
        let mut animation = animation.clone();
        animation.update(elapsed);
        *animation.state()
    }
}

impl Label {
    fn read(metrics: &Metrics<'_>, key: &str) -> io::Result<Option<Self>> {
        let Some(position) = metrics.position(key)? else {
            return Ok(None);
        };
        let commands = metrics
            .value(&format!("{key}OnCommand"))?
            .unwrap_or_default();
        let text = StyledPlayback::styled(TweenState::default(), commands)
            .map_err(|error| invalid(metrics.section, error))?;
        Ok(Some(Self { text, position }))
    }

    fn push(
        &self,
        actors: &mut Vec<Actor>,
        font: &'static str,
        value: &str,
        origin: [f32; 2],
        elapsed: f32,
        z: i16,
    ) {
        let mut text = self.text.clone();
        text.advance(elapsed);
        let at = [origin[0] + self.position[0], origin[1] + self.position[1]];
        let content =
            TextContent::inline_str(value).unwrap_or_else(|| TextContent::Owned(value.to_owned()));
        if let Some(mut actor) = text.actor(font, content, at, WHITE) {
            if let Actor::Text { z: layer, .. } = &mut actor {
                *layer = z;
            }
            actors.push(actor);
        }
    }
}

impl<'a> Metrics<'a> {
    const fn new(package: &'a ThemePackage, section: &'static str) -> Self {
        Self { package, section }
    }
    fn value(&self, key: &str) -> io::Result<Option<&'a str>> {
        self.package
            .metric(self.section, key)
            .map_err(io::Error::other)
    }
    fn number(&self, key: &str) -> io::Result<Option<f32>> {
        self.value(key)?
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                coordinate(value).map_err(|error| invalid(self.section, format!("{key}: {error}")))
            })
            .transpose()
    }
    fn required(&self, key: &str) -> io::Result<f32> {
        self.number(key)?
            .ok_or_else(|| invalid(self.section, format!("missing {key}")))
    }
    fn count(&self, key: &str, max: usize) -> io::Result<usize> {
        let value = self.required(key)?;
        if !(1.0..=max as f32).contains(&value) || value.fract() != 0.0 {
            return Err(invalid(
                self.section,
                format!("{key} must be a whole number between 1 and {max}"),
            ));
        }
        Ok(value as usize)
    }
    fn position(&self, key: &str) -> io::Result<Option<[f32; 2]>> {
        Ok(self
            .number(&format!("{key}X"))?
            .zip(self.number(&format!("{key}Y"))?)
            .map(|(x, y)| [x, y]))
    }
    fn motion(&self, key: &str, state: TweenState) -> io::Result<TweenSeq> {
        let commands = self.value(key)?.unwrap_or_default();
        ActorAnimation::parse(commands, CANVAS)
            .map(|animation| animation.start(state))
            .map_err(|error| invalid(self.section, format!("{key}: {error}")))
    }
}

fn optional_pose(
    metrics: &Metrics<'_>,
    key: &str,
    size: Option<[f32; 2]>,
) -> io::Result<Option<Pose>> {
    metrics
        .position(key)?
        .zip(size)
        .map(|(position, size)| Pose::read(metrics, key, position, size))
        .transpose()
}

fn draw_sprite(source: SpriteSource, state: &TweenState, z: i16, uv: Option<[f32; 4]>) -> Actor {
    let mut sprite = SpriteBuilder::with_source(source);
    sprite.apply_tween_state(state);
    sprite.z(z);
    if let Some(uv) = uv {
        sprite.customtexturerect(uv);
    }
    sprite.build(0)
}

fn difficulty_index(name: &str) -> usize {
    ["Beginner", "Easy", "Medium", "Hard", "Challenge", "Edit"]
        .iter()
        .position(|level| level.eq_ignore_ascii_case(name))
        .unwrap_or(5)
}

fn invalid(section: &str, detail: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("[{section}] {detail}"))
}
