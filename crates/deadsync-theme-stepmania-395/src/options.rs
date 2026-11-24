use std::{
    io,
    path::{Path, PathBuf},
};

use deadlib_assets::{AssetManager, TextureDecodeJob};
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::anim::{TweenSeq, TweenState};
use deadlib_present::dsl::SpriteBuilder;
use deadlib_present::font::measure_line_width_logical;
use deadsync_theme::system_options::{SystemOptionsLayout, SystemOptionsPresentation};
use deadsync_theme::views::{
    SystemOptionsPage, SystemOptionsPageKind, SystemOptionsPhase, SystemOptionsRenderer,
    SystemOptionsRow, SystemOptionsTiming,
};

use crate::animation::numbers;
use crate::bitmap_text::TextPlayback;
use crate::scene::{CANVAS, SceneTextures, coordinate, frame};
use crate::{
    ActorAnimation, BackgroundPlayback, BackgroundTexture, ResourceCategory, ThemePackage,
};

mod player;
pub use player::{PackagePlayerOptions, PreparedPlayerOptions};

const MENU: &str = "ScreenOptionsMenu";
const SUBMENU: &str = "ScreenOptionsSubmenu";
const OPTIONS: &str = "ScreenOptions";
const LABEL_FONT: &str = "local-theme-options-label";
const ITEM_FONT: &str = "local-theme-options-item";
const HELP_FONT: &str = "local-theme-options-explanation";
const WHITE: [f32; 4] = [1.0; 4];

fn font_paths(
    package: &ThemePackage,
    names: [&'static str; 3],
    show_help: bool,
) -> io::Result<Vec<(&'static str, PathBuf)>> {
    let mut fonts = Vec::new();
    for (name, resource) in names.into_iter().zip(["title", "item", "explanation"]) {
        if resource == "explanation" && !show_help {
            continue;
        }
        let font = package
            .screen_resource(ResourceCategory::Fonts, OPTIONS, resource)
            .map_err(io::Error::other)?
            .ok_or_else(|| invalid(OPTIONS, format!("missing {resource} font")))?;
        fonts.push((name, font.to_owned()));
    }
    Ok(fonts)
}

pub struct PreparedOptions {
    pub presentation: PackageOptions,
    pub fonts: Vec<(&'static str, PathBuf)>,
    pub textures: Vec<TextureDecodeJob>,
}

pub struct PackageOptions {
    menu: OptionsPageStyle,
    submenu: OptionsPageStyle,
    background: Option<(BackgroundPlayback, BackgroundPlayback)>,
    last_frame: Option<f64>,
    elapsed: f32,
    page_start: Option<(usize, f32)>,
}

struct OptionsPageStyle {
    row_y: Vec<f32>,
    done_y: Option<f32>,
    label_x: f32,
    value_x: [f32; 2],
    long_value_x: Option<f32>,
    item_gap: f32,
    heading: Option<[f32; 2]>,
    label: TextPlayback,
    item: TextPlayback,
    help: Option<([f32; 2], TextPlayback)>,
    colors: [[f32; 4]; 3],
    seconds: Option<f32>,
    enter: TweenSeq,
    leave: TweenSeq,
    panel: Option<Panel>,
    highlight: Option<BackgroundTexture>,
}

struct Panel {
    texture: BackgroundTexture,
    on: TweenSeq,
}

impl PackageOptions {
    pub fn open(root: &Path, language: Option<&str>) -> io::Result<PreparedOptions> {
        let package = ThemePackage::open([root], language)?;
        let mut textures = SceneTextures::new("options");
        let menu = OptionsPageStyle::read(&package, MENU, &mut textures)?;
        let submenu = OptionsPageStyle::read(&package, SUBMENU, &mut textures)?;
        let background = textures
            .background(&package, MENU)?
            .map(|current| (current.clone(), current));
        let fonts = font_paths(
            &package,
            [LABEL_FONT, ITEM_FONT, HELP_FONT],
            menu.help.is_some() || submenu.help.is_some(),
        )?;
        Ok(PreparedOptions {
            presentation: Self {
                menu,
                submenu,
                background,
                last_frame: None,
                elapsed: 0.0,
                page_start: None,
            },
            fonts,
            textures: textures.into_jobs(),
        })
    }
}

impl SystemOptionsPresentation<Actor> for PackageOptions {
    fn layout(&self) -> SystemOptionsLayout {
        SystemOptionsLayout {
            timing: self.menu.seconds.zip(self.submenu.seconds).map(
                |(service_seconds, settings_seconds)| SystemOptionsTiming {
                    service_seconds,
                    settings_seconds,
                },
            ),
            ..Default::default()
        }
    }

    fn reset(&mut self) {
        self.last_frame = None;
        self.elapsed = 0.0;
        self.page_start = None;
        if let Some((initial, current)) = &mut self.background {
            *current = initial.clone();
        }
    }

    fn with_renderer(
        &mut self,
        phase: SystemOptionsPhase,
        now_seconds: f64,
        assets: &AssetManager,
        draw: &mut dyn FnMut(SystemOptionsRenderer<'_, Actor>),
    ) {
        if now_seconds.is_finite() {
            let delta = self
                .last_frame
                .replace(now_seconds)
                .map_or(0.0, |last| (now_seconds - last).max(0.0) as f32);
            if delta.is_finite() {
                self.elapsed += delta;
                if let Some((_, current)) = &mut self.background {
                    current.advance(delta);
                }
            }
        }
        let background = |actors: &mut Vec<Actor>| {
            if let Some((_, current)) = &self.background {
                let mut layers = Vec::new();
                current.push_actors(&mut layers, -100);
                actors.push(frame(layers));
            }
        };
        let mut page = |actors: &mut Vec<Actor>, page: SystemOptionsPage<'_>| {
            let started = match self.page_start {
                Some((id, started)) if id == page.id => started,
                _ => {
                    let entered = match page.phase {
                        SystemOptionsPhase::Entering(time) => time,
                        _ => 0.0,
                    };
                    let started = self.elapsed - entered;
                    self.page_start = Some((page.id, started));
                    started
                }
            };
            let style = if page.kind == SystemOptionsPageKind::ServiceMenu {
                &self.menu
            } else {
                &self.submenu
            };
            style.push(actors, page, (self.elapsed - started).max(0.0), assets);
        };
        draw(SystemOptionsRenderer {
            phase,
            background: &background,
            page: &mut page,
        });
    }
}

impl OptionsPageStyle {
    fn read(
        package: &ThemePackage,
        section: &str,
        textures: &mut SceneTextures,
    ) -> io::Result<Self> {
        let metric = |key: &str| option_metric(package, section, key);
        let number = |key: &str| option_number(package, section, key);
        let required =
            |key: &str| number(key)?.ok_or_else(|| invalid(section, format!("missing {key}")));
        let color = |key: &str| {
            let value = metric(key)?.ok_or_else(|| invalid(section, format!("missing {key}")))?;
            numbers(&value.split(',').collect::<Vec<_>>())
                .ok_or_else(|| invalid(section, format!("invalid {key}")))
        };
        let text_style = |key: &str, state: TweenState| {
            TextPlayback::styled(state, metric(key)?.unwrap_or_default())
                .map_err(|error| invalid(section, format!("{key}: {error}")))
        };
        let motion = |key: &str, state: TweenState| {
            ActorAnimation::parse(metric(key)?.unwrap_or_default(), CANVAS)
                .map(|animation| animation.start(state))
                .map_err(|error| invalid(section, format!("{key}: {error}")))
        };
        let rows = required("NumRowsShown")?;
        if !(1.0..=CANVAS[1]).contains(&rows) || rows.fract() != 0.0 {
            return Err(invalid(
                section,
                "NumRowsShown must be a positive whole number within the canvas height",
            ));
        }
        let row_y = (1..=rows as usize)
            .map(|row| required(&format!("Row{row}Y")))
            .collect::<io::Result<Vec<_>>>()?;
        let done_y = if number("SeparateExitRow")?.unwrap_or(0.0) != 0.0 {
            Some(required("SeparateExitRowY")?)
        } else {
            None
        };
        let value_x = [required("ItemsStartX")?, required("ItemsEndX")?];
        let item_gap = required("ItemsGapX")?;
        if value_x[0] >= value_x[1] || item_gap < 0.0 {
            return Err(invalid(section, "invalid item column or gap"));
        }
        let help = if number("ShowExplanations")?.unwrap_or(0.0) != 0.0 {
            let position = [
                required("ExplanationTogetherX")?,
                required("ExplanationTogetherY")?,
            ];
            let scale = required("ExplanationZoom")?;
            let style = text_style(
                "ExplanationTogetherOnCommand",
                TweenState {
                    scale: [scale; 2],
                    ..Default::default()
                },
            )?;
            Some((position, style))
        } else {
            None
        };
        let read_graphic =
            |textures: &mut SceneTextures, name| -> io::Result<Option<BackgroundTexture>> {
                match textures.graphic(package, section, name)? {
                    Some(texture) => Ok(Some(texture)),
                    None => textures.graphic(package, OPTIONS, name),
                }
            };
        let panel = read_graphic(textures, "page")?
            .map(|texture| -> io::Result<Panel> {
                let [w, h] = texture.dimensions;
                let on = motion(
                    "PageOnCommand",
                    TweenState {
                        x: required("PageX")?,
                        y: required("PageY")?,
                        w,
                        h,
                        ..Default::default()
                    },
                )?;
                Ok(Panel { texture, on })
            })
            .transpose()?;
        Ok(Self {
            row_y,
            done_y,
            label_x: required("LabelsX")?,
            value_x,
            long_value_x: number("ItemsLongRowSharedX")?,
            item_gap,
            heading: number("HeaderX")?
                .zip(number("HeaderY")?)
                .map(|(x, y)| [x, y]),
            label: text_style("LabelsOnCommand", TweenState::default())?,
            item: text_style(
                "ItemsOnCommand",
                TweenState {
                    hx: 0.0,
                    ..Default::default()
                },
            )?,
            help,
            colors: [
                color("ColorSelected")?,
                color("ColorNotSelected")?,
                color("ColorDisabled")?,
            ],
            seconds: number("TweenSeconds")?.filter(|seconds| *seconds > 0.0),
            enter: motion("FrameOnCommand", TweenState::default())?,
            leave: motion("FrameOffCommand", TweenState::default())?,
            panel,
            highlight: read_graphic(textures, "line highlight")?,
        })
    }

    fn push(
        &self,
        actors: &mut Vec<Actor>,
        page: SystemOptionsPage<'_>,
        elapsed: f32,
        assets: &AssetManager,
    ) {
        let mut children = Vec::new();
        self.push_chrome(&mut children, LABEL_FONT, page.title, elapsed, 1.0);
        let mut labels = self.label.clone();
        let mut items = self.item.clone();
        labels.advance(elapsed);
        items.advance(elapsed);
        let total = page.row_count + usize::from(self.done_y.is_none());
        let first = page
            .selected
            .saturating_sub(self.row_y.len() / 2)
            .min(total.saturating_sub(self.row_y.len()));
        let mut push_row = |index, y| {
            let focused = index == page.selected;
            if focused {
                self.push_highlight(&mut children, y, 1.0);
            }
            let row = if index < page.row_count {
                (page.row)(index)
            } else {
                None
            };
            let (label, disabled) = if index == page.row_count {
                (TextContent::Static("Done"), false)
            } else if let Some(row) = &row {
                (TextContent::Shared(row.label.clone()), row.disabled)
            } else {
                return;
            };
            let color = self.color(focused, disabled);
            children.extend(labels.actor(LABEL_FONT, label, [self.label_x, y], color));
            if let Some(row) = row {
                self.push_choices(&mut children, &items, &row, y, focused, assets);
            }
        };
        for (index, &y) in (first..total).zip(&self.row_y) {
            push_row(index, y);
        }
        if let Some(y) = self.done_y {
            push_row(page.row_count, y);
        }
        if let Some((at, style)) = &self.help
            && let Some(description) = page.description
        {
            let mut style = style.clone();
            style.advance(elapsed);
            let description = if let Some(detail) = page.detail {
                TextContent::Owned(format!("{description}\n{detail}"))
            } else {
                TextContent::Shared(description)
            };
            children.extend(style.actor(HELP_FONT, description, *at, WHITE));
        }
        actors.push(page_frame(children, self.frame_state(page.phase)));
    }

    fn push_chrome(
        &self,
        actors: &mut Vec<Actor>,
        font: &'static str,
        title: &'static str,
        elapsed: f32,
        alpha: f32,
    ) {
        if let Some(panel) = &self.panel {
            let mut on = panel.on.clone();
            on.update(elapsed);
            let mut actor = sprite(&panel.texture, on.state(), -10);
            actor.mul_alpha(alpha);
            actors.push(actor);
        }
        if let Some(heading) = self.heading {
            let style = TextPlayback::new(TweenState::default());
            actors.extend(style.actor(
                font,
                TextContent::Static(title),
                heading,
                [1.0, 1.0, 1.0, alpha],
            ));
        }
    }

    fn push_highlight(&self, actors: &mut Vec<Actor>, y: f32, alpha: f32) {
        if let Some(highlight) = &self.highlight {
            let [w, h] = highlight.dimensions;
            let state = TweenState {
                x: CANVAS[0] * 0.5,
                y,
                w,
                h,
                tint: [1.0, 1.0, 1.0, alpha],
                ..Default::default()
            };
            actors.push(sprite(highlight, &state, -1));
        }
    }

    fn frame_state(&self, phase: SystemOptionsPhase) -> TweenState {
        let (motion, time) = match phase {
            SystemOptionsPhase::Entering(time) => (&self.enter, time),
            SystemOptionsPhase::Leaving(time) => (&self.leave, time),
            SystemOptionsPhase::Steady => return TweenState::default(),
        };
        let mut motion = motion.clone();
        motion.update(time);
        *motion.state()
    }

    fn push_choices(
        &self,
        actors: &mut Vec<Actor>,
        style: &TextPlayback,
        row: &SystemOptionsRow,
        y: f32,
        focused: bool,
        assets: &AssetManager,
    ) {
        let Some(choices) = &row.choices else {
            return;
        };
        let span = self.value_x[1] - self.value_x[0];
        let fonts = assets.fonts();
        let font = fonts.get(ITEM_FONT);
        let width = |text: &str| {
            font.map_or(0.0, |font| {
                measure_line_width_logical(font, text, fonts) as f32 * style.scale_x().abs()
            })
        };
        let inline = row.inline
            && font.is_some()
            && choices.iter().map(|text| width(text)).sum::<f32>()
                + self.item_gap * choices.len().saturating_sub(1) as f32
                <= span;
        let long_x = self.long_value_x.filter(|_| !inline);
        let mut x = long_x.unwrap_or(self.value_x[0]);
        for (index, choice) in choices.iter().enumerate() {
            if !inline && index != row.selected_choice {
                continue;
            }
            let selected = index == row.selected_choice;
            let enabled = row.enabled_choices.map_or(selected, |bits| {
                index < u64::BITS as usize && bits & (1 << index) != 0
            });
            let mut color = self.color(focused && selected || enabled, row.disabled);
            if selected
                && !row.disabled
                && let Some(tint) = row.choice_tint
            {
                color = std::array::from_fn(|channel| color[channel] * tint[channel]);
            }
            if let Some(mut actor) = style.actor(
                ITEM_FONT,
                TextContent::Shared(choice.clone()),
                [x, y],
                color,
            ) {
                if !inline
                    && let Actor::Text {
                        align, max_width, ..
                    } = &mut actor
                {
                    if let Some(center) = long_x {
                        align[0] = 0.5;
                        let half_width = (center - self.value_x[0]).min(self.value_x[1] - center);
                        *max_width = Some(2.0 * half_width.max(0.0));
                    } else {
                        *max_width = Some(span);
                    }
                }
                actors.push(actor);
            }
            x += width(choice) + self.item_gap;
        }
    }

    fn color(&self, focused: bool, disabled: bool) -> [f32; 4] {
        self.colors[if disabled { 2 } else { usize::from(!focused) }]
    }
}

fn sprite(texture: &BackgroundTexture, state: &TweenState, z: i16) -> Actor {
    let mut builder = SpriteBuilder::with_source(texture.source.clone());
    builder.apply_tween_state(state);
    builder.z(z);
    builder.build(0)
}

fn page_frame(children: Vec<Actor>, motion: TweenState) -> Actor {
    let mut frame = frame(children);
    if let Actor::Frame { offset, .. } = &mut frame {
        offset[0] += motion.x;
        offset[1] += motion.y;
    }
    if motion.tint[3] != 1.0 {
        frame.mul_alpha(motion.tint[3]);
    }
    frame
}

fn option_metric<'a>(
    package: &'a ThemePackage,
    section: &str,
    key: &str,
) -> io::Result<Option<&'a str>> {
    if let Some(value) = package.metric(section, key).map_err(io::Error::other)? {
        return Ok(Some(value));
    }
    package.metric(OPTIONS, key).map_err(io::Error::other)
}

fn option_number(package: &ThemePackage, section: &str, key: &str) -> io::Result<Option<f32>> {
    option_metric(package, section, key)?
        .filter(|value| !value.trim().is_empty())
        .map(|value| coordinate(value).map_err(|error| invalid(section, format!("{key}: {error}"))))
        .transpose()
}

fn invalid(section: &str, detail: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("[{section}] {detail}"))
}
