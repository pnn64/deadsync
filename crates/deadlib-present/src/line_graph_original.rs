// Graph builder frozen from 057aae6f496f3dbfc41aa51c36cc29a92aac9e40.
// Keep the iterator collection and original geometry together as the oracle.
use deadlib_render_core::MeshVertex;
use std::sync::Arc;

const LINE_MIN_LEN_SQ: f32 = 0.000_000_01_f32;

#[allow(
    clippy::map_flatten,
    reason = "preserve the frozen iterator implementation"
)]
pub(super) fn build_graph_line_mesh(
    points: &[[f32; 2]],
    thickness: f32,
    color: [f32; 4],
) -> Arc<[MeshVertex]> {
    const CAP_SIDES: usize = 4;

    if points.len() < 2 || !thickness.is_finite() || thickness <= 0.0 {
        return Arc::from([]);
    }

    let half = thickness * 0.5;
    let segment = |pair: &[[f32; 2]]| {
        let a = pair[0];
        let b = pair[1];
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = dx.hypot(dy);

        // GraphLine uses (dy, -dx) as its segment normal.
        let offset = [dy / len * half, -dx / len * half];
        let v0 = [a[0] + offset[0], a[1] + offset[1]];
        let v1 = [a[0] - offset[0], a[1] - offset[1]];
        let v2 = [b[0] - offset[0], b[1] - offset[1]];
        let v3 = [b[0] + offset[0], b[1] + offset[1]];
        [
            MeshVertex { pos: v0, color },
            MeshVertex { pos: v1, color },
            MeshVertex { pos: v2, color },
            MeshVertex { pos: v0, color },
            MeshVertex { pos: v2, color },
            MeshVertex { pos: v3, color },
        ]
    };

    // At small radii this diamond-shaped fan approximates a round cap.
    const CAP_DIRS: [[f32; 2]; CAP_SIDES + 1] =
        [[1.0, 0.0], [0.0, -1.0], [-1.0, 0.0], [0.0, 1.0], [1.0, 0.0]];
    let cap = |point: [f32; 2]| {
        let mut vertices = [MeshVertex::default(); CAP_SIDES * 3];
        for side in 0..CAP_SIDES {
            let a = CAP_DIRS[side];
            let b = CAP_DIRS[side + 1];
            let base = side * 3;
            vertices[base] = MeshVertex { pos: point, color };
            vertices[base + 1] = MeshVertex {
                pos: [a[0].mul_add(half, point[0]), a[1].mul_add(half, point[1])],
                color,
            };
            vertices[base + 2] = MeshVertex {
                pos: [b[0].mul_add(half, point[0]), b[1].mul_add(half, point[1])],
                color,
            };
        }
        vertices
    };

    // Reject coincident points instead of normalizing a zero-length segment.
    if points.windows(2).any(|pair| {
        let dx = pair[1][0] - pair[0][0];
        let dy = pair[1][1] - pair[0][1];
        dx.mul_add(dx, dy * dy) <= LINE_MIN_LEN_SQ
    }) {
        return Arc::from([]);
    }

    // Flattened fixed arrays retain an exact trusted length, so Arc collection
    // allocates the immutable vertex payload exactly once.
    points
        .windows(2)
        .map(segment)
        .flatten()
        .chain(points.iter().copied().map(cap).flatten())
        .collect()
}
