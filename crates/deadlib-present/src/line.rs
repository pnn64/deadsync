//! Line meshes from caller-supplied points, widths, and colors.
//!
//! Clipped updates require points ordered by nondecreasing x. Reusable updates
//! retain uniquely owned vertex capacity; shared frames remain immutable.

use crate::color::lerp;
use deadlib_render_core::MeshVertex;
use std::sync::Arc;

const LINE_MIN_LEN_SQ: f32 = 0.000_000_01_f32;
const LINE_SEGMENT_VERTS: usize = 18;

#[derive(Clone, Copy, Debug)]
struct LineWindow {
    left: f32,
    right: f32,
    start: usize,
    end: usize,
    point_count: usize,
    clip_left: bool,
}

#[inline(always)]
fn interp_line_point(a: [f32; 2], b: [f32; 2], x: f32) -> [f32; 2] {
    let dx = (b[0] - a[0]).max(0.000_001_f32);
    let t = ((x - a[0]) / dx).clamp(0.0, 1.0);
    [x, lerp(a[1], b[1], t)]
}

#[inline(always)]
fn line_window_point(points: &[[f32; 2]], window: LineWindow, index: usize) -> [f32; 2] {
    if window.clip_left && index == 0 {
        return interp_line_point(points[window.start - 1], points[window.start], window.left);
    }
    let source_index = index - usize::from(window.clip_left) + window.start;
    if source_index < window.end {
        return points[source_index];
    }
    interp_line_point(points[window.end - 1], points[window.end], window.right)
}

#[inline(always)]
fn line_normal(a: [f32; 2], b: [f32; 2], half: f32) -> [f32; 2] {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = dx.hypot(dy);
    if len <= f32::EPSILON {
        return [0.0, 0.0];
    }
    [-dy / len * half, dx / len * half]
}

#[inline(always)]
fn line_join(prev: [f32; 2], current: [f32; 2], next: [f32; 2], half: f32) -> [f32; 2] {
    let prev_normal = line_normal(prev, current, 1.0);
    let next_normal = line_normal(current, next, 1.0);
    let miter = [
        prev_normal[0] + next_normal[0],
        prev_normal[1] + next_normal[1],
    ];
    let miter_len = miter[0].hypot(miter[1]);
    if miter_len <= f32::EPSILON {
        return [prev_normal[0] * half, prev_normal[1] * half];
    }
    let miter = [miter[0] / miter_len, miter[1] / miter_len];
    let denom = miter[1].mul_add(prev_normal[1], miter[0] * prev_normal[0]);
    if denom.abs() <= 0.1 {
        return [next_normal[0] * half, next_normal[1] * half];
    }
    let scale = (half / denom).clamp(-half * 4.0, half * 4.0);
    [miter[0] * scale, miter[1] * scale]
}

#[inline(always)]
fn line_offset(points: &[[f32; 2]], window: LineWindow, index: usize, half: f32) -> [f32; 2] {
    let current = line_window_point(points, window, index);
    if index == 0 {
        return line_normal(current, line_window_point(points, window, 1), half);
    }
    if index + 1 == window.point_count {
        return line_normal(line_window_point(points, window, index - 1), current, half);
    }
    line_join(
        line_window_point(points, window, index - 1),
        current,
        line_window_point(points, window, index + 1),
        half,
    )
}

#[inline(always)]
fn write_line_segment(
    dst: &mut [MeshVertex],
    written: usize,
    a: [f32; 2],
    b: [f32; 2],
    a_offset: [f32; 2],
    b_offset: [f32; 2],
    a_outer: [f32; 2],
    b_outer: [f32; 2],
    color: [f32; 4],
) -> usize {
    let l0 = [a[0] + a_offset[0], a[1] + a_offset[1]];
    let r0 = [a[0] - a_offset[0], a[1] - a_offset[1]];
    let l1 = [b[0] + b_offset[0], b[1] + b_offset[1]];
    let r1 = [b[0] - b_offset[0], b[1] - b_offset[1]];
    let ol0 = [a[0] + a_outer[0], a[1] + a_outer[1]];
    let or0 = [a[0] - a_outer[0], a[1] - a_outer[1]];
    let ol1 = [b[0] + b_outer[0], b[1] + b_outer[1]];
    let or1 = [b[0] - b_outer[0], b[1] - b_outer[1]];
    let edge_color = [color[0], color[1], color[2], 0.0];

    let verts = [
        MeshVertex { pos: l0, color },
        MeshVertex { pos: r0, color },
        MeshVertex { pos: l1, color },
        MeshVertex { pos: r0, color },
        MeshVertex { pos: r1, color },
        MeshVertex { pos: l1, color },
        // Transparent fringes provide antialiased coverage for triangle meshes.
        MeshVertex {
            pos: ol0,
            color: edge_color,
        },
        MeshVertex { pos: l0, color },
        MeshVertex {
            pos: ol1,
            color: edge_color,
        },
        MeshVertex { pos: l0, color },
        MeshVertex { pos: l1, color },
        MeshVertex {
            pos: ol1,
            color: edge_color,
        },
        MeshVertex { pos: r0, color },
        MeshVertex {
            pos: or0,
            color: edge_color,
        },
        MeshVertex { pos: r1, color },
        MeshVertex {
            pos: or0,
            color: edge_color,
        },
        MeshVertex {
            pos: or1,
            color: edge_color,
        },
        MeshVertex { pos: r1, color },
    ];
    dst[written..written + verts.len()].copy_from_slice(&verts);
    written + verts.len()
}

#[inline(always)]
fn line_outer_offset(offset: [f32; 2], scale: f32) -> [f32; 2] {
    [offset[0] * scale, offset[1] * scale]
}

#[inline(always)]
fn fill_line_vertices(
    dst: &mut [MeshVertex],
    points: &[[f32; 2]],
    window: LineWindow,
    half: f32,
    feather: f32,
    color: [f32; 4],
) -> usize {
    let inner_half = feather.mul_add(-0.5, half).max(f32::EPSILON);
    let outer_scale = feather.mul_add(0.5, half) / inner_half;
    let mut written = 0usize;
    let mut a_offset = line_offset(points, window, 0, inner_half);
    let mut a_outer = line_outer_offset(a_offset, outer_scale);
    for index in 0..window.point_count - 1 {
        let mut a = line_window_point(points, window, index);
        let mut b = line_window_point(points, window, index + 1);
        let b_offset = line_offset(points, window, index + 1, inner_half);
        let b_outer = line_outer_offset(b_offset, outer_scale);
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        if dx.mul_add(dx, dy * dy) <= LINE_MIN_LEN_SQ {
            a_offset = b_offset;
            a_outer = b_outer;
            continue;
        }
        a[0] -= window.left;
        b[0] -= window.left;
        written = write_line_segment(
            dst, written, a, b, a_offset, b_offset, a_outer, b_outer, color,
        );
        a_offset = b_offset;
        a_outer = b_outer;
    }
    written
}

#[inline(always)]
fn line_window(
    points: &[[f32; 2]],
    offset: f32,
    width: f32,
    thickness: f32,
) -> Option<(LineWindow, f32)> {
    if points.len() < 2 || width <= 0.0_f32 || thickness <= 0.0_f32 {
        return None;
    }

    let left = offset.max(0.0);
    let right = left + width;
    let start = points.partition_point(|p| p[0] < left);
    let end = points.partition_point(|p| p[0] <= right);
    let clip_left = start > 0 && start < points.len() && points[start][0] > left;
    let clip_right = end > 0 && end < points.len() && points[end - 1][0] < right;
    let point_count = end.saturating_sub(start) + usize::from(clip_left) + usize::from(clip_right);
    if point_count < 2 {
        return None;
    }

    let window = LineWindow {
        left,
        right,
        start,
        end,
        point_count,
        clip_left,
    };
    Some((window, thickness * 0.5_f32))
}

#[inline(always)]
fn line_mesh_window(
    points: &[[f32; 2]],
    offset: f32,
    width: f32,
    thickness: f32,
) -> Option<(LineWindow, usize, f32)> {
    let (window, half) = line_window(points, offset, width, thickness)?;
    let segment_count = (0..window.point_count - 1)
        .filter(|&index| {
            let a = line_window_point(points, window, index);
            let b = line_window_point(points, window, index + 1);
            let dx = b[0] - a[0];
            let dy = b[1] - a[1];
            dx.mul_add(dx, dy * dy) > LINE_MIN_LEN_SQ
        })
        .count();
    (segment_count != 0).then_some((window, segment_count * LINE_SEGMENT_VERTS, half))
}

pub fn update_line_mesh(
    mesh: &mut Option<Arc<[MeshVertex]>>,
    points: &[[f32; 2]],
    offset: f32,
    width: f32,
    thickness: f32,
    feather: f32,
    color: [f32; 4],
) {
    let Some((window, len, half)) = line_mesh_window(points, offset, width, thickness) else {
        *mesh = None;
        return;
    };
    let feather = feather.max(0.0);
    if let Some(existing) = mesh.as_mut().and_then(Arc::get_mut)
        && existing.len() == len
    {
        let written = fill_line_vertices(existing, points, window, half, feather, color);
        debug_assert_eq!(written, len);
        return;
    }

    let mut verts = vec![MeshVertex::default(); len];
    let written = fill_line_vertices(&mut verts, points, window, half, feather, color);
    debug_assert_eq!(written, len);
    *mesh = Some(Arc::from(verts.into_boxed_slice()));
}

/// Update a clipped polyline while retaining its growable vertex allocation.
///
/// The buffer is mutated only while uniquely owned. If a renderer still holds
/// the preceding frame, a replacement is allocated and that frame remains
/// immutable.
/// # Panics
///
/// Panics if an internal state invariant is violated.
pub fn update_line_mesh_reusable(
    mesh: &mut Option<Arc<Vec<MeshVertex>>>,
    points: &[[f32; 2]],
    offset: f32,
    width: f32,
    thickness: f32,
    feather: f32,
    color: [f32; 4],
) {
    let Some((window, half)) = line_window(points, offset, width, thickness) else {
        *mesh = None;
        return;
    };
    let feather = feather.max(0.0);
    let max_len = (window.point_count - 1) * LINE_SEGMENT_VERTS;

    if mesh.as_mut().and_then(Arc::get_mut).is_none() {
        *mesh = Some(Arc::new(Vec::with_capacity(max_len)));
    }
    let vertices = mesh
        .as_mut()
        .and_then(Arc::get_mut)
        .expect("replacement line mesh must be uniquely owned");
    vertices.resize(max_len, MeshVertex::default());
    let written = fill_line_vertices(vertices, points, window, half, feather, color);
    vertices.truncate(written);
    if written == 0 {
        *mesh = None;
    }
}

/// Build connected segment quads with a four-sided radius cap at every sample.
#[must_use]
pub fn build_graph_line_mesh(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_mesh_matches(actual: &[MeshVertex], expected: &[MeshVertex]) {
        assert_eq!(actual.len(), expected.len());
        for (index, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
            assert_eq!(actual.pos, expected.pos, "pos mismatch at {index}");
            assert_eq!(actual.color, expected.color, "color mismatch at {index}");
        }
    }

    #[test]
    fn update_line_mesh_matches_line_strip_joins() {
        let mut mesh = None;
        let points = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]];

        update_line_mesh(&mut mesh, &points, 0.0, 32.0, 4.0, 0.5, [1.0; 4]);

        let mesh = mesh.expect("life mesh");
        assert_eq!(mesh.len(), 2 * LINE_SEGMENT_VERTS);
        let has_pos = |expected: [f32; 2]| {
            mesh.iter().any(|vertex| {
                (vertex.pos[0] - expected[0]).abs() < 0.000_1
                    && (vertex.pos[1] - expected[1]).abs() < 0.000_1
            })
        };
        assert!(has_pos([8.25, 1.75]));
        assert!(has_pos([11.75, -1.75]));
        assert!(!mesh.iter().any(|vertex| points.contains(&vertex.pos)));
    }

    #[test]
    fn graph_line_mesh_matches_graph_display_quads_and_caps() {
        let points = [[4.0, 8.0], [14.0, 8.0], [14.0, 18.0]];
        let color = [1.0, 1.0, 1.0, 1.0];

        let mesh = build_graph_line_mesh(&points, 2.0, color);

        assert_eq!(mesh.len(), 2 * 6 + 3 * 12);
        assert!(mesh.iter().all(|vertex| vertex.color == color));
        assert!(mesh.iter().any(|vertex| vertex.pos == [3.0, 8.0]));
        assert!(mesh.iter().any(|vertex| vertex.pos == [15.0, 18.0]));
        assert!(mesh.iter().any(|vertex| vertex.pos == [4.0, 7.0]));
    }

    #[test]
    fn update_line_mesh_clips_fractional_window_edges() {
        let mut mesh = None;
        let points = [[0.0, 8.0], [12.0, 8.0], [24.0, 8.0]];

        update_line_mesh(&mut mesh, &points, 0.25, 10.0, 2.0, 0.5, [1.0; 4]);

        let mesh = mesh.expect("life mesh");
        assert_eq!(mesh.len(), LINE_SEGMENT_VERTS);
        assert!(
            mesh.iter()
                .all(|vertex| matches!(vertex.pos[0], 0.0 | 10.0))
        );
        assert!(
            mesh.iter()
                .all(|vertex| (6.75..=9.25).contains(&vertex.pos[1]))
        );
        assert!(mesh.iter().any(|vertex| vertex.color[3] == 0.0));
        assert!(mesh.iter().any(|vertex| vertex.color[3] == 1.0));
    }

    #[test]
    fn update_line_mesh_reuses_existing_buffer_when_vertex_count_matches() {
        let mut mesh = None;
        let points = [[0.0, 8.0], [12.0, 8.0], [24.0, 20.0]];

        update_line_mesh(
            &mut mesh,
            &points,
            0.0,
            32.0,
            2.0,
            0.5,
            [1.0, 1.0, 1.0, 1.0],
        );
        let expected = mesh.as_ref().expect("life mesh").to_vec();
        let first_ptr = mesh.as_ref().expect("life mesh").as_ptr();

        update_line_mesh(
            &mut mesh,
            &points,
            0.0,
            32.0,
            2.0,
            0.5,
            [1.0, 1.0, 1.0, 1.0],
        );
        let second_ptr = mesh.as_ref().expect("life mesh").as_ptr();

        assert_eq!(first_ptr, second_ptr);
        assert_mesh_matches(mesh.as_ref().expect("life mesh"), &expected);
    }

    #[test]
    fn reusable_line_mesh_matches_shared_mesh_and_reuses_changed_lengths() {
        let points = [[0.0, 8.0], [8.0, 12.0], [16.0, 6.0], [24.0, 20.0]];
        let mut shared = None;
        let mut reusable = None;

        update_line_mesh(
            &mut shared,
            &points,
            0.0,
            32.0,
            2.0,
            0.5,
            [0.5, 0.75, 1.0, 1.0],
        );
        update_line_mesh_reusable(
            &mut reusable,
            &points,
            0.0,
            32.0,
            2.0,
            0.5,
            [0.5, 0.75, 1.0, 1.0],
        );
        assert_mesh_matches(
            reusable.as_ref().expect("reusable life mesh"),
            shared.as_ref().expect("shared life mesh"),
        );
        let allocation = reusable.as_ref().expect("reusable life mesh").as_ptr();

        update_line_mesh(
            &mut shared,
            &points[..3],
            0.0,
            32.0,
            2.0,
            0.5,
            [0.5, 0.75, 1.0, 1.0],
        );
        update_line_mesh_reusable(
            &mut reusable,
            &points[..3],
            0.0,
            32.0,
            2.0,
            0.5,
            [0.5, 0.75, 1.0, 1.0],
        );

        assert_eq!(
            reusable.as_ref().expect("reusable life mesh").as_ptr(),
            allocation
        );
        assert_mesh_matches(
            reusable.as_ref().expect("reusable life mesh"),
            shared.as_ref().expect("shared life mesh"),
        );
    }

    #[test]
    fn reusable_line_mesh_keeps_a_busy_previous_frame_immutable() {
        let points = [[0.0, 8.0], [12.0, 8.0], [24.0, 20.0]];
        let mut mesh = None;
        update_line_mesh_reusable(&mut mesh, &points, 0.0, 32.0, 2.0, 0.5, [1.0; 4]);
        let previous = Arc::clone(mesh.as_ref().expect("life mesh"));
        let previous_vertices = previous.as_slice().to_vec();

        update_line_mesh_reusable(&mut mesh, &points[..2], 0.0, 32.0, 2.0, 0.5, [0.5; 4]);

        assert!(!Arc::ptr_eq(
            mesh.as_ref().expect("replacement mesh"),
            &previous
        ));
        assert_mesh_matches(previous.as_slice(), &previous_vertices);
    }
}
