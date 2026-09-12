// Frozen from 9e6ab0553 (0.5.1140): retain the previous implementation for parity and benchmarks.
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
