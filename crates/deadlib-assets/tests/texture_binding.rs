use deadlib_assets::upload::TextureUploadBudget;
use deadlib_assets::{METADATA_TEXTURE_CONTEXT, TextureStore, register_texture_dims};
use deadlib_present::actors::{ActorResourceArena, SpriteSource};
use deadlib_present::texture::TextureContext;
use deadlib_render_core::{INVALID_TEXTURE_HANDLE, SamplerDesc};
use image::RgbaImage;
use std::sync::Arc;

// Metadata remains process-wide during CPU asset preparation.
static METADATA_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn dims(store: &TextureStore<()>, key: &str) -> Option<(u32, u32)> {
    store
        .bind_texture(key)?
        .dimensions
        .map(|meta| (meta.w, meta.h))
}

#[test]
fn metadata_does_not_create_a_gpu_identity() {
    let _guard = METADATA_TEST_LOCK.lock().unwrap();
    let key = "binding-metadata 4x2.png";
    let mut store = TextureStore::<()>::new();
    register_texture_dims(key, 80, 40);
    assert_eq!(
        METADATA_TEXTURE_CONTEXT.texture_handle(key),
        INVALID_TEXTURE_HANDLE
    );
    assert_eq!(
        store.texture_dims(key).map(|meta| (meta.w, meta.h)),
        Some((80, 40))
    );
    assert!(store.bind_texture(key).is_none());
    let handle = store.reserve_texture_handle(key.into());
    let bound = store.bind_texture(key).unwrap();
    assert_eq!(bound.handle, handle);
    assert_eq!(bound.sheet, (4, 2));
    assert_eq!(dims(&store, key), Some((80, 40)));
    assert!(!store.has_uploaded_texture_key(key));
}

#[test]
fn reserved_identity_can_acquire_metadata_later() {
    let _guard = METADATA_TEST_LOCK.lock().unwrap();
    let key = "binding-late-metadata.png";
    let mut store = TextureStore::<()>::new();
    let handle = store.reserve_texture_handle(key.into());
    let before = store.revision();
    assert!(store.bind_texture(key).unwrap().dimensions.is_none());
    register_texture_dims(key, 32, 16);
    assert_ne!(store.revision(), before);
    assert_eq!(store.bind_texture(key).unwrap().handle, handle);
    assert_eq!(dims(&store, key), Some((32, 16)));
}

#[test]
fn aliases_and_reload_follow_local_identity_lifetime() {
    let _guard = METADATA_TEST_LOCK.lock().unwrap();
    let key = "Graphics/Binding 2x1.PNG";
    let mut store = TextureStore::<()>::new();
    store.insert_texture(key.into(), (), 64, 16);
    let first = store.texture_handle(key);
    assert_eq!(store.texture_handle("graphics/binding 2x1.png"), first);
    assert_eq!(dims(&store, "graphics/binding 2x1.png"), Some((64, 16)));
    store.remove_texture(key);
    assert!(store.bind_texture(key).is_none());
    assert_eq!(
        store.texture_handle("graphics/binding 2x1.png"),
        INVALID_TEXTURE_HANDLE
    );
    store.insert_texture(key.into(), (), 128, 32);
    assert_ne!(store.texture_handle(key), first);
    store.queue_texture_upload("queued-before-reload".into(), RgbaImage::new(2, 2));
    store.take_textures();
    assert_eq!(store.texture_handle(key), INVALID_TEXTURE_HANDLE);
    assert!(
        store
            .pop_next_upload(
                TextureUploadBudget {
                    max_uploads: 1,
                    max_bytes: 64
                },
                0,
                0
            )
            .is_none()
    );
}

#[test]
fn colliding_case_aliases_keep_exact_keys_and_recover_on_removal() {
    let _guard = METADATA_TEST_LOCK.lock().unwrap();
    let mut store = TextureStore::<()>::new();
    let upper = store.reserve_texture_handle("Alias.PNG".into());
    let lower = store.reserve_texture_handle("alias.png".into());
    assert_ne!(upper, lower);
    assert_eq!(store.texture_handle("Alias.PNG"), upper);
    assert_eq!(store.texture_handle("alias.png"), lower);
    store.remove_texture("Alias.PNG");
    assert_eq!(store.texture_handle("ALIAS.PNG"), lower);
}

#[test]
fn shared_metadata_cannot_hide_a_local_resize_or_leak_store_handles() {
    let _guard = METADATA_TEST_LOCK.lock().unwrap();
    let key: Arc<str> = Arc::from("binding-independent 2x1.png");
    let mut first = TextureStore::<()>::new();
    let mut second = TextureStore::<()>::new();
    first.insert_texture(key.to_string(), (), 32, 16);
    second.reserve_texture_handle("another-slot".into());
    second.insert_texture(key.to_string(), (), 64, 32);
    let choice = deadlib_assets::TextureChoice::new(key.to_string(), String::new());
    let arena = ActorResourceArena::new(1);
    let SpriteSource::ArenaTextureHandle {
        handle: a,
        generation: a_revision,
        ..
    } = choice.actor_texture_source(&arena, &first)
    else {
        panic!("expected an arena texture");
    };
    let SpriteSource::ArenaTextureHandle {
        handle: b,
        generation: b_revision,
        ..
    } = choice.actor_texture_source(&arena, &second)
    else {
        panic!("expected an arena texture");
    };
    assert_ne!(a_revision, b_revision);
    assert_ne!(a, b);
    assert_eq!(b, second.texture_handle(&key));
    assert_eq!(dims(&first, &key), Some((32, 16)));
    assert_eq!(dims(&second, &key), Some((64, 32)));
    register_texture_dims(&key, 64, 32);
    let before = first.revision();
    first.queue_texture_upload(key.to_string(), RgbaImage::new(64, 32));
    assert_ne!(first.revision(), before);
    assert_eq!(dims(&first, &key), Some((64, 32)));
    second.take_textures();
    assert_eq!(first.texture_handle(&key), a);
}

#[test]
fn steady_uploads_preserve_bindings_and_cancelled_resizes_refresh_them() {
    let _guard = METADATA_TEST_LOCK.lock().unwrap();
    let key = "binding-video";
    let mut store = TextureStore::<()>::new();
    store.queue_texture_upload(key.into(), RgbaImage::new(2, 2));
    let handle = store.texture_handle(key);
    let budget = TextureUploadBudget {
        max_uploads: 1,
        max_bytes: 64,
    };
    store.pop_next_upload(budget, 0, 0).unwrap();
    store.set_texture_for_handle(handle, (), 2, 2);
    let before = store.revision();
    for _ in 0..8 {
        store.queue_texture_upload_shared(
            key.into(),
            Arc::new(RgbaImage::new(2, 2)),
            SamplerDesc::default(),
        );
        store.pop_next_upload(budget, 0, 0).unwrap();
        assert!(store.apply_upload_update(handle, 2, 2).is_some());
    }
    assert_eq!(store.revision(), before);
    store.queue_texture_upload(key.into(), RgbaImage::new(4, 4));
    assert_eq!(dims(&store, key), Some((4, 4)));
    let resized = store.revision();
    store.set_texture_for_key(key.into(), (), 2, 2);
    assert_ne!(store.revision(), resized);
    assert_eq!(dims(&store, key), Some((2, 2)));
    assert_eq!(store.texture_handle(key), handle);
    store.queue_texture_upload(key.into(), RgbaImage::new(4, 4));
    let queued = store.revision();
    // A failed upload leaves the previous GPU image installed.
    drop(store.pop_next_upload(budget, 0, 0).unwrap());
    assert_ne!(store.revision(), queued);
    assert_eq!(dims(&store, key), Some((2, 2)));
}
