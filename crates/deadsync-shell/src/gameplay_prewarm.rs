use deadlib_assets::AssetManager;
use deadlib_render::Backend;
use deadlib_render_core::SamplerDesc;
use deadsync_assets::media_cache;
use deadsync_assets::noteskin::Noteskin;
use deadsync_assets::song_lua::{SongLuaOverlayActor, SongLuaOverlayKind};
use deadsync_chart::{SongBackgroundChange, SongData};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::SongLuaRuntimeVisuals;
use hashbrown::HashSet as FastHashSet;
use log::warn;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[inline]
pub(super) fn insert_texture_key(seen: &mut FastHashSet<String>, key: &str) -> bool {
    let previous_len = seen.len();
    seen.get_or_insert_with(key, str::to_owned);
    seen.len() != previous_len
}

fn prewarm_model_texture_key(
    assets: &mut AssetManager,
    backend: &mut Backend,
    seen: &mut FastHashSet<String>,
    seen_model_textures: &mut FastHashSet<String>,
    key: &str,
) {
    let key = deadsync_assets::canonical_texture_key(key);
    if !seen_model_textures.insert(key.clone()) {
        return;
    }
    deadsync_assets::textures::ensure_texture_for_key_with_sampler(
        assets,
        backend,
        &key,
        deadsync_assets::textures::model_texture_sampler(&key),
    );
    seen.insert(key);
}

fn prewarm_noteskin_textures(
    assets: &mut AssetManager,
    backend: &mut Backend,
    seen: &mut FastHashSet<String>,
    seen_model_textures: &mut FastHashSet<String>,
    noteskin: &Noteskin,
) {
    noteskin.for_each_slot(|slot| {
        let key = slot.texture_key();
        if insert_texture_key(seen, key) {
            deadsync_assets::textures::ensure_texture_for_key(
                assets,
                backend,
                key,
                deadsync_theme_simply_love::asset_manifest().texture_needs_repeat_sampler,
            );
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
    let mut seen = FastHashSet::<String>::with_capacity(256);
    let mut seen_model_textures = FastHashSet::<String>::with_capacity(64);
    let mut seen_song_lua_fonts = FastHashSet::<&'static str>::with_capacity(8);
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

    for path in deadsync_assets::dynamic_media::gameplay_media_paths(song, background_changes) {
        let key = path.to_string_lossy();
        if insert_texture_key(&mut seen, key.as_ref()) {
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
                        && let Err(err) = deadsync_assets::fonts::load_font_from_ini_path(
                            assets, backend, font_name, font_path,
                        )
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
                    ..
                }
                | SongLuaOverlayKind::ActorMultiVertex {
                    texture_path: Some(texture_path),
                    texture_key: Some(texture_key),
                    ..
                } => {
                    let key = texture_key.as_ref();
                    let first_seen = insert_texture_key(&mut seen, key);
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
                        } else if insert_texture_key(&mut seen, slot.texture_key()) {
                            deadsync_assets::textures::ensure_texture_for_key(
                                assets,
                                backend,
                                slot.texture_key(),
                                deadsync_theme_simply_love::asset_manifest()
                                    .texture_needs_repeat_sampler,
                            );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_key_insert_reports_only_first_owner() {
        let mut seen = FastHashSet::new();
        assert!(insert_texture_key(&mut seen, "noteskin/receptor.png"));
        assert!(!insert_texture_key(&mut seen, "noteskin/receptor.png"));
        assert!(insert_texture_key(&mut seen, "noteskin/tap.png"));
        assert_eq!(seen.len(), 2);
        assert!(seen.contains("noteskin/receptor.png"));
        assert!(seen.contains("noteskin/tap.png"));
    }
}

const BOOM_SFX_PATH: &str = "assets/sounds/boom.ogg";

/// Song-lifetime resolved sounds retained by the application thread.
///
/// Fixed gameplay sounds take direct fields. Arbitrary `SongLua` paths use the
/// prewarmed map because their domain is chart-authored and not statically
/// bounded. Submission borrows an `SfxId`; no decode, allocation, lock, or
/// audio-cache lookup occurs on gameplay frames.
#[derive(Default)]
pub struct GameplaySfx {
    boom: Option<deadsync_audio_stream::SfxId>,
    assist_tick: Option<deadsync_audio_stream::SfxId>,
    song_lua: HashMap<String, deadsync_audio_stream::SfxId>,
}

impl GameplaySfx {
    pub fn resolve(&self, path: &str) -> Option<&deadsync_audio_stream::SfxId> {
        match path {
            BOOM_SFX_PATH => self.boom.as_ref(),
            deadsync_gameplay::ASSIST_TICK_SFX_PATH => self.assist_tick.as_ref(),
            _ => self.song_lua.get(path),
        }
    }

    pub fn resolve_path(&self, path: &Path) -> Option<&deadsync_audio_stream::SfxId> {
        self.resolve(path.to_string_lossy().as_ref())
    }
}

pub fn prewarm_gameplay_sfx<CapturedActor, StateDelta>(
    audio: &mut deadsync_audio_stream::AudioControl,
    song_lua_visuals: &SongLuaRuntimeVisuals<SongLuaOverlayActor, CapturedActor, StateDelta>,
    song_lua_sound_paths: &[PathBuf],
) -> GameplaySfx {
    let boom = audio.prepare_sfx(BOOM_SFX_PATH);
    let assist_tick = audio.prepare_sfx(deadsync_gameplay::ASSIST_TICK_SFX_PATH);

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
    let mut song_lua = HashMap::with_capacity(sound_paths.len());
    for sound_path in sound_paths {
        let key = sound_path.to_string_lossy();
        if let Some(sound) = audio.prepare_sfx(key.as_ref()) {
            song_lua.insert(key.into_owned(), sound);
        }
    }
    GameplaySfx {
        boom,
        assist_tick,
        song_lua,
    }
}
