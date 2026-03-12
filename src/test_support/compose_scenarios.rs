use crate::assets;
use crate::core::gfx::{BlendMode, MeshMode, MeshVertex, TexturedMeshVertex};
use crate::core::space::{Metrics, metrics_for_window};
use crate::test_support::density_graph_bench;
use crate::test_support::density_graph_life_bench;
use crate::test_support::music_wheel_bench;
use crate::test_support::pane_stats_bench;
use crate::ui::actors::{Actor, Background, SizeSpec, SpriteSource, TextAlign, TextContent};
use crate::ui::anim::{EffectMode, EffectState};
use crate::ui::font::{Font, Glyph};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

const SCENARIO_NAMES: [&str; 9] = [
    density_graph_bench::SCENARIO_NAME,
    density_graph_life_bench::SCENARIO_NAME,
    "hud",
    "text",
    "text-ci",
    "resolve-ci",
    "mask",
    music_wheel_bench::SCENARIO_NAME,
    pane_stats_bench::SCENARIO_NAME,
];
const BENCH_FONT: &str = "bench";
const MISO_FONT: &str = "miso";
const SCREENEVAL_FONT: &str = "wendy_screenevaluation";
const FONT_MAIN: &str = "bench/font_main.png";
const FONT_STROKE: &str = "bench/font_stroke.png";
const PANEL_TEX: &str = "bench/panel.png";
const BANNER_TEX: &str = "bench/banner.png";
const ICON_TEX: &str = "bench/icon.png";
const HAS_EDIT_TEX: &str = "has_edit.png";
const SHEET_TEX: &str = "bench/sheet.png";
const MESH_TEX: &str = "bench/mesh.png";
const CASEFOLD_TEX_COUNT: usize = 64;
const SCREEN_W: f32 = 854.0;
const SCREEN_H: f32 = 480.0;

pub struct ComposeScenario {
    pub name: &'static str,
    pub actors: Vec<Actor>,
    pub clear_color: [f32; 4],
    pub metrics: Metrics,
    pub fonts: HashMap<&'static str, Font>,
    pub total_elapsed: f32,
}

pub fn scenario_names() -> &'static [&'static str] {
    &SCENARIO_NAMES
}

pub fn build_scenario(name: &str) -> Option<ComposeScenario> {
    ensure_textures();
    let metrics = metrics_for_window(1920, 1080);
    let fonts = bench_fonts();
    match name {
        density_graph_bench::SCENARIO_NAME => Some(density_graph_scenario(metrics, fonts)),
        density_graph_life_bench::SCENARIO_NAME => {
            Some(density_graph_life_scenario(metrics, fonts))
        }
        "hud" => Some(hud_scenario(metrics, fonts)),
        "text" => Some(text_scenario(metrics, fonts)),
        "text-ci" => Some(text_ci_scenario(metrics, fonts)),
        "resolve-ci" => Some(resolve_ci_scenario(metrics, fonts)),
        "mask" => Some(mask_scenario(metrics, fonts)),
        music_wheel_bench::SCENARIO_NAME => Some(music_wheel_scenario(metrics, fonts)),
        pane_stats_bench::SCENARIO_NAME => Some(pane_stats_scenario(metrics, fonts)),
        _ => None,
    }
}

fn density_graph_scenario(metrics: Metrics, fonts: HashMap<&'static str, Font>) -> ComposeScenario {
    let fixture = density_graph_bench::fixture();
    ComposeScenario {
        name: density_graph_bench::SCENARIO_NAME,
        actors: fixture.build(),
        clear_color: [0.01, 0.02, 0.03, 1.0],
        metrics,
        fonts,
        total_elapsed: 0.0,
    }
}

fn density_graph_life_scenario(
    metrics: Metrics,
    fonts: HashMap<&'static str, Font>,
) -> ComposeScenario {
    let fixture = density_graph_life_bench::fixture();
    ComposeScenario {
        name: density_graph_life_bench::SCENARIO_NAME,
        actors: fixture.build(),
        clear_color: [0.01, 0.02, 0.03, 1.0],
        metrics,
        fonts,
        total_elapsed: 0.0,
    }
}

fn ensure_textures() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        for (key, w, h) in [
            ("__white", 1, 1),
            (FONT_MAIN, 512, 256),
            (FONT_STROKE, 512, 256),
            (PANEL_TEX, 512, 192),
            (BANNER_TEX, 512, 128),
            (ICON_TEX, 128, 128),
            (HAS_EDIT_TEX, 128, 64),
            (SHEET_TEX, 256, 256),
            (MESH_TEX, 256, 256),
        ] {
            assets::register_texture_dims(key, w, h);
        }
        for idx in 0..CASEFOLD_TEX_COUNT {
            let key = casefold_tex_key(idx);
            let mixed = mixed_case_texture_key(&key);
            assets::register_texture_dims(&key, 128, 64);
            assets::register_texture_dims(&mixed, 128, 64);
        }
    });
}

pub(crate) fn bench_fonts() -> HashMap<&'static str, Font> {
    let mut fonts = HashMap::with_capacity(3);
    fonts.insert(BENCH_FONT, bench_font());
    fonts.insert(MISO_FONT, bench_font());
    fonts.insert(SCREENEVAL_FONT, bench_font());
    fonts
}

fn bench_font() -> Font {
    let mut glyph_map = HashMap::with_capacity(95);
    for code in 32u8..=126 {
        let ch = char::from(code);
        glyph_map.insert(ch, bench_glyph(ch));
    }

    let mut stroke_texture_map = HashMap::with_capacity(1);
    stroke_texture_map.insert(FONT_MAIN.to_string(), FONT_STROKE.to_string());

    Font {
        glyph_map,
        default_glyph: Some(bench_glyph('?')),
        line_spacing: 20,
        height: 18,
        fallback_font_name: None,
        default_stroke_color: [0.05, 0.05, 0.05, 1.0],
        stroke_texture_map,
        texture_hints_map: HashMap::new(),
    }
}

fn bench_glyph(ch: char) -> Glyph {
    let idx = (ch as u32).saturating_sub(32);
    let col = idx % 16;
    let row = idx / 16;
    let x = col as f32 * 24.0;
    let y = row as f32 * 32.0;
    let advance = if ch == ' ' { 8.0 } else { 14.0 };
    Glyph {
        texture_key: FONT_MAIN.to_string(),
        tex_rect: [x, y, x + 22.0, y + 30.0],
        uv_scale: [22.0 / 512.0, 30.0 / 256.0],
        uv_offset: [x / 512.0, y / 256.0],
        size: [14.0, 18.0],
        offset: [0.0, -14.0],
        advance,
        advance_i32: advance.round_ties_even() as i32,
    }
}

fn hud_scenario(metrics: Metrics, fonts: HashMap<&'static str, Font>) -> ComposeScenario {
    let mut actors = Vec::with_capacity(7);
    actors.push(Actor::Frame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children: Vec::new(),
        background: Some(Background::Color([0.06, 0.07, 0.09, 1.0])),
        z: -20,
    });

    let x_offsets = [-330.0, -110.0, 110.0, 330.0];
    let y_offsets = [-120.0, 90.0];
    let labels = [
        "SCORE ATTACK",
        "LIFE GRAPH",
        "PACEMAKER",
        "STREAM INFO",
        "OFFSETS",
        "TIMING",
        "INPUT",
        "NETWORK",
    ];
    let mut label_idx = 0usize;
    for &y in &y_offsets {
        for &x in &x_offsets {
            actors.push(panel_frame(x, y, labels[label_idx]));
            label_idx += 1;
        }
    }

    ComposeScenario {
        name: "hud",
        actors,
        clear_color: [0.02, 0.03, 0.05, 1.0],
        metrics,
        fonts,
        total_elapsed: 12.5,
    }
}

fn pane_stats_scenario(metrics: Metrics, fonts: HashMap<&'static str, Font>) -> ComposeScenario {
    let fixture = pane_stats_bench::fixture();
    ComposeScenario {
        name: pane_stats_bench::SCENARIO_NAME,
        actors: fixture.build(),
        clear_color: [0.03, 0.04, 0.05, 1.0],
        metrics,
        fonts,
        total_elapsed: 0.41,
    }
}

fn panel_frame(x: f32, y: f32, label: &'static str) -> Actor {
    let [x, y] = screen_pos(x, y);
    Actor::Frame {
        align: [0.5, 0.5],
        offset: [x, y],
        size: [SizeSpec::Px(196.0), SizeSpec::Px(154.0)],
        background: Some(Background::Color([0.11, 0.13, 0.17, 0.95])),
        z: 0,
        children: vec![
            sprite_actor(BANNER_TEX, [0.5, 0.0], [0.0, 8.0], [180.0, 36.0], 1),
            sprite_actor(ICON_TEX, [0.0, 0.0], [12.0, 12.0], [28.0, 28.0], 2),
            animated_sheet([0.0, 1.0], [14.0, 114.0], [48.0, 28.0], 2),
            text_actor(label, [0.0, 0.0], [48.0, 12.0], [0.95, 0.97, 1.0, 1.0], 3),
            text_actor(
                "95.27%  0015 EX",
                [0.0, 0.0],
                [14.0, 62.0],
                [0.74, 0.86, 0.98, 1.0],
                3,
            ),
            text_actor(
                "J4  C725  M003  H000",
                [0.0, 0.0],
                [14.0, 88.0],
                [0.84, 0.84, 0.88, 1.0],
                3,
            ),
        ],
    }
}

fn text_scenario(metrics: Metrics, fonts: HashMap<&'static str, Font>) -> ComposeScenario {
    let mut actors = Vec::with_capacity(25);
    actors.push(Actor::Frame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children: Vec::new(),
        background: Some(Background::Color([0.04, 0.02, 0.02, 1.0])),
        z: -20,
    });

    let lines = [
        "JUDGE WINDOW STABILITY CHECK",
        "MEASURE 128  NPS 17.35  STREAM 42.1S",
        "OFFSET +0.007  BIAS +0.002  DROPS 000",
        "COMPOSE HOT PATH  TEXT STROKE  CLIP TEST",
    ];
    for row in 0..6 {
        for col in 0..4 {
            let idx = (row + col) % lines.len();
            actors.push(stroked_text_actor(
                lines[idx],
                -360.0 + col as f32 * 240.0,
                -170.0 + row as f32 * 62.0,
                row,
            ));
        }
    }

    ComposeScenario {
        name: "text",
        actors,
        clear_color: [0.02, 0.01, 0.01, 1.0],
        metrics,
        fonts,
        total_elapsed: 24.0,
    }
}

fn text_ci_scenario(metrics: Metrics, mut fonts: HashMap<&'static str, Font>) -> ComposeScenario {
    for font in fonts.values_mut() {
        remap_font_texture_case(font);
    }
    let mut scenario = text_scenario(metrics, fonts);
    scenario.name = "text-ci";
    remap_actor_texture_case(&mut scenario.actors);
    scenario
}

fn resolve_ci_scenario(metrics: Metrics, fonts: HashMap<&'static str, Font>) -> ComposeScenario {
    let mut actors = Vec::with_capacity(1 + 48 * 32);
    actors.push(Actor::Frame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children: Vec::new(),
        background: Some(Background::Color([0.03, 0.03, 0.04, 1.0])),
        z: -20,
    });

    for row in 0..32 {
        for col in 0..48 {
            let idx = (row * 17 + col * 11) % CASEFOLD_TEX_COUNT;
            let x = -392.0 + col as f32 * 16.5;
            let y = -220.0 + row as f32 * 14.0;
            actors.push(Actor::Sprite {
                align: [0.0, 0.0],
                offset: screen_pos(x, y),
                size: [SizeSpec::Px(16.0), SizeSpec::Px(12.0)],
                source: SpriteSource::Texture(Arc::<str>::from(mixed_case_texture_key(
                    &casefold_tex_key(idx),
                ))),
                tint: [0.85, 0.9, 1.0, 0.9],
                glow: [0.0; 4],
                z: 1,
                cell: None,
                grid: None,
                uv_rect: None,
                visible: true,
                flip_x: false,
                flip_y: false,
                cropleft: 0.0,
                cropright: 0.0,
                croptop: 0.0,
                cropbottom: 0.0,
                fadeleft: 0.0,
                faderight: 0.0,
                fadetop: 0.0,
                fadebottom: 0.0,
                blend: BlendMode::Alpha,
                mask_source: false,
                mask_dest: false,
                rot_x_deg: 0.0,
                rot_y_deg: 0.0,
                rot_z_deg: 0.0,
                local_offset: [0.0, 0.0],
                local_offset_rot_sin_cos: [0.0, 1.0],
                texcoordvelocity: None,
                animate: false,
                state_delay: 0.0,
                scale: [1.0, 1.0],
                effect: EffectState::default(),
            });
        }
    }

    ComposeScenario {
        name: "resolve-ci",
        actors,
        clear_color: [0.02, 0.02, 0.03, 1.0],
        metrics,
        fonts,
        total_elapsed: 5.0,
    }
}

fn stroked_text_actor(text: &'static str, x: f32, y: f32, row: usize) -> Actor {
    let [x, y] = screen_pos(x, y);
    let scale = 0.9 + (row % 3) as f32 * 0.1;
    Actor::Text {
        align: [0.0, 0.0],
        offset: [x, y],
        color: [0.92, 0.94, 0.98, 0.96],
        stroke_color: Some([0.05, 0.08, 0.12, 0.9]),
        glow: [0.0; 4],
        font: BENCH_FONT,
        content: TextContent::Shared(Arc::<str>::from(text)),
        align_text: if row % 2 == 0 {
            TextAlign::Left
        } else {
            TextAlign::Center
        },
        z: 2,
        scale: [scale, scale],
        fit_width: None,
        fit_height: None,
        max_width: Some(220.0),
        max_height: Some(22.0),
        max_w_pre_zoom: row % 2 == 0,
        max_h_pre_zoom: false,
        clip: Some([x, y, 210.0, 24.0]),
        blend: BlendMode::Alpha,
        effect: EffectState {
            mode: EffectMode::Pulse,
            magnitude: [0.98, 1.02, 1.0],
            ..EffectState::default()
        },
    }
}

fn mask_scenario(metrics: Metrics, fonts: HashMap<&'static str, Font>) -> ComposeScenario {
    let mut actors = Vec::with_capacity(40);
    actors.push(Actor::Frame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children: Vec::new(),
        background: Some(Background::Color([0.01, 0.03, 0.05, 1.0])),
        z: -20,
    });
    actors.push(mask_source_actor());

    for idx in 0..24 {
        let col = idx % 6;
        let row = idx / 6;
        let x = -310.0 + col as f32 * 124.0;
        let y = -130.0 + row as f32 * 96.0;
        actors.push(masked_rotating_sprite(x, y, idx as f32 * 7.5));
    }

    actors.push(shadowed_sprite(-280.0, 168.0));
    actors.push(shadowed_sprite(280.0, 168.0));
    actors.push(colored_mesh());
    actors.push(textured_mesh());

    ComposeScenario {
        name: "mask",
        actors,
        clear_color: [0.0, 0.02, 0.04, 1.0],
        metrics,
        fonts,
        total_elapsed: 37.0,
    }
}

fn music_wheel_scenario(metrics: Metrics, fonts: HashMap<&'static str, Font>) -> ComposeScenario {
    let fixture = music_wheel_bench::fixture();
    ComposeScenario {
        name: music_wheel_bench::SCENARIO_NAME,
        actors: fixture.build(),
        clear_color: [0.02, 0.03, 0.05, 1.0],
        metrics,
        fonts,
        total_elapsed: 18.0,
    }
}

fn mask_source_actor() -> Actor {
    let [x, y] = screen_pos(0.0, 0.0);
    Actor::Sprite {
        align: [0.5, 0.5],
        offset: [x, y],
        size: [SizeSpec::Px(680.0), SizeSpec::Px(300.0)],
        source: SpriteSource::Solid,
        tint: [1.0; 4],
        glow: [0.0; 4],
        z: 0,
        cell: None,
        grid: None,
        uv_rect: None,
        visible: true,
        flip_x: false,
        flip_y: false,
        cropleft: 0.0,
        cropright: 0.0,
        croptop: 0.0,
        cropbottom: 0.0,
        fadeleft: 0.0,
        faderight: 0.0,
        fadetop: 0.0,
        fadebottom: 0.0,
        blend: BlendMode::Alpha,
        mask_source: true,
        mask_dest: false,
        rot_x_deg: 0.0,
        rot_y_deg: 0.0,
        rot_z_deg: 0.0,
        local_offset: [0.0, 0.0],
        local_offset_rot_sin_cos: [0.0, 1.0],
        texcoordvelocity: None,
        animate: false,
        state_delay: 0.0,
        scale: [1.0, 1.0],
        effect: EffectState::default(),
    }
}

fn masked_rotating_sprite(x: f32, y: f32, rot_z_deg: f32) -> Actor {
    let [x, y] = screen_pos(x, y);
    Actor::Sprite {
        align: [0.5, 0.5],
        offset: [x, y],
        size: [SizeSpec::Px(116.0), SizeSpec::Px(64.0)],
        source: SpriteSource::Texture(Arc::<str>::from(PANEL_TEX)),
        tint: [0.82, 0.93, 1.0, 0.95],
        glow: [0.0; 4],
        z: 1,
        cell: None,
        grid: None,
        uv_rect: None,
        visible: true,
        flip_x: false,
        flip_y: false,
        cropleft: 0.08,
        cropright: 0.12,
        croptop: 0.04,
        cropbottom: 0.0,
        fadeleft: 0.08,
        faderight: 0.08,
        fadetop: 0.08,
        fadebottom: 0.0,
        blend: BlendMode::Alpha,
        mask_source: false,
        mask_dest: true,
        rot_x_deg: 0.0,
        rot_y_deg: 0.0,
        rot_z_deg,
        local_offset: [0.0, 0.0],
        local_offset_rot_sin_cos: [0.0, 1.0],
        texcoordvelocity: Some([0.03, -0.02]),
        animate: false,
        state_delay: 0.0,
        scale: [1.0, 1.0],
        effect: EffectState {
            mode: EffectMode::DiffuseShift,
            color1: [1.0, 1.0, 1.0, 1.0],
            color2: [0.6, 0.8, 1.0, 0.85],
            ..EffectState::default()
        },
    }
}

fn shadowed_sprite(x: f32, y: f32) -> Actor {
    let [x, y] = screen_pos(x, y);
    Actor::Shadow {
        len: [7.0, -7.0],
        color: [0.0, 0.0, 0.0, 0.55],
        child: Box::new(sprite_actor(ICON_TEX, [0.5, 0.5], [x, y], [80.0, 80.0], 2)),
    }
}

fn colored_mesh() -> Actor {
    let verts = Arc::<[MeshVertex]>::from(vec![
        MeshVertex {
            pos: [-70.0, -30.0],
            color: [0.95, 0.25, 0.25, 0.8],
        },
        MeshVertex {
            pos: [70.0, -20.0],
            color: [0.25, 0.9, 0.55, 0.8],
        },
        MeshVertex {
            pos: [0.0, 48.0],
            color: [0.25, 0.45, 0.98, 0.8],
        },
    ]);
    Actor::Mesh {
        align: [0.5, 0.5],
        offset: screen_pos(0.0, 178.0),
        size: [SizeSpec::Px(140.0), SizeSpec::Px(80.0)],
        vertices: verts,
        mode: MeshMode::Triangles,
        visible: true,
        blend: BlendMode::Add,
        z: 2,
    }
}

fn textured_mesh() -> Actor {
    let verts = Arc::<[TexturedMeshVertex]>::from(vec![
        textured_vertex([-60.0, -40.0], [0.0, 1.0]),
        textured_vertex([60.0, -40.0], [1.0, 1.0]),
        textured_vertex([60.0, 40.0], [1.0, 0.0]),
        textured_vertex([-60.0, -40.0], [0.0, 1.0]),
        textured_vertex([60.0, 40.0], [1.0, 0.0]),
        textured_vertex([-60.0, 40.0], [0.0, 0.0]),
    ]);
    Actor::TexturedMesh {
        align: [0.5, 0.5],
        offset: screen_pos(0.0, -184.0),
        size: [SizeSpec::Px(120.0), SizeSpec::Px(80.0)],
        texture: Arc::<str>::from(MESH_TEX),
        vertices: verts,
        mode: MeshMode::Triangles,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        visible: true,
        blend: BlendMode::Alpha,
        z: 2,
    }
}

fn textured_vertex(pos: [f32; 2], uv: [f32; 2]) -> TexturedMeshVertex {
    TexturedMeshVertex {
        pos,
        uv,
        tex_matrix_scale: [1.0, 1.0],
        color: [1.0; 4],
    }
}

fn sprite_actor(
    texture: &'static str,
    align: [f32; 2],
    offset: [f32; 2],
    size: [f32; 2],
    z: i16,
) -> Actor {
    Actor::Sprite {
        align,
        offset,
        size: [SizeSpec::Px(size[0]), SizeSpec::Px(size[1])],
        source: SpriteSource::Texture(Arc::<str>::from(texture)),
        tint: [1.0; 4],
        glow: [0.0; 4],
        z,
        cell: None,
        grid: None,
        uv_rect: None,
        visible: true,
        flip_x: false,
        flip_y: false,
        cropleft: 0.0,
        cropright: 0.0,
        croptop: 0.0,
        cropbottom: 0.0,
        fadeleft: 0.0,
        faderight: 0.0,
        fadetop: 0.0,
        fadebottom: 0.0,
        blend: BlendMode::Alpha,
        mask_source: false,
        mask_dest: false,
        rot_x_deg: 0.0,
        rot_y_deg: 0.0,
        rot_z_deg: 0.0,
        local_offset: [0.0, 0.0],
        local_offset_rot_sin_cos: [0.0, 1.0],
        texcoordvelocity: None,
        animate: false,
        state_delay: 0.0,
        scale: [1.0, 1.0],
        effect: EffectState::default(),
    }
}

fn animated_sheet(align: [f32; 2], offset: [f32; 2], size: [f32; 2], z: i16) -> Actor {
    Actor::Sprite {
        align,
        offset,
        size: [SizeSpec::Px(size[0]), SizeSpec::Px(size[1])],
        source: SpriteSource::Texture(Arc::<str>::from(SHEET_TEX)),
        tint: [0.9, 0.95, 1.0, 0.95],
        glow: [0.0; 4],
        z,
        cell: Some((0, u32::MAX)),
        grid: Some((4, 4)),
        uv_rect: None,
        visible: true,
        flip_x: false,
        flip_y: false,
        cropleft: 0.0,
        cropright: 0.0,
        croptop: 0.0,
        cropbottom: 0.0,
        fadeleft: 0.0,
        faderight: 0.0,
        fadetop: 0.0,
        fadebottom: 0.0,
        blend: BlendMode::Add,
        mask_source: false,
        mask_dest: false,
        rot_x_deg: 0.0,
        rot_y_deg: 0.0,
        rot_z_deg: 0.0,
        local_offset: [0.0, 0.0],
        local_offset_rot_sin_cos: [0.0, 1.0],
        texcoordvelocity: Some([0.02, 0.0]),
        animate: true,
        state_delay: 0.08,
        scale: [1.0, 1.0],
        effect: EffectState {
            mode: EffectMode::Spin,
            magnitude: [0.0, 0.0, 32.0],
            ..EffectState::default()
        },
    }
}

fn text_actor(
    text: &'static str,
    align: [f32; 2],
    offset: [f32; 2],
    color: [f32; 4],
    z: i16,
) -> Actor {
    Actor::Text {
        align,
        offset,
        color,
        stroke_color: None,
        glow: [0.0; 4],
        font: BENCH_FONT,
        content: TextContent::Shared(Arc::<str>::from(text)),
        align_text: TextAlign::Left,
        z,
        scale: [1.0, 1.0],
        fit_width: None,
        fit_height: None,
        max_width: None,
        max_height: None,
        max_w_pre_zoom: false,
        max_h_pre_zoom: false,
        clip: None,
        blend: BlendMode::Alpha,
        effect: EffectState::default(),
    }
}

fn remap_font_texture_case(font: &mut Font) {
    for glyph in font.glyph_map.values_mut() {
        glyph.texture_key = mixed_case_texture_key(&glyph.texture_key);
    }
    if let Some(glyph) = font.default_glyph.as_mut() {
        glyph.texture_key = mixed_case_texture_key(&glyph.texture_key);
    }

    let mut stroke_texture_map = HashMap::with_capacity(font.stroke_texture_map.len());
    for (key, value) in std::mem::take(&mut font.stroke_texture_map) {
        stroke_texture_map.insert(mixed_case_texture_key(&key), mixed_case_texture_key(&value));
    }
    font.stroke_texture_map = stroke_texture_map;
}

fn remap_actor_texture_case(actors: &mut [Actor]) {
    for actor in actors {
        match actor {
            Actor::Sprite {
                source: SpriteSource::Texture(texture),
                ..
            } => *texture = Arc::<str>::from(mixed_case_texture_key(texture.as_ref())),
            Actor::Sprite {
                source: SpriteSource::Solid,
                ..
            } => {}
            Actor::TexturedMesh { texture, .. } => {
                *texture = Arc::<str>::from(mixed_case_texture_key(texture.as_ref()));
            }
            Actor::Frame {
                background,
                children,
                ..
            } => {
                if let Some(Background::Texture(texture)) = background {
                    *texture = Box::leak(mixed_case_texture_key(texture).into_boxed_str());
                }
                remap_actor_texture_case(children);
            }
            Actor::Camera { children, .. } => remap_actor_texture_case(children),
            Actor::Shadow { child, .. } => {
                remap_actor_texture_case(std::slice::from_mut(child.as_mut()))
            }
            Actor::Text { .. } | Actor::Mesh { .. } => {}
        }
    }
}

fn mixed_case_texture_key(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for (idx, ch) in input.bytes().enumerate() {
        out.push(char::from(if ch.is_ascii_lowercase() && idx % 2 == 0 {
            ch.to_ascii_uppercase()
        } else {
            ch
        }));
    }
    out
}

fn casefold_tex_key(idx: usize) -> String {
    format!("bench/casefold/tex{:02}.png", idx)
}

fn screen_pos(x: f32, y: f32) -> [f32; 2] {
    [0.5f32.mul_add(SCREEN_W, x), 0.5f32.mul_add(SCREEN_H, y)]
}
