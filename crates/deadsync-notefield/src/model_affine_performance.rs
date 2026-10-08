use super::*;
use std::hint::black_box;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

// Starting-main implementation, retained only for differential checks/benchmarks.
#[inline(always)]
fn original_affine(
    model: &ModelMesh,
    size: [f32; 2],
    rotation_deg: f32,
    draw: ModelDrawState,
) -> Matrix4 {
    let model_size = model.size();
    let model_h = model_size[1];
    let scale = if model_h > f32::EPSILON && size[1] > f32::EPSILON {
        size[1] / model_h
    } else {
        1.0
    };
    let local_scale = Vector3::new(
        scale * draw.zoom[0].max(0.0),
        scale * draw.zoom[1].max(0.0),
        scale * draw.zoom[2].max(0.0),
    );
    let align_y = (0.5 - draw.vert_align) * size[1];
    Matrix4::from_translation(Vector3::new(draw.pos[0], draw.pos[1], draw.pos[2]))
        * sm_rotation_xyz(draw.rot[0], draw.rot[1], draw.rot[2] + rotation_deg)
        * Matrix4::from_translation(Vector3::new(0.0, align_y, 0.0))
        * Matrix4::from_scale(local_scale)
}

fn model() -> ModelMesh {
    ModelMesh {
        vertices: Arc::from([]),
        bounds: [-32.0, -32.0, -4.0, 32.0, 32.0, 4.0],
    }
}

#[test]
fn affine_matches_original_for_authored_transforms_and_nonfinite_inputs() {
    let model = model();
    for value in [
        -f32::MAX,
        -1000.0,
        -1.0,
        -0.0,
        0.0,
        0.25,
        1.0,
        1000.0,
        f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ] {
        for field in 0..12 {
            let mut draw = ModelDrawState::default();
            draw.rot = [13.0, 27.0, 31.0];
            draw.pos = [1.0, 2.0, 3.0];
            draw.zoom = [0.8, 1.2, 0.6];
            let mut size = [48.0, 56.0];
            let mut rotation = 34.0;
            match field {
                0..=2 => draw.rot[field] = value,
                3..=5 => draw.pos[field - 3] = value,
                6..=8 => draw.zoom[field - 6] = value,
                9 => draw.vert_align = value,
                10 => size[1] = value,
                _ => rotation = value,
            }
            let expected = original_affine(&model, size, rotation, draw);
            let actual = model_affine_transform(&model, size, rotation, draw);
            for (actual, expected) in actual
                .to_cols_array()
                .into_iter()
                .zip(expected.to_cols_array())
            {
                if expected.is_nan() {
                    assert!(actual.is_nan(), "field {field}, value {value}");
                } else if expected.is_infinite() {
                    assert_eq!(actual, expected);
                } else {
                    assert_eq!(actual, expected, "field {field}, value {value}");
                }
            }
        }
    }
}

#[test]
#[ignore = "paired release throughput benchmark"]
fn benchmark_model_affine() {
    let model = model();
    let mut draw = ModelDrawState::default();
    draw.rot = [13.0, 27.0, 31.0];
    draw.pos = [1.0, 2.0, 3.0];
    draw.zoom = [0.8, 1.2, 0.6];
    for (label, draw, rotation) in [
        ("model affine (default)", ModelDrawState::default(), 0.0),
        ("model affine (animated)", draw, 34.0),
    ] {
        paired_bench::compare(label, 200_000, |current| {
            let (model, size, rotation, draw) = black_box((&model, [48.0, 56.0], rotation, draw));
            black_box(if current {
                model_affine_transform(model, size, rotation, draw)
            } else {
                original_affine(model, size, rotation, draw)
            });
        });
    }
}
