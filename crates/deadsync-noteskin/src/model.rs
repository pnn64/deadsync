use crate::itg as noteskin_itg;
use crate::lua::itg_extract_quoted_strings;
use crate::{ModelAutoRotKey, ModelMesh, ModelVertex};
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
    let ext = model_path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase);
    if let Some(ref ext) = ext {
        if itg_is_texture_image_ext(ext) {
            return Some(ItgResolvedModelTexture::from_path(model_path.to_path_buf()));
        }
        if ext == "ini" {
            return itg_resolve_animated_texture_ini(model_path);
        }
    }
    let content = fs::read_to_string(model_path).ok()?;
    for candidate in itg_extract_quoted_strings(&content) {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(candidate_path) = itg_resolve_relative_or_noteskin_path(data, model_path, trimmed)
        else {
            continue;
        };
        let ext = candidate_path
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase);
        let Some(ext) = ext else {
            continue;
        };
        if itg_is_texture_image_ext(&ext) {
            return Some(ItgResolvedModelTexture::from_path(candidate_path));
        }
        if ext == "ini"
            && let Some(resolved) = itg_resolve_animated_texture_ini(&candidate_path)
        {
            return Some(resolved);
        }
    }
    let stem = model_path.file_stem().and_then(|s| s.to_str())?;
    let stem_lower = stem.to_ascii_lowercase();
    let derived = if stem_lower.ends_with(" model") {
        format!("{} tex", &stem[..stem.len().saturating_sub(6)])
    } else if stem_lower.ends_with("model") {
        format!("{}tex", &stem[..stem.len().saturating_sub(5)])
    } else {
        format!("{stem} tex")
    };
    data.resolve_path("", &derived).and_then(|path| {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if itg_is_texture_image_ext(&ext) {
            Some(ItgResolvedModelTexture::from_path(path))
        } else if ext == "ini" {
            itg_resolve_animated_texture_ini(&path)
        } else {
            None
        }
    })
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
    if let Some(parent) = base_file.parent() {
        if let Some(path) = itg_resolve_relative_file(parent, rel_path) {
            return Some(path);
        }
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
    let frame_key = format!("Frame{first_frame_idx:04}");
    let frame = ini.get("AnimatedTexture", &frame_key)?;
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
        let frame_key = format!("Frame{idx:04}");
        let delay_key = format!("Delay{idx:04}");
        if ini.get("AnimatedTexture", &frame_key).is_none() {
            break;
        }
        let Some(delay) = ini
            .get("AnimatedTexture", &delay_key)
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

fn itg_parse_model_material_flags(name: &str) -> ItgModelMaterialFlags {
    let lower = name.to_ascii_lowercase();
    ItgModelMaterialFlags {
        nomove: lower.contains("nomove"),
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

pub fn itg_parse_milkshape_model_auto_rot(path: &Path) -> Option<ItgModelAutoRot> {
    let content = fs::read_to_string(path).ok()?;
    if !content.to_ascii_lowercase().contains("milkshape 3d ascii") {
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
            for _ in 0..rot_count {
                let rot_line = lines.next()?;
                let mut parts = rot_line.split_whitespace();
                let frame = parts.next()?.parse::<f32>().ok()?;
                let _x = parts.next()?.parse::<f32>().ok()?;
                let _y = parts.next()?.parse::<f32>().ok()?;
                let z = parts.next()?.parse::<f32>().ok()?;
                total_frames = total_frames.max(frame);
                if bone_idx == 0 {
                    first_bone.push((frame, z.to_degrees()));
                }
            }
        }
        if first_bone.is_empty() || total_frames <= f32::EPSILON {
            return None;
        }
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
        return Some(ItgModelAutoRot {
            total_frames,
            z_keys: Arc::from(keys),
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
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if itg_is_texture_image_ext(&ext) {
        Some(ItgResolvedModelTexture::from_path(texture_path))
    } else if ext == "ini" {
        itg_resolve_animated_texture_ini(&texture_path)
    } else if texture_path.is_file() {
        itg_resolve_model_texture_path(data, &texture_path)
    } else {
        None
    }
}

pub fn itg_parse_milkshape_model_layers(
    data: &noteskin_itg::NoteskinData,
    path: &Path,
) -> Option<Vec<ItgResolvedModelLayer>> {
    let content = fs::read_to_string(path).ok()?;
    if !content.to_ascii_lowercase().contains("milkshape 3d ascii") {
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
        let name = lines.next()?.trim().to_string();
        let _ambient = lines.next()?;
        let _diffuse = lines.next()?;
        let _specular = lines.next()?;
        let _emissive = lines.next()?;
        let _shininess = lines.next()?;
        let _transparency = lines.next()?;
        let texture_line = lines.next()?.trim().to_string();
        let _alpha_map = lines.next()?;
        material_textures.push((texture_line, itg_parse_model_material_flags(&name)));
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

pub fn itg_parse_milkshape_model(
    data: &noteskin_itg::NoteskinData,
    path: &Path,
) -> Option<Arc<ModelMesh>> {
    itg_parse_milkshape_model_layers(data, path)
        .and_then(|layers| layers.into_iter().next().map(|layer| layer.mesh))
}

fn itg_is_texture_image_ext(ext: &str) -> bool {
    matches!(ext, "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp")
}
