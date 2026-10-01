// Frozen from 1be9bf0d7 (0.5.1653); unchanged helpers and types are shared.
use super::*;

pub(super) fn itg_normalized_asset_ref(raw: &str) -> Option<String> {
    let rel = raw.trim().trim_matches('"').trim_matches('\'');
    if rel.is_empty() {
        None
    } else {
        Some(rel.replace('\\', "/"))
    }
}

pub(super) fn itg_resolve_relative_or_noteskin_path(
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
    for dir in &data.search_dirs {
        if let Some(path) = itg_resolve_relative_file(dir, rel_path) {
            return Some(data.override_path(path));
        }
    }
    data.resolve_path("", &rel)
}

pub(super) fn itg_resolve_animated_texture_ini(
    data: &noteskin_itg::NoteskinData,
    path: &Path,
) -> Option<ItgResolvedModelTexture> {
    let ini = noteskin_itg::IniData::parse_file(path).ok()?;
    let (first_frame_idx, frame) = if let Some(frame) = ini.get("AnimatedTexture", "Frame0000") {
        (0, frame)
    } else {
        (1, ini.get("AnimatedTexture", "Frame0001")?)
    };
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
        frames.push(ItgTextureFrame {
            path: itg_resolve_relative_or_noteskin_path(data, path, frame)?,
            delay,
        });
        cycle_seconds += delay;
    }
    Some(ItgResolvedModelTexture {
        sphere_mapped: path.to_string_lossy().contains("sphere"),
        // Repeated references to one image need no atlas. Keep its full UV
        // domain for scrolling materials instead of adding duplicate tiles.
        animation: (frames.iter().any(|frame| frame.path != texture_path)
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

pub(super) fn itg_resolve_model_material_texture(
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

pub(super) fn itg_resolve_model_texture_path(
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

pub(super) fn itg_parse_milkshape_model_layers(
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

    for _ in 0..mesh_count {
        let mesh_header = lines.next()?;
        let material_index = itg_parse_milkshape_mesh_material_index(mesh_header);
        let vertex_count = lines.next()?.trim().parse::<usize>().ok()?;
        let mut mesh_vertices = Vec::with_capacity(vertex_count);
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
        let mut normals = Vec::with_capacity(normal_count);
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
        let mut tri_vertices: Vec<ModelVertex> = Vec::with_capacity(triangle_count * 3);
        let mut bounds = [
            f32::INFINITY,
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        let mut triangles = Vec::with_capacity(triangle_count);
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
        for indices in triangles {
            for vtx in indices.map(|index| mesh_vertices[index]) {
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
                bone_index,
                vertices: tri_vertices,
                bounds,
            });
        }
    }

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
        let texture_line = lines.next()?.trim().to_string();
        let additive = lines.next()?.trim().to_string();
        material_textures.push((texture_line, additive, itg_parse_model_material_flags(name)));
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
        .filter_map(|(raw, _, _)| itg_resolve_model_material_texture(data, materials_path, raw))
        .map(|texture| texture.tex.uv_cycle_seconds.unwrap_or(1.0))
        .fold(0.0f32, f32::max);
    let mut layers = Vec::with_capacity(meshes.len());
    for mesh in meshes {
        let texture_with_flags = if mesh.material_index >= 0 {
            material_textures
                .get(mesh.material_index as usize)
                .and_then(|(raw, _, flags)| {
                    (if raw.trim().trim_matches('"').is_empty() {
                        Some(ItgResolvedModelTexture::from_path(PathBuf::from(
                            MODEL_WHITE_TEXTURE,
                        )))
                    } else {
                        itg_resolve_model_material_texture(data, materials_path, raw)
                    })
                    .map(|resolved| (resolved, *flags))
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
            .and_then(|(_, raw, _)| itg_resolve_model_material_texture(data, materials_path, raw));
        layers.push(ItgResolvedModelLayer {
            animation_length,
            additive,
            mesh: Arc::new(ModelMesh {
                vertices: mesh.vertices.into(),
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
