pub(crate) use crate::compiled_layers::{merge_runtime_messages, split_compiled_song_lua};
#[cfg(test)]
use crate::compiled_layers::wrapper_overlay_order;
use crate::lua_util::run_actor_startup_commands;
use deadlib_present::actors::TextAttribute;
use mlua::{Lua, Table, Value};
use std::path::Path;
use std::sync::Arc;

use crate::{
    SongLuaCapturedChildActor, SongLuaCompileContext, SongLuaCompileTimer,
    SongLuaHostState, SongLuaMessageEvent, SongLuaNoteskinResolver, SongLuaOverlayActor,
    SongLuaOverlayCommandBlock, SongLuaOverlayKind, SongLuaOverlayMessageCommand,
    SongLuaOverlayModelLayer, SongLuaOverlayState, SongLuaOverlayStateDelta, SongLuaTimeUnit,
    SongLuaTrackedActorTarget as TrackedCompileActorTarget,
    add_actor_child_from_path as add_host_actor_child_from_path,
    capture_stable_cross_actor_message_commands, compile_multitap_update_overlays_for_actors,
    compile_perframes, compile_update_functions, create_dummy_actor as create_host_dummy_actor,
    create_named_child_actor as create_host_named_child_actor, ensure_overlay_arrow_visual,
    entry_file_path, install_actor_methods as install_host_actor_methods,
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

type CompiledSongLua<A> = crate::CompiledSongLuaData<A, crate::live::SongLuaLiveScene>;
type DefaultCompiledSongLua<NoteskinSlot, ModelVertex, LiveScene = crate::live::SongLuaLiveScene> = crate::CompiledSongLuaData<
    SongLuaOverlayActor<SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>>,
    LiveScene,
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
        None,
        noteskin_resolver,
        create_default_dummy_actor,
        create_default_named_child_actor,
        install_default_actor_methods,
        read_model_slots,
        model_layer_from_slot,
        multitap_arrow_visual_spec,
    )
}

/// Reconstruct a gameplay screen at a consumed runtime navigation boundary.
/// This is load-time work: callers must rebuild the domain and prewarm the
/// returned resources before resuming updates, just as at initial entry.
pub fn reload_song_lua_with_default_host<NoteskinSlot, ModelVertex, MultitapArrowVisualSpec>(
    mut scene: crate::live::SongLuaLiveScene,
    read_model_slots: fn(&Path, &Path, &Path) -> Result<Arc<[NoteskinSlot]>, String>,
    model_layer_from_slot: fn(&NoteskinSlot) -> Option<SongLuaOverlayModelLayer<ModelVertex>>,
    multitap_arrow_visual_spec: MultitapArrowVisualSpec,
) -> Result<Vec<DefaultCompiledSongLua<NoteskinSlot, ModelVertex>>, String>
where
    MultitapArrowVisualSpec: FnMut(&SongLuaCompileContext, &str) -> Option<(
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>, SongLuaOverlayState)>,
{
    let mut ending = scene.end().map_err(|err| err.to_string())?;
    let context = scene.selected_context().map_err(|err| err.to_string())?;
    let entry_paths = scene.entry_paths;
    let paths = entry_paths.iter().map(|path| path.as_path()).collect::<Vec<_>>();
    let mut compiled = compile_song_lua_layers_with_actors(&paths, scene.primary_index, &context, Some(scene.lua),
        SongLuaNoteskinResolver::default(), create_default_dummy_actor,
        create_default_named_child_actor, install_default_actor_methods,
        read_model_slots, model_layer_from_slot, multitap_arrow_visual_spec)?;
    let replacement = compiled.iter_mut().find_map(|compiled| compiled.live_scene.as_mut())
        .ok_or("gameplay reload did not return an owned Lua scene")?;
    ending.append(replacement.take_startup_output());
    replacement.startup_output = ending;
    Ok(compiled)
}

pub(crate) fn create_default_named_child_actor(lua: &Lua, parent: &Table, name: &str) -> mlua::Result<Table> {
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

pub(crate) fn create_default_dummy_actor(lua: &Lua, actor_type: &'static str) -> mlua::Result<Table> {
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

/// Reuse the production root ownership and clone installer in migration controls.
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_own_actor_root(lua: &Lua, root: Value) -> mlua::Result<Value> {
    crate::lua_util::own_actor_root(lua, root, install_default_actor_methods)
}

/// Exercise the production actor constructor/Init path without loading a whole
/// gameplay screen. Used only as the migration's legacy behavior control.
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_actor_control(source: &str) -> mlua::Result<Lua> {
    let mut context = SongLuaCompileContext::new(Path::new("."), "actor migration control");
    context.live = true;
    migration_actor_control_with_context(source, &context)
}

#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_actor_control_with_context(source: &str, context: &SongLuaCompileContext) -> mlua::Result<Lua> {
    migration_actor_control_with_setup(source, context, |_| Ok(()))
}

#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_actor_control_with_setup(
    source: &str,
    context: &SongLuaCompileContext,
    setup: impl FnOnce(&Lua) -> mlua::Result<()>,
) -> mlua::Result<Lua> {
    let lua = Lua::new();
    crate::lua_util::begin_live_runtime(&lua)?;
    if !context.live { lua.remove_app_data::<crate::live::LiveMode>(); }
    crate::install_default_stdlib_compat(&lua, &context.song_dir)?;
    crate::host::install_message_manager_globals(&lua, crate::lua_util::broadcast_song_lua_message)?;
    lua.globals().set(crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY, 0)?;
    let clock = crate::create_song_runtime_table(&lua, context)?;
    lua.globals().set(crate::SONG_LUA_RUNTIME_KEY, clock)?;
    crate::install_def_globals(&lua, crate::song_lua_human_player_count(context), create_default_dummy_actor, install_default_actor_methods)?;
    setup(&lua)?;
    let declaration = lua.load(source).set_name("@/fixture/actors.lua").eval()?;
    let root = crate::lua_util::own_actor_root(&lua, declaration, install_default_actor_methods)?;
    if !context.live {
        // This control compares native tween replay in both clock modes. The
        // live constructor registers replay automatically; preparation normally
        // installs it later from captured startup states. Populate that same
        // native storage here before Init so these local lifecycle comparisons
        // do not accidentally compare raw capture destinations with replay poses.
        // Full preparation/capture/baking requires the compiler-path controls.
        let mut pending = if let Value::Table(actor) = &root { vec![actor.clone()] } else { Vec::new() };
        while let Some(actor) = pending.pop() {
            crate::lua_util::ensure_tween_replay(&lua, &actor)?;
            pending.extend(crate::lua_util::actor_direct_children(&lua, &actor)?);
        }
    }
    lua.globals().set("root", root.clone())?;
    crate::run_actor_init_commands(&lua, &root)?;
    Ok(lua)
}

#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_null_actor_control(lua: &Lua) -> mlua::Result<Table> {
    crate::install_def_globals(lua, 1, create_default_dummy_actor, install_default_actor_methods)?;
    lua.globals().get("NullActor")
}

#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_install_file_loaders(lua: &Lua, song_dir: &Path) -> mlua::Result<()> {
    crate::install_file_loader_globals(lua, song_dir.to_owned(), create_default_dummy_actor)
}

/// Actual production base constructor, excluding subsequent theme-file loads.
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_gameplay_screen(
    context: &SongLuaCompileContext,
    setup: impl FnOnce(&Lua) -> mlua::Result<()>,
) -> mlua::Result<Lua> {
    let lua = Lua::new();
    if context.live {
        crate::lua_util::begin_live_runtime(&lua)?;
    }
    crate::install_default_stdlib_compat(&lua, &context.song_dir)?;
    crate::clock_inputs::install(&lua, context);
    let math: Table = lua.globals().get("math")?;
    math.get::<mlua::Function>("randomseed")?
        .call::<()>(context.random_seed as i32)?;
    let state = crate::install_core_globals(
        &lua,
        context,
        crate::song_lua_local_date_globals(),
        crate::create_noteskin_table(
            &lua,
            context,
            SongLuaNoteskinResolver::default(),
            create_default_dummy_actor,
        )?,
        crate::current_song_lua_style_name,
    )?;
    lua.set_app_data(state.clone());
    lua.globals()
        .set("SL", crate::create_sl_table(&lua, context)?)?;
    setup(&lua)?;
    let screen = crate::create_top_screen_table(
        &lua,
        context,
        state.current_sort_order,
        state.current_song,
        create_default_dummy_actor,
        create_default_named_child_actor,
    )?;
    crate::install_screen_manager_globals(&lua, screen.top_screen, screen.players)?;
    crate::install_sound_globals(&lua, &context.song_dir)?;
    crate::install_message_manager_globals(&lua, crate::broadcast_song_lua_message)?;
    crate::install_late_globals(&lua, context)?;
    Ok(lua)
}

/// The existing bundled-theme phases, isolated from the unported phases.
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_gameplay_theme_definitions(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> mlua::Result<()> {
    crate::install_cmd_helpers(lua)?;
    crate::install_def_globals(
        lua,
        crate::song_lua_human_player_count(context),
        create_default_dummy_actor,
        install_default_actor_methods,
    )?;
    crate::install_file_loader_globals(lua, context.song_dir.clone(), create_default_dummy_actor)
}

#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_gameplay_combo(lua: &Lua) -> mlua::Result<()> {
    crate::lua_util::install_native_gameplay_combo(lua, Some(install_default_actor_methods))
}

#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_gameplay_transitions(lua: &Lua) -> mlua::Result<()> {
    crate::lua_util::install_native_gameplay_transitions(lua, Some(install_default_actor_methods))
}

/// Existing underlay phase, using its production actor method installer.
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_gameplay_underlay(lua: &Lua) -> mlua::Result<()> {
    crate::lua_util::install_native_gameplay_underlay(lua, Some(install_default_actor_methods))
}

/// Existing overlay and decoration phases, with their production actor installer.
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_gameplay_overlay(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> mlua::Result<()> {
    crate::lua_util::install_native_gameplay_overlay(
        lua,
        Some(install_default_actor_methods),
        context,
    )
}
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_gameplay_decorations(lua: &Lua) -> mlua::Result<()> {
    crate::lua_util::install_native_gameplay_decorations(lua, Some(install_default_actor_methods))
}

/// Actual retained-VM screen replacement, used by migration comparisons.
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_reconstruct_gameplay_screen(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> mlua::Result<()> {
    crate::host::replace_gameplay_screen(
        lua,
        context,
        create_default_dummy_actor,
        create_default_named_child_actor,
        install_default_actor_methods,
    )
}

/// Full production host, used as the screen-controls migration oracle.
#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_top_screen(context: &SongLuaCompileContext) -> mlua::Result<Lua> {
    let lua = Lua::new();
    if context.live {
        crate::lua_util::begin_live_runtime(&lua)?;
    }
    install_compile_host(
        &lua,
        context,
        SongLuaNoteskinResolver::default(),
        create_default_dummy_actor,
        create_default_named_child_actor,
        install_default_actor_methods,
    )?;
    Ok(lua)
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
        None,
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
    retained_lua: Option<Lua>,
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
    if entry_paths.is_empty() && !context.live {
        return Ok(Vec::new());
    }
    if context.background_layer_count > entry_paths.len() {
        return Err("song lua background count exceeds its session entries".to_owned());
    }
    if !context.foreground_names.is_empty()
        && context.foreground_names.len() != entry_paths.len() - context.background_layer_count
    {
        return Err("song lua foreground names do not match its session entries".into());
    }
    if !context.layer_start_beats.is_empty()
        && (context.layer_start_beats.len() != entry_paths.len()
            || context.layer_start_beats.iter().any(|beat| !beat.is_finite()))
    {
        return Err("song lua layer beats do not match its session entries".into());
    }
    if primary_index >= entry_paths.len() && !(entry_paths.is_empty() && primary_index == 0) {
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
    let trace_entry_path = entry_paths.get(primary_index).cloned()
        .unwrap_or_else(|| context.song_dir.clone());
    let reloaded = retained_lua.is_some();
    // The oracle supplies an empty VM only to retain observations after failure.
    // Exercise the ordinary first-load branch on its first compiler invocation.
    #[cfg(all(feature = "test-support", any(test, feature = "legacy-oracle")))]
    let reloaded = reloaded && !retained_lua.as_ref().is_some_and(|lua|
        lua.remove_app_data::<MigrationFreshCompile>().is_some());
    let lua = retained_lua.unwrap_or_else(Lua::new);
    #[cfg(feature = "test-support")]
    if !reloaded && context.initial_lua_gc_stopped {
        lua.gc_stop();
    }
    if context.live {
        crate::lua_util::begin_live_runtime(&lua).map_err(|err| err.to_string())?;
    }
    let mut host = SongLuaHostState::default();
    if reloaded {
        // ScreenManager::LoadDelayedScreen reconstructs screen-owned state in
        // the process Lua VM. Reinstalling the host would reset GAMESTATE,
        // authored globals, player options and the random generator.
        crate::host::replace_gameplay_screen(&lua, context,
            create_dummy_actor, create_named_child_actor, install_actor_methods).map_err(|err| err.to_string())?;
    } else {
        install_compile_host(&lua, context, noteskin_resolver,
            create_dummy_actor, create_named_child_actor, install_actor_methods)
            .map_err(|err| err.to_string())?;
    }
    #[cfg(feature = "test-support")]
    crate::lua_util::install_message_order(&lua, &context.message_actor_order)
        .map_err(|err| err.to_string())?;
    crate::live::install_query_trace(&lua).map_err(|err| err.to_string())?;
    #[cfg(feature = "test-support")]
    crate::live::set_dynamic_command_observer(&lua, context.trace_dynamic_commands);
    compile_timer.push_stage("host");
    #[cfg(feature = "test-support")]
    lua.set_app_data(crate::song_tables::SongLuaSkinWrites::default());
    #[cfg(feature = "test-support")]
    lua.set_app_data(crate::song_tables::SongLuaBoolWrites::default());
    #[cfg(feature = "test-support")]
    lua.set_app_data(crate::song_tables::SongLuaSpeedWrites::default());
    #[cfg(feature = "test-support")]
    lua.set_app_data(crate::host::SongLuaStateCalls::default());
    let mut scene_context = crate::host::selected_scene_context(&lua, context)
        .map_err(|err| err.to_string())?;
    let mut reloads = 0u32;
    let mut prior_broadcasts = Vec::new();
    let (root, screen_layer_states, mut captured_startup, initialized_global_actions,
        initialized_global_perframes, initialized_global_mods, initialized_global_mod_eases) = loop {
        let context = &scene_context;
        lua.globals().raw_set("__songlua_screen_loading", context.screen_loading)
            .map_err(|err| err.to_string())?;
        #[cfg(feature = "test-support")]
        if let Some(mut capture) = lua.app_data_mut::<crate::song_tables::SongLuaSkinWrites>() {
            capture.active = true;
        }
        let screen_layer_states =
            crate::lua_util::screen_layer_states(&lua).map_err(|err| err.to_string())?;
        let roots = lua.create_table().map_err(|err| err.to_string())?;
        let mut initial_actor_states = std::collections::HashMap::new();
        // Native Update(0) can start a player's tween from a song callback.
        // Players are screen siblings, so traversing the song roots misses them.
        for name in [
            "__songlua_top_screen_player_1",
            "__songlua_top_screen_player_2",
        ] {
            let actor = lua
                .globals()
                .get::<Table>(name)
                .map_err(|err| err.to_string())?;
            crate::lua_util::collect_initial_states(&Value::Table(actor), &mut initial_actor_states)
                .map_err(|err| err.to_string())?;
        }
        #[cfg(all(feature = "test-support", any(test, feature = "legacy-oracle")))]
        migration_record_initial(&lua, &initial_actor_states, &screen_layer_states);
        for (index, entry_path) in entry_paths.iter().enumerate() {
            // Foreground::LoadFromSong concatenates the raw authored name.
            // Its directory default.lua and direct-file fallback both retain
            // that spelling as the Lua chunk name, including "./" and slashes.
            let source_path = index.checked_sub(context.background_layer_count)
                .and_then(|index| context.foreground_names.get(index))
                .map(|name| {
                    let suffix = if context.song_dir.join(name).is_dir() { "/default.lua" } else { "" };
                    std::path::PathBuf::from(format!("{}/{name}{suffix}",
                        crate::file_path_string(&context.song_dir).trim_end_matches('/')))
                })
                .unwrap_or_else(|| entry_path.clone());
            let root = match crate::lua_util::execute_script_file_with_source(
                &lua, entry_path, &source_path, context.song_dir.as_path())
            {
                Ok(root) if !context.live || matches!(&root, Value::Table(_)) => root,
                Ok(_) => {
                    // ActorUtil::MakeActor substitutes a plain Actor when a
                    // Lua chunk fails to return a declaration table.
                    log::warn!("Failed to load song actor '{}': must return a table; using native dummy Actor", entry_path.display());
                    Value::Table(create_dummy_actor(&lua, "Actor").map_err(|err| err.to_string())?)
                }
                Err(err) if context.live => {
                    // Native construction catches the load error without
                    // replacing the process Lua VM or its earlier side effects.
                    log::warn!("Failed to load song actor '{}': {err}; using native dummy Actor", entry_path.display());
                    Value::Table(create_dummy_actor(&lua, "Actor").map_err(|err| err.to_string())?)
                }
                Err(err) => return Err(format!("failed to execute '{}': {err}", entry_path.display())),
            };
            let root = match (root, context.background_upper_left_colors.get(index)) {
                (Value::Table(actor), Some(Some(color))) => Value::Table(
                    crate::lua_util::install_background_upper_left(&lua, &actor, *color)
                        .map_err(|err| err.to_string())?),
                (root, _) => root,
            };
            let root = crate::lua_util::own_actor_root(&lua, root, install_actor_methods)
                .map_err(|err| err.to_string())?;
            let parent = if let Value::Table(actor) = &root {
                #[cfg(feature = "test-support")]
                actor.raw_set("__songlua_message_path", (index + 1).to_string())
                    .map_err(|err| err.to_string())?;
                let screen = lua
                    .globals()
                    .get::<Table>("__songlua_top_screen")
                    .map_err(|err| err.to_string())?;
                let children = crate::actor_children(&lua, &screen).map_err(|err| err.to_string())?;
                // BackgroundImpl::Layer::CreateBackground calls MakeActor
                // without a parent; its Layer map owns the result separately.
                let parent = if index < context.background_layer_count {
                    None
                } else {
                    Some(children.get::<Table>("SongForeground").map_err(|err| err.to_string())?)
                };
                if context.live && let Some(&beat) = context.layer_start_beats.get(index)
                {
                    let key = if index < context.background_layer_count {
                        "__songlua_background_start_beat"
                    } else {
                        "__songlua_foreground_start_beat"
                    };
                    actor.raw_set(key, beat).map_err(|err| err.to_string())?;
                    actor.raw_set("__songlua_defer_on", true).map_err(|err| err.to_string())?;
                    actor.raw_set("__songlua_live_inactive", true).map_err(|err| err.to_string())?;
                }
                if let Some(parent) = &parent {
                    actor.set("__songlua_parent", parent.clone()).map_err(|err| err.to_string())?;
                }
                parent
            } else {
                None
            };
            crate::lua_util::collect_initial_states(&root, &mut initial_actor_states)
                .map_err(|err| err.to_string())?;
            #[cfg(all(feature = "test-support", any(test, feature = "legacy-oracle")))]
            migration_record_initial(&lua, &initial_actor_states, &screen_layer_states);
            run_actor_init_commands(&lua, &root).map_err(|err| {
                format!(
                    "failed to run actor init commands for '{}': {err}",
                    entry_path.display()
                )
            })?;
            // MakeActor runs Init with the parent set; Foreground::LoadFromSong
            // adds the initialized actor to its children afterward.
            if let (Value::Table(actor), Some(parent)) = (&root, parent) {
                if actor.raw_get::<Option<f32>>("__songlua_foreground_start_beat")
                    .map_err(|err| err.to_string())?.is_some()
                {
                    // Foreground::LoadFromSong overwrites an authored or
                    // Init-assigned name after MakeActor, before On activation.
                    if let Some(name) = context.foreground_names.get(index - context.background_layer_count) {
                        crate::lua_util::set_actor_name(actor, lua.create_string(name.as_str())
                            .map_err(|err| err.to_string())?).map_err(|err| err.to_string())?;
                        actor.raw_set("Name", name.as_str()).map_err(|err| err.to_string())?;
                    }
                    // Foreground::LoadFromSong hides the actor after MakeActor
                    // has run Init, before its scheduled On activation.
                    actor.get::<mlua::Function>("visible").map_err(|err| err.to_string())?
                        .call::<()>((actor.clone(), false)).map_err(|err| err.to_string())?;
                }
                // BackgroundImpl's Layer owns background actors separately;
                // they are not part of ActorFrame::GetChildren. Foreground
                // adds its initialized entries to the ordinary child list.
                if index >= context.background_layer_count {
                    crate::lua_util::push_sequence_child_once(&parent, actor.clone())
                        .map_err(|err| err.to_string())?;
                }
            }
            roots
                .raw_set(index + 1, root)
                .map_err(|err| err.to_string())?;
        }
        let initialized_global_actions = lua
            .globals()
            .get::<Option<Table>>("mod_actions")
            .map_err(|err| err.to_string())?;
        // ScreenManager::LoadDelayedScreen rejects requests made while
        // constructing or initializing the next screen, before activation.
        if lua.remove_app_data::<crate::host::SongLuaScreenRequest>().is_some() {
            log::warn!("Setting a new screen during an InitCommand is not allowed.");
        }
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
        #[cfg(all(feature = "test-support", any(test, feature = "legacy-oracle")))]
        if let Some(mut probe) = lua.app_data_mut::<MigrationActivationProbe>() {
            probe.initialized = [
                initialized_global_actions.clone(),
                initialized_global_perframes.clone(),
                initialized_global_mods.clone(),
                initialized_global_mod_eases.clone(),
            ];
        }
        compile_timer.push_stage("execute_init");
        let root = Value::Table(roots);
        // Startup queues and the initial update can consume one-shot broadcasts
        // before the sampled replay starts. Retain their events as well.
        // PushLoadedScreen makes the initialized screen visible to Begin.
        lua.globals().raw_set("__songlua_screen_loading", false)
            .map_err(|err| err.to_string())?;
        crate::lua_util::begin_overlay_update_capture_from_indices(&lua, std::iter::empty());
        let captured_startup = run_actor_startup_commands(&lua, &root, initial_actor_states, context)
            .map_err(|err| {
                format!(
                    "failed to run actor startup commands for song lua session '{}': {err}",
                    trace_entry_path.display()
                )
            })?;
        let request = lua.remove_app_data::<crate::host::SongLuaScreenRequest>()
            .map(|request| request.0);
        let Some(request) = request else {
            break (root, screen_layer_states, captured_startup, initialized_global_actions,
                initialized_global_perframes, initialized_global_mods, initialized_global_mod_eases);
        };
        if request != "ScreenGameplay" {
            return Err(format!("song Lua startup requested unsupported screen '{request}'"));
        }
        if reloads == 128 {
            return Err("song Lua exceeded 128 startup gameplay reloads".into());
        }
        prior_broadcasts.extend(captured_startup.broadcasts);
        if let Value::Table(root) = &root {
            crate::lua_util::end_actor_scene(&lua, root).map_err(|err| err.to_string())?;
        }
        // Keep GAMESTATE, source globals and the PRNG in this Lua VM. Only
        // screen-owned actors and stage data are reconstructed at the queue boundary.
        scene_context = crate::host::selected_scene_context(&lua, context)
            .map_err(|err| err.to_string())?;
        if scene_context.live {
            crate::lua_util::begin_live_runtime(&lua).map_err(|err| err.to_string())?;
        }
        crate::host::replace_gameplay_screen(&lua, &scene_context,
            create_dummy_actor, create_named_child_actor, install_actor_methods).map_err(|err| err.to_string())?;
        scene_context = crate::host::selected_scene_context(&lua, &scene_context)
            .map_err(|err| err.to_string())?;
        crate::live::install_query_trace(&lua).map_err(|err| err.to_string())?;
        #[cfg(feature = "test-support")]
        crate::live::set_dynamic_command_observer(&lua, scene_context.trace_dynamic_commands);
        reloads += 1;
    };
    prior_broadcasts.append(&mut captured_startup.broadcasts);
    captured_startup.broadcasts = prior_broadcasts;
    let context = &scene_context;
    #[cfg(feature = "test-support")]
    let startup_skin_writes = lua.remove_app_data::<crate::song_tables::SongLuaSkinWrites>()
        .map(|capture| capture.writes).unwrap_or_default();
    #[cfg(feature = "test-support")]
    let startup_bool_writes = lua.remove_app_data::<crate::song_tables::SongLuaBoolWrites>()
        .map(|capture| capture.0).unwrap_or_default();
    #[cfg(feature = "test-support")]
    let startup_speed_writes = lua.remove_app_data::<crate::song_tables::SongLuaSpeedWrites>()
        .map(|capture| capture.0).unwrap_or_default();
    #[cfg(feature = "test-support")]
    let startup_state_calls = lua.remove_app_data::<crate::host::SongLuaStateCalls>()
        .map(|capture| capture.0).unwrap_or_default();
    let startup = captured_startup.options;
    let startup_states = captured_startup.queued;
    let mut startup_tweens = captured_startup.tweens;
    let mut initial_updates = captured_startup.updates;
    let initial_broadcasts = captured_startup.broadcasts;
    compile_timer.push_stage("startup_commands");
    if !context.live { crate::input::require_offline_compatible(&lua)?; }
    if context.live {
        // A live scene stops at native Update(0). In particular, extracting
        // assets must never call listeners or replay future queued commands.
        mark_compile_layers(&root).map_err(|err| err.to_string())?;
        let mut overlays = read_overlay_compile_actors(
            &lua, &root, context,
            |actor| read_actor_model_layers(actor, read_model_slots, model_layer_from_slot),
            |actor, _| read_noteskin_tap_actor_slots(actor, read_model_slots),
            |_| {},
        )?;
        let screen_index = overlays.len();
        let screen = lua.globals().get::<Table>("__songlua_top_screen")
            .map_err(|err| err.to_string())?;
        screen.set(COMPILE_LAYER_KEY, primary_index).map_err(|err| err.to_string())?;
        overlays.push(crate::SongLuaOverlayCompileActor {
            actor: SongLuaOverlayActor {
                kind: SongLuaOverlayKind::ActorFrame,
                name: Some("ScreenGameplay".into()), parent_index: None,
                initial_state: crate::live::read_state(&lua, &screen).map_err(|err| err.to_string())?,
                message_commands: Vec::new(),
            }, table: screen, message_sounds: Vec::new(),
        });
        let tracked = read_tracked_compile_actors(&lua, create_named_child_actor)?;
        let mut hidden_players = std::array::from_fn(|player| tracked.get(player * 3)
            .is_some_and(|actor| !actor.actor.initial_state.visible));
        resolve_late_actor_targets(&mut overlays, &mut hidden_players)?;
        let overlay_layers = overlays.iter().map(|actor| actor.table
            .get::<Option<usize>>(COMPILE_LAYER_KEY).map(|layer| layer.unwrap_or(primary_index))
            .map_err(|err| err.to_string())).collect::<Result<Vec<_>, _>>()?;
        let mut out = CompiledSongLua {
            overlays: Vec::with_capacity(overlays.len()),
            startup, loaded_steps: Some(context.player_step_indices.map(|index| index.and_then(|index|
                if context.song_step_chart_indices.is_empty() { Some(index) }
                else { context.song_step_chart_indices.get(index).copied().flatten() }))),
            loaded_style: Some(context.style_name.clone()), startup_reloads: reloads,
            entry_path: trace_entry_path.clone(),
            screen_width: context.screen_width, screen_height: context.screen_height,
            screen_overlay_index: Some(screen_index), hidden_players,
            sound_paths: read_song_lua_sound_paths(&lua)?,
            ..CompiledSongLua::default()
        };
        let mut tables = Vec::with_capacity(overlays.len());
        for mut overlay in overlays {
            overlay.actor.initial_state = crate::live::read_state(&lua, &overlay.table)
                .map_err(|err| err.to_string())?;
            tables.push(overlay.table);
            out.overlays.push(overlay.actor);
        }
        let mut tracked_tables = Vec::with_capacity(tracked.len());
        for actor in tracked {
            let state = crate::live::read_state(&lua, &actor.table).map_err(|err| err.to_string())?;
            tracked_tables.push((actor.target, actor.table, state));
            match actor.target {
                TrackedCompileActorTarget::Player(player) => out.player_actors[player].initial_state = state,
                TrackedCompileActorTarget::PlayerJudgment(player) => out.player_actors[player].judgment.initial_state = state,
                TrackedCompileActorTarget::PlayerCombo(player) => out.player_actors[player].combo.initial_state = state,
                TrackedCompileActorTarget::SongForeground => out.song_foreground.initial_state = state,
                TrackedCompileActorTarget::ScreenLayer(index) => out.screen_layers[index].initial_state = state,
            }
        }
        out.live_scene = Some(crate::live::SongLuaLiveScene::new(lua, root, scene_context,
            entry_paths.clone(), primary_index, tables, tracked_tables, initial_broadcasts)
            .map_err(|err| err.to_string())?);
        compile_timer.push_stage("live_scene");
        log_song_lua_compile_timing(&trace_entry_path, &compile_timer);
        if entry_paths.is_empty() || (entry_paths.len() == 1
            && overlay_layers.iter().all(|&layer| layer == 0)
            && out.overlays.iter().enumerate().all(|(index, overlay)|
                overlay.parent_index.is_none_or(|parent| parent < index)))
        {
            // ScreenGameplay and its theme actors exist without authored song
            // actors. A single already-ordered layer also owns the output as-is.
            out.live_indices.extend(0..out.overlays.len());
            return Ok(vec![out]);
        }
        return split_compiled_song_lua(out, overlay_layers, &entry_paths, primary_index);
    }
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
        crate::perframe::reference_actor_frames(context).as_deref(),
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
        crate::perframe::first_update_seconds(context),
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
        loaded_steps: Some(context.player_step_indices.map(|index| index.and_then(|index|
            if context.song_step_chart_indices.is_empty() { Some(index) }
            else { context.song_step_chart_indices.get(index).copied().flatten() }))),
        loaded_style: Some(context.style_name.clone()),
        startup_reloads: reloads,
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
        out.state_calls = startup_state_calls;
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
    let mut tracked_actors = read_tracked_compile_actors(&lua, create_named_child_actor)
        .map_err(|err| format!("failed to read tracked actors: {err}"))?;
    let mut hidden_players = std::array::from_fn(|player| {
        tracked_actors
            .get(player)
            .is_some_and(|tracked| !tracked.actor.initial_state.visible)
    });
    let mut overlay_trigger_counter = 0usize;
    let mut sound_events = startup_sounds;
    let prefix_is_runtime = update_tree_reads_global(&lua, &root, "prefix_globals")
        .map_err(|err| format!("failed to inspect prefix update callbacks: {err}"))?;
    let prefix_perframes = globals
        .get::<Option<Table>>("prefix_globals")
        .map_err(|err| format!("failed to read prefix_globals: {err}"))?
        .and_then(|table| table.get::<Option<Table>>("perframes").ok().flatten());
    let global_perframes = globals
        .get::<Option<Table>>("mod_perframes")
        .map_err(|err| format!("failed to read mod_perframes: {err}"))?;
    let runtime_perframe_tables =
        read_update_function_tables(&lua, &root, &["mod_perframes", "perframes"])
            .map_err(|err| format!("failed to read update callback tables: {err}"))?;
    let update_reads_global_perframes = update_tree_reads_global(&lua, &root, "mod_perframes")
        .map_err(|err| format!("failed to inspect global update callbacks: {err}"))?;
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
    #[cfg(all(feature = "test-support", any(test, feature = "legacy-oracle")))]
    if lua
        .remove_app_data::<crate::rust_engine::test_support::StaticScheduleProbe>()
        .is_some()
    {
        // Observe the real production stage while its lexical Lua handles are
        // still retained. This branch is absent from normal application builds.
        if let Some(check) = globals
            .get::<Option<mlua::Function>>("__migration_static_boundary")
            .map_err(|err| err.to_string())?
        {
            check.call::<()>(()).map_err(|err| err.to_string())?;
        }
        lua.set_app_data(crate::rust_engine::test_support::LegacyStaticSchedule {
            output: crate::rust_engine::SongStaticSchedule {
                beat_mods: out.beat_mods.clone(),
                time_mods: out.time_mods.clone(),
                eases: out.eases.clone(),
                overlay_eases: out.overlay_eases.clone(),
                column_offsets: out.column_offsets.clone(),
                messages: out.messages.clone(),
                sound_events: sound_events.clone(),
                info: out.info.clone(),
                counter: overlay_trigger_counter,
            },
            overlay_commands: overlays
                .iter()
                .map(|r| r.actor.message_commands.clone())
                .collect(),
            tracked_commands: tracked_actors
                .iter()
                .map(|r| r.actor.message_commands.clone())
                .collect(),
            ownership: [
                prefix_is_runtime,
                global_perframes_are_runtime,
                global_mods_are_runtime,
                global_mod_eases_are_runtime,
                global_actions_are_runtime,
            ],
            perframes: [
                if prefix_is_runtime {
                    None
                } else {
                    prefix_perframes.clone()
                },
                if global_perframes_are_runtime {
                    None
                } else {
                    global_perframes.clone()
                },
            ],
        });
        return Err("migration static schedule boundary reached".into());
    }
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
    if let Some(calls) = lua.remove_app_data::<crate::host::SongLuaStateCalls>() {
        out.state_calls.extend(calls.0);
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
    finalize_sprite_textures(&mut overlays, sprite_textures);
    let startup_seconds = crate::perframe::first_update_seconds(context);
    let startup_time = context.song_timing.as_ref().map_or_else(
        || crate::song_beat_at_elapsed_seconds(startup_seconds, context),
        |timing| {
            timing.get_time_for_beat_exact(0.0) + startup_seconds * crate::song_music_rate(context)
        },
    );
    for track in &mut out.overlay_updates {
        // EffectTime anchors describe the actor's existing clock, including
        // before its first update; they are not callback property writes.
        if track.target != crate::SongLuaOverlayUpdateTarget::EffectTime
            && startup_states.contains_key(&(overlays[track.overlay_index].table.to_pointer() as usize))
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
    finalize_tracked_actors(&mut out, tracked_actors, judgment_textures);
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
    crate::input::require_offline_compatible(&lua)?;
    split_compiled_song_lua(out, overlay_layers, &entry_paths, primary_index)
}

// Earlier aliases receive independent lists; the final destination takes ownership.
fn take_final_events<T: Clone>(events: &mut Vec<T>, remaining: &mut usize) -> Vec<T> {
    if events.is_empty() {
        return Vec::new();
    }
    *remaining -= 1;
    if *remaining == 0 {
        std::mem::take(events)
    } else {
        events.clone()
    }
}

fn finalize_sprite_textures<NoteskinSlot, ModelVertex>(
    overlays: &mut [crate::SongLuaOverlayCompileActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
    sprite_textures: Vec<(usize, crate::SongLuaSpriteTexture)>,
) {
    let mut groups =
        rustc_hash::FxHashMap::<usize, (Vec<crate::SongLuaSpriteTexture>, usize)>::default();
    if !sprite_textures.is_empty() {
        for overlay in overlays.iter() {
            if matches!(overlay.actor.kind, SongLuaOverlayKind::Sprite { .. }) {
                groups
                    .entry(overlay.table.to_pointer() as usize)
                    .or_default()
                    .1 += 1;
            }
        }
        for (pointer, texture) in sprite_textures {
            if let Some((events, _)) = groups.get_mut(&pointer) {
                events.push(texture);
            }
        }
        for (events, _) in groups.values_mut() {
            events.sort_by(|a, b| a.second.total_cmp(&b.second));
        }
    }
    for overlay in overlays {
        if let SongLuaOverlayKind::Sprite { textures, .. } = &mut overlay.actor.kind {
            *textures = if groups.is_empty() {
                Vec::new()
            } else {
                let (events, remaining) = groups
                    .get_mut(&(overlay.table.to_pointer() as usize))
                    .unwrap();
                take_final_events(events, remaining)
            }
            .into();
        }
    }
}

fn finalize_tracked_actors<NoteskinSlot, ModelVertex>(
    out: &mut DefaultCompiledSongLua<NoteskinSlot, ModelVertex>,
    tracked_actors: Vec<crate::SongLuaTrackedActor>,
    judgment_textures: Vec<(usize, crate::SongLuaJudgmentTexture)>,
) {
    let mut groups: [(Vec<crate::SongLuaJudgmentTexture>, usize); crate::LUA_PLAYERS] =
        std::array::from_fn(|_| (Vec::new(), 0));
    if !judgment_textures.is_empty() {
        for tracked in &tracked_actors {
            if let TrackedCompileActorTarget::PlayerJudgment(player) = tracked.target {
                groups[player].1 += 1;
            }
        }
        for (player, texture) in judgment_textures {
            if let Some((events, remaining)) = groups.get_mut(player) {
                if *remaining != 0 {
                    events.push(texture);
                }
            }
        }
        for (events, _) in &mut groups {
            events.sort_by(|a, b| a.second.total_cmp(&b.second));
            events.dedup_by(|a, b| a.path == b.path);
        }
    }
    for tracked in tracked_actors {
        match tracked.target {
            TrackedCompileActorTarget::Player(player) => out.player_actors[player] = tracked.actor,
            TrackedCompileActorTarget::PlayerJudgment(player) => {
                out.player_actors[player].judgment = SongLuaCapturedChildActor {
                    initial_state: tracked.actor.initial_state,
                    message_commands: tracked.actor.message_commands,
                    textures: {
                        let (events, remaining) = &mut groups[player];
                        take_final_events(events, remaining)
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
}





fn resolve_late_actor_targets<NoteskinSlot, ModelVertex>(
    overlays: &mut [crate::SongLuaOverlayCompileActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
    hidden_players: &mut [bool; crate::LUA_PLAYERS],
) -> Result<(), String> {
    let actor_indices = if overlays.iter().any(|overlay|
        matches!(overlay.actor.kind, SongLuaOverlayKind::ActorProxy { .. }))
    {
        overlays
            .iter()
            .enumerate()
            .map(|(index, overlay)| (overlay.table.to_pointer() as usize, index))
            .collect::<std::collections::HashMap<_, _>>()
    } else {
        std::collections::HashMap::new()
    };
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
                    texture_key: crate::lua_util::texture_path_key(&texture_path),
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





#[cfg(test)]
#[path = "../tests/perf/compile_finalize.rs"]
mod perf;

#[cfg(test)]
#[path = "../tests/perf/finalization_events.rs"]
mod finalization_events_perf;

#[cfg(test)]
#[path = "../tests/perf/reload_inputs.rs"]
mod reload_inputs_perf;

#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_install_noteskin(
    lua: &Lua,
    context: &SongLuaCompileContext,
    resolver: SongLuaNoteskinResolver,
) -> mlua::Result<()> {
    lua.globals().set(
        "NOTESKIN",
        crate::create_noteskin_table(lua, context, resolver, create_default_dummy_actor)?,
    )
}

#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_player_base(
    lua: &Lua,
    context: &SongLuaCompileContext,
    index: usize,
) -> mlua::Result<Table> {
    let meter = crate::lua_util::create_life_meter_table(
        lua,
        if index == 0 { "LifeP1" } else { "LifeP2" },
        create_default_dummy_actor,
    )?;
    let player = crate::lua_util::create_top_screen_player_actor(
        lua,
        context.players[index].clone(),
        index,
        &context.style_name,
        create_default_dummy_actor,
    )?;
    player.raw_set("__songlua_life_meter", meter)?;
    Ok(player)
}

#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_note_field(lua: &Lua, player: usize, style: &str) -> mlua::Result<Table> {
    crate::lua_util::create_note_field_actor(lua, player, style, create_default_dummy_actor)
}

#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_gameplay_player(
    lua: &Lua,
    context: &SongLuaCompileContext,
    index: usize,
) -> mlua::Result<Table> {
    let player = migration_player_base(lua, context, index)?;
    crate::lua_util::migration_player_parts(
        lua,
        &player,
        index,
        &context.style_name,
        create_default_dummy_actor,
        create_default_named_child_actor,
    )?;
    Ok(player)
}

#[cfg(all(any(test, feature = "legacy-oracle"), feature = "test-support"))]
pub(crate) fn migration_screen_part(lua: &Lua, name: &str) -> mlua::Result<Table> {
    if name.eq_ignore_ascii_case("Timer") {
        let actor = crate::lua_util::create_screen_timer_actor(lua, create_default_dummy_actor)?;
        actor.set("Name", name)?;
        return Ok(actor);
    }
    if let Some(index) = crate::top_screen_score_index(name) {
        return crate::lua_util::create_top_screen_score_actor(
            lua,
            index,
            create_default_dummy_actor,
        );
    }
    crate::lua_util::create_top_screen_theme_actor(
        lua,
        &lua.create_table()?,
        name,
        create_default_dummy_actor,
        create_default_named_child_actor,
    )?
    .ok_or_else(|| mlua::Error::runtime("screen part is not implemented"))
}

// Migration oracle: execute the production compiler in a retained VM so tests
// can observe partial Init state. The fixture supplies a malformed mod_actions
// sentinel to stop at the existing typed read before Begin/On and baking.
#[cfg(all(feature = "test-support", any(test, feature = "legacy-oracle")))]
pub(crate) fn migration_compile_song_layers(
    lua: &Lua,
    paths: &[&Path],
    primary: usize,
    context: &SongLuaCompileContext,
) -> Result<(), String> {
    compile_song_lua_layers_with_actors::<(), (), _>(
        paths,
        primary,
        context,
        Some(lua.clone()),
        SongLuaNoteskinResolver::default(),
        create_default_dummy_actor,
        create_default_named_child_actor,
        install_default_actor_methods,
        |_, _, _| Ok(Arc::from([])),
        |_| None,
        |_, _| None,
    )
    .map(|_| ())
}

#[cfg(all(feature = "test-support", any(test, feature = "legacy-oracle")))]
#[derive(Clone)]
pub(crate) struct MigrationInitialStates {
    pub actors: Vec<(Table, crate::SongLuaOverlayState)>,
    pub screen_layers: Vec<(Table, crate::SongLuaOverlayState)>,
}
#[cfg(all(feature = "test-support", any(test, feature = "legacy-oracle")))]
fn migration_record_initial(
    lua: &Lua,
    actors: &std::collections::HashMap<usize, (Table, crate::SongLuaOverlayState)>,
    screen_layers: &std::collections::HashMap<usize, (Table, crate::SongLuaOverlayState)>,
) {
    lua.set_app_data(MigrationInitialStates {
        actors: actors.values().cloned().collect(),
        screen_layers: screen_layers.values().cloned().collect(),
    });
}

#[cfg(all(feature = "test-support", any(test, feature = "legacy-oracle")))]
#[derive(Default)]
pub(crate) struct MigrationActivationProbe {
    pub initialized: [Option<Table>; 4],
}
#[cfg(all(feature = "test-support", any(test, feature = "legacy-oracle")))]
pub(crate) struct MigrationActivationResult {
    pub root: Table,
    pub initial_states: Vec<(Table, crate::SongLuaOverlayState)>,
    pub updates: std::collections::HashMap<usize, crate::rust_engine::SongStartupState>,
    pub broadcasts: Vec<(f32, String, bool)>,
    pub startup_queue: Vec<Table>,
    pub initialized: [Option<Table>; 4],
    pub options: crate::SongLuaStartup,
    pub new_screen: Option<String>,
}

#[cfg(all(feature = "test-support", any(test, feature = "legacy-oracle")))]
pub(crate) struct MigrationFreshCompile;
