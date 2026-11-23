//! Music-wheel presentation over the application's current browsing view.

mod banner;
pub use banner::{WHEEL_BANNER_FADE, WheelBannerFrame, WheelBannerMotion};

use crate::presentation::{ACCENT, HEIGHT, INK, Layout, MUTED, WIDTH, quad};
use deadlib_present::actors::{Actor, SizeSpec, TextContent};
use deadlib_present::cache::{SharedStrCache, cached_shared_str, shared_str_cache_with_capacity};
use deadlib_present::render::{BlendMode, MeshVertex};
use deadlib_present::space::{
    Metrics, current_metrics, menu_perspective, screen_height, screen_width,
};
use deadsync_theme::select_music::{
    SELECT_MUSIC_WHEEL_ROWS, SelectCourseItem, SelectCourseView, SelectMusicPlayerView,
    SelectMusicPrompt, SelectMusicView, SelectMusicWheelRow,
};
use glam::{Mat4, Vec3};
use std::cell::RefCell;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

const WHEEL_CENTER: [f32; 2] = [212.0, 222.0];
const HEADER_Y: f32 = 23.0;
const WHEEL_RADIUS: f32 = 200.0;
const WHEEL_CURVE_X: f32 = 24.0;
const WHEEL_VISIBLE_HALF_SPAN: f32 = 7.5;
const WHEEL_ANGLE_PER_ROW: f32 = TAU * 0.42 / SELECT_MUSIC_WHEEL_ROWS as f32;
const WHEEL_CARD_SIZE: [f32; 2] = [368.0, 32.0];
const WHEEL_HIGHLIGHT_Z: i16 = -52;
const DETAILS_LEFT: f32 = 264.0;
const DIFFICULTY_CENTER: [f32; 2] = [460.0, 242.0];
const DIFFICULTY_PITCH: f32 = 20.5;
const TEXT_CACHE_LIMIT: usize = 4096;

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

/// The camera scopes carry the 3D card pose for both its quads and text.
/// Keeping the scopes flat lets the wheel refill the caller's actor buffer.
struct WheelCameras {
    bounds: [f32; 4],
    offset: f32,
    rows: [Option<Mat4>; SELECT_MUSIC_WHEEL_ROWS],
    highlight: Mat4,
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
        Self {
            bounds: Self::bounds(metrics),
            offset,
            rows: std::array::from_fn(|index| {
                let distance = index as f32 - (SELECT_MUSIC_WHEEL_ROWS / 2) as f32 + offset;
                (distance.abs() <= WHEEL_VISIBLE_HALF_SPAN).then(|| card_camera(distance))
            }),
            highlight: card_camera(0.0),
        }
    }
}

/// Draws the browsing wheel, banner, currently available charts, and launch feedback.
pub fn push_select_music_actors(
    actors: &mut Vec<Actor>,
    view: SelectMusicView<'_>,
    banner: WheelBannerFrame<'_>,
) {
    let layout = push_backdrop(actors);
    push_banner(actors, layout, &view, banner);
    push_wheel(
        actors,
        layout,
        &view.rows,
        view.wheel_offset,
        view.selection_beat,
        "NO SONGS",
    );
    push_difficulties(actors, layout, &view);
    for (side, player) in view.players.iter().enumerate() {
        if player.joined {
            push_chart_pane(actors, layout, side, player);
        }
    }
    push_footer(actors, layout, &view);
    push_prompt(actors, layout, view.prompt);
}

fn push_backdrop(actors: &mut Vec<Actor>) -> Layout {
    let (width, height) = (screen_width(), screen_height());
    let layout = Layout::centered(width, height);
    actors.push(quad([0.0, 0.0], [width, height], banner::WHEEL_MATTE, -100));
    let canvas = [0.063, 0.087, 0.126, 1.0];
    actors.push(layout.panel(0.0, 0.0, [WIDTH, HEIGHT], canvas, -99));
    actors.push(layout.panel(0.0, 0.0, [WIDTH, 44.0], canvas, -5));
    actors.push(layout.panel(0.0, 423.0, [WIDTH, 57.0], canvas, -5));
    layout
}

fn details_plate(layout: Layout) -> Actor {
    const LEFT_EDGE: [[f32; 2]; 11] = [
        [326.0, 44.0],
        [308.0, 72.0],
        [289.0, 104.0],
        [277.0, 140.0],
        [267.0, 180.0],
        [DETAILS_LEFT, 222.0],
        [267.0, 266.0],
        [276.0, 305.0],
        [288.0, 345.0],
        [305.0, 383.0],
        [328.0, 423.0],
    ];
    let [scale, _] = layout.size([1.0; 2]);
    let vertices = DETAILS_PLATE.with_borrow_mut(|cached| {
        if cached.as_ref().is_none_or(|(old, _)| *old != scale) {
            let mut vertices = Vec::with_capacity(12 * (LEFT_EDGE.len() - 1));
            for edge in LEFT_EDGE.windows(2) {
                let [x0, y0] = edge[0];
                let [x1, y1] = edge[1];
                for (right, color) in [
                    ([WIDTH; 2], [0.075, 0.112, 0.165, 1.0]),
                    ([x0 + 2.0, x1 + 2.0], opaque(MUTED)),
                ] {
                    let [a, b, c, d] = [[x0, y0], [right[0], y0], [right[1], y1], [x1, y1]];
                    vertices.extend([a, b, c, a, c, d].map(|pos| MeshVertex {
                        pos: pos.map(|value| value * scale),
                        color,
                    }));
                }
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

fn push_details_frame(actors: &mut Vec<Actor>, layout: Layout) {
    actors.push(details_plate(layout));
    actors.push(layout.panel(276.0, 168.0, [352.0, 50.0], [0.04, 0.07, 0.11, 1.0], 0));
}

fn push_banner(
    actors: &mut Vec<Actor>,
    layout: Layout,
    view: &SelectMusicView<'_>,
    frame: WheelBannerFrame<'_>,
) {
    push_details_frame(actors, layout);
    let section = view.rows[SELECT_MUSIC_WHEEL_ROWS / 2];
    banner::push(
        actors,
        layout,
        frame,
        view.banner_texture,
        |actors, moving| {
            if let Some(song) = &view.song {
                actors.push(moving.bounded_label(
                    [banner::CENTER[0], 105.0],
                    0.5,
                    280.0,
                    shared_text(song.title),
                    1.48,
                    opaque(INK),
                ));
                if !song.subtitle.is_empty() {
                    actors.push(moving.bounded_label(
                        [banner::CENTER[0], 130.0],
                        0.5,
                        280.0,
                        shared_text(song.subtitle),
                        0.75,
                        opaque(MUTED),
                    ));
                }
            } else if let Some(section) = section {
                actors.push(moving.bounded_label(
                    banner::CENTER,
                    0.5,
                    280.0,
                    shared_text(section.title),
                    1.1,
                    opaque(INK),
                ));
            }
        },
    );
    if let Some(song) = &view.song {
        if view.banner_texture.is_some() {
            actors.push(layout.bounded_label(
                [280.0, 182.0],
                0.0,
                332.0,
                shared_text(song.title),
                1.05,
                opaque(INK),
            ));
            if !song.subtitle.is_empty() {
                actors.push(layout.bounded_label(
                    [280.0, 194.0],
                    0.0,
                    332.0,
                    shared_text(song.subtitle),
                    0.75,
                    opaque(MUTED),
                ));
            }
        }
        actors.push(layout.label([280.0, 201.0], 0.0, "ARTIST".into(), 0.66, opaque(MUTED)));
        actors.push(layout.bounded_label(
            [280.0, 211.0],
            0.0,
            180.0,
            shared_text(song.artist),
            0.85,
            opaque(INK),
        ));
        if !song.display_bpm.is_empty() {
            actors.push(layout.label([495.0, 201.0], 0.0, "BPM".into(), 0.66, opaque(MUTED)));
            actors.push(layout.bounded_label(
                [495.0, 211.0],
                0.0,
                125.0,
                shared_text(song.display_bpm),
                0.95,
                opaque(INK),
            ));
        }
    } else if let (Some(section), Some(_)) = (section, view.banner_texture) {
        actors.push(layout.bounded_label(
            [280.0, 182.0],
            0.0,
            332.0,
            shared_text(section.title),
            1.10,
            opaque(INK),
        ));
    }
}

fn push_wheel(
    actors: &mut Vec<Actor>,
    layout: Layout,
    rows: &[Option<SelectMusicWheelRow<'_>>; SELECT_MUSIC_WHEEL_ROWS],
    wheel_offset: f32,
    selection_beat: f32,
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
                push_wheel_card(actors, card, row, -90 + 2 * layer as i16);
                actors.push(Actor::CameraPop);
            }
        }
        let phase = (selection_beat + 0.2).rem_euclid(1.0);
        let pulse = 1.0 - (2.0 * phase - 1.0).abs();
        let accent = [ACCENT[0], ACCENT[1], ACCENT[2], 0.3 + 0.4 * pulse];
        let [width, height] = WHEEL_CARD_SIZE;
        actors.push(Actor::CameraPush {
            view_proj: cameras.highlight,
        });
        for y in [-0.5 * height, 0.5 * height - 1.25] {
            actors.push(card.panel(-0.5 * width, y, [width, 1.25], accent, WHEEL_HIGHLIGHT_Z));
        }
        actors.push(Actor::CameraPop);
    });
}

fn push_wheel_card(actors: &mut Vec<Actor>, card: Layout, row: SelectMusicWheelRow<'_>, z: i16) {
    let [width, height] = WHEEL_CARD_SIZE;
    let section = row.song_count.is_some();
    let tint = match (section, row.expanded) {
        (true, true) => [0.22, 0.28, 0.35, 1.0],
        (true, false) => [0.15, 0.22, 0.30, 1.0],
        (false, _) => [0.095, 0.145, 0.22, 1.0],
    };
    actors.push(card.panel(-0.5 * width, -0.5 * height, [width, height - 0.8], tint, z));
    let (x, max_width, tint) = if section {
        (109.0, 210.0, opaque(ACCENT))
    } else {
        (109.0, 221.0, opaque(INK))
    };
    let mut title = card.text([x, 0.0], 1.0, shared_text(row.title), 0.92, tint);
    title.maxwidth(max_width);
    title.z(z + 1);
    actors.push(title.build(0));
    if let Some(count) = row.song_count {
        let mut count = card.text(
            [-95.0, 0.0],
            1.0,
            TextContent::inline_u32(count.min(u32::MAX as usize) as u32),
            0.65,
            opaque(MUTED),
        );
        count.z(z + 1);
        actors.push(count.build(0));
    }
}

fn push_difficulties(actors: &mut Vec<Actor>, layout: Layout, view: &SelectMusicView<'_>) {
    if view.song.is_none() {
        return;
    }
    let [x, base_y] = DIFFICULTY_CENTER;
    for (index, row) in view.difficulty_rows.iter().enumerate() {
        let y = base_y + index as f32 * DIFFICULTY_PITCH;
        let selected = view.players.iter().any(|player| {
            player.joined && player.chart.is_some() && player.step_index == row.step_index
        });
        let panel_tint = if selected {
            [0.19, 0.30, 0.39, 1.0]
        } else {
            [0.10, 0.16, 0.23, 1.0]
        };
        actors.push(layout.panel(x - 113.0, y - 8.5, [249.0, 17.0], panel_tint, 2));
        let Some(chart) = row.chart else { continue };
        let level =
            if chart.difficulty.eq_ignore_ascii_case("edit") && !chart.description.is_empty() {
                chart.description.as_str()
            } else {
                chart.difficulty.as_str()
            };
        actors.push(layout.bounded_label(
            [x - 69.0, y],
            1.0,
            115.0,
            shared_text(level),
            0.73,
            opaque(INK),
        ));
        actors.push(layout.label(
            [x - 22.0, y],
            0.5,
            TextContent::inline_u32(chart.meter),
            0.89,
            opaque(ACCENT),
        ));
        actors.push(layout.panel(x + 3.0, y - 2.5, [85.0, 5.0], [0.15, 0.23, 0.34, 1.0], 3));
        let fill = 85.0 * (chart.meter as f32 / 12.0).clamp(0.0, 1.0);
        if fill > 0.0 {
            actors.push(layout.panel(x + 3.0, y - 2.5, [fill, 5.0], opaque(ACCENT), 4));
        }
        for (player, side) in view.players.iter().enumerate() {
            if side.joined && side.chart.is_some() && side.step_index == row.step_index {
                let (offset, label) = if player == 0 {
                    (103.0, "P1")
                } else {
                    (126.0, "P2")
                };
                actors.push(layout.label([x + offset, y], 0.5, label.into(), 0.72, opaque(ACCENT)));
            }
        }
    }
}

fn push_chart_pane(
    actors: &mut Vec<Actor>,
    layout: Layout,
    side: usize,
    player: &SelectMusicPlayerView<'_>,
) {
    let x = 280.0 + side as f32 * 165.0;
    let side_label = if side == 0 { "P1" } else { "P2" };
    let Some(chart) = player.chart else {
        if !player.name.is_empty() {
            actors.push(layout.bounded_label(
                [x + 79.5, 389.0],
                0.5,
                141.0,
                shared_text(player.name),
                0.80,
                opaque(INK),
            ));
        }
        return;
    };
    actors.push(layout.panel(x, 344.0, [159.0, 77.0], [0.10, 0.16, 0.23, 1.0], 2));
    actors.push(layout.panel(x, 344.0, [159.0, 1.0], opaque(ACCENT), 3));
    actors.push(layout.label(
        [x + 8.0, 355.0],
        0.0,
        side_label.into(),
        0.73,
        opaque(ACCENT),
    ));
    if !player.name.is_empty() && player.name != side_label {
        actors.push(layout.bounded_label(
            [x + 151.0, 355.0],
            1.0,
            112.0,
            shared_text(player.name),
            0.79,
            opaque(INK),
        ));
    }
    let stats = &chart.stats;
    for (index, (label, count)) in [
        ("TAPS", stats.total_steps),
        ("JUMPS", stats.jumps),
        ("HANDS", stats.hands),
        ("HOLDS", stats.holds),
        ("ROLLS", stats.rolls),
        ("MINES", chart.mines_nonfake),
    ]
    .into_iter()
    .enumerate()
    {
        let label = TextContent::inline_format(format_args!("{label} {count}"))
            .unwrap_or_else(|| TextContent::inline_u32(count));
        actors.push(layout.bounded_label(
            [
                x + 8.0 + (index % 3) as f32 * 49.0,
                369.0 + (index / 3) as f32 * 11.0,
            ],
            0.0,
            46.0,
            label,
            0.63,
            opaque(MUTED),
        ));
    }
    actors.push(layout.panel(x + 8.0, 388.0, [143.0, 1.0], [0.28, 0.36, 0.46, 1.0], 3));
    if let Some(record) = player.local_records.machine {
        actors.push(layout.label([x + 8.0, 397.0], 0.0, "MACHINE".into(), 0.64, opaque(MUTED)));
        if !record.name.is_empty() {
            actors.push(layout.bounded_label(
                [x + 100.0, 397.0],
                1.0,
                44.0,
                shared_text(&record.name),
                0.73,
                opaque(INK),
            ));
        }
        push_chart_score(actors, layout, x, 397.0, record.percent());
    }
    if let Some(record) = player.local_records.personal {
        actors.push(layout.label(
            [x + 8.0, 411.0],
            0.0,
            "PERSONAL BEST".into(),
            0.64,
            opaque(MUTED),
        ));
        push_chart_score(actors, layout, x, 411.0, record.percent());
    }
}

fn push_chart_score(actors: &mut Vec<Actor>, layout: Layout, x: f32, y: f32, percent: f64) {
    let score = TextContent::inline_format(format_args!("{percent:.2}%"))
        .expect("normalized percentages fit inline");
    actors.push(layout.label([x + 151.0, y], 1.0, score, 0.81, opaque(ACCENT)));
}

fn push_footer(actors: &mut Vec<Actor>, layout: Layout, view: &SelectMusicView<'_>) {
    actors.push(layout.label(
        [36.0, HEADER_Y],
        0.0,
        "PICK A TUNE".into(),
        1.55,
        opaque(INK),
    ));
    actors.push(layout.panel(36.0, 425.0, [568.0, 1.0], [0.32, 0.40, 0.48, 1.0], 2));
    if let Some(stage) = view.stage_number {
        let label = TextContent::inline_format(format_args!("STAGE {stage}"))
            .unwrap_or_else(|| "STAGE".into());
        actors.push(layout.label([36.0, 58.0], 0.0, label, 0.9, opaque(MUTED)));
    }
    if (view.music_rate - 1.0).abs() >= 0.005 {
        let label = TextContent::inline_format(format_args!("{:.2}x", view.music_rate))
            .unwrap_or_else(|| "RATE".into());
        actors.push(layout.label([604.0, 447.0], 1.0, label, 0.9, opaque(ACCENT)));
    }
}

fn push_prompt(actors: &mut Vec<Actor>, layout: Layout, prompt: SelectMusicPrompt) {
    match prompt {
        SelectMusicPrompt::Hidden => return,
        SelectMusicPrompt::OfferOptions => {
            actors.push(layout.label(
                [WIDTH * 0.5, 447.0],
                0.5,
                "PRESS START FOR PLAYER OPTIONS".into(),
                0.97,
                opaque(INK),
            ));
            return;
        }
        SelectMusicPrompt::EnteringOptions => {}
    }
    actors.push(quad(
        [0.0, 0.0],
        [screen_width(), screen_height()],
        [0.02, 0.03, 0.05, 0.70],
        1400,
    ));
    actors.push(layout.panel(112.0, 212.0, [416.0, 56.0], [0.11, 0.16, 0.23, 1.0], 1401));
    let mut heading = layout.bounded_label(
        [WIDTH * 0.5, HEIGHT * 0.5],
        0.5,
        384.0,
        "ENTERING PLAYER OPTIONS".into(),
        1.0,
        opaque(INK),
    );
    if let Actor::Text { z, .. } = &mut heading {
        *z = 1402;
    }
    actors.push(heading);
}

/// Present the course variant of the music wheel using its current rating and records.
pub fn push_select_course_actors(
    actors: &mut Vec<Actor>,
    view: SelectCourseView<'_>,
    banner: WheelBannerFrame<'_>,
) {
    let layout = push_backdrop(actors);
    push_details_frame(actors, layout);
    banner::push(actors, layout, banner, view.banner_texture, |_, _| {});
    push_wheel(
        actors,
        layout,
        &view.rows,
        view.wheel_offset,
        view.selection_beat,
        "NO COURSES",
    );
    actors.push(layout.label(
        [36.0, HEADER_Y],
        0.0,
        "PICK A COURSE".into(),
        1.55,
        opaque(INK),
    ));
    actors.push(layout.panel(36.0, 425.0, [568.0, 1.0], [0.32, 0.40, 0.48, 1.0], 2));
    if let Some(course) = &view.item {
        push_course_details(actors, layout, course);
    }
    for (player, name) in view.player_names.into_iter().enumerate() {
        if let Some(name) = name.filter(|name| !name.is_empty()) {
            let x = 343.0 + player as f32 * 125.0;
            actors.push(layout.bounded_label(
                [x, 447.0],
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
        });
        SelectMusicView {
            rows,
            wheel_offset: 0.0,
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
                        if *align == [0.5, 0.5] && *offset == [478.0 + delta, 112.0] && *w == 300.0 && *h == 128.0
                )));
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Text { content, offset, .. }
                        if content.as_str() == "Artist" && *offset == [280.0 + delta, 211.0]
                )));
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Sprite { offset, size: [SizeSpec::Px(w), SizeSpec::Px(h)], .. }
                        if *offset == [347.0 + delta, 315.5] && *w == 249.0 && *h == 17.0
                )));
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Text { content, offset, z, .. }
                        if content.as_str() == "ENTERING PLAYER OPTIONS" && *offset == [320.0 + delta, 240.0] && *z > 1401
                )));
            });
        }
    }
}
