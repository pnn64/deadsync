//! Music-wheel presentation over the application's current browsing view.

mod banner;
pub use banner::{WHEEL_BANNER_FADE, WheelBannerFrame, WheelBannerMotion};

use crate::presentation::{
    ACCENT, HEIGHT, INK, Layout, MUTED, WIDTH, italic, push_red_backdrop, push_red_streaks, quad,
    sprite,
};
use crate::resources::{MENU_HEADING_FONT, MENU_NORMAL_FONT, TITLE_FONT};
use deadlib_present::actors::{Actor, SizeSpec, TextContent};
use deadlib_present::cache::{SharedStrCache, cached_shared_str, shared_str_cache_with_capacity};
use deadlib_present::dsl::TextBuilder;
use deadlib_present::render::{BlendMode, MeshVertex};
use deadlib_present::space::{
    Metrics, current_metrics, menu_perspective, screen_height, screen_width,
};
use deadsync_theme::select_music::{
    SELECT_MUSIC_WHEEL_ROWS, SelectCourseItem, SelectCourseView, SelectMusicPlayerView,
    SelectMusicPrompt, SelectMusicView, SelectMusicWheelAction, SelectMusicWheelRow,
};
use glam::{Mat4, Vec3};
use std::cell::RefCell;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

const WHEEL_CENTER: [f32; 2] = [212.0, 222.0];
const HEADER_Y: f32 = 38.0;
const WHEEL_RADIUS: f32 = 200.0;
const WHEEL_CURVE_X: f32 = 24.0;
const WHEEL_VISIBLE_HALF_SPAN: f32 = 7.5;
const WHEEL_ANGLE_PER_ROW: f32 = TAU * 0.42 / SELECT_MUSIC_WHEEL_ROWS as f32;
const WHEEL_CARD_SIZE: [f32; 2] = [368.0, 32.0];
const LEFT_RIM_CENTER: [f32; 2] = [67.0, 223.0];
const WHEEL_HIGHLIGHT_Z: i16 = -52;
const FRAME_SEAM: [f32; 2] = [375.0, 222.0];
const FRAME_EDGE_WIDTH: f32 = 132.0;
const FRAME_MIDDLE_WIDTH: f32 = 353.0;
const FRAME_HEIGHT: f32 = 414.0;
const DIFFICULTY_X: f32 = 474.0;
// The first lane is between the silver divider and the first rule at texel 234.
const DIFFICULTY_Y: f32 = FRAME_SEAM[1] + (223.0 / 412.0 - 0.5) * FRAME_HEIGHT;
const DIFFICULTY_PITCH: f32 = 20.0 * FRAME_HEIGHT / 412.0;
const TEXT_CACHE_LIMIT: usize = 4096;
const WHITE: [f32; 4] = [1.0; 4];
const CREAM: [f32; 4] = [0.82, 0.73, 0.69, 1.0];

const fn opaque([red, green, blue]: [f32; 3]) -> [f32; 4] {
    [red, green, blue, 1.0]
}

thread_local! {
    static WHEEL_LABELS: RefCell<SharedStrCache> = RefCell::new(shared_str_cache_with_capacity(256));
    static WHEEL_CAMERAS: RefCell<Option<WheelCameras>> = const { RefCell::new(None) };
    static DETAILS_PLATE: RefCell<Option<(f32, Arc<[MeshVertex]>)>> = const { RefCell::new(None) };
}

fn shared_text(text: &str) -> TextContent {
    TextContent::inline_str(text).unwrap_or_else(|| {
        TextContent::Shared(cached_shared_str(&WHEEL_LABELS, text, TEXT_CACHE_LIMIT))
    })
}

fn wheel_text(
    layout: Layout,
    position: [f32; 2],
    align: f32,
    content: TextContent,
    zoom: f32,
    color: [f32; 4],
) -> TextBuilder {
    let mut text = layout.text(position, align, content, zoom, color);
    text.font(MENU_NORMAL_FONT);
    text
}

/// The camera scopes carry the 3D card pose for both its quads and text.
/// Keeping the scopes flat lets the wheel refill the caller's actor buffer.
struct WheelCameras {
    bounds: [f32; 4],
    offset: f32,
    rows: [Option<Mat4>; SELECT_MUSIC_WHEEL_ROWS],
    left_crops: [f32; SELECT_MUSIC_WHEEL_ROWS],
    highlight: Mat4,
    highlight_crop: f32,
}

impl WheelCameras {
    fn bounds(metrics: Metrics) -> [f32; 4] {
        [metrics.left, metrics.right, metrics.top, metrics.bottom]
    }

    fn new(metrics: Metrics, offset: f32) -> Self {
        let width = metrics.right - metrics.left;
        let height = metrics.top - metrics.bottom;
        let scale = (width / WIDTH).min(height / HEIGHT);
        let viewport = Mat4::from_scale(Vec3::new(
            scale * WIDTH / width,
            scale * HEIGHT / height,
            1.0,
        ));
        let (projection, view) =
            menu_perspective([WIDTH, HEIGHT], [WIDTH * 0.5, WHEEL_CENTER[1]], 58.0)
                .expect("the wheel has a fixed positive camera size and field of view");
        let parent = viewport
            * projection
            * view
            * Mat4::from_translation(Vec3::new(WHEEL_CENTER[0], WHEEL_CENTER[1], 0.0))
            * Mat4::from_scale(Vec3::new(0.86, 0.96, 1.40))
            * Mat4::from_rotation_y(-20.0_f32.to_radians());
        let card_camera = |distance: f32| {
            let angle = distance * WHEEL_ANGLE_PER_ROW;
            let (rise, depth) = angle.sin_cos();
            let x = (1.0 - (distance / PI).cos()) * WHEEL_CURVE_X;
            // Fold the conversion from centered world coordinates to the
            // card's top-left plane into the X rotation and translation.
            parent
                * Mat4::from_cols_array(&[
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    -depth,
                    rise,
                    0.0,
                    0.0,
                    rise,
                    depth,
                    0.0,
                    x - metrics.left,
                    depth * metrics.top + WHEEL_RADIUS * rise,
                    WHEEL_RADIUS * depth - 100.0 - rise * metrics.top,
                    1.0,
                ])
        };
        let crop_left = |camera: Mat4| {
            let half_width = WHEEL_CARD_SIZE[0] * 0.5;
            let left = camera * Vec3::new(metrics.left - half_width, metrics.top, 0.0).extend(1.0);
            let right = camera * Vec3::new(metrics.left + half_width, metrics.top, 0.0).extend(1.0);
            let y = (1.0 - left.y / (left.w * viewport.y_axis.y)) * HEIGHT * 0.5;
            // Keep the exposed edge under the silver rim as the wheel turns.
            // The visible rim starts at x=17 and curves out to x=120.
            let height = ((y - LEFT_RIM_CENTER[1]) / 145.0).clamp(-1.0, 1.0);
            let x = 19.0 + 101.0 * (1.0 - (1.0 - height * height).sqrt());
            let edge = (2.0 * x / WIDTH - 1.0) * viewport.x_axis.x;
            // Intersect the row's projected horizontal center with that edge.
            let left = left.x - edge * left.w;
            let right = right.x - edge * right.w;
            (-left / (right - left)).clamp(0.0, 1.0)
        };
        let rows = std::array::from_fn(|index| {
            let distance = index as f32 - (SELECT_MUSIC_WHEEL_ROWS / 2) as f32 + offset;
            (distance.abs() <= WHEEL_VISIBLE_HALF_SPAN).then(|| card_camera(distance))
        });
        let highlight = card_camera(0.0);
        Self {
            bounds: Self::bounds(metrics),
            offset,
            rows,
            left_crops: rows.map(|camera| camera.map_or(0.0, crop_left)),
            highlight,
            highlight_crop: crop_left(highlight),
        }
    }
}

/// Draws the browsing wheel, banner, currently available charts, and launch feedback.
pub fn push_select_music_actors(
    actors: &mut Vec<Actor>,
    view: SelectMusicView<'_>,
    banner: WheelBannerFrame<'_>,
) {
    let layout = push_backdrop(actors, view.selection_beat);
    let camera = details_camera(layout);
    push_details_frame(actors, layout, camera);
    push_banner(actors, layout, &view, banner);
    push_wheel(
        actors,
        layout,
        &view.rows,
        view.wheel_offset,
        view.selection_beat,
        view.wheel_locked,
        "NO SONGS",
    );
    push_difficulties(actors, layout, &view, camera);
    for (side, player) in view.players.iter().enumerate() {
        if player.joined {
            push_chart_pane(actors, layout, side, player);
        }
    }
    push_footer(actors, layout, &view);
    push_prompt(actors, layout, view.prompt);
}

fn push_backdrop(actors: &mut Vec<Actor>, selection_beat: f32) -> Layout {
    let (width, height) = (screen_width(), screen_height());
    push_red_backdrop(
        actors,
        [width, height],
        selection_beat.rem_euclid(16.0) * 0.2,
        -100,
    );
    push_red_streaks(actors, [width, height], 1.0, -20);
    Layout::centered(width, height)
}

fn details_plate(layout: Layout) -> Actor {
    // Points along the silver rail in the source edge sprite's 158x412 grid.
    const LEFT_EDGE: [[f32; 2]; 13] = [
        [150.0, 16.0],
        [101.0, 30.0],
        [69.0, 60.0],
        [44.0, 100.0],
        [30.0, 140.0],
        [23.0, 180.0],
        [22.0, 205.0],
        [27.0, 260.0],
        [39.0, 300.0],
        [62.0, 340.0],
        [90.0, 370.0],
        [126.0, 390.0],
        [157.0, 395.0],
    ];
    let [scale, _] = layout.size([1.0; 2]);
    let vertices = DETAILS_PLATE.with_borrow_mut(|cached| {
        if cached.as_ref().is_none_or(|(old, _)| *old != scale) {
            let mut vertices = Vec::with_capacity(6 * (LEFT_EDGE.len() - 1));
            let to_layout = |[x, y]: [f32; 2]| {
                [
                    FRAME_SEAM[0] + (x / 158.0 - 1.0) * FRAME_EDGE_WIDTH,
                    FRAME_SEAM[1] + (y / 412.0 - 0.5) * FRAME_HEIGHT,
                ]
            };
            let right = FRAME_SEAM[0] + FRAME_MIDDLE_WIDTH;
            for edge in LEFT_EDGE.windows(2) {
                let [x0, y0] = to_layout(edge[0]);
                let [x1, y1] = to_layout(edge[1]);
                let [a, b, c, d] = [[x0, y0], [right, y0], [right, y1], [x1, y1]];
                vertices.extend([a, b, c, a, c, d].map(|[x, y]| MeshVertex {
                    pos: [x * scale, y * scale],
                    color: [0.25, 0.068, 0.054, 1.0],
                }));
            }
            *cached = Some((scale, vertices.into()));
        }
        Arc::clone(&cached.as_ref().expect("detail frame is prepared").1)
    });
    Actor::Mesh {
        align: [0.0; 2],
        offset: layout.position(0.0, 0.0),
        size: layout.size([WIDTH, HEIGHT]).map(SizeSpec::Px),
        tint: [1.0; 4],
        vertex_color_encoding: Default::default(),
        vertices,
        visible: true,
        blend: BlendMode::Alpha,
        z: -9,
    }
}

fn push_details_frame(actors: &mut Vec<Actor>, layout: Layout, view_proj: Mat4) {
    actors.push(
        sprite(
            "openitg/wheel-frame-left.png",
            layout.position(LEFT_RIM_CENTER[0], LEFT_RIM_CENTER[1]),
            layout.size([106.0, 312.0]),
            WHITE,
            -8,
        )
        .build(0),
    );

    actors.push(Actor::CameraPush { view_proj });
    actors.push(details_plate(layout));
    for (key, x, width) in [
        (
            "openitg/wheel-frame-edge.png",
            FRAME_SEAM[0] - FRAME_EDGE_WIDTH / 2.0,
            FRAME_EDGE_WIDTH,
        ),
        (
            "openitg/wheel-frame-middle.png",
            FRAME_SEAM[0] + FRAME_MIDDLE_WIDTH / 2.0,
            FRAME_MIDDLE_WIDTH,
        ),
    ] {
        actors.push(
            sprite(
                key,
                layout.position(x, FRAME_SEAM[1]),
                layout.size([width, FRAME_HEIGHT]),
                WHITE,
                -5,
            )
            .build(0),
        );
    }
    actors.push(Actor::CameraPop);
}

fn details_camera(layout: Layout) -> Mat4 {
    let metrics = current_metrics();
    let [x, y] = layout.position(FRAME_SEAM[0], FRAME_SEAM[1]);
    let [scale, _] = layout.size([1.0; 2]);
    let viewport = Mat4::from_scale(Vec3::new(
        scale * WIDTH / (metrics.right - metrics.left),
        scale * HEIGHT / (metrics.top - metrics.bottom),
        1.0,
    ));
    let (projection, view) = menu_perspective([WIDTH, HEIGHT], [320.0, FRAME_SEAM[1]], 58.0)
        .expect("the wheel has a fixed positive camera size and field of view");
    let pivot = Vec3::new(metrics.left + x, metrics.top - y, 0.0);
    let pose = Mat4::from_translation(Vec3::new(FRAME_SEAM[0], FRAME_SEAM[1], 0.0))
        * Mat4::from_rotation_y(20.0_f32.to_radians())
        * Mat4::from_scale(Vec3::new(scale.recip(), -scale.recip(), 1.0))
        * Mat4::from_translation(-pivot);
    viewport * projection * view * pose
}

fn push_banner(
    actors: &mut Vec<Actor>,
    layout: Layout,
    view: &SelectMusicView<'_>,
    frame: WheelBannerFrame<'_>,
) {
    let section = view.rows[SELECT_MUSIC_WHEEL_ROWS / 2];
    banner::push(
        actors,
        layout,
        frame,
        view.banner_texture,
        |actors, moving| {
            if let Some(song) = &view.song {
                actors.push(
                    sprite(
                        "openitg/wheel-fallback-banner.png",
                        moving.position(banner::CENTER[0], banner::CENTER[1]),
                        moving.size(banner::SIZE),
                        WHITE,
                        -6,
                    )
                    .build(0),
                );
                if section.is_some_and(|row| row.action.is_some()) {
                    let mut text = wheel_text(
                        moving,
                        [banner::CENTER[0], 161.0],
                        0.5,
                        shared_text(song.title),
                        0.65,
                        WHITE,
                    );
                    text.maxwidth(moving.size([280.0, 0.0])[0]);
                    text.shadowlength(moving.size([2.0, 0.0])[0]);
                    actors.push(text.build(0));
                }
            } else if let Some(section) = section {
                let mut text = wheel_text(
                    moving,
                    banner::CENTER,
                    0.5,
                    shared_text(wheel_row_title(section)),
                    1.1,
                    WHITE,
                );
                text.maxwidth(moving.size([280.0, 0.0])[0]);
                actors.push(text.build(0));
            }
        },
    );
    if let Some(song) = &view.song {
        // The names in the wheel and ribbon follow the live selection while
        // the outgoing banner may still be visible during its crossfade.
        if !song.subtitle.is_empty() {
            let mut text = wheel_text(
                layout,
                [598.0, 161.0],
                1.0,
                shared_text(song.subtitle),
                0.55,
                WHITE,
            );
            text.maxwidth(layout.size([260.0, 0.0])[0]);
            text.shadowlength(layout.size([2.0, 0.0])[0]);
            actors.push(text.build(0));
        }
        actors.push(wheel_text(layout, [280.0, 186.0], 0.0, "ARTIST".into(), 0.53, CREAM).build(0));
        let mut artist = wheel_text(
            layout,
            [280.0, 202.0],
            0.0,
            shared_text(song.artist),
            0.64,
            WHITE,
        );
        artist.maxwidth(layout.size([196.0, 0.0])[0]);
        actors.push(artist.build(0));
        if !song.display_bpm.is_empty() {
            actors
                .push(wheel_text(layout, [482.0, 191.0], 0.0, "BPM".into(), 0.53, CREAM).build(0));
            let mut bpm = wheel_text(
                layout,
                [482.0, 207.0],
                0.0,
                shared_text(song.display_bpm),
                0.65,
                WHITE,
            );
            bpm.maxwidth(layout.size([135.0, 0.0])[0]);
            actors.push(bpm.build(0));
        }
    } else if let (Some(section), Some(_)) = (section, view.banner_texture) {
        let mut title = wheel_text(
            layout,
            [280.0, 202.0],
            0.0,
            shared_text(section.title),
            0.64,
            WHITE,
        );
        title.maxwidth(layout.size([332.0, 0.0])[0]);
        actors.push(title.build(0));
    }
}

fn push_wheel(
    actors: &mut Vec<Actor>,
    layout: Layout,
    rows: &[Option<SelectMusicWheelRow<'_>>; SELECT_MUSIC_WHEEL_ROWS],
    wheel_offset: f32,
    selection_beat: f32,
    locked: bool,
    empty_text: &'static str,
) {
    if rows.iter().all(Option::is_none) {
        actors.push(layout.label(
            [83.0, WHEEL_CENTER[1]],
            0.0,
            empty_text.into(),
            1.0,
            opaque(MUTED),
        ));
        return;
    }
    let metrics = current_metrics();
    let card = Layout::centered(WIDTH, HEIGHT);
    WHEEL_CAMERAS.with_borrow_mut(|cached| {
        if cached.as_ref().is_none_or(|cameras| {
            cameras.bounds != WheelCameras::bounds(metrics) || cameras.offset != wheel_offset
        }) {
            *cached = Some(WheelCameras::new(metrics, wheel_offset));
        }
        let cameras = cached.as_ref().expect("wheel cameras have been prepared");
        let center = SELECT_MUSIC_WHEEL_ROWS / 2;
        let order = (0..center).chain((center..SELECT_MUSIC_WHEEL_ROWS).rev());
        for (layer, index) in order.enumerate() {
            if let (Some(row), Some(view_proj)) = (rows[index], cameras.rows[index]) {
                actors.push(Actor::CameraPush { view_proj });
                let brightness = if locked && index != center { 0.5 } else { 1.0 };
                push_wheel_card(
                    actors,
                    card,
                    row,
                    -90 + 2 * layer as i16,
                    brightness,
                    cameras.left_crops[index],
                );
                actors.push(Actor::CameraPop);
            }
        }
        let phase = (selection_beat + 0.2).rem_euclid(1.0);
        let pulse = 1.0 - (2.0 * phase - 1.0).abs();
        let accent = [1.0, 1.0, 1.0, 0.3 + 0.4 * pulse];
        actors.push(Actor::CameraPush {
            view_proj: cameras.highlight,
        });
        let mut highlight = sprite(
            "openitg/wheel-highlight.png",
            card.position(0.0, 0.0),
            [WHEEL_CARD_SIZE[0], 34.0],
            accent,
            WHEEL_HIGHLIGHT_Z,
        );
        highlight.cropleft(cameras.highlight_crop);
        actors.push(highlight.build(0));
        actors.push(Actor::CameraPop);
    });
}

fn wheel_row_title(row: SelectMusicWheelRow<'_>) -> &str {
    match row.action {
        Some(SelectMusicWheelAction::RevealSong) => "CHANCE",
        Some(SelectMusicWheelAction::PreviewedSong) => "RANDOM",
        None => row.title,
    }
}

fn push_wheel_card(
    actors: &mut Vec<Actor>,
    card: Layout,
    row: SelectMusicWheelRow<'_>,
    z: i16,
    brightness: f32,
    left_crop: f32,
) {
    let [width, height] = WHEEL_CARD_SIZE;
    let section = row.song_count.is_some();
    let dim = |[r, g, b]: [f32; 3]| [r * brightness, g * brightness, b * brightness, 1.0];
    let key = match (row.action.is_some(), section, row.expanded) {
        (true, _, _) => "openitg/wheel-mode.png",
        (_, true, true) => "openitg/wheel-expanded.png",
        (_, true, false) => "openitg/wheel-section.png",
        _ => "openitg/wheel-song.png",
    };
    let mut tile = sprite(
        key,
        card.position(0.0, 0.0),
        [width, height],
        dim([1.0; 3]),
        z,
    );
    tile.cropleft(left_crop);
    actors.push(tile.build(0));
    let (x, align, zoom, max_width, tint) = if row.action.is_some() {
        (4.0, 0.5, 0.97, 170.0, dim([0.68, 0.63, 0.62]))
    } else {
        (105.0, 1.0, 0.83, 221.0, dim([1.0; 3]))
    };
    let mut title = wheel_text(
        card,
        [x, 0.0],
        align,
        shared_text(wheel_row_title(row)),
        zoom,
        tint,
    );
    title.maxwidth(max_width);
    title.z(z + 39);
    actors.push(italic(
        title.build(0),
        if row.action.is_some() { -0.16 } else { 0.0 },
    ));
    if let Some(count) = row.song_count {
        let mut count = wheel_text(
            card,
            [-95.0, 0.0],
            1.0,
            TextContent::inline_u32(count.min(u32::MAX as usize) as u32),
            0.52,
            dim([0.83, 0.72, 0.49]),
        );
        count.z(z + 39);
        actors.push(count.build(0));
    }
}

const DIFFICULTY_STYLES: [(&str, &str, [f32; 4]); 6] = [
    ("Beginner", "NOVICE", [0.79, 0.46, 1.0, 1.0]),
    ("Easy", "EASY", [0.39, 0.91, 0.29, 1.0]),
    ("Medium", "MEDIUM", [1.0, 0.86, 0.25, 1.0]),
    ("Hard", "HARD", [1.0, 0.24, 0.32, 1.0]),
    ("Expert", "EXPERT", [0.30, 0.73, 1.0, 1.0]),
    ("Edit", "EDIT", WHITE),
];

pub(crate) fn difficulty_style(difficulty: &str) -> (usize, &str, [f32; 4]) {
    DIFFICULTY_STYLES
        .iter()
        .enumerate()
        .find(|(_, (name, _, _))| difficulty.eq_ignore_ascii_case(name))
        .map_or((5, difficulty, WHITE), |(index, &(_, title, color))| {
            (index, title, color)
        })
}

fn push_difficulties(
    actors: &mut Vec<Actor>,
    layout: Layout,
    view: &SelectMusicView<'_>,
    view_proj: Mat4,
) {
    if view.song.is_none() {
        return;
    }
    let x = DIFFICULTY_X;
    actors.push(Actor::CameraPush { view_proj });
    for (index, row) in view.difficulty_rows.iter().enumerate() {
        let Some(chart) = row.chart else { continue };
        let y = DIFFICULTY_Y + index as f32 * DIFFICULTY_PITCH;
        let (state, level, color) = difficulty_style(&chart.difficulty);
        let level = if state == 5 && !chart.description.is_empty() {
            chart.description.as_str()
        } else {
            level
        };
        let mut title = wheel_text(layout, [x - 72.0, y], 1.0, shared_text(level), 0.58, color);
        title.maxwidth(layout.size([119.0, 0.0])[0]);
        actors.push(title.build(0));
        actors.push(
            wheel_text(
                layout,
                [x - 25.0, y],
                0.5,
                TextContent::inline_u32(chart.meter),
                0.70,
                color,
            )
            .build(0),
        );

        for segment in 0..12 {
            let filled = segment < chart.meter;
            let cell = if filled { state } else { 6 };
            let mut block = sprite(
                "openitg/wheel-meter.png",
                layout.position(x + 6.0 + segment as f32 * 13.45, y),
                layout.size([13.45, 16.0]),
                WHITE,
                4,
            );
            block.customtexturerect([cell as f32 / 7.0, 0.0, (cell + 1) as f32 / 7.0, 1.0]);
            actors.push(block.build(0));
        }
        for (player, side) in view.players.iter().enumerate() {
            if side.joined && side.chart.is_some() && side.step_index == row.step_index {
                let (offset, key) = [
                    (-59.0, "openitg/wheel-p1.png"),
                    (175.0, "openitg/wheel-p2.png"),
                ][player];
                actors.push(
                    sprite(
                        key,
                        layout.position(x + offset, y),
                        layout.size([18.0, 16.0]),
                        WHITE,
                        5,
                    )
                    .build(0),
                );
            }
        }
    }
    actors.push(Actor::CameraPop);
}

fn push_chart_pane(
    actors: &mut Vec<Actor>,
    layout: Layout,
    side: usize,
    player: &SelectMusicPlayerView<'_>,
) {
    let Some(chart) = player.chart else { return };
    let center = [168.0, 472.0][side];
    actors.push(
        sprite(
            "openitg/wheel-pane.png",
            layout.position(center, 383.0),
            layout.size([290.0, 74.0]),
            WHITE,
            8,
        )
        .build(0),
    );
    let stats = &chart.stats;
    for ([x, y], (label, value)) in [
        ([center - 125.0, 361.0], ("Steps", stats.total_steps)),
        ([center - 125.0, 375.0], ("Holds", stats.holds)),
        ([center - 14.0, 361.0], ("Jumps", stats.jumps)),
        ([center - 14.0, 375.0], ("Mines", chart.mines_nonfake)),
        ([center - 14.0, 389.0], ("Hands", stats.hands)),
        ([center - 14.0, 403.0], ("Rolls", stats.rolls)),
    ] {
        actors.push(wheel_text(layout, [x, y], 0.0, label.into(), 0.48, WHITE).build(0));
        let count = TextContent::inline_u32(value);
        actors.push(wheel_text(layout, [x + 77.0, y], 1.0, count, 0.49, WHITE).build(0));
    }
    for (y, label, record) in [
        (
            389.0,
            "Best",
            player.local_records.machine.map(|record| record.percent()),
        ),
        (
            403.0,
            "Card",
            player.local_records.personal.map(|record| record.percent()),
        ),
    ] {
        actors
            .push(wheel_text(layout, [center - 125.0, y], 0.0, label.into(), 0.48, WHITE).build(0));
        let score = record.map_or_else(
            || "N/A".into(),
            |percent| {
                TextContent::inline_format(format_args!("{percent:.2}%"))
                    .expect("normalized percentages fit inline")
            },
        );
        actors.push(wheel_text(layout, [center - 23.0, y], 1.0, score, 0.49, WHITE).build(0));
    }
    let (_, level, color) = difficulty_style(&chart.difficulty);
    actors.push(
        wheel_text(
            layout,
            [center + 97.0, 376.0],
            0.5,
            TextContent::inline_u32(chart.meter),
            1.38,
            color,
        )
        .build(0),
    );
    let mut difficulty = wheel_text(
        layout,
        [center + 97.0, 400.0],
        0.5,
        shared_text(level),
        0.49,
        color,
    );
    difficulty.maxwidth(layout.size([70.0, 0.0])[0]);
    actors.push(difficulty.build(0));
    if let Some(record) = player
        .local_records
        .machine
        .filter(|record| !record.name.is_empty())
    {
        let mut holder = wheel_text(
            layout,
            [center + 97.0, 357.0],
            0.5,
            shared_text(&record.name),
            0.46,
            CREAM,
        );
        holder.maxwidth(layout.size([70.0, 0.0])[0]);
        actors.push(holder.build(0));
    }
}

fn push_footer(actors: &mut Vec<Actor>, layout: Layout, view: &SelectMusicView<'_>) {
    push_wheel_title(actors, layout, "PICK A TUNE");
    if let Some(stage) = view.stage_number {
        let label = TextContent::inline_format(format_args!("ROUND {stage}"))
            .unwrap_or_else(|| "ROUND".into());
        let mut text = layout.text([30.0, 73.0], 0.0, label, 0.72, WHITE);
        text.font(TITLE_FONT);
        text.z(21);
        actors.push(text.build(0));
    }
    for ((x, align), player) in [(40.0, 0.0), (600.0, 1.0)].into_iter().zip(view.players) {
        let content = if player.joined {
            player.name
        } else {
            "NOT PRESENT"
        };
        let mut text = layout.text([x, 465.0], align, shared_text(content), 0.58, WHITE);
        text.font(TITLE_FONT);
        text.maxwidth(layout.size([230.0, 0.0])[0]);
        text.z(21);
        actors.push(text.build(0));
    }
    if (view.music_rate - 1.0).abs() >= 0.005 {
        let label = TextContent::inline_format(format_args!("{:.2}x", view.music_rate))
            .unwrap_or_else(|| "RATE".into());
        actors.push(layout.label([604.0, 447.0], 1.0, label, 0.9, opaque(ACCENT)));
    }
}

fn push_wheel_title(actors: &mut Vec<Actor>, layout: Layout, title: &'static str) {
    let mut text = layout.text([35.0, HEADER_Y], 0.0, title.into(), 1.0, WHITE);
    text.font(MENU_HEADING_FONT);
    text.shadowlength(layout.size([4.0, 0.0])[0]);
    text.z(22);
    actors.push(italic(text.build(0), -0.16));
}

fn push_prompt(actors: &mut Vec<Actor>, layout: Layout, prompt: SelectMusicPrompt) {
    if prompt != SelectMusicPrompt::EnteringOptions {
        let label = if prompt == SelectMusicPrompt::OfferOptions {
            "Press Start for Player Options"
        } else {
            "Press Start to choose"
        };
        let mut text = wheel_text(layout, [320.0, 445.0], 0.5, label.into(), 0.68, WHITE);
        text.z(22);
        actors.push(text.build(0));
        return;
    }
    actors.push(quad(
        [0.0, 0.0],
        [screen_width(), screen_height()],
        [0.05, 0.01, 0.01, 0.70],
        1400,
    ));
    actors.push(layout.panel(112.0, 212.0, [416.0, 56.0], [0.23, 0.055, 0.049, 1.0], 1401));
    let mut heading = wheel_text(
        layout,
        [WIDTH * 0.5, HEIGHT * 0.5],
        0.5,
        "ENTERING PLAYER OPTIONS".into(),
        0.74,
        WHITE,
    );
    heading.maxwidth(layout.size([384.0, 0.0])[0]);
    heading.z(1402);
    actors.push(heading.build(0));
}

/// Present the course variant of the music wheel using its current rating and records.
pub fn push_select_course_actors(
    actors: &mut Vec<Actor>,
    view: SelectCourseView<'_>,
    banner: WheelBannerFrame<'_>,
) {
    let layout = push_backdrop(actors, view.selection_beat);
    push_details_frame(actors, layout, details_camera(layout));
    banner::push(actors, layout, banner, view.banner_texture, |_, _| {});
    push_wheel(
        actors,
        layout,
        &view.rows,
        view.wheel_offset,
        view.selection_beat,
        false,
        "NO COURSES",
    );
    push_wheel_title(actors, layout, "PICK A COURSE");
    if let Some(course) = &view.item {
        push_course_details(actors, layout, course);
    }
    for (player, name) in view.player_names.into_iter().enumerate() {
        if let Some(name) = name.filter(|name| !name.is_empty()) {
            let x = 343.0 + player as f32 * 125.0;
            actors.push(layout.bounded_label(
                [x, 465.0],
                0.0,
                112.0,
                shared_text(name),
                0.79,
                opaque(INK),
            ));
        }
    }
    if (view.music_rate - 1.0).abs() >= 0.005 {
        let label = TextContent::inline_format(format_args!("{:.2}x", view.music_rate))
            .unwrap_or_else(|| "RATE".into());
        actors.push(layout.label([604.0, 447.0], 1.0, label, 0.9, opaque(ACCENT)));
    }
    push_prompt(actors, layout, view.prompt);
}

fn push_course_details(actors: &mut Vec<Actor>, layout: Layout, course: &SelectCourseItem<'_>) {
    actors.push(layout.bounded_label(
        [280.0, 168.0],
        0.0,
        330.0,
        shared_text(course.title),
        1.10,
        opaque(INK),
    ));
    if !course.author.is_empty() {
        actors.push(layout.bounded_label(
            [280.0, 189.0],
            0.0,
            325.0,
            shared_text(course.author),
            0.83,
            opaque(MUTED),
        ));
    }
    for (x, label, value, width) in [
        (280.0, "SONGS", course.songs, 88.0),
        (380.0, "BPM", course.bpm, 137.0),
        (529.0, "TIME", course.length, 98.0),
    ] {
        let text = TextContent::inline_format(format_args!("{label} {value}"))
            .unwrap_or_else(|| shared_text(value));
        actors.push(layout.bounded_label([x, 211.0], 0.0, width, text, 0.83, opaque(INK)));
    }
    if let Some(rating) = course.rating {
        let fill = if course.rating_focused {
            [0.33, 0.27, 0.16, 1.0]
        } else {
            [0.19, 0.30, 0.39, 1.0]
        };
        actors.push(layout.panel(280.0, 226.0, [324.0, 22.0], fill, 2));
        actors.push(layout.bounded_label(
            [442.0, 237.0],
            0.5,
            238.0,
            shared_text(rating),
            0.94,
            opaque(ACCENT),
        ));
        if course.can_change_rating {
            for (x, arrow) in [(297.0, "<"), (587.0, ">")] {
                actors.push(layout.label([x, 237.0], 0.5, arrow.into(), 0.90, opaque(ACCENT)));
            }
        }
    }
    for (index, track) in course.tracks.iter().enumerate() {
        let Some(track) = track else { continue };
        let y = 261.0 + index as f32 * 20.0;
        actors.push(layout.panel(280.0, y - 9.0, [324.0, 18.0], [0.10, 0.16, 0.23, 1.0], 2));
        actors.push(layout.bounded_label(
            [289.0, y],
            0.0,
            250.0,
            track.title.into(),
            0.81,
            opaque(INK),
        ));
        actors.push(layout.bounded_label(
            [591.0, y],
            1.0,
            45.0,
            track.meter.into(),
            0.81,
            opaque(ACCENT),
        ));
    }
    if !course.description.is_empty() {
        actors.push(layout.paragraph(
            [280.0, 379.0],
            [324.0, 41.0],
            course.description.into(),
            0.77,
            opaque(MUTED),
        ));
    }
    for (x, label, record) in [
        (36.0, "MACHINE", course.machine_record),
        (185.0, "PERSONAL", course.player_record),
    ] {
        if let Some([name, score]) = record {
            let value = TextContent::inline_format(format_args!("{label} {name} {score}"))
                .unwrap_or_else(|| shared_text(score));
            actors.push(layout.bounded_label([x, 447.0], 0.0, 142.0, value, 0.78, opaque(MUTED)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::space::{Metrics, with_current_metrics};
    use deadsync_theme::select_music::{
        SelectMusicDifficultyRow, SelectMusicSongView, SelectMusicWheelRow,
    };

    fn view(banner: &Arc<str>, prompt: SelectMusicPrompt) -> SelectMusicView<'_> {
        let mut rows = [None; SELECT_MUSIC_WHEEL_ROWS];
        rows[SELECT_MUSIC_WHEEL_ROWS / 2] = Some(SelectMusicWheelRow {
            title: "Track",
            song_count: None,
            expanded: false,
            action: None,
        });
        SelectMusicView {
            rows,
            wheel_offset: 0.0,
            wheel_locked: false,
            selection_beat: 0.0,
            song: Some(SelectMusicSongView {
                title: "Track",
                subtitle: "",
                artist: "Artist",
                display_bpm: "150",
            }),
            banner_texture: Some(banner),
            players: [SelectMusicPlayerView {
                joined: false,
                name: "",
                chart: None,
                step_index: 0,
                local_records: Default::default(),
            }; 2],
            difficulty_rows: std::array::from_fn(|index| SelectMusicDifficultyRow {
                step_index: index,
                chart: None,
            }),
            stage_number: Some(1),
            music_rate: 1.0,
            prompt,
        }
    }

    #[test]
    fn wheel_projects_tilted_cards_behind_a_fixed_highlight() {
        let metrics = Metrics::centered(640.0, 480.0);
        let settled = WheelCameras::new(metrics, 0.0);
        let scrolling = WheelCameras::new(metrics, 1.0);
        let center = SELECT_MUSIC_WHEEL_ROWS / 2;
        assert_eq!(settled.rows.iter().flatten().count(), 15);
        assert!(settled.rows[0].is_none());
        assert!(scrolling.rows[0].is_some());
        assert_eq!(settled.rows[center + 1], scrolling.rows[center]);
        assert_eq!(settled.highlight, scrolling.highlight);
        let tilted = settled.rows[center - 4].unwrap();
        let left = tilted.project_point3(Vec3::new(-450.0, 240.0, 0.0));
        let right = tilted.project_point3(Vec3::new(-270.0, 240.0, 0.0));
        assert!(
            (left.y - right.y).abs() > 0.015,
            "the card edge should have perspective tilt"
        );
    }

    #[test]
    fn native_and_wide_frames_keep_banner_artist_difficulties_and_prompt_anchors() {
        let banner = Arc::<str>::from("selection-banner");
        for (width, delta) in [(640.0, 0.0), (854.0, 107.0)] {
            with_current_metrics(Metrics::centered(width, 480.0), || {
                let mut actors = Vec::new();
                push_select_music_actors(
                    &mut actors,
                    view(&banner, SelectMusicPrompt::EnteringOptions),
                    WheelBannerFrame::default(),
                );
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Sprite { align, offset, size: [SizeSpec::Px(w), SizeSpec::Px(h)], .. }
                        if *align == [0.5, 0.5] && *offset == [478.0 + delta, banner::CENTER[1]]
                            && *w == banner::SIZE[0] && *h == banner::SIZE[1]
                )));
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Text { content, offset, .. }
                        if content.as_str() == "Artist" && *offset == [280.0 + delta, 202.0]
                )));
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Sprite { source: deadlib_present::actors::SpriteSource::TextureStatic("openitg/wheel-frame-edge.png"), offset, .. }
                        if *offset == [309.0 + delta, 222.0]
                )));
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Text { content, offset, z, .. }
                        if content.as_str() == "ENTERING PLAYER OPTIONS" && *offset == [320.0 + delta, 240.0] && *z > 1401
                )));
            });
        }
    }
}
