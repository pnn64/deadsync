use super::*;
use crate::perf;
use deadsync_noteskin::{ModelVertex, SpriteDefinition};
use std::hint::black_box;

mod baseline;

struct Slot {
    def: SpriteDefinition,
    model: ModelMesh,
    mode: u8,
}

impl NoteskinSlot for Slot {
    fn sprite_def(&self) -> &SpriteDefinition {
        &self.def
    }
    fn source_size(&self) -> [i32; 2] {
        [64; 2]
    }
    fn texture_key_shared(&self) -> Arc<str> {
        unreachable!()
    }
    fn model(&self) -> Option<&ModelMesh> {
        Some(&self.model)
    }
    fn model_texture_mode(&self) -> u8 {
        self.mode
    }
    fn base_rot_sin_cos(&self) -> [f32; 2] {
        [0.0, 1.0]
    }
    fn frame_index(&self, _: f32, _: f32) -> usize {
        0
    }
    fn frame_index_from_phase(&self, _: f32) -> usize {
        0
    }
    fn uv_for_frame_at(&self, _: usize, _: f32) -> [f32; 4] {
        [0.0, 0.0, 1.0, 1.0]
    }
    fn model_draw_at(&self, _: f32, _: f32) -> ModelDrawState {
        ModelDrawState::default()
    }
    fn model_glow_with_draw(&self, _: ModelDrawState, _: f32, _: f32, _: f32) -> Option<[f32; 4]> {
        None
    }
    fn model_uv_params(&self, _: [f32; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
        ([1.0; 2], [0.0; 2], [0.0; 2])
    }
}

fn slot(count: usize, mode: u8, mirror_h: bool, mirror_v: bool) -> Slot {
    Slot {
        def: SpriteDefinition {
            mirror_h,
            mirror_v,
            rotation_deg: 37,
            ..SpriteDefinition::default()
        },
        mode,
        model: ModelMesh {
            vertices: (0..count)
                .map(|n| ModelVertex {
                    normal: [0.25, -0.0, -1.0],
                    pos: [n as f32 - 0.75, -(n as f32), 0.5],
                    uv: [n as f32 * 0.001, -0.25],
                    tex_matrix_scale: [0.5, 2.0],
                })
                .collect(),
            bounds: [0.0; 6],
        },
    }
}

fn compare(a: &[TexturedMeshVertex], b: &[TexturedMeshVertex]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(a.normal.map(f32::to_bits), b.normal.map(f32::to_bits));
        assert_eq!(a.pos.map(f32::to_bits), b.pos.map(f32::to_bits));
        assert_eq!(a.uv.map(f32::to_bits), b.uv.map(f32::to_bits));
        assert_eq!(a.color.map(f32::to_bits), b.color.map(f32::to_bits));
        assert_eq!(
            a.tex_matrix_scale.map(f32::to_bits),
            b.tex_matrix_scale.map(f32::to_bits)
        );
    }
}

#[test]
fn note_field_geometry_matches_parent_vertex_bits_and_allocation_budget() {
    for count in [0, 1, 3, 128, 4096] {
        for mode in 0..8 {
            for (h, v) in [(false, false), (true, false), (false, true), (true, true)] {
                let slot = slot(count, mode, h, v);
                compare(
                    &baseline::build_model_geometry(&slot),
                    &build_model_geometry(&slot),
                );
                perf::assert_churn_budget(
                    usize::from(count != 0),
                    (count * std::mem::size_of::<TexturedMeshVertex>() + 16)
                        .next_multiple_of(std::mem::align_of::<usize>()),
                    || {
                        black_box(build_model_geometry(&slot));
                    },
                );
                if count != 0 {
                    perf::assert_reduced_churn(
                        || {
                            black_box(baseline::build_model_geometry(&slot));
                        },
                        || {
                            black_box(build_model_geometry(&slot));
                        },
                    );
                }
            }
        }
    }
    let slot = slot(0, 0, false, false);
    let empty = build_model_geometry(&slot);
    assert!(Arc::ptr_eq(&empty, &build_model_geometry(&slot)));
    perf::assert_no_churn(|| {
        black_box(build_model_geometry(&slot));
    });
}

#[test]
fn geometry_cache_still_retains_the_final_vertex_allocation() {
    let slot = slot(128, 7, true, true);
    let mut cache = ModelMeshCache::default();
    let (key, vertices) = cache.get_or_insert_with(&slot, || build_model_geometry(&slot));
    let (same_key, same_vertices) =
        cache.get_or_insert_with(&slot, || panic!("geometry was rebuilt"));
    assert_eq!(key, same_key);
    assert!(Arc::ptr_eq(&vertices, &same_vertices));
    compare(&baseline::build_model_geometry(&slot), &vertices);
}

#[test]
#[ignore = "manual paired release CPU, throughput and allocation benchmark"]
fn benchmark_note_field_geometry_preparation() {
    for (name, count, iterations) in [
        ("empty", 0, 1024),
        ("triangle", 3, 1024),
        ("mesh128", 128, 512),
        ("mesh4096", 4096, 64),
        ("mesh32768", 32768, 16),
    ] {
        let slot = slot(count, 7, true, true);
        compare(
            &baseline::build_model_geometry(&slot),
            &build_model_geometry(&slot),
        );
        let work = |label: &str, new: bool| {
            perf::measure_sampled(
                &format!("source_notefield_{name}_{label}"),
                iterations,
                1,
                || {
                    black_box(if new {
                        build_model_geometry(black_box(&slot))
                    } else {
                        baseline::build_model_geometry(black_box(&slot))
                    });
                },
            )
        };
        if std::env::var("DEADSYNC_PERF_ORDER").as_deref() == Ok("new-first") {
            work("new", true);
            work("old", false);
        } else {
            work("old", false);
            work("new", true);
        }
    }
}
