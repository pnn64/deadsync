//! Exact structural comparisons for renderer parity tests.
//!
//! This module compares the existing renderer contracts directly. It does not
//! create a normalized frame or add work to production builds.

use crate::{
    MeshVertices, ObjectType, RenderList, RenderObject, TexturedMeshVertices,
    draw_prep::DrawScratch,
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

/// Compares every renderer-visible field in two render lists.
///
/// Floating-point values are compared by bit pattern. Geometry storage variants
/// and shared allocation identity are part of the contract; vector capacity is
/// intentionally not.
pub fn compare_render_lists(expected: &RenderList, actual: &RenderList) -> CompareResult {
    compare_pod_value(
        "render_list",
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
    compare_count("object", expected.objects.len(), actual.objects.len())?;
    for (index, (expected, actual)) in expected
        .objects
        .iter()
        .zip(actual.objects.iter())
        .enumerate()
    {
        compare_object(index, expected, actual)?;
    }
    compare_values("batch", &expected.batches, &actual.batches)
}

/// Compares the exact upload data and draw operations produced by draw prep.
pub fn compare_draw_scratch(expected: &DrawScratch, actual: &DrawScratch) -> CompareResult {
    compare_pod_slices(
        "draw_mesh_vertex",
        &expected.mesh_vertices,
        &actual.mesh_vertices,
    )?;
    compare_pod_slices(
        "draw_tmesh_vertex",
        &expected.tmesh_vertices,
        &actual.tmesh_vertices,
    )?;
    compare_pod_slices(
        "draw_tmesh_instance",
        &expected.tmesh_instances,
        &actual.tmesh_instances,
    )?;
    compare_values("draw_op", &expected.ops, &actual.ops)
}

fn compare_object(index: usize, expected: &RenderObject, actual: &RenderObject) -> CompareResult {
    compare_value(
        "object",
        index,
        "texture_handle",
        expected.texture_handle,
        actual.texture_handle,
    )?;
    compare_value("object", index, "blend", expected.blend, actual.blend)?;
    compare_value("object", index, "z", expected.z, actual.z)?;
    compare_value("object", index, "order", expected.order, actual.order)?;
    compare_value("object", index, "camera", expected.camera, actual.camera)?;

    match (&expected.object_type, &actual.object_type) {
        (ObjectType::Sprite(expected), ObjectType::Sprite(actual)) => {
            compare_value("object", index, "sprite_instance", *expected, *actual)
        }
        (
            ObjectType::Mesh {
                transform: expected_transform,
                tint: expected_tint,
                vertices: expected_vertices,
            },
            ObjectType::Mesh {
                transform: actual_transform,
                tint: actual_tint,
                vertices: actual_vertices,
            },
        ) => {
            compare_mat(
                "object",
                index,
                "mesh_transform",
                expected_transform,
                actual_transform,
            )?;
            compare_pod_value("object", index, "mesh_tint", expected_tint, actual_tint)?;
            compare_mesh_vertices(index, expected_vertices, actual_vertices)
        }
        (
            ObjectType::TexturedMesh {
                instance: expected_instance,
                vertices: expected_vertices,
                geom_cache_key: expected_key,
                depth_test: expected_depth,
            },
            ObjectType::TexturedMesh {
                instance: actual_instance,
                vertices: actual_vertices,
                geom_cache_key: actual_key,
                depth_test: actual_depth,
            },
        ) => {
            compare_pod_value(
                "object",
                index,
                "tmesh_instance",
                expected_instance,
                actual_instance,
            )?;
            compare_value(
                "object",
                index,
                "geom_cache_key",
                *expected_key,
                *actual_key,
            )?;
            compare_value(
                "object",
                index,
                "depth_test",
                *expected_depth,
                *actual_depth,
            )?;
            compare_tmesh_vertices(index, expected_vertices, actual_vertices)
        }
        _ => difference("object", index, "kind"),
    }
}

fn compare_mesh_vertices(
    index: usize,
    expected: &MeshVertices,
    actual: &MeshVertices,
) -> CompareResult {
    match (expected, actual) {
        (MeshVertices::Shared(expected), MeshVertices::Shared(actual)) => {
            compare_arc_identity("mesh_geometry", index, expected, actual)?;
        }
        (MeshVertices::Reusable(expected), MeshVertices::Reusable(actual)) => {
            compare_arc_identity("mesh_geometry", index, expected, actual)?;
        }
        _ => return difference("mesh_geometry", index, "storage"),
    }
    compare_pod_slices_at("mesh_geometry", index, expected.as_ref(), actual.as_ref())
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
        compare_mat(section, index, "value", expected, actual)?;
    }
    Ok(())
}

fn compare_mat(
    section: &'static str,
    index: usize,
    field: &'static str,
    expected: &Mat4,
    actual: &Mat4,
) -> CompareResult {
    let expected = expected.to_cols_array().map(f32::to_bits);
    let actual = actual.to_cols_array().map(f32::to_bits);
    compare_value(section, index, field, expected, actual)
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
