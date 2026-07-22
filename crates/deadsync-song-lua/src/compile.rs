use deadlib_present::actors::TextAttribute;
use mlua::{Lua, Table};
use std::path::Path;
use std::sync::Arc;

use crate::{
    CompiledSongLua, SongLuaCompileContext, SongLuaCompileTimer, SongLuaHostState,
    SongLuaNoteskinResolver, SongLuaOverlayActor, SongLuaOverlayKind, SongLuaOverlayModelLayer,
    SongLuaOverlayState, SongLuaTimeUnit, SongLuaTrackedActorTarget as TrackedCompileActorTarget,
    add_actor_child_from_path as add_host_actor_child_from_path,
    compile_multitap_update_overlays_for_actors, compile_perframes,
    compile_update_function_overlays, create_dummy_actor as create_host_dummy_actor,
    create_named_child_actor as create_host_named_child_actor, ensure_overlay_arrow_visual,
    entry_file_path, execute_script_file, install_actor_methods as install_host_actor_methods,
    install_compile_host, log_song_lua_compile_timing, merge_compile_info,
    note_field_column_actors as host_note_field_column_actors, push_startup_message_if_listened,
    push_unique_compile_detail, read_actor_model_layers, read_eases_for_overlay_actors,
    read_global_function_nested_tables, read_mod_windows, read_note_column_zoom_hides,
    read_noteskin_tap_actor_slots, read_overlay_compile_actor_actions, read_overlay_compile_actors,
    read_runtime_mod_eases, read_song_lua_sound_paths, read_tracked_compile_actors,
    read_update_function_nested_tables, read_update_function_overlay_compile_actor_actions,
    read_update_function_tables, read_xero_runtime_mod_eases_for_overlay_actors,
    register_loaded_easing_names, restore_compile_globals, run_actor_draw_functions,
    run_actor_init_commands, run_actor_startup_commands, run_actor_update_functions,
    runtime_static_overlay_index_for_actors, snapshot_compile_globals, sort_compiled_song_lua,
};

pub fn compile_song_lua_with_default_host<NoteskinSlot, ModelVertex, MultitapArrowVisualSpec>(
    entry_path: &Path,
    context: &SongLuaCompileContext,
    noteskin_resolver: SongLuaNoteskinResolver,
    read_model_slots: fn(&Path) -> Result<Arc<[NoteskinSlot]>, String>,
    model_layer_from_slot: fn(&NoteskinSlot) -> Option<SongLuaOverlayModelLayer<ModelVertex>>,
    multitap_arrow_visual_spec: MultitapArrowVisualSpec,
) -> Result<
    CompiledSongLua<
        SongLuaOverlayActor<SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>>,
    >,
    String,
>
where
    MultitapArrowVisualSpec: FnMut(
        &SongLuaCompileContext,
        &str,
    ) -> Option<(
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
        SongLuaOverlayState,
    )>,
{
    compile_song_lua_with_actors(
        entry_path,
        context,
        noteskin_resolver,
        create_default_dummy_actor,
        create_default_named_child_actor,
        install_default_actor_methods,
        read_model_slots,
        model_layer_from_slot,
        multitap_arrow_visual_spec,
    )
}

fn create_default_named_child_actor(lua: &Lua, parent: &Table, name: &str) -> mlua::Result<Table> {
    create_host_named_child_actor(
        lua,
        parent,
        name,
        create_default_dummy_actor,
        create_default_named_child_actor,
    )
}

fn default_note_field_column_actors(lua: &Lua, note_field: &Table) -> mlua::Result<Table> {
    host_note_field_column_actors(lua, note_field, create_default_dummy_actor)
}

fn create_default_dummy_actor(lua: &Lua, actor_type: &'static str) -> mlua::Result<Table> {
    create_host_dummy_actor(lua, actor_type, install_default_actor_methods)
}

fn install_default_actor_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    install_host_actor_methods(
        lua,
        actor,
        add_default_actor_child_from_path,
        default_note_field_column_actors,
        create_default_named_child_actor,
        create_default_dummy_actor,
    )
}

fn add_default_actor_child_from_path(lua: &Lua, actor: &Table, path: &str) -> mlua::Result<()> {
    add_host_actor_child_from_path(lua, actor, path, create_default_dummy_actor)
}

pub fn compile_song_lua_with_actors<NoteskinSlot, ModelVertex, MultitapArrowVisualSpec>(
    entry_path: &Path,
    context: &SongLuaCompileContext,
    noteskin_resolver: SongLuaNoteskinResolver,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
    install_actor_methods: fn(&Lua, &Table) -> mlua::Result<()>,
    read_model_slots: fn(&Path) -> Result<Arc<[NoteskinSlot]>, String>,
    model_layer_from_slot: fn(&NoteskinSlot) -> Option<SongLuaOverlayModelLayer<ModelVertex>>,
    mut multitap_arrow_visual_spec: MultitapArrowVisualSpec,
) -> Result<
    CompiledSongLua<
        SongLuaOverlayActor<SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>>,
    >,
    String,
>
where
    MultitapArrowVisualSpec: FnMut(
        &SongLuaCompileContext,
        &str,
    ) -> Option<(
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
        SongLuaOverlayState,
    )>,
{
    let mut compile_timer = SongLuaCompileTimer::start();
    let entry_path = entry_file_path(entry_path)
        .ok_or_else(|| format!("song lua entry '{}' does not exist", entry_path.display()))?;
    let trace_entry_path = entry_path.clone();
    let lua = Lua::new();
    let mut host = SongLuaHostState::default();
    install_compile_host(
        &lua,
        context,
        &mut host,
        noteskin_resolver,
        create_dummy_actor,
        create_named_child_actor,
        install_actor_methods,
    )
    .map_err(|err| err.to_string())?;
    compile_timer.push_stage("host");
    let root = execute_script_file(&lua, &entry_path, context.song_dir.as_path())
        .map_err(|err| format!("failed to execute '{}': {err}", entry_path.display()))?;
    compile_timer.push_stage("execute");
    run_actor_init_commands(&lua, &root).map_err(|err| {
        format!(
            "failed to run actor init commands for '{}': {err}",
            entry_path.display()
        )
    })?;
    compile_timer.push_stage("init_commands");
    run_actor_startup_commands(&lua, &root).map_err(|err| {
        format!(
            "failed to run actor startup commands for '{}': {err}",
            entry_path.display()
        )
    })?;
    compile_timer.push_stage("startup_commands");
    run_actor_update_functions(&lua, &root).map_err(|err| {
        format!(
            "failed to run actor update functions for '{}': {err}",
            entry_path.display()
        )
    })?;
    compile_timer.push_stage("update_functions");
    run_actor_draw_functions(&lua, &root);
    compile_timer.push_stage("draw_functions");
    register_loaded_easing_names(&lua, &mut host).map_err(|err| err.to_string())?;
    compile_timer.push_stage("easing_names");

    let globals = lua.globals();
    let mut out = CompiledSongLua {
        entry_path,
        screen_width: context.screen_width,
        screen_height: context.screen_height,
        ..CompiledSongLua::default()
    };
    let compile_globals =
        snapshot_compile_globals(&lua, &globals).map_err(|err| err.to_string())?;
    let overlays = read_overlay_compile_actors(
        &lua,
        &root,
        context,
        |actor| read_actor_model_layers(actor, read_model_slots, model_layer_from_slot),
        |actor, _context| read_noteskin_tap_actor_slots(actor, read_model_slots),
        |skipped| {
            push_unique_compile_detail(&mut out.info.skipped_message_command_captures, skipped)
        },
    );
    restore_compile_globals(&globals, compile_globals).map_err(|err| err.to_string())?;
    let mut overlays = overlays?;
    compile_timer.push_stage("read_overlays");
    let mut tracked_actors = read_tracked_compile_actors(&lua, create_named_child_actor)?;
    let mut overlay_trigger_counter = 0usize;
    let hidden_players = std::array::from_fn(|player| {
        let key = if player == 0 {
            "__songlua_top_screen_player_1"
        } else {
            "__songlua_top_screen_player_2"
        };
        globals
            .get::<Option<Table>>(key)
            .ok()
            .flatten()
            .and_then(|actor| {
                actor
                    .get::<Option<bool>>("__songlua_visible")
                    .ok()
                    .flatten()
            })
            .is_some_and(|visible| !visible)
    });
    let prefix_perframes = globals
        .get::<Option<Table>>("prefix_globals")
        .map_err(|err| err.to_string())?
        .and_then(|table| table.get::<Option<Table>>("perframes").ok().flatten());
    let global_perframes = globals
        .get::<Option<Table>>("mod_perframes")
        .map_err(|err| err.to_string())?;
    compile_timer.push_stage("read_globals");

    if let Some(prefix_globals) = globals
        .get::<Option<Table>>("prefix_globals")
        .map_err(|err| err.to_string())?
    {
        out.beat_mods.extend(read_mod_windows(
            prefix_globals
                .get::<Option<Table>>("mods")
                .map_err(|err| err.to_string())?,
            SongLuaTimeUnit::Beat,
        )?);
        compile_timer.push_stage("prefix_mods");
        let (eases, overlay_eases, column_offsets, info) = read_eases_for_overlay_actors(
            &lua,
            prefix_globals
                .get::<Option<Table>>("ease")
                .map_err(|err| err.to_string())?,
            SongLuaTimeUnit::Beat,
            &host.easing_names,
            &mut overlays,
        )?;
        out.eases.extend(eases);
        out.overlay_eases.extend(overlay_eases);
        out.column_offsets.extend(column_offsets);
        merge_compile_info(&mut out.info, info);
        compile_timer.push_stage("prefix_eases");
        read_overlay_compile_actor_actions(
            &lua,
            prefix_globals
                .get::<Option<Table>>("actions")
                .map_err(|err| err.to_string())?,
            &mut overlays,
            &mut tracked_actors,
            &mut out.messages,
            &mut overlay_trigger_counter,
            &mut out.info,
        )?;
        compile_timer.push_stage("prefix_actions");
    }

    let global_mods = globals
        .get::<Option<Table>>("mods")
        .map_err(|err| err.to_string())?;
    out.beat_mods.extend(read_mod_windows(
        global_mods.clone(),
        SongLuaTimeUnit::Beat,
    )?);
    let (runtime_eases, runtime_overlay_eases) = read_runtime_mod_eases(
        global_mods,
        &host.easing_names,
        runtime_static_overlay_index_for_actors(&overlays),
        context,
    )?;
    out.eases.extend(runtime_eases);
    out.overlay_eases.extend(runtime_overlay_eases);
    out.time_mods.extend(read_mod_windows(
        globals
            .get::<Option<Table>>("mod_time")
            .map_err(|err| err.to_string())?,
        SongLuaTimeUnit::Second,
    )?);
    for table in read_update_function_tables(&lua, &root, &["mod_time"])? {
        out.time_mods
            .extend(read_mod_windows(Some(table), SongLuaTimeUnit::Second)?);
    }
    compile_timer.push_stage("global_mods");
    let (global_eases, global_overlay_eases, global_column_offsets, global_info) =
        read_eases_for_overlay_actors(
            &lua,
            globals
                .get::<Option<Table>>("mods_ease")
                .map_err(|err| err.to_string())?,
            SongLuaTimeUnit::Beat,
            &host.easing_names,
            &mut overlays,
        )?;
    out.eases.extend(global_eases);
    out.overlay_eases.extend(global_overlay_eases);
    out.column_offsets.extend(global_column_offsets);
    merge_compile_info(&mut out.info, global_info);
    compile_timer.push_stage("global_eases");
    let mut xero_node_tables = read_update_function_nested_tables(&lua, &root, &["nodes"])?;
    xero_node_tables.extend(read_global_function_nested_tables(
        &lua,
        "xero",
        &["definemod", "node"],
        &["nodes"],
    )?);
    let (xero_eases, xero_overlay_eases, xero_info) =
        read_xero_runtime_mod_eases_for_overlay_actors(
            &lua,
            read_update_function_nested_tables(&lua, &root, &["eases"])?,
            xero_node_tables,
            &host.easing_names,
            &overlays,
        )?;
    out.eases.extend(xero_eases);
    out.overlay_eases.extend(xero_overlay_eases);
    merge_compile_info(&mut out.info, xero_info);
    compile_timer.push_stage("xero_eases");
    read_overlay_compile_actor_actions(
        &lua,
        globals
            .get::<Option<Table>>("mod_actions")
            .map_err(|err| err.to_string())?,
        &mut overlays,
        &mut tracked_actors,
        &mut out.messages,
        &mut overlay_trigger_counter,
        &mut out.info,
    )?;
    compile_timer.push_stage("global_actions");
    read_update_function_overlay_compile_actor_actions(
        &lua,
        &root,
        &mut overlays,
        &mut tracked_actors,
        &mut out.messages,
        &mut overlay_trigger_counter,
        &mut out.info,
    )?;
    compile_timer.push_stage("update_actions");
    let (perframe_eases, perframe_overlay_eases, perframe_info) = compile_perframes(
        &lua,
        prefix_perframes,
        global_perframes,
        context,
        &mut overlays,
        &tracked_actors,
    )?;
    out.eases.extend(perframe_eases);
    out.overlay_eases.extend(perframe_overlay_eases);
    merge_compile_info(&mut out.info, perframe_info);
    compile_timer.push_stage("perframes");
    out.note_hides = read_note_column_zoom_hides(&lua)?;
    compile_timer.push_stage("note_hides");
    let update_overlay_eases = match compile_multitap_update_overlays_for_actors(
        &lua,
        context,
        &mut overlays,
        &mut out.messages,
        noteskin_resolver,
        |overlays, arrow_index, noteskin| {
            ensure_overlay_arrow_visual(
                &lua,
                overlays,
                arrow_index,
                noteskin,
                create_dummy_actor,
                |noteskin| multitap_arrow_visual_spec(context, noteskin),
            )
        },
    )? {
        Some(eases) => eases,
        None => {
            compile_update_function_overlays(&lua, &root, context, &mut overlays, &tracked_actors)?
        }
    };
    out.overlay_eases.extend(update_overlay_eases);
    compile_timer.push_stage("update_overlays");
    push_startup_message_if_listened(
        &mut out.messages,
        overlays
            .iter()
            .map(|overlay| overlay.actor.message_commands.as_slice()),
    );
    out.overlays = overlays.into_iter().map(|overlay| overlay.actor).collect();
    for tracked in tracked_actors {
        match tracked.target {
            TrackedCompileActorTarget::Player(player) => out.player_actors[player] = tracked.actor,
            TrackedCompileActorTarget::SongForeground => out.song_foreground = tracked.actor,
        }
    }
    out.hidden_players = hidden_players;

    sort_compiled_song_lua(&mut out);
    out.sound_paths = read_song_lua_sound_paths(&lua)?;
    compile_timer.push_stage("finalize");
    log_song_lua_compile_timing(&trace_entry_path, &compile_timer);
    Ok(out)
}
