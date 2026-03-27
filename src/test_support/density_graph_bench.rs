use crate::engine::gfx::{BlendMode, MeshMode};
use crate::engine::present::actors::{Actor, SizeSpec};
use crate::engine::present::density::{self, DensityHistCache};
use crate::engine::space::screen_center_x;
use crate::game::timing::{TimingData, TimingSegments};
use std::sync::Arc;

pub const SCENARIO_NAME: &str = "density-graph";

pub struct DensityGraphBenchFixture {
    cache: Option<DensityHistCache>,
    offset: f32,
    visible_width: f32,
    offset_xy: [f32; 2],
}

impl DensityGraphBenchFixture {
    pub fn build(&self) -> Vec<Actor> {
        let Some(cache) = self.cache.as_ref() else {
            return Vec::new();
        };
        let verts = cache.mesh(self.offset, self.visible_width);
        if verts.is_empty() {
            return Vec::new();
        }
        vec![Actor::Mesh {
            align: [0.0, 0.0],
            offset: self.offset_xy,
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            vertices: Arc::from(verts.into_boxed_slice()),
            mode: MeshMode::Triangles,
            visible: true,
            blend: BlendMode::Alpha,
            z: 40,
        }]
    }
}

pub fn fixture() -> DensityGraphBenchFixture {
    let measure_nps = bench_measure_nps();
    let peak_nps = measure_nps.iter().copied().fold(0.0_f64, f64::max);
    let timing = TimingData::from_segments(
        0.0,
        0.0,
        &TimingSegments {
            beat0_offset_adjust: 0.0,
            bpms: vec![(0.0, 180.0)],
            stops: Vec::new(),
            delays: Vec::new(),
            warps: Vec::new(),
            speeds: Vec::new(),
            scrolls: Vec::new(),
            fakes: Vec::new(),
        },
        &[],
    );
    let measure_seconds: Vec<f32> = (0..measure_nps.len())
        .map(|measure| timing.get_time_for_beat((measure as f32) * 4.0))
        .collect();
    let visible_width = 286.0_f32;
    let cache = density::build_density_histogram_cache(
        &measure_nps,
        peak_nps,
        &measure_seconds,
        0.0,
        timing.get_time_for_beat(measure_nps.len() as f32 * 4.0),
        1144.0,
        64.0,
        None,
        1.0,
    );

    DensityGraphBenchFixture {
        cache,
        offset: 319.0,
        visible_width,
        offset_xy: [screen_center_x() - visible_width * 0.5, 128.0],
    }
}

fn bench_measure_nps() -> Vec<f64> {
    let mut out = Vec::with_capacity(768);
    for idx in 0..768usize {
        let value = if idx < 10 {
            0.0
        } else {
            let block = (idx / 64) as f64;
            let phase = (idx % 64) as f64;
            let ridge = if phase < 12.0 {
                phase * 0.42
            } else if phase < 36.0 {
                5.04 + (phase - 12.0) * 0.17
            } else {
                9.12 - (phase - 36.0) * 0.31
            };
            let accent = if idx % 23 == 0 { 3.25 } else { 0.0 };
            (ridge + block * 0.85 + accent).max(0.0)
        };
        out.push(value);
    }
    out
}
