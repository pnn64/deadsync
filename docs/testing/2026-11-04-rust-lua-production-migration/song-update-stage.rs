impl RustSongRuntime {
    /// The existing compiler's update stage: capture factory hits, install
    /// material clocks, then choose analytic curves or chronological Rust replay.
    #[allow(clippy::too_many_arguments)]
    pub fn compile_song_update_stage<S, V, A>(
        &mut self,
        screen: ActorScene,
        root: Value,
        context: &SongLuaCompileContext,
        overlays: &mut SongOverlayActors<SongLuaOverlayKind<S, V, A>>,
        tracked: &mut SongTrackedActors,
        messages: &[crate::SongLuaMessageEvent],
        activation: &SongActivation,
        sounds: &mut Vec<crate::SongLuaSoundEvent>,
        judgments: &mut Vec<(usize, crate::SongLuaJudgmentTexture)>,
        sprites: &mut Vec<(usize, crate::SongLuaSpriteTexture)>,
        noteskins: SongLuaNoteskinResolver,
        mut read_layer: impl FnMut(&S) -> Option<crate::SongLuaOverlayModelLayer<V>>,
        visual: impl FnMut(&str) -> Option<(SongLuaOverlayKind<S, V, A>, SongLuaOverlayState)>,
    ) -> Result<SongUpdateOutput, String> {
        self.actor_scene_root(screen).ok_or("released or foreign song screen")?;
        let mut roots = super::song_compiler_roots::Roots::default();
        let result = (|| {
            roots.keep(&mut self.vm, root)?;
            self.install_song_multitap_hits(context, overlays)?;
            let mut materials = Vec::new();
            for row in &overlays.actors {
                let initial = row.actor.initial_state.sprite_state_index;
                let clocks = match &row.actor.kind {
                    SongLuaOverlayKind::Model { layers } => layers.iter()
                        .map(|layer| crate::model_texture::Clock::with_initial_state(layer, initial))
                        .collect::<Vec<_>>(),
                    SongLuaOverlayKind::NoteskinActor { slots, .. } => slots.iter()
                        .map(|slot| read_layer(slot).as_ref().and_then(|layer|
                            crate::model_texture::Clock::with_initial_state(layer, initial)))
                        .collect(),
                    _ => continue,
                };
                if clocks.iter().any(Option::is_some) {
                    let Value::Table(actor) = self.song_overlay_actor_value(row).map_err(|e|e.to_string())? else {unreachable!()};
                    materials.push((actor,clocks));
                }
            }
            let active = self.actors.install_compiler_materials(&mut self.vm,materials).map_err(|e|e.to_string())?;
            if !active && let Some(overlay_eases) = self.compile_song_multitap_update_overlays(root, context, overlays, noteskins, visual)? {
                return Ok(SongUpdateOutput{overlay_eases,..Default::default()});
            }
            self.compile_song_update_functions(screen,root,context,overlays,tracked,messages,activation,sounds,judgments,sprites)
        })();
        roots.clear(&mut self.vm)?;
        result
    }
}
