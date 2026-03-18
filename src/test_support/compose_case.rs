use crate::assets;
use crate::core::gfx::{
    BlendMode, MeshMode, MeshVertex, ObjectType, RenderList, RenderObject, TexturedMeshVertex,
};
use crate::core::space::Metrics;
use crate::ui::actors::{Actor, Background, SizeSpec, SpriteSource, TextAlign, TextContent};
use crate::ui::anim::{EffectClock, EffectMode, EffectState};
use crate::ui::compose;
use crate::ui::font::{Font, Glyph};
use cgmath::Matrix4;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use twox_hash::XxHash64;

const CASE_VERSION: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComposeCase {
    pub version: u32,
    pub screen: String,
    pub total_elapsed: f32,
    pub clear_color: [f32; 4],
    pub metrics: MetricsSnapshot,
    pub textures: BTreeMap<String, TextureMetaSnapshot>,
    pub fonts: BTreeMap<String, FontSnapshot>,
    pub actors: Vec<ActorSnapshot>,
    pub expected: ComposeOutputExpectation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComposeOutputExpectation {
    pub output_hash: String,
    pub objects: usize,
    pub cameras: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextureMetaSnapshot {
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FontSnapshot {
    pub glyphs: Vec<GlyphEntrySnapshot>,
    pub default_glyph: Option<GlyphSnapshot>,
    pub line_spacing: i32,
    pub height: i32,
    pub fallback_font_name: Option<String>,
    pub default_stroke_color: [f32; 4],
    pub stroke_texture_map: BTreeMap<String, String>,
    pub texture_hints_map: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GlyphEntrySnapshot {
    pub codepoint: u32,
    pub glyph: GlyphSnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GlyphSnapshot {
    pub texture_key: String,
    pub tex_rect: [f32; 4],
    pub uv_scale: [f32; 2],
    pub uv_offset: [f32; 2],
    pub size: [f32; 2],
    pub offset: [f32; 2],
    pub advance: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ActorSnapshot {
    Sprite {
        align: [f32; 2],
        offset: [f32; 2],
        size: [SizeSpecSnapshot; 2],
        source: SpriteSourceSnapshot,
        tint: [f32; 4],
        glow: [f32; 4],
        z: i16,
        cell: Option<(u32, u32)>,
        grid: Option<(u32, u32)>,
        uv_rect: Option<[f32; 4]>,
        visible: bool,
        flip_x: bool,
        flip_y: bool,
        cropleft: f32,
        cropright: f32,
        croptop: f32,
        cropbottom: f32,
        fadeleft: f32,
        faderight: f32,
        fadetop: f32,
        fadebottom: f32,
        blend: BlendModeSnapshot,
        mask_source: bool,
        mask_dest: bool,
        rot_x_deg: f32,
        rot_y_deg: f32,
        rot_z_deg: f32,
        local_offset: [f32; 2],
        local_offset_rot_sin_cos: [f32; 2],
        texcoordvelocity: Option<[f32; 2]>,
        animate: bool,
        state_delay: f32,
        scale: [f32; 2],
        effect: EffectStateSnapshot,
    },
    Text {
        align: [f32; 2],
        offset: [f32; 2],
        color: [f32; 4],
        stroke_color: Option<[f32; 4]>,
        glow: [f32; 4],
        font: String,
        content: String,
        align_text: TextAlignSnapshot,
        z: i16,
        scale: [f32; 2],
        fit_width: Option<f32>,
        fit_height: Option<f32>,
        wrap_width_pixels: Option<i32>,
        max_width: Option<f32>,
        max_height: Option<f32>,
        max_w_pre_zoom: bool,
        max_h_pre_zoom: bool,
        clip: Option<[f32; 4]>,
        blend: BlendModeSnapshot,
        effect: EffectStateSnapshot,
    },
    Mesh {
        align: [f32; 2],
        offset: [f32; 2],
        size: [SizeSpecSnapshot; 2],
        vertices: Vec<MeshVertex>,
        mode: MeshModeSnapshot,
        visible: bool,
        blend: BlendModeSnapshot,
        z: i16,
    },
    TexturedMesh {
        align: [f32; 2],
        offset: [f32; 2],
        size: [SizeSpecSnapshot; 2],
        texture: String,
        vertices: Vec<TexturedMeshVertex>,
        mode: MeshModeSnapshot,
        uv_scale: [f32; 2],
        uv_offset: [f32; 2],
        uv_tex_shift: [f32; 2],
        visible: bool,
        blend: BlendModeSnapshot,
        z: i16,
    },
    Frame {
        align: [f32; 2],
        offset: [f32; 2],
        size: [SizeSpecSnapshot; 2],
        children: Vec<Self>,
        background: Option<BackgroundSnapshot>,
        z: i16,
    },
    Camera {
        view_proj: [[f32; 4]; 4],
        children: Vec<Self>,
    },
    Shadow {
        len: [f32; 2],
        color: [f32; 4],
        child: Box<Self>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SpriteSourceSnapshot {
    Texture(String),
    Solid,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BackgroundSnapshot {
    Color([f32; 4]),
    Texture(String),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum SizeSpecSnapshot {
    Px(f32),
    Fill,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum TextAlignSnapshot {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum BlendModeSnapshot {
    Alpha,
    Add,
    Multiply,
    Subtract,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum MeshModeSnapshot {
    Triangles,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum EffectClockSnapshot {
    Time,
    Beat,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum EffectModeSnapshot {
    None,
    DiffuseRamp,
    DiffuseShift,
    GlowShift,
    Pulse,
    Spin,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct EffectStateSnapshot {
    pub clock: EffectClockSnapshot,
    pub mode: EffectModeSnapshot,
    pub color1: [f32; 4],
    pub color2: [f32; 4],
    pub period: f32,
    pub offset: f32,
    pub timing: [f32; 5],
    pub magnitude: [f32; 3],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RenderListSnapshot {
    pub clear_color: [f32; 4],
    pub cameras: Vec<[[f32; 4]; 4]>,
    pub objects: Vec<RenderObjectSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActorListSnapshot {
    pub actors: Vec<ActorSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextureResolveSnapshot {
    pub objects: Vec<TextureResolveObjectSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextureResolveObjectSnapshot {
    pub texture_id: Option<String>,
    pub texture_handle: crate::core::gfx::TextureHandle,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RenderObjectSnapshot {
    pub object_type: RenderObjectTypeSnapshot,
    pub transform: [[f32; 4]; 4],
    pub blend: BlendModeSnapshot,
    pub z: i16,
    pub order: u32,
    pub camera: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RenderObjectTypeSnapshot {
    Sprite {
        texture_id: String,
        tint: [f32; 4],
        uv_scale: [f32; 2],
        uv_offset: [f32; 2],
        local_offset: [f32; 2],
        local_offset_rot_sin_cos: [f32; 2],
        edge_fade: [f32; 4],
    },
    Mesh {
        vertices: Vec<MeshVertex>,
        mode: MeshModeSnapshot,
    },
    TexturedMesh {
        texture_id: String,
        vertices: Vec<TexturedMeshVertex>,
        mode: MeshModeSnapshot,
        uv_scale: [f32; 2],
        uv_offset: [f32; 2],
        uv_tex_shift: [f32; 2],
    },
}

pub struct ReplayCase {
    pub screen: String,
    pub clear_color: [f32; 4],
    pub metrics: Metrics,
    pub total_elapsed: f32,
    pub actors: Vec<Actor>,
    pub fonts: HashMap<&'static str, Font>,
    pub expected: ComposeOutputExpectation,
}

pub fn capture_case(
    screen: &str,
    actors: &[Actor],
    clear_color: [f32; 4],
    metrics: &Metrics,
    fonts: &HashMap<&'static str, Font>,
    total_elapsed: f32,
) -> Result<(ComposeCase, RenderListSnapshot), Box<dyn Error>> {
    let font_names = collect_font_names(actors, fonts);
    let textures = collect_texture_meta(actors, fonts, &font_names);
    let font_snapshots = font_names
        .iter()
        .filter_map(|name| {
            fonts
                .get(name.as_str())
                .map(|font| (name.clone(), font_snapshot(font)))
        })
        .collect::<BTreeMap<_, _>>();
    let actor_snapshots = actors.iter().map(actor_snapshot).collect::<Vec<_>>();
    let render = compose::build_screen(actors, clear_color, metrics, fonts, total_elapsed);
    let render_snapshot = render_list_snapshot(&render);
    let output_hash = render_snapshot_hash(&render_snapshot)?;

    Ok((
        ComposeCase {
            version: CASE_VERSION,
            screen: screen.to_string(),
            total_elapsed,
            clear_color,
            metrics: MetricsSnapshot::from(*metrics),
            textures,
            fonts: font_snapshots,
            actors: actor_snapshots,
            expected: ComposeOutputExpectation {
                output_hash,
                objects: render_snapshot.objects.len(),
                cameras: render_snapshot.cameras.len(),
            },
        },
        render_snapshot,
    ))
}

pub fn replay_case(case: &ComposeCase) -> Result<ReplayCase, Box<dyn Error>> {
    if case.version != CASE_VERSION {
        return Err(format!(
            "unsupported compose case version {}, expected {}",
            case.version, CASE_VERSION
        )
        .into());
    }

    for (key, meta) in &case.textures {
        assets::register_texture_dims(key, meta.w, meta.h);
    }

    let mut name_map = HashMap::with_capacity(case.fonts.len());
    for name in case.fonts.keys() {
        name_map.insert(name.clone(), leak_str(name));
    }

    let mut fonts = HashMap::with_capacity(case.fonts.len());
    for (name, font) in &case.fonts {
        let leaked = *name_map
            .get(name)
            .ok_or_else(|| format!("missing leaked font name '{name}'"))?;
        fonts.insert(leaked, font_runtime(font, &name_map));
    }

    let actors = case
        .actors
        .iter()
        .map(|actor| actor_runtime(actor, &name_map))
        .collect::<Vec<_>>();

    Ok(ReplayCase {
        screen: case.screen.clone(),
        clear_color: case.clear_color,
        metrics: case.metrics.into(),
        total_elapsed: case.total_elapsed,
        actors,
        fonts,
        expected: case.expected.clone(),
    })
}

pub fn render_case_output(case: &ComposeCase) -> Result<RenderListSnapshot, Box<dyn Error>> {
    let replay = replay_case(case)?;
    Ok(render_list_snapshot(&compose::build_screen(
        &replay.actors,
        replay.clear_color,
        &replay.metrics,
        &replay.fonts,
        replay.total_elapsed,
    )))
}

pub fn asset_manager_for_case(case: &ComposeCase) -> Result<assets::AssetManager, Box<dyn Error>> {
    asset_manager_for_case_impl(case, |key| key.to_string())
}

pub fn asset_manager_for_case_lowercase(
    case: &ComposeCase,
) -> Result<assets::AssetManager, Box<dyn Error>> {
    asset_manager_for_case_impl(case, |key| key.to_ascii_lowercase())
}

fn asset_manager_for_case_impl(
    case: &ComposeCase,
    map_key: impl Fn(&str) -> String,
) -> Result<assets::AssetManager, Box<dyn Error>> {
    let mut assets = assets::AssetManager::new();
    for key in case
        .textures
        .keys()
        .map(String::as_str)
        .chain(["__white", "__black"])
        .map(map_key)
        .collect::<BTreeSet<_>>()
    {
        assets.reserve_texture_handle(key);
    }

    Ok(assets)
}

pub fn render_snapshot_hash(snapshot: &RenderListSnapshot) -> Result<String, Box<dyn Error>> {
    let bytes = serde_json::to_vec(snapshot)?;
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(&bytes);
    Ok(format!("{:016x}", hasher.finish()))
}

pub fn actor_list_snapshot(actors: &[Actor]) -> ActorListSnapshot {
    ActorListSnapshot {
        actors: actors.iter().map(actor_snapshot).collect(),
    }
}

pub fn actor_snapshot_hash(snapshot: &ActorListSnapshot) -> Result<String, Box<dyn Error>> {
    let bytes = serde_json::to_vec(snapshot)?;
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(&bytes);
    Ok(format!("{:016x}", hasher.finish()))
}

pub fn texture_resolve_snapshot(render: &RenderList<'_>) -> TextureResolveSnapshot {
    TextureResolveSnapshot {
        objects: render
            .objects
            .iter()
            .map(texture_resolve_object_snapshot)
            .collect(),
    }
}

pub fn texture_resolve_snapshot_hash(
    snapshot: &TextureResolveSnapshot,
) -> Result<String, Box<dyn Error>> {
    let bytes = serde_json::to_vec(snapshot)?;
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(&bytes);
    Ok(format!("{:016x}", hasher.finish()))
}

pub fn write_case(path: &Path, case: &ComposeCase) -> Result<(), Box<dyn Error>> {
    write_json(path, case)
}

pub fn write_render_snapshot(
    path: &Path,
    snapshot: &RenderListSnapshot,
) -> Result<(), Box<dyn Error>> {
    write_json(path, snapshot)
}

pub fn write_actor_snapshot(
    path: &Path,
    snapshot: &ActorListSnapshot,
) -> Result<(), Box<dyn Error>> {
    write_json(path, snapshot)
}

pub fn write_texture_resolve_snapshot(
    path: &Path,
    snapshot: &TextureResolveSnapshot,
) -> Result<(), Box<dyn Error>> {
    write_json(path, snapshot)
}

pub fn read_case(path: &Path) -> Result<ComposeCase, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn read_render_snapshot(path: &Path) -> Result<RenderListSnapshot, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn render_list_runtime(snapshot: &RenderListSnapshot) -> RenderList<'static> {
    RenderList {
        clear_color: snapshot.clear_color,
        cameras: snapshot
            .cameras
            .iter()
            .copied()
            .map(matrix_runtime)
            .collect(),
        objects: snapshot.objects.iter().map(render_object_runtime).collect(),
    }
}

pub fn default_capture_paths(screen: &str) -> (PathBuf, PathBuf) {
    let stem = format!(
        "{}-{}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S"),
        sanitize_label(screen)
    );
    let dir = Path::new("save").join("compose-captures");
    (
        dir.join(format!("{stem}.case.json")),
        dir.join(format!("{stem}.output.json")),
    )
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn sanitize_label(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn collect_font_names(actors: &[Actor], fonts: &HashMap<&'static str, Font>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for actor in actors {
        collect_font_names_actor(actor, fonts, &mut out);
    }
    out
}

fn collect_font_names_actor(
    actor: &Actor,
    fonts: &HashMap<&'static str, Font>,
    out: &mut BTreeSet<String>,
) {
    match actor {
        Actor::Text { font, .. } => collect_font_chain(font, fonts, out),
        Actor::Frame { children, .. } | Actor::Camera { children, .. } => {
            for child in children {
                collect_font_names_actor(child, fonts, out);
            }
        }
        Actor::Shadow { child, .. } => collect_font_names_actor(child, fonts, out),
        Actor::Sprite { .. } | Actor::Mesh { .. } | Actor::TexturedMesh { .. } => {}
    }
}

fn collect_font_chain(name: &str, fonts: &HashMap<&'static str, Font>, out: &mut BTreeSet<String>) {
    let mut current = Some(name);
    while let Some(font_name) = current {
        if !out.insert(font_name.to_string()) {
            break;
        }
        current = fonts
            .get(font_name)
            .and_then(|font| font.fallback_font_name);
    }
}

fn collect_texture_meta(
    actors: &[Actor],
    fonts: &HashMap<&'static str, Font>,
    font_names: &BTreeSet<String>,
) -> BTreeMap<String, TextureMetaSnapshot> {
    let mut keys = BTreeSet::new();
    for actor in actors {
        collect_actor_texture_keys(actor, &mut keys);
    }
    for name in font_names {
        let Some(font) = fonts.get(name.as_str()) else {
            continue;
        };
        for glyph in font.glyph_map.values() {
            keys.insert(glyph.texture_key.clone());
        }
        if let Some(glyph) = &font.default_glyph {
            keys.insert(glyph.texture_key.clone());
        }
        keys.extend(font.stroke_texture_map.keys().cloned());
        keys.extend(font.stroke_texture_map.values().cloned());
    }

    let mut out = BTreeMap::new();
    for key in keys {
        if let Some(meta) = assets::texture_dims(&key) {
            out.insert(
                key,
                TextureMetaSnapshot {
                    w: meta.w,
                    h: meta.h,
                },
            );
        }
    }
    out
}

fn collect_actor_texture_keys(actor: &Actor, out: &mut BTreeSet<String>) {
    match actor {
        Actor::Sprite { source, .. } => {
            if let SpriteSource::Texture(key) = source {
                out.insert(key.to_string());
            }
        }
        Actor::TexturedMesh { texture, .. } => {
            out.insert(texture.to_string());
        }
        Actor::Frame {
            children,
            background,
            ..
        } => {
            if let Some(Background::Texture(tex)) = background {
                out.insert((*tex).to_string());
            }
            for child in children {
                collect_actor_texture_keys(child, out);
            }
        }
        Actor::Camera { children, .. } => {
            for child in children {
                collect_actor_texture_keys(child, out);
            }
        }
        Actor::Shadow { child, .. } => collect_actor_texture_keys(child, out),
        Actor::Text { .. } | Actor::Mesh { .. } => {}
    }
}

fn font_snapshot(font: &Font) -> FontSnapshot {
    let mut glyphs = font
        .glyph_map
        .iter()
        .map(|(&ch, glyph)| GlyphEntrySnapshot {
            codepoint: ch as u32,
            glyph: glyph_snapshot(glyph),
        })
        .collect::<Vec<_>>();
    glyphs.sort_unstable_by_key(|entry| entry.codepoint);
    FontSnapshot {
        glyphs,
        default_glyph: font.default_glyph.as_ref().map(glyph_snapshot),
        line_spacing: font.line_spacing,
        height: font.height,
        fallback_font_name: font.fallback_font_name.map(str::to_string),
        default_stroke_color: font.default_stroke_color,
        stroke_texture_map: font
            .stroke_texture_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        texture_hints_map: font
            .texture_hints_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    }
}

fn font_runtime(font: &FontSnapshot, name_map: &HashMap<String, &'static str>) -> Font {
    let glyph_map = font
        .glyphs
        .iter()
        .filter_map(|entry| {
            char::from_u32(entry.codepoint).map(|ch| (ch, glyph_runtime(&entry.glyph)))
        })
        .collect::<HashMap<_, _>>();
    Font {
        glyph_map,
        default_glyph: font.default_glyph.as_ref().map(glyph_runtime),
        line_spacing: font.line_spacing,
        height: font.height,
        fallback_font_name: font
            .fallback_font_name
            .as_ref()
            .and_then(|name| name_map.get(name).copied()),
        default_stroke_color: font.default_stroke_color,
        stroke_texture_map: font
            .stroke_texture_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        texture_hints_map: font
            .texture_hints_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    }
}

fn glyph_snapshot(glyph: &Glyph) -> GlyphSnapshot {
    GlyphSnapshot {
        texture_key: glyph.texture_key.clone(),
        tex_rect: glyph.tex_rect,
        uv_scale: glyph.uv_scale,
        uv_offset: glyph.uv_offset,
        size: glyph.size,
        offset: glyph.offset,
        advance: glyph.advance,
    }
}

fn glyph_runtime(glyph: &GlyphSnapshot) -> Glyph {
    Glyph {
        texture_key: glyph.texture_key.clone(),
        tex_rect: glyph.tex_rect,
        uv_scale: glyph.uv_scale,
        uv_offset: glyph.uv_offset,
        size: glyph.size,
        offset: glyph.offset,
        advance: glyph.advance,
        advance_i32: glyph.advance.round_ties_even() as i32,
    }
}

fn actor_snapshot(actor: &Actor) -> ActorSnapshot {
    match actor {
        Actor::Sprite {
            align,
            offset,
            size,
            source,
            tint,
            glow,
            z,
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            blend,
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            effect,
            ..
        } => ActorSnapshot::Sprite {
            align: *align,
            offset: *offset,
            size: size.map(SizeSpecSnapshot::from),
            source: SpriteSourceSnapshot::from(source),
            tint: *tint,
            glow: *glow,
            z: *z,
            cell: *cell,
            grid: *grid,
            uv_rect: *uv_rect,
            visible: *visible,
            flip_x: *flip_x,
            flip_y: *flip_y,
            cropleft: *cropleft,
            cropright: *cropright,
            croptop: *croptop,
            cropbottom: *cropbottom,
            fadeleft: *fadeleft,
            faderight: *faderight,
            fadetop: *fadetop,
            fadebottom: *fadebottom,
            blend: BlendModeSnapshot::from(*blend),
            mask_source: *mask_source,
            mask_dest: *mask_dest,
            rot_x_deg: *rot_x_deg,
            rot_y_deg: *rot_y_deg,
            rot_z_deg: *rot_z_deg,
            local_offset: *local_offset,
            local_offset_rot_sin_cos: *local_offset_rot_sin_cos,
            texcoordvelocity: *texcoordvelocity,
            animate: *animate,
            state_delay: *state_delay,
            scale: *scale,
            effect: EffectStateSnapshot::from(*effect),
        },
        Actor::Text {
            align,
            offset,
            color,
            stroke_color,
            glow,
            font,
            content,
            align_text,
            z,
            scale,
            fit_width,
            fit_height,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            clip,
            blend,
            effect,
        } => ActorSnapshot::Text {
            align: *align,
            offset: *offset,
            color: *color,
            stroke_color: *stroke_color,
            glow: *glow,
            font: (*font).to_string(),
            content: content.as_str().to_string(),
            align_text: TextAlignSnapshot::from(*align_text),
            z: *z,
            scale: *scale,
            fit_width: *fit_width,
            fit_height: *fit_height,
            wrap_width_pixels: *wrap_width_pixels,
            max_width: *max_width,
            max_height: *max_height,
            max_w_pre_zoom: *max_w_pre_zoom,
            max_h_pre_zoom: *max_h_pre_zoom,
            clip: *clip,
            blend: BlendModeSnapshot::from(*blend),
            effect: EffectStateSnapshot::from(*effect),
        },
        Actor::Mesh {
            align,
            offset,
            size,
            vertices,
            mode,
            visible,
            blend,
            z,
        } => ActorSnapshot::Mesh {
            align: *align,
            offset: *offset,
            size: size.map(SizeSpecSnapshot::from),
            vertices: vertices.to_vec(),
            mode: MeshModeSnapshot::from(*mode),
            visible: *visible,
            blend: BlendModeSnapshot::from(*blend),
            z: *z,
        },
        Actor::TexturedMesh {
            align,
            offset,
            size,
            texture,
            vertices,
            mode,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            visible,
            blend,
            z,
            ..
        } => ActorSnapshot::TexturedMesh {
            align: *align,
            offset: *offset,
            size: size.map(SizeSpecSnapshot::from),
            texture: texture.to_string(),
            vertices: vertices.to_vec(),
            mode: MeshModeSnapshot::from(*mode),
            uv_scale: *uv_scale,
            uv_offset: *uv_offset,
            uv_tex_shift: *uv_tex_shift,
            visible: *visible,
            blend: BlendModeSnapshot::from(*blend),
            z: *z,
        },
        Actor::Frame {
            align,
            offset,
            size,
            children,
            background,
            z,
        } => ActorSnapshot::Frame {
            align: *align,
            offset: *offset,
            size: size.map(SizeSpecSnapshot::from),
            children: children.iter().map(actor_snapshot).collect(),
            background: background.as_ref().map(BackgroundSnapshot::from),
            z: *z,
        },
        Actor::Camera {
            view_proj,
            children,
        } => ActorSnapshot::Camera {
            view_proj: matrix_snapshot(view_proj),
            children: children.iter().map(actor_snapshot).collect(),
        },
        Actor::Shadow { len, color, child } => ActorSnapshot::Shadow {
            len: *len,
            color: *color,
            child: Box::new(actor_snapshot(child)),
        },
    }
}

fn actor_runtime(actor: &ActorSnapshot, name_map: &HashMap<String, &'static str>) -> Actor {
    match actor {
        ActorSnapshot::Sprite {
            align,
            offset,
            size,
            source,
            tint,
            glow,
            z,
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            blend,
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            effect,
        } => Actor::Sprite {
            align: *align,
            offset: *offset,
            world_z: 0.0,
            size: size.map(SizeSpec::from),
            source: SpriteSource::from(source),
            tint: *tint,
            glow: *glow,
            z: *z,
            cell: *cell,
            grid: *grid,
            uv_rect: *uv_rect,
            visible: *visible,
            flip_x: *flip_x,
            flip_y: *flip_y,
            cropleft: *cropleft,
            cropright: *cropright,
            croptop: *croptop,
            cropbottom: *cropbottom,
            fadeleft: *fadeleft,
            faderight: *faderight,
            fadetop: *fadetop,
            fadebottom: *fadebottom,
            blend: BlendMode::from(*blend),
            mask_source: *mask_source,
            mask_dest: *mask_dest,
            rot_x_deg: *rot_x_deg,
            rot_y_deg: *rot_y_deg,
            rot_z_deg: *rot_z_deg,
            local_offset: *local_offset,
            local_offset_rot_sin_cos: *local_offset_rot_sin_cos,
            texcoordvelocity: *texcoordvelocity,
            animate: *animate,
            state_delay: *state_delay,
            scale: *scale,
            effect: EffectState::from(*effect),
        },
        ActorSnapshot::Text {
            align,
            offset,
            color,
            stroke_color,
            glow,
            font,
            content,
            align_text,
            z,
            scale,
            fit_width,
            fit_height,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            clip,
            blend,
            effect,
        } => Actor::Text {
            align: *align,
            offset: *offset,
            color: *color,
            stroke_color: *stroke_color,
            glow: *glow,
            font: *name_map
                .get(font)
                .unwrap_or_else(|| panic!("missing font mapping for '{font}'")),
            content: TextContent::Owned(content.clone()),
            align_text: TextAlign::from(*align_text),
            z: *z,
            scale: *scale,
            fit_width: *fit_width,
            fit_height: *fit_height,
            wrap_width_pixels: *wrap_width_pixels,
            max_width: *max_width,
            max_height: *max_height,
            max_w_pre_zoom: *max_w_pre_zoom,
            max_h_pre_zoom: *max_h_pre_zoom,
            clip: *clip,
            blend: BlendMode::from(*blend),
            effect: EffectState::from(*effect),
        },
        ActorSnapshot::Mesh {
            align,
            offset,
            size,
            vertices,
            mode,
            visible,
            blend,
            z,
        } => Actor::Mesh {
            align: *align,
            offset: *offset,
            size: size.map(SizeSpec::from),
            vertices: Arc::from(vertices.clone()),
            mode: MeshMode::from(*mode),
            visible: *visible,
            blend: BlendMode::from(*blend),
            z: *z,
        },
        ActorSnapshot::TexturedMesh {
            align,
            offset,
            size,
            texture,
            vertices,
            mode,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            visible,
            blend,
            z,
        } => Actor::TexturedMesh {
            align: *align,
            offset: *offset,
            world_z: 0.0,
            size: size.map(SizeSpec::from),
            texture: Arc::from(texture.as_str()),
            vertices: Arc::from(vertices.clone()),
            mode: MeshMode::from(*mode),
            uv_scale: *uv_scale,
            uv_offset: *uv_offset,
            uv_tex_shift: *uv_tex_shift,
            visible: *visible,
            blend: BlendMode::from(*blend),
            z: *z,
        },
        ActorSnapshot::Frame {
            align,
            offset,
            size,
            children,
            background,
            z,
        } => Actor::Frame {
            align: *align,
            offset: *offset,
            size: size.map(SizeSpec::from),
            children: children
                .iter()
                .map(|child| actor_runtime(child, name_map))
                .collect(),
            background: background.as_ref().map(Background::from),
            z: *z,
        },
        ActorSnapshot::Camera {
            view_proj,
            children,
        } => Actor::Camera {
            view_proj: matrix_runtime(*view_proj),
            children: children
                .iter()
                .map(|child| actor_runtime(child, name_map))
                .collect(),
        },
        ActorSnapshot::Shadow { len, color, child } => Actor::Shadow {
            len: *len,
            color: *color,
            child: Box::new(actor_runtime(child, name_map)),
        },
    }
}

pub fn render_list_snapshot(render: &RenderList<'_>) -> RenderListSnapshot {
    RenderListSnapshot {
        clear_color: render.clear_color,
        cameras: render.cameras.iter().map(matrix_snapshot).collect(),
        objects: render.objects.iter().map(render_object_snapshot).collect(),
    }
}

fn render_object_snapshot(render: &RenderObject<'_>) -> RenderObjectSnapshot {
    RenderObjectSnapshot {
        object_type: match &render.object_type {
            ObjectType::Sprite {
                texture_id,
                tint,
                uv_scale,
                uv_offset,
                local_offset,
                local_offset_rot_sin_cos,
                edge_fade,
            } => RenderObjectTypeSnapshot::Sprite {
                texture_id: texture_id.to_string(),
                tint: *tint,
                uv_scale: *uv_scale,
                uv_offset: *uv_offset,
                local_offset: *local_offset,
                local_offset_rot_sin_cos: *local_offset_rot_sin_cos,
                edge_fade: *edge_fade,
            },
            ObjectType::Mesh { vertices, mode } => RenderObjectTypeSnapshot::Mesh {
                vertices: vertices.to_vec(),
                mode: MeshModeSnapshot::from(*mode),
            },
            ObjectType::TexturedMesh {
                texture_id,
                vertices,
                mode,
                uv_scale,
                uv_offset,
                uv_tex_shift,
            } => RenderObjectTypeSnapshot::TexturedMesh {
                texture_id: texture_id.to_string(),
                vertices: vertices.to_vec(),
                mode: MeshModeSnapshot::from(*mode),
                uv_scale: *uv_scale,
                uv_offset: *uv_offset,
                uv_tex_shift: *uv_tex_shift,
            },
        },
        transform: matrix_snapshot(&render.transform),
        blend: BlendModeSnapshot::from(render.blend),
        z: render.z,
        order: render.order,
        camera: render.camera,
    }
}

fn texture_resolve_object_snapshot(render: &RenderObject<'_>) -> TextureResolveObjectSnapshot {
    TextureResolveObjectSnapshot {
        texture_id: match &render.object_type {
            ObjectType::Sprite { texture_id, .. } | ObjectType::TexturedMesh { texture_id, .. } => {
                Some(texture_id.as_ref().to_string())
            }
            ObjectType::Mesh { .. } => None,
        },
        texture_handle: render.texture_handle,
    }
}

fn render_object_runtime(render: &RenderObjectSnapshot) -> RenderObject<'static> {
    RenderObject {
        object_type: match &render.object_type {
            RenderObjectTypeSnapshot::Sprite {
                texture_id,
                tint,
                uv_scale,
                uv_offset,
                local_offset,
                local_offset_rot_sin_cos,
                edge_fade,
            } => ObjectType::Sprite {
                texture_id: Cow::Owned(texture_id.clone()),
                tint: *tint,
                uv_scale: *uv_scale,
                uv_offset: *uv_offset,
                local_offset: *local_offset,
                local_offset_rot_sin_cos: *local_offset_rot_sin_cos,
                edge_fade: *edge_fade,
            },
            RenderObjectTypeSnapshot::Mesh { vertices, mode } => ObjectType::Mesh {
                vertices: Cow::Owned(vertices.clone()),
                mode: MeshMode::from(*mode),
            },
            RenderObjectTypeSnapshot::TexturedMesh {
                texture_id,
                vertices,
                mode,
                uv_scale,
                uv_offset,
                uv_tex_shift,
            } => ObjectType::TexturedMesh {
                texture_id: Cow::Owned(texture_id.clone()),
                vertices: Cow::Owned(vertices.clone()),
                mode: MeshMode::from(*mode),
                uv_scale: *uv_scale,
                uv_offset: *uv_offset,
                uv_tex_shift: *uv_tex_shift,
            },
        },
        texture_handle: crate::core::gfx::INVALID_TEXTURE_HANDLE,
        transform: matrix_runtime(render.transform),
        blend: BlendMode::from(render.blend),
        z: render.z,
        order: render.order,
        camera: render.camera,
    }
}

fn matrix_snapshot(m: &Matrix4<f32>) -> [[f32; 4]; 4] {
    [
        [m.x.x, m.x.y, m.x.z, m.x.w],
        [m.y.x, m.y.y, m.y.z, m.y.w],
        [m.z.x, m.z.y, m.z.z, m.z.w],
        [m.w.x, m.w.y, m.w.z, m.w.w],
    ]
}

fn matrix_runtime(m: [[f32; 4]; 4]) -> Matrix4<f32> {
    Matrix4::new(
        m[0][0], m[0][1], m[0][2], m[0][3], m[1][0], m[1][1], m[1][2], m[1][3], m[2][0], m[2][1],
        m[2][2], m[2][3], m[3][0], m[3][1], m[3][2], m[3][3],
    )
}

fn leak_str(value: &str) -> &'static str {
    Box::leak(value.to_string().into_boxed_str())
}

impl From<Metrics> for MetricsSnapshot {
    fn from(value: Metrics) -> Self {
        Self {
            left: value.left,
            right: value.right,
            top: value.top,
            bottom: value.bottom,
        }
    }
}

impl From<MetricsSnapshot> for Metrics {
    fn from(value: MetricsSnapshot) -> Self {
        Self {
            left: value.left,
            right: value.right,
            top: value.top,
            bottom: value.bottom,
        }
    }
}

impl From<SizeSpec> for SizeSpecSnapshot {
    fn from(value: SizeSpec) -> Self {
        match value {
            SizeSpec::Px(v) => Self::Px(v),
            SizeSpec::Fill => Self::Fill,
        }
    }
}

impl From<SizeSpecSnapshot> for SizeSpec {
    fn from(value: SizeSpecSnapshot) -> Self {
        match value {
            SizeSpecSnapshot::Px(v) => Self::Px(v),
            SizeSpecSnapshot::Fill => Self::Fill,
        }
    }
}

impl From<&SpriteSource> for SpriteSourceSnapshot {
    fn from(value: &SpriteSource) -> Self {
        match value {
            SpriteSource::Texture(key) => Self::Texture(key.to_string()),
            SpriteSource::Solid => Self::Solid,
        }
    }
}

impl From<&SpriteSourceSnapshot> for SpriteSource {
    fn from(value: &SpriteSourceSnapshot) -> Self {
        match value {
            SpriteSourceSnapshot::Texture(key) => Self::Texture(Arc::from(key.as_str())),
            SpriteSourceSnapshot::Solid => Self::Solid,
        }
    }
}

impl From<&Background> for BackgroundSnapshot {
    fn from(value: &Background) -> Self {
        match value {
            Background::Color(c) => Self::Color(*c),
            Background::Texture(tex) => Self::Texture((*tex).to_string()),
        }
    }
}

impl From<&BackgroundSnapshot> for Background {
    fn from(value: &BackgroundSnapshot) -> Self {
        match value {
            BackgroundSnapshot::Color(c) => Self::Color(*c),
            BackgroundSnapshot::Texture(tex) => Self::Texture(leak_str(tex)),
        }
    }
}

impl From<TextAlign> for TextAlignSnapshot {
    fn from(value: TextAlign) -> Self {
        match value {
            TextAlign::Left => Self::Left,
            TextAlign::Center => Self::Center,
            TextAlign::Right => Self::Right,
        }
    }
}

impl From<TextAlignSnapshot> for TextAlign {
    fn from(value: TextAlignSnapshot) -> Self {
        match value {
            TextAlignSnapshot::Left => Self::Left,
            TextAlignSnapshot::Center => Self::Center,
            TextAlignSnapshot::Right => Self::Right,
        }
    }
}

impl From<BlendMode> for BlendModeSnapshot {
    fn from(value: BlendMode) -> Self {
        match value {
            BlendMode::Alpha => Self::Alpha,
            BlendMode::Add => Self::Add,
            BlendMode::Multiply => Self::Multiply,
            BlendMode::Subtract => Self::Subtract,
        }
    }
}

impl From<BlendModeSnapshot> for BlendMode {
    fn from(value: BlendModeSnapshot) -> Self {
        match value {
            BlendModeSnapshot::Alpha => Self::Alpha,
            BlendModeSnapshot::Add => Self::Add,
            BlendModeSnapshot::Multiply => Self::Multiply,
            BlendModeSnapshot::Subtract => Self::Subtract,
        }
    }
}

impl From<MeshMode> for MeshModeSnapshot {
    fn from(value: MeshMode) -> Self {
        match value {
            MeshMode::Triangles => Self::Triangles,
        }
    }
}

impl From<MeshModeSnapshot> for MeshMode {
    fn from(value: MeshModeSnapshot) -> Self {
        match value {
            MeshModeSnapshot::Triangles => Self::Triangles,
        }
    }
}

impl From<EffectClock> for EffectClockSnapshot {
    fn from(value: EffectClock) -> Self {
        match value {
            EffectClock::Time => Self::Time,
            EffectClock::Beat => Self::Beat,
        }
    }
}

impl From<EffectClockSnapshot> for EffectClock {
    fn from(value: EffectClockSnapshot) -> Self {
        match value {
            EffectClockSnapshot::Time => Self::Time,
            EffectClockSnapshot::Beat => Self::Beat,
        }
    }
}

impl From<EffectMode> for EffectModeSnapshot {
    fn from(value: EffectMode) -> Self {
        match value {
            EffectMode::None => Self::None,
            EffectMode::DiffuseRamp => Self::DiffuseRamp,
            EffectMode::DiffuseShift => Self::DiffuseShift,
            EffectMode::GlowShift => Self::GlowShift,
            EffectMode::Pulse => Self::Pulse,
            EffectMode::Spin => Self::Spin,
        }
    }
}

impl From<EffectModeSnapshot> for EffectMode {
    fn from(value: EffectModeSnapshot) -> Self {
        match value {
            EffectModeSnapshot::None => Self::None,
            EffectModeSnapshot::DiffuseRamp => Self::DiffuseRamp,
            EffectModeSnapshot::DiffuseShift => Self::DiffuseShift,
            EffectModeSnapshot::GlowShift => Self::GlowShift,
            EffectModeSnapshot::Pulse => Self::Pulse,
            EffectModeSnapshot::Spin => Self::Spin,
        }
    }
}

impl From<EffectState> for EffectStateSnapshot {
    fn from(value: EffectState) -> Self {
        Self {
            clock: EffectClockSnapshot::from(value.clock),
            mode: EffectModeSnapshot::from(value.mode),
            color1: value.color1,
            color2: value.color2,
            period: value.period,
            offset: value.offset,
            timing: value.timing,
            magnitude: value.magnitude,
        }
    }
}

impl From<EffectStateSnapshot> for EffectState {
    fn from(value: EffectStateSnapshot) -> Self {
        Self {
            clock: EffectClock::from(value.clock),
            mode: EffectMode::from(value.mode),
            color1: value.color1,
            color2: value.color2,
            period: value.period,
            offset: value.offset,
            timing: value.timing,
            magnitude: value.magnitude,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_roundtrip_keeps_output_hash() {
        let scenario = crate::test_support::compose_scenarios::build_scenario("hud")
            .expect("hud scenario should exist");
        let (case, output) = capture_case(
            scenario.name,
            &scenario.actors,
            scenario.clear_color,
            &scenario.metrics,
            &scenario.fonts,
            scenario.total_elapsed,
        )
        .expect("capture should succeed");
        let encoded = serde_json::to_vec(&case).expect("case should serialize");
        let decoded: ComposeCase = serde_json::from_slice(&encoded).expect("case should decode");
        let replay_output = render_case_output(&decoded).expect("replay should compose");
        let baseline_hash = render_snapshot_hash(&output).expect("hash should succeed");
        let replay_hash = render_snapshot_hash(&replay_output).expect("hash should succeed");

        assert_eq!(case.expected.output_hash, baseline_hash);
        assert_eq!(baseline_hash, replay_hash);
        assert_eq!(case.expected.objects, replay_output.objects.len());
        assert_eq!(case.expected.cameras, replay_output.cameras.len());
    }

    #[test]
    fn render_snapshot_roundtrip_keeps_output_hash() {
        let scenario = crate::test_support::compose_scenarios::build_scenario("mask")
            .expect("mask scenario should exist");
        let (_, output) = capture_case(
            scenario.name,
            &scenario.actors,
            scenario.clear_color,
            &scenario.metrics,
            &scenario.fonts,
            scenario.total_elapsed,
        )
        .expect("capture should succeed");
        let render = render_list_runtime(&output);
        let roundtrip = render_list_snapshot(&render);
        let output_hash = render_snapshot_hash(&output).expect("hash should succeed");
        let roundtrip_hash = render_snapshot_hash(&roundtrip).expect("hash should succeed");

        assert_eq!(output_hash, roundtrip_hash);
    }
}
