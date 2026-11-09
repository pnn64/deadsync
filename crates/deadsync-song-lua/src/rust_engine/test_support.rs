//! Rust runtime controls used by integration fixtures.
//! Kept behind `test-support` so the tests exercise the actual compiled library
//! without copying its private live-mode or reset implementation.

pub use super::managers::{fixture_single_value_array, fixture_string_method, fixture_path_methods};

/// Drive the existing callback-table reader with persistent deduplication sets.
/// Call the rooted function with the inspected Lua function; release this root
/// after consuming all returned table values.
pub fn function_tables_probe(
    runtime: &mut super::RustSongRuntime,
    names: &[&str],
    nested: bool,
) -> Result<deadlib_lua::Root, deadlib_lua::Error> {
    runtime
        .actors
        .function_tables_probe(&mut runtime.vm, names, nested)
}

/// Invoke the production multitap visual insertion helper on a rooted batch.
pub fn ensure_multitap_visual<S, V, A>(
    runtime: &mut super::RustSongRuntime,
    overlays: &mut super::SongOverlayActors<crate::SongLuaOverlayKind<S, V, A>>,
    arrow: usize,
    noteskin: &str,
    mut visual: impl FnMut(
        &str,
    ) -> Option<(
        crate::SongLuaOverlayKind<S, V, A>,
        crate::SongLuaOverlayState,
    )>,
) -> Result<(), String> {
    runtime.ensure_multitap_visual(overlays, arrow, noteskin, &mut visual)
}

/// Observe the existing initial-state reader for kind-extraction fixtures.
pub fn read_actor_initial_state(
    runtime: &mut super::RustSongRuntime,
    actor: deadlib_lua::Value,
) -> Result<crate::SongLuaOverlayState, deadlib_lua::Error> {
    runtime
        .actors
        .perframe_access()
        .state(&mut runtime.vm, actor)
}

/// Observe the production live-state reader at the same boundary as live scenes.
pub fn read_live_actor_state(
    runtime: &mut super::RustSongRuntime,
    actor: deadlib_lua::Value,
) -> Result<crate::SongLuaOverlayState, deadlib_lua::Error> {
    Ok(runtime
        .actors
        .compiler_read(
            &mut runtime.vm,
            actor,
            super::actors::compiler::Query::LiveState,
        )?
        .state
        .expect("live-state observation"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorMethodIdentity {
    Lookup,
    Assign,
    Forward(&'static str),
    Bound(&'static str, deadlib_lua::TableId),
}

/// Observe the production method registry without retaining a function or actor.
pub fn actor_method_identity(
    runtime: &super::RustSongRuntime,
    function: deadlib_lua::Value,
) -> Option<ActorMethodIdentity> {
    runtime.actors.method_identity(function)
}

/// Invoke the production key classifier with a rooted VM string.
pub fn actor_method_flags_probe(
    vm: &mut deadlib_lua::Vm,
) -> Result<deadlib_lua::Value, deadlib_lua::Error> {
    super::actors::method_flags_probe(vm)
}

pub use super::managers::screen::expose_publish as expose_screen_publication;
pub fn install_base_table_helpers(
    runtime: &mut super::RustSongRuntime,
) -> Result<(), deadlib_lua::Error> {
    super::managers::compat::install_base_table_helpers(runtime.vm_mut())
}

pub fn seed_rust_transition(runtime: &super::RustSongRuntime, name: &str) {
    runtime.screen.state.lock().unwrap().transition = Some(name.into());
}

pub use super::input::testing::{
    input_compiled_count, input_dispatch_state, input_functions, install_input_controls,
    seed_pending_input_removal,
};

pub fn rust_live_players(
    runtime: &super::RustSongRuntime,
) -> Option<[crate::SongLuaLivePlayer; crate::LUA_PLAYERS]> {
    runtime.statistics.lock().unwrap().players
}
/// Retire statistics' additional public ownership before isolated catalog GC tests.
pub fn retire_statistics_globals(
    runtime: &mut super::RustSongRuntime,
) -> Result<(), deadlib_lua::Error> {
    runtime.vm.set_global(b"STATSMAN", deadlib_lua::Value::Nil)
}

/// Expose the retained song objects while GAMESTATE migration is in progress.
pub fn expose_catalog(runtime: &mut super::RustSongRuntime) -> Result<(), deadlib_lua::Error> {
    let value = runtime.vm.rooted(runtime.catalog)?;
    runtime.vm.set_global(b"catalog", value)
}

/// Release the staging catalog root for collection tests; call once, after exposing it.
pub fn release_catalog_root(
    runtime: &mut super::RustSongRuntime,
) -> Result<(), deadlib_lua::Error> {
    runtime.vm.unpin(runtime.catalog).map(|_| ())
}

/// Remove the newly installed public selection owners for isolated lifetime tests.
/// The caller separately releases the private catalog and its exposed test alias.
pub fn retire_game_state_globals(
    runtime: &mut super::RustSongRuntime,
) -> Result<(), deadlib_lua::Error> {
    for name in [
        "GAMESTATE",
        "GameState",
        "GetSongAndSteps",
        "GetPlayerOptionsString",
    ]
    .into_iter()
    .chain(crate::SONG_LUA_PLAYER_OPTIONS_KEYS)
    {
        runtime
            .vm
            .set_global(name.as_bytes(), deadlib_lua::Value::Nil)?;
    }
    Ok(())
}

pub fn rebuild_sl(
    runtime: &mut super::RustSongRuntime,
    context: &crate::SongLuaCompileContext,
) -> Result<(), deadlib_lua::Error> {
    super::managers::rebuild_sl(runtime.vm_mut(), context)
}

pub fn gameplay_player_base(
    runtime: &mut super::RustSongRuntime,
    context: &crate::SongLuaCompileContext,
    index: usize,
) -> Result<super::ActorScene, deadlib_lua::Error> {
    let catalog = runtime.vm.rooted(runtime.catalog)?;
    runtime.actors.player_base(
        &mut runtime.vm,
        context,
        index,
        runtime.statistics.clone(),
        catalog,
    )
}

pub fn note_field(
    runtime: &mut super::RustSongRuntime,
    player: usize,
    style: &str,
) -> Result<super::ActorScene, deadlib_lua::Error> {
    runtime.actors.note_field(&mut runtime.vm, player, style)
}

pub fn gameplay_player(
    runtime: &mut super::RustSongRuntime,
    context: &crate::SongLuaCompileContext,
    index: usize,
) -> Result<super::ActorScene, deadlib_lua::Error> {
    let catalog = runtime.vm.rooted(runtime.catalog)?;
    runtime.actors.gameplay_player(
        &mut runtime.vm,
        context,
        index,
        runtime.statistics.clone(),
        catalog,
    )
}

pub fn gameplay_screen(
    runtime: &mut super::RustSongRuntime,
    context: &crate::SongLuaCompileContext,
) -> Result<super::ActorScene, deadlib_lua::Error> {
    let catalog = runtime.vm.rooted(runtime.catalog)?;
    let (scene, players) = runtime.actors.gameplay_screen(
        &mut runtime.vm,
        context,
        runtime.statistics.clone(),
        catalog,
        runtime.screen.state.clone(),
    )?;
    let screen = runtime.actor_scene_root(scene).unwrap();
    let result = (|| {
        runtime.bind_screen_input(screen)?;
        runtime.publish_screen(screen, players)
    })();
    if let Err(error) = result {
        runtime.release_actor_scene(scene)?;
        return Err(error);
    }
    Ok(scene)
}

pub fn gameplay_combo(
    runtime: &mut super::RustSongRuntime,
    scene: super::ActorScene,
) -> Result<bool, deadlib_lua::Error> {
    runtime.actors.load_gameplay_theme_phase(
        &mut runtime.vm,
        scene,
        super::actors::gameplay_theme::Phase::Combo,
    )
}

pub fn gameplay_transitions(
    runtime: &mut super::RustSongRuntime,
    scene: super::ActorScene,
) -> Result<bool, deadlib_lua::Error> {
    runtime.actors.load_gameplay_theme_phase(
        &mut runtime.vm,
        scene,
        super::actors::gameplay_theme::Phase::Transitions,
    )
}

pub fn gameplay_underlay(
    runtime: &mut super::RustSongRuntime,
    scene: super::ActorScene,
) -> Result<bool, deadlib_lua::Error> {
    runtime.actors.load_gameplay_theme_phase(
        &mut runtime.vm,
        scene,
        super::actors::gameplay_theme::Phase::Underlay,
    )
}

/// Controls over real generated players; complete theme assembly is a separate phase.
pub fn top_screen_controls(
    runtime: &mut super::RustSongRuntime,
    context: &crate::SongLuaCompileContext,
) -> Result<Vec<super::ActorScene>, deadlib_lua::Error> {
    use deadlib_lua::Value;
    let scene = runtime.actors.top_screen_base(&mut runtime.vm, context)?;
    let screen = runtime.actor_scene_root(scene).unwrap();
    let mut scenes = vec![scene];
    let mut players = [Value::Nil; crate::LUA_PLAYERS];
    let mut meters = [Value::Nil; crate::LUA_PLAYERS];
    for index in 0..crate::LUA_PLAYERS {
        let player = gameplay_player(runtime, context, index)?;
        scenes.push(player);
        players[index] = runtime.actor_scene_root(player).unwrap();
        let key = runtime.vm.try_intern(b"__songlua_life_meter")?;
        let Value::Table(table) = players[index] else {
            unreachable!()
        };
        meters[index] = runtime.vm.raw_table_get(table, key)?;
    }
    let catalog = runtime.vm.rooted(runtime.catalog)?;
    runtime.actors.bind_top_screen(
        &mut runtime.vm,
        catalog,
        [screen, players[0], players[1], meters[0], meters[1]],
        context.players.each_ref().map(|p| p.enabled),
        runtime.screen.state.clone(),
    )?;
    runtime.bind_screen_input(screen)?;
    runtime.publish_screen(screen, players)?;
    Ok(scenes)
}

pub fn screen_part(
    runtime: &mut super::RustSongRuntime,
    name: &str,
) -> Result<super::ActorScene, deadlib_lua::Error> {
    runtime.actors.screen_part(&mut runtime.vm, name)
}

pub fn start_actor_transition(
    runtime: &mut super::RustSongRuntime,
    actor: deadlib_lua::Value,
) -> Result<(), deadlib_lua::Error> {
    runtime.actors.start_transition(&mut runtime.vm, actor)
}

pub fn initialize_steps_displays(
    runtime: &mut super::RustSongRuntime,
    screen: deadlib_lua::Value,
) -> Result<(), deadlib_lua::Error> {
    runtime
        .actors
        .initialize_steps_displays(&mut runtime.vm, screen)
}

pub fn captured_blocks(
    runtime: &super::RustSongRuntime,
    actor: deadlib_lua::Value,
) -> Option<Vec<crate::SongLuaOverlayCommandBlock>> {
    runtime.actors.captured_blocks(actor)
}

pub fn gameplay_overlay(
    runtime: &mut super::RustSongRuntime,
    scene: super::ActorScene,
    capture_probe: bool,
) -> Result<bool, deadlib_lua::Error> {
    runtime.actors.load_gameplay_theme_phase(
        &mut runtime.vm,
        scene,
        super::actors::gameplay_theme::Phase::Overlay { capture_probe },
    )
}
pub fn gameplay_decorations(
    runtime: &mut super::RustSongRuntime,
    scene: super::ActorScene,
) -> Result<bool, deadlib_lua::Error> {
    runtime.actors.load_gameplay_theme_phase(
        &mut runtime.vm,
        scene,
        super::actors::gameplay_theme::Phase::Decorations,
    )
}

/// Actor metadata, owned scenes and retired graphs after consuming GC notices.
pub fn actor_ownership_counts(runtime: &super::RustSongRuntime) -> [usize; 3] {
    runtime.actors.ownership_counts()
}

/// Exercise the same subscription boundary used by gameplay reconstruction.
pub fn reset_actor_subscriptions(
    runtime: &mut super::RustSongRuntime,
) -> Result<(), deadlib_lua::Error> {
    runtime.actors.clear_message_subscriptions(&mut runtime.vm)
}

/// Pending discovery order after real startup, before any future command runs.
pub fn song_startup_queue(runtime: &super::RustSongRuntime) -> Vec<deadlib_lua::Value> {
    runtime.actors.song_startup_queue()
}

pub(crate) struct StaticScheduleProbe;
pub fn static_schedule_startup_output(
    runtime: &mut super::RustSongRuntime,
    activation: &super::SongActivation,
    context: &crate::SongLuaCompileContext,
) -> super::SongStaticSchedule {
    let queued = runtime.actors.take_song_broadcasts();
    let mut sounds = runtime.take_prepared_sounds();
    let mut output = super::SongStaticSchedule::default();
    crate::compiled_layers::merge_runtime_messages(&mut output.messages, 0, &activation.broadcasts);
    let start = output.messages.len();
    let beat = crate::song_beat_at_elapsed_seconds(
        crate::perframe::first_update_seconds(context),
        context,
    );
    let queued = queued
        .iter()
        .map(|(_, name, params)| (beat, name.clone(), *params))
        .collect::<Vec<_>>();
    crate::compiled_layers::merge_runtime_messages(&mut output.messages, start, &queued);
    for sound in &mut sounds {
        if sound.queued_startup {
            sound.beat = beat;
        }
    }
    output.sound_events = sounds;
    output
}

/// Retain successful return values across rewind. The caller releases their roots.
pub fn rust_preserving_function_state(
    runtime: &mut super::RustSongRuntime,
    owner: deadlib_lua::Value,
    function: deadlib_lua::Value,
    args: &[deadlib_lua::Value],
) -> Result<Vec<deadlib_lua::Root>, deadlib_lua::Error> {
    use deadlib_lua::Value;
    let vm = runtime.vm_mut();
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let result = (|| {
        for value in std::iter::once(owner)
            .chain(std::iter::once(function))
            .chain(args.iter().copied())
        {
            inputs.push(vm.pin(value)?);
        }
        let Value::Table(owner) = owner else {
            return vm
                .with_native_stack(|stack| Err(stack.error(b"snapshot owner table expected")));
        };
        let saved = super::function_snapshot::Snapshot::capture_functions(vm, owner, &[function])?;
        let result = vm
            .call(function, args)
            .map(|values| values.to_vec())
            .and_then(|values| {
                for value in values {
                    outputs.push(vm.pin(value)?);
                }
                Ok(())
            });
        let restored = saved.restore(vm);
        restored?;
        result
    })();
    for root in inputs {
        vm.unpin(root)?;
    }
    match result {
        Ok(()) => Ok(outputs),
        Err(error) => {
            for root in outputs {
                vm.unpin(root)?;
            }
            Err(error)
        }
    }
}

/// Check the existing actor-source snapshot's identity retention independently
/// of the runtime registry, authored tree, globals and closure upvalues.
pub fn rust_function_snapshot_retains_actor_source(
    runtime: &mut super::RustSongRuntime,
    owner: deadlib_lua::Value,
    actor: deadlib_lua::Value,
) -> Result<bool, deadlib_lua::Error> {
    use deadlib_lua::Value;
    let vm = runtime.vm_mut();
    let (Value::Table(owner), Value::Table(actor)) = (owner, actor) else {
        return vm.with_native_stack(|stack| Err(stack.error(b"snapshot tables expected")));
    };
    let pin = vm.pin(Value::Table(actor))?;
    let saved = super::function_snapshot::Snapshot::capture(vm, owner, &[actor]);
    vm.unpin(pin)?;
    let saved = saved?;
    let collected = deadlib_lua::compile_named(
        b"collectgarbage('collect');collectgarbage('collect')",
        b"=snapshot identity collection",
    )
    .and_then(|code| vm.execute(&code).map(|_| ()));
    let retained = collected.map(|()| vm.raw_table_len(actor).is_ok());
    saved.restore(vm)?;
    retained
}

pub fn rust_preserving_command_locals<T>(
    runtime: &mut super::RustSongRuntime,
    probe: impl FnOnce(&mut super::RustSongRuntime) -> Result<T, deadlib_lua::Error>,
) -> Result<T, deadlib_lua::Error> {
    runtime.preserving_command_locals(probe)
}

pub fn rust_deferred_actor_messages(
    runtime: &super::RustSongRuntime,
) -> Vec<(deadlib_lua::TableId, String, String, bool)> {
    runtime.actors.deferred_message_states()
}

pub fn rust_seed_message_sound_block(
    runtime: &super::RustSongRuntime,
    actor: deadlib_lua::Value,
    played: bool,
) -> bool {
    runtime.actors.seed_message_sound_block(actor, played)
}

pub fn rust_clear_deferred_actor_messages(
    runtime: &mut super::RustSongRuntime,
) -> Result<(), deadlib_lua::Error> {
    runtime
        .actors
        .clear_deferred_message_states(&mut runtime.vm)
}

/// Supply the same rooted selection as the static compiler without running its
/// earlier readers again. The production pass consumes both selected inputs.
pub fn compile_perframes_for_tables(
    runtime: &mut super::RustSongRuntime,
    tables: [Option<deadlib_lua::Value>; 2],
    context: &crate::SongLuaCompileContext,
    targets: &[super::SongMessageTarget<'_>],
    tracked: &super::SongTrackedActors,
    messages: &[crate::SongLuaMessageEvent],
) -> Result<
    (
        Vec<crate::SongLuaEaseWindow>,
        Vec<crate::SongLuaOverlayEase>,
        crate::SongLuaCompileInfo,
    ),
    String,
> {
    let mut roots = super::song_compiler_roots::Roots::default();
    let mut selected = [None, None];
    for (slot, value) in selected.iter_mut().zip(tables) {
        *slot = value.map(|v| roots.keep(&mut runtime.vm, v)).transpose()?;
    }
    let mut keeper = super::SongStaticTables {
        roots: Some(roots),
        perframes: selected,
        prefix_is_runtime: false,
        global_perframes_are_runtime: false,
        global_mods_are_runtime: false,
        global_mod_eases_are_runtime: false,
        global_actions_are_runtime: false,
    };
    let result =
        runtime.compile_song_static_perframes(&mut keeper, context, targets, tracked, messages);
    assert_eq!(runtime.song_static_perframe_tables(&keeper)?, [None, None]);
    runtime.release_song_static_tables(&mut keeper)?;
    result
}

pub struct RustUpdateOptionsProbe(super::actors::update_options::Options);
impl RustUpdateOptionsProbe {
    pub fn new(runtime: &mut super::RustSongRuntime) -> Result<Self, deadlib_lua::Error> {
        runtime.actors.update_options(&mut runtime.vm).map(Self)
    }
    pub fn sample(
        &mut self,
        runtime: &mut super::RustSongRuntime,
        speeds: bool,
    ) -> Result<[crate::SongLuaUpdateModState; crate::LUA_PLAYERS], deadlib_lua::Error> {
        use crate::perframe::ModState;
        self.0.sample(&mut runtime.vm, speeds).map(|values| {
            values.map(|snapshot| {
                snapshot
                    .entries()
                    .map(|(name, &value)| (name.to_owned(), value))
                    .collect()
            })
        })
    }
    pub fn release(
        &mut self,
        runtime: &mut super::RustSongRuntime,
    ) -> Result<(), deadlib_lua::Error> {
        self.0.release(&mut runtime.vm)
    }
}

mod update_driver;
pub use update_driver::*;

/// Inspect the production direct-child traversal without invoking authored getters.
/// The active scene continues to own every returned actor.
pub fn actor_direct_children(
    runtime: &mut super::RustSongRuntime,
    actor: deadlib_lua::Value,
) -> Result<Vec<deadlib_lua::Value>, deadlib_lua::Error> {
    runtime
        .actors
        .perframe_access()
        .fixture_children(&mut runtime.vm, actor)
}
/// Read the production Sprite Frames decoder.
pub fn read_sprite_states(
    runtime: &mut super::RustSongRuntime,
    actor: deadlib_lua::Value,
) -> Result<Vec<crate::SongLuaSpriteState>, deadlib_lua::Error> {
    runtime
        .actors
        .perframe_access()
        .fixture_sprite_states(&mut runtime.vm, actor)
}
/// Drive the same recursive InitState operation as foreground activation.
pub fn init_actor_native_state(
    runtime: &mut super::RustSongRuntime,
    actor: deadlib_lua::Value,
) -> Result<(), deadlib_lua::Error> {
    runtime
        .actors
        .perframe_access()
        .fixture_init_state(&mut runtime.vm, actor)
}
/// Observe the active entry root and compiler actor sequence; the scene owns them.
pub fn live_scene_actor_handles(
    scene: &super::SongLiveScene,
) -> Result<(deadlib_lua::Value, Vec<deadlib_lua::Value>), deadlib_lua::Error> {
    scene.fixture_actor_handles()
}

/// Inspect the actual native child lookup on frames and leaf actors alike.
pub fn actor_named_children(
    runtime: &mut super::RustSongRuntime,
    actor: deadlib_lua::Value,
) -> Result<deadlib_lua::Root, deadlib_lua::Error> {
    runtime
        .actors
        .perframe_access()
        .fixture_named_children(&mut runtime.vm, actor)
}

/// Drive the production publication operation on one existing visual slot,
/// including partial writes when a later authored field read fails.
pub fn publish_live_visual_content<N, M>(
    runtime: &mut super::RustSongRuntime,
    actor: deadlib_lua::Value,
    kind: &mut crate::SongLuaOverlayKind<N, M, deadlib_present::actors::TextAttribute>,
) -> Result<(), String> {
    runtime
        .actors
        .publish_visual_content(&mut runtime.vm, actor, kind)
}

/// Observe the production raw query encoder, including identity and ancestry checks.
pub fn live_query_value(
    runtime: &mut super::RustSongRuntime,
    value: deadlib_lua::Value,
    depth: usize,
) -> Result<serde_json::Value, deadlib_lua::Error> {
    runtime
        .actors
        .fixture_query_value(&mut runtime.vm, value, depth)
}
pub fn seed_live_query_objects(
    runtime: &super::RustSongRuntime,
    objects: &[(deadlib_lua::Value, serde_json::Value)],
) {
    runtime.actors.fixture_query_objects(objects)
}
pub fn hook_live_query(
    runtime: &mut super::RustSongRuntime,
    owner: deadlib_lua::Value,
    class: &str,
    name: &str,
) -> Result<(), deadlib_lua::Error> {
    runtime
        .actors
        .fixture_query_hook(&mut runtime.vm, owner, class, name)
}
pub fn live_query_hooks(runtime: &super::RustSongRuntime) -> Vec<(deadlib_lua::Value, String)> {
    runtime.actors.fixture_query_hooks()
}

/// Inspect construction inputs separately from the live GAMESTATE selection.
pub fn live_scene_initial_context(scene: &super::SongLiveScene) -> &crate::SongLuaCompileContext {
    scene.fixture_initial_context()
}
/// Consume the actual live output phase after direct authored operations.
pub fn take_live_scene_output(
    scene: &mut super::SongLiveScene,
) -> Result<crate::SongLuaLiveOutput, deadlib_lua::Error> {
    scene.fixture_take_output()
}

/// Observe the actual native tween's current pose without exporting a Lua snapshot.
pub fn read_live_tween_pose(
    runtime: &super::RustSongRuntime,
    value: deadlib_lua::Value,
    state: &mut crate::SongLuaOverlayState,
) -> bool {
    runtime.actors.apply_song_replay_pose(value, state)
}

/// Existing native registry and retained draw storage, without consuming output.
pub fn live_actor_registry(runtime: &super::RustSongRuntime) -> Vec<deadlib_lua::Value> {
    runtime.actors.fixture_live_registry()
}
pub fn live_draw_storage(runtime: &super::RustSongRuntime) -> (usize, usize, usize) {
    runtime.actors.fixture_draw_storage()
}
pub fn live_command_depth(runtime: &super::RustSongRuntime) -> usize {
    runtime.actors.fixture_command_depth()
}
pub fn set_live_command_depth(runtime: &super::RustSongRuntime, depth: usize) {
    runtime.actors.fixture_set_command_depth(depth)
}
pub fn install_live_command_depth_probe(
    runtime: &mut super::RustSongRuntime,
) -> Result<(), deadlib_lua::Error> {
    runtime
        .actors
        .fixture_command_depth_function(&mut runtime.vm)
}
pub fn advance_live_actor(
    runtime: &mut super::RustSongRuntime,
    actor: deadlib_lua::Value,
    delta: f32,
) -> Result<(), deadlib_lua::Error> {
    runtime
        .actors
        .fixture_advance_actor(&mut runtime.vm, actor, delta)
}
pub fn invoke_live_tap_callback(
    runtime: &mut super::RustSongRuntime,
    field: deadlib_lua::Value,
    callback: deadlib_lua::Value,
    tap: deadsync_gameplay::GameplayNoteFieldTap,
    columns: usize,
) -> Result<deadsync_gameplay::GameplayNoteFieldTap, deadlib_lua::Error> {
    runtime
        .actors
        .gameplay_tap(&mut runtime.vm, field, callback, tap, columns)
}

/// Drive the same native pose export used by DeepCopy, with existing snapshot selection/restore.
pub fn live_destination_vertices(
    runtime: &super::RustSongRuntime,
    value: deadlib_lua::Value,
) -> Option<[[f32; 4]; 4]> {
    runtime.actors.fixture_destination_vertices(value)
}
pub fn copy_live_actor_pose(
    runtime: &mut super::RustSongRuntime,
    source: deadlib_lua::Value,
    into: deadlib_lua::Value,
) -> Result<(), deadlib_lua::Error> {
    runtime
        .actors
        .fixture_copy_live_pose(&mut runtime.vm, source, into)
}
pub fn live_color_storage(
    runtime: &super::RustSongRuntime,
    value: deadlib_lua::Value,
) -> Option<(usize, [f32; 4], [f32; 4])> {
    runtime.actors.fixture_live_color_storage(value)
}
pub fn export_live_actor_state(
    runtime: &mut super::RustSongRuntime,
    source: deadlib_lua::Value,
    mutable: bool,
) -> Result<deadlib_lua::Root, deadlib_lua::Error> {
    runtime
        .actors
        .fixture_export_live_state(&mut runtime.vm, source, mutable)
}
pub fn restore_live_actor_state(
    runtime: &mut super::RustSongRuntime,
    into: deadlib_lua::Value,
    saved: deadlib_lua::Value,
) -> Result<(), deadlib_lua::Error> {
    runtime
        .actors
        .fixture_restore_live_state(&mut runtime.vm, into, saved)
}
pub fn end_live_update_capture(scene: &super::SongLiveScene) {
    scene.fixture_end_update_capture()
}

/// Observe native effect-clock ownership rather than a retired Lua mirror table.
pub fn live_actor_has_motion(runtime: &super::RustSongRuntime, value: deadlib_lua::Value) -> bool {
    runtime.actors.fixture_has_motion(value)
}
pub fn base_transform_state_keys() -> impl Iterator<Item = &'static str> {
    crate::base_transform::BaseField::ALL
        .into_iter()
        .map(crate::base_transform::BaseField::state_key)
}
pub fn render_flag_state_keys() -> impl Iterator<Item = &'static str> {
    crate::render_flags::RenderFlag::ALL
        .into_iter()
        .map(crate::render_flags::RenderFlag::state_key)
}

/// Drive the actual sprite publication reader/comparator without texture lookup.
pub fn sync_sprite_states(
    runtime: &mut super::RustSongRuntime,
    actor: deadlib_lua::Value,
    states: &mut std::sync::Arc<[crate::SongLuaSpriteState]>,
) -> Result<(), deadlib_lua::Error> {
    runtime
        .actors
        .fixture_sync_sprite_states(&mut runtime.vm, actor, states)
}

/// Install the production calendar closures with explicit original fixture dates.
pub fn install_calendar_for_date(
    runtime: &mut super::RustSongRuntime,
    context: &crate::SongLuaCompileContext,
    date: crate::SongLuaDateGlobals,
) -> Result<(), deadlib_lua::Error> {
    super::managers::compat::install_calendar_for_date(&mut runtime.vm, context, date)
}

/// Prepare one rooted reader using the actual actor helper captures. Reuse it
/// across fixture calls and release it through Vm::unpin after the measurement.
pub fn prepare_actor_reader<T: std::any::Any + Send + Sync>(runtime:&mut super::RustSongRuntime, actor:deadlib_lua::Value, callback:deadlib_lua::HostFunction, data:T)->Result<deadlib_lua::Root,deadlib_lua::Error>{
    runtime.actors.fixture_reader(&mut runtime.vm,actor,callback,data)
}
pub use super::actors::{fixture_text_content,fixture_text_value,fixture_text_attributes,fixture_sync_text,fixture_sync_text_attributes,fixture_vector_components};

/// Observe the production multitap timing helper used by compiled actor curves.
pub fn multitap_y_offset(context:&crate::SongLuaCompileContext,player:usize,beat:f32,pos:f32)->f32{crate::multitap::multitap_y_offset(context,player,beat,pos)}

pub use super::actors::fixture_vertex_colors;

pub use super::actors::{FixtureStateOutput,fixture_field_text};
pub fn prepare_actor_state_reader(runtime:&mut super::RustSongRuntime,actor:deadlib_lua::Value,live:bool)->Result<(deadlib_lua::Root,FixtureStateOutput),deadlib_lua::Error>{runtime.actors.fixture_state_reader(&mut runtime.vm,actor,live)}

/// Drive the production timer directly, retaining negative deltas and the
/// native single-subtraction rule independently of frame-delta policy.
pub fn prepare_effect_clock(runtime:&mut super::RustSongRuntime,actor:deadlib_lua::Value)->Result<deadlib_lua::Root,deadlib_lua::Error>{runtime.actors.fixture_effect_clock(&mut runtime.vm,actor)}

pub use super::actors::fixture_tap_note;

/// Promote base metadata through the real constructor helpers and inspect the
/// actual presence bits, independently of the public render defaults.
pub fn read_base_transform_values(runtime:&mut super::RustSongRuntime,actor:deadlib_lua::Value)->Result<[Option<f32>;6],deadlib_lua::Error>{runtime.actors.fixture_base_values(&mut runtime.vm,actor)}
pub fn base_transform_size()->usize{std::mem::size_of::<crate::base_transform::BaseTransform>()}

pub fn prepare_capture_reset(runtime:&mut super::RustSongRuntime,actor:deadlib_lua::Value)->Result<deadlib_lua::Root,deadlib_lua::Error>{runtime.actors.fixture_capture_reset(&mut runtime.vm,actor)}
pub fn invalidate_actor_update_plans(runtime:&mut super::RustSongRuntime)->Result<(),deadlib_lua::Error>{runtime.actors.fixture_invalidate_plans(&mut runtime.vm)}
pub use super::actors::fixture_mesh_points;

/// Apply the compiler's existing sampled getter handoff to one owned actor.
pub fn set_actor_sampled_value(runtime:&mut super::RustSongRuntime,actor:deadlib_lua::Value,target:crate::SongLuaOverlayUpdateTarget,value:&crate::SongLuaOverlayUpdateValue)->Result<(),deadlib_lua::Error>{runtime.actors.perframe_access().update_getter(&mut runtime.vm,actor,target,value)}

pub use super::actors::{FixtureNativeSnapshot,fixture_vector_value};
pub fn save_native_actor_state(runtime:&mut super::RustSongRuntime,actor:deadlib_lua::Value)->Result<FixtureNativeSnapshot,deadlib_lua::Error>{runtime.actors.fixture_save_native(&mut runtime.vm,actor)}
pub fn restore_native_actor_state(runtime:&mut super::RustSongRuntime,snapshot:FixtureNativeSnapshot)->Result<(),deadlib_lua::Error>{runtime.actors.fixture_restore_native(&mut runtime.vm,snapshot)}
pub fn native_shadow_storage(runtime:&super::RustSongRuntime,actor:deadlib_lua::Value)->(usize,[f32;2]){runtime.actors.fixture_shadow_storage(actor)}

/// Attach the existing optional sample recorder directly to a native actor.
pub fn begin_pose_observation(runtime:&mut super::RustSongRuntime,actor:deadlib_lua::Value)->Result<(),deadlib_lua::Error>{runtime.actors.fixture_begin_pose_observation(&mut runtime.vm,actor)}
pub fn observed_vertex_sample(runtime:&super::RustSongRuntime)->(usize,Option<std::sync::Arc<[[f32;4];4]>>){runtime.actors.fixture_pose_sample()}
pub fn set_pose_observation_scope(runtime:&super::RustSongRuntime,queued:usize,update:bool,message:Option<&str>){runtime.actors.fixture_pose_scope(queued,update,message)}

pub fn ensure_native_replay(runtime:&mut super::RustSongRuntime,actor:deadlib_lua::Value)->Result<(),deadlib_lua::Error>{runtime.actors.fixture_ensure_native_replay(&mut runtime.vm,actor)}

pub fn cmd_preprocess_cow(source:&str)->Result<std::borrow::Cow<'_,str>,String>{crate::cmd::preprocess_lua_cmd_syntax_cow(source)}
pub fn cmd_quoted_end(source:&str,start:usize)->Result<usize,String>{crate::cmd::lua_quoted_end(source,start)}

pub fn cmd_commands(source:&str)->impl Iterator<Item=Result<&str,String>>{crate::cmd::lua_cmd_commands(source)}
pub fn cmd_newline_count(source:&str)->usize{crate::cmd::lua_newline_count(source)}
pub fn cmd_append_newlines(out:&mut String,source:&str){crate::cmd::append_lua_newlines(out,source)}

pub fn prepare_loading_size_getter(runtime:&mut super::RustSongRuntime,actor:deadlib_lua::Value,name:&'static str)->Result<deadlib_lua::Root,deadlib_lua::Error>{runtime.actors.fixture_loading_size_getter(&mut runtime.vm,actor,name)}
