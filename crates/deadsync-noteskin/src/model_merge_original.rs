// Frozen from 3a64350287c8fda7f3d0bfa697cfa39c40c4af2c for differential tests and paired benchmarks.
use super::*;

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

    // Retain parsing scratch capacity across mesh sections, then release it
    // before resolving materials. Each expanded mesh still owns its vertices.
    let mut mesh_vertices = Vec::new();
    let mut normals = Vec::new();
    let mut triangles = Vec::new();
    for _ in 0..mesh_count {
        let mesh_header = lines.next()?;
        let name = mesh_header.strip_prefix('"')?.split_once('"')?.0;
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
                name: name.to_owned(),
                material_index,
                bone_index,
                vertices: tri_vertices,
                bounds,
            });
        }
    }

    drop((mesh_vertices, normals, triangles));

    // The rendering path supports per-vertex texture matrix scaling. Native
    // RageModelGeometry::MergeMeshes appends mesh 1 to mesh 0 without removing
    // mesh 1 or replacing mesh 0's material/bone binding.
    if mesh_count == 2 && meshes.len() == 2 && meshes[0].name == meshes[1].name {
        let mut vertices = Vec::with_capacity(meshes[0].vertices.len() + meshes[1].vertices.len());
        vertices.extend_from_slice(&meshes[0].vertices);
        vertices.extend_from_slice(&meshes[1].vertices);
        meshes[0].vertices = vertices.into();
        for axis in 0..3 {
            meshes[0].bounds[axis] = meshes[0].bounds[axis].min(meshes[1].bounds[axis]);
            meshes[0].bounds[axis + 3] = meshes[0].bounds[axis + 3].max(meshes[1].bounds[axis + 3]);
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
        let mut read_color = || {
            let mut parts = lines.next()?.split_whitespace();
            let mut color = [0.0; 4];
            for value in &mut color {
                *value = parts.next()?.parse::<f32>().ok()?;
                if !value.is_finite() {
                    return None;
                }
            }
            Some(color)
        };
        let ambient = read_color()?;
        let diffuse = read_color()?;
        let specular = read_color()?;
        let emissive = read_color()?;
        let shininess = lines.next()?.parse::<f32>().ok()?;
        let transparency = lines.next()?.parse::<f32>().ok()?;
        if !shininess.is_finite() || !transparency.is_finite() {
            return None;
        }
        let texture = lines.next()?.trim();
        let additive = lines.next()?.trim();
        material_textures.push(ItgMilkshapeMaterial {
            material: ModelMaterial {
                ambient,
                diffuse,
                specular,
                emissive,
                shininess,
                transparency,
                modulate: true,
            },
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
                material: Some(
                    usize::try_from(mesh.material_index)
                        .ok()
                        .and_then(|index| material_textures.get(index))
                        .map_or_else(ModelMaterial::unassigned, |material| material.material),
                ),
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
