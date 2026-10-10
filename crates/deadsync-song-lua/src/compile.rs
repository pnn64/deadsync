use crate::lua_util::run_actor_startup_commands;
use deadlib_present::actors::TextAttribute;
use mlua::{Lua, Table, Value};
use std::path::Path;
use std::sync::Arc;

use crate::{
    CompiledSongLua, SongLuaCapturedChildActor, SongLuaCompileContext, SongLuaCompileTimer,
    SongLuaHostState, SongLuaMessageEvent, SongLuaNoteskinResolver, SongLuaOverlayActor,
    SongLuaOverlayCommandBlock, SongLuaOverlayKind, SongLuaOverlayMessageCommand,
    SongLuaOverlayModelLayer, SongLuaOverlayState, SongLuaOverlayStateDelta, SongLuaTimeUnit,
    SongLuaTrackedActorTarget as TrackedCompileActorTarget,
    add_actor_child_from_path as add_host_actor_child_from_path,
    capture_stable_cross_actor_message_commands, compile_multitap_update_overlays_for_actors,
    compile_perframes, compile_update_functions, create_dummy_actor as create_host_dummy_actor,
    create_named_child_actor as create_host_named_child_actor, ensure_overlay_arrow_visual,
    entry_file_path, execute_script_file, install_actor_methods as install_host_actor_methods,
    install_compile_host, log_song_lua_compile_timing, merge_compile_info,
    note_field_column_actors as host_note_field_column_actors, push_startup_message_if_listened,
    push_unique_compile_detail, read_actor_model_layers, read_eases_for_overlay_actors,
    read_global_function_nested_tables, read_mod_windows, read_note_column_zoom_hides,
    read_noteskin_tap_actor_slots, read_overlay_compile_actor_actions, read_overlay_compile_actors,
    read_proxy_target_kind, read_runtime_mod_eases, read_song_lua_sound_paths,
    read_tracked_compile_actors, read_update_function_nested_tables, read_update_function_tables,
    read_xero_runtime_mod_eases_for_overlay_actors, register_loaded_easing_names,
    restore_compile_globals, run_actor_draw_functions, run_actor_init_commands,
    runtime_static_overlay_index_for_actors, snapshot_compile_globals, sort_compiled_song_lua,
    update_tree_reads_global,
};

const COMPILE_LAYER_KEY: &str = "__songlua_compile_layer";

type DefaultCompiledSongLua<NoteskinSlot, ModelVertex> = CompiledSongLua<
    SongLuaOverlayActor<SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>>,
>;

pub fn compile_song_lua_with_default_host<NoteskinSlot, ModelVertex, MultitapArrowVisualSpec>(
    entry_path: &Path,
    context: &SongLuaCompileContext,
    noteskin_resolver: SongLuaNoteskinResolver,
    read_model_slots: fn(&Path, &Path, &Path) -> Result<Arc<[NoteskinSlot]>, String>,
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

pub fn compile_song_lua_layers_with_default_host<
    NoteskinSlot,
    ModelVertex,
    MultitapArrowVisualSpec,
>(
    entry_paths: &[&Path],
    primary_index: usize,
    context: &SongLuaCompileContext,
    noteskin_resolver: SongLuaNoteskinResolver,
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
    compile_song_lua_layers_with_actors(
        entry_paths,
        primary_index,
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
    read_model_slots: fn(&Path, &Path, &Path) -> Result<Arc<[NoteskinSlot]>, String>,
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
    compile_song_lua_layers_with_actors(
        &[entry_path],
        0,
        context,
        noteskin_resolver,
        create_dummy_actor,
        create_named_child_actor,
        install_actor_methods,
        read_model_slots,
        model_layer_from_slot,
        multitap_arrow_visual_spec,
    )?
    .into_iter()
    .next()
    .ok_or_else(|| "song lua compiler returned no result for one entry".to_string())
}

pub fn compile_song_lua_layers_with_actors<NoteskinSlot, ModelVertex, MultitapArrowVisualSpec>(
    entry_paths: &[&Path],
    primary_index: usize,
    context: &SongLuaCompileContext,
    noteskin_resolver: SongLuaNoteskinResolver,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
    install_actor_methods: fn(&Lua, &Table) -> mlua::Result<()>,
    read_model_slots: fn(&Path, &Path, &Path) -> Result<Arc<[NoteskinSlot]>, String>,
    model_layer_from_slot: fn(&NoteskinSlot) -> Option<SongLuaOverlayModelLayer<ModelVertex>>,
    mut multitap_arrow_visual_spec: MultitapArrowVisualSpec,
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
    if entry_paths.is_empty() {
        return Ok(Vec::new());
    }
    if context.background_layer_count > entry_paths.len() {
        return Err("song lua background count exceeds its session entries".to_owned());
    }
    if primary_index >= entry_paths.len() {
        return Err(format!(
            "song lua primary index {primary_index} is outside {} entries",
            entry_paths.len()
        ));
    }
    let mut compile_timer = SongLuaCompileTimer::start();
    let entry_paths = entry_paths
        .iter()
        .map(|path| {
            entry_file_path(path)
                .ok_or_else(|| format!("song lua entry '{}' does not exist", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let trace_entry_path = entry_paths[primary_index].clone();
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
    #[cfg(feature = "test-support")]
    crate::lua_util::install_message_order(&lua, &context.message_actor_order)
        .map_err(|err| err.to_string())?;
    compile_timer.push_stage("host");
    let screen_layer_states =
        crate::lua_util::screen_layer_states(&lua).map_err(|err| err.to_string())?;
    let roots = lua.create_table().map_err(|err| err.to_string())?;
    let mut initial_actor_states = std::collections::HashMap::new();
    #[cfg(feature = "test-support")]
    lua.set_app_data(crate::song_tables::SongLuaSkinWrites::default());
    #[cfg(feature = "test-support")]
    lua.set_app_data(crate::song_tables::SongLuaBoolWrites::default());
    #[cfg(feature = "test-support")]
    lua.set_app_data(crate::song_tables::SongLuaSpeedWrites::default());
    for (index, entry_path) in entry_paths.iter().enumerate() {
        let root = execute_script_file(&lua, entry_path, context.song_dir.as_path())
            .map_err(|err| format!("failed to execute '{}': {err}", entry_path.display()))?;
        let parent = if let Value::Table(actor) = &root {
            #[cfg(feature = "test-support")]
            actor.raw_set("__songlua_message_path", (index + 1).to_string())
                .map_err(|err| err.to_string())?;
            let screen = lua
                .globals()
                .get::<Table>("__songlua_top_screen")
                .map_err(|err| err.to_string())?;
            let children = crate::actor_children(&lua, &screen).map_err(|err| err.to_string())?;
            let name = if index < context.background_layer_count {
                "SongBackground"
            } else {
                "SongForeground"
            };
            let parent = children.get::<Table>(name).map_err(|err| err.to_string())?;
            actor
                .set("__songlua_parent", parent.clone())
                .map_err(|err| err.to_string())?;
            Some(parent)
        } else {
            None
        };
        crate::lua_util::collect_initial_states(&root, &mut initial_actor_states)
            .map_err(|err| err.to_string())?;
        run_actor_init_commands(&lua, &root).map_err(|err| {
            format!(
                "failed to run actor init commands for '{}': {err}",
                entry_path.display()
            )
        })?;
        // MakeActor runs Init with the parent set; Foreground::LoadFromSong
        // adds the initialized actor to its children afterward.
        if let (Value::Table(actor), Some(parent)) = (&root, parent) {
            crate::lua_util::push_sequence_child_once(&parent, actor.clone())
                .map_err(|err| err.to_string())?;
        }
        roots
            .raw_set(index + 1, root)
            .map_err(|err| err.to_string())?;
    }
    let initialized_global_actions = lua
        .globals()
        .get::<Option<Table>>("mod_actions")
        .map_err(|err| err.to_string())?;
    let initialized_global_perframes = lua
        .globals()
        .get::<Option<Table>>("mod_perframes")
        .map_err(|err| err.to_string())?;
    let initialized_global_mods = lua
        .globals()
        .get::<Option<Table>>("mods")
        .map_err(|err| err.to_string())?;
    let initialized_global_mod_eases = lua
        .globals()
        .get::<Option<Table>>("mods_ease")
        .map_err(|err| err.to_string())?;
    compile_timer.push_stage("execute_init");
    let root = Value::Table(roots);
    // Startup queues and the initial update can consume one-shot broadcasts
    // before the sampled replay starts. Retain their events as well.
    crate::lua_util::begin_overlay_update_capture_from_indices(&lua, std::iter::empty());
    let captured_startup = run_actor_startup_commands(&lua, &root, initial_actor_states, context)
        .map_err(|err| {
        format!(
            "failed to run actor startup commands for song lua session '{}': {err}",
            trace_entry_path.display()
        )
    })?;
    #[cfg(feature = "test-support")]
    let startup_skin_writes = lua.remove_app_data::<crate::song_tables::SongLuaSkinWrites>()
        .map(|capture| capture.writes).unwrap_or_default();
    #[cfg(feature = "test-support")]
    let startup_bool_writes = lua.remove_app_data::<crate::song_tables::SongLuaBoolWrites>()
        .map(|capture| capture.0).unwrap_or_default();
    #[cfg(feature = "test-support")]
    let startup_speed_writes = lua.remove_app_data::<crate::song_tables::SongLuaSpeedWrites>()
        .map(|capture| capture.0).unwrap_or_default();
    let startup = captured_startup.options;
    let startup_states = captured_startup.queued;
    let mut startup_tweens = captured_startup.tweens;
    let mut initial_updates = captured_startup.updates;
    let initial_broadcasts = captured_startup.broadcasts;
    compile_timer.push_stage("startup_commands");
    let mut screen_layer_startup = crate::lua_util::capture_startup_states(screen_layer_states)?;
    crate::lua_util::bake_startup_tweens(
        startup_tweens
            .values_mut()
            .chain(screen_layer_startup.values_mut())
            .chain(initial_updates.values_mut()),
        crate::perframe::update_function_replay_beats(
            context,
            0.0,
            crate::perframe::update_function_end_beat(context),
        )
        .into_iter(),
    );
    compile_timer.push_stage("update_functions");
    run_actor_draw_functions(&lua, &root);
    compile_timer.push_stage("draw_functions");
    let mut startup_broadcasts = crate::lua_util::runtime_broadcast_captures(&lua);
    let mut startup_sounds = crate::lua_util::take_runtime_sounds(&lua);
    let mut judgment_textures = crate::lua_util::take_judgment_textures(&lua);
    let mut sprite_textures = crate::lua_util::take_sprite_textures(&lua);
    crate::lua_util::end_overlay_update_capture(&lua);
    // Actor::UpdateTweening needs positive delta. Match the first-frame clock
    // used when restoring the state before queued startup commands.
    let startup_beat = crate::song_beat_at_elapsed_seconds(
        1.0 / crate::perframe::SONG_LUA_UPDATE_REFERENCE_FPS,
        context,
    );
    for (beat, _, _) in &mut startup_broadcasts {
        *beat = startup_beat;
    }
    for event in &mut startup_sounds {
        if event.queued_startup {
            event.beat = startup_beat;
        }
    }
    register_loaded_easing_names(&lua, &mut host).map_err(|err| err.to_string())?;
    compile_timer.push_stage("easing_names");
    mark_compile_layers(&root).map_err(|err| err.to_string())?;

    let globals = lua.globals();
    let mut out = CompiledSongLua {
        startup,
        entry_path: entry_paths[primary_index].clone(),
        screen_width: context.screen_width,
        screen_height: context.screen_height,
        ..CompiledSongLua::default()
    };
    #[cfg(feature = "test-support")]
    {
        out.noteskin_writes = startup_skin_writes;
        out.boolean_writes = startup_bool_writes;
        out.speed_writes = startup_speed_writes;
    }
    // Real frame-zero broadcasts survive separately from discovery events.
    // Only the latter are replaced by their chronological queue dispatch.
    merge_runtime_messages(&mut out.messages, 0, &initial_broadcasts);
    let runtime_message_start = out.messages.len();
    merge_runtime_messages(
        &mut out.messages,
        runtime_message_start,
        &startup_broadcasts,
    );
    let compile_globals =
        snapshot_compile_globals(&lua, &globals).map_err(|err| err.to_string())?;
    let overlays = read_overlay_compile_actors(
        &lua,
        &root,
        context,
        |actor| read_actor_model_layers(actor, read_model_slots, model_layer_from_slot),
        |actor, _context| read_noteskin_tap_actor_slots(actor, read_model_slots),
        |skipped| {
            push_unique_compile_detail(&mut out.info.skipped_message_command_captures, skipped);
        },
    );
    restore_compile_globals(&globals, compile_globals).map_err(|err| err.to_string())?;
    let mut overlays = overlays?;
    let screen_index = overlays.len();
    let screen = globals
        .get::<Table>("__songlua_top_screen")
        .map_err(|err| err.to_string())?;
    screen
        .set(COMPILE_LAYER_KEY, primary_index)
        .map_err(|err| err.to_string())?;
    overlays.push(crate::lua_util::SongLuaOverlayCompileActor {
        actor: SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorFrame,
            name: Some("ScreenGameplay".to_owned()),
            parent_index: None,
            initial_state: crate::lua_util::actor_overlay_initial_state(&screen)?,
            message_commands: Vec::new(),
        },
        table: screen,
        message_sounds: Vec::new(),
    });
    out.screen_overlay_index = Some(screen_index);
    capture_stable_cross_actor_message_commands(&lua, &mut overlays, |skipped| {
        push_unique_compile_detail(&mut out.info.skipped_message_command_captures, skipped);
    })?;
    let mut message_sounds = Vec::new();
    for overlay in &overlays {
        let layer = overlay
            .table
            .get::<Option<usize>>(COMPILE_LAYER_KEY)
            .map_err(|err| err.to_string())?
            .unwrap_or(primary_index);
        message_sounds.extend(
            overlay
                .message_sounds
                .iter()
                .cloned()
                .map(|(message, path)| (layer, message, path)),
        );
    }
    compile_timer.push_stage("read_overlays");
    let mut tracked_actors = read_tracked_compile_actors(&lua, create_named_child_actor)?;
    let mut hidden_players = std::array::from_fn(|player| {
        tracked_actors
            .get(player)
            .is_some_and(|tracked| !tracked.actor.initial_state.visible)
    });
    let mut overlay_trigger_counter = 0usize;
    let mut sound_events = startup_sounds;
    let prefix_is_runtime =
        update_tree_reads_global(&lua, &root, "prefix_globals").map_err(|err| err.to_string())?;
    let prefix_perframes = globals
        .get::<Option<Table>>("prefix_globals")
        .map_err(|err| err.to_string())?
        .and_then(|table| table.get::<Option<Table>>("perframes").ok().flatten());
    let global_perframes = globals
        .get::<Option<Table>>("mod_perframes")
        .map_err(|err| err.to_string())?;
    let runtime_perframe_tables =
        read_update_function_tables(&lua, &root, &["mod_perframes", "perframes"])?;
    let update_reads_global_perframes =
        update_tree_reads_global(&lua, &root, "mod_perframes").map_err(|err| err.to_string())?;
    let global_perframes_are_runtime = global_perframes.as_ref().is_some_and(|global| {
        runtime_perframe_tables
            .iter()
            .any(|runtime| runtime.to_pointer() == global.to_pointer())
            || (update_reads_global_perframes && initialized_global_perframes.is_some())
    });
    compile_timer.push_stage("read_globals");

    if !prefix_is_runtime
        && let Some(prefix_globals) = globals
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
            &mut sound_events,
            &mut overlay_trigger_counter,
            &mut out.info,
        )?;
        compile_timer.push_stage("prefix_actions");
    }

    let global_mods = globals
        .get::<Option<Table>>("mods")
        .map_err(|err| err.to_string())?;
    let runtime_mod_tables = read_update_function_tables(&lua, &root, &["mods"])?;
    let update_reads_global_mods =
        update_tree_reads_global(&lua, &root, "mods").map_err(|err| err.to_string())?;
    let global_mods_are_runtime = global_mods.as_ref().is_some_and(|global| {
        runtime_mod_tables
            .iter()
            .any(|runtime| runtime.to_pointer() == global.to_pointer())
            || (update_reads_global_mods && initialized_global_mods.is_some())
    });
    // Recurring Lua owns stateful starts and the easing function body. Capture
    // its actual writes instead of duplicating them with endpoint-based eases.
    if !global_mods_are_runtime {
        out.beat_mods.extend(read_mod_windows(
            global_mods.clone(),
            SongLuaTimeUnit::Beat,
        )?);
        let (runtime_eases, runtime_overlay_eases) = read_runtime_mod_eases(
            global_mods.clone(),
            &host.easing_names,
            runtime_static_overlay_index_for_actors(&overlays),
            context,
        )?;
        out.eases.extend(runtime_eases);
        out.overlay_eases.extend(runtime_overlay_eases);
    }
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
    let global_mod_eases = globals
        .get::<Option<Table>>("mods_ease")
        .map_err(|err| err.to_string())?;
    let runtime_mod_ease_tables = read_update_function_tables(&lua, &root, &["mods_ease"])?;
    let update_reads_global_mod_eases =
        update_tree_reads_global(&lua, &root, "mods_ease").map_err(|err| err.to_string())?;
    let global_mod_eases_are_runtime = global_mod_eases.as_ref().is_some_and(|global| {
        runtime_mod_ease_tables
            .iter()
            .any(|runtime| runtime.to_pointer() == global.to_pointer())
            || (update_reads_global_mod_eases && initialized_global_mod_eases.is_some())
    });
    // The recurring reader owns every callback in its ease table. Probing
    // endpoints first mutates shared upvalues and bypasses its write order;
    // table setters also appear unsupported despite being captured in replay.
    if !global_mod_eases_are_runtime {
        let (eases, overlay_eases, column_offsets, info) = read_eases_for_overlay_actors(
            &lua,
            global_mod_eases,
            SongLuaTimeUnit::Beat,
            &host.easing_names,
            &mut overlays,
        )?;
        out.eases.extend(eases);
        out.overlay_eases.extend(overlay_eases);
        out.column_offsets.extend(column_offsets);
        merge_compile_info(&mut out.info, info);
    }
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
    let global_actions = globals
        .get::<Option<Table>>("mod_actions")
        .map_err(|err| err.to_string())?;
    let runtime_action_tables =
        read_update_function_tables(&lua, &root, &["mod_actions", "actions"])?;
    let update_reads_global_actions =
        update_tree_reads_global(&lua, &root, "mod_actions").map_err(|err| err.to_string())?;
    let global_actions_are_runtime = global_actions.as_ref().is_some_and(|global| {
        runtime_action_tables
            .iter()
            .any(|runtime| runtime.to_pointer() == global.to_pointer())
            || (update_reads_global_actions && initialized_global_actions.is_some())
    });
    if !global_actions_are_runtime {
        read_overlay_compile_actor_actions(
            &lua,
            global_actions,
            &mut overlays,
            &mut tracked_actors,
            &mut out.messages,
            &mut sound_events,
            &mut overlay_trigger_counter,
            &mut out.info,
        )?;
    }
    // The sampled reader owns callbacks and named actions. Probing callbacks
    // changes nested tables, upvalues and random state; emitting named actions
    // speculatively also plays schedules the reader never reaches.
    compile_timer.push_stage("global_actions");
    crate::perframe::apply_startup_states(
        context,
        &mut overlays,
        &startup_states,
        &mut out.messages,
    );
    crate::perframe::apply_startup_tweens(&mut overlays, &startup_tweens, &mut out.messages);
    crate::perframe::apply_initial_updates(&mut overlays, &initial_updates);
    let (perframe_eases, perframe_overlay_eases, perframe_info) = compile_perframes(
        &lua,
        if prefix_is_runtime {
            None
        } else {
            prefix_perframes
        },
        if global_perframes_are_runtime {
            None
        } else {
            global_perframes
        },
        context,
        &mut overlays,
        &tracked_actors,
        &out.messages,
    )?;
    out.eases.extend(perframe_eases);
    out.overlay_eases.extend(perframe_overlay_eases);
    merge_compile_info(&mut out.info, perframe_info);
    compile_timer.push_stage("perframes");
    out.note_hides = read_note_column_zoom_hides(&lua)?;
    compile_timer.push_stage("note_hides");
    crate::multitap::install_multitap_hits(&lua, context, &mut overlays)?;
    crate::model_texture::install(&lua, &overlays, model_layer_from_slot);
    let (
        update_eases,
        update_overlay_eases,
        update_overlay_tracks,
        update_column_transforms,
        stateful_message_captures,
        runtime_broadcasts,
    ) = match if crate::model_texture::active(&lua) {
        // Native material clocks depend on the original actor update tree.
        // Retain chronological replay instead of replacing it with curves.
        None
    } else {
        compile_multitap_update_overlays_for_actors(
            &lua,
            &root,
            context,
            &mut overlays,
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
        )?
    } {
        Some(eases) => (
            Vec::new(),
            eases,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ),
        None => {
            // Run the reader intact: its parser, easing parameters and write
            // order determine the targets actually sent to PlayerOptions.
            compile_update_functions(
                &lua,
                &root,
                context,
                &mut overlays,
                &mut tracked_actors,
                &out.messages,
                &startup_tweens,
                &mut sound_events,
                &mut judgment_textures,
                &mut sprite_textures,
                &mut out.column_splines,
            )?
        }
    };
    out.eases.extend(update_eases);
    out.overlay_eases.extend(update_overlay_eases);
    out.overlay_update_unit = if context.song_timing.is_some() {
        crate::SongLuaTimeUnit::Second
    } else {
        crate::SongLuaTimeUnit::Beat
    };
    out.overlay_updates.extend(update_overlay_tracks);
    #[cfg(feature = "test-support")]
    if let Some(writes) = lua.remove_app_data::<crate::song_tables::SongLuaBoolWrites>() {
        out.boolean_writes.extend(writes.0);
    }
    #[cfg(feature = "test-support")]
    if let Some(writes) = lua.remove_app_data::<crate::song_tables::SongLuaSpeedWrites>() {
        out.speed_writes.extend(writes.0);
    }
    #[cfg(feature = "test-support")]
    if let Some(writes) = lua.remove_app_data::<crate::song_tables::SongLuaSkinWrites>() {
        out.noteskin_writes.extend(writes.writes);
    }
    #[cfg(feature = "test-support")]
    if let Some(writes) = lua.remove_app_data::<crate::lua_util::SongLuaOverlayWrites>() {
        out.overlay_writes = writes.0;
    }
    merge_runtime_messages(
        &mut out.messages,
        runtime_message_start,
        &runtime_broadcasts,
    );
    for capture in stateful_message_captures {
        out.info.skipped_message_command_captures.retain(|detail| {
            !detail.contains(&format!(
                ".{}MessageCommand changes cross-actor targets or effects",
                capture.message
            ))
        });
        out.stateful_message_captures.push(capture);
    }
    out.column_offsets.extend(update_column_transforms);
    out.column_spline_origin = context
        .song_timing
        .as_ref()
        .map(|timing| timing.get_time_for_beat_exact(0.0));
    if let Some(errors) = lua.remove_app_data::<crate::lua_util::SongLuaUpdateErrors>() {
        out.info.unsupported_perframes += errors.0.len();
        for detail in errors.0 {
            push_unique_compile_detail(&mut out.info.unsupported_perframe_captures, detail);
        }
    }
    compile_timer.push_stage("update_overlays");
    for (index, message, path) in crate::lua_util::capture_deferred_messages(
        &lua,
        &mut overlays,
        &mut tracked_actors,
        &out.stateful_message_captures,
        &mut out.info.skipped_message_command_captures,
    )? {
        let layer = overlays[index]
            .table
            .get::<Option<usize>>(COMPILE_LAYER_KEY)
            .map_err(|err| err.to_string())?
            .unwrap_or(primary_index);
        message_sounds.push((layer, message, path));
    }
    compile_timer.push_stage("deferred_messages");
    for overlay in &mut overlays {
        for command in &mut overlay.actor.message_commands {
            if runtime_broadcasts
                .iter()
                .any(|(_, message, _)| message == &command.message)
            {
                command.blocks.retain(|block| !block.queued);
            }
        }
    }
    resolve_late_actor_targets(&mut overlays, &mut hidden_players)?;
    for overlay in &mut overlays {
        if matches!(
            overlay.actor.kind,
            SongLuaOverlayKind::ActorFrameTexture { .. }
        ) {
            overlay.actor.kind = crate::lua_util::read_aft_kind(&overlay.table)?;
        } else if matches!(overlay.actor.kind, SongLuaOverlayKind::Actor)
            && crate::lua_util::actor_type_is(&overlay.table, "Sprite")
                .map_err(|err| err.to_string())?
            && let Some(capture_name) = overlay
                .table
                .raw_get::<Option<String>>("__songlua_aft_capture_name")
                .map_err(|err| err.to_string())?
        {
            overlay.actor.kind = SongLuaOverlayKind::AftSprite { capture_name };
        }
    }
    for overlay in &mut overlays {
        if let SongLuaOverlayKind::Sprite { textures, .. } = &mut overlay.actor.kind {
            let pointer = overlay.table.to_pointer() as usize;
            let mut bindings = sprite_textures
                .iter()
                .filter(|(actor, _)| *actor == pointer)
                .map(|(_, texture)| texture.clone())
                .collect::<Vec<_>>();
            bindings.sort_by(|a, b| a.second.total_cmp(&b.second));
            *textures = bindings.into();
        }
    }
    let startup_seconds = 1.0 / crate::perframe::SONG_LUA_UPDATE_REFERENCE_FPS;
    let startup_time = context.song_timing.as_ref().map_or_else(
        || crate::song_beat_at_elapsed_seconds(startup_seconds, context),
        |timing| {
            timing.get_time_for_beat_exact(0.0) + startup_seconds * crate::song_music_rate(context)
        },
    );
    for track in &mut out.overlay_updates {
        if startup_states.contains_key(&(overlays[track.overlay_index].table.to_pointer() as usize))
        {
            // Zero-time callbacks run after queued startup; retain the state
            // before setup until the first positive update frame.
            for sample in track
                .samples
                .iter_mut()
                .take_while(|sample| sample.time < startup_time)
            {
                sample.time = startup_time;
            }
        }
    }
    crate::perframe::apply_layer_startup(
        &mut tracked_actors,
        &screen_layer_startup,
        &mut out.messages,
    );
    for overlay in &mut overlays {
        overlay.actor.initial_state.spin_baked = crate::lua_util::spin_render_pose(&overlay.table)
            .map_err(|err| err.to_string())?
            .is_some();
    }
    for tracked in &mut tracked_actors {
        tracked.actor.initial_state.spin_baked = crate::lua_util::spin_render_pose(&tracked.table)
            .map_err(|err| err.to_string())?
            .is_some();
    }
    push_startup_message_if_listened(
        &mut out.messages,
        overlays
            .iter()
            .map(|overlay| overlay.actor.message_commands.as_slice()),
    );
    // Leave ordinary song trees unchanged when no script touches the screen.
    if overlays[screen_index].actor.initial_state == crate::SongLuaOverlayState::default()
        && overlays[screen_index].actor.message_commands.is_empty()
        && !out
            .overlay_eases
            .iter()
            .any(|ease| ease.overlay_index == screen_index)
        && !out
            .overlay_updates
            .iter()
            .any(|track| track.overlay_index == screen_index)
    {
        overlays.remove(screen_index);
        out.screen_overlay_index = None;
        let remap = |index: &mut usize| {
            if *index > screen_index {
                *index -= 1;
            }
        };
        for overlay in &mut overlays {
            if let Some(parent) = &mut overlay.actor.parent_index {
                remap(parent);
            }
            if let SongLuaOverlayKind::ActorProxy {
                target: crate::SongLuaProxyTarget::Actor { overlay_index },
            } = &mut overlay.actor.kind
            {
                remap(overlay_index);
            }
        }
        for ease in &mut out.overlay_eases {
            remap(&mut ease.overlay_index);
        }
        for track in &mut out.overlay_updates {
            remap(&mut track.overlay_index);
        }
        #[cfg(feature = "test-support")]
        for track in &mut out.overlay_writes {
            remap(&mut track.overlay_index);
        }
        for capture in &mut out.stateful_message_captures {
            for (index, _) in &mut capture.overlay_targets {
                remap(index);
            }
            for write in &mut capture.writes {
                remap(&mut write.overlay_index);
            }
        }
    }
    crate::model_texture::take(&lua, &mut overlays);
    let draw_frames = crate::draw_capture::take(&lua, &overlays, &tracked_actors)?;
    let mut overlay_layers = overlays
        .iter()
        .map(|overlay| {
            overlay
                .table
                .get::<Option<usize>>(COMPILE_LAYER_KEY)
                .map_err(|err| err.to_string())
                .map(|index| index.unwrap_or(primary_index))
        })
        .collect::<Result<Vec<_>, _>>()?;
    out.overlays = overlays.into_iter().map(|overlay| overlay.actor).collect();
    out.draw_frames = draw_frames.into_iter().map(|(_, frame)| frame).collect();
    for (layer, message, sound_path) in message_sounds {
        // Runtime listener calls were captured at their actual frame. Do not
        // also play the speculative sound attached to the named message.
        if sound_events
            .iter()
            .any(|event| event.source_message.as_deref() == Some(message.as_str()))
        {
            continue;
        }
        out.overlays.push(SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Sound { sound_path },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: vec![SongLuaOverlayMessageCommand {
                frame_advance: 0.0,
                message,
                blocks: vec![SongLuaOverlayCommandBlock {
                    progress: None,
                    queued: false,
                    start: 0.0,
                    duration: 0.0,
                    easing: None,
                    opt1: None,
                    opt2: None,
                    delta: SongLuaOverlayStateDelta {
                        sound_play: Some(true),
                        ..SongLuaOverlayStateDelta::default()
                    },
                }],
            }],
        });
        overlay_layers.push(layer);
    }
    for (index, event) in sound_events.into_iter().enumerate() {
        let message = format!("__songlua_sound_call_{index}");
        out.messages.push(crate::SongLuaMessageEvent {
            beat: event.beat,
            message: message.clone(),
            persists: false,
        });
        out.overlays.push(SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Sound {
                sound_path: event.path,
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: vec![SongLuaOverlayMessageCommand {
                frame_advance: 0.0,
                message,
                blocks: vec![SongLuaOverlayCommandBlock {
                    progress: None,
                    queued: false,
                    start: 0.0,
                    duration: 0.0,
                    easing: None,
                    opt1: None,
                    opt2: None,
                    delta: SongLuaOverlayStateDelta {
                        sound_play: Some(true),
                        ..SongLuaOverlayStateDelta::default()
                    },
                }],
            }],
        });
        overlay_layers.push(primary_index);
    }
    for tracked in tracked_actors {
        match tracked.target {
            TrackedCompileActorTarget::Player(player) => out.player_actors[player] = tracked.actor,
            TrackedCompileActorTarget::PlayerJudgment(player) => {
                out.player_actors[player].judgment = SongLuaCapturedChildActor {
                    initial_state: tracked.actor.initial_state,
                    message_commands: tracked.actor.message_commands,
                    textures: {
                        let mut textures = judgment_textures
                            .iter()
                            .filter(|(index, _)| *index == player)
                            .map(|(_, texture)| texture.clone())
                            .collect::<Vec<_>>();
                        textures.sort_by(|a, b| a.second.total_cmp(&b.second));
                        textures.dedup_by(|a, b| a.path == b.path);
                        textures
                    },
                };
            }
            TrackedCompileActorTarget::PlayerCombo(player) => {
                out.player_actors[player].combo = SongLuaCapturedChildActor {
                    initial_state: tracked.actor.initial_state,
                    message_commands: tracked.actor.message_commands,
                    ..SongLuaCapturedChildActor::default()
                };
            }
            TrackedCompileActorTarget::SongForeground => out.song_foreground = tracked.actor,
            TrackedCompileActorTarget::ScreenLayer(index) => {
                out.screen_layers[index] = tracked.actor
            }
        }
    }
    out.hidden_players = hidden_players;

    sort_compiled_song_lua(&mut out);
    out.sound_paths = read_song_lua_sound_paths(&lua)?;
    let mut seen = out
        .sound_paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    crate::push_song_lua_overlay_sound_paths(&out.overlays, &mut seen, &mut out.sound_paths);
    compile_timer.push_stage("finalize");
    log_song_lua_compile_timing(&trace_entry_path, &compile_timer);
    split_compiled_song_lua(out, overlay_layers, &entry_paths, primary_index)
}

fn merge_runtime_messages(
    messages: &mut Vec<SongLuaMessageEvent>,
    action_start: usize,
    runtime_broadcasts: &[(f32, String, bool)],
) {
    let action_end = messages.len();
    let mut matched = vec![false; action_end - action_start];
    for (runtime_beat, runtime_message, _) in runtime_broadcasts {
        let Some(index) = messages
            .iter()
            .enumerate()
            .take(action_end)
            .skip(action_start)
            .filter(|(index, event)| {
                !matched[*index - action_start]
                    && !event.message.starts_with("__songlua_overlay_fn_action_")
                    && event.message == *runtime_message
                    && event.beat <= *runtime_beat + f32::EPSILON
            })
            .max_by(|(_, left), (_, right)| left.beat.total_cmp(&right.beat))
            .map(|(index, _)| index)
        else {
            // MessageManager::Broadcast dispatches every runtime call, including
            // schedules stored under local or chart-specific table names. The
            // sampled tracks retain parameter-dependent listener writes.
            messages.push(SongLuaMessageEvent {
                beat: *runtime_beat,
                message: runtime_message.clone(),
                persists: false,
            });
            continue;
        };
        messages[index].beat = *runtime_beat;
        matched[index - action_start] = true;
    }
}

fn resolve_late_actor_targets<NoteskinSlot, ModelVertex>(
    overlays: &mut [crate::SongLuaOverlayCompileActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
    hidden_players: &mut [bool; crate::LUA_PLAYERS],
) -> Result<(), String> {
    let actor_indices = overlays
        .iter()
        .enumerate()
        .map(|(index, overlay)| (overlay.table.to_pointer() as usize, index))
        .collect::<std::collections::HashMap<_, _>>();
    for overlay in overlays {
        if matches!(overlay.actor.kind, SongLuaOverlayKind::Actor)
            && overlay
                .table
                .get::<Option<String>>("__songlua_actor_type")
                .map_err(|err| err.to_string())?
                .is_some_and(|kind| kind.eq_ignore_ascii_case("Sprite"))
        {
            if let Some(capture_name) = crate::lua_util::actor_aft_capture_name(&overlay.table)
                .map_err(|err| err.to_string())?
            {
                overlay.actor.kind = SongLuaOverlayKind::AftSprite { capture_name };
            } else if let Some(texture) = overlay
                .table
                .get::<Option<String>>("Texture")
                .map_err(|err| err.to_string())?
            {
                let texture_path =
                    crate::lua_util::resolve_actor_asset_path(&overlay.table, &texture)?;
                overlay.actor.kind = SongLuaOverlayKind::Sprite {
                    texture_key: std::sync::Arc::from(texture_path.to_string_lossy().into_owned()),
                    texture_path,
                    states: crate::lua_util::read_sprite_states(&overlay.table)?.into(),
                    textures: [].into(),
                };
            }
        }
        let SongLuaOverlayKind::ActorProxy { target } = &mut overlay.actor.kind else {
            continue;
        };
        let Some(mut resolved) = read_proxy_target_kind(&overlay.table)? else {
            continue;
        };
        if let crate::SongLuaProxyTarget::Actor { overlay_index } = &mut resolved {
            *overlay_index = actor_indices
                .get(overlay_index)
                .copied()
                .unwrap_or(usize::MAX);
        }
        if let crate::SongLuaProxyTarget::Player { player_index } = resolved {
            let target_actor = overlay
                .table
                .get::<Option<Table>>("__songlua_proxy_target_actor")
                .map_err(|err| err.to_string())?;
            let target_hidden = target_actor
                .and_then(|actor| {
                    actor
                        .get::<Option<bool>>("__songlua_visible")
                        .ok()
                        .flatten()
                })
                .is_some_and(|visible| !visible);
            if target_hidden {
                if let Some(hidden) = hidden_players.get_mut(player_index) {
                    *hidden = true;
                }
            }
        }
        *target = resolved;
    }
    Ok(())
}

fn mark_compile_layers(root: &Value) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    for (index, actor) in root.sequence_values::<Value>().enumerate() {
        mark_actor_layer(&actor?, index)?;
    }
    Ok(())
}

fn mark_actor_layer(actor: &Value, index: usize) -> mlua::Result<()> {
    let Value::Table(actor) = actor else {
        return Ok(());
    };
    actor.set(COMPILE_LAYER_KEY, index)?;
    if let Some(wrappers) = actor.get::<Option<Table>>("__songlua_wrappers")? {
        for wrapper in wrappers.sequence_values::<Value>() {
            mark_actor_layer(&wrapper?, index)?;
        }
    }
    for child in actor.sequence_values::<Value>() {
        mark_actor_layer(&child?, index)?;
    }
    Ok(())
}

// Preserve an actor's original sibling position when later wrappers surround it.
// This load-time traversal emits parents first, as composition and camera scopes
// require, and rejects cycles instead of leaving forward parents unresolved.
fn wrapper_overlay_order<NoteskinSlot, ModelVertex>(
    overlays: &[SongLuaOverlayActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
) -> Result<Vec<usize>, String> {
    if overlays
        .iter()
        .enumerate()
        .all(|(index, overlay)| overlay.parent_index.is_none_or(|parent| parent < index))
    {
        return Ok((0..overlays.len()).collect());
    }
    let count = overlays.len();
    let mut children = vec![Vec::new(); count + 1];
    for (index, overlay) in overlays.iter().enumerate() {
        let parent = overlay.parent_index.map_or(0, |parent| parent + 1);
        let Some(list) = children.get_mut(parent) else {
            return Err("Runtime wrapper has an invalid parent".into());
        };
        list.push(index);
    }
    let mut source = overlays
        .iter()
        .enumerate()
        .map(|(index, overlay)| {
            if matches!(overlay.kind, SongLuaOverlayKind::WrapperState) {
                usize::MAX
            } else {
                index
            }
        })
        .collect::<Vec<_>>();
    let mut chain = Vec::new();
    for index in 0..count {
        chain.clear();
        let mut next = index;
        while source[next] == usize::MAX {
            if chain.len() >= count {
                return Err("Cycle in runtime wrapper ownership".into());
            }
            chain.push(next);
            let Some(&child) = children[next + 1].first() else {
                source[next] = next;
                break;
            };
            next = child;
        }
        for index in chain.drain(..) {
            source[index] = source[next];
        }
    }
    for list in &mut children {
        list.sort_by_key(|&index| source[index]);
    }
    let mut order = Vec::with_capacity(count);
    let mut pending = children[0].iter().rev().copied().collect::<Vec<_>>();
    while let Some(index) = pending.pop() {
        if order.len() >= count {
            return Err("Cycle in runtime wrapper hierarchy".into());
        }
        order.push(index);
        pending.extend(children[index + 1].iter().rev().copied());
    }
    if order.len() != count {
        return Err("Runtime wrapper hierarchy has no reachable root".into());
    }
    Ok(order)
}

fn split_compiled_song_lua<NoteskinSlot, ModelVertex>(
    mut compiled: DefaultCompiledSongLua<NoteskinSlot, ModelVertex>,
    overlay_layers: Vec<usize>,
    entry_paths: &[std::path::PathBuf],
    primary_index: usize,
) -> Result<Vec<DefaultCompiledSongLua<NoteskinSlot, ModelVertex>>, String> {
    if overlay_layers.len() != compiled.overlays.len() {
        return Err("song lua overlay ownership did not match compiled overlays".to_string());
    }
    let overlay_order = wrapper_overlay_order(&compiled.overlays)?;
    let mut local_counts = vec![0usize; entry_paths.len()];
    let mut overlay_map = vec![(0usize, 0usize); compiled.overlays.len()];
    for &index in &overlay_order {
        let layer = overlay_layers[index];
        if layer >= local_counts.len() {
            return Err(format!("song lua overlay has invalid layer index {layer}"));
        }
        overlay_map[index] = (layer, local_counts[layer]);
        local_counts[layer] += 1;
    }
    let mut outputs = entry_paths
        .iter()
        .map(|entry_path| DefaultCompiledSongLua {
            entry_path: entry_path.clone(),
            screen_width: compiled.screen_width,
            screen_height: compiled.screen_height,
            overlay_update_unit: compiled.overlay_update_unit,
            messages: compiled.messages.clone(),
            sound_paths: compiled.sound_paths.clone(),
            ..DefaultCompiledSongLua::default()
        })
        .collect::<Vec<_>>();

    if let Some(index) = compiled.screen_overlay_index {
        let (layer, local) = overlay_map[index];
        outputs[layer].screen_overlay_index = Some(local);
    }

    for mut frame in compiled.draw_frames.drain(..) {
        let owner = *frame
            .owners
            .first()
            .ok_or("custom draw frame has no owner")?;
        let (layer, _) = overlay_map[owner];
        let remap = |index: &mut usize| -> Result<(), String> {
            let &(target_layer, local) = overlay_map
                .get(*index)
                .ok_or("custom draw has an invalid overlay index")?;
            if target_layer != layer {
                return Err("custom draw crosses separately played song layers".to_owned());
            }
            *index = local;
            Ok(())
        };
        for owner in &mut frame.owners {
            remap(owner)?;
        }
        for op in &mut frame.ops {
            match op {
                crate::SongLuaDrawOp::Begin { capture, .. }
                | crate::SongLuaDrawOp::Finish { capture } => remap(capture)?,
                crate::SongLuaDrawOp::Draw {
                    source: crate::SongLuaDrawSource::Overlay(index),
                    ..
                } => remap(index)?,
                _ => {}
            }
        }
        let frames = &mut outputs[layer].draw_frames;
        if let Some(last) = frames.last_mut().filter(|last| last.second == frame.second) {
            last.owner_ends
                .extend(frame.owner_ends.iter().map(|end| last.ops.len() + end));
            last.owners.extend(frame.owners);
            last.ops.extend(frame.ops);
        } else {
            frames.push(frame);
        }
    }

    let mut overlays = compiled.overlays.drain(..).map(Some).collect::<Vec<_>>();
    for global_index in overlay_order {
        // The validated traversal visits each owned node exactly once.
        let mut overlay = overlays[global_index].take().expect("unique overlay traversal");
        let (layer, _) = overlay_map[global_index];
        overlay.parent_index = overlay.parent_index.and_then(|parent| {
            overlay_map
                .get(parent)
                .filter(|(parent_layer, _)| *parent_layer == layer)
                .map(|(_, local)| *local)
        });
        if let SongLuaOverlayKind::ActorProxy {
            target: crate::SongLuaProxyTarget::Actor { overlay_index },
        } = &mut overlay.kind
        {
            *overlay_index = overlay_map
                .get(*overlay_index)
                .filter(|(target_layer, _)| *target_layer == layer)
                .map_or(usize::MAX, |(_, local)| *local);
        }
        outputs[layer].overlays.push(overlay);
    }
    for mut ease in compiled.overlay_eases.drain(..) {
        let Some(&(layer, local)) = overlay_map.get(ease.overlay_index) else {
            continue;
        };
        ease.overlay_index = local;
        outputs[layer].overlay_eases.push(ease);
    }
    for mut update in compiled.overlay_updates.drain(..) {
        let Some(&(layer, local)) = overlay_map.get(update.overlay_index) else {
            continue;
        };
        update.overlay_index = local;
        outputs[layer].overlay_updates.push(update);
    }
    #[cfg(feature = "test-support")]
    for mut write in compiled.overlay_writes.drain(..) {
        let Some(&(layer, local)) = overlay_map.get(write.overlay_index) else {
            continue;
        };
        write.overlay_index = local;
        outputs[layer].overlay_writes.push(write);
    }
    for capture in compiled.stateful_message_captures.drain(..) {
        let mut targets_by_layer = vec![Vec::new(); outputs.len()];
        for (global, targets) in capture.overlay_targets {
            let Some(&(layer, local)) = overlay_map.get(global) else {
                continue;
            };
            targets_by_layer[layer].push((local, targets));
        }
        let mut writes_by_layer = vec![Vec::new(); outputs.len()];
        for mut write in capture.writes {
            let Some(&(layer, local)) = overlay_map.get(write.overlay_index) else {
                continue;
            };
            write.overlay_index = local;
            writes_by_layer[layer].push(write);
        }
        for (layer, overlay_targets) in targets_by_layer.into_iter().enumerate() {
            if overlay_targets.is_empty() {
                continue;
            }
            outputs[layer]
                .stateful_message_captures
                .push(crate::SongLuaStatefulMessageCapture {
                    message: capture.message.clone(),
                    overlay_targets,
                    writes: std::mem::take(&mut writes_by_layer[layer]),
                });
        }
    }

    let primary = &mut outputs[primary_index];
    #[cfg(feature = "test-support")]
    {
        primary.boolean_writes = compiled.boolean_writes;
        primary.speed_writes = compiled.speed_writes;
        primary.noteskin_writes = compiled.noteskin_writes;
    }
    primary.startup = compiled.startup;
    primary.beat_mods = compiled.beat_mods;
    primary.time_mods = compiled.time_mods;
    primary.eases = compiled.eases;
    primary.player_actors = compiled.player_actors;
    primary.song_foreground = compiled.song_foreground;
    primary.screen_layers = compiled.screen_layers;
    primary.hidden_players = compiled.hidden_players;
    primary.note_hides = compiled.note_hides;
    primary.column_offsets = compiled.column_offsets;
    primary.column_splines = compiled.column_splines;
    primary.column_spline_origin = compiled.column_spline_origin;
    primary.info = compiled.info;
    for output in &mut outputs {
        sort_compiled_song_lua(output);
    }
    Ok(outputs)
}
