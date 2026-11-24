use std::{
    io,
    path::{Path, PathBuf},
    rc::Rc,
};

use deadlib_assets::{AssetManager, TextureDecodeJob, parse_sprite_sheet_dims};
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::anim::TweenState;
use deadlib_present::dsl::SpriteBuilder;
use deadlib_present::font::{FontMap, measure_line_width_logical};
use deadlib_present::space::{screen_center_x, screen_center_y};
use deadsync_theme::player_options::{
    PlayerOptionsLayout, PlayerOptionsPage, PlayerOptionsRow, PlayerOptionsViewport,
};
use deadsync_theme::{Screen, TransitionState};

use super::{
    CANVAS, OPTIONS, OptionsPageStyle, SystemOptionsPhase, TextPlayback, font_paths, frame,
    invalid, option_metric, option_number, page_frame,
};
use crate::scene::SceneTextures;
use crate::{BackgroundPlayback, BackgroundTexture, ResourceCategory, ThemePackage};

const SECTION: &str = "ScreenPlayerOptions";
const LABEL: &str = "local-theme-player-options-label";
const ITEM: &str = "local-theme-player-options-item";
const HELP: &str = "local-theme-player-options-explanation";
type Page<'a> = PlayerOptionsPage<'a, TextContent, Actor>;
type Help = ([f32; 2], TextPlayback);

pub struct PreparedPlayerOptions {
    pub presentation: PackagePlayerOptions,
    pub fonts: Vec<(&'static str, PathBuf)>,
    pub textures: Vec<TextureDecodeJob>,
}

#[derive(Clone)]
pub struct PackagePlayerOptions {
    style: Rc<PlayerStyle>,
    background: Option<BackgroundPlayback>,
    last_frame: Option<f64>,
    pane: Option<&'static str>,
    pane_elapsed: f32,
}

struct PlayerStyle {
    page: OptionsPageStyle,
    seats: [f32; 2],
    previews: [Option<f32>; 2],
    help: [Option<Help>; 2],
    offscreen_y: [f32; 2],
    cursor: Option<(BackgroundTexture, bool)>,
}

struct PlayerValues<'a> {
    item: &'a TextPlayback,
    font: &'a FontMap,
    active: [bool; 2],
    seats: [f32; 2],
}

impl PackagePlayerOptions {
    pub fn open(root: &Path, language: Option<&str>) -> io::Result<PreparedPlayerOptions> {
        let package = ThemePackage::open([root], language)?;
        let mut textures = SceneTextures::new("player-options");
        let page = OptionsPageStyle::read(&package, SECTION, &mut textures)?;
        if !(4..=u8::MAX as usize).contains(&page.row_y.len()) {
            return Err(invalid(
                SECTION,
                "the editor viewport supports 4 to 255 row slots",
            ));
        }
        let number = |key: &str| option_number(&package, SECTION, key);
        let [left, right] = page.value_x;
        let seats = [
            number("ItemsLongRowP1X")?.unwrap_or(left + (right - left) * 0.25),
            number("ItemsLongRowP2X")?.unwrap_or(left + (right - left) * 0.75),
        ];
        let seat_help = |seat: &str, anchor| -> io::Result<Option<Help>> {
            let Some((_, shared_style)) = &page.help else {
                return Ok(None);
            };
            let x = number(&format!("Explanation{seat}X"))?;
            let y = number(&format!("Explanation{seat}Y"))?;
            let command_key = format!("Explanation{seat}OnCommand");
            x.zip(y)
                .map(|(x, y)| {
                    let scale = shared_style.scale_x();
                    let state = TweenState {
                        scale: [scale; 2],
                        hx: anchor,
                        ..Default::default()
                    };
                    let commands =
                        option_metric(&package, SECTION, &command_key)?.unwrap_or_default();
                    let style = TextPlayback::styled(state, commands)
                        .map_err(|error| invalid(SECTION, format!("{command_key}: {error}")))?;
                    Ok(([x, y], style))
                })
                .transpose()
        };
        let help = [seat_help("P1", 0.0)?, seat_help("P2", 1.0)?];
        let rows = &page.row_y;
        let last = rows.len() - 1;
        let offscreen_y = [
            number("RowYOffScreenTop")?.unwrap_or(rows[0] - (rows[1] - rows[0]) * 0.5),
            number("RowYOffScreenBottom")?
                .unwrap_or(rows[last] + (rows[last] - rows[last - 1]) * 0.5),
        ];
        let cursor_path = package
            .screen_resource(ResourceCategory::Graphics, SECTION, "cursor")
            .map_err(io::Error::other)?;
        let cursor_path = if cursor_path.is_some() {
            cursor_path
        } else {
            package
                .screen_resource(ResourceCategory::Graphics, OPTIONS, "cursor")
                .map_err(io::Error::other)?
        };
        let cursor = cursor_path
            .map(|path| -> io::Result<_> {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let atlas = match parse_sprite_sheet_dims(&name) {
                    (3, 2) => true,
                    (1, 1) => false,
                    _ => {
                        return Err(invalid(
                            SECTION,
                            "cursor image needs one frame or a 3x2 sheet",
                        ));
                    }
                };
                Ok((textures.bind(path, false)?, atlas))
            })
            .transpose()?;
        let background = textures.background(&package, SECTION)?;
        let fonts = font_paths(&package, [LABEL, ITEM, HELP], page.help.is_some())?;
        Ok(PreparedPlayerOptions {
            presentation: Self {
                style: Rc::new(PlayerStyle {
                    page,
                    seats,
                    previews: [number("IconsP1X")?, number("IconsP2X")?],
                    help,
                    offscreen_y,
                    cursor,
                }),
                background,
                last_frame: None,
                pane: None,
                pane_elapsed: 0.0,
            },
            fonts,
            textures: textures.into_jobs(),
        })
    }
}

impl PlayerOptionsLayout<Actor, TextContent> for PackagePlayerOptions {
    fn viewport(&self) -> PlayerOptionsViewport {
        PlayerOptionsViewport {
            slots: self.style.page.row_y.len() as u8,
            separate_exit: self.style.page.done_y.is_some(),
            row_tween_seconds: self.style.page.seconds.unwrap_or_default(),
        }
    }

    fn push_page(
        &mut self,
        actors: &mut Vec<Actor>,
        page: Page<'_>,
        assets: &AssetManager,
        now: f64,
        transition: &TransitionState<Screen>,
    ) {
        let phase = match *transition {
            TransitionState::ActorsFadeIn { elapsed, .. }
            | TransitionState::FadingIn { elapsed, .. } => SystemOptionsPhase::Entering(elapsed),
            TransitionState::ActorsFadeOut { elapsed, .. }
            | TransitionState::FadingOut { elapsed, .. } => SystemOptionsPhase::Leaving(elapsed),
            TransitionState::Idle => SystemOptionsPhase::Steady,
        };
        let delta = if now.is_finite() {
            self.last_frame
                .replace(now)
                .map_or(0.0, |last| (now - last).max(0.0) as f32)
        } else {
            0.0
        };
        if self.pane != Some(page.title) {
            self.pane = Some(page.title);
            self.pane_elapsed = match phase {
                SystemOptionsPhase::Entering(time) => time,
                _ => 0.0,
            };
        } else if delta.is_finite() {
            self.pane_elapsed += delta;
        }
        if let Some(background) = &mut self.background {
            background.advance(delta);
            let mut layers = Vec::new();
            background.push_actors(&mut layers, -100);
            actors.push(frame(layers));
        }
        self.style
            .push(actors, page, assets.fonts(), self.pane_elapsed, phase);
    }

    fn entry_transition(&self) -> (Vec<Actor>, f32) {
        (Vec::new(), self.style.page.enter.remaining_seconds())
    }

    fn exit_transition(&self, _target: Screen) -> (Vec<Actor>, f32) {
        (Vec::new(), self.style.page.leave.remaining_seconds())
    }
}

impl PlayerStyle {
    fn push(
        &self,
        actors: &mut Vec<Actor>,
        page: Page<'_>,
        fonts: &FontMap,
        elapsed: f32,
        phase: SystemOptionsPhase,
    ) {
        let style = &self.page;
        let mut children = Vec::new();
        style.push_chrome(&mut children, LABEL, page.title, elapsed, page.alpha);
        let mut label = style.label.clone();
        let mut item = style.item.clone();
        label.advance(elapsed);
        item.advance(elapsed);
        let motion = style.frame_state(phase);
        let origin = [
            screen_center_x() - CANVAS[0] * 0.5 + motion.x,
            screen_center_y() - CANVAS[1] * 0.5 + motion.y,
        ];
        let two_players = page.active == [true, true];
        let mid = (style.value_x[0] + style.value_x[1]) * 0.5;
        let shared_x = style.long_value_x.unwrap_or(mid);
        let seats = if two_players {
            self.seats
        } else {
            [shared_x; 2]
        };
        let values = PlayerValues {
            item: &item,
            font: fonts,
            active: page.active,
            seats,
        };
        for index in 0..page.row_count {
            let Some(row) = (page.row)(index) else {
                continue;
            };
            let y = if row.exit {
                style.done_y.unwrap_or_else(|| self.row_y(row.slot))
            } else {
                self.row_y(row.slot)
            };
            let focused: [bool; 2] =
                std::array::from_fn(|seat| page.active[seat] && page.selected_rows[seat] == index);
            if focused.contains(&true) {
                style.push_highlight(&mut children, y, row.alpha);
            }
            let text = row.detail.map_or_else(
                || row.label.clone(),
                |detail| TextContent::Owned(format!("{}\n{}", row.label.as_str(), detail.as_str())),
            );
            children.extend(label.actor(
                LABEL,
                text,
                [style.label_x, y],
                self.tint(focused.contains(&true), row.alpha),
            ));
            self.push_values(&mut children, &values, &row, y, focused);
            if row.has_preview {
                let start = actors.len();
                let x = std::array::from_fn(|seat| {
                    origin[0] + self.previews[seat].unwrap_or(seats[seat])
                });
                (page.draw_preview)(actors, index, x, origin[1] + y);
                for actor in &mut actors[start..] {
                    actor.mul_alpha(motion.tint[3]);
                }
            }
        }
        for seat in 0..page.active.len() {
            if !page.active[seat] {
                continue;
            }
            let help = if two_players {
                self.help[seat].as_ref()
            } else {
                style.help.as_ref()
            };
            if let (Some((position, help)), Some(content)) = (help, (page.help)(seat, 0)) {
                let mut help = help.clone();
                help.advance(elapsed);
                children.extend(help.actor(
                    HELP,
                    TextContent::Shared(content.clone()),
                    *position,
                    [1.0, 1.0, 1.0, page.alpha],
                ));
            }
        }
        actors.push(page_frame(children, motion));
    }

    fn push_values(
        &self,
        actors: &mut Vec<Actor>,
        values: &PlayerValues<'_>,
        row: &PlayerOptionsRow<'_, TextContent>,
        y: f32,
        focused: [bool; 2],
    ) {
        let [left, right] = self.page.value_x;
        let span = right - left;
        let style = values.item;
        let font = values.font.get(ITEM);
        let width = |text: &TextContent| {
            font.map_or(0.0, |font| {
                measure_line_width_logical(font, text.as_str(), values.font) as f32
                    * style.scale_x().abs()
            })
        };
        if let Some(inline) = &row.inline
            && font.is_some()
            && inline.choices.iter().map(width).sum::<f32>()
                + self.page.item_gap * inline.choices.len().saturating_sub(1) as f32
                <= span
        {
            let mut x = left;
            for (choice, text) in inline.choices.iter().enumerate() {
                let mut selected = false;
                let width = width(text);
                for (seat, focused) in focused.into_iter().enumerate() {
                    selected |= values.active[seat]
                        && inline.enabled[seat].map_or(inline.selected[seat] == choice, |mask| {
                            choice < u16::BITS as usize && mask & (1 << choice) != 0
                        });
                    if focused && inline.focused[seat] == Some(choice) {
                        self.push_cursor(actors, seat, [x + width * 0.5, y], width, row.alpha);
                    }
                }
                actors.extend(style.actor(
                    ITEM,
                    text.clone(),
                    [x, y],
                    self.tint(selected, row.alpha),
                ));
                x += width + self.page.item_gap;
            }
            return;
        }
        let two_players = values.active == [true, true];
        let seats = values.seats;
        let column_width = if two_players {
            span.min((seats[1] - seats[0]).abs())
        } else {
            span
        };
        for (seat, focused) in focused.into_iter().enumerate() {
            if !values.active[seat] {
                continue;
            }
            let value = row.values[seat].or_else(|| {
                let inline = row.inline.as_ref()?;
                inline
                    .choices
                    .get(inline.focused[seat].unwrap_or(inline.selected[seat]))
            });
            let Some(text) = value else {
                continue;
            };
            if focused && !row.next_row[seat] {
                self.push_cursor(
                    actors,
                    seat,
                    [seats[seat], y],
                    width(text).min(column_width),
                    row.alpha,
                );
            }
            if let Some(mut actor) = style.actor(
                ITEM,
                text.clone(),
                [seats[seat], y],
                self.tint(focused, row.alpha),
            ) {
                if let Actor::Text {
                    align, max_width, ..
                } = &mut actor
                {
                    align[0] = 0.5;
                    *max_width = Some(column_width);
                }
                actors.push(actor);
            }
        }
    }

    fn tint(&self, selected: bool, alpha: f32) -> [f32; 4] {
        let mut color = self.page.color(selected, false);
        color[3] *= alpha;
        color
    }

    fn row_y(&self, slot: f32) -> f32 {
        let rows = &self.page.row_y;
        let last = rows.len() - 1;
        if slot < 0.0 {
            return lerp(rows[0], self.offscreen_y[0], (-slot * 2.0).min(1.0));
        }
        if slot > last as f32 {
            return lerp(
                rows[last],
                self.offscreen_y[1],
                ((slot - last as f32) * 2.0).min(1.0),
            );
        }
        let start = (slot.floor() as usize).min(last);
        lerp(rows[start], rows[(start + 1).min(last)], slot.fract())
    }

    fn push_cursor(
        &self,
        actors: &mut Vec<Actor>,
        seat: usize,
        [x, y]: [f32; 2],
        width: f32,
        alpha: f32,
    ) {
        let Some((texture, atlas)) = &self.cursor else {
            return;
        };
        if !atlas {
            let mut sprite = SpriteBuilder::with_source(texture.source.clone());
            sprite.xy(x, y);
            sprite.size(width.max(texture.dimensions[0]), texture.dimensions[1]);
            sprite.alpha(alpha);
            actors.push(sprite.build(0));
            return;
        }
        // Cursor sheets contain left, stretchable center, and right cells for each seat.
        let cap = texture.dimensions[0] / 3.0;
        let height = texture.dimensions[1] * 0.5;
        let width = width.max(cap);
        let top = seat as f32 * 0.5;
        for (part, (center, span)) in [
            (x - (width + cap) * 0.5, cap),
            (x, width),
            (x + (width + cap) * 0.5, cap),
        ]
        .into_iter()
        .enumerate()
        {
            let mut sprite = SpriteBuilder::with_source(texture.source.clone());
            sprite.xy(center, y);
            sprite.size(span, height);
            sprite.alpha(alpha);
            sprite.customtexturerect([part as f32 / 3.0, top, (part + 1) as f32 / 3.0, top + 0.5]);
            actors.push(sprite.build(0));
        }
    }
}

fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    (to - from).mul_add(progress, from)
}
