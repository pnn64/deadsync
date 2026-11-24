use std::sync::{Arc, LazyLock};

use deadlib_present::actors::{Actor, MeshEnvironment, MeshTexture, SizeSpec};
use deadlib_present::render::{BlendMode, CullMode, MeshSampler, TexturedMeshVertex};
use deadlib_present::space::{current_metrics, menu_perspective};
use deadsync_theme::views::Grade;
use glam::{Mat4, Vec3};

use crate::presentation::Layout;

#[repr(usize)]
#[derive(Clone, Copy)]
enum Model {
    Star,
    S,
    A,
    B,
    C,
    D,
    F,
    Plus,
    Minus,
    SmoothPlus,
    SmoothMinus,
    Arrow,
}

struct Geometry {
    plain: Arc<[TexturedMeshVertex]>,
    reflective: Arc<[TexturedMeshVertex]>,
}

// Indexed OpenITG MilkShape exports. Each entry starts with u16 vertex and
// index counts, then eight f32s per vertex (position, UV, normal) and u16 indices.
static MODELS: LazyLock<[Geometry; Model::Arrow as usize + 1]> = LazyLock::new(|| {
    let mut bytes = include_bytes!("../../assets/evaluation-models.bin").as_slice();
    let models = std::array::from_fn(|_| {
        let plain = read_indexed_mesh(&mut bytes, 1.0);
        let reflective = plain
            .iter()
            .map(|vertex| {
                let mut vertex = *vertex;
                vertex.normal[3] = 7.0;
                vertex
            })
            .collect();
        Geometry { plain, reflective }
    });
    assert!(bytes.is_empty());
    models
});

pub(super) fn read_indexed_mesh(bytes: &mut &[u8], texture_mode: f32) -> Arc<[TexturedMeshVertex]> {
    let vertex_count = read_u16(bytes);
    let index_count = read_u16(bytes);
    let vertices = (0..vertex_count)
        .map(|_| {
            let [x, y, z, u, v, nx, ny, nz] =
                std::array::from_fn(|_| f32::from_le_bytes(read(bytes)));
            TexturedMeshVertex {
                pos: [x, -y, z],
                uv: [u, v],
                normal: [nx, -ny, nz, texture_mode],
                color: [1.0; 4],
                ..Default::default()
            }
        })
        .collect::<Vec<_>>();
    (0..index_count)
        .map(|_| vertices[read_u16(bytes)])
        .collect()
}

fn read_u16(bytes: &mut &[u8]) -> usize {
    u16::from_le_bytes(read(bytes)) as usize
}

fn read<const N: usize>(bytes: &mut &[u8]) -> [u8; N] {
    let (value, remaining) = bytes.split_first_chunk().unwrap();
    *bytes = remaining;
    *value
}

#[repr(usize)]
#[derive(Clone, Copy)]
enum Surface {
    Gold,
    Silver,
    Red,
    Yellow,
    Green,
    Orange,
    Purple,
    Arrow,
    Plastic,
    Reverse,
    ArrowAdd,
}

static SURFACES: LazyLock<[Arc<str>; Surface::ArrowAdd as usize + 1]> = LazyLock::new(|| {
    [
        "gold",
        "silver",
        "red",
        "yellow",
        "green",
        "orange",
        "purple",
        "arrow-sphere",
        "plastic",
        "plastic-reverse",
        "arrow-sphere-add",
    ]
    .map(|name| Arc::from(format!("openitg/evaluation-{name}.png")))
});

pub(super) struct Scene {
    projection: Mat4,
    view: Option<Mat4>,
    center: [f32; 2],
    transform: Mat4,
    opacity: f32,
    layer: i16,
}

impl Scene {
    pub(super) fn orthographic(
        layout: Layout,
        center: [f32; 2],
        transform: Mat4,
        layer: i16,
    ) -> Self {
        Self {
            projection: current_metrics().projection()
                * Mat4::from_scale(Vec3::new(1.0, 1.0, 1.0 / 512.0)),
            view: None,
            center: layout.position(center[0], center[1]),
            transform,
            opacity: 1.0,
            layer,
        }
    }

    fn model(&self, model: Model, surface: Surface, x: f32) -> Actor {
        let additive = match surface {
            Surface::Red | Surface::Yellow | Surface::Green | Surface::Orange => {
                Some(Surface::Plastic)
            }
            Surface::Purple => Some(Surface::Reverse),
            Surface::Arrow => Some(Surface::ArrowAdd),
            _ => None,
        };
        let geom = &MODELS[model as usize];
        let vertices = if additive.is_some() {
            &geom.reflective
        } else {
            &geom.plain
        };
        self.mesh(
            vertices,
            &SURFACES[surface as usize],
            additive.map(|key| &SURFACES[key as usize]),
            None,
            x,
        )
    }

    pub(super) fn mesh(
        &self,
        vertices: &Arc<[TexturedMeshVertex]>,
        texture: &Arc<str>,
        additive: Option<&Arc<str>>,
        sampler: Option<MeshSampler>,
        x: f32,
    ) -> Actor {
        let transform = self.transform * Mat4::from_translation(Vec3::new(x, 0.0, 0.0));
        Actor::TexturedMesh {
            environment: Some(MeshEnvironment {
                sampler,
                camera: self.view.map(|view| (self.projection, view)),
                transform,
                additive_texture: additive.cloned(),
                additive_uv: [0.0, 0.0, 1.0, 1.0],
            }),
            align: [0.0; 2],
            offset: self.center,
            world_z: 0.0,
            size: [SizeSpec::Px(0.0); 2],
            local_transform: transform,
            texture: MeshTexture::Key(Arc::clone(texture)),
            tint: [1.0, 1.0, 1.0, self.opacity],
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
            clear_depth: false,
            clear_depth_after: false,
            cull_mode: CullMode::Back,
            visible: true,
            blend: BlendMode::Alpha,
            z: self.layer,
        }
    }

    pub(super) fn push(
        &mut self,
        actors: &mut Vec<Actor>,
        draw: impl FnOnce(&mut Self, &mut Vec<Actor>),
    ) {
        actors.push(Actor::CameraPush {
            view_proj: self.projection,
        });
        let first = actors.len();
        draw(self, actors);
        if let Actor::TexturedMesh { clear_depth, .. } = &mut actors[first] {
            *clear_depth = true;
        }
        if let Some(Actor::TexturedMesh {
            clear_depth_after, ..
        }) = actors.last_mut()
        {
            *clear_depth_after = true;
        }
        actors.push(Actor::CameraPop);
    }
}

pub(super) fn push_background_arrow(
    actors: &mut Vec<Actor>,
    [width, height]: [f32; 2],
    elapsed: f32,
) {
    let center = [width * 0.5, height * 0.5];
    let Some((projection, view)) = menu_perspective([width, height], center, 45.0) else {
        return;
    };
    let metrics = current_metrics();
    let world_to_screen = Mat4::from_translation(Vec3::new(-metrics.left, metrics.top, 0.0))
        * Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0));
    let view = view * world_to_screen;
    let tilt = (elapsed * std::f32::consts::TAU / 5.0).sin() * 5.0;
    let mut scene = Scene {
        projection: projection * view,
        view: Some(view),
        center,
        transform: Mat4::from_rotation_z(-30.0_f32.to_radians())
            * Mat4::from_rotation_y(-15.0_f32.to_radians())
            * Mat4::from_rotation_x(-30.0_f32.to_radians())
            * Mat4::from_rotation_z(tilt.to_radians())
            * Mat4::from_scale(Vec3::splat(2.0 * height / 480.0)),
        opacity: 0.12,
        layer: -23,
    };
    scene.push(actors, |scene, actors| {
        actors.push(scene.model(Model::Arrow, Surface::Arrow, 0.0))
    });
}

pub(super) fn push_grade(
    actors: &mut Vec<Actor>,
    layout: Layout,
    [x, y]: [f32; 2],
    scale: f32,
    grade: Grade,
    elapsed: f32,
) {
    let scale = layout.size([scale, 0.0])[0];
    let mut scene = Scene::orthographic(layout, [x, y], Mat4::from_scale(Vec3::splat(scale)), 101);
    let wave = (elapsed * std::f32::consts::TAU).sin();
    let wag = (wave * 10.0).to_radians();
    let spin = (-90.0 + elapsed * 40.0).to_radians();
    scene.push(actors, |scene, actors| {
        use Grade::*;
        if let Some((zoom, positions)) = stars(grade) {
            let center = scene.center;
            scene.transform *= Mat4::from_rotation_y((wave * 4.0).to_radians())
                * Mat4::from_rotation_z(wag)
                * Mat4::from_scale(Vec3::splat(zoom));
            for &[dx, dy] in positions {
                scene.center = [center[0] + dx * scale, center[1] + dy * scale];
                actors.push(scene.model(Model::Star, Surface::Gold, 0.0));
            }
            return;
        }
        let (model, surface, rotation, suffix, xs) = match grade {
            Tier05 => (
                Model::S,
                Surface::Silver,
                spin,
                Some(Model::SmoothPlus),
                [-25.0, 30.0],
            ),
            Tier06 => (Model::S, Surface::Silver, spin, None, [0.0; 2]),
            Tier07 => (
                Model::S,
                Surface::Silver,
                spin,
                Some(Model::SmoothMinus),
                [-25.0, 30.0],
            ),
            Tier08 => (
                Model::A,
                Surface::Red,
                spin,
                Some(Model::Plus),
                [-20.0, 35.0],
            ),
            Tier09 => (Model::A, Surface::Red, spin, None, [0.0; 2]),
            Tier10 => (
                Model::A,
                Surface::Red,
                spin,
                Some(Model::Minus),
                [-20.0, 35.0],
            ),
            Tier11 => (
                Model::B,
                Surface::Yellow,
                spin,
                Some(Model::Plus),
                [-20.0, 35.0],
            ),
            Tier12 => (Model::B, Surface::Yellow, spin, None, [0.0; 2]),
            Tier13 => (
                Model::B,
                Surface::Yellow,
                spin,
                Some(Model::Minus),
                [-20.0, 35.0],
            ),
            Tier14 => (
                Model::C,
                Surface::Green,
                wag,
                Some(Model::Plus),
                [-25.0, 35.0],
            ),
            Tier15 => (Model::C, Surface::Green, wag, None, [0.0; 2]),
            Tier16 => (
                Model::C,
                Surface::Green,
                wag,
                Some(Model::Minus),
                [-25.0, 35.0],
            ),
            Tier17 => (Model::D, Surface::Orange, 0.0, None, [0.0; 2]),
            Failed => (Model::F, Surface::Purple, wag, None, [0.0; 2]),
            _ => unreachable!(),
        };
        let zoom = if suffix.is_none() {
            0.85
        } else if matches!(model, Model::S) {
            0.8
        } else {
            0.7
        };
        scene.transform *= Mat4::from_rotation_y(rotation);
        if matches!(grade, Tier17 | Failed) {
            scene.transform *= Mat4::from_rotation_z(wag);
        }
        scene.transform *= Mat4::from_scale(Vec3::splat(zoom));
        actors.push(scene.model(model, surface, xs[0]));
        if let Some(suffix) = suffix {
            actors.push(scene.model(suffix, surface, xs[1]));
        }
    });
}

fn stars(grade: Grade) -> Option<(f32, &'static [[f32; 2]])> {
    Some(match grade {
        Grade::Quint => (
            0.43,
            &[
                [-27.0, -21.0],
                [27.0, -21.0],
                [0.0, 0.0],
                [-27.0, 21.0],
                [27.0, 21.0],
            ],
        ),
        Grade::Tier01 => (
            0.5,
            &[[-25.0, -22.0], [25.0, -22.0], [-25.0, 22.0], [25.0, 22.0]],
        ),
        Grade::Tier02 => (0.5, &[[-25.0, 20.0], [0.0, -20.0], [25.0, 20.0]]),
        Grade::Tier03 => (0.6, &[[-19.0, 20.0], [19.0, -20.0]]),
        Grade::Tier04 => (0.8, &[[0.0; 2]]),
        _ => return None,
    })
}
