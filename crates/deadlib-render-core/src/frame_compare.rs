//! Exact structural comparisons for renderer parity tests.

use crate::{
    DrawOp, RenderFrame, RenderTargetFrame, SpriteInstanceRaw, SpriteRun, TexturedMeshVertices,
};
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
    compare_render_targets(
        &expected.render_targets,
        &actual.render_targets,
        compare_render_pass,
    )?;
    compare_render_pass(expected, actual)
}

fn compare_render_targets(
    expected: &[RenderTargetFrame],
    actual: &[RenderTargetFrame],
    compare_pass: impl Fn(&RenderTargetFrame, &RenderTargetFrame) -> CompareResult,
) -> CompareResult {
    compare_count("render_target", expected.len(), actual.len())?;
    for (index, (expected, actual)) in expected.iter().zip(actual).enumerate() {
        compare_value(
            "render_target",
            index,
            "texture_handle",
            expected.texture_handle,
            actual.texture_handle,
        )?;
        compare_value(
            "render_target",
            index,
            "size",
            (expected.width, expected.height),
            (actual.width, actual.height),
        )?;
        compare_value(
            "render_target",
            index,
            "alpha",
            expected.alpha,
            actual.alpha,
        )?;
        compare_value(
            "render_target",
            index,
            "depth",
            expected.depth,
            actual.depth,
        )?;
        compare_value(
            "render_target",
            index,
            "preserve",
            expected.preserve,
            actual.preserve,
        )?;
        compare_pass(expected, actual)?;
    }
    Ok(())
}

trait RenderPass {
    fn cameras(&self) -> &[Mat4];
    fn sprite_instances(&self) -> &[SpriteInstanceRaw];
    fn mesh_vertices(&self) -> &[crate::MeshVertex];
    fn tmesh_instances(&self) -> &[crate::TexturedMeshInstanceRaw];
    fn tmesh_geometries(&self) -> &[crate::TexturedMeshGeometry];
    fn ops(&self) -> &[DrawOp];
}

macro_rules! impl_render_pass {
    ($type:ty) => {
        impl RenderPass for $type {
            fn cameras(&self) -> &[Mat4] {
                &self.cameras
            }
            fn sprite_instances(&self) -> &[SpriteInstanceRaw] {
                &self.sprite_instances
            }
            fn mesh_vertices(&self) -> &[crate::MeshVertex] {
                &self.mesh_vertices
            }
            fn tmesh_instances(&self) -> &[crate::TexturedMeshInstanceRaw] {
                &self.tmesh_instances
            }
            fn tmesh_geometries(&self) -> &[crate::TexturedMeshGeometry] {
                &self.tmesh_geometries
            }
            fn ops(&self) -> &[DrawOp] {
                &self.ops
            }
        }
    };
}

impl_render_pass!(RenderFrame);
impl_render_pass!(RenderTargetFrame);

fn compare_render_pass(expected: &impl RenderPass, actual: &impl RenderPass) -> CompareResult {
    compare_mat_slices("camera", expected.cameras(), actual.cameras())?;
    compare_pod_slices(
        "sprite_instance",
        expected.sprite_instances(),
        actual.sprite_instances(),
    )?;
    compare_pod_slices(
        "mesh_vertex",
        expected.mesh_vertices(),
        actual.mesh_vertices(),
    )?;
    compare_pod_slices(
        "tmesh_instance",
        expected.tmesh_instances(),
        actual.tmesh_instances(),
    )?;
    compare_count(
        "tmesh_geometry",
        expected.tmesh_geometries().len(),
        actual.tmesh_geometries().len(),
    )?;
    for (index, (expected, actual)) in expected
        .tmesh_geometries()
        .iter()
        .zip(actual.tmesh_geometries())
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
    compare_values("draw_op", expected.ops(), actual.ops())
}

/// Compares backend-visible painter output while allowing compatible draws to
/// be gathered into different contiguous runs, including offscreen passes.
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
    compare_render_targets(
        &expected.render_targets,
        &actual.render_targets,
        compare_render_pass_semantic,
    )?;
    compare_render_pass_semantic(expected, actual)
}

fn compare_render_pass_semantic(
    expected: &impl RenderPass,
    actual: &impl RenderPass,
) -> CompareResult {
    compare_mat_slices("camera", expected.cameras(), actual.cameras())?;

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
            (Some(SemanticDraw::Mesh(a, av)), Some(SemanticDraw::Mesh(b, bv))) => {
                compare_value(
                    "draw_primitive",
                    index,
                    "mesh_state",
                    (a.blend, a.camera),
                    (b.blend, b.camera),
                )?;
                compare_pod_slices_at("mesh_triangle", index, av, bv)?;
            }
            (
                Some(SemanticDraw::TexturedMesh(a, ai, ag)),
                Some(SemanticDraw::TexturedMesh(b, bi, bg)),
            ) => {
                compare_value(
                    "draw_primitive",
                    index,
                    "tmesh_state",
                    (a.blend, a.texture_handle, a.camera, a.depth_test),
                    (b.blend, b.texture_handle, b.camera, b.depth_test),
                )?;
                compare_pod_value("tmesh_instance", index, "value", ai, bi)?;
                compare_value(
                    "tmesh_geometry",
                    index,
                    "cache_key",
                    ag.cache_key,
                    bg.cache_key,
                )?;
                compare_tmesh_vertex_bytes(index, &ag.vertices, &bg.vertices)?;
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
    Mesh(crate::MeshRun, &'a [crate::MeshVertex]),
    TexturedMesh(
        crate::TexturedMeshRun,
        &'a crate::TexturedMeshInstanceRaw,
        &'a crate::TexturedMeshGeometry,
    ),
}

struct SemanticDraws<'a, P> {
    frame: &'a P,
    op: usize,
    offset: u32,
}

impl<'a, P: RenderPass> SemanticDraws<'a, P> {
    const fn new(frame: &'a P) -> Self {
        Self {
            frame,
            op: 0,
            offset: 0,
        }
    }
}

impl<'a, P: RenderPass> Iterator for SemanticDraws<'a, P> {
    type Item = SemanticDraw<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let op = *self.frame.ops().get(self.op)?;
            match op {
                DrawOp::Sprite(run) if self.offset < run.instance_count => {
                    let instance = self
                        .frame
                        .sprite_instances()
                        .get(run.instance_start.saturating_add(self.offset) as usize)
                        .expect("sprite draw range references a live instance");
                    self.offset += 1;
                    return Some(SemanticDraw::Sprite(run, instance));
                }
                DrawOp::Mesh(run) if self.offset < run.vertex_count / 3 => {
                    let start = (run.vertex_start + self.offset * 3) as usize;
                    let triangle = &self.frame.mesh_vertices()[start..start + 3];
                    self.offset += 1;
                    return Some(SemanticDraw::Mesh(run, triangle));
                }
                DrawOp::TexturedMesh(run) if self.offset < run.instance_count => {
                    let instance =
                        &self.frame.tmesh_instances()[(run.instance_start + self.offset) as usize];
                    let geometry = &self.frame.tmesh_geometries()[run.geometry as usize];
                    self.offset += 1;
                    return Some(SemanticDraw::TexturedMesh(run, instance, geometry));
                }
                _ => {
                    self.op += 1;
                    self.offset = 0;
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

const fn difference<T>(
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
