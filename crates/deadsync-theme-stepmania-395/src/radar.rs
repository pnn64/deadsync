use std::sync::Arc;

use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_present::line::build_graph_line_mesh;
use deadlib_present::render::{BlendMode, MeshVertex};

use crate::presentation::Layout;

pub(crate) fn push_values(
    actors: &mut Vec<Actor>,
    layout: Layout,
    players: [Option<[f32; 5]>; 2],
    zoom: f32,
    rotation: f32,
) {
    let radius = 166.0 * 0.5 * 1.1;
    let (rotate_sin, rotate_cos) = (-rotation.to_radians()).sin_cos();
    for (side, values) in players.into_iter().enumerate() {
        let Some(values) = values else {
            continue;
        };
        let color = if side == 0 {
            [0.4, 1.0, 0.8, 1.0]
        } else {
            [1.0, 0.5, 0.2, 1.0]
        };
        let points = std::array::from_fn::<_, 6, _>(|index| {
            let index = index % 5;
            let angle = std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * index as f32 / 5.0;
            let distance = (values[index].clamp(0.0, 1.0) + 0.07) * radius * zoom;
            let (x, y) = (angle.cos() * distance, -angle.sin() * distance);
            layout.position([
                160.0 + x * rotate_cos - y * rotate_sin,
                334.0 + x * rotate_sin + y * rotate_cos,
            ])
        });
        let center = layout.position([160.0, 334.0]);
        let fill = [color[0], color[1], color[2], 0.5];
        let vertices: Arc<[MeshVertex]> = (0..5)
            .flat_map(|index| [center, points[index], points[index + 1]])
            .map(|pos| MeshVertex { pos, color: fill })
            .collect();
        push_mesh(actors, vertices);
        push_mesh(
            actors,
            build_graph_line_mesh(&points, 2.0 * layout.scale * zoom, color),
        );
    }
}

fn push_mesh(actors: &mut Vec<Actor>, vertices: Arc<[MeshVertex]>) {
    actors.push(Actor::Mesh {
        align: [0.0; 2],
        offset: [0.0; 2],
        size: [SizeSpec::Px(0.0); 2],
        tint: [1.0; 4],
        vertex_color_encoding: Default::default(),
        vertices,
        visible: true,
        blend: BlendMode::Alpha,
        z: 4,
    });
}
