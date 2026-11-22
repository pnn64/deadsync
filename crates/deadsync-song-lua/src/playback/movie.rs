//! Sample authored movie clocks without advancing actors or decoding on render.
use super::*;

#[derive(Default)]
pub(super) struct Movies {
    root: Layer,
    backgrounds: Vec<Layer>,
    foregrounds: Vec<Layer>,
}

#[derive(Default)]
struct Layer {
    actor_count: usize,
    update_actor: SongLuaOverlayIndex,
    cursors: Vec<usize>,
    movies: Vec<Movie>,
}

struct Movie {
    index: usize,
    updates: std::ops::Range<usize>,
    messages: SongLuaMessageStateCache,
}

impl Layer {
    fn new<S: NoteskinSlot + Clone>(overlays: &[SongLuaOverlayActor<S>]) -> Self {
        let update_actor =
            SongLuaOverlayIndex::new(overlays.iter().position(|overlay| {
                matches!(overlay.kind, SongLuaOverlayKind::UpdateTracks { .. })
            }));
        let tracks = song_lua_overlay_runtime_updates_at(overlays, update_actor);
        let movies = overlays
            .iter()
            .enumerate()
            .filter_map(|(index, overlay)| {
                let SongLuaOverlayKind::Sprite {
                    texture_path,
                    textures,
                    ..
                } = &overlay.kind
                else {
                    return None;
                };
                if !crate::is_song_lua_video_path(texture_path)
                    && !textures
                        .iter()
                        .any(|texture| crate::is_song_lua_video_path(&texture.path))
                {
                    return None;
                }
                Some(Movie {
                    index,
                    updates: tracks.partition_point(|track| track.overlay_index < index)
                        ..tracks.partition_point(|track| track.overlay_index <= index),
                    messages: Default::default(),
                })
            })
            .collect();
        Self {
            actor_count: overlays.len(),
            update_actor,
            cursors: vec![0; tracks.len()],
            movies,
        }
    }

    fn sample<S: NoteskinSlot + Clone>(
        &mut self,
        now: f32,
        overlays: &[SongLuaOverlayActor<S>],
        events: &[Vec<SongLuaOverlayMessageRuntime>],
        eases: &[SongLuaOverlayEaseWindowRuntime],
        ease_ranges: &[std::ops::Range<usize>],
        visit: &mut impl FnMut(&Path, f32, bool),
    ) {
        // Live chart loads may add actors. Baked scenes keep the prepared plan.
        if self.actor_count != overlays.len() {
            *self = Self::new(overlays);
        }
        let tracks = song_lua_overlay_runtime_updates_at(overlays, self.update_actor);
        for movie in &mut self.movies {
            let overlay = &overlays[movie.index];
            let state = song_lua_overlay_render_state_from(
                now,
                movie.index,
                overlay,
                events,
                eases,
                ease_ranges,
                tracks,
                movie.updates.clone(),
                &mut self.cursors,
                &mut movie.messages,
            );
            let SongLuaOverlayKind::Sprite {
                texture_path,
                textures,
                ..
            } = &overlay.kind
            else {
                continue;
            };
            let path = crate::sprite_texture_at(textures, now)
                .map_or(texture_path.as_path(), |texture| texture.path.as_path());
            if crate::is_song_lua_video_path(path) {
                visit(path, state.movie_position_seconds, state.sprite_loop);
            }
        }
    }
}

impl Movies {
    pub(super) fn new<S: NoteskinSlot + Clone>(
        visuals: &SongLuaRuntimeVisuals<
            SongLuaOverlayActor<S>,
            SongLuaCapturedActor,
            SongLuaRuntimeOverlayStateDelta,
        >,
    ) -> Self {
        Self {
            root: Layer::new(&visuals.overlays),
            backgrounds: visuals
                .background_visual_layers
                .iter()
                .map(|layer| Layer::new(&layer.overlays))
                .collect(),
            foregrounds: visuals
                .foreground_visual_layers
                .iter()
                .map(|layer| Layer::new(&layer.overlays))
                .collect(),
        }
    }

    pub(super) fn sample<P: deadsync_gameplay::GameplayProfileData, S: NoteskinSlot + Clone>(
        &mut self,
        gameplay: &GameplayCoreState<P, S>,
        mut visit: impl FnMut(&Path, f32, bool),
    ) {
        let visuals = gameplay.song_lua_visuals();
        let now = gameplay.current_music_time_display();
        self.root.sample(
            now,
            &visuals.overlays,
            &visuals.overlay_events,
            &visuals.overlay_eases,
            &visuals.overlay_ease_ranges,
            &mut visit,
        );
        for (plans, layers) in [
            (&mut self.backgrounds, &visuals.background_visual_layers),
            (&mut self.foregrounds, &visuals.foreground_visual_layers),
        ] {
            if plans.len() != layers.len() {
                *plans = layers
                    .iter()
                    .map(|layer| Layer::new(&layer.overlays))
                    .collect();
            }
            for (plan, layer) in plans.iter_mut().zip(layers) {
                plan.sample(
                    now,
                    &layer.overlays,
                    &layer.overlay_events,
                    &layer.overlay_eases,
                    &layer.overlay_ease_ranges,
                    &mut visit,
                );
            }
        }
    }
}

#[cfg(all(test, feature = "test-support"))]
playback_fixture_tests! {
    #[path = "movie_tests.rs"]
    mod tests;
}
