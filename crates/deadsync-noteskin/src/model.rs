use crate::itg as noteskin_itg;
use crate::lua::itg_quoted_strings;
use crate::{
    ModelAutoRotKey, ModelDrawState, ModelEffectState, ModelMesh, ModelTweenSegment, ModelVertex,
};
use std::cell::OnceCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Virtual texture identifier for meshes with no material. The asset bridge
/// supplies its built-in white texel instead of opening an image file.
pub const MODEL_WHITE_TEXTURE: &str = "__white";

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
pub struct ItgTextureFrame {
    pub path: PathBuf,
    pub delay: f32,
}

#[derive(Debug, Clone)]
pub struct ItgTextureAnimation {
    pub path: PathBuf,
    pub frames: Vec<ItgTextureFrame>,
}

#[derive(Debug, Clone)]
pub struct ItgResolvedModelTexture {
    pub sphere_mapped: bool,
    pub texture_path: PathBuf,
    pub animation: Option<ItgTextureAnimation>,
    pub tex: ItgModelTexturePath,
}

impl ItgResolvedModelTexture {
    fn from_path(texture_path: PathBuf) -> Self {
        Self {
            sphere_mapped: texture_path.to_string_lossy().contains("sphere"),
            texture_path,
            animation: None,
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
                return itg_resolve_animated_texture_ini(data, model_path);
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
                if let Some(resolved) = itg_resolve_animated_texture_ini(data, &candidate_path) {
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
            ItgModelTextureKind::Animated => itg_resolve_animated_texture_ini(data, &path),
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
        return Some(data.override_path(rel_path.to_path_buf()));
    }
    if let Some(parent) = base_file.parent()
        && let Some(path) = itg_resolve_relative_file(parent, rel_path)
    {
        return Some(data.override_path(path));
    }
    for (index, dir) in data.search_dirs.iter().enumerate() {
        // A failed directory probe need not be repeated within this lookup.
        // Keep the first occurrence's priority and the noteskin fallback.
        if Some(dir.as_path()) == base_file.parent() || data.search_dirs[..index].contains(dir) {
            continue;
        }
        if let Some(path) = itg_resolve_relative_file(dir, rel_path) {
            return Some(data.override_path(path));
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

fn itg_resolve_animated_texture_ini(
    data: &noteskin_itg::NoteskinData,
    path: &Path,
) -> Option<ItgResolvedModelTexture> {
    let ini = noteskin_itg::IniData::parse_file(path).ok()?;
    let (first_frame_idx, frame) = if let Some(frame) = ini.get("AnimatedTexture", "Frame0000") {
        (0, frame)
    } else {
        (1, ini.get("AnimatedTexture", "Frame0001")?)
    };
    let first_frame = frame;
    let texture_path = itg_resolve_relative_or_noteskin_path(data, path, frame)?;
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
    let mut frames = Vec::new();
    let mut has_distinct_image = false;
    // Keep common prefixes on the stack. Longer single-image sequences still
    // need no heap scratch; a later path change can revisit the parsed INI.
    let mut prefix_delays = [0.0f32; 32];
    for idx in first_frame_idx..1000 {
        let frame_key = itg_animated_texture_key(*b"Frame0000", idx);
        let delay_key = itg_animated_texture_key(*b"Delay0000", idx);
        let Some(frame) = ini.get("AnimatedTexture", itg_animated_texture_key_str(&frame_key))
        else {
            break;
        };
        let Some(delay) = ini
            .get("AnimatedTexture", itg_animated_texture_key_str(&delay_key))
            .and_then(noteskin_itg::parse_ini_float)
        else {
            break;
        };
        if !delay.is_finite() || delay < 0.0 {
            return None;
        }
        if frames.is_empty()
            && let Some(cached) = prefix_delays.get_mut(idx - first_frame_idx)
        {
            *cached = delay;
        }
        // Borrow the first image while frames retain its exact path spelling.
        // Exact repeated references reuse this load's successful resolution.
        let frame_path = if idx == first_frame_idx || frame == first_frame {
            std::borrow::Cow::Borrowed(texture_path.as_path())
        } else {
            std::borrow::Cow::Owned(itg_resolve_relative_or_noteskin_path(data, path, frame)?)
        };
        if frames.is_empty() && frame_path.as_os_str() != texture_path.as_os_str() {
            // Earlier paths all had the first path's exact spelling. Their
            // validated delays are inline, with an INI fallback past 32.
            frames.reserve(idx - first_frame_idx + 1);
            for previous in first_frame_idx..idx {
                let delay = if let Some(cached) = prefix_delays.get(previous - first_frame_idx) {
                    *cached
                } else {
                    let key = itg_animated_texture_key(*b"Delay0000", previous);
                    ini.get("AnimatedTexture", itg_animated_texture_key_str(&key))
                        .and_then(noteskin_itg::parse_ini_float)
                        .expect("preceding frame delays were validated")
                };
                frames.push(ItgTextureFrame {
                    path: texture_path.clone(),
                    delay,
                });
            }
        }
        // Path equality also accepts aliases such as ./frames/a.png. Preserve
        // those spellings in stored frames without creating a duplicate atlas.
        has_distinct_image = has_distinct_image || frame_path.as_ref() != texture_path;
        if !frames.is_empty() {
            frames.push(ItgTextureFrame {
                path: frame_path.into_owned(),
                delay,
            });
        }
        cycle_seconds += delay;
    }
    Some(ItgResolvedModelTexture {
        sphere_mapped: path.to_string_lossy().contains("sphere"),
        // Repeated references to one image need no atlas. Keep its full UV
        // domain for scrolling materials instead of adding duplicate tiles.
        animation: (has_distinct_image
            && cycle_seconds > f32::EPSILON
            && cycle_seconds.is_finite())
        .then(|| ItgTextureAnimation {
            path: path.to_path_buf(),
            frames,
        }),
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

#[derive(Debug, Clone)]
pub struct ItgResolvedModelLayer {
    pub mesh: Arc<ModelMesh>,
    pub texture: ItgResolvedModelTexture,
    pub additive: Option<ItgResolvedModelTexture>,
    pub animation_length: f32,
    pub flags: ItgModelMaterialFlags,
    /// Shared vertex binding; unbound and mixed-bone meshes have no rigid bone.
    pub bone_index: Option<u8>,
}

#[derive(Debug)]
struct ItgSharedMilkshapeMeshLayer {
    material_index: i32,
    bone_index: Option<u8>,
    vertices: Arc<[ModelVertex]>,
    bounds: [f32; 6],
}

// Existing frozen loaders retain their original intermediate Vec storage.
#[cfg(test)]
struct ItgMilkshapeMeshLayer {
    material_index: i32,
    bone_index: Option<u8>,
    vertices: Vec<ModelVertex>,
    bounds: [f32; 6],
}

// Resolution is scoped to one model load. Materials reused by several meshes
// share their file/INI lookup, including misses, while layers keep owned data.
struct ItgMilkshapeMaterial<'a> {
    texture: &'a str,
    additive: &'a str,
    flags: ItgModelMaterialFlags,
    resolved_texture: OnceCell<Option<ItgResolvedModelTexture>>,
    resolved_additive: OnceCell<Option<ItgResolvedModelTexture>>,
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
    pub sphere_mapped: bool,
    pub additive: Option<ItgResolvedModelTexture>,
    pub animation_length: f32,
    pub texture_animation: Option<ItgTextureAnimation>,
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
        // The auto-rotation track represents bone zero, not the whole actor.
        // Match ITG's mesh binding: unbound geometry must stay stationary.
        let auto_rot = auto_rot.filter(|_| layer.bone_index == Some(0));
        let tex = layer.texture.tex;
        let (note_color_translate, uv_velocity) = if layer.flags.nomove {
            (false, [0.0, 0.0])
        } else {
            (true, tex.uv_velocity)
        };
        Self {
            sphere_mapped: layer.texture.sphere_mapped,
            additive: layer.additive,
            animation_length: layer.animation_length,
            texture_animation: layer.texture.animation,
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
            sphere_mapped: texture.sphere_mapped,
            additive: None,
            animation_length: tex.uv_cycle_seconds.unwrap_or(1.0),
            texture_animation: texture.animation,
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

pub fn itg_load_model_slots<T>(
    meshes_path: &Path,
    materials_path: &Path,
    bones_path: &Path,
    mut slot_from_texture_path: impl FnMut(&Path) -> Option<T>,
    mut apply_slot_plan: impl FnMut(&mut T, ItgModelSlotPlan),
) -> Result<Vec<T>, String> {
    for path in [meshes_path, materials_path, bones_path] {
        if !path.is_file() {
            return Err(format!("model '{}' was not found", path.display()));
        }
    }

    let Some(search_dir) = materials_path.parent() else {
        return Err(format!(
            "model '{}' has no parent directory",
            materials_path.display()
        ));
    };
    let data = noteskin_itg::NoteskinData {
        overrides: Vec::new(),
        name: "shared-model".to_string(),
        metrics: noteskin_itg::IniData::default(),
        search_dirs: vec![search_dir.to_path_buf()],
    };
    let model_auto_rot = itg_parse_milkshape_model_auto_rot(bones_path);
    let mut slots = Vec::new();

    if let Some(model_layers) = itg_parse_milkshape_model_layers(&data, meshes_path, materials_path)
    {
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
        let Some(model_texture) = itg_resolve_model_texture_path(&data, materials_path) else {
            return Err(format!(
                "model '{}' did not resolve a texture",
                materials_path.display()
            ));
        };
        let Some(mut slot) = slot_from_texture_path(&model_texture.texture_path) else {
            return Err(format!(
                "model texture '{}' did not load",
                model_texture.texture_path.display()
            ));
        };
        let model = itg_parse_milkshape_model(&data, meshes_path);
        if model.is_none() {
            return Err(format!(
                "model '{}' did not produce any geometry",
                meshes_path.display()
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

pub fn itg_parse_milkshape_model_auto_rot(path: &Path) -> Option<ItgModelAutoRot> {
    let content = fs::read_to_string(path).ok()?;
    // ITG's LoadMilkshapeAsciiBones scans for Bones directly. Separate bone
    // files need neither the MilkShape comment nor the mesh file's headers.
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
        ItgModelTextureKind::Animated => itg_resolve_animated_texture_ini(data, &texture_path),
        ItgModelTextureKind::Other if texture_path.is_file() => {
            itg_resolve_model_texture_path(data, &texture_path)
        }
        ItgModelTextureKind::Other => None,
    }
}

pub fn itg_parse_milkshape_model_layers(
    data: &noteskin_itg::NoteskinData,
    meshes_path: &Path,
    materials_path: &Path,
) -> Option<Vec<ItgResolvedModelLayer>> {
    let content = fs::read_to_string(meshes_path).ok()?;
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

    // Retain parsing scratch capacity across mesh sections, then release it
    // before resolving materials. Each expanded mesh still owns its vertices.
    let mut mesh_vertices = Vec::new();
    let mut normals = Vec::new();
    let mut triangles = Vec::new();
    for _ in 0..mesh_count {
        let mesh_header = lines.next()?;
        let material_index = itg_parse_milkshape_mesh_material_index(mesh_header);
        let vertex_count = lines.next()?.trim().parse::<usize>().ok()?;
        mesh_vertices.clear();
        mesh_vertices.reserve_exact(vertex_count);
        let mut bone_index = None;
        for vertex_index in 0..vertex_count {
            let line = lines.next()?;
            let mut parts = line.split_whitespace();
            let flags = parts.next()?.parse::<u32>().ok()?;
            let x = parts.next()?.parse::<f32>().ok()?;
            let y = parts.next()?.parse::<f32>().ok()?;
            let z = parts.next()?.parse::<f32>().ok()?;
            let mut u = parts.next()?.parse::<f32>().ok()?;
            let mut v = parts.next()?.parse::<f32>().ok()?;
            let bone = parts.next()?.parse::<i8>().ok()?;
            let bone = u8::try_from(bone).ok();
            if vertex_index == 0 {
                bone_index = bone;
            } else if bone_index != bone {
                bone_index = None;
            }
            if flags & 4 != 0 {
                if u.abs() > f32::EPSILON {
                    u = x / u;
                }
                if v.abs() > f32::EPSILON {
                    v = y / v;
                }
            }
            mesh_vertices.push(ModelVertex {
                normal: [0.0, 0.0, 1.0],
                pos: [x, y, z],
                uv: [u, v],
                tex_matrix_scale: [
                    if flags & 1 != 0 { 0.0 } else { 1.0 },
                    if flags & 2 != 0 { 0.0 } else { 1.0 },
                ],
            });
        }

        let normal_count = lines.next()?.trim().parse::<usize>().ok()?;
        normals.clear();
        normals.reserve_exact(normal_count);
        for _ in 0..normal_count {
            let mut parts = lines.next()?.split_whitespace();
            let normal = [
                parts.next()?.parse::<f32>().ok()?,
                parts.next()?.parse::<f32>().ok()?,
                parts.next()?.parse::<f32>().ok()?,
            ];
            let length = normal.iter().map(|x| x * x).sum::<f32>().sqrt();
            normals.push(if length.is_finite() && length > 0.0 {
                normal.map(|x| x / length)
            } else {
                [0.0; 3]
            });
        }

        let triangle_count = lines.next()?.trim().parse::<usize>().ok()?;
        triangles.clear();
        triangles.reserve_exact(triangle_count);
        for _ in 0..triangle_count {
            let line = lines.next()?;
            let mut parts = line.split_whitespace();
            let _flags = parts.next()?;
            let i0 = parts.next()?.parse::<usize>().ok()?;
            let i1 = parts.next()?.parse::<usize>().ok()?;
            let i2 = parts.next()?.parse::<usize>().ok()?;

            let normal_indices = [
                parts.next()?.parse::<usize>().ok()?,
                parts.next()?.parse::<usize>().ok()?,
                parts.next()?.parse::<usize>().ok()?,
            ];
            let indices = [i0, i1, i2];
            if indices.iter().any(|&i| i >= mesh_vertices.len()) {
                continue;
            }
            for (&index, &normal) in indices.iter().zip(&normal_indices) {
                // Empty normal tables are tolerated for legacy flat fixtures.
                mesh_vertices[index].normal = *normals.get(normal).unwrap_or(&[0.0; 3]);
            }
            triangles.push(indices);
        }
        if triangles.is_empty() {
            continue;
        }
        let (tri_vertices, bounds) = expand_mesh_vertices(&mesh_vertices, &mut triangles);

        if !tri_vertices.is_empty() {
            model_bounds[0] = model_bounds[0].min(bounds[0]);
            model_bounds[1] = model_bounds[1].min(bounds[1]);
            model_bounds[2] = model_bounds[2].min(bounds[2]);
            model_bounds[3] = model_bounds[3].max(bounds[3]);
            model_bounds[4] = model_bounds[4].max(bounds[4]);
            model_bounds[5] = model_bounds[5].max(bounds[5]);
            meshes.push(ItgSharedMilkshapeMeshLayer {
                material_index,
                bone_index,
                vertices: tri_vertices,
                bounds,
            });
        }
    }

    drop((mesh_vertices, normals, triangles));

    if meshes.is_empty() {
        return None;
    }

    // ITG Model::LoadPieces reads each section from its own declared file.
    // Materials-only files need neither a MilkShape header nor a mesh section.
    let materials_content = if materials_path == meshes_path {
        content
    } else {
        fs::read_to_string(materials_path).ok()?
    };
    let mut lines = materials_content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"));
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
        let texture = lines.next()?.trim();
        let additive = lines.next()?.trim();
        material_textures.push(ItgMilkshapeMaterial {
            texture,
            additive,
            flags: itg_parse_model_material_flags(name),
            resolved_texture: OnceCell::new(),
            resolved_additive: OnceCell::new(),
        });
    }

    let fallback_texture = std::cell::OnceCell::new();
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
    let animation_length = material_textures
        .iter()
        .filter_map(|material| {
            material
                .resolved_texture
                .get_or_init(|| {
                    itg_resolve_model_material_texture(data, materials_path, material.texture)
                })
                .as_ref()
        })
        .map(|texture| texture.tex.uv_cycle_seconds.unwrap_or(1.0))
        .fold(0.0f32, f32::max);
    let mut layers = Vec::with_capacity(meshes.len());
    for mesh in meshes {
        let texture_with_flags = if mesh.material_index >= 0 {
            material_textures
                .get(mesh.material_index as usize)
                .and_then(|material| {
                    (if material.texture.trim().trim_matches('"').is_empty() {
                        Some(ItgResolvedModelTexture::from_path(PathBuf::from(
                            MODEL_WHITE_TEXTURE,
                        )))
                    } else {
                        material
                            .resolved_texture
                            .get()
                            .and_then(Option::as_ref)
                            .cloned()
                    })
                    .map(|resolved| (resolved, material.flags))
                })
        } else if mesh.material_index == -1 {
            Some((
                ItgResolvedModelTexture::from_path(PathBuf::from(MODEL_WHITE_TEXTURE)),
                ItgModelMaterialFlags::default(),
            ))
        } else {
            None
        }
        .or_else(|| {
            fallback_texture
                .get_or_init(|| itg_resolve_model_texture_path(data, materials_path))
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
        let additive = usize::try_from(mesh.material_index)
            .ok()
            .and_then(|index| material_textures.get(index))
            .and_then(|material| {
                material
                    .resolved_additive
                    .get_or_init(|| {
                        itg_resolve_model_material_texture(data, materials_path, material.additive)
                    })
                    .clone()
            });
        layers.push(ItgResolvedModelLayer {
            animation_length,
            additive,
            mesh: Arc::new(ModelMesh {
                vertices: mesh.vertices,
                bounds,
            }),
            texture,
            flags,
            bone_index: mesh.bone_index,
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
    itg_parse_milkshape_model_layers(data, path, path)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    mod animated_materials {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/animated_materials/cases.rs"
        ));
    }

    mod model_preparation {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/model_preparation/cases.rs"
        ));
    }

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
                normal: [0.0, 0.0, 1.0],
                pos: [0.0, 0.0, 0.0],
                uv: [0.0, 0.0],
                tex_matrix_scale: [1.0, 1.0],
            }]),
            bounds: [0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        })
    }

    #[test]
    fn bone_animation_accepts_headerless_files_and_loops() {
        let path = std::env::temp_dir().join(format!(
            "deadsync-headerless-bones-{}.txt",
            std::process::id()
        ));
        let bones = "Bones: 1\n\"root\"\n\"\"\n0 0 0 0 0 0 0\n0\n3\n\
                     0 0 0 0\n30 0 0 2.35619449\n60 0 0 -1.570796327\n";
        for header in ["", "// MilkShape 3D ASCII\nFrames: 999\nFrame: 1\n"] {
            fs::write(&path, format!("{header}{bones}")).unwrap();
            let rotation = itg_parse_milkshape_model_auto_rot(&path)
                .expect("the Bones section defines the animation without a file header");
            assert_eq!(rotation.total_frames, 60.0);
            assert_eq!(rotation.z_keys.len(), 3);
            for (time, expected) in [(0.0, 0.0), (0.5, 67.5), (1.5, 202.5), (2.0, 0.0)] {
                let angle =
                    crate::draw::model_auto_rot_z_at(rotation.total_frames, &rotation.z_keys, time)
                        .unwrap();
                assert!((angle - expected).abs() < 1e-4, "{time}: {angle}");
            }
        }
        for invalid in ["Materials: 0\n", "Bones: 0\n", "Bones: 1\n\"root\"\n"] {
            fs::write(&path, invalid).unwrap();
            assert!(itg_parse_milkshape_model_auto_rot(&path).is_none());
        }
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn model_slot_plan_from_layer_honors_nomove_flags() {
        let layer = ItgResolvedModelLayer {
            animation_length: 1.0,
            additive: None,
            mesh: test_mesh(),
            texture: ItgResolvedModelTexture {
                sphere_mapped: false,
                texture_path: PathBuf::from("tap.png"),
                animation: None,
                tex: ItgModelTexturePath {
                    uv_velocity: [2.0, -1.0],
                    uv_offset: [0.25, 0.5],
                    uv_cycle_seconds: Some(0.75),
                },
            },
            flags: ItgModelMaterialFlags { nomove: true },
            bone_index: None,
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
    fn material_texgen_preserves_normals_and_both_texture_stages() {
        let root = temp_model_root("material-stages");
        fs::write(root.join("base.png"), []).unwrap();
        fs::write(root.join("reflection.png"), []).unwrap();
        fs::write(root.join("second.png"), []).unwrap();
        fs::write(root.join("shine sphere.ini"), "[AnimatedTexture]\nFrame0000=reflection.png\nDelay0000=0.2\nFrame0001=second.png\nDelay0001=0.3\n").unwrap();
        let path = root.join("model.txt");
        let source = r#"// MilkShape 3D ASCII
Meshes: 1
"shell" 0 0
3
0 -1 -1 0 0 0 -1
0 1 -1 0 1 0 -1
0 0 1 0 0 1 -1
2
0 0 3
2 0 0
2
0 0 1 2 0 0 0 1
0 0 1 2 1 1 1 1
Materials: 1
"reflective"
0 0 0 1
1 1 1 1
0 0 0 1
0 0 0 1
0
1
"base.png"
"shine sphere.ini"
"#;
        fs::write(&path, source).unwrap();
        let data = noteskin_itg::NoteskinData {
            name: "fixture".into(),
            overrides: vec![],
            metrics: noteskin_itg::IniData::default(),
            search_dirs: vec![root.clone()],
        };
        let layers = itg_parse_milkshape_model_layers(&data, &path, &path).unwrap();
        assert_eq!(layers.len(), 1);
        let layer = &layers[0];
        assert!(!layer.texture.sphere_mapped);
        let additive = layer.additive.as_ref().unwrap();
        assert!(
            additive.sphere_mapped,
            "the INI name selects texgen, not its frame names"
        );
        assert_eq!(additive.animation.as_ref().unwrap().frames.len(), 2);
        assert_eq!(additive.tex.uv_cycle_seconds, Some(0.5));
        assert_eq!(
            layer.animation_length, 1.0,
            "only diffuse materials set Model animation length"
        );
        // ITG deflates normals into indexed vertices: the last assignment wins.
        assert!(
            layer
                .mesh
                .vertices
                .iter()
                .all(|vertex| vertex.normal == [1.0, 0.0, 0.0])
        );
        fs::write(&path, source.replace("\"base.png\"", "\"\"")).unwrap();
        let layers = itg_parse_milkshape_model_layers(&data, &path, &path).unwrap();
        assert_eq!(
            layers[0].texture.texture_path,
            Path::new(MODEL_WHITE_TEXTURE)
        );
        assert!(layers[0].additive.is_some());
        let _ = fs::remove_dir_all(root);
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
            sphere_mapped: false,
            texture_path: PathBuf::from("tap.png"),
            animation: None,
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

        let slots = itg_load_model_slots(
            &model_path,
            &model_path,
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
    fn load_model_slots_reads_separate_pieces() {
        let root = temp_model_root("model-pieces");
        let material_dir = root.join("materials");
        fs::create_dir(&material_dir).unwrap();
        let texture_path = material_dir.join("tap.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
            .save(&texture_path)
            .unwrap();
        let meshes = root.join("meshes.txt");
        let materials = material_dir.join("materials.txt");
        let bones = root.join("bones.txt");
        fs::write(
            &meshes,
            "// MilkShape 3D ASCII\nMeshes: 1\n\"mesh\" 0 0\n3\n\
             0 -1 -1 0 0 0 0\n0 1 -1 0 1 0 0\n0 0 1 0 0 1 0\n\
             0\n1\n0 0 1 2 0 0 0 1\n",
        )
        .unwrap();
        fs::write(
            &materials,
            "Materials: 1\n\"mat\"\n0 0 0 1\n1 1 1 1\n0 0 0 1\n\
             0 0 0 1\n0\n1\n\"tap.png\"\n\"\"\n",
        )
        .unwrap();
        fs::write(
            &bones,
            "Bones: 1\n\"rotor\"\n\"\"\n0 0 0 0 0 0 0\n0\n2\n\
             0 0 0 0\n30 0 0 1.570796327\n",
        )
        .unwrap();
        let slots = itg_load_model_slots(
            &meshes,
            &materials,
            &bones,
            |path| {
                assert_eq!(path, texture_path);
                Some(None)
            },
            |slot, plan| *slot = Some(plan),
        )
        .expect("load separate mesh, material and bone files");
        assert_eq!(slots.len(), 1);
        let plan = slots[0].as_ref().unwrap();
        assert_eq!(plan.model.as_ref().unwrap().vertices.len(), 3);
        assert_eq!(plan.model_auto_rot_total_frames, 30.0);
        assert_eq!(plan.model_auto_rot_z_keys.len(), 2);
        assert!((plan.model_auto_rot_z_keys[1].z_deg - 90.0).abs() < 1e-4);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn model_bone_rotation_leaves_unbound_meshes_still() {
        let root = temp_model_root("bone-bindings");
        let path = root.join("model.txt");
        let mut source = String::from("// MilkShape 3D ASCII\nMeshes: 5\n");
        // Four stationary pieces and one rotating piece share a material.
        for (index, bone) in [-1, -1, -1, -1, 0].into_iter().enumerate() {
            source.push_str(&format!(
                "\"part{index}\" 0 -1\n3\n\
                 0 -1 -1 0 0 0 {bone}\n\
                 0 1 -1 0 1 0 {bone}\n\
                 0 0 1 0 0 1 {bone}\n\
                 0\n1\n0 0 1 2 0 0 0 1\n"
            ));
        }
        source.push_str(
            "Materials: 0\nBones: 1\n\"rotor\"\n\"\"\n0 0 0 0 0 0 0\n0\n3\n\
             0 0 0 0\n30 0 0 1.570796327\n60 0 0 3.141592654\n",
        );
        fs::write(&path, source).unwrap();
        let slots = itg_load_model_slots(
            &path,
            &path,
            &path,
            |texture| {
                assert_eq!(texture, Path::new(MODEL_WHITE_TEXTURE));
                Some(None)
            },
            |slot, plan| *slot = Some(plan),
        )
        .expect("load synthetic model with rigid bone bindings");
        assert_eq!(slots.len(), 5);
        for (index, slot) in slots.into_iter().enumerate() {
            let plan = slot.expect("model plan");
            assert_eq!(plan.model.as_ref().unwrap().vertices.len(), 3);
            assert_eq!(plan.model_auto_rot_z_keys.is_empty(), index != 4);
            for (time, angle) in [(0.0, 0.0), (0.5, 45.0), (1.5, 135.0), (2.0, 0.0)] {
                let draw = crate::draw::model_draw_at(
                    plan.model_draw,
                    &plan.model_timeline,
                    plan.model_effect,
                    plan.model_auto_rot_total_frames,
                    &plan.model_auto_rot_z_keys,
                    time,
                    0.0,
                );
                let expected = if index == 4 { angle } else { 0.0 };
                assert!(
                    (draw.rot[2] - expected).abs() < 1e-4,
                    "part {index}, t={time}"
                );
            }
        }
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn animated_texture_keeps_zero_based_precedence_and_one_based_fallback() {
        let root = temp_model_root("first-frame");
        for name in ["zero.png", "one.png"] {
            fs::write(root.join(name), []).unwrap();
        }
        let path = root.join("texture.ini");
        let data = noteskin_itg::NoteskinData {
            overrides: Vec::new(),
            name: "test".to_string(),
            metrics: noteskin_itg::IniData::default(),
            search_dirs: vec![root.clone()],
        };
        for (frames, expected) in [
            (
                "Frame0000=zero.png\nFrame0001=one.png\nDelay0000=0.25\nDelay0001=0.5\n",
                Some(("zero.png", 0.75)),
            ),
            ("Frame0001=one.png\nDelay0001=0.5\n", Some(("one.png", 0.5))),
            (
                "Frame0000=one.png\nDelay0000=0.5\nFrame0001=one.png\nDelay0001=0.75\n",
                Some(("one.png", 1.25)),
            ),
            ("Frame0000=\nFrame0001=one.png\nDelay0001=0.5\n", None),
            ("Delay0000=1\n", None),
        ] {
            fs::write(&path, format!("[AnimatedTexture]\n{frames}")).unwrap();
            let resolved = itg_resolve_animated_texture_ini(&data, &path);
            if let Some((name, cycle)) = expected {
                let resolved = resolved.expect("first frame should resolve");
                assert_eq!(resolved.texture_path, root.join(name));
                assert_eq!(resolved.tex.uv_cycle_seconds, Some(cycle));
                assert_eq!(resolved.animation.is_some(), name == "zero.png");
            } else {
                assert!(resolved.is_none());
            }
        }
        for name in ["zero.png", "one.png", "texture.ini"] {
            fs::remove_file(root.join(name)).unwrap();
        }
        fs::remove_dir(root).unwrap();
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
            overrides: Vec::new(),
            name: "test".to_string(),
            metrics: noteskin_itg::IniData::default(),
            search_dirs: vec![root.clone()],
        };

        let layers = itg_parse_milkshape_model_layers(&data, &model_path, &model_path)
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

// Parsed triangles contain valid indices. Their three-vertex arrays give
// collect an exact length, so each mesh needs only its final shared buffer.
fn expand_mesh_vertices(
    mesh_vertices: &[ModelVertex],
    triangles: &mut Vec<[usize; 3]>,
) -> (Arc<[ModelVertex]>, [f32; 6]) {
    let vertices: Arc<[ModelVertex]> = triangles
        .drain(..)
        .flat_map(|indices| indices.map(|index| mesh_vertices[index]))
        .collect();
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    // Keep this scan outside the iterator so vertex copies and bounds updates
    // can each be optimized without mutable state captured by the collector.
    for vertex in vertices.iter() {
        bounds[0] = bounds[0].min(vertex.pos[0]);
        bounds[1] = bounds[1].min(vertex.pos[1]);
        bounds[2] = bounds[2].min(vertex.pos[2]);
        bounds[3] = bounds[3].max(vertex.pos[0]);
        bounds[4] = bounds[4].max(vertex.pos[1]);
        bounds[5] = bounds[5].max(vertex.pos[2]);
    }
    (vertices, bounds)
}

#[cfg(test)]
#[path = "../tests/shared_mesh_preparation/mod.rs"]
mod shared_mesh_preparation;
