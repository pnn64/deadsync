use crate::itg as noteskin_itg;
use crate::lua::itg_quoted_strings;
use crate::{
    ModelAutoRotKey, ModelDrawState, ModelEffectState, ModelMesh, ModelTweenSegment, ModelVertex,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub struct ItgModelTexturePath {
    pub uv_velocity: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_cycle_seconds: Option<f32>,
}

impl Default for ItgModelTexturePath {
    fn default() -> Self {
        Self {
            uv_velocity: [0.0, 0.0],
            uv_offset: [0.0, 0.0],
            uv_cycle_seconds: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ItgResolvedModelTexture {
    pub texture_path: PathBuf,
    pub tex: ItgModelTexturePath,
}

impl ItgResolvedModelTexture {
    fn from_path(texture_path: PathBuf) -> Self {
        Self {
            texture_path,
            tex: ItgModelTexturePath::default(),
        }
    }
}

pub fn itg_resolve_model_texture_path(
    data: &noteskin_itg::NoteskinData,
    model_path: &Path,
) -> Option<ItgResolvedModelTexture> {
    if !model_path.is_file() {
        return None;
    }
    if let Some(ext) = model_path.extension().and_then(|s| s.to_str()) {
        match itg_model_texture_kind(ext) {
            ItgModelTextureKind::Image => {
                return Some(ItgResolvedModelTexture::from_path(model_path.to_path_buf()));
            }
            ItgModelTextureKind::Animated => {
                return itg_resolve_animated_texture_ini(model_path);
            }
            ItgModelTextureKind::Other => {}
        }
    }
    let content = fs::read_to_string(model_path).ok()?;
    for candidate in itg_quoted_strings(&content) {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(candidate_path) = itg_resolve_relative_or_noteskin_path(data, model_path, trimmed)
        else {
            continue;
        };
        let Some(ext) = candidate_path.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        match itg_model_texture_kind(ext) {
            ItgModelTextureKind::Image => {
                return Some(ItgResolvedModelTexture::from_path(candidate_path));
            }
            ItgModelTextureKind::Animated => {
                if let Some(resolved) = itg_resolve_animated_texture_ini(&candidate_path) {
                    return Some(resolved);
                }
            }
            ItgModelTextureKind::Other => {}
        }
    }
    let stem = model_path.file_stem().and_then(|s| s.to_str())?;
    let derived = itg_derived_model_texture_stem(stem);
    data.resolve_path("", &derived).and_then(|path| {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        match itg_model_texture_kind(ext) {
            ItgModelTextureKind::Image => Some(ItgResolvedModelTexture::from_path(path)),
            ItgModelTextureKind::Animated => itg_resolve_animated_texture_ini(&path),
            ItgModelTextureKind::Other => None,
        }
    })
}

#[inline]
fn itg_ascii_suffix_start(value: &str, suffix: &[u8]) -> Option<usize> {
    let start = value.len().checked_sub(suffix.len())?;
    value.as_bytes()[start..]
        .eq_ignore_ascii_case(suffix)
        .then_some(start)
}

fn itg_derived_model_texture_stem(stem: &str) -> String {
    let (base, suffix) = if let Some(start) = itg_ascii_suffix_start(stem, b" model") {
        (&stem[..start], " tex")
    } else if let Some(start) = itg_ascii_suffix_start(stem, b"model") {
        (&stem[..start], "tex")
    } else {
        (stem, " tex")
    };
    let mut derived = String::with_capacity(base.len() + suffix.len());
    derived.push_str(base);
    derived.push_str(suffix);
    derived
}

fn itg_resolve_relative_or_noteskin_path(
    data: &noteskin_itg::NoteskinData,
    base_file: &Path,
    raw: &str,
) -> Option<PathBuf> {
    let rel = itg_normalized_asset_ref(raw)?;
    let rel_path = Path::new(&rel);
    if rel_path.is_absolute() && rel_path.is_file() {
        return Some(rel_path.to_path_buf());
    }
    if let Some(parent) = base_file.parent()
        && let Some(path) = itg_resolve_relative_file(parent, rel_path)
    {
        return Some(path);
    }
    for dir in &data.search_dirs {
        if let Some(path) = itg_resolve_relative_file(dir, rel_path) {
            return Some(path);
        }
    }
    data.resolve_path("", &rel)
}

fn itg_normalized_asset_ref(raw: &str) -> Option<String> {
    let rel = raw.trim().trim_matches('"').trim_matches('\'');
    if rel.is_empty() {
        None
    } else {
        Some(rel.replace('\\', "/"))
    }
}

fn itg_resolve_relative_file(base: &Path, rel: &Path) -> Option<PathBuf> {
    let direct = base.join(rel);
    if direct.is_file() {
        return Some(direct);
    }

    let mut current = base.to_path_buf();
    for component in rel.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(part) => {
                let name = part.to_str()?;
                current = itg_find_child_case_insensitive(&current, name)?;
            }
            _ => return None,
        }
    }
    current.is_file().then_some(current)
}

fn itg_find_child_case_insensitive(parent: &Path, name: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(parent).ok()?.flatten() {
        if entry
            .file_name()
            .to_str()
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
        {
            return Some(entry.path());
        }
    }
    None
}

fn itg_resolve_animated_texture_ini(path: &Path) -> Option<ItgResolvedModelTexture> {
    let ini = noteskin_itg::IniData::parse_file(path).ok()?;
    let first_frame_idx = if ini.get("AnimatedTexture", "Frame0000").is_some() {
        0
    } else {
        1
    };
    let frame = ini.get(
        "AnimatedTexture",
        if first_frame_idx == 0 {
            "Frame0000"
        } else {
            "Frame0001"
        },
    )?;
    let rel = itg_normalized_asset_ref(frame)?;
    let rel_path = Path::new(&rel);
    let texture_path = if rel_path.is_absolute() && rel_path.is_file() {
        rel_path.to_path_buf()
    } else {
        let base = path.parent()?;
        itg_resolve_relative_file(base, rel_path)?
    };
    let tex_velocity_x = ini
        .get("AnimatedTexture", "TexVelocityX")
        .and_then(noteskin_itg::parse_ini_float)
        .unwrap_or(0.0);
    let tex_velocity_y = ini
        .get("AnimatedTexture", "TexVelocityY")
        .and_then(noteskin_itg::parse_ini_float)
        .unwrap_or(0.0);
    let tex_offset_x = ini
        .get("AnimatedTexture", "TexOffsetX")
        .and_then(noteskin_itg::parse_ini_float)
        .unwrap_or(0.0);
    let tex_offset_y = ini
        .get("AnimatedTexture", "TexOffsetY")
        .and_then(noteskin_itg::parse_ini_float)
        .unwrap_or(0.0);
    let mut cycle_seconds = 0.0f32;
    for idx in first_frame_idx..1000 {
        let frame_key = itg_animated_texture_key(*b"Frame0000", idx);
        let delay_key = itg_animated_texture_key(*b"Delay0000", idx);
        if ini
            .get("AnimatedTexture", itg_animated_texture_key_str(&frame_key))
            .is_none()
        {
            break;
        }
        let Some(delay) = ini
            .get("AnimatedTexture", itg_animated_texture_key_str(&delay_key))
            .and_then(noteskin_itg::parse_ini_float)
        else {
            break;
        };
        cycle_seconds += delay.max(0.0);
    }
    Some(ItgResolvedModelTexture {
        texture_path,
        tex: ItgModelTexturePath {
            uv_velocity: [tex_velocity_x, tex_velocity_y],
            uv_offset: [tex_offset_x, tex_offset_y],
            uv_cycle_seconds: (cycle_seconds > f32::EPSILON && cycle_seconds.is_finite())
                .then_some(cycle_seconds),
        },
    })
}

#[inline]
fn itg_animated_texture_key(mut key: [u8; 9], mut index: usize) -> [u8; 9] {
    debug_assert!(index < 10_000);
    for digit in key[5..].iter_mut().rev() {
        *digit = b'0' + (index % 10) as u8;
        index /= 10;
    }
    key
}

#[inline]
fn itg_animated_texture_key_str(key: &[u8; 9]) -> &str {
    std::str::from_utf8(key).expect("animated texture keys are always ASCII")
}

#[cfg(any(test, feature = "bench-support"))]
fn itg_animated_texture_key_reference(prefix: &str, index: usize) -> String {
    format!("{prefix}{index:04}")
}

#[derive(Debug, Clone)]
pub struct ItgResolvedModelLayer {
    pub mesh: Arc<ModelMesh>,
    pub texture: ItgResolvedModelTexture,
    pub flags: ItgModelMaterialFlags,
}

#[derive(Debug)]
struct ItgMilkshapeMeshLayer {
    material_index: i32,
    vertices: Vec<ModelVertex>,
    bounds: [f32; 6],
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ItgModelMaterialFlags {
    pub nomove: bool,
}

#[derive(Debug, Clone)]
pub struct ItgModelAutoRot {
    pub total_frames: f32,
    pub z_keys: Arc<[ModelAutoRotKey]>,
}

#[derive(Debug, Clone)]
pub struct ItgModelSlotPlan {
    pub model: Option<Arc<ModelMesh>>,
    pub model_draw: ModelDrawState,
    pub model_timeline: Arc<[ModelTweenSegment]>,
    pub model_effect: ModelEffectState,
    pub model_auto_rot_total_frames: f32,
    pub model_auto_rot_z_keys: Arc<[ModelAutoRotKey]>,
    pub note_color_translate: bool,
    pub uv_velocity: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_cycle_seconds: Option<f32>,
}

impl ItgModelSlotPlan {
    #[must_use]
    pub fn from_layer(
        layer: ItgResolvedModelLayer,
        model_draw: ModelDrawState,
        model_timeline: Arc<[ModelTweenSegment]>,
        model_effect: ModelEffectState,
        auto_rot: Option<&ItgModelAutoRot>,
    ) -> Self {
        let tex = layer.texture.tex;
        let (note_color_translate, uv_velocity) = if layer.flags.nomove {
            (false, [0.0, 0.0])
        } else {
            (true, tex.uv_velocity)
        };
        Self {
            model: Some(layer.mesh),
            model_draw,
            model_timeline,
            model_effect,
            model_auto_rot_total_frames: auto_rot.map_or(0.0, |auto_rot| auto_rot.total_frames),
            model_auto_rot_z_keys: auto_rot
                .map(|auto_rot| Arc::clone(&auto_rot.z_keys))
                .unwrap_or_else(|| Arc::from(Vec::<ModelAutoRotKey>::new())),
            note_color_translate,
            uv_velocity,
            uv_offset: tex.uv_offset,
            uv_cycle_seconds: tex.uv_cycle_seconds,
        }
    }

    #[must_use]
    pub fn from_texture(
        model: Option<Arc<ModelMesh>>,
        texture: ItgResolvedModelTexture,
        model_draw: ModelDrawState,
        model_timeline: Arc<[ModelTweenSegment]>,
        model_effect: ModelEffectState,
        auto_rot: Option<&ItgModelAutoRot>,
    ) -> Self {
        let tex = texture.tex;
        Self {
            model,
            model_draw,
            model_timeline,
            model_effect,
            model_auto_rot_total_frames: auto_rot.map_or(0.0, |auto_rot| auto_rot.total_frames),
            model_auto_rot_z_keys: auto_rot
                .map(|auto_rot| Arc::clone(&auto_rot.z_keys))
                .unwrap_or_else(|| Arc::from(Vec::<ModelAutoRotKey>::new())),
            note_color_translate: true,
            uv_velocity: tex.uv_velocity,
            uv_offset: tex.uv_offset,
            uv_cycle_seconds: tex.uv_cycle_seconds,
        }
    }
}

pub fn itg_load_model_slots_from_path<T>(
    model_path: &Path,
    mut slot_from_texture_path: impl FnMut(&Path) -> Option<T>,
    mut apply_slot_plan: impl FnMut(&mut T, ItgModelSlotPlan),
) -> Result<Vec<T>, String> {
    if !model_path.is_file() {
        return Err(format!("model '{}' was not found", model_path.display()));
    }

    let Some(search_dir) = model_path.parent() else {
        return Err(format!(
            "model '{}' has no parent directory",
            model_path.display()
        ));
    };
    let data = noteskin_itg::NoteskinData {
        name: "shared-model".to_string(),
        metrics: noteskin_itg::IniData::default(),
        search_dirs: vec![search_dir.to_path_buf()],
    };
    let model_auto_rot = itg_parse_milkshape_model_auto_rot(model_path);
    let mut slots = Vec::new();

    if let Some(model_layers) = itg_parse_milkshape_model_layers(&data, model_path) {
        for layer in model_layers {
            let Some(mut slot) = slot_from_texture_path(&layer.texture.texture_path) else {
                continue;
            };
            apply_slot_plan(
                &mut slot,
                ItgModelSlotPlan::from_layer(
                    layer,
                    ModelDrawState::default(),
                    Arc::from(Vec::<ModelTweenSegment>::new()),
                    ModelEffectState::default(),
                    model_auto_rot.as_ref(),
                ),
            );
            slots.push(slot);
        }
    }

    if slots.is_empty() {
        let Some(model_texture) = itg_resolve_model_texture_path(&data, model_path) else {
            return Err(format!(
                "model '{}' did not resolve a texture",
                model_path.display()
            ));
        };
        let Some(mut slot) = slot_from_texture_path(&model_texture.texture_path) else {
            return Err(format!(
                "model texture '{}' did not load",
                model_texture.texture_path.display()
            ));
        };
        let model = itg_parse_milkshape_model(&data, model_path);
        if model.is_none() {
            return Err(format!(
                "model '{}' did not produce any geometry",
                model_path.display()
            ));
        }
        apply_slot_plan(
            &mut slot,
            ItgModelSlotPlan::from_texture(
                model,
                model_texture,
                ModelDrawState::default(),
                Arc::from(Vec::<ModelTweenSegment>::new()),
                ModelEffectState::default(),
                model_auto_rot.as_ref(),
            ),
        );
        slots.push(slot);
    }

    Ok(slots)
}

fn itg_parse_model_material_flags(name: &str) -> ItgModelMaterialFlags {
    ItgModelMaterialFlags {
        nomove: name
            .as_bytes()
            .windows(b"nomove".len())
            .any(|candidate| candidate.eq_ignore_ascii_case(b"nomove")),
    }
}

fn itg_parse_milkshape_mesh_material_index(header: &str) -> i32 {
    let trimmed = header.trim();
    let rest = if let Some(end_quote) = trimmed.rfind('"') {
        &trimmed[end_quote + 1..]
    } else {
        trimmed
    };
    let mut parts = rest.split_whitespace();
    let _flags = parts.next();
    parts
        .next()
        .and_then(|raw| raw.parse::<i32>().ok())
        .unwrap_or(0)
}

#[inline]
fn itg_contains_milkshape_ascii_signature(mut content: &[u8], signature: &[u8]) -> bool {
    while content.len() >= signature.len() {
        let candidate_bytes = content.len() - signature.len() + 1;
        let Some(offset) = content[..candidate_bytes]
            .iter()
            .position(|byte| *byte == b'm' || *byte == b'M')
        else {
            return false;
        };
        content = &content[offset..];
        if content[..signature.len()].eq_ignore_ascii_case(signature) {
            return true;
        }
        content = &content[1..];
    }
    false
}

#[inline]
fn has_milkshape_ascii_signature(content: &str) -> bool {
    const SIGNATURE: &[u8] = b"milkshape 3d ascii";
    const FAST_PREFIX_BYTES: usize = 256;

    let bytes = content.as_bytes();
    let prefix_len = bytes.len().min(FAST_PREFIX_BYTES);
    if itg_contains_milkshape_ascii_signature(&bytes[..prefix_len], SIGNATURE) {
        return true;
    }
    if bytes.len() <= FAST_PREFIX_BYTES {
        return false;
    }

    // The overlap preserves matches crossing the fast-prefix boundary while
    // avoiding a lowercase copy of the complete model source.
    let suffix_start = FAST_PREFIX_BYTES.saturating_sub(SIGNATURE.len() - 1);
    itg_contains_milkshape_ascii_signature(&bytes[suffix_start..], SIGNATURE)
}

#[cfg(any(test, feature = "bench-support"))]
fn has_milkshape_ascii_signature_reference(content: &str) -> bool {
    const SIGNATURE: &[u8] = b"milkshape 3d ascii";
    const FAST_PREFIX_BYTES: usize = 256;

    let bytes = content.as_bytes();
    let prefix_len = bytes.len().min(FAST_PREFIX_BYTES);
    if bytes[..prefix_len]
        .windows(SIGNATURE.len())
        .any(|candidate| candidate.eq_ignore_ascii_case(SIGNATURE))
    {
        return true;
    }
    if bytes.len() <= FAST_PREFIX_BYTES {
        return false;
    }

    content.to_ascii_lowercase().contains("milkshape 3d ascii")
}

fn itg_finish_model_auto_rot_keys(mut keys: Vec<ModelAutoRotKey>) -> Arc<[ModelAutoRotKey]> {
    keys.sort_by(|a, b| {
        a.frame
            .partial_cmp(&b.frame)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for idx in 1..keys.len() {
        let prev_z_deg = keys[idx - 1].z_deg;
        let z_deg = &mut keys[idx].z_deg;
        while *z_deg - prev_z_deg > 180.0 {
            *z_deg -= 360.0;
        }
        while *z_deg - prev_z_deg < -180.0 {
            *z_deg += 360.0;
        }
    }
    Arc::from(keys)
}

#[cfg(any(test, feature = "bench-support"))]
fn itg_finish_model_auto_rot_keys_reference(
    mut first_bone: Vec<(f32, f32)>,
) -> Arc<[ModelAutoRotKey]> {
    first_bone.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut keys: Vec<ModelAutoRotKey> = Vec::with_capacity(first_bone.len());
    for (frame, mut z_deg) in first_bone {
        if let Some(prev) = keys.last().copied() {
            while z_deg - prev.z_deg > 180.0 {
                z_deg -= 360.0;
            }
            while z_deg - prev.z_deg < -180.0 {
                z_deg += 360.0;
            }
        }
        keys.push(ModelAutoRotKey { frame, z_deg });
    }
    Arc::from(keys)
}

pub fn itg_parse_milkshape_model_auto_rot(path: &Path) -> Option<ItgModelAutoRot> {
    let content = fs::read_to_string(path).ok()?;
    if !has_milkshape_ascii_signature(&content) {
        return None;
    }
    let mut lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"));
    while let Some(line) = lines.next() {
        let Some(raw_bones) = line.strip_prefix("Bones:") else {
            continue;
        };
        let bone_count = raw_bones.trim().parse::<usize>().ok()?;
        if bone_count == 0 {
            return None;
        }
        let mut total_frames = 0.0f32;
        let mut first_bone = Vec::new();
        for bone_idx in 0..bone_count {
            let _name = lines.next()?;
            let _parent = lines.next()?;
            let _bind = lines.next()?;
            let pos_count = lines.next()?.trim().parse::<usize>().ok()?;
            for _ in 0..pos_count {
                let frame = lines
                    .next()?
                    .split_whitespace()
                    .next()?
                    .parse::<f32>()
                    .ok()?;
                total_frames = total_frames.max(frame);
            }
            let rot_count = lines.next()?.trim().parse::<usize>().ok()?;
            if bone_idx == 0 {
                first_bone.reserve_exact(rot_count);
            }
            for _ in 0..rot_count {
                let rot_line = lines.next()?;
                let mut parts = rot_line.split_whitespace();
                let frame = parts.next()?.parse::<f32>().ok()?;
                let _x = parts.next()?.parse::<f32>().ok()?;
                let _y = parts.next()?.parse::<f32>().ok()?;
                let z = parts.next()?.parse::<f32>().ok()?;
                total_frames = total_frames.max(frame);
                if bone_idx == 0 {
                    first_bone.push(ModelAutoRotKey {
                        frame,
                        z_deg: z.to_degrees(),
                    });
                }
            }
        }
        if first_bone.is_empty() || total_frames <= f32::EPSILON {
            return None;
        }
        return Some(ItgModelAutoRot {
            total_frames,
            z_keys: itg_finish_model_auto_rot_keys(first_bone),
        });
    }
    None
}

fn itg_resolve_model_material_texture(
    data: &noteskin_itg::NoteskinData,
    model_path: &Path,
    raw_texture: &str,
) -> Option<ItgResolvedModelTexture> {
    let texture_ref = raw_texture.trim().trim_matches('"').trim_matches('\'');
    if texture_ref.is_empty() {
        return None;
    }
    let texture_path = itg_resolve_relative_or_noteskin_path(data, model_path, texture_ref)?;
    let ext = texture_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    match itg_model_texture_kind(ext) {
        ItgModelTextureKind::Image => Some(ItgResolvedModelTexture::from_path(texture_path)),
        ItgModelTextureKind::Animated => itg_resolve_animated_texture_ini(&texture_path),
        ItgModelTextureKind::Other if texture_path.is_file() => {
            itg_resolve_model_texture_path(data, &texture_path)
        }
        ItgModelTextureKind::Other => None,
    }
}

pub fn itg_parse_milkshape_model_layers(
    data: &noteskin_itg::NoteskinData,
    path: &Path,
) -> Option<Vec<ItgResolvedModelLayer>> {
    let content = fs::read_to_string(path).ok()?;
    if !has_milkshape_ascii_signature(&content) {
        return None;
    }

    let mut lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"));

    let mesh_count = loop {
        let line = lines.next()?;
        if let Some(raw_count) = line.strip_prefix("Meshes:") {
            break raw_count.trim().parse::<usize>().ok()?;
        }
    };

    let mut meshes = Vec::with_capacity(mesh_count);
    let mut model_bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];

    for _ in 0..mesh_count {
        let mesh_header = lines.next()?;
        let material_index = itg_parse_milkshape_mesh_material_index(mesh_header);
        let vertex_count = lines.next()?.trim().parse::<usize>().ok()?;
        let mut mesh_vertices = Vec::with_capacity(vertex_count);
        for _ in 0..vertex_count {
            let line = lines.next()?;
            let mut parts = line.split_whitespace();
            let flags = parts.next()?.parse::<u32>().ok()?;
            let x = parts.next()?.parse::<f32>().ok()?;
            let y = parts.next()?.parse::<f32>().ok()?;
            let z = parts.next()?.parse::<f32>().ok()?;
            let mut u = parts.next()?.parse::<f32>().ok()?;
            let mut v = parts.next()?.parse::<f32>().ok()?;
            if flags & 4 != 0 {
                if u.abs() > f32::EPSILON {
                    u = x / u;
                }
                if v.abs() > f32::EPSILON {
                    v = y / v;
                }
            }
            mesh_vertices.push(ModelVertex {
                pos: [x, y, z],
                uv: [u, v],
                tex_matrix_scale: [
                    if flags & 1 != 0 { 0.0 } else { 1.0 },
                    if flags & 2 != 0 { 0.0 } else { 1.0 },
                ],
            });
        }

        let normal_count = lines.next()?.trim().parse::<usize>().ok()?;
        for _ in 0..normal_count {
            let _ = lines.next()?;
        }

        let triangle_count = lines.next()?.trim().parse::<usize>().ok()?;
        let mut tri_vertices: Vec<ModelVertex> = Vec::with_capacity(triangle_count * 3);
        let mut bounds = [
            f32::INFINITY,
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for _ in 0..triangle_count {
            let line = lines.next()?;
            let mut parts = line.split_whitespace();
            let _flags = parts.next()?;
            let i0 = parts.next()?.parse::<usize>().ok()?;
            let i1 = parts.next()?.parse::<usize>().ok()?;
            let i2 = parts.next()?.parse::<usize>().ok()?;

            let Some(v0) = mesh_vertices.get(i0).copied() else {
                continue;
            };
            let Some(v1) = mesh_vertices.get(i1).copied() else {
                continue;
            };
            let Some(v2) = mesh_vertices.get(i2).copied() else {
                continue;
            };
            for vtx in [v0, v1, v2] {
                bounds[0] = bounds[0].min(vtx.pos[0]);
                bounds[1] = bounds[1].min(vtx.pos[1]);
                bounds[2] = bounds[2].min(vtx.pos[2]);
                bounds[3] = bounds[3].max(vtx.pos[0]);
                bounds[4] = bounds[4].max(vtx.pos[1]);
                bounds[5] = bounds[5].max(vtx.pos[2]);
                tri_vertices.push(vtx);
            }
        }

        if !tri_vertices.is_empty() {
            model_bounds[0] = model_bounds[0].min(bounds[0]);
            model_bounds[1] = model_bounds[1].min(bounds[1]);
            model_bounds[2] = model_bounds[2].min(bounds[2]);
            model_bounds[3] = model_bounds[3].max(bounds[3]);
            model_bounds[4] = model_bounds[4].max(bounds[4]);
            model_bounds[5] = model_bounds[5].max(bounds[5]);
            meshes.push(ItgMilkshapeMeshLayer {
                material_index,
                vertices: tri_vertices,
                bounds,
            });
        }
    }

    if meshes.is_empty() {
        return None;
    }

    let material_count = loop {
        let line = lines.next()?;
        if let Some(raw_count) = line.strip_prefix("Materials:") {
            break raw_count.trim().parse::<usize>().ok()?;
        }
    };
    let mut material_textures = Vec::with_capacity(material_count);
    for _ in 0..material_count {
        let name = lines.next()?.trim();
        let _ambient = lines.next()?;
        let _diffuse = lines.next()?;
        let _specular = lines.next()?;
        let _emissive = lines.next()?;
        let _shininess = lines.next()?;
        let _transparency = lines.next()?;
        let texture_line = lines.next()?.trim().to_string();
        let _alpha_map = lines.next()?;
        material_textures.push((texture_line, itg_parse_model_material_flags(name)));
    }

    let fallback_texture = itg_resolve_model_texture_path(data, path);
    let shared_bounds = if model_bounds[0].is_finite()
        && model_bounds[1].is_finite()
        && model_bounds[2].is_finite()
        && model_bounds[3].is_finite()
        && model_bounds[4].is_finite()
        && model_bounds[5].is_finite()
    {
        model_bounds
    } else {
        [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]
    };
    let mut layers = Vec::with_capacity(meshes.len());
    for mesh in meshes {
        let texture_with_flags = if mesh.material_index >= 0 {
            material_textures
                .get(mesh.material_index as usize)
                .and_then(|(raw, flags)| {
                    itg_resolve_model_material_texture(data, path, raw)
                        .map(|resolved| (resolved, *flags))
                })
        } else {
            None
        }
        .or_else(|| {
            fallback_texture
                .clone()
                .map(|resolved| (resolved, ItgModelMaterialFlags::default()))
        });
        let Some((texture, flags)) = texture_with_flags else {
            continue;
        };
        let bounds = if shared_bounds[3] > shared_bounds[0] && shared_bounds[4] > shared_bounds[1] {
            shared_bounds
        } else {
            mesh.bounds
        };
        layers.push(ItgResolvedModelLayer {
            mesh: Arc::new(ModelMesh {
                vertices: mesh.vertices.into(),
                bounds,
            }),
            texture,
            flags,
        });
    }

    if layers.is_empty() {
        None
    } else {
        Some(layers)
    }
}

#[must_use]
pub fn itg_parse_milkshape_model(
    data: &noteskin_itg::NoteskinData,
    path: &Path,
) -> Option<Arc<ModelMesh>> {
    itg_parse_milkshape_model_layers(data, path)
        .and_then(|layers| layers.into_iter().next().map(|layer| layer.mesh))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum ItgModelTextureKind {
    Image,
    Animated,
    Other,
}

#[inline]
fn itg_model_texture_kind(ext: &str) -> ItgModelTextureKind {
    match ext.len() {
        3 if ext.eq_ignore_ascii_case("png")
            || ext.eq_ignore_ascii_case("jpg")
            || ext.eq_ignore_ascii_case("bmp")
            || ext.eq_ignore_ascii_case("gif") =>
        {
            ItgModelTextureKind::Image
        }
        4 if ext.eq_ignore_ascii_case("jpeg") || ext.eq_ignore_ascii_case("webp") => {
            ItgModelTextureKind::Image
        }
        3 if ext.eq_ignore_ascii_case("ini") => ItgModelTextureKind::Animated,
        _ => ItgModelTextureKind::Other,
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn itg_model_texture_kind_reference(ext: &str) -> ItgModelTextureKind {
    let ext = ext.to_ascii_lowercase();
    if matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp"
    ) {
        ItgModelTextureKind::Image
    } else if ext == "ini" {
        ItgModelTextureKind::Animated
    } else {
        ItgModelTextureKind::Other
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn itg_derived_model_texture_stem_reference(stem: &str) -> String {
    let stem_lower = stem.to_ascii_lowercase();
    if stem_lower.ends_with(" model") {
        format!("{} tex", &stem[..stem.len().saturating_sub(6)])
    } else if stem_lower.ends_with("model") {
        format!("{}tex", &stem[..stem.len().saturating_sub(5)])
    } else {
        format!("{stem} tex")
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn itg_parse_model_material_flags_reference(name: &str) -> ItgModelMaterialFlags {
    let lower = name.to_ascii_lowercase();
    ItgModelMaterialFlags {
        nomove: lower.contains("nomove"),
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub mod model_scan_bench_support {
    use super::{
        ModelAutoRotKey, has_milkshape_ascii_signature, has_milkshape_ascii_signature_reference,
        itg_animated_texture_key, itg_animated_texture_key_reference,
        itg_derived_model_texture_stem, itg_derived_model_texture_stem_reference,
        itg_finish_model_auto_rot_keys, itg_finish_model_auto_rot_keys_reference,
        itg_model_texture_kind, itg_model_texture_kind_reference, itg_parse_model_material_flags,
        itg_parse_model_material_flags_reference,
    };

    fn mix(checksum: u64, value: u64) -> u64 {
        checksum.wrapping_mul(1_099_511_628_211).wrapping_add(value)
    }

    fn byte_checksum(mut checksum: u64, value: &[u8]) -> u64 {
        checksum = mix(checksum, value.len() as u64);
        value
            .iter()
            .fold(checksum, |sum, byte| mix(sum, u64::from(*byte)))
    }

    fn auto_rot_checksum(keys: &[ModelAutoRotKey]) -> u64 {
        keys.iter().fold(keys.len() as u64, |checksum, key| {
            mix(
                mix(checksum, u64::from(key.frame.to_bits())),
                u64::from(key.z_deg.to_bits()),
            )
        })
    }

    #[must_use]
    pub fn extension_kind_current(ext: &str) -> u8 {
        itg_model_texture_kind(ext) as u8
    }

    #[must_use]
    pub fn extension_kind_reference(ext: &str) -> u8 {
        itg_model_texture_kind_reference(ext) as u8
    }

    #[must_use]
    pub fn derived_texture_stem_current(stem: &str) -> String {
        itg_derived_model_texture_stem(stem)
    }

    #[must_use]
    pub fn derived_texture_stem_reference(stem: &str) -> String {
        itg_derived_model_texture_stem_reference(stem)
    }

    #[must_use]
    pub fn material_nomove_current(line: &str) -> bool {
        itg_parse_model_material_flags(line.trim()).nomove
    }

    #[must_use]
    pub fn material_nomove_reference(line: &str) -> bool {
        let name = line.trim().to_string();
        itg_parse_model_material_flags_reference(&name).nomove
    }

    #[must_use]
    pub fn milkshape_signature_current(content: &str) -> bool {
        has_milkshape_ascii_signature(content)
    }

    #[must_use]
    pub fn milkshape_signature_reference(content: &str) -> bool {
        has_milkshape_ascii_signature_reference(content)
    }

    #[must_use]
    pub fn animated_texture_keys_current(index: usize) -> u64 {
        let frame = itg_animated_texture_key(*b"Frame0000", index);
        let delay = itg_animated_texture_key(*b"Delay0000", index);
        byte_checksum(byte_checksum(0, &frame), &delay)
    }

    #[must_use]
    pub fn animated_texture_keys_reference(index: usize) -> u64 {
        let frame = itg_animated_texture_key_reference("Frame", index);
        let delay = itg_animated_texture_key_reference("Delay", index);
        byte_checksum(byte_checksum(0, frame.as_bytes()), delay.as_bytes())
    }

    #[must_use]
    pub fn auto_rot_keys_current(rotations: &[(f32, f32)]) -> u64 {
        let mut keys = Vec::with_capacity(rotations.len());
        for &(frame, z_deg) in rotations {
            keys.push(ModelAutoRotKey { frame, z_deg });
        }
        auto_rot_checksum(&itg_finish_model_auto_rot_keys(keys))
    }

    #[must_use]
    pub fn auto_rot_keys_reference(rotations: &[(f32, f32)]) -> u64 {
        let mut first_bone = Vec::new();
        for &rotation in rotations {
            first_bone.push(rotation);
        }
        auto_rot_checksum(&itg_finish_model_auto_rot_keys_reference(first_bone))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_model_root(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "deadsync-noteskin-model-{name}-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_mesh() -> Arc<ModelMesh> {
        Arc::new(ModelMesh {
            vertices: Arc::from([ModelVertex {
                pos: [0.0, 0.0, 0.0],
                uv: [0.0, 0.0],
                tex_matrix_scale: [1.0, 1.0],
            }]),
            bounds: [0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        })
    }

    #[test]
    fn model_extension_classifier_matches_owned_lowercase_behavior() {
        for ext in [
            "png", "PNG", "JpG", "jpeg", "BMP", "Gif", "WEBP", "ini", "INI", "txt", "", "café",
        ] {
            assert_eq!(
                itg_model_texture_kind(ext),
                itg_model_texture_kind_reference(ext),
                "extension {ext:?}"
            );
        }
    }

    #[test]
    fn derived_model_texture_stem_matches_owned_lowercase_behavior() {
        for stem in [
            "Down Tap Note Model",
            "Center Hold MODEL",
            "Up Lift model",
            "FallbackModel",
            "model",
            "Café Model",
            "Arrow",
            "",
        ] {
            assert_eq!(
                itg_derived_model_texture_stem(stem),
                itg_derived_model_texture_stem_reference(stem),
                "stem {stem:?}"
            );
        }
    }

    #[test]
    fn model_material_flags_match_copied_lowercase_behavior() {
        for line in [
            "material",
            "NoMove",
            "tap NOMOVE glow",
            "xnomovey",
            "no move",
            "nømove",
            "",
            "  MixedNoMove  ",
        ] {
            let name = line.trim().to_string();
            assert_eq!(
                itg_parse_model_material_flags(line.trim()).nomove,
                itg_parse_model_material_flags_reference(&name).nomove,
                "material line {line:?}"
            );
        }
    }

    #[test]
    fn milkshape_signature_scan_matches_lowercase_fallback_behavior() {
        let cases = [
            String::new(),
            "MilkShape 3D ASCII\nMeshes: 0".to_string(),
            format!("{}mIlKsHaPe 3D aScIi\nMeshes: 0", "x".repeat(400)),
            format!("{}MILKSHAPE 3D ASCII", "x".repeat(247)),
            format!("{}not a model", "cafÃ©".repeat(160)),
        ];

        for content in cases {
            assert_eq!(
                has_milkshape_ascii_signature(&content),
                has_milkshape_ascii_signature_reference(&content),
                "content length {}",
                content.len()
            );
        }
    }

    #[test]
    fn animated_texture_stack_keys_match_formatted_keys() {
        for index in [0, 1, 9, 10, 99, 100, 999] {
            for (template, prefix) in [(*b"Frame0000", "Frame"), (*b"Delay0000", "Delay")] {
                let key = itg_animated_texture_key(template, index);
                assert_eq!(
                    itg_animated_texture_key_str(&key),
                    itg_animated_texture_key_reference(prefix, index)
                );
            }
        }
    }

    #[test]
    fn in_place_auto_rotation_keys_match_two_buffer_behavior() {
        let rotations = vec![
            (30.0, -725.0),
            (0.0, 350.0),
            (20.0, 725.0),
            (10.0, 5.0),
            (40.0, 185.0),
        ];
        let keys = rotations
            .iter()
            .map(|&(frame, z_deg)| ModelAutoRotKey { frame, z_deg })
            .collect();
        let current = itg_finish_model_auto_rot_keys(keys);
        let reference = itg_finish_model_auto_rot_keys_reference(rotations);

        assert_eq!(current.len(), reference.len());
        for (current, reference) in current.iter().zip(reference.iter()) {
            assert_eq!(current.frame.to_bits(), reference.frame.to_bits());
            assert_eq!(current.z_deg.to_bits(), reference.z_deg.to_bits());
        }
    }

    #[test]
    fn model_slot_plan_from_layer_honors_nomove_flags() {
        let layer = ItgResolvedModelLayer {
            mesh: test_mesh(),
            texture: ItgResolvedModelTexture {
                texture_path: PathBuf::from("tap.png"),
                tex: ItgModelTexturePath {
                    uv_velocity: [2.0, -1.0],
                    uv_offset: [0.25, 0.5],
                    uv_cycle_seconds: Some(0.75),
                },
            },
            flags: ItgModelMaterialFlags { nomove: true },
        };

        let plan = ItgModelSlotPlan::from_layer(
            layer,
            ModelDrawState::default(),
            Arc::from(Vec::<ModelTweenSegment>::new()),
            ModelEffectState::default(),
            None,
        );

        assert!(plan.model.is_some());
        assert!(!plan.note_color_translate);
        assert_eq!(plan.uv_velocity, [0.0, 0.0]);
        assert_eq!(plan.uv_offset, [0.25, 0.5]);
        assert_eq!(plan.uv_cycle_seconds, Some(0.75));
    }

    #[test]
    fn model_slot_plan_carries_auto_rot_and_texture_motion() {
        let auto_rot = ItgModelAutoRot {
            total_frames: 120.0,
            z_keys: Arc::from([ModelAutoRotKey {
                frame: 10.0,
                z_deg: 45.0,
            }]),
        };
        let texture = ItgResolvedModelTexture {
            texture_path: PathBuf::from("tap.png"),
            tex: ItgModelTexturePath {
                uv_velocity: [1.0, 2.0],
                uv_offset: [0.1, 0.2],
                uv_cycle_seconds: Some(3.0),
            },
        };

        let plan = ItgModelSlotPlan::from_texture(
            Some(test_mesh()),
            texture,
            ModelDrawState::default(),
            Arc::from(Vec::<ModelTweenSegment>::new()),
            ModelEffectState::default(),
            Some(&auto_rot),
        );

        assert!(plan.note_color_translate);
        assert_eq!(plan.uv_velocity, [1.0, 2.0]);
        assert_eq!(plan.uv_offset, [0.1, 0.2]);
        assert_eq!(plan.model_auto_rot_total_frames, 120.0);
        assert_eq!(plan.model_auto_rot_z_keys.len(), 1);
        assert_eq!(plan.model_auto_rot_z_keys[0].z_deg, 45.0);
    }

    #[test]
    fn load_model_slots_builds_layer_plans() {
        let root = temp_model_root("slot-loader");
        let texture_path = root.join("Tap Note.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
            .save(&texture_path)
            .unwrap();
        let model_path = root.join("_down tap note model.txt");
        fs::write(
            &model_path,
            r#"MilkShape 3D ASCII
Meshes: 1
"mesh" 0 0
3
0 -1.0 -1.0 0.0 0.0 0.0 -1
0 1.0 -1.0 0.0 1.0 0.0 -1
0 0.0 1.0 0.0 0.0 1.0 -1
0
1
0 0 1 2 0 0 0 1
Materials: 1
"mat"
0.0 0.0 0.0 1.0
1.0 1.0 1.0 1.0
0.0 0.0 0.0 1.0
0.0 0.0 0.0 1.0
0.0
1.0
"Tap Note.png"
""
"#,
        )
        .unwrap();

        let slots = itg_load_model_slots_from_path(
            &model_path,
            |path| {
                assert_eq!(path, texture_path.as_path());
                Some("tap".to_string())
            },
            |slot, plan| {
                assert!(plan.model.is_some());
                if plan.note_color_translate {
                    slot.push_str(":model");
                }
            },
        )
        .expect("model slot loader should build one layer-backed slot");

        assert_eq!(slots, ["tap:model"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn model_material_paths_accept_windows_separators() {
        let root = temp_model_root("windows-separators");
        let texture_dir = root.join("textures");
        fs::create_dir_all(&texture_dir).unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
            .save(texture_dir.join("Tap Note parts.png"))
            .unwrap();
        fs::write(
            texture_dir.join("Tap Note parts.ini"),
            "[AnimatedTexture]\nTexVelocityY=-1\nFrame0000=Tap Note parts.png\nDelay0000=1.0\n",
        )
        .unwrap();

        let model_path = root.join("_down tap note model.txt");
        fs::write(
            &model_path,
            r#"MilkShape 3D ASCII
Meshes: 1
"mesh" 0 0
3
0 -1.0 -1.0 0.0 0.0 0.0 -1
0 1.0 -1.0 0.0 1.0 0.0 -1
0 0.0 1.0 0.0 0.0 1.0 -1
0
1
0 0 1 2 0 0 0 1
Materials: 1
"mat"
0.0 0.0 0.0 1.0
1.0 1.0 1.0 1.0
0.0 0.0 0.0 1.0
0.0 0.0 0.0 1.0
0.0
1.0
"textures\Tap Note parts.ini"
""
"#,
        )
        .unwrap();
        let data = noteskin_itg::NoteskinData {
            name: "test".to_string(),
            metrics: noteskin_itg::IniData::default(),
            search_dirs: vec![root.clone()],
        };

        let layers = itg_parse_milkshape_model_layers(&data, &model_path)
            .expect("model should resolve backslash material texture path");
        let layer = layers.first().expect("expected one model-backed layer");

        assert!(
            layer
                .texture
                .texture_path
                .to_string_lossy()
                .replace('\\', "/")
                .ends_with("textures/Tap Note parts.png")
        );
        assert_eq!(layer.texture.tex.uv_velocity, [0.0, -1.0]);

        let _ = fs::remove_dir_all(root);
    }
}
