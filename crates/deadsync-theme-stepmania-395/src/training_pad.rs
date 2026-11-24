use std::sync::{Arc, OnceLock};

use deadlib_present::actors::{Actor, MeshTexture, SizeSpec};
use deadlib_present::render::{BlendMode, CullMode, TexturedMeshVertex};
use glam::{Mat4, Vec3};

use crate::presentation::Layout;

struct Material {
    texture: Arc<str>,
    // Lighting changes during the original one-second pad turn. These immutable
    // vertex sets also give the renderer stable geometry-cache identities.
    poses: Vec<Arc<[TexturedMeshVertex]>>,
}

fn materials() -> &'static [Material] {
    static PAD: OnceLock<Vec<Material>> = OnceLock::new();
    PAD.get_or_init(|| {
        let bytes = include_bytes!("../assets/training-pad.bin");
        assert_eq!(&bytes[..4], b"PAD1");
        let mut cursor = 4;
        let count = read_u32(bytes, &mut cursor);
        let keys = [
            "stepmania-395/training-pad-metal",
            "stepmania-395/training-pad-blue",
            "stepmania-395/training-pad-red",
            "stepmania-395/training-pad-white",
        ];
        let mut materials = Vec::new();
        for key in keys.into_iter().take(count as usize) {
            let ambient = std::array::from_fn::<_, 4, _>(|_| read_float(bytes, &mut cursor));
            let diffuse = std::array::from_fn::<_, 4, _>(|_| read_float(bytes, &mut cursor));
            let count = read_u32(bytes, &mut cursor);
            let vertices: Vec<[f32; 8]> = (0..count)
                .map(|_| std::array::from_fn(|_| read_float(bytes, &mut cursor)))
                .collect();
            if vertices.is_empty() {
                continue;
            }
            let poses = (0..=60)
                .map(|frame| {
                    let rotation = Mat4::from_rotation_x(35.0_f32.to_radians())
                        * Mat4::from_rotation_y((180.0 + 3.0 * frame as f32).to_radians());
                    vertices
                        .iter()
                        .map(|vertex| {
                            let normal = Vec3::new(vertex[3], vertex[4], vertex[5]);
                            let diffuse_light = rotation
                                .transform_vector3(normal)
                                .normalize_or_zero()
                                .z
                                .max(0.0);
                            let color = std::array::from_fn(|channel| {
                                if channel == 3 {
                                    diffuse[3]
                                } else {
                                    (0.5 * ambient[channel] + diffuse_light * diffuse[channel])
                                        .clamp(0.0, 1.0)
                                }
                            });
                            TexturedMeshVertex {
                                pos: [vertex[0], vertex[1], vertex[2]],
                                uv: [vertex[6], vertex[7]],
                                color,
                                normal: [normal.x, normal.y, normal.z, 0.0],
                                tex_matrix_scale: [1.0; 2],
                            }
                        })
                        .collect::<Arc<[_]>>()
                })
                .collect();
            materials.push(Material {
                texture: Arc::from(key),
                poses,
            });
        }
        assert_eq!(cursor, bytes.len());
        materials
    })
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> u32 {
    let value = u32::from_le_bytes(bytes[*cursor..*cursor + 4].try_into().unwrap());
    *cursor += 4;
    value
}
fn read_float(bytes: &[u8], cursor: &mut usize) -> f32 {
    f32::from_bits(read_u32(bytes, cursor))
}

pub(crate) fn push_pad(actors: &mut Vec<Actor>, elapsed: f32) {
    let layout = Layout::current();
    let turn = ((elapsed - 4.7) / 1.0).clamp(0.0, 1.0);
    let zoom = (15.0 + 5.0 * turn) * layout.scale;
    let rotation = Mat4::from_rotation_x(35.0_f32.to_radians())
        * Mat4::from_rotation_y((180.0 + 180.0 * turn).to_radians());
    let transform = Mat4::from_scale(Vec3::new(zoom, -zoom, zoom * 0.001)) * rotation;
    let offset = layout.position([40.0 + 190.0 * turn, 310.0 + 80.0 * turn]);
    let materials = materials();
    for (index, material) in materials.iter().enumerate() {
        let vertices = &material.poses[(turn * 60.0).round() as usize];
        actors.push(Actor::TexturedMesh {
            environment: None,
            align: [0.0; 2],
            offset,
            world_z: 0.0,
            size: [SizeSpec::Px(0.0); 2],
            local_transform: transform,
            texture: MeshTexture::Key(Arc::clone(&material.texture)),
            tint: [1.0; 4],
            vertex_color_encoding: Default::default(),
            glow: [0.0; 4],
            vertices: Arc::clone(vertices),
            geom_cache_key: vertices.as_ptr() as u64,
            uv_scale: [1.0; 2],
            uv_offset: [0.0; 2],
            uv_tex_shift: [0.0; 2],
            depth_test: true,
            depth_write: true,
            depth_test_on_fail: false,
            clear_depth: index == 0,
            clear_depth_after: index + 1 == materials.len(),
            cull_mode: CullMode::None,
            visible: true,
            blend: BlendMode::Alpha,
            z: 20,
        });
    }
}
