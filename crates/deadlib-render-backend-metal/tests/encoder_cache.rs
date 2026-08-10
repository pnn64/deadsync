#[path = "../src/encoder_cache.rs"]
mod encoder_cache;

use encoder_cache::{BufferUpdate, CullMode, DrawKind, EncoderCache};

#[test]
fn instance_buffers_bind_on_transitions_and_use_offsets_within_runs() {
    let mut cache = EncoderCache::default();

    assert_eq!(cache.instance_buffer(DrawKind::Sprite), BufferUpdate::Bind);
    assert_eq!(
        cache.instance_buffer(DrawKind::Sprite),
        BufferUpdate::Offset
    );
    assert!(cache.kind_changed(DrawKind::Mesh));
    assert_eq!(cache.instance_buffer(DrawKind::Sprite), BufferUpdate::Bind);
    assert_eq!(
        cache.instance_buffer(DrawKind::TexturedMesh),
        BufferUpdate::Bind
    );
    assert_eq!(
        cache.instance_buffer(DrawKind::TexturedMesh),
        BufferUpdate::Offset
    );
}

#[test]
fn independent_encoder_state_survives_pipeline_changes() {
    let mut cache = EncoderCache::default();

    assert!(cache.pipeline_changed(DrawKind::Sprite, 0));
    assert!(!cache.pipeline_changed(DrawKind::Sprite, 0));
    assert!(cache.texture_changed(41));
    assert!(cache.sampler_changed(41, false));
    assert!(cache.depth_changed(false));
    assert!(cache.cull_changed(CullMode::Back));

    assert!(cache.pipeline_changed(DrawKind::Mesh, 0));
    assert!(!cache.texture_changed(41));
    assert!(!cache.sampler_changed(41, false));
    assert!(!cache.depth_changed(false));
    assert!(!cache.cull_changed(CullMode::Back));
    assert!(cache.cull_changed(CullMode::None));

    assert!(cache.sampler_changed(41, true));
    assert!(!cache.texture_changed(41));
    assert!(cache.texture_changed(42));
    assert!(cache.sampler_changed(42, true));
}

#[test]
fn camera_slots_only_reupload_when_their_binding_was_replaced() {
    let mut cache = EncoderCache::default();

    assert!(cache.camera_changed(0, 3));
    assert!(!cache.camera_changed(0, 3));
    assert!(cache.camera_changed(1, 3));
    assert!(!cache.camera_changed(1, 3));

    assert_eq!(
        cache.instance_buffer(DrawKind::TexturedMesh),
        BufferUpdate::Bind
    );
    assert!(cache.camera_changed(0, 3));
    assert!(!cache.camera_changed(1, 3));
}

#[test]
fn mixed_draw_trace_preserves_effective_state() {
    let mut cache = EncoderCache::default();

    assert_eq!(cache.instance_buffer(DrawKind::Sprite), BufferUpdate::Bind);
    assert!(cache.pipeline_changed(DrawKind::Sprite, 1));
    assert!(cache.camera_changed(0, 7));
    assert!(cache.texture_changed(11));
    assert!(cache.sampler_changed(11, false));
    assert!(cache.depth_changed(false));
    assert!(cache.cull_changed(CullMode::Back));

    assert!(cache.kind_changed(DrawKind::Mesh));
    assert!(cache.pipeline_changed(DrawKind::Mesh, 1));
    assert!(!cache.camera_changed(0, 7));
    assert!(!cache.depth_changed(false));
    assert!(cache.cull_changed(CullMode::None));

    assert_eq!(
        cache.instance_buffer(DrawKind::TexturedMesh),
        BufferUpdate::Bind
    );
    assert!(cache.pipeline_changed(DrawKind::TexturedMesh, 1));
    assert!(cache.camera_changed(1, 7));
    assert!(cache.depth_changed(true));
    assert!(cache.cull_changed(CullMode::Back));
    assert!(!cache.texture_changed(11));
    assert!(!cache.sampler_changed(11, false));

    assert_eq!(cache.instance_buffer(DrawKind::Sprite), BufferUpdate::Bind);
    assert!(cache.camera_changed(0, 7));
    assert!(!cache.camera_changed(1, 7));
    assert!(!cache.texture_changed(11));
    assert!(!cache.sampler_changed(11, false));
}
