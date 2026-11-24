use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, LazyLock};

use deadlib_present::actors::{Actor, MeshTexture, SizeSpec};
use deadlib_present::render::{BlendMode, CullMode, TexturedMeshVertex};
use deadsync_theme::views::StageLifeTimeline;
use glam::{Mat4, Vec3};

use crate::presentation::{Layout, sprite};
use crate::resources::MENU_NORMAL_FONT;

const SAMPLE_COUNT: usize = 200;
const SLICE_COUNT: usize = SAMPLE_COUNT - 1;

static TEXTURES: LazyLock<[[Arc<str>; 4]; 2]> = LazyLock::new(|| {
    [
        [
            "openitg/evaluation-life-ffc-p1.png",
            "openitg/evaluation-life-fec-p1.png",
            "openitg/evaluation-life-fgc-p1.png",
            "openitg/evaluation-life-p1.png",
        ],
        [
            "openitg/evaluation-life-ffc-p2.png",
            "openitg/evaluation-life-fec-p2.png",
            "openitg/evaluation-life-fgc-p2.png",
            "openitg/evaluation-life-p2.png",
        ],
    ]
    .map(|player| player.map(Arc::from))
});

struct Band {
    vertices: Arc<[TexturedMeshVertex]>,
    key: u64,
}

pub(super) struct LifeGraph {
    source: StageLifeTimeline,
    bands: [Option<Band>; 4],
    barely: Option<f32>,
}

impl LifeGraph {
    pub(super) fn matches(&self, timeline: &StageLifeTimeline) -> bool {
        Arc::ptr_eq(&self.source.samples, &timeline.samples)
            && self.source.possible_seconds == timeline.possible_seconds
            && self.source.judgment_times == timeline.judgment_times
    }

    pub(super) fn new(timeline: &StageLifeTimeline) -> Self {
        let mut graph = Self {
            source: timeline.clone(),
            bands: [const { None }; 4],
            barely: None,
        };
        let duration = timeline.possible_seconds;
        if timeline.samples.is_empty() || !duration.is_finite() || duration <= 0.0 {
            return graph;
        }
        let samples: [f32; SAMPLE_COUNT] = std::array::from_fn(|index| {
            timeline
                .sample((index as f32 * duration) / SAMPLE_COUNT as f32)
                .clamp(0.0, 1.0)
        });
        let to_slice = |seconds: f32| {
            ((seconds * SAMPLE_COUNT as f32) / duration).clamp(0.0, SLICE_COUNT as f32) as usize
        };
        let [excellent, great, plain] = timeline.judgment_times.thresholds().map(to_slice);
        let edges = [0, excellent, great, plain, SLICE_COUNT];
        graph.bands =
            std::array::from_fn(|band| build_band(&samples, edges[band], edges[band + 1]));
        let mut min = 1.0;
        for (index, &life) in samples[..SLICE_COUNT].iter().enumerate() {
            if life < min {
                min = life;
                graph.barely = Some(index as f32 / SLICE_COUNT as f32);
            }
        }
        if !(0.0 < min && min < 0.1) {
            graph.barely = None;
        }
        graph
    }

    pub(super) fn push(
        &self,
        actors: &mut Vec<Actor>,
        layout: Layout,
        origin: [f32; 2],
        size: [f32; 2],
        p2: bool,
    ) {
        let offset = layout.position(origin[0], origin[1]);
        let [width, height] = layout.size(size);
        let transform = Mat4::from_scale(Vec3::new(width, height, 1.0));
        for (band, key) in self.bands.iter().zip(&TEXTURES[usize::from(p2)]) {
            let Some(band) = band else { continue };
            actors.push(Actor::TexturedMesh {
                environment: None,
                align: [0.0; 2],
                offset,
                world_z: 0.0,
                size: [SizeSpec::Px(width), SizeSpec::Px(height)],
                local_transform: transform,
                texture: MeshTexture::Key(Arc::clone(key)),
                tint: [1.0; 4],
                vertex_color_encoding: Default::default(),
                glow: [0.0; 4],
                vertices: Arc::clone(&band.vertices),
                geom_cache_key: band.key,
                uv_scale: [1.0; 2],
                uv_offset: [0.0; 2],
                uv_tex_shift: [0.0; 2],
                depth_test: false,
                depth_write: false,
                depth_test_on_fail: false,
                clear_depth: false,
                clear_depth_after: false,
                cull_mode: CullMode::None,
                visible: true,
                blend: BlendMode::Alpha,
                z: 8,
            });
        }
    }

    pub(super) fn push_barely(
        &self,
        actors: &mut Vec<Actor>,
        layout: Layout,
        [x, y]: [f32; 2],
        width: f32,
        elapsed: f32,
    ) {
        let Some(position) = self.barely.filter(|_| elapsed > 3.5) else {
            return;
        };
        let x = position.mul_add(width, x);
        let drop = ((elapsed - 3.5) / 0.3).clamp(0.0, 1.0).powi(2);
        let bounce = ((elapsed - 3.8) / 0.3).clamp(0.0, 1.0);
        let settle = ((elapsed - 4.1) / 0.3).clamp(0.0, 1.0).powi(2);
        let arrow_y = y - 14.0 + 20.0 * drop - 10.0 * bounce * (2.0 - bounce) + 10.0 * settle;
        let pulse = 0.75 - 0.25 * (elapsed * std::f32::consts::TAU).cos();
        actors.push(
            sprite(
                "openitg/evaluation-barely.png",
                layout.position(x, arrow_y),
                layout.size([16.0; 2]),
                [pulse, pulse, pulse, drop],
                10,
            )
            .build(0),
        );
        if elapsed > 4.25 {
            let alpha = ((elapsed - 4.25) / 0.2).clamp(0.0, 1.0).powi(2);
            let mut text = layout.text(
                [x, y - 32.0 + 20.0 * alpha],
                0.5,
                "Barely!".into(),
                0.5,
                [1.0, 1.0, 1.0, alpha],
            );
            text.font(MENU_NORMAL_FONT);
            text.shadowlength(layout.size([2.0, 0.0])[0]);
            actors.push(text.build(0));
        }
    }
}

fn build_band(samples: &[f32; SAMPLE_COUNT], start: usize, end: usize) -> Option<Band> {
    if start >= end || samples[start..=end].iter().all(|&value| value <= 0.0) {
        return None;
    }
    let mut vertices = Vec::with_capacity((end - start) * 6);
    let mut hasher = DefaultHasher::new();
    "openitg/evaluation-life".hash(&mut hasher);
    start.hash(&mut hasher);
    end.hash(&mut hasher);
    for &sample in &samples[start..=end] {
        sample.to_bits().hash(&mut hasher);
    }
    let vertex = |index: usize, y| {
        let x = index as f32 / SLICE_COUNT as f32;
        TexturedMeshVertex {
            pos: [x, y, 0.0],
            uv: [x, y],
            color: [1.0; 4],
            ..Default::default()
        }
    };
    for index in start..end {
        let left = vertex(index, 1.0 - samples[index]);
        let right = vertex(index + 1, 1.0 - samples[index + 1]);
        let bottom_left = vertex(index, 1.0);
        let bottom_right = vertex(index + 1, 1.0);
        vertices.extend([left, bottom_left, bottom_right, left, bottom_right, right]);
    }
    Some(Band {
        vertices: vertices.into(),
        key: hasher.finish().max(1),
    })
}
