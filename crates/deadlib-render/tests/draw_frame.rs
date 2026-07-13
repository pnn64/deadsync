use deadlib_render::{
    BlendMode, MeshVertex, ObjectType, RenderList, RenderObject, RetainedTMeshGeometry,
    SpriteInstanceRaw, TMeshGeometryId, TexturedMeshInstanceRaw, TexturedMeshVertex,
    TexturedMeshVertices,
    draw_prep::{
        DrawFrame, DrawOp, DrawScratch, FrameCapacity, FramePrepareStats, MeshRun, PrepareStats,
        SpriteRun, TMeshCacheResult, TexturedMeshRun, TexturedMeshSource, prepare,
        prepare_render_list,
    },
};
use glam::{Mat4, Vec3};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

struct CountingAlloc;

thread_local! {
    static COUNT_ALLOCS: Cell<bool> = const { Cell::new(false) };
}

static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static REALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static ALLOC_TEST_LOCK: Mutex<()> = Mutex::new(());

// SAFETY: every operation delegates to `System` with the original pointer and
// layout. The additional state is const-initialized TLS plus relaxed atomics;
// neither invokes this allocator recursively.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged to the system allocator.
        let ptr = unsafe { System.alloc(layout) };
        COUNT_ALLOCS.with(|enabled| {
            if enabled.get() {
                ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            }
        });
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged to the system allocator.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        COUNT_ALLOCS.with(|enabled| {
            if enabled.get() {
                ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            }
        });
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` and `layout` came from the matching system allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the original allocation and layout came from `System`, and
        // `new_size` is forwarded unchanged.
        let ptr = unsafe { System.realloc(ptr, layout, new_size) };
        COUNT_ALLOCS.with(|enabled| {
            if enabled.get() {
                REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            }
        });
        ptr
    }
}

#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

fn count_allocs(run: impl FnOnce()) -> (usize, usize) {
    let _guard = ALLOC_TEST_LOCK
        .lock()
        .expect("allocation-test lock must not be poisoned");
    COUNT_ALLOCS.with(|enabled| enabled.set(false));
    ALLOC_CALLS.store(0, Ordering::Relaxed);
    REALLOC_CALLS.store(0, Ordering::Relaxed);
    COUNT_ALLOCS.with(|enabled| enabled.set(true));
    run();
    COUNT_ALLOCS.with(|enabled| enabled.set(false));
    (
        ALLOC_CALLS.load(Ordering::Relaxed),
        REALLOC_CALLS.load(Ordering::Relaxed),
    )
}

fn sprite(x: f32) -> SpriteInstanceRaw {
    SpriteInstanceRaw {
        center: [x, 2.0, 0.0, 1.0],
        size: [16.0, 24.0],
        rot_sin_cos: [0.0, 1.0],
        tint: [1.0, 0.5, 0.25, 1.0],
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        local_offset: [0.0, 0.0],
        local_offset_rot_sin_cos: [0.0, 1.0],
        edge_fade: [0.0; 4],
        texture_mask: 0.0,
    }
}

fn tmesh_instance(x: f32) -> TexturedMeshInstanceRaw {
    TexturedMeshInstanceRaw::new(
        Mat4::from_translation(Vec3::new(x, 0.0, 0.0)),
        [1.0; 4],
        [1.0; 2],
        [0.0; 2],
        [0.0; 2],
        false,
    )
}

fn render_fixture() -> RenderList {
    let mesh: Arc<[MeshVertex]> = Arc::from([
        MeshVertex {
            pos: [0.0, 0.0],
            color: [1.0, 0.5, 0.25, 1.0],
        },
        MeshVertex {
            pos: [1.0, 0.0],
            color: [0.5, 1.0, 0.25, 1.0],
        },
        MeshVertex {
            pos: [0.0, 1.0],
            color: [0.5, 0.5, 1.0, 1.0],
        },
    ]);
    let tmesh: Arc<[TexturedMeshVertex]> = Arc::from([
        TexturedMeshVertex::default(),
        TexturedMeshVertex {
            pos: [1.0, 0.0, 0.0],
            uv: [1.0, 0.0],
            ..TexturedMeshVertex::default()
        },
        TexturedMeshVertex {
            pos: [0.0, 1.0, 0.0],
            uv: [0.0, 1.0],
            ..TexturedMeshVertex::default()
        },
    ]);
    RenderList {
        clear_color: [0.1, 0.2, 0.3, 0.4],
        cameras: vec![Mat4::IDENTITY, Mat4::from_scale(Vec3::splat(2.0))],
        sprite_instances: vec![sprite(1.0), sprite(2.0)],
        objects: vec![
            RenderObject {
                object_type: ObjectType::Sprite(0),
                texture_handle: 7,
                blend: BlendMode::Alpha,
                z: -1,
                order: 0,
                camera: 0,
            },
            RenderObject {
                object_type: ObjectType::Sprite(1),
                texture_handle: 7,
                blend: BlendMode::Alpha,
                z: -1,
                order: 1,
                camera: 0,
            },
            RenderObject {
                object_type: ObjectType::Mesh {
                    transform: Mat4::from_translation(Vec3::new(3.0, 4.0, 0.0)),
                    tint: [0.5, 0.25, 1.0, 0.5],
                    vertices: mesh,
                },
                texture_handle: 0,
                blend: BlendMode::Add,
                z: 0,
                order: 2,
                camera: 1,
            },
            RenderObject {
                object_type: ObjectType::TexturedMesh {
                    instance: tmesh_instance(5.0),
                    vertices: TexturedMeshVertices::Shared(Arc::clone(&tmesh)),
                    depth_test: true,
                },
                texture_handle: 9,
                blend: BlendMode::Alpha,
                z: 1,
                order: 3,
                camera: 0,
            },
            RenderObject {
                object_type: ObjectType::TexturedMesh {
                    instance: tmesh_instance(6.0),
                    vertices: TexturedMeshVertices::Shared(tmesh),
                    depth_test: true,
                },
                texture_handle: 9,
                blend: BlendMode::Alpha,
                z: 1,
                order: 4,
                camera: 0,
            },
        ],
    }
}

fn cached_render_fixture() -> RenderList {
    let mut render = render_fixture();
    for object in &mut render.objects {
        if let ObjectType::TexturedMesh { vertices, .. } = &mut object.object_type {
            let previous = std::mem::replace(vertices, TexturedMeshVertices::Transient(Vec::new()));
            let shared = match previous {
                TexturedMeshVertices::Shared(vertices) => vertices,
                TexturedMeshVertices::Transient(vertices) => Arc::from(vertices),
                TexturedMeshVertices::Retained(geometry) => Arc::from(geometry.vertices().to_vec()),
            };
            let retained = RetainedTMeshGeometry::new(41, shared)
                .expect("fixture geometry must be retainable");
            *vertices = TexturedMeshVertices::Retained(Arc::new(retained));
        }
    }
    render
}

fn frame_capacity() -> FrameCapacity {
    FrameCapacity {
        cameras: 2,
        sprite_instances: 2,
        mesh_vertices: 3,
        tmesh_vertices: 3,
        tmesh_instances: 2,
        ops: 3,
    }
}

fn emit_direct(frame: &mut DrawFrame) {
    frame.begin([0.1, 0.2, 0.3, 0.4]);
    frame.render_objects = 5;
    frame
        .cameras
        .extend_from_slice(&[Mat4::IDENTITY, Mat4::from_scale(Vec3::splat(2.0))]);
    frame
        .sprite_instances
        .extend_from_slice(&[sprite(1.0), sprite(2.0)]);
    frame.mesh_vertices.extend_from_slice(&[
        MeshVertex::default(),
        MeshVertex::default(),
        MeshVertex::default(),
    ]);
    frame.tmesh_vertices.extend_from_slice(&[
        TexturedMeshVertex::default(),
        TexturedMeshVertex::default(),
        TexturedMeshVertex::default(),
    ]);
    frame
        .tmesh_instances
        .extend_from_slice(&[tmesh_instance(5.0), tmesh_instance(6.0)]);
    frame.ops.extend_from_slice(&[
        DrawOp::Sprite(SpriteRun {
            instance_start: 0,
            instance_count: 2,
            blend: BlendMode::Alpha,
            texture_handle: 7,
            camera: 0,
        }),
        DrawOp::Mesh(MeshRun {
            vertex_start: 0,
            vertex_count: 3,
            blend: BlendMode::Add,
            camera: 1,
        }),
        DrawOp::TexturedMesh(TexturedMeshRun {
            source: TexturedMeshSource::Transient {
                vertex_start: 0,
                vertex_count: 3,
                geom_key: 3,
            },
            instance_start: 0,
            instance_count: 2,
            blend: BlendMode::Alpha,
            texture_handle: 9,
            camera: 0,
            depth_test: true,
        }),
    ]);
}

#[test]
fn direct_view_borrows_complete_frame_storage() {
    let mut frame = DrawFrame::with_capacity(frame_capacity());
    emit_direct(&mut frame);
    let view = frame.view();

    assert_eq!(view.clear_color, frame.clear_color);
    assert_eq!(view.cameras, frame.cameras);
    assert_eq!(view.sprite_instances, frame.sprite_instances);
    assert_eq!(view.mesh_vertices.as_ptr(), frame.mesh_vertices.as_ptr());
    assert_eq!(view.tmesh_vertices.as_ptr(), frame.tmesh_vertices.as_ptr());
    assert_eq!(view.tmesh_instances, frame.tmesh_instances);
    assert_eq!(view.ops, frame.ops);
}

#[test]
fn direct_frame_reports_counts_without_preparation() {
    let mut frame = DrawFrame::with_capacity(frame_capacity());
    emit_direct(&mut frame);

    let stats = frame.prepare_stats();

    assert_eq!(stats.dynamic_upload_vertices, 3);
    assert_eq!(stats.render_objects, 5);
    assert_eq!(stats.sprite_instances, 2);
    assert_eq!(stats.mesh_vertices, 3);
    assert_eq!(stats.tmesh_vertices, 3);
    assert_eq!(stats.tmesh_instances, 2);
    assert_eq!(stats.draw_ops, 3);
    assert_eq!(stats.mesh_vertex_capacity, frame.mesh_vertices.capacity());
    assert_eq!(stats.tmesh_vertex_capacity, frame.tmesh_vertices.capacity());
    assert_eq!(
        stats.tmesh_instance_capacity,
        frame.tmesh_instances.capacity()
    );
    assert_eq!(stats.op_capacity, frame.ops.capacity());
}

#[test]
fn render_list_adapter_matches_legacy_preparation() {
    let render_list = render_fixture();
    let mut legacy = DrawScratch::with_capacity(3, 3, 2, 5);
    let legacy_stats: PrepareStats = prepare(&render_list, &mut legacy, |_, _| {
        TMeshCacheResult::CapacityExceeded
    });
    let mut adapted = DrawScratch::with_capacity(3, 3, 2, 5);
    let (view, stats): (_, FramePrepareStats) =
        prepare_render_list(&render_list, &mut adapted, |_, _| {
            TMeshCacheResult::CapacityExceeded
        });

    assert_eq!(stats, legacy_stats);
    assert_eq!(stats.dynamic_upload_vertices, 3);
    assert_eq!(stats.cached_upload_vertices, 0);
    assert_eq!(stats.render_objects, 5);
    assert_eq!(stats.sprite_instances, 2);
    assert_eq!(stats.mesh_vertices, 3);
    assert_eq!(stats.tmesh_vertices, 3);
    assert_eq!(stats.tmesh_instances, 2);
    assert_eq!(stats.draw_ops, 3);
    assert_eq!(stats.sprite_runs, 1);
    assert_eq!(stats.mesh_runs, 1);
    assert_eq!(stats.tmesh_runs, 1);
    assert_eq!(view.clear_color, render_list.clear_color);
    assert_eq!(view.cameras.as_ptr(), render_list.cameras.as_ptr());
    assert_eq!(
        view.sprite_instances.as_ptr(),
        render_list.sprite_instances.as_ptr()
    );
    assert_eq!(view.mesh_vertices.len(), 3);
    assert_eq!(
        bytemuck::cast_slice::<MeshVertex, u8>(view.mesh_vertices),
        bytemuck::cast_slice::<MeshVertex, u8>(legacy.mesh_vertices.as_slice())
    );
    assert_eq!(
        bytemuck::cast_slice::<TexturedMeshVertex, u8>(view.tmesh_vertices),
        bytemuck::cast_slice::<TexturedMeshVertex, u8>(legacy.tmesh_vertices.as_slice())
    );
    assert_eq!(view.tmesh_instances, legacy.tmesh_instances);
    assert_eq!(view.ops, legacy.ops.as_slice());
    assert_eq!(
        view.ops,
        [
            DrawOp::Sprite(SpriteRun {
                instance_start: 0,
                instance_count: 2,
                blend: BlendMode::Alpha,
                texture_handle: 7,
                camera: 0,
            }),
            DrawOp::Mesh(MeshRun {
                vertex_start: 0,
                vertex_count: 3,
                blend: BlendMode::Add,
                camera: 1,
            }),
            DrawOp::TexturedMesh(TexturedMeshRun {
                source: TexturedMeshSource::Transient {
                    vertex_start: 0,
                    vertex_count: 3,
                    geom_key: 3,
                },
                instance_start: 0,
                instance_count: 2,
                blend: BlendMode::Alpha,
                texture_handle: 9,
                camera: 0,
                depth_test: true,
            }),
        ]
    );
}

#[test]
fn cached_upload_stats_distinguish_uploads_from_resident_hits() {
    let render_list = cached_render_fixture();
    let mut scratch = DrawScratch::default();
    let mut upload_calls = 0;
    let (uploaded_view, uploaded) = prepare_render_list(&render_list, &mut scratch, |_, _| {
        upload_calls += 1;
        TMeshCacheResult::Uploaded
    });

    assert_eq!(upload_calls, 1);
    assert_eq!(uploaded.cached_upload_vertices, 3);
    assert_eq!(uploaded.dynamic_upload_vertices, 0);
    assert!(matches!(
        uploaded_view.ops.last(),
        Some(DrawOp::TexturedMesh(TexturedMeshRun {
            source: TexturedMeshSource::Cached { geometry_id, .. },
            ..
        })) if geometry_id.logical_key() == 41
    ));

    let mut resident_calls = 0;
    let (_, resident) = prepare_render_list(&render_list, &mut scratch, |_, _| {
        resident_calls += 1;
        TMeshCacheResult::Resident
    });
    assert_eq!(resident_calls, 1);
    assert_eq!(resident.cached_upload_vertices, 0);
    assert_eq!(resident.dynamic_upload_vertices, 0);

    let (_, failed) = prepare_render_list(&render_list, &mut scratch, |_, _| {
        TMeshCacheResult::UploadFailed
    });
    assert_eq!(failed.cached_upload_vertices, 0);
    assert_eq!(failed.dynamic_upload_vertices, 3);
}

#[test]
fn geometry_identity_rejects_invalid_or_empty_payloads() {
    let vertex = TexturedMeshVertex::default();

    assert!(TMeshGeometryId::new(0, &[vertex]).is_none());
    assert!(TMeshGeometryId::new(41, &[]).is_none());
    assert!(TMeshGeometryId::from_content(&[]).is_none());
    assert!(RetainedTMeshGeometry::new(0, Arc::from([vertex])).is_none());
    assert!(RetainedTMeshGeometry::new(41, Arc::from([])).is_none());

    let semantic = TMeshGeometryId::new(41, &[vertex]).expect("valid geometry identity");
    let content = TMeshGeometryId::from_content(&[vertex]).expect("content geometry identity");
    let retained = RetainedTMeshGeometry::new(41, Arc::from([vertex]))
        .expect("valid retained geometry payload");
    assert_eq!(semantic.logical_key(), 41);
    assert_ne!(content.logical_key(), 0);
    assert_eq!(semantic.fingerprint(), content.fingerprint());
    assert_eq!(retained.id(), semantic);
    assert_eq!(retained.vertices().len(), 1);
    assert_eq!(
        std::mem::size_of::<Option<TMeshGeometryId>>(),
        std::mem::size_of::<TMeshGeometryId>()
    );
    assert_eq!(std::mem::size_of::<TMeshGeometryId>(), 16);
}

#[test]
fn retained_geometry_keeps_identity_bound_to_its_bytes() {
    let vertices_a: Arc<[TexturedMeshVertex]> = Arc::from([TexturedMeshVertex::default()]);
    let vertices_b: Arc<[TexturedMeshVertex]> = Arc::from([TexturedMeshVertex {
        pos: [1.0, 0.0, 0.0],
        ..TexturedMeshVertex::default()
    }]);
    let retained_a = RetainedTMeshGeometry::new(41, vertices_a).expect("valid first geometry");
    let retained_b = RetainedTMeshGeometry::new(41, vertices_b).expect("valid second geometry");
    let id_a = retained_a.id();
    let vertices = TexturedMeshVertices::Retained(Arc::new(retained_b));

    assert_ne!(vertices.geometry_id(), Some(id_a));
    assert_eq!(
        vertices.geometry_id(),
        TMeshGeometryId::new(41, vertices.as_ref())
    );
}

#[test]
fn legacy_prep_keeps_revisions_of_one_logical_geometry_distinct() {
    let mut render = render_fixture();
    let mut cached = 0usize;
    for object in &mut render.objects {
        let ObjectType::TexturedMesh { vertices, .. } = &mut object.object_type else {
            continue;
        };
        let mut revised = vertices.as_ref().to_vec();
        if cached == 1 {
            revised[0].pos[0] = 12.0;
        }
        let retained = RetainedTMeshGeometry::new(41, Arc::from(revised))
            .expect("fixture geometry must be retainable");
        *vertices = TexturedMeshVertices::Retained(Arc::new(retained));
        cached += 1;
    }

    let mut ensured = Vec::new();
    let mut scratch = DrawScratch::default();
    let (_, stats) = prepare_render_list(&render, &mut scratch, |id, _| {
        ensured.push(id);
        TMeshCacheResult::Resident
    });

    assert_eq!(ensured.len(), 2);
    assert_eq!(ensured[0].logical_key(), 41);
    assert_eq!(ensured[1].logical_key(), 41);
    assert_ne!(ensured[0].fingerprint(), ensured[1].fingerprint());
    assert_eq!(stats.tmesh_runs, 2);
}

#[test]
fn preparation_reports_cold_growth_and_warm_run_counts() {
    let render_list = render_fixture();
    let mut scratch = DrawScratch::default();

    let (_, cold) = prepare_render_list(&render_list, &mut scratch, |_, _| {
        TMeshCacheResult::CapacityExceeded
    });
    let (_, warm) = prepare_render_list(&render_list, &mut scratch, |_, _| {
        TMeshCacheResult::CapacityExceeded
    });

    assert!(cold.scratch_growth_events > 0);
    assert_eq!(warm.scratch_growth_events, 0);
    assert_eq!(warm.draw_ops, 3);
    assert_eq!(warm.sprite_runs, 1);
    assert_eq!(warm.mesh_runs, 1);
    assert_eq!(warm.tmesh_runs, 1);
    assert_eq!(warm.op_capacity, scratch.ops.capacity());
    assert_eq!(warm.mesh_vertex_capacity, scratch.mesh_vertices.capacity());
    assert_eq!(
        warm.tmesh_vertex_capacity,
        scratch.tmesh_vertices.capacity()
    );
    assert_eq!(
        warm.tmesh_instance_capacity,
        scratch.tmesh_instances.capacity()
    );
}

#[test]
fn begin_retains_reserved_frame_capacity() {
    let capacity = frame_capacity();
    let mut frame = DrawFrame::default();
    frame.reserve(capacity);
    emit_direct(&mut frame);
    let warmed_capacity = frame.capacity();
    let pointers = (
        frame.cameras.as_ptr(),
        frame.sprite_instances.as_ptr(),
        frame.mesh_vertices.as_ptr(),
        frame.tmesh_vertices.as_ptr(),
        frame.tmesh_instances.as_ptr(),
        frame.ops.as_ptr(),
    );

    emit_direct(&mut frame);

    assert!(warmed_capacity.cameras >= capacity.cameras);
    assert!(warmed_capacity.sprite_instances >= capacity.sprite_instances);
    assert!(warmed_capacity.mesh_vertices >= capacity.mesh_vertices);
    assert!(warmed_capacity.tmesh_vertices >= capacity.tmesh_vertices);
    assert!(warmed_capacity.tmesh_instances >= capacity.tmesh_instances);
    assert!(warmed_capacity.ops >= capacity.ops);
    assert_eq!(FrameCapacity::default().growth_events(warmed_capacity), 6);
    assert_eq!(warmed_capacity.growth_events(warmed_capacity), 0);
    assert_eq!(frame.capacity(), warmed_capacity);
    assert_eq!(frame.cameras.as_ptr(), pointers.0);
    assert_eq!(frame.sprite_instances.as_ptr(), pointers.1);
    assert_eq!(frame.mesh_vertices.as_ptr(), pointers.2);
    assert_eq!(frame.tmesh_vertices.as_ptr(), pointers.3);
    assert_eq!(frame.tmesh_instances.as_ptr(), pointers.4);
    assert_eq!(frame.ops.as_ptr(), pointers.5);
}

#[test]
fn warmed_direct_emission_allocates_nothing() {
    let mut frame = DrawFrame::with_capacity(frame_capacity());
    emit_direct(&mut frame);

    let allocations = count_allocs(|| {
        for _ in 0..128 {
            emit_direct(&mut frame);
            black_box(frame.view());
        }
    });

    assert_eq!(allocations, (0, 0));
}

#[test]
fn warmed_render_list_adapter_allocates_nothing() {
    let render_list = render_fixture();
    let mut scratch = DrawScratch::with_capacity(3, 3, 2, 5);
    let (_, cold_stats) = prepare_render_list(&render_list, &mut scratch, |_, _| {
        TMeshCacheResult::CapacityExceeded
    });
    assert_eq!(cold_stats.scratch_growth_events, 0);
    let warmed_capacity = scratch.capacity();
    let mut growth_events = 0;

    let allocations = count_allocs(|| {
        for _ in 0..128 {
            let (view, stats) = prepare_render_list(&render_list, &mut scratch, |_, _| {
                TMeshCacheResult::CapacityExceeded
            });
            growth_events += stats.scratch_growth_events;
            black_box((view, stats));
        }
    });

    assert_eq!(allocations, (0, 0));
    assert_eq!(growth_events, 0);
    assert_eq!(scratch.capacity(), warmed_capacity);
}
