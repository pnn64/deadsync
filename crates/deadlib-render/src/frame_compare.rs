//! Exact structural comparisons for renderer parity tests.

use crate::{DrawOp, RenderFrame, SpriteInstanceRaw, SpriteRun, TexturedMeshVertices};
use bytemuck::Pod;
use glam::Mat4;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameMismatch {
    pub section: &'static str,
    pub index: usize,
    pub field: &'static str,
}

impl core::fmt::Display for FrameMismatch {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            formatter,
            "{}[{}].{} differs",
            self.section, self.index, self.field
        )
    }
}

type CompareResult = Result<(), FrameMismatch>;

/// Compares every backend-visible field in two final render frames.
///
/// Floating-point values are compared by bit pattern. Retained geometry storage
/// variants and shared allocation identity are part of the contract; vector
/// capacity is intentionally not.
pub fn compare_render_frames(expected: &RenderFrame, actual: &RenderFrame) -> CompareResult {
    compare_pod_value(
        "frame",
        0,
        "clear_color",
        &expected.clear_color,
        &actual.clear_color,
    )?;
    compare_mat_slices("camera", &expected.cameras, &actual.cameras)?;
    compare_pod_slices(
        "sprite_instance",
        &expected.sprite_instances,
        &actual.sprite_instances,
    )?;
    compare_pod_slices(
        "mesh_vertex",
        &expected.mesh_vertices,
        &actual.mesh_vertices,
    )?;
    compare_pod_slices(
        "tmesh_instance",
        &expected.tmesh_instances,
        &actual.tmesh_instances,
    )?;
    compare_count(
        "tmesh_geometry",
        expected.tmesh_geometries.len(),
        actual.tmesh_geometries.len(),
    )?;
    for (index, (expected, actual)) in expected
        .tmesh_geometries
        .iter()
        .zip(&actual.tmesh_geometries)
        .enumerate()
    {
        compare_value(
            "tmesh_geometry",
            index,
            "cache_key",
            expected.cache_key,
            actual.cache_key,
        )?;
        compare_tmesh_vertices(index, &expected.vertices, &actual.vertices)?;
    }
    compare_values("draw_op", &expected.ops, &actual.ops)
}

/// Compares backend-visible painter output while allowing sprite instances to
/// be gathered into different contiguous runs.
///
/// This is stricter than an image comparison: every sprite instance and mesh
/// byte is checked in painter order. Retained allocation identity is ignored
/// because it changes reuse behavior, not renderer output.
pub fn compare_render_frames_semantic(
    expected: &RenderFrame,
    actual: &RenderFrame,
) -> CompareResult {
    compare_pod_value(
        "frame",
        0,
        "clear_color",
        &expected.clear_color,
        &actual.clear_color,
    )?;
    compare_mat_slices("camera", &expected.cameras, &actual.cameras)?;
    compare_pod_slices(
        "mesh_vertex",
        &expected.mesh_vertices,
        &actual.mesh_vertices,
    )?;
    compare_pod_slices(
        "tmesh_instance",
        &expected.tmesh_instances,
        &actual.tmesh_instances,
    )?;
    compare_count(
        "tmesh_geometry",
        expected.tmesh_geometries.len(),
        actual.tmesh_geometries.len(),
    )?;
    for (index, (expected, actual)) in expected
        .tmesh_geometries
        .iter()
        .zip(&actual.tmesh_geometries)
        .enumerate()
    {
        compare_value(
            "tmesh_geometry",
            index,
            "cache_key",
            expected.cache_key,
            actual.cache_key,
        )?;
        compare_tmesh_vertex_bytes(index, &expected.vertices, &actual.vertices)?;
    }

    let mut expected = SemanticDraws::new(expected);
    let mut actual = SemanticDraws::new(actual);
    let mut index = 0usize;
    loop {
        match (expected.next(), actual.next()) {
            (None, None) => return Ok(()),
            (
                Some(SemanticDraw::Sprite(expected_run, expected_instance)),
                Some(SemanticDraw::Sprite(actual_run, actual_instance)),
            ) => {
                compare_value(
                    "draw_primitive",
                    index,
                    "sprite_state",
                    sprite_state(expected_run),
                    sprite_state(actual_run),
                )?;
                compare_pod_value(
                    "draw_primitive",
                    index,
                    "sprite_instance",
                    expected_instance,
                    actual_instance,
                )?;
            }
            (Some(SemanticDraw::Other(expected)), Some(SemanticDraw::Other(actual))) => {
                compare_value("draw_primitive", index, "value", expected, actual)?;
            }
            (Some(_), Some(_)) => return difference("draw_primitive", index, "kind"),
            _ => return difference("draw_primitive", index, "count"),
        }
        index += 1;
    }
}

#[derive(Clone, Copy)]
enum SemanticDraw<'a> {
    Sprite(SpriteRun, &'a SpriteInstanceRaw),
    Other(DrawOp),
}

struct SemanticDraws<'a> {
    frame: &'a RenderFrame,
    op: usize,
    sprite_offset: u32,
}

impl<'a> SemanticDraws<'a> {
    const fn new(frame: &'a RenderFrame) -> Self {
        Self {
            frame,
            op: 0,
            sprite_offset: 0,
        }
    }
}

impl<'a> Iterator for SemanticDraws<'a> {
    type Item = SemanticDraw<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let op = *self.frame.ops.get(self.op)?;
            match op {
                DrawOp::Sprite(run) if self.sprite_offset < run.instance_count => {
                    let instance = self
                        .frame
                        .sprite_instances
                        .get(run.instance_start.saturating_add(self.sprite_offset) as usize)
                        .expect("sprite draw range references a live instance");
                    self.sprite_offset += 1;
                    return Some(SemanticDraw::Sprite(run, instance));
                }
                DrawOp::Sprite(_) => {
                    self.op += 1;
                    self.sprite_offset = 0;
                }
                other => {
                    self.op += 1;
                    self.sprite_offset = 0;
                    return Some(SemanticDraw::Other(other));
                }
            }
        }
    }
}

#[inline(always)]
const fn sprite_state(run: SpriteRun) -> (crate::BlendMode, crate::TextureHandle, u8) {
    (run.blend, run.texture_handle, run.camera)
}

fn compare_tmesh_vertices(
    index: usize,
    expected: &TexturedMeshVertices,
    actual: &TexturedMeshVertices,
) -> CompareResult {
    match (expected, actual) {
        (TexturedMeshVertices::Shared(expected), TexturedMeshVertices::Shared(actual)) => {
            compare_arc_identity("tmesh_geometry", index, expected, actual)?;
        }
        (TexturedMeshVertices::Reusable(expected), TexturedMeshVertices::Reusable(actual)) => {
            compare_arc_identity("tmesh_geometry", index, expected, actual)?;
        }
        (TexturedMeshVertices::Transient(_), TexturedMeshVertices::Transient(_)) => {}
        _ => return difference("tmesh_geometry", index, "storage"),
    }
    compare_pod_slices_at("tmesh_geometry", index, expected.as_ref(), actual.as_ref())
}

fn compare_tmesh_vertex_bytes(
    index: usize,
    expected: &TexturedMeshVertices,
    actual: &TexturedMeshVertices,
) -> CompareResult {
    compare_pod_slices_at("tmesh_geometry", index, expected.as_ref(), actual.as_ref())
}

fn compare_arc_identity<T: ?Sized>(
    section: &'static str,
    index: usize,
    expected: &Arc<T>,
    actual: &Arc<T>,
) -> CompareResult {
    if Arc::ptr_eq(expected, actual) {
        Ok(())
    } else {
        difference(section, index, "identity")
    }
}

fn compare_mat_slices(section: &'static str, expected: &[Mat4], actual: &[Mat4]) -> CompareResult {
    compare_count(section, expected.len(), actual.len())?;
    for (index, (expected, actual)) in expected.iter().zip(actual.iter()).enumerate() {
        let expected = expected.to_cols_array().map(f32::to_bits);
        let actual = actual.to_cols_array().map(f32::to_bits);
        compare_value(section, index, "value", expected, actual)?;
    }
    Ok(())
}

fn compare_pod_slices<T: Pod>(
    section: &'static str,
    expected: &[T],
    actual: &[T],
) -> CompareResult {
    compare_count(section, expected.len(), actual.len())?;
    for (index, (expected, actual)) in expected.iter().zip(actual.iter()).enumerate() {
        compare_pod_value(section, index, "value", expected, actual)?;
    }
    Ok(())
}

fn compare_pod_slices_at<T: Pod>(
    section: &'static str,
    index: usize,
    expected: &[T],
    actual: &[T],
) -> CompareResult {
    if expected.len() != actual.len() {
        return difference(section, index, "count");
    }
    if bytemuck::cast_slice::<T, u8>(expected) == bytemuck::cast_slice::<T, u8>(actual) {
        Ok(())
    } else {
        difference(section, index, "bytes")
    }
}

fn compare_pod_value<T: Pod>(
    section: &'static str,
    index: usize,
    field: &'static str,
    expected: &T,
    actual: &T,
) -> CompareResult {
    if bytemuck::bytes_of(expected) == bytemuck::bytes_of(actual) {
        Ok(())
    } else {
        difference(section, index, field)
    }
}

fn compare_values<T: PartialEq>(
    section: &'static str,
    expected: &[T],
    actual: &[T],
) -> CompareResult {
    compare_count(section, expected.len(), actual.len())?;
    for (index, (expected, actual)) in expected.iter().zip(actual.iter()).enumerate() {
        if expected != actual {
            return difference(section, index, "value");
        }
    }
    Ok(())
}

fn compare_value<T: PartialEq>(
    section: &'static str,
    index: usize,
    field: &'static str,
    expected: T,
    actual: T,
) -> CompareResult {
    if expected == actual {
        Ok(())
    } else {
        difference(section, index, field)
    }
}

fn compare_count(section: &'static str, expected: usize, actual: usize) -> CompareResult {
    compare_value(section, 0, "count", expected, actual)
}

fn difference<T>(
    section: &'static str,
    index: usize,
    field: &'static str,
) -> Result<T, FrameMismatch> {
    Err(FrameMismatch {
        section,
        index,
        field,
    })
}
