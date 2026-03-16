use crate::core::gfx::{BlendMode, MeshMode};
use crate::core::space::screen_center_x;
use crate::screens::components::shared::density_graph;
use crate::ui::actors::{Actor, SizeSpec};
use std::cell::RefCell;
use std::sync::Arc;

pub const SCENARIO_NAME: &str = "density-graph-life";

pub struct DensityGraphLifeBenchFixture {
    points: Vec<[f32; 2]>,
    offset: f32,
    visible_width: f32,
    thickness: f32,
    color: [f32; 4],
    offset_xy: [f32; 2],
    mesh: RefCell<Option<Arc<[crate::core::gfx::MeshVertex]>>>,
}

impl DensityGraphLifeBenchFixture {
    pub fn build(&self) -> Vec<Actor> {
        let mut mesh = self.mesh.borrow_mut();
        density_graph::update_density_life_mesh(
            &mut mesh,
            &self.points,
            self.offset,
            self.visible_width,
            self.thickness,
            self.color,
        );
        let Some(vertices) = mesh.as_ref() else {
            return Vec::new();
        };
        vec![Actor::Mesh {
            align: [0.0, 0.0],
            offset: self.offset_xy,
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            vertices: Arc::clone(vertices),
            mode: MeshMode::Triangles,
            visible: true,
            blend: BlendMode::Alpha,
            z: 41,
        }]
    }
}

pub fn fixture() -> DensityGraphLifeBenchFixture {
    let visible_width = 286.0_f32;
    DensityGraphLifeBenchFixture {
        points: bench_points(),
        offset: 312.0,
        visible_width,
        thickness: 2.0,
        color: [1.0, 1.0, 1.0, 0.8],
        offset_xy: [screen_center_x() - visible_width * 0.5, 212.0],
        mesh: RefCell::new(None),
    }
}

fn bench_points() -> Vec<[f32; 2]> {
    let mut out = Vec::with_capacity(2048);
    for idx in 0..2048usize {
        let x = idx as f32 * 0.75;
        let swing = ((idx % 64) as f32 - 32.0) * 0.52;
        let block = (idx / 128) as f32 * 3.7;
        let y = (26.0 + swing.abs() * 0.85 + block).min(63.0);
        out.push([x, y]);
        if idx % 41 == 0 {
            out.push([x, y]);
        }
    }
    out
}
