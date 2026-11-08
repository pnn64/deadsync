use super::*;

pub fn reload_song_lua_with_default_host<NoteskinSlot, ModelVertex, MultitapArrowVisualSpec>(
    mut scene: crate::SongLuaLiveScene,
    read_model_slots: fn(&Path, &Path, &Path) -> Result<Arc<[NoteskinSlot]>, String>,
    model_layer_from_slot: fn(&NoteskinSlot) -> Option<SongLuaOverlayModelLayer<ModelVertex>>,
    multitap_arrow_visual_spec: MultitapArrowVisualSpec,
) -> Result<Vec<DefaultCompiledSongLua<NoteskinSlot, ModelVertex>>, String>
where
    MultitapArrowVisualSpec: FnMut(
        &SongLuaCompileContext,
        &str,
    ) -> Option<(
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
        SongLuaOverlayState,
    )>,
{
    let mut ending = scene.end().map_err(|err| err.to_string())?;
    let context = scene.selected_context().map_err(|err| err.to_string())?;
    let entry_paths = scene.entry_paths.clone();
    let paths = entry_paths
        .iter()
        .map(|path| path.as_path())
        .collect::<Vec<_>>();
    let mut compiled = compile_song_lua_layers_with_actors(
        &paths,
        scene.primary_index,
        &context,
        Some(scene.lua),
        SongLuaNoteskinResolver::default(),
        create_default_dummy_actor,
        create_default_named_child_actor,
        install_default_actor_methods,
        read_model_slots,
        model_layer_from_slot,
        multitap_arrow_visual_spec,
    )?;
    let replacement = compiled
        .iter_mut()
        .find_map(|compiled| compiled.live_scene.as_mut())
        .ok_or("gameplay reload did not return an owned Lua scene")?;
    ending.append(replacement.take_startup_output());
    replacement.startup_output = ending;
    Ok(compiled)
}
