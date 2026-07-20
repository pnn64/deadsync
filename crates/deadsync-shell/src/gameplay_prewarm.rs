use deadlib_render::SamplerDesc;
use deadlib_renderer::Backend;
use deadsync_assets::noteskin::Noteskin;
use deadsync_assets::song_lua::{SongLuaOverlayActor, SongLuaOverlayKind};
use deadsync_assets::{AssetManager, media_cache};
use deadsync_chart::{SongBackgroundChange, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::SongLuaRuntimeVisuals;
use log::warn;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

fn prewarm_model_texture_key(
    assets: &mut AssetManager,
    backend: &mut Backend,
    seen: &mut HashSet<String>,
    seen_model_textures: &mut HashSet<String>,
    key: &str,
) {
    let key = deadsync_assets::canonical_texture_key(key);
    if !seen_model_textures.insert(key.clone()) {
        return;
    }
    assets.ensure_texture_for_key_with_sampler(
        backend,
        &key,
        deadsync_assets::textures::model_texture_sampler(&key),
    );
    seen.insert(key);
}

fn prewarm_noteskin_textures(
    assets: &mut AssetManager,
    backend: &mut Backend,
    seen: &mut HashSet<String>,
    seen_model_textures: &mut HashSet<String>,
    noteskin: &Noteskin,
) {
    noteskin.for_each_slot(|slot| {
        let key = slot.texture_key();
        if seen.insert(key.to_owned()) {
            assets.ensure_texture_for_key(backend, key);
        }
    });
    noteskin.for_each_slot(|slot| {
        if slot.model.is_some() {
            prewarm_model_texture_key(
                assets,
                backend,
                seen,
                seen_model_textures,
                slot.texture_key(),
            );
        }
    });
}

pub fn prewarm_gameplay_assets<CapturedActor, StateDelta>(
    assets: &mut AssetManager,
    backend: &mut Backend,
    noteskin_sets: [&[Option<Arc<Noteskin>>; MAX_PLAYERS]; 4],
    song: &SongData,
    background_changes: &[SongBackgroundChange],
    song_lua_visuals: &SongLuaRuntimeVisuals<SongLuaOverlayActor, CapturedActor, StateDelta>,
) {
    let mut seen = HashSet::<String>::with_capacity(256);
    let mut seen_model_textures = HashSet::<String>::with_capacity(64);
    let mut seen_song_lua_fonts = HashSet::<&'static str>::with_capacity(8);
    for noteskin in noteskin_sets
        .into_iter()
        .flat_map(|set| set.iter().flatten())
    {
        prewarm_noteskin_textures(
            assets,
            backend,
            &mut seen,
            &mut seen_model_textures,
            noteskin,
        );
    }

    let mut media_paths = Vec::with_capacity(
        deadsync_assets::dynamic_media::gameplay_media_paths_capacity(song, background_changes),
    );
    deadsync_assets::dynamic_media::push_gameplay_media_paths(
        &mut media_paths,
        song,
        background_changes,
    );
    for path in media_paths {
        let key = path.to_string_lossy().into_owned();
        if seen.insert(key) {
            media_cache::ensure_banner_texture(assets, backend, path);
        }
    }

    let mut prewarm_song_lua_overlays = |overlays: &[SongLuaOverlayActor]| {
        for overlay in overlays {
            match &overlay.kind {
                SongLuaOverlayKind::BitmapText {
                    font_name,
                    font_path,
                    ..
                } => {
                    if seen_song_lua_fonts.insert(*font_name)
                        && assets.with_font(font_name, |_| ()).is_none()
                        && let Err(err) =
                            assets.load_font_from_ini_path(backend, *font_name, font_path)
                    {
                        warn!(
                            "Failed to load song lua bitmap font '{}': {}",
                            font_path.display(),
                            err
                        );
                    }
                }
                SongLuaOverlayKind::Sprite {
                    texture_path,
                    texture_key,
                }
                | SongLuaOverlayKind::ActorMultiVertex {
                    texture_path: Some(texture_path),
                    texture_key: Some(texture_key),
                    ..
                } => {
                    let key = texture_key.as_ref();
                    let first_seen = seen.insert(key.to_owned());
                    let sampler = deadsync_assets::song_lua::overlay_sampler(overlay);
                    if sampler != SamplerDesc::default() {
                        match media_cache::load_banner_source_rgba(texture_path) {
                            Ok(rgba) => {
                                if let Err(error) = assets.update_texture_for_key_with_sampler(
                                    backend, key, &rgba, sampler,
                                ) {
                                    warn!(
                                        "Failed to create custom-sampled GPU texture for image {texture_path:?}: {error}. Skipping."
                                    );
                                }
                            }
                            Err(error) => {
                                warn!(
                                    "Failed to load song lua texture source {texture_path:?}: {error}. Skipping."
                                );
                            }
                        }
                    } else if first_seen {
                        media_cache::ensure_banner_texture(assets, backend, texture_path);
                    }
                }
                SongLuaOverlayKind::Model { layers } => {
                    for layer in layers.iter() {
                        prewarm_model_texture_key(
                            assets,
                            backend,
                            &mut seen,
                            &mut seen_model_textures,
                            layer.texture_key.as_ref(),
                        );
                    }
                }
                SongLuaOverlayKind::NoteskinActor { slots } => {
                    for slot in slots.iter() {
                        if slot.model.is_some() {
                            prewarm_model_texture_key(
                                assets,
                                backend,
                                &mut seen,
                                &mut seen_model_textures,
                                slot.texture_key(),
                            );
                        } else if seen.insert(slot.texture_key().to_owned()) {
                            assets.ensure_texture_for_key(backend, slot.texture_key());
                        }
                    }
                }
                _ => {}
            }
        }
    };
    prewarm_song_lua_overlays(&song_lua_visuals.overlays);
    for layer in &song_lua_visuals.background_visual_layers {
        prewarm_song_lua_overlays(&layer.overlays);
    }
    for layer in &song_lua_visuals.foreground_visual_layers {
        prewarm_song_lua_overlays(&layer.overlays);
    }
}

pub fn prewarm_gameplay_sfx<CapturedActor, StateDelta>(
    song_lua_visuals: &SongLuaRuntimeVisuals<SongLuaOverlayActor, CapturedActor, StateDelta>,
    song_lua_sound_paths: &[PathBuf],
) {
    deadsync_audio_stream::preload_sfx("assets/sounds/boom.ogg");
    deadsync_audio_stream::preload_sfx("assets/sounds/assist_tick.ogg");

    let mut sound_paths = Vec::<PathBuf>::with_capacity(song_lua_sound_paths.len());
    let mut seen = HashSet::<String>::with_capacity(song_lua_sound_paths.len());
    let mut prewarm_sound_overlays = |overlays: &[SongLuaOverlayActor]| {
        deadsync_song_lua::push_song_lua_overlay_sound_paths(overlays, &mut seen, &mut sound_paths);
    };
    prewarm_sound_overlays(&song_lua_visuals.overlays);
    for layer in &song_lua_visuals.background_visual_layers {
        prewarm_sound_overlays(&layer.overlays);
    }
    for layer in &song_lua_visuals.foreground_visual_layers {
        prewarm_sound_overlays(&layer.overlays);
    }
    deadsync_song_lua::push_unique_song_lua_sound_paths(
        song_lua_sound_paths,
        &mut seen,
        &mut sound_paths,
    );
    for sound_path in sound_paths {
        let key = sound_path.to_string_lossy();
        deadsync_audio_stream::preload_sfx(key.as_ref());
    }
}
