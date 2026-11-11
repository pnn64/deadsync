//! Song Lua compilation, host bindings, and playback.
#[cfg_attr(
    test,
    expect(
        dead_code,
        reason = "unit tests build actor math without the playback renderer"
    )
)]
mod actor_quaternion;
pub mod gameplay;
#[cfg(feature = "test-support")]
pub mod trace_capture;
// Playback is tested with concrete asset fixtures by tests/playback.rs.

#[cfg(not(test))]
pub mod playback;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use deadlib_present::actors::TextAlign;
use deadlib_present::anim::{EffectClock, EffectMode};

mod input_types;
pub use input_types::{
    SongLuaActorInputAction, SongLuaInputEvent, SongLuaInputScalar, SongLuaInputType,
};

pub use actors::{
    CompiledActorInputs as SongLuaCompiledActorInputs,
    CompiledActorInteractions as SongLuaCompiledActorInteractions,
};

mod cmd;
mod compat;

mod compiled_layers;

mod compile_timing;

mod bitmap_font;
mod blank_texture;
mod draw_commands;
pub use draw_commands::{
    DrawFrame as SongLuaDrawFrame, DrawOp as SongLuaDrawOp, DrawSource as SongLuaDrawSource,
};
#[cfg(feature = "test-support")]
mod clock_inputs;
mod eases;
mod files;
mod host;

pub use input::CompiledInput as SongLuaCompiledInput;

mod lua_util;

mod live;
pub use live::{
    SongLuaActorSound, SongLuaLiveBroadcast, SongLuaLiveFrame, SongLuaLiveOutput,
    SongLuaLivePlayer, SongLuaLivePlayerWrite, SongLuaLiveRequest, SongLuaLiveSourceRequest,
    SongLuaLiveTarget,
};

#[cfg(feature = "test-support")]
pub use live::SongLuaOverlayStageObservation;
pub use live_scene::SongLiveScene as SongLuaLiveScene;

mod model_texture;
#[cfg(feature = "test-support")]
pub use model_texture::replay_model_texture;
pub use model_texture::{
    SongLuaModelTextureFrame, SongLuaModelTextureUv, SongLuaTextureSample, model_texture_at,
};
mod multitap;

mod base_transform;
mod net;
mod render_flags;
mod tween_pose;

mod frame_capture;
mod option_rows;
mod perframe;
mod player_options;
mod rage_file;

use crate::{
    SONG_LUA_RUNTIME_BEAT_KEY as BEAT, SONG_LUA_RUNTIME_BPS_KEY as BPS,
    SONG_LUA_RUNTIME_DELTA_BEAT_KEY as DELTA_BEAT,
    SONG_LUA_RUNTIME_DELTA_SECONDS_KEY as DELTA_SECONDS, SONG_LUA_RUNTIME_RATE_KEY as RATE,
    SONG_LUA_RUNTIME_SECONDS_KEY as SECONDS,
};
use deadlib_lua::{Error, HostCall, Root, TableId, Value, Vm};
mod actors;
mod bake_progress;
pub use bake_progress::SongLuaBakeProgress;
#[path = "input/program/mod.rs"]
pub mod baked_program;
mod compiler;
pub use compiler::{
    compile_prepared_song_lua_layers, compile_song_lua, compile_song_lua_layers, reload_song_lua,
};
mod compiler_debug;
mod definitions;
mod function_snapshot;
mod gameplay_screen;
mod song_action;
mod song_action_compile;
pub use song_action_compile::SongActionOutput;
mod song_callback_tables;
mod song_column_eases;
mod song_column_output;
pub use song_column_output::SongColumnSplineCapture;
mod song_compiler_roots;
mod song_mod_tables;
mod song_multitap;
mod song_perframes;
mod song_update_output;
mod song_updates;
pub use song_update_output::SongStaticUpdateOutput;
pub use song_updates::SongUpdateOutput;
mod song_frame_capture;
pub use song_frame_capture::{
    SongFrameCaptureClock, SongFrameCaptureStates, SongOverlayFrameCapture,
};
mod song_prepare_globals;
mod song_static_schedule;
mod song_xero;
pub use song_callback_tables::SongCallbackTables;
pub use song_static_schedule::{SongStaticInputs, SongStaticSchedule, SongStaticTables};
mod song_ease_probe;
mod song_easing;
pub use song_ease_probe::SongFunctionEaseProbe;
mod song_easing_reader;
pub use song_easing_reader::{SongEasingNames, SongFunctionEaseInput};
mod song_tree;
pub use song_tree::{SongOverlayActor, SongOverlayActors};
mod song_activation;
mod song_discovery;
mod song_message_retry;
mod song_messages;
pub use song_message_retry::SongMessageTarget;
mod song_output;
pub use song_output::{
    SongActorFunctionBlocks, SongOverlayKindInputs, SongTrackedActor, SongTrackedActors,
};
mod song_layers;
pub use song_activation::{SongActivation, SongStartupState};
pub use song_discovery::{
    SongReplayImmediateUpdates, SongReplayPropertyUpdates, SongReplayScheduledUpdate,
    SongTweenReplay,
};
pub use song_layers::{SongInitialStates, SongLayerPlan};
mod arrows;
mod host_clocks;
mod input;
pub use actors::{
    ActorCommandCapture, ActorMesh, ActorScene, ActorSnapshot, BitmapText, CompiledActorInputs,
    CompiledActorInteractions, FrameTexture, GraphDisplay, SongMeterDisplay,
};
pub use input::CompiledInput;
mod live_actors;
mod live_layers;
pub use live_layers::SongLiveLayers;
mod live_scene;
pub use live_scene::{SongLiveScene, SongLiveSceneBindings};
mod live_runtime;
pub use live_runtime::SongLiveRuntime;
mod live_frames;
pub use live_frames::SongLiveFrames;
mod loader;
mod managers;
mod options;
mod statistics;

pub use options::PlayerStateStatus;
mod random;
pub use random::RandomSnapshot;
#[cfg(feature = "test-support")]
pub mod test_support;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

const FREEZE: &str = "__songlua_freeze";
const DELAY: &str = "__songlua_delay";
const BPMS: &str = "__songlua_timing_bpms";
const PLAYERS: &str = PLAYER_SONG_POSITIONS_KEY;

#[derive(Default)]
struct Clock {
    timing: Option<deadsync_rules::timing::TimingData>,
    no_offset: Option<deadsync_rules::timing::TimingData>,
    live: AtomicBool,
}

#[derive(Clone, Copy)]
enum Operation {
    Float(&'static str),
    Boolean(&'static str),
    MusicSeconds,
    Values(bool),
    SetValues(bool),
    SetBeat,
    Reset,
}

#[derive(Clone)]
struct Binding {
    clock: Arc<Clock>,
    operation: Operation,
}

#[cfg(feature = "test-support")]
mod observations;

/// An independent Rust VM with the song clock's Rust host bindings installed.
/// All retained Lua objects are explicit roots or traced callback captures.
/// Separate instances have separate heaps and clock state.
pub struct SongRuntime {
    #[cfg(feature = "test-support")]
    observations: observations::Observations,
    vm: Vm,
    arrows: arrows::Arrows,
    compiler_globals_installed: bool,
    random: random::RandomState,
    loader: Root,
    actor_loader: definitions::Loader,
    actors: actors::Actors,
    catalog: Root,
    player_status: Arc<Mutex<[PlayerStateStatus; LUA_PLAYERS]>>,
    statistics: Arc<Mutex<statistics::State>>,
    input: input::Input,
    screen: managers::screen::Screen,
    clock: Arc<Clock>,
    runtime: Root,
    position: Root,
    players: Vec<Root>,
    operations: [Root; 6],
}

impl SongRuntime {
    /// Enter compiler preparation before loading declarations. New actors capture
    /// writes; the first Init pass holds queues across subsequent layers and On.
    /// Returns false if preparation already began or this is a playback runtime.
    pub fn begin_actor_preparation(&mut self) -> Result<bool, Error> {
        let began = self.actors.begin_preparation(&mut self.vm)?;
        if began {
            let function = self.vm.get_global(b"LoadActor")?;
            self.actor_loader.set_function(&mut self.vm, function)?;
        }
        Ok(began)
    }

    /// Release the Init/On barrier after the compiler has captured startup state
    /// and run Update(0). Discover zero-time commands; delayed commands and queued
    /// messages stay pending for chronological frame replay. This does not rewind
    /// script state or install that replay; those are separate compiler phases.
    pub fn discover_actor_startup_commands(&mut self) -> Result<(), Error> {
        self.actors.discover_startup(&mut self.vm)
    }

    /// Copy the actor's captured writes and unconsumed command/tween metadata.
    pub fn actor_command_capture(&self, actor: Value) -> Option<ActorCommandCapture> {
        self.actors.command_capture(actor)
    }

    pub fn new(context: &SongLuaCompileContext) -> Result<Self, Error> {
        Self::new_with_noteskin_resolver(context, crate::SongLuaNoteskinResolver::default())
    }

    /// Bind the application's existing skin resolver to this isolated song VM.
    pub fn new_with_noteskin_resolver(
        context: &SongLuaCompileContext,
        noteskins: crate::SongLuaNoteskinResolver,
    ) -> Result<Self, Error> {
        #[cfg(feature = "test-support")]
        let observations = observations::Observations::default();
        let mut vm = Vm::new();
        #[cfg(feature = "test-support")]
        if context.initial_lua_gc_stopped {
            vm.with_native_stack(|stack| stack.gc(deadlib_lua::GcOperation::Stop).map(|_| ()))?;
        }
        let source_loader = vm.get_global(b"loadstring")?;
        let source_loader_root = vm.pin(source_loader)?;
        let random = random::install(&mut vm, context.random_seed)?;
        let loader = loader::install(&mut vm, &context.song_dir)?;
        let actor_loader = definitions::install(&mut vm)?;
        let clock = Arc::new(Clock {
            timing: context.song_timing.clone(),
            no_offset: context.song_timing.clone().map(|mut timing| {
                timing.set_global_offset_seconds(0.0);
                timing
            }),
            live: AtomicBool::new(context.live),
        });
        let actors = actors::Actors::install(
            &mut vm,
            clock.clone(),
            crate::song_lua_human_player_count(context),
            #[cfg(feature = "test-support")]
            observations.clone(),
        )?;
        actors.install_sound_manager(&mut vm, &context.song_dir)?;
        let actor_file_loader = vm.rooted(loader)?;
        actors.bind_source_loader(&mut vm, actor_file_loader, &context.song_dir)?;
        let catalog = managers::install(
            &mut vm,
            context,
            #[cfg(feature = "test-support")]
            observations.clone(),
        )?;
        actors.install_host_compat(&mut vm)?;
        let input = input::install(&mut vm)?;
        let screen = managers::screen::install(
            &mut vm,
            #[cfg(feature = "test-support")]
            observations.clone(),
        )?;
        let runtime = vm.try_new_table()?;
        let runtime_root = vm.pin(Value::Table(runtime))?;
        for key in [BEAT, SECONDS, DELTA_BEAT, DELTA_SECONDS] {
            put(&mut vm, runtime, key, Value::Number(0.0))?;
        }
        put(
            &mut vm,
            runtime,
            BPS,
            number(if context.live {
                10.0
            } else {
                song_display_bps(context)
            }),
        )?;
        put(&mut vm, runtime, RATE, number(song_music_rate(context)))?;
        put(&mut vm, runtime, FREEZE, Value::Boolean(false))?;
        put(&mut vm, runtime, DELAY, Value::Boolean(false))?;
        let tonumber = vm.get_global(b"tonumber")?;
        // Keep the original primitive conversion even if a script replaces it.
        let tonumber_root = vm.pin(tonumber)?;
        let song_position = position(&mut vm, &clock, runtime, tonumber)?;
        let mut players = Vec::new();
        if context.live {
            let positions = vm.try_new_table()?;
            put(&mut vm, runtime, PLAYERS, Value::Table(positions))?;
            for index in 1..=LUA_PLAYERS {
                let player = vm.try_new_table()?;
                vm.raw_table_set(positions, Value::Number(index as f64), Value::Table(player))?;
                for key in [BEAT, SECONDS, BPS, RATE, FREEZE, DELAY] {
                    let key = vm.try_intern(key.as_bytes())?;
                    let value = vm.raw_table_get(runtime, key)?;
                    vm.raw_table_set(player, key, value)?;
                }
                players.push(position(&mut vm, &clock, player, tonumber)?);
            }
        }
        let bpms = vm.try_new_table()?;
        put(&mut vm, runtime, BPMS, Value::Table(bpms))?;
        for (index, &(beat, bpm)) in context.song_timing_bpms.iter().enumerate() {
            let segment = vm.try_new_table()?;
            vm.raw_table_set(
                bpms,
                Value::Number((index + 1) as f64),
                Value::Table(segment),
            )?;
            for (slot, value) in [
                beat,
                bpm,
                song_elapsed_seconds_at(beat, context) * song_music_rate(context),
            ]
            .into_iter()
            .enumerate()
            {
                vm.raw_table_set(segment, Value::Number((slot + 1) as f64), number(value))?;
            }
        }
        let mut roots = Vec::new();
        for operation in [
            Operation::Values(false),
            Operation::Values(true),
            Operation::SetValues(false),
            Operation::SetValues(true),
            Operation::SetBeat,
            Operation::Reset,
        ] {
            let function = vm.new_host_function_with_data(
                callback,
                &[tonumber],
                Binding {
                    clock: clock.clone(),
                    operation,
                },
            )?;
            roots.push(vm.pin(function)?);
        }
        vm.set_global(SONG_LUA_RUNTIME_KEY.as_bytes(), Value::Table(runtime))?;
        host_clocks::install(&mut vm, clock.clone(), context)?;
        let player_status = Arc::new(Mutex::new([PlayerStateStatus::default(); LUA_PLAYERS]));
        // Preparation has distinct PlayerState position wrappers over the shared
        // clock. Live wrappers instead retain each player's original backing.
        let mut preparation_positions = Vec::new();
        if !context.live {
            for _ in 0..LUA_PLAYERS {
                preparation_positions.push(position(&mut vm, &clock, runtime, tonumber)?);
            }
        }
        let positions = if context.live {
            &players
        } else {
            &preparation_positions
        };
        let positions = [vm.rooted(positions[0])?, vm.rooted(positions[1])?];
        let catalog_value = vm.rooted(catalog)?;
        managers::install_player_states(
            &mut vm,
            catalog_value,
            positions,
            context,
            player_status.clone(),
            #[cfg(feature = "test-support")]
            observations.clone(),
        )?;
        let position_value = vm.rooted(song_position)?;
        let messages = actors.host_messages(&vm)?;
        managers::install_game_state(
            &mut vm,
            catalog_value,
            Value::Table(runtime),
            position_value,
            messages,
            context,
            #[cfg(feature = "test-support")]
            observations.clone(),
        )?;
        let statistics = Arc::new(Mutex::new(statistics::State::default()));
        managers::statistics::install(
            &mut vm,
            catalog_value,
            context,
            statistics.clone(),
            clock.clone(),
        )?;
        let file_loader = vm.rooted(loader)?;
        let arrows = arrows::install(&mut vm, catalog_value, clock.clone(), context)?;
        let template = actors.host_template(&vm)?;
        managers::install_noteskin(
            &mut vm,
            context,
            noteskins,
            file_loader,
            source_loader,
            template,
        )?;
        vm.unpin(source_loader_root)?;
        for root in preparation_positions {
            vm.unpin(root)?;
        }
        vm.unpin(tonumber_root)?;
        Ok(Self {
            #[cfg(feature = "test-support")]
            observations,
            vm,
            arrows,
            compiler_globals_installed: false,
            random,
            loader,
            actor_loader,
            actors,
            catalog,
            player_status,
            statistics,
            input,
            screen,
            clock,
            runtime: runtime_root,
            position: song_position,
            players,
            operations: roots.try_into().expect("six clock operations"),
        })
    }

    /// Register additional song bindings or execute scripts in this same VM.
    /// Values obtained here follow `deadlib-lua`'s ordinary rooting contract.
    pub fn vm_mut(&mut self) -> &mut Vm {
        &mut self.vm
    }

    /// Materialize an actor declaration in this song's VM and run its Init commands.
    /// Unsupported classes return an explicit error until their bindings are migrated.
    pub fn instantiate_actor(&mut self, declaration: Value) -> Result<ActorScene, Error> {
        self.actors.instantiate(&mut self.vm, declaration)
    }

    /// Construct and own the complete actor graph without running Init. The
    /// compiler assigns its external parent and captures initial state next.
    pub fn construct_actor(&mut self, declaration: Value) -> Result<ActorScene, Error> {
        self.actors.construct(&mut self.vm, declaration)
    }

    /// Run child-first Init once. An error retains partial state and ownership
    /// so the caller can inspect, retry or explicitly release the scene.
    pub fn initialize_actor_scene(&mut self, scene: ActorScene) -> Result<bool, Error> {
        self.actors.initialize(&mut self.vm, scene)
    }

    /// Set the root's parent without adding it to the parent's child list.
    /// Keep the parent's scene owned through Init and subsequent attachment.
    pub fn set_actor_scene_parent(
        &mut self,
        scene: ActorScene,
        parent: Option<Value>,
    ) -> Result<bool, Error> {
        self.actors.parent(&mut self.vm, scene, parent, false)
    }

    /// Append the initialized root once, sharing its subtree with the parent's
    /// scene owners. Releasing this scene then leaves the attached actors alive.
    pub fn append_actor_scene(&mut self, scene: ActorScene, parent: Value) -> Result<bool, Error> {
        self.actors.parent(&mut self.vm, scene, Some(parent), true)
    }

    pub fn actor_scene_root(&self, scene: ActorScene) -> Option<Value> {
        self.actors.root(scene)
    }

    pub fn actor_scene_snapshot(&self, scene: ActorScene) -> Option<Vec<ActorSnapshot>> {
        self.actors.snapshot(scene)
    }

    /// Local render states with the retained effect phase and native spin flag.
    /// `frame_clock` is [music seconds, song beat] at the sampling boundary.
    pub fn actor_scene_render_states(
        &self,
        scene: ActorScene,
        frame_clock: [f32; 2],
    ) -> Option<Vec<crate::SongLuaOverlayState>> {
        self.actors.render_states(scene, frame_clock)
    }

    /// Deliver Begin and On to this actor subtree once. Screen activation and
    /// its initial zero-delta update are separate host phases.
    pub fn start_actor_scene(&mut self, scene: ActorScene) -> Result<bool, Error> {
        self.actors.start(&mut self.vm, scene)
    }

    /// Enter retained playback after preparation/startup. This preserves the
    /// production boundary for publishing cached rotation getters on writes.
    pub fn begin_actor_playback(&self) {
        self.actors.begin_playback();
    }

    /// Advance the subtree's existing queues and callbacks in native order.
    pub fn advance_actor_scene(&mut self, scene: ActorScene, delta: f64) -> Result<bool, Error> {
        self.actors.advance(&mut self.vm, scene, delta)
    }

    /// Capture the scene's custom draw callbacks at its current clock and pose.
    /// Run after Update. Live capture replaces the previous ordinary frame,
    /// retaining immediate command submissions until the caller consumes them.
    pub fn draw_actor_scene(&mut self, scene: ActorScene) -> Result<bool, Error> {
        self.actors.draw(&mut self.vm, scene)
    }

    /// Resolve captured actor identities to the compiler/playback source map,
    /// consuming the pending immutable frames. Unmapped references are errors.
    pub fn take_actor_draw_frames(
        &mut self,
        sources: &[(Value, crate::SongLuaDrawSource)],
    ) -> Result<Vec<(usize, crate::SongLuaDrawFrame)>, String> {
        self.actors.take_draw_frames(&mut self.vm, sources)
    }

    /// End an owned screen tree once: detach its top-screen global, deliver End
    /// to descendants of its synthetic root, then stop its sounds. End does not
    /// advance queued work or release Lua instances. An ended tree cannot advance.
    /// Returns false for an already ended, released or foreign scene.
    pub fn end_actor_scene(&mut self, scene: ActorScene) -> Result<bool, Error> {
        self.actors.end(&mut self.vm, scene)
    }

    /// Retire native actors and release their Lua roots. Native scene operations
    /// and renderer reads reject retired actors; retained Lua resource handles
    /// keep their captured values until those handles are collected.
    pub fn release_actor_scene(&mut self, scene: ActorScene) -> Result<bool, Error> {
        self.actors.release(&mut self.vm, scene)
    }

    /// Relinquish a screen's ownership after End, retaining Lua-visible actor
    /// state until the outgoing graph becomes unreachable. The scene handle
    /// stops accepting host operations immediately; saved Lua actors remain usable.
    pub fn retire_actor_scene(&mut self, scene: ActorScene) -> Result<bool, Error> {
        self.actors.retire(&mut self.vm, scene)
    }

    pub fn take_actor_diagnostics(&self) -> Vec<String> {
        self.actors.take_diagnostics()
    }

    /// Read copied mesh vertices and their texture using the actor's current
    /// draw range. Authored field getters may execute Lua during this boundary.
    pub fn actor_multi_vertex_mesh(&mut self, actor: Value) -> Result<Option<ActorMesh>, Error> {
        self.actors.mesh(&mut self.vm, actor)
    }
    /// Read an owned text/font/attribute snapshot. Authored getters may execute Lua;
    /// the returned payload keeps no VM handles and survives scene retirement.
    pub fn actor_bitmap_text(&mut self, actor: Value) -> Result<BitmapText, Error> {
        self.actors.bitmap_text(&mut self.vm, actor)
    }

    /// Copy graph samples and body/line styles into an owned renderer payload.
    /// The returned snapshot contains no Lua handles and survives scene retirement.
    pub fn actor_graph_display(&mut self, actor: Value) -> Result<GraphDisplay, Error> {
        self.actors.graph(&mut self.vm, actor)
    }
    /// Resolve Model Meshes/Materials/Bones using the actor's current path fields.
    pub fn actor_model_paths(
        &mut self,
        actor: Value,
    ) -> Result<Option<[std::path::PathBuf; 3]>, Error> {
        self.actors.model_paths(&mut self.vm, actor)
    }

    /// Read prepared model layers with the application's existing model loader.
    pub fn actor_model_layers<S, V>(
        &mut self,
        actor: Value,
        read_slots: impl Fn(
            &std::path::Path,
            &std::path::Path,
            &std::path::Path,
        ) -> Result<Arc<[S]>, String>,
        read_layer: impl Fn(&S) -> Option<crate::SongLuaOverlayModelLayer<V>>,
    ) -> Result<Option<Arc<[crate::SongLuaOverlayModelLayer<V>]>>, String> {
        let Some(paths) = self.actor_model_paths(actor).map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        let slots = read_slots(&paths[0], &paths[1], &paths[2])?;
        Ok(crate::lua_util::overlay_model_layers_from_slots(
            &slots, read_layer,
        ))
    }

    /// Start the existing preparation material clocks for one actor's layers.
    /// This also accepts model-backed noteskin actors. Static layers retain their
    /// indices with empty sample tracks. Reinstallation starts a fresh capture.
    pub fn install_actor_model_materials<V>(
        &mut self,
        actor: Value,
        layers: &[crate::SongLuaOverlayModelLayer<V>],
    ) -> Result<(), String> {
        self.actors.install_model_materials(actor, layers)
    }

    /// Record a preparation frame after advancing the scene. The compiler owns
    /// the finite frame schedule and transfers these samples to immutable output.
    pub fn sample_model_materials(&mut self, second: f32) {
        self.actors.sample_model_materials(second);
    }

    /// Finish and detach material capture for this actor, retaining layer order.
    pub fn take_actor_model_material_samples(
        &mut self,
        actor: Value,
    ) -> Result<Vec<Arc<[crate::SongLuaTextureSample]>>, String> {
        self.actors.take_model_material_samples(actor)
    }

    /// Read the existing meter stream width and owned render state, if drawable.
    pub fn actor_song_meter_display(
        &mut self,
        actor: Value,
    ) -> Result<Option<SongMeterDisplay>, Error> {
        self.actors.song_meter(&mut self.vm, actor)
    }

    /// Read allocation settings, or current requests before successful creation.
    pub fn actor_frame_texture(&mut self, actor: Value) -> Result<FrameTexture, Error> {
        self.actors.framebuffer(&mut self.vm, actor)
    }

    /// Read the current proxy target, including authored classification fields.
    /// Actor identities remain runtime-local until resolved to overlay indices.
    pub fn actor_proxy_target(
        &mut self,
        actor: Value,
    ) -> Result<Option<crate::SongLuaProxyTarget>, Error> {
        self.actors.proxy(&mut self.vm, actor)
    }

    /// Drain immediate sound commands, retaining each native actor identity.
    pub fn take_actor_sounds(&self) -> Vec<crate::SongLuaActorSound> {
        self.actors.take_sounds()
    }

    /// Begin the compiler's media-capture phase. This does not advance time or
    /// execute queued commands. A new phase clears its previous captured events.
    pub fn begin_actor_capture(&self, queued_startup: bool) {
        self.actors.begin_sound_capture(queued_startup);
    }
    pub fn take_prepared_sounds(&self) -> Vec<crate::SongLuaSoundEvent> {
        self.actors.take_prepared_sounds()
    }
    pub fn take_sprite_textures(&self) -> Vec<(u64, crate::SongLuaSpriteTexture)> {
        self.actors.take_sprite_textures()
    }
    pub fn take_judgment_textures(&self) -> Vec<(usize, crate::SongLuaJudgmentTexture)> {
        self.actors.take_judgment_textures()
    }
    pub fn end_actor_capture(&self) {
        self.actors.end_sound_capture();
    }

    /// Deliver a host message with the existing authored-message normalization.
    /// Omitted and explicit nil parameters preserve their different arities.
    pub fn broadcast_message(&mut self, name: &str, params: Option<Value>) -> Result<(), Error> {
        self.actors.broadcast(&mut self.vm, name, params, true)
    }

    /// Engine events already carry native note objects and optional fields;
    /// deliver those values without filling authored Judgment defaults.
    pub fn broadcast_native_message(
        &mut self,
        name: &str,
        params: Option<Value>,
    ) -> Result<(), Error> {
        self.actors.broadcast(&mut self.vm, name, params, false)
    }

    /// Imported subscriber identity ranks for differential controls only.
    #[cfg(feature = "test-support")]
    pub fn set_actor_message_ranks(&self, scene: ActorScene, ranks: &[usize]) -> bool {
        self.actors.set_message_ranks(scene, ranks)
    }

    #[cfg(feature = "test-support")]
    pub fn set_actor_message_identity(&self, actor: Value, rank: usize) -> bool {
        self.actors.set_message_identity(actor, rank)
    }

    /// Load a song script with cmd preprocessing and its file environment.
    /// Store or pin the returned function before another VM entry or collection.
    pub fn load_file(&mut self, path: &std::path::Path) -> Result<Value, Error> {
        let path = self.vm.try_intern(path.to_string_lossy().as_bytes())?;
        let loader = self.vm.rooted(self.loader)?;
        Ok(self.vm.call(loader, &[path])?[0])
    }

    /// Load an actor declaration or Lua-returned template with native LoadActor
    /// semantics. Preparation loaders return constructed media/font instances
    /// or the first Lua-file result; Init remains a separate host phase.
    /// Relative entry paths are resolved against the process working directory;
    /// Lua callers resolve relative paths against their own source file.
    pub fn load_actor(
        &mut self,
        path: &std::path::Path,
        arguments: &[Value],
    ) -> Result<&[Value], Error> {
        let mut roots = Vec::with_capacity(arguments.len());
        let inputs = (|| {
            for &value in arguments {
                roots.push(self.vm.pin(value)?);
            }
            let path = self.vm.try_intern(path.to_string_lossy().as_bytes())?;
            let mut inputs = Vec::with_capacity(arguments.len() + 1);
            inputs.push(path);
            inputs.extend_from_slice(arguments);
            Ok::<_, Error>(inputs)
        })();
        for root in roots {
            self.vm.unpin(root)?;
        }
        let loader = self.vm.rooted(self.actor_loader.entry)?;
        self.vm.call(loader, &inputs?)
    }

    /// Execute a song file with arguments, preserving every returned value.
    /// This loads code into the existing session; it does not construct a scene.
    pub fn execute_file(
        &mut self,
        path: &std::path::Path,
        arguments: &[Value],
    ) -> Result<&[Value], Error> {
        let mut roots = Vec::with_capacity(arguments.len());
        let function = (|| {
            for &value in arguments {
                roots.push(self.vm.pin(value)?);
            }
            self.load_file(path)
        })();
        for root in roots {
            self.vm.unpin(root)?;
        }
        self.vm.call(function?, arguments)
    }

    /// Preserve this song's random stream across a baking/analysis probe.
    pub fn preserve_random(&self) -> RandomSnapshot {
        self.random.preserve()
    }

    pub fn runtime_table(&self) -> Result<Value, Error> {
        self.vm.rooted(self.runtime)
    }
    pub fn song_position(&self) -> Result<Value, Error> {
        self.vm.rooted(self.position)
    }

    /// Zero-based player index. Player positions exist for live contexts only.
    pub fn player_position(&self, index: usize) -> Result<Option<Value>, Error> {
        self.players
            .get(index)
            .map(|&root| self.vm.rooted(root))
            .transpose()
    }

    /// Live frames contain audio music time; preparation uses elapsed wall time.
    /// This changes clock interpretation without replacing captured Lua tables.
    /// Entering live playback discards preparation-only queue forecasts.
    pub fn set_live(&mut self, live: bool) {
        self.clock.live.store(live, Ordering::Relaxed);
        if live {
            self.actors.discard_song_replay_clock();
        }
    }

    pub fn values(&mut self) -> Result<(f32, f32), Error> {
        self.read_values(0)
    }
    pub fn delta_values(&mut self) -> Result<(f32, f32), Error> {
        self.read_values(1)
    }
    pub fn set_values(&mut self, beat: f32, seconds: f32) -> Result<(), Error> {
        self.run(2, &[number(beat), number(seconds)]).map(|_| ())
    }
    pub fn set_delta_values(&mut self, beat: f32, seconds: f32) -> Result<(), Error> {
        self.run(3, &[number(beat), number(seconds)]).map(|_| ())
    }
    pub fn set_beat(&mut self, beat: f32) -> Result<(), Error> {
        self.run(4, &[number(beat)]).map(|_| ())
    }
    pub fn reset(&mut self) -> Result<(), Error> {
        self.run(5, &[]).map(|_| ())
    }

    fn read_values(&mut self, operation: usize) -> Result<(f32, f32), Error> {
        let [Value::Number(a), Value::Number(b)] = self.run(operation, &[])? else {
            unreachable!("clock getters return two numbers")
        };
        Ok((*a as f32, *b as f32))
    }

    fn run(&mut self, operation: usize, args: &[Value]) -> Result<&[Value], Error> {
        let function = self.vm.rooted(self.operations[operation])?;
        // Match legacy helpers: look up the current runtime, while SongPosition
        // closures retain the table they were originally constructed with.
        let runtime = self.vm.get_global(SONG_LUA_RUNTIME_KEY.as_bytes())?;
        let mut arguments = vec![runtime];
        arguments.extend_from_slice(args);
        self.vm.call(function, &arguments)
    }
}

fn number(value: f32) -> Value {
    Value::Number(f64::from(value))
}

fn put(vm: &mut Vm, table: TableId, key: &str, value: Value) -> Result<(), Error> {
    let key = vm.try_intern(key.as_bytes())?;
    vm.raw_table_set(table, key, value)
}

fn position(
    vm: &mut Vm,
    clock: &Arc<Clock>,
    runtime: TableId,
    tonumber: Value,
) -> Result<Root, Error> {
    let table = vm.try_new_table()?;
    let root = vm.pin(Value::Table(table))?;
    for (name, operation) in [
        ("GetSongBeat", Operation::Float(BEAT)),
        ("GetSongBeatVisible", Operation::Float(BEAT)),
        ("GetMusicSeconds", Operation::MusicSeconds),
        ("GetMusicSecondsVisible", Operation::MusicSeconds),
        ("GetCurBPS", Operation::Float(BPS)),
        ("GetFreeze", Operation::Boolean(FREEZE)),
        ("GetDelay", Operation::Boolean(DELAY)),
    ] {
        let function = vm.new_host_function_with_data(
            callback,
            &[tonumber, Value::Table(runtime)],
            Binding {
                clock: clock.clone(),
                operation,
            },
        )?;
        put(vm, table, name, function)?;
    }
    Ok(root)
}

fn field(call: &mut HostCall<'_>, table: Value, key: &str) -> Result<Value, Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.table_get(table, key)
}

fn set(call: &mut HostCall<'_>, table: Value, key: &str, value: Value) -> Result<(), Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.table_set(table, key, value)
}

fn numeric(call: &mut HostCall<'_>, value: Value) -> Result<f64, Error> {
    if let Value::Number(number) = value {
        return Ok(number);
    }
    let tonumber = call.captures()[0];
    match call.call(tonumber, &[value])?.first().copied() {
        Some(Value::Number(number)) => Ok(number),
        _ => Err(call.error(b"expected number or string coercible to number")),
    }
}

fn float(call: &mut HostCall<'_>, table: Value, key: &str) -> Result<f32, Error> {
    let value = field(call, table, key)?;
    Ok(numeric(call, value)? as f32)
}

fn optional_float(
    call: &mut HostCall<'_>,
    table: Value,
    key: &str,
    default: f32,
) -> Result<f32, Error> {
    let value = field(call, table, key)?;
    if value == Value::Nil {
        Ok(default)
    } else {
        Ok(numeric(call, value)? as f32)
    }
}

fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Binding { clock, operation } = call
        .function_data::<Binding>()
        .expect("clock binding")
        .clone();
    let table = call
        .captures()
        .get(1)
        .copied()
        .unwrap_or_else(|| call.arguments()[0]);
    if !matches!(table, Value::Table(_)) {
        return Err(call.error(b"expected song runtime table"));
    }
    match operation {
        Operation::Float(key) => {
            let value = float(call, table, key)?;
            call.push(number(value))
        }
        Operation::Boolean(key) => {
            let value = field(call, table, key)?;
            call.push(Value::Boolean(value.is_truthy()))
        }
        Operation::MusicSeconds => {
            let elapsed = field(call, table, SECONDS)?;
            let elapsed = numeric(call, elapsed)?;
            let rate = float(call, table, RATE)?;
            let seconds = if clock.live.load(Ordering::Relaxed) {
                elapsed as f32
            } else {
                (elapsed * f64::from(rate)) as f32
                    + clock
                        .timing
                        .as_ref()
                        .map_or(0.0, |clock| clock.get_time_for_beat_exact(0.0))
            };
            // Match the legacy whole-float integer path, including -0 -> +0.
            let seconds = if seconds.fract() == 0.0 && seconds.abs() < 1e14 {
                (seconds as i64) as f64
            } else {
                f64::from(seconds)
            };
            call.push(Value::Number(seconds))
        }
        Operation::Values(delta) => {
            let (a, b) = if delta {
                (DELTA_BEAT, DELTA_SECONDS)
            } else {
                (BEAT, SECONDS)
            };
            let a = float(call, table, a)?;
            let b = float(call, table, b)?;
            call.push(number(a))?;
            call.push(number(b))
        }
        Operation::SetValues(delta) => {
            let (a, b) = if delta {
                (DELTA_BEAT, DELTA_SECONDS)
            } else {
                (BEAT, SECONDS)
            };
            let first = call.arguments()[1];
            let second = call.arguments()[2];
            set(call, table, a, first)?;
            set(call, table, b, second)
        }
        Operation::SetBeat => {
            let beat = call.number(1)? as f32;
            let mut bps = optional_float(call, table, BPS, 1.0)?;
            let rate = optional_float(call, table, RATE, 1.0)?;
            if let Some(clock) = &clock.timing {
                let seconds = clock.get_time_for_beat_exact(beat);
                let position = clock.get_song_position(seconds);
                set(call, table, BEAT, number(beat))?;
                set(
                    call,
                    table,
                    SECONDS,
                    number((seconds - clock.get_time_for_beat_exact(0.0)) / rate.max(f32::EPSILON)),
                )?;
                set(call, table, FREEZE, Value::Boolean(position.is_in_freeze))?;
                set(call, table, DELAY, Value::Boolean(position.is_in_delay))?;
                return set(call, table, BPS, number(position.bpm / 60.0));
            }
            let mut segment_beat = 0.0;
            let mut segment_seconds = 0.0;
            let bpms = field(call, table, BPMS)?;
            if bpms != Value::Nil {
                // The nested argument window roots even a table synthesized by
                // __index while numeric conversion can reenter the collector.
                let tonumber = call.captures()[0];
                let reader = call.new_host_function(read_segments, &[tonumber])?;
                let values = call.call(reader, &[bpms, number(beat), number(bps)])?;
                let [Value::Number(a), Value::Number(b), Value::Number(c)] = values else {
                    unreachable!("segment tuple")
                };
                segment_beat = *a as f32;
                bps = *b as f32;
                segment_seconds = *c as f32;
            }
            set(call, table, BEAT, number(beat))?;
            set(
                call,
                table,
                SECONDS,
                number(
                    (segment_seconds + (beat - segment_beat) / bps.max(f32::EPSILON))
                        / rate.max(f32::EPSILON),
                ),
            )?;
            set(call, table, BPS, number(bps))
        }
        Operation::Reset => {
            let reset = call.new_host_function(reset_position, &[])?;
            call.call(reset, &[table])?;
            set(call, table, DELTA_BEAT, number(0.0))?;
            set(call, table, DELTA_SECONDS, number(0.0))?;
            let key = call.try_intern(PLAYERS.as_bytes())?;
            let Value::Table(table) = table else {
                return Err(call.error(b"expected song runtime table"));
            };
            let players = call.raw_table_get(table, key)?;
            if players != Value::Nil {
                let reset = call.new_host_function(reset_players, &[])?;
                call.call(reset, &[players])?;
            }
            Ok(())
        }
    }
}

fn read_segments(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Value::Table(bpms) = call.arguments()[0] else {
        return Err(call.error(b"expected timing BPM table"));
    };
    let beat = call.number(1)? as f32;
    let mut bps = call.number(2)? as f32;
    let mut segment_beat = 0.0;
    let mut seconds = 0.0;
    for index in 1.. {
        let segment = call.raw_table_get(bpms, Value::Number(index as f64))?;
        if segment == Value::Nil {
            break;
        }
        let Value::Table(segment) = segment else {
            return Err(call.error(b"expected BPM segment table"));
        };
        let value = call.raw_table_get(segment, Value::Number(1.0))?;
        let next = numeric(call, value)? as f32;
        if next > beat {
            break;
        }
        segment_beat = next;
        let value = call.raw_table_get(segment, Value::Number(2.0))?;
        bps = numeric(call, value)? as f32 / 60.0;
        let value = call.raw_table_get(segment, Value::Number(3.0))?;
        seconds = numeric(call, value)? as f32;
    }
    for value in [segment_beat, bps, seconds] {
        call.push(number(value))?;
    }
    Ok(())
}

fn reset_players(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Value::Table(players) = call.arguments()[0] else {
        return Err(call.error(b"expected player positions table"));
    };
    for index in 1.. {
        let player = call.raw_table_get(players, Value::Number(index as f64))?;
        if player == Value::Nil {
            break;
        }
        // Each player's table stays alive if its setters detach it from the
        // runtime and collect before the rest of its fields have been reset.
        let reset = call.new_host_function(reset_position, &[])?;
        call.call(reset, &[player])?;
    }
    Ok(())
}

fn reset_position(call: &mut HostCall<'_>) -> Result<(), Error> {
    let table = call.arguments()[0];
    if !matches!(table, Value::Table(_)) {
        return Err(call.error(b"expected song position table"));
    }
    for (key, value) in [
        (BEAT, number(0.0)),
        (SECONDS, number(0.0)),
        (BPS, number(10.0)),
        (FREEZE, Value::Boolean(false)),
        (DELAY, Value::Boolean(false)),
    ] {
        set(call, table, key, value)?;
    }
    Ok(())
}

mod runtime_fallback;
mod runtime_mod;
mod sl;

mod tables;
mod theme_colors;
mod timing;
mod top_screen;
mod values;
mod version;

pub use cmd::preprocess_lua_cmd_syntax;

pub use compile_timing::{
    SongLuaCompileTimer, log_song_lua_compile_timing, song_lua_compile_stage_summary,
};

pub use deadlib_assets::parse_sprite_sheet_dims;
pub use eases::SongLuaFunctionEaseDecision;

pub use eases::SongLuaFunctionEaseResult;
pub use eases::SongLuaReadEasesResult;
pub use eases::SongLuaReadEasesStats;

pub use files::actor_util_class_registered;
pub use files::actor_util_file_type;

pub use files::entry_file_path;
pub use files::file_path_string;
pub use files::fileman_dir_listing;
pub use files::find_compat_files;
pub use files::is_song_lua_audio_path;
pub use files::is_song_lua_image_path;
pub use files::is_song_lua_media_path;
pub use files::is_song_lua_simfile_path;
pub use files::is_song_lua_video_path;
pub use files::path_basename;

pub use files::resolve_compat_path;

pub use files::song_dir_string;
pub use files::song_group_name;
pub use files::song_lookup_matches;
pub use files::song_music_path;
pub use files::song_named_image_path;
pub use files::song_simfile_path;
pub use files::strip_sprite_hints;
pub use files::theme_path;
pub use files::wildcard_matches;
pub use host::SONG_LUA_STARTUP_MESSAGE;

pub use host::SongLuaDateGlobals;

pub use host::is_compile_global_name;

pub use host::song_lua_local_date_globals;

pub use lua_util::SongLuaCapturedMessageCommands;
pub use lua_util::SongLuaFunctionActionCapture;

pub use lua_util::SongLuaNoteskinTapActorModel;

pub use lua_util::actor_indices_for_pointers;

pub use lua_util::actor_pointers_touch_actor;

pub use lua_util::banner_sort_order_path;

pub use lua_util::overlay_model_layers_from_slots;

pub use lua_util::push_note_hide_window;

pub use lua_util::read_graph_display_size;

pub use multitap::MULTITAP_PREVISIBLE_BEATS;
pub use multitap::MultitapDesc;
pub use multitap::MultitapPhase;
pub use multitap::calc_multitap_phase;

pub use multitap::multitap_deco_child_state;
pub use multitap::multitap_deco_state;
pub use multitap::multitap_explosion_state;
pub use multitap::multitap_frame_state;
pub use multitap::overlay_delta_pair_from_states;
pub use multitap::push_multitap_actor_eases;
pub use multitap::push_multitap_arrow_sample;
pub use multitap::push_multitap_explosion_eases;
pub use multitap::push_overlay_sample_eases;

pub use net::url_encode_component;

pub use option_rows::SongLuaNamedOptionRowSpec;
pub use option_rows::SongLuaOperatorOptionRowSpec;
pub use option_rows::SongLuaOptionRowSpec;
pub use option_rows::SongLuaOptionValues;
pub use option_rows::THEME_PREF_ROW_NAMES;
pub use option_rows::conf_option_row_spec;

pub use option_rows::custom_option_default_text;
pub use option_rows::custom_option_row_spec;
pub use option_rows::operator_menu_option_row_spec;
pub use option_rows::option_value_text;
pub use option_rows::theme_pref_row_spec;
pub use perframe::SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES;

pub use perframe::SongLuaPerframePlayerState;
pub use perframe::SongLuaPerframeSample;
pub use perframe::SongLuaUpdateModState;

pub use perframe::perframe_delta_seconds;
pub use perframe::perframe_samples;
pub use perframe::perframe_segment_step;

pub use perframe::push_perframe_overlay_targets;
pub use perframe::push_perframe_player_target;
pub use perframe::push_perframe_player_targets;
pub use perframe::push_perframe_static_targets;
pub use perframe::push_sampled_perframe_targets;
pub use perframe::push_update_mod_targets;

pub use perframe::relative_player_target;

pub use perframe::update_function_end_beat;
pub use perframe::update_function_sample_step;
pub use perframe::update_function_samples;

pub use player_options::SONG_LUA_PLAYER_OPTION_CAPABILITIES;
pub use player_options::SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES;

pub use player_options::is_player_option_method_name;
pub use player_options::normalize_player_option_key;

pub use player_options::parse_player_option_amount;
pub use player_options::parse_player_speed_option;
pub use player_options::player_option_default_string;
pub use player_options::player_option_uses_bool;

pub use player_options::strip_player_option_prefix;

pub use runtime_mod::RuntimeModEaseEntry;
pub use runtime_mod::RuntimeOverlayCaptureKey;

pub use runtime_mod::extend_runtime_mod_sustains;

pub use runtime_mod::record_unsupported_xero_overlay_function_ease;
pub use runtime_mod::runtime_mod_end_value;
pub use runtime_mod::runtime_mod_entry_players;
pub use runtime_mod::runtime_mod_key;
pub use runtime_mod::runtime_mod_start_value;

pub use runtime_mod::runtime_player_option_ease_target;

pub use tables::format_number_and_suffix;

pub use tables::scale_value;
pub use tables::seconds_to_hhmmss;
pub use tables::seconds_to_mmss;
pub use tables::seconds_to_mmss_ms_ms;
pub use tables::seconds_to_mss;
pub use tables::seconds_to_mss_ms_ms;

pub use theme_colors::DDR_DIFF_COLORS;
pub use theme_colors::ITG_DIFF_COLORS;
pub use theme_colors::SL_COLORS;
pub use theme_colors::SL_DECORATIVE_COLORS;
pub use theme_colors::SL_FA_PLUS_COLORS;
pub use theme_colors::SL_JUDGMENT_COLORS;
pub use theme_colors::SONG_LUA_ACTIVE_COLOR_INDEX;
pub use theme_colors::blend_color;
pub use theme_colors::color_to_hex;
pub use theme_colors::custom_difficulty_color;

pub use theme_colors::judgment_line_color;
pub use theme_colors::light_color;
pub use theme_colors::palette_color;
pub use theme_colors::parse_color_text;
pub use theme_colors::song_lua_difficulty_color;
pub use theme_colors::song_lua_difficulty_index;
pub use theme_colors::song_lua_palette;
pub use theme_colors::song_lua_player_color;
pub use theme_colors::song_lua_player_dark_color;
pub use theme_colors::song_lua_player_score_color;
pub use theme_colors::stage_color;
pub use theme_colors::tone_color;
pub use timing::SONG_LUA_TIMING_WINDOW_NAMES;

pub use timing::timing_window_seconds;

pub use top_screen::SONG_LUA_TOP_SCREEN_OPTION_ROWS;
pub use top_screen::TOP_SCREEN_THEME_CHILD_NAMES;
pub use top_screen::UNDERLAY_THEME_CHILD_NAMES;
pub use top_screen::option_row_default_text;
pub use top_screen::player_child_proxy_name;
pub use top_screen::top_screen_danger_index;
pub use top_screen::top_screen_life_meter_index;
pub use top_screen::top_screen_life_meter_name;

pub use top_screen::top_screen_option_row_name_at;
pub use top_screen::top_screen_player_index;
pub use top_screen::top_screen_player_name;
pub use top_screen::top_screen_score_index;
pub use top_screen::top_screen_score_name;
pub use top_screen::top_screen_score_percent_name;
pub use top_screen::top_screen_step_stats_pane_index;
pub use top_screen::top_screen_steps_display_index;
pub use top_screen::underlay_score_index;
pub use top_screen::underlay_score_name;
pub use values::SONG_LUA_EASING_NAME_KEY;

pub use values::player_number_name;

pub use version::version_parts;

pub const LUA_PLAYERS: usize = 2;
pub const SONG_LUA_DEFAULT_NOTESKIN_NAME: &str = "cel";
pub const SONG_LUA_PRODUCT_FAMILY: &str = "ITGmania";
pub const SONG_LUA_PRODUCT_ID: &str = "ITGmania";
pub const SONG_LUA_PRODUCT_VERSION: &str = "1.2.0";
pub const SONG_LUA_RUNTIME_KEY: &str = "__songlua_compile_song_runtime";
pub const SONG_LUA_RUNTIME_BEAT_KEY: &str = "__songlua_song_beat";
pub const SONG_LUA_RUNTIME_SECONDS_KEY: &str = "__songlua_music_seconds";
pub const SONG_LUA_RUNTIME_DELTA_BEAT_KEY: &str = "__songlua_song_delta_beat";
pub const SONG_LUA_RUNTIME_DELTA_SECONDS_KEY: &str = "__songlua_music_delta_seconds";
pub const SONG_LUA_RUNTIME_BPS_KEY: &str = "__songlua_song_bps";
pub const SONG_LUA_RUNTIME_RATE_KEY: &str = "__songlua_music_rate";
pub const SONG_LUA_SIDE_EFFECT_COUNT_KEY: &str = "__songlua_side_effect_count";
pub const SONG_LUA_BROADCASTS_KEY: &str = "__songlua_broadcast_messages";
pub const SONG_LUA_SUPPRESS_BROADCAST_KEY: &str = "__songlua_suppress_broadcast_dispatch";
pub const SONG_LUA_SOUND_PATHS_KEY: &str = "__songlua_sound_paths";
pub const SONG_LUA_SOUND_CALLS_KEY: &str = "__songlua_sound_calls";
pub const SONG_LUA_PLAYER_OPTIONS_KEYS: [&str; LUA_PLAYERS] =
    ["__songlua_player_options_1", "__songlua_player_options_2"];
pub const SONG_LUA_PROBE_METHODS_KEY: &str = "__songlua_probe_methods";
pub const SONG_LUA_PROBE_ACTORS_KEY: &str = "__songlua_probe_actors";
pub const SONG_LUA_PROBE_ACTOR_SET_KEY: &str = "__songlua_probe_actor_set";
pub const SONG_LUA_CAPTURE_ACTORS_KEY: &str = "__songlua_capture_scope_actors";
pub const SONG_LUA_CAPTURE_ACTOR_SET_KEY: &str = "__songlua_capture_scope_actor_set";
pub const SONG_LUA_CAPTURE_SNAPSHOTS_KEY: &str = "__songlua_capture_scope_snapshots";
pub const SONG_LUA_THEME_PATH_PREFIX: &str = "__songlua_theme_path/";
pub const SONG_LUA_THEME_NAME: &str = "Simply Love";
pub const THEME_RECEPTOR_Y_STD: f32 = -125.0;
pub const THEME_RECEPTOR_Y_REV: f32 = 145.0;
pub const SONG_LUA_INITIAL_LIFE: f32 = 0.5;
pub const SONG_LUA_DANGER_LIFE: f32 = 0.2;
pub const SONG_LUA_NOTE_COLUMNS: usize = 4;
pub const SONG_LUA_DOUBLE_NOTE_COLUMNS: usize = 8;
pub const GRAPH_DISPLAY_VALUE_RESOLUTION: usize = 100;
pub const SONG_LUA_SPRITE_STATE_CLEAR: u32 = u32::MAX;
pub const SONG_LUA_EASING_NAMES: &[&str] = &[
    "instant",
    "linear",
    "inQuad",
    "outQuad",
    "inOutQuad",
    "outInQuad",
    "inCubic",
    "outCubic",
    "inOutCubic",
    "outInCubic",
    "inQuart",
    "outQuart",
    "inOutQuart",
    "outInQuart",
    "inQuint",
    "outQuint",
    "inOutQuint",
    "outInQuint",
    "inSine",
    "outSine",
    "inOutSine",
    "outInSine",
    "inExpo",
    "outExpo",
    "inOutExpo",
    "outInExpo",
    "inCirc",
    "outCirc",
    "inOutCirc",
    "outInCirc",
    "inElastic",
    "outElastic",
    "inOutElastic",
    "outInElastic",
    "inBack",
    "outBack",
    "inOutBack",
    "outInBack",
    "inBounce",
    "outBounce",
    "inOutBounce",
    "outInBounce",
];

const SONG_LUA_COLUMN_X: [f32; SONG_LUA_NOTE_COLUMNS] = [-96.0, -32.0, 32.0, 96.0];
const SONG_LUA_DOUBLE_COLUMN_X: [f32; SONG_LUA_DOUBLE_NOTE_COLUMNS] =
    [-224.0, -160.0, -96.0, -32.0, 32.0, 96.0, 160.0, 224.0];
const SONG_LUA_PUMP_COLUMN_X: [f32; 5] = [-96.0, -48.0, 0.0, 48.0, 96.0];
const SONG_LUA_PUMP_DOUBLE_X: [f32; 10] = [
    -220.0, -172.0, -124.0, -76.0, -28.0, 28.0, 76.0, 124.0, 172.0, 220.0,
];
const SONG_LUA_COLUMN_NAMES: [&str; SONG_LUA_NOTE_COLUMNS] = ["Left", "Down", "Up", "Right"];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaStyleInfo {
    pub name: &'static str,
    pub steps_type: &'static str,
    pub style_type: &'static str,
    pub columns: usize,
    pub width: f32,
    pub x_offsets: &'static [f32],
}

const SONG_LUA_CLASSIFIER_KEY_STACK_BYTES: usize = 64;

#[inline]
fn with_compact_ascii_lowercase_song_key<T>(raw: &str, use_key: impl FnOnce(&str) -> T) -> T {
    let raw = raw.trim();
    if raw.len() <= SONG_LUA_CLASSIFIER_KEY_STACK_BYTES {
        let mut key = [0u8; SONG_LUA_CLASSIFIER_KEY_STACK_BYTES];
        let mut len = 0;
        for byte in raw.bytes() {
            if matches!(byte, b'_' | b'-' | b' ') {
                continue;
            }
            key[len] = byte.to_ascii_lowercase();
            len += 1;
        }
        let key = std::str::from_utf8(&key[..len])
            .expect("removing ASCII separators preserves valid UTF-8");
        use_key(key)
    } else {
        let mut key = String::with_capacity(raw.len());
        key.extend(
            raw.chars()
                .filter(|ch| !matches!(ch, '_' | '-' | ' '))
                .map(|ch| ch.to_ascii_lowercase()),
        );
        use_key(&key)
    }
}

#[must_use]
pub fn song_lua_style_info(style_name: &str) -> SongLuaStyleInfo {
    with_compact_ascii_lowercase_song_key(style_name, song_lua_style_info_normalized)
}

fn song_lua_style_info_normalized(normalized: &str) -> SongLuaStyleInfo {
    // GameManager.cpp dance/pump style metadata; Style::GetWidth uses the
    // span plus average column spacing, including the pump center gap.
    let (name, steps_type, style_type, x_offsets): (_, _, _, &'static [f32]) = match normalized {
        "double" | "dancedouble" | "stepstypedancedouble" => (
            "double",
            "StepsType_Dance_Double",
            "StyleType_OnePlayerTwoSides",
            &SONG_LUA_DOUBLE_COLUMN_X,
        ),
        "versus" => (
            "versus",
            "StepsType_Dance_Single",
            "StyleType_TwoPlayersTwoSides",
            &SONG_LUA_COLUMN_X,
        ),
        "couple" | "dancecouple" | "stepstypedancecouple" => (
            "couple",
            "StepsType_Dance_Couple",
            "StyleType_TwoPlayersTwoSides",
            &SONG_LUA_COLUMN_X,
        ),
        "routine" | "danceroutine" | "stepstypedanceroutine" => (
            "routine",
            "StepsType_Dance_Routine",
            "StyleType_TwoPlayersSharedSides",
            &SONG_LUA_DOUBLE_COLUMN_X,
        ),
        "pumpsingle" | "stepstypepumpsingle" => (
            "single",
            "StepsType_Pump_Single",
            "StyleType_OnePlayerOneSide",
            &SONG_LUA_PUMP_COLUMN_X,
        ),
        "pumpversus" => (
            "versus",
            "StepsType_Pump_Single",
            "StyleType_TwoPlayersTwoSides",
            &SONG_LUA_PUMP_COLUMN_X,
        ),
        "pumpdouble" | "stepstypepumpdouble" => (
            "double",
            "StepsType_Pump_Double",
            "StyleType_OnePlayerTwoSides",
            &SONG_LUA_PUMP_DOUBLE_X,
        ),
        "pumpcouple" | "stepstypepumpcouple" => (
            "couple",
            "StepsType_Pump_Couple",
            "StyleType_TwoPlayersTwoSides",
            &SONG_LUA_PUMP_COLUMN_X,
        ),
        "pumproutine" | "stepstypepumproutine" => (
            "routine",
            "StepsType_Pump_Routine",
            "StyleType_TwoPlayersSharedSides",
            &SONG_LUA_PUMP_DOUBLE_X,
        ),
        _ => (
            "single",
            "StepsType_Dance_Single",
            "StyleType_OnePlayerOneSide",
            &SONG_LUA_COLUMN_X,
        ),
    };
    let span = x_offsets[x_offsets.len() - 1] - x_offsets[0];
    SongLuaStyleInfo {
        name,
        steps_type,
        style_type,
        columns: x_offsets.len(),
        width: span + span / (x_offsets.len() - 1) as f32,
        x_offsets,
    }
}

#[inline(always)]
#[must_use]
pub fn song_lua_style_column_x(style_name: &str, column_index: usize) -> f32 {
    song_lua_style_info(style_name)
        .x_offsets
        .get(column_index)
        .copied()
        .unwrap_or(0.0)
}

#[inline(always)]
#[must_use]
pub const fn song_lua_style_column_name(column_index: usize) -> &'static str {
    SONG_LUA_COLUMN_NAMES[column_index % SONG_LUA_COLUMN_NAMES.len()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaDifficulty {
    Beginner,
    Easy,
    Medium,
    Hard,
    Challenge,
    Edit,
}

impl SongLuaDifficulty {
    #[inline(always)]
    #[must_use]
    pub const fn sm_name(self) -> &'static str {
        match self {
            Self::Beginner => "Difficulty_Beginner",
            Self::Easy => "Difficulty_Easy",
            Self::Medium => "Difficulty_Medium",
            Self::Hard => "Difficulty_Hard",
            Self::Challenge => "Difficulty_Challenge",
            Self::Edit => "Difficulty_Edit",
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn meter(self) -> i32 {
        match self {
            Self::Beginner => 1,
            Self::Easy => 4,
            Self::Medium => 7,
            Self::Hard => 10,
            Self::Challenge => 12,
            Self::Edit => 0,
        }
    }

    #[inline(always)]
    #[must_use]
    pub const fn default_enabled() -> Self {
        Self::Challenge
    }

    #[inline(always)]
    #[must_use]
    pub const fn sort_key(self) -> u8 {
        match self {
            Self::Beginner => 0,
            Self::Easy => 1,
            Self::Medium => 2,
            Self::Hard => 3,
            Self::Challenge => 4,
            Self::Edit => 5,
        }
    }
}

fn song_lua_difficulty_from_normalized(normalized: &str) -> Option<SongLuaDifficulty> {
    let raw = normalized.strip_prefix("difficulty").unwrap_or(normalized);
    match raw {
        "beginner" => Some(SongLuaDifficulty::Beginner),
        "easy" => Some(SongLuaDifficulty::Easy),
        "medium" => Some(SongLuaDifficulty::Medium),
        "hard" => Some(SongLuaDifficulty::Hard),
        "challenge" | "expert" => Some(SongLuaDifficulty::Challenge),
        "edit" => Some(SongLuaDifficulty::Edit),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SongLuaSpeedMod {
    X(f32),
    C(f32),
    M(f32),
    A(f32),
}

impl Default for SongLuaSpeedMod {
    fn default() -> Self {
        Self::X(1.0)
    }
}

#[must_use]
pub const fn song_lua_speedmod_parts(speedmod: SongLuaSpeedMod) -> (&'static str, f32) {
    match speedmod {
        SongLuaSpeedMod::X(value) => ("X", value),
        SongLuaSpeedMod::C(value) => ("C", value),
        SongLuaSpeedMod::M(value) => ("M", value),
        SongLuaSpeedMod::A(value) => ("A", value),
    }
}

#[must_use]
pub fn song_music_rate_value(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}

#[must_use]
pub fn format_song_options_text(music_rate: f32) -> String {
    let rate = song_music_rate_value(music_rate);
    if rate == 1.0 {
        return String::new();
    }
    // SongOptions::GetMods rounds to two decimals and removes one trailing zero.
    let mut text = format!("{rate:.2}");
    if text.ends_with('0') {
        text.pop();
    }
    text.push_str("xMusic");
    text
}

#[must_use]
pub fn display_bpms_text(bpms: [f32; 2], rate: f32) -> String {
    let lower = format_display_bpm(bpms[0], rate);
    if (bpms[0] - bpms[1]).abs() <= f32::EPSILON {
        lower
    } else {
        format!("{lower} - {}", format_display_bpm(bpms[1], rate))
    }
}

fn format_display_bpm(value: f32, rate: f32) -> String {
    let text = if (rate - 1.0).abs() <= f32::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    text.strip_suffix(".0").unwrap_or(&text).to_string()
}

#[must_use]
pub fn player_short_name(player: usize) -> &'static str {
    match player {
        0 => "P1",
        1 => "P2",
        _ => unreachable!("song lua only exposes two player numbers"),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaPlayerContext {
    pub enabled: bool,
    /// Preferred controller option already set before song Lua loads.
    pub autoplay: f32,
    pub difficulty: SongLuaDifficulty,
    pub speedmod: SongLuaSpeedMod,
    /// Saved Mini amount already applied before song Lua loads (60% is 0.6).
    pub mini: f32,
    /// Saved scroll options already applied to every ModsLevel before song Lua loads.
    pub scroll: deadsync_gameplay::ScrollEffects,
    pub perspective: deadsync_gameplay::PerspectiveEffects,
    pub display_bpms: [f32; 2],
    pub noteskin_name: String,
    /// Profile-selected graphic already loaded by the gameplay theme.
    pub judgment_texture: Option<PathBuf>,
    /// Profile option used by Simply Love's Lua-owned cue actors.
    pub column_cues: bool,
    /// Initial profile filter opacity. Zero disables the theme's filter Quad.
    pub background_filter_percent: u8,
    /// Initial profile selection for Simply Love's Lua-owned life meter.
    pub hide_lifebar: bool,
    /// Hide native danger flashes while retaining the fail flash.
    pub hide_danger: bool,
    pub life_meter_type: String,
    /// Initial profile layout choice used by the shared BPM owner.
    pub nps_graph_at_top: bool,
    /// Actual profile selection for the native StepStatistics pane.
    pub step_statistics_enabled: bool,
    /// Enabled W1 through W5 windows from the initial player profile.
    pub timing_windows: [bool; 5],
    pub screen_x: f32,
    pub screen_y: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct SongLuaActorMultiVertexPoint {
    pub pos: [f32; 2],
    pub z: f32,
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

impl Default for SongLuaPlayerContext {
    fn default() -> Self {
        Self {
            enabled: true,
            autoplay: 0.0,
            difficulty: SongLuaDifficulty::default_enabled(),
            speedmod: SongLuaSpeedMod::default(),
            mini: 0.0,
            scroll: deadsync_gameplay::ScrollEffects::default(),
            perspective: deadsync_gameplay::PerspectiveEffects::default(),
            display_bpms: [60.0, 60.0],
            noteskin_name: SONG_LUA_DEFAULT_NOTESKIN_NAME.to_string(),
            judgment_texture: None,
            column_cues: false,
            background_filter_percent: 0,
            hide_lifebar: false,
            hide_danger: false,
            life_meter_type: "Standard".into(),
            nps_graph_at_top: false,
            step_statistics_enabled: true,
            timing_windows: [true; 5],
            screen_x: 320.0,
            screen_y: 240.0,
        }
    }
}

#[must_use]
pub fn easiest_steps_difficulty(
    players: &[SongLuaPlayerContext; LUA_PLAYERS],
) -> Option<SongLuaDifficulty> {
    players
        .iter()
        .filter(|player| player.enabled)
        .map(|player| player.difficulty)
        .min_by_key(|difficulty| difficulty.sort_key())
}

#[must_use]
pub fn song_lua_human_player_count(context: &SongLuaCompileContext) -> usize {
    context
        .players
        .iter()
        .filter(|player| player.enabled)
        .count()
}

#[must_use]
pub const fn graph_display_body_size(human_player_count: usize) -> [f32; 2] {
    [
        if human_player_count == 1 {
            610.0
        } else {
            300.0
        },
        64.0,
    ]
}

#[must_use]
pub fn theme_metric_number(group: &str, name: &str) -> Option<f32> {
    theme_metric_number_for_human_players(group, name, LUA_PLAYERS)
}

#[must_use]
pub fn theme_metric_number_for_human_players(
    group: &str,
    name: &str,
    human_player_count: usize,
) -> Option<f32> {
    theme_metric_number_for_screen(group, name, human_player_count, 640.0, 480.0)
}

const THEME_PLAYER_X_STYLES: [&str; 5] = [
    "OnePlayerOneSide",
    "TwoPlayersTwoSides",
    "OnePlayerBothSides",
    "OnePlayerTwoSides",
    "TwoPlayersSharedSides",
];

#[must_use]
pub fn theme_metric_number_for_screen(
    group: &str,
    name: &str,
    human_player_count: usize,
    screen_width: f32,
    screen_height: f32,
) -> Option<f32> {
    if group.eq_ignore_ascii_case("Common") && name.eq_ignore_ascii_case("ScreenHeight") {
        return Some(screen_height);
    }
    if group.eq_ignore_ascii_case("ScreenGameplay") {
        let side = if name.get(..8)?.eq_ignore_ascii_case("PlayerP1") {
            -1.0
        } else if name.get(..8)?.eq_ignore_ascii_case("PlayerP2") {
            1.0
        } else {
            return None;
        };
        let suffix = name.get(8..)?;
        let style = suffix
            .strip_suffix('X')
            .or_else(|| suffix.strip_suffix('x'))?;
        let index = THEME_PLAYER_X_STYLES
            .iter()
            .position(|known| style.eq_ignore_ascii_case(known))?;
        // Simply-Love-SM5/metrics.ini: side fields use a clamped quarter-width.
        return Some(
            screen_width * 0.5
                + if index < 2 {
                    side * screen_width.clamp(640.0, 854.0) * 0.25
                } else {
                    0.0
                },
        );
    }
    if group.eq_ignore_ascii_case("Player") {
        if name.eq_ignore_ascii_case("ReceptorArrowsYStandard") {
            return Some(THEME_RECEPTOR_Y_STD);
        }
        if name.eq_ignore_ascii_case("ReceptorArrowsYReverse") {
            return Some(THEME_RECEPTOR_Y_REV);
        }
        if name.eq_ignore_ascii_case("DrawDistanceBeforeTargetsPixels") {
            return Some(screen_height.max(1.0) * 1.5);
        }
        if name.eq_ignore_ascii_case("DrawDistanceAfterTargetsPixels") {
            return Some(-130.0);
        }
    }
    if group.eq_ignore_ascii_case("Combo") && name.eq_ignore_ascii_case("ShowComboAt") {
        return Some(4.0);
    }
    if group.eq_ignore_ascii_case("GraphDisplay") {
        if name.eq_ignore_ascii_case("BodyWidth") {
            return Some(graph_display_body_size(human_player_count)[0]);
        }
        if name.eq_ignore_ascii_case("BodyHeight") {
            return Some(graph_display_body_size(human_player_count)[1]);
        }
    }
    if group.eq_ignore_ascii_case("LifeMeterBar") && name.eq_ignore_ascii_case("InitialValue") {
        return Some(SONG_LUA_INITIAL_LIFE);
    }
    if group.eq_ignore_ascii_case("MusicWheel") && name.eq_ignore_ascii_case("NumWheelItems") {
        return Some(15.0);
    }
    if group.eq_ignore_ascii_case("PlayerStageStats")
        && name.eq_ignore_ascii_case("NumGradeTiersUsed")
    {
        return Some(7.0);
    }
    None
}

const SONG_LUA_SCREEN_PLAYER_OPTIONS_LINE_NAMES: &str = "SpeedModType,SpeedMod,Mini,Perspective,NoteSkinSL,NoteSkinVariant,Judgment,ComboFont,HoldJudgment,BackgroundFilter,NoteFieldOffsetX,NoteFieldOffsetY,VisualDelay,MusicRate,Stepchart,ScreenAfterPlayerOptions";
const SONG_LUA_SCREEN_PLAYER_OPTIONS2_LINE_NAMES: &str = "Turn,Scroll,Hide,LifeMeterType,DataVisualizations,TargetScore,ActionOnMissedTarget,GameplayExtras,GameplayExtrasB,GameplayExtrasC,TiltMultiplier,ErrorBar,ErrorBarTrim,ErrorBarOptions,MeasureCounter,MeasureCounterOptions,MeasureLines,TimingWindowOptions,FaPlus,ScreenAfterPlayerOptions2";
const SONG_LUA_SCREEN_PLAYER_OPTIONS3_LINE_NAMES: &str =
    "Insert,Remove,Holds,11,12,13,Attacks,Characters,HideLightType,ScreenAfterPlayerOptions3";
const SONG_LUA_SCREEN_ATTACK_MENU_LINE_NAMES: &str =
    "SpeedModType,SpeedMod,Mini,Perspective,NoteSkin,MusicRate,Assist,ShowBGChangesPlay";
const SONG_LUA_SCREEN_OPTIONS_SERVICE_LINE_NAMES: &str = "SystemOptions,MapControllers,TestInput,InputOptions,GraphicsSoundOptions,VisualOptions,ArcadeOptions,Bookkeeping,AdvancedOptions,MenuTimerOptions,USBProfileOptions,OptionsManageProfiles,ThemeOptions,TournamentModeOptions,GrooveStatsOptions,StepManiaCredits,Reload";
const SONG_LUA_SCREEN_SYSTEM_OPTIONS_LINE_NAMES: &str =
    "Game,Theme,Language,Announcer,DefaultNoteSkin,EditorNoteSkin";
const SONG_LUA_SCREEN_INPUT_OPTIONS_LINE_NAMES: &str =
    "AutoMap,OnlyDedicatedMenu,OptionsNav,Debounce,ThreeKey,AxisFix";
const SONG_LUA_SCREEN_GRAPHICS_SOUND_OPTIONS_LINE_NAMES: &str = "VideoRenderer,DisplayMode,DisplayAspectRatio,DisplayResolution,RefreshRate,FullscreenType,DisplayColorDepth,HighResolutionTextures,MaxTextureResolution";
const SONG_LUA_SCREEN_VISUAL_OPTIONS_LINE_NAMES: &str =
    "AppearanceOptions,Set BG Fit Mode,Overscan Correction,CRT Test Patterns";
const SONG_LUA_SCREEN_APPEARANCE_OPTIONS_LINE_NAMES: &str = "Center1Player,ShowBanners,BGBrightness,RandomBackgroundMode,NumBackgrounds,ShowLyrics,ShowNativeLanguage,ShowDancingCharacters";
const SONG_LUA_SCREEN_ARCADE_OPTIONS_LINE_NAMES: &str = "Event,Coin,CoinsPerCredit,MaxNumCredits,ResetCoinsAtStartup,Premium,SongsPerPlay,Long Time,Marathon Time";
const SONG_LUA_SCREEN_ADVANCED_OPTIONS_LINE_NAMES: &str =
    "DefaultFailType,TimingWindowScale,LifeDifficulty,HiddenSongs,EasterEggs,AllowExtraStage";
const SONG_LUA_SCREEN_THEME_OPTIONS_LINE_NAMES: &str =
    "VisualStyle,MusicWheelSpeed,MusicWheelStyle,AutoStyle,DefaultGameMode,CasualMaxMeter";
const SONG_LUA_SCREEN_MENU_TIMER_OPTIONS_LINE_NAMES: &str =
    "MenuTimer,ScreenSelectMusicMenuTimer,ScreenPlayerOptionsMenuTimer,ScreenEvaluationMenuTimer";
const SONG_LUA_SCREEN_USB_PROFILE_OPTIONS_LINE_NAMES: &str = "MemoryCards,CustomSongs,MaxCount,CustomSongsLoadTimeout,CustomSongsMaxSeconds,CustomSongsMaxMegabytes";
const SONG_LUA_SCREEN_TOURNAMENT_MODE_OPTIONS_LINE_NAMES: &str =
    "EnableTournamentMode,ScoringSystem,StepStats,EnforceNoCmod";
const SONG_LUA_SCREEN_GROOVE_STATS_OPTIONS_LINE_NAMES: &str =
    "EnableGrooveStats,AutoDownloadUnlocks,SeparateUnlocksByPlayer,QRLogin,EnableOnlineLobbies";

fn theme_metric_string(group: &str, name: &str) -> Option<String> {
    if name.eq_ignore_ascii_case("LineNames") {
        return theme_line_names(group).map(str::to_string);
    }
    if name.eq_ignore_ascii_case("Fallback") {
        return theme_screen_fallback(group).map(str::to_string);
    }
    if let Some(row) = name.strip_prefix("Line") {
        if let Some(metric) = theme_explicit_line_metric(group, row) {
            return Some(metric.to_string());
        }
        if group.eq_ignore_ascii_case("ScreenOptionsService") {
            return Some(format!("gamecommand;screen,Screen{row};name,{row}"));
        }
        if theme_screen_fallback(group).is_some() && !row.trim().is_empty() {
            return Some(format!("conf,{row}"));
        }
    }
    None
}

fn theme_explicit_line_metric(group: &str, row: &str) -> Option<&'static str> {
    if group.eq_ignore_ascii_case("ScreenGraphicsSoundOptions") {
        return match row {
            "VideoRenderer" => Some("lua,OperatorMenuOptionRows.VideoRenderer()"),
            "DisplayAspectRatio" => Some("lua,ConfAspectRatio()"),
            "DisplayResolution" => Some("lua,ConfDisplayResolution()"),
            "DisplayMode" => Some("lua,ConfDisplayMode()"),
            "FullscreenType" => Some("lua,ConfFullscreenType()"),
            "GlobalOffsetSeconds" => Some("lua,OperatorMenuOptionRows.GlobalOffsetSeconds()"),
            "VisualDelaySeconds" => Some("lua,OperatorMenuOptionRows.VisualDelaySeconds()"),
            _ => None,
        };
    }
    if group.eq_ignore_ascii_case("ScreenSystemOptions") {
        return match row {
            "Theme" => Some("lua,OperatorMenuOptionRows.Theme()"),
            "EditorNoteSkin" => Some("lua,OperatorMenuOptionRows.EditorNoteskin()"),
            _ => None,
        };
    }
    None
}

const fn theme_line_names(group: &str) -> Option<&'static str> {
    if group.eq_ignore_ascii_case("ScreenPlayerOptions") {
        Some(SONG_LUA_SCREEN_PLAYER_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenPlayerOptions2") {
        Some(SONG_LUA_SCREEN_PLAYER_OPTIONS2_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenPlayerOptions3") {
        Some(SONG_LUA_SCREEN_PLAYER_OPTIONS3_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenAttackMenu") {
        Some(SONG_LUA_SCREEN_ATTACK_MENU_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenOptionsService") {
        Some(SONG_LUA_SCREEN_OPTIONS_SERVICE_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenSystemOptions") {
        Some(SONG_LUA_SCREEN_SYSTEM_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenInputOptions") {
        Some(SONG_LUA_SCREEN_INPUT_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenGraphicsSoundOptions") {
        Some(SONG_LUA_SCREEN_GRAPHICS_SOUND_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenVisualOptions") {
        Some(SONG_LUA_SCREEN_VISUAL_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenAppearanceOptions") {
        Some(SONG_LUA_SCREEN_APPEARANCE_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenArcadeOptions") {
        Some(SONG_LUA_SCREEN_ARCADE_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenAdvancedOptions") {
        Some(SONG_LUA_SCREEN_ADVANCED_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenThemeOptions") {
        Some(SONG_LUA_SCREEN_THEME_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenMenuTimerOptions") {
        Some(SONG_LUA_SCREEN_MENU_TIMER_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenUSBProfileOptions") {
        Some(SONG_LUA_SCREEN_USB_PROFILE_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenTournamentModeOptions") {
        Some(SONG_LUA_SCREEN_TOURNAMENT_MODE_OPTIONS_LINE_NAMES)
    } else if group.eq_ignore_ascii_case("ScreenGrooveStatsOptions") {
        Some(SONG_LUA_SCREEN_GROOVE_STATS_OPTIONS_LINE_NAMES)
    } else {
        None
    }
}

fn theme_screen_fallback(group: &str) -> Option<&'static str> {
    with_ascii_lowercase_overlay_key(group, theme_screen_fallback_normalized)
}

fn theme_screen_fallback_normalized(key: &str) -> Option<&'static str> {
    match key {
        "screenoptionsservice" => Some("ScreenOptionsSimple"),
        "screenvisualoptions" => Some("ScreenOptionsServiceSub"),
        "screensystemoptions"
        | "screeninputoptions"
        | "screengraphicssoundoptions"
        | "screenappearanceoptions"
        | "screenarcadeoptions"
        | "screenadvancedoptions"
        | "screenthemeoptions"
        | "screenmenutimeroptions"
        | "screenusbprofileoptions"
        | "screentournamentmodeoptions"
        | "screengroovestatsoptions" => Some("ScreenOptionsServiceChild"),
        _ => None,
    }
}

pub fn theme_metric_names(group: &str) -> Vec<String> {
    let mut names = Vec::new();
    if group.eq_ignore_ascii_case("ScreenGameplay") {
        names.extend(
            [
                "MinSecondsToStep",
                "MinSecondsToMusic",
                "MinSecondsToStepNextSong",
            ]
            .into_iter()
            .map(str::to_string),
        );
        for player in 1..=2 {
            names.extend(THEME_PLAYER_X_STYLES.map(|style| format!("PlayerP{player}{style}X")));
        }
    }
    if group.eq_ignore_ascii_case("Common") {
        names.push("ScreenHeight".to_owned());
    }
    if theme_line_names(group).is_some() {
        names.push("LineNames".to_string());
    }
    if theme_screen_fallback(group).is_some() {
        names.push("Fallback".to_string());
    }
    if let Some(lines) = theme_line_names(group) {
        names.extend(
            lines
                .split(',')
                .filter(|line| !line.trim().is_empty())
                .map(|line| format!("Line{}", line.trim())),
        );
    }
    if group.eq_ignore_ascii_case("Player") {
        names.extend(
            [
                "ReceptorArrowsYStandard",
                "ReceptorArrowsYReverse",
                "DrawDistanceBeforeTargetsPixels",
                "DrawDistanceAfterTargetsPixels",
            ]
            .into_iter()
            .map(str::to_string),
        );
    } else if group.eq_ignore_ascii_case("Common") {
        names.extend(
            ["DefaultNoteSkinName", "AutoSetStyle"]
                .into_iter()
                .map(str::to_string),
        );
    } else if group.eq_ignore_ascii_case("Combo") {
        names.push("ShowComboAt".to_string());
    } else if group.eq_ignore_ascii_case("GraphDisplay") {
        names.extend(["BodyWidth", "BodyHeight"].into_iter().map(str::to_string));
    } else if group.eq_ignore_ascii_case("LifeMeterBar") {
        names.push("InitialValue".to_string());
        names.extend(
            [
                "W1", "W2", "W3", "W4", "W5", "Miss", "LetGo", "Held", "HitMine",
            ]
            .map(|judgment| format!("LifePercentChange{judgment}")),
        );
    } else if group.eq_ignore_ascii_case("ScoreKeeperNormal") {
        for prefix in ["PercentScoreWeight", "GradeWeight"] {
            names.extend(
                [
                    "W1",
                    "W2",
                    "W3",
                    "W4",
                    "W5",
                    "Miss",
                    "LetGo",
                    "Held",
                    "HitMine",
                    "CheckpointHit",
                ]
                .map(|judgment| format!("{prefix}{judgment}")),
            );
        }
    } else if group.eq_ignore_ascii_case("MusicWheel") {
        names.push("NumWheelItems".to_string());
    } else if group.eq_ignore_ascii_case("PlayerStageStats") {
        names.push("NumGradeTiersUsed".to_string());
    } else if group.eq_ignore_ascii_case("ScreenHeartEntry") {
        names.push("HeartEntryEnabled".to_string());
    }
    names.sort_unstable();
    names.dedup();
    names
}

pub fn theme_string_names(section: &str) -> Vec<String> {
    if section.eq_ignore_ascii_case("Difficulty")
        || section.eq_ignore_ascii_case("CustomDifficulty")
    {
        return [
            SongLuaDifficulty::Beginner,
            SongLuaDifficulty::Easy,
            SongLuaDifficulty::Medium,
            SongLuaDifficulty::Hard,
            SongLuaDifficulty::Challenge,
            SongLuaDifficulty::Edit,
        ]
        .into_iter()
        .map(|difficulty| difficulty.sm_name().to_string())
        .collect();
    }
    if matches!(
        section,
        "OptionTitles"
            | "OptionNames"
            | "ThemePrefs"
            | "SLPlayerOptions"
            | "ScreenSelectPlayMode"
            | "ScreenSelectStyle"
            | "GameButton"
            | "TapNoteScore"
            | "TapNoteScoreFA+"
            | "HoldNoteScore"
            | "Stage"
            | "Months"
    ) {
        return [
            "Yes",
            "No",
            "Cancel",
            "DisplayMode",
            "MusicRate",
            "SpeedMod",
            "NoteSkin",
            "Difficulty_Hard",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
    }
    Vec::new()
}

#[must_use]
pub fn theme_string(section: &str, name: &str) -> String {
    // Simply Love and its fallback define no gameplay HeaderText string.
    // ThemeManager's missing lookup returns an empty string, not the key.
    if section == "ScreenGameplay" && name == "HeaderText" {
        return String::new();
    }
    if section == "OptionTitles" && name == "MusicRateMultiplier" {
        return "x rate".into();
    }
    if section.eq_ignore_ascii_case("Difficulty")
        || section.eq_ignore_ascii_case("CustomDifficulty")
    {
        return name.trim_start_matches("Difficulty_").to_string();
    }
    if matches!(
        section,
        "OptionTitles"
            | "OptionNames"
            | "ThemePrefs"
            | "SLPlayerOptions"
            | "ScreenSelectPlayMode"
            | "ScreenSelectStyle"
            | "GameButton"
            | "TapNoteScore"
            | "TapNoteScoreFA+"
            | "HoldNoteScore"
            | "Stage"
            | "Months"
    ) {
        return name.replace('_', " ");
    }
    match name {
        "Yes" => "Yes".to_string(),
        "No" => "No".to_string(),
        "Cancel" => "Cancel".to_string(),
        _ => name.to_string(),
    }
}

#[must_use]
pub fn theme_has_string(section: &str, name: &str) -> bool {
    section.eq_ignore_ascii_case("Difficulty")
        || section.eq_ignore_ascii_case("CustomDifficulty")
        || matches!(
            section,
            "OptionTitles"
                | "OptionNames"
                | "ThemePrefs"
                | "SLPlayerOptions"
                | "ScreenSelectPlayMode"
                | "ScreenSelectStyle"
                | "GameButton"
                | "TapNoteScore"
                | "TapNoteScoreFA+"
                | "HoldNoteScore"
                | "Stage"
                | "Months"
        )
        || matches!(name, "Yes" | "No" | "Cancel")
}

#[must_use]
pub const fn song_lua_arch_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "Mac OS X"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(target_os = "freebsd") {
        "FreeBSD"
    } else {
        "Unknown"
    }
}

#[must_use]
pub fn custom_multi_modifier_key(option_name: &str, choice: &str) -> String {
    if option_name.eq_ignore_ascii_case("Hide") {
        format!("Hide{choice}")
    } else {
        choice.to_string()
    }
}

pub type SongLuaNoteskinPathResolver = fn(&str, &str, &str) -> Option<PathBuf>;
pub type SongLuaNoteskinMetricResolver = fn(&str, &str, &str) -> Option<String>;
pub type SongLuaNoteskinMetricFResolver = fn(&str, &str, &str) -> Option<f32>;
pub type SongLuaNoteskinMetricBResolver = fn(&str, &str, &str) -> Option<bool>;
pub type SongLuaNoteskinExistsResolver = fn(&str) -> bool;
pub type SongLuaNoteskinNamesResolver = fn() -> Vec<String>;

#[derive(Clone, Copy)]
pub struct SongLuaNoteskinResolver {
    pub resolve_path: SongLuaNoteskinPathResolver,
    pub metric: SongLuaNoteskinMetricResolver,
    pub metric_f: SongLuaNoteskinMetricFResolver,
    pub metric_b: SongLuaNoteskinMetricBResolver,
    pub exists: SongLuaNoteskinExistsResolver,
    pub names: SongLuaNoteskinNamesResolver,
}

const fn missing_noteskin_path(_: &str, _: &str, _: &str) -> Option<PathBuf> {
    None
}

const fn missing_noteskin_metric(_: &str, _: &str, _: &str) -> Option<String> {
    None
}

const fn missing_noteskin_metric_f(_: &str, _: &str, _: &str) -> Option<f32> {
    None
}

const fn missing_noteskin_metric_b(_: &str, _: &str, _: &str) -> Option<bool> {
    None
}

const fn missing_noteskin_exists(_: &str) -> bool {
    false
}

const fn missing_noteskin_names() -> Vec<String> {
    Vec::new()
}

impl Default for SongLuaNoteskinResolver {
    fn default() -> Self {
        Self {
            resolve_path: missing_noteskin_path,
            metric: missing_noteskin_metric,
            metric_f: missing_noteskin_metric_f,
            metric_b: missing_noteskin_metric_b,
            exists: missing_noteskin_exists,
            names: missing_noteskin_names,
        }
    }
}

impl SongLuaNoteskinResolver {
    #[inline(always)]
    #[must_use]
    pub fn resolve_path(self, skin: &str, button: &str, element: &str) -> Option<PathBuf> {
        (self.resolve_path)(skin, button, element)
    }

    #[inline(always)]
    #[must_use]
    pub fn path_string(self, skin: &str, button: &str, element: &str) -> String {
        self.resolve_path(skin, button, element)
            .map(|path| file_path_string(path.as_path()))
            .unwrap_or_default()
    }

    #[inline(always)]
    #[must_use]
    pub fn metric(self, skin: &str, element: &str, value: &str) -> Option<String> {
        (self.metric)(skin, element, value)
    }

    #[inline(always)]
    #[must_use]
    pub fn metric_f(self, skin: &str, element: &str, value: &str) -> Option<f32> {
        (self.metric_f)(skin, element, value)
    }

    #[must_use]
    pub fn metric_i(self, skin: &str, element: &str, value: &str) -> i64 {
        let Some(metric) = self.metric(skin, element, value) else {
            return 0;
        };
        let metric = metric.trim();
        metric
            .parse::<i64>()
            .ok()
            .or_else(|| {
                metric
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .map(|value| value.round().clamp(i64::MIN as f64, i64::MAX as f64) as i64)
            })
            .unwrap_or(0)
    }

    #[inline(always)]
    #[must_use]
    pub fn metric_b(self, skin: &str, element: &str, value: &str) -> Option<bool> {
        (self.metric_b)(skin, element, value)
    }

    #[inline(always)]
    #[must_use]
    pub fn exists(self, skin: &str) -> bool {
        (self.exists)(skin)
    }

    #[inline(always)]
    #[must_use]
    pub fn names(self) -> Vec<String> {
        (self.names)()
    }
}

#[derive(Debug, Clone)]
pub struct SongLuaStep {
    pub steps_type: String,
    pub difficulty: SongLuaDifficulty,
    pub description: String,
    pub chart_name: String,
    pub credit: String,
    pub meter: u32,
    pub music_file: String,
    /// Chart-load maximum before gameplay modifiers, in ITG dance points.
    pub possible_grade_points: i32,
}

impl From<&deadsync_chart::ChartData> for SongLuaStep {
    fn from(chart: &deadsync_chart::ChartData) -> Self {
        let steps_type = chart
            .chart_type
            .split('-')
            .map(|part| {
                let mut chars = part.chars();
                chars.next().map_or_else(String::new, |first| {
                    format!(
                        "{}{rest}",
                        first.to_ascii_uppercase(),
                        rest = chars.as_str()
                    )
                })
            })
            .collect::<Vec<_>>()
            .join("_");
        Self {
            steps_type: format!("StepsType_{steps_type}"),
            difficulty: with_compact_ascii_lowercase_song_key(
                &chart.difficulty,
                song_lua_difficulty_from_normalized,
            )
            .unwrap_or(SongLuaDifficulty::Edit),
            description: chart.description.clone(),
            chart_name: chart.chart_name.clone(),
            credit: chart.step_artist.clone(),
            meter: chart.meter,
            music_file: chart.music_file.clone(),
            possible_grade_points: chart.possible_grade_points,
        }
    }
}

/// A raw process/calendar clock read observed outside chart state.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone)]
pub struct SongLuaHostClockInput {
    pub clock: String,
    pub source: String,
    pub line: i32,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct SongLuaCompileContext {
    /// Optional worker progress; never changes script execution or output.
    pub bake_progress: Option<SongLuaBakeProgress>,
    /// Retain the initialized scene for actual gameplay instead of probing
    /// future callbacks on the load worker.
    pub live: bool,
    /// Let the caller supply the first live Update(0) clock after preparation.
    /// Recorded gameplay replay uses this instead of a placeholder clock.
    pub defer_initial_live_update: bool,
    /// Load a gameplay screen before it is pushed onto SCREENMAN's stack.
    /// Construction and Init see no top screen; Begin sees the loaded screen.
    /// Standalone actor compilation uses its already active host by default.
    pub screen_loading: bool,
    /// Session entry paths list backgrounds first, then foregrounds.
    /// A single-entry compile defaults to the foreground layer.
    pub background_layer_count: usize,
    /// Authored song beats for session entries, in the same path order.
    /// Empty for direct actor compilation without a simfile layer schedule.
    pub layer_start_beats: Vec<f32>,
    /// Native background layer and complete activation schedule per cached owner.
    pub background_activations: Vec<(u8, Vec<deadsync_chart::song::SongBackgroundLuaActivation>)>,
    /// Native UpperLeft effect colors for cached Lua background owners.
    pub background_upper_left_colors: Vec<Option<[f32; 4]>>,
    /// Authored foreground filenames, in foreground entry order. Empty when
    /// compiling standalone actors without simfile foreground declarations.
    pub foreground_names: Vec<String>,
    pub song_dir: PathBuf,
    /// Resolved artwork from the parsed song, including non-conventional filenames.
    pub song_banner_path: Option<PathBuf>,
    pub main_title: String,
    pub song_display_bpms: [f32; 2],
    pub song_timing_bpms: Vec<(f32, f32)>,
    /// Global song timing with pauses/warps; distinct from selected chart timing.
    pub song_timing: Option<deadsync_rules::timing::TimingData>,
    /// Selected chart timing, shared with gameplay; populated before Lua compilation.
    pub player_timing: [Option<deadsync_rules::timing::TimingData>; LUA_PLAYERS],
    /// Timing for every chart in `song_steps` order, loaded before compilation.
    /// Song-local ownership keeps unselected SSC timing out of the library cache.
    pub song_step_timing: Vec<deadsync_rules::timing::TimingData>,
    /// Minimized chart notes in `song_steps` order, loaded on the song worker.
    pub song_step_notes: Vec<Arc<[u8]>>,
    /// Beat positions for minimized rows, in the same chart and row order.
    pub song_step_row_beats: Vec<Arc<[f32]>>,
    pub song_music_rate: f32,
    /// Initial ITGmania PRNG seed; fixed per compilation for repeatable playback.
    /// Scripts may reseed it through math.randomseed or MersenneTwister.Seed.
    pub random_seed: u32,
    /// Captured subscriber identity ranks for deterministic native replay.
    /// Paths start with a one-based session entry and child indices, or an
    /// external screen path. These inputs contain no handler results.
    #[cfg(feature = "test-support")]
    pub message_actor_order: Vec<(String, usize)>,
    /// Observational capture profile; does not change actor dispatch or queries.
    #[cfg(feature = "test-support")]
    pub trace_dynamic_commands: bool,
    /// Explicit initial collector input for deterministic diagnostic captures.
    #[cfg(feature = "test-support")]
    pub initial_lua_gc_stopped: bool,
    /// Include the capture tool's visible Overlay probe when replaying its input.
    #[cfg(feature = "test-support")]
    pub capture_overlay_probe: bool,
    /// External clock reads for diagnostic replay, never actor/query results.
    #[cfg(feature = "test-support")]
    pub host_clock_inputs: Vec<SongLuaHostClockInput>,
    /// Diagnostic compiler inputs: raw music seconds, song beat, actor delta.
    /// These are external clocks, never expected actor states or draw results.
    #[cfg(feature = "test-support")]
    pub compile_clock_inputs: Vec<[f64; 3]>,
    pub music_length_seconds: f32,
    /// Whole-song note bounds resolved at catalog load, independently of
    /// audio duration. Note times include the load offset; hints stay absolute.
    pub song_note_bounds: Option<[f32; 2]>,
    pub style_name: String,
    /// Song charts in simfile load order; never synthesize missing difficulties.
    pub song_steps: Vec<SongLuaStep>,
    /// Lua catalog entries map to playable payloads; an empty map is identity.
    pub song_step_chart_indices: Vec<Option<usize>>,
    pub player_step_indices: [Option<usize>; LUA_PLAYERS],
    pub event_mode: bool,
    pub global_offset_seconds: f32,
    /// Theme coordinate space exposed through `SCREEN_WIDTH/SCREEN_HEIGHT`.
    pub screen_width: f32,
    pub screen_height: f32,
    /// Physical video mode exposed through `DISPLAY` and `PREFSMAN`.
    pub display_width: f32,
    pub display_height: f32,
    /// Configured preference, independent of the selected renderer's size.
    pub display_aspect_ratio: f32,
    /// Capability of the selected renderer, independent of theme helpers.
    pub render_to_texture: bool,
    pub video_renderers: String,
    /// Last driver recorded by the engine, independent of renderer selection.
    pub last_seen_video_driver: String,
    pub players: [SongLuaPlayerContext; LUA_PLAYERS],
    pub confusion_offset_available: bool,
    pub confusion_available: bool,
    pub amod_available: bool,
}

impl SongLuaCompileContext {
    pub(crate) fn report_bake_progress(&self, fraction: f32) {
        if !self.live && let Some(progress) = &self.bake_progress {
            progress.report(fraction);
        }
    }

    pub fn new(song_dir: impl Into<PathBuf>, main_title: impl Into<String>) -> Self {
        // LoadActor stores resolved media paths. Resolve the entry directory
        // once so a relative song root is not prepended again when a nested
        // actor's texture is compiled or rebound by the live renderer.
        let song_dir = song_dir.into();
        let song_dir = std::path::absolute(&song_dir).unwrap_or(song_dir);
        Self {
            background_layer_count: 0,
            bake_progress: None,
            layer_start_beats: Vec::new(),
            background_activations: Vec::new(),
            background_upper_left_colors: Vec::new(),
            foreground_names: Vec::new(),
            live: false,
            defer_initial_live_update: false,
            screen_loading: false,
            song_dir,
            song_banner_path: None,
            main_title: main_title.into(),
            song_display_bpms: [60.0, 60.0],
            song_timing_bpms: Vec::new(),
            song_timing: None,
            player_timing: std::array::from_fn(|_| None),
            song_step_timing: Vec::new(),
            song_step_notes: Vec::new(),
            song_step_row_beats: Vec::new(),
            song_music_rate: 1.0,
            random_seed: 1,
            #[cfg(feature = "test-support")]
            message_actor_order: Vec::new(),
            #[cfg(feature = "test-support")]
            trace_dynamic_commands: false,
            #[cfg(feature = "test-support")]
            initial_lua_gc_stopped: false,
            #[cfg(feature = "test-support")]
            capture_overlay_probe: false,
            #[cfg(feature = "test-support")]
            host_clock_inputs: Vec::new(),
            #[cfg(feature = "test-support")]
            compile_clock_inputs: Vec::new(),
            music_length_seconds: 0.0,
            song_note_bounds: None,
            style_name: "single".to_string(),
            song_steps: Vec::new(),
            song_step_chart_indices: Vec::new(),
            player_step_indices: [None; LUA_PLAYERS],
            // PrefsManager defaults EventMode to true in ITGmania.
            event_mode: true,
            global_offset_seconds: 0.0,
            screen_width: 640.0,
            screen_height: 480.0,
            display_width: 640.0,
            display_height: 480.0,
            display_aspect_ratio: 16.0 / 9.0,
            render_to_texture: true,
            video_renderers: "software".to_string(),
            // PrefsManager initializes this preference before video setup.
            last_seen_video_driver: String::new(),
            players: std::array::from_fn(|_| SongLuaPlayerContext::default()),
            confusion_offset_available: true,
            confusion_available: true,
            amod_available: true,
        }
    }
}

#[must_use]
pub fn song_lua_default_noteskin_name(context: &SongLuaCompileContext) -> String {
    context
        .players
        .iter()
        .find(|player| player.enabled)
        .map(|player| player.noteskin_name.clone())
        .or_else(|| {
            context
                .players
                .first()
                .map(|player| player.noteskin_name.clone())
        })
        .unwrap_or_else(|| SONG_LUA_DEFAULT_NOTESKIN_NAME.to_string())
}

#[inline(always)]
#[must_use]
pub fn song_display_bps(context: &SongLuaCompileContext) -> f32 {
    (context.song_display_bpms[0].max(context.song_display_bpms[1]) / 60.0).max(f32::EPSILON)
}

#[inline(always)]
#[must_use]
pub fn song_music_rate(context: &SongLuaCompileContext) -> f32 {
    song_music_rate_value(context.song_music_rate)
}

#[must_use]
pub fn parse_song_timing_bpms(source: &str) -> Vec<(f32, f32)> {
    let mut bpms = source
        .split(',')
        .filter_map(|entry| {
            let (beat, bpm) = entry.split_once('=')?;
            let beat = beat.trim().parse::<f32>().ok()?;
            let bpm = bpm.trim().parse::<f32>().ok()?;
            (beat.is_finite() && bpm.is_finite() && bpm > 0.0).then_some((beat, bpm))
        })
        .collect::<Vec<_>>();
    bpms.sort_by(|left, right| left.0.total_cmp(&right.0));
    bpms
}

#[must_use]
pub fn song_elapsed_seconds_at(beat: f32, context: &SongLuaCompileContext) -> f32 {
    if let Some(timing) = &context.song_timing {
        return (timing.get_time_for_beat_exact(beat) - timing.get_time_for_beat_exact(0.0))
            / song_music_rate(context);
    }
    let rate = song_music_rate(context);
    let mut cursor_beat = 0.0;
    let mut seconds = 0.0;
    let mut bpm = context
        .song_timing_bpms
        .first()
        .filter(|(segment_beat, segment_bpm)| *segment_beat <= 0.0 && *segment_bpm > 0.0)
        .map_or_else(|| song_display_bps(context) * 60.0, |segment| segment.1);
    for &(segment_beat, segment_bpm) in &context.song_timing_bpms {
        if segment_beat > beat {
            break;
        }
        seconds += (segment_beat - cursor_beat) * 60.0 / bpm;
        cursor_beat = segment_beat;
        bpm = segment_bpm.max(f32::EPSILON);
    }
    (seconds + (beat - cursor_beat) * 60.0 / bpm) / rate
}

#[must_use]
pub fn song_beat_at_elapsed_seconds(seconds: f32, context: &SongLuaCompileContext) -> f32 {
    song_beat_at_seconds64(f64::from(seconds), context) as f32
}

// Lua numbers and the reference update clock are doubles. Narrow only when
// storing runtime windows, after the chart's strict boundary predicates run.
fn song_beat_at_seconds64(seconds: f64, context: &SongLuaCompileContext) -> f64 {
    if let Some(timing) = &context.song_timing {
        let music_second = (seconds * f64::from(song_music_rate(context))) as f32
            + timing.get_time_for_beat_exact(0.0);
        return f64::from(timing.get_song_position(music_second).beat);
    }
    let target = seconds * f64::from(song_music_rate(context));
    let mut cursor_beat = 0.0;
    let mut cursor_seconds = 0.0;
    let mut bpm = context
        .song_timing_bpms
        .first()
        .filter(|(segment_beat, segment_bpm)| *segment_beat <= 0.0 && *segment_bpm > 0.0)
        .map_or_else(
            || {
                f64::from(context.song_display_bpms[0].max(context.song_display_bpms[1]))
                    .max(f64::from(f32::EPSILON) * 60.0)
            },
            |segment| f64::from(segment.1),
        );
    for &(segment_beat, segment_bpm) in &context.song_timing_bpms {
        let segment_beat = f64::from(segment_beat);
        let segment_bpm = f64::from(segment_bpm);
        let next_seconds = cursor_seconds + (segment_beat - cursor_beat) * 60.0 / bpm;
        if next_seconds > target {
            break;
        }
        cursor_beat = segment_beat;
        cursor_seconds = next_seconds;
        bpm = segment_bpm.max(f64::EPSILON);
    }
    cursor_beat + (target - cursor_seconds) * bpm / 60.0
}

#[must_use]
pub fn song_bps_at_beat(beat: f32, context: &SongLuaCompileContext) -> f32 {
    context
        .song_timing_bpms
        .iter()
        .take_while(|(segment_beat, _)| *segment_beat <= beat)
        .last()
        .map_or_else(|| song_display_bps(context), |segment| segment.1 / 60.0)
        .max(f32::EPSILON)
}

#[inline(always)]
#[must_use]
pub fn song_elapsed_seconds_for_beat(beat: f32, song_bps: f32, music_rate: f32) -> f32 {
    beat / (song_bps.max(f32::EPSILON) * music_rate.max(f32::EPSILON))
}

#[inline(always)]
#[must_use]
pub fn mod_window_cmp(left: &SongLuaModWindow, right: &SongLuaModWindow) -> std::cmp::Ordering {
    left.start
        .total_cmp(&right.start)
        .then_with(|| left.limit.total_cmp(&right.limit))
        .then_with(|| left.mods.cmp(&right.mods))
}

#[inline(always)]
#[must_use]
pub fn ease_window_cmp(left: &SongLuaEaseWindow, right: &SongLuaEaseWindow) -> std::cmp::Ordering {
    left.start
        .total_cmp(&right.start)
        .then_with(|| left.limit.total_cmp(&right.limit))
}

#[inline(always)]
#[must_use]
pub fn message_event_cmp(
    left: &SongLuaMessageEvent,
    right: &SongLuaMessageEvent,
) -> std::cmp::Ordering {
    left.beat.total_cmp(&right.beat)
}

#[inline(always)]
#[must_use]
pub fn overlay_ease_cmp(
    left: &SongLuaOverlayEase,
    right: &SongLuaOverlayEase,
) -> std::cmp::Ordering {
    left.start
        .total_cmp(&right.start)
        .then_with(|| left.limit.total_cmp(&right.limit))
        .then_with(|| left.overlay_index.cmp(&right.overlay_index))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaTimeUnit {
    Beat,
    Second,
    /// Sampled music seconds relative to beat zero on a matching BPM clock.
    BeatClock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaSpanMode {
    Len,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SongLuaEaseTarget {
    Mod(String),
    PlayerX,
    PlayerY,
    PlayerZ,
    PlayerRotationX,
    PlayerRotationZ,
    PlayerRotationY,
    PlayerSkewX,
    PlayerSkewY,
    PlayerZoom,
    PlayerZoomX,
    PlayerZoomY,
    PlayerZoomZ,
    Function,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaModWindow {
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaSpanMode,
    pub mods: String,
    pub player: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaEaseWindow {
    /// Native option units per second for sampled Song-level targets.
    /// None applies the authored ease directly to Current.
    pub approach_speed: Option<f32>,
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaSpanMode,
    pub from: f32,
    pub to: f32,
    pub target: SongLuaEaseTarget,
    pub easing: Option<String>,
    pub player: Option<u8>,
    pub sustain: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaMessageEvent {
    pub beat: f32,
    pub message: String,
    pub persists: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SongLuaCompileInfo {
    pub unsupported_perframes: usize,
    pub unsupported_function_eases: usize,
    pub unsupported_function_actions: usize,
    pub unsupported_perframe_captures: Vec<String>,
    pub unsupported_function_ease_captures: Vec<String>,
    pub unsupported_function_action_captures: Vec<String>,
    pub skipped_message_command_captures: Vec<String>,
}

pub fn push_unique_compile_detail(out: &mut Vec<String>, detail: String) {
    if !out.contains(&detail) {
        out.push(detail);
    }
}

pub fn record_unsupported_function_ease_capture(
    info: &mut SongLuaCompileInfo,
    unit: SongLuaTimeUnit,
    start: f32,
    limit: f32,
    span_mode: SongLuaSpanMode,
    from: f32,
    to: f32,
    easing: &Option<String>,
    probe_methods: &[String],
) -> String {
    info.unsupported_function_eases += 1;
    let detail = format!(
        "function ease unit={unit:?} start={start:.3} limit={limit:.3} \
         span={span_mode:?} from={from:.3} to={to:.3} easing={easing:?} \
         probe_methods={probe_methods:?}"
    );
    push_unique_compile_detail(&mut info.unsupported_function_ease_captures, detail.clone());
    detail
}

pub fn record_unsupported_function_action_capture(
    info: &mut SongLuaCompileInfo,
    beat: f32,
    persists: bool,
) -> String {
    info.unsupported_function_actions += 1;
    let detail = format!("function action beat={beat:.3} persists={persists}");
    push_unique_compile_detail(
        &mut info.unsupported_function_action_captures,
        detail.clone(),
    );
    detail
}

pub fn merge_compile_info(out: &mut SongLuaCompileInfo, info: SongLuaCompileInfo) {
    out.unsupported_perframes += info.unsupported_perframes;
    out.unsupported_function_eases += info.unsupported_function_eases;
    out.unsupported_function_actions += info.unsupported_function_actions;
    for detail in info.unsupported_perframe_captures {
        push_unique_compile_detail(&mut out.unsupported_perframe_captures, detail);
    }
    for detail in info.unsupported_function_ease_captures {
        push_unique_compile_detail(&mut out.unsupported_function_ease_captures, detail);
    }
    for detail in info.unsupported_function_action_captures {
        push_unique_compile_detail(&mut out.unsupported_function_action_captures, detail);
    }
    for detail in info.skipped_message_command_captures {
        push_unique_compile_detail(&mut out.skipped_message_command_captures, detail);
    }
}

/// Requests captured after script initialization, before timeline sampling.
/// They apply only to this play and never modify persisted player preferences.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SongLuaStartup {
    pub noteskins: [Option<String>; LUA_PLAYERS],
    pub min_seconds_to_music: Option<f32>,
    pub hide_in: bool,
}

/// Direct boolean setters observed during chronological reference replay.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaBoolWrite {
    pub player: usize,
    pub key: String,
    pub beat: f64,
    pub second: f64,
    pub previous: bool,
    pub current: bool,
    pub chained: bool,
}

/// Native speed fields in TimeSpacing, ScrollSpeed, ScrollBPM, MaxScrollBPM order.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaSpeedWrite {
    pub player: usize,
    pub key: String,
    pub beat: f64,
    pub second: f64,
    pub previous: [[f32; 2]; 4],
    pub current: [[f32; 2]; 4],
    pub failed: bool,
    pub chained: bool,
}

#[cfg(feature = "test-support")]
#[derive(Debug, Clone)]
pub struct SongLuaSkinWrite {
    pub player: usize,
    pub key: String,
    pub beat: f64,
    pub second: f64,
    pub previous: String,
    pub current: String,
}

#[cfg(feature = "test-support")]
#[derive(Debug, Clone)]
pub struct SongLuaStateCall {
    pub operation: &'static str,
    pub beat: f32,
    pub second: f32,
    pub args: Vec<serde_json::Value>,
}

/// Compiled song output and its optional retained live scene.
pub struct CompiledSongLua<OverlayActor> {
    pub live_scene: Option<SongLuaLiveScene>,
    /// Retained scene positions corresponding to this layer's overlays.
    pub live_indices: Vec<usize>,
    /// Charts whose note fields were constructed after startup reloads.
    pub loaded_steps: Option<[Option<usize>; LUA_PLAYERS]>,
    pub loaded_style: Option<String>,
    pub startup_reloads: u32,
    pub startup: SongLuaStartup,
    pub entry_path: PathBuf,
    pub screen_width: f32,
    pub screen_height: f32,
    pub beat_mods: Vec<SongLuaModWindow>,
    pub time_mods: Vec<SongLuaModWindow>,
    pub eases: Vec<SongLuaEaseWindow>,
    pub messages: Vec<SongLuaMessageEvent>,
    pub sound_paths: Vec<PathBuf>,
    pub overlays: Vec<OverlayActor>,
    /// Chronological explicit draw passes, baked on the song-load worker.
    pub draw_frames: Vec<SongLuaDrawFrame>,
    /// Captured ScreenGameplay translation and vibration shared by all screen draws.
    pub screen_overlay_index: Option<usize>,
    pub overlay_eases: Vec<SongLuaOverlayEase>,
    /// Clock shared by rendered update samples; pauses need distinct song seconds.
    pub overlay_update_unit: SongLuaTimeUnit,
    pub overlay_updates: Vec<SongLuaOverlayUpdateTrack>,
    /// Setter arguments retained only for reference audits; rendered tracks may tween them.
    #[cfg(feature = "test-support")]
    pub overlay_writes: Vec<SongLuaOverlayUpdateTrack>,
    #[cfg(feature = "test-support")]
    pub boolean_writes: Vec<SongLuaBoolWrite>,
    #[cfg(feature = "test-support")]
    pub speed_writes: Vec<SongLuaSpeedWrite>,
    #[cfg(feature = "test-support")]
    pub state_calls: Vec<SongLuaStateCall>,
    #[cfg(feature = "test-support")]
    pub noteskin_writes: Vec<SongLuaSkinWrite>,
    pub stateful_message_captures: Vec<SongLuaStatefulMessageCapture>,
    pub player_actors: [SongLuaCapturedActor; LUA_PLAYERS],
    pub song_foreground: SongLuaCapturedActor,
    /// Underlay, Overlay, and SongBackground retain their own visibility/tweens.
    pub screen_layers: [SongLuaCapturedActor; 4],
    pub hidden_players: [bool; LUA_PLAYERS],
    pub note_hides: Vec<SongLuaNoteHideWindow>,
    pub column_offsets: Vec<SongLuaColumnOffsetWindow>,
    pub column_splines: Vec<deadsync_gameplay::SongLuaColumnSplineTrack>,
    pub column_spline_origin: Option<f32>,
    pub info: SongLuaCompileInfo,
}

impl<A: std::fmt::Debug> std::fmt::Debug for CompiledSongLua<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = f.debug_struct("CompiledSongLua");
        out.field("live_scene", &self.live_scene);
        out.field("live_indices", &self.live_indices);
        out.field("loaded_steps", &self.loaded_steps);
        out.field("loaded_style", &self.loaded_style);
        out.field("startup_reloads", &self.startup_reloads);
        out.field("startup", &self.startup);
        out.field("entry_path", &self.entry_path);
        out.field("screen_width", &self.screen_width);
        out.field("screen_height", &self.screen_height);
        out.field("beat_mods", &self.beat_mods);
        out.field("time_mods", &self.time_mods);
        out.field("eases", &self.eases);
        out.field("messages", &self.messages);
        out.field("sound_paths", &self.sound_paths);
        out.field("overlays", &self.overlays);
        out.field("draw_frames", &self.draw_frames);
        out.field("screen_overlay_index", &self.screen_overlay_index);
        out.field("overlay_eases", &self.overlay_eases);
        out.field("overlay_update_unit", &self.overlay_update_unit);
        out.field("overlay_updates", &self.overlay_updates);
        #[cfg(feature = "test-support")]
        out.field("overlay_writes", &self.overlay_writes);
        #[cfg(feature = "test-support")]
        out.field("boolean_writes", &self.boolean_writes);
        #[cfg(feature = "test-support")]
        out.field("speed_writes", &self.speed_writes);
        #[cfg(feature = "test-support")]
        out.field("state_calls", &self.state_calls);
        #[cfg(feature = "test-support")]
        out.field("noteskin_writes", &self.noteskin_writes);
        out.field("stateful_message_captures", &self.stateful_message_captures);
        out.field("player_actors", &self.player_actors);
        out.field("song_foreground", &self.song_foreground);
        out.field("screen_layers", &self.screen_layers);
        out.field("hidden_players", &self.hidden_players);
        out.field("note_hides", &self.note_hides);
        out.field("column_offsets", &self.column_offsets);
        out.field("column_splines", &self.column_splines);
        out.field("column_spline_origin", &self.column_spline_origin);
        out.field("info", &self.info);
        out.finish()
    }
}

impl<OverlayActor> CompiledSongLua<OverlayActor> {
    /// Positions in the retained scene, in this layer's overlay order.
    pub fn live_actor_indices(&self) -> &[usize] {
        &self.live_indices
    }
}

impl<OverlayActor> Default for CompiledSongLua<OverlayActor> {
    fn default() -> Self {
        Self {
            live_scene: None,
            live_indices: Vec::new(),
            loaded_steps: None,
            loaded_style: None,
            startup_reloads: 0,
            startup: SongLuaStartup::default(),
            entry_path: PathBuf::new(),
            screen_width: 0.0,
            screen_height: 0.0,
            beat_mods: Vec::new(),
            time_mods: Vec::new(),
            eases: Vec::new(),
            messages: Vec::new(),
            sound_paths: Vec::new(),
            overlays: Vec::new(),
            draw_frames: Vec::new(),
            screen_overlay_index: None,
            overlay_eases: Vec::new(),
            overlay_update_unit: SongLuaTimeUnit::Beat,
            overlay_updates: Vec::new(),
            #[cfg(feature = "test-support")]
            overlay_writes: Vec::new(),
            #[cfg(feature = "test-support")]
            boolean_writes: Vec::new(),
            #[cfg(feature = "test-support")]
            speed_writes: Vec::new(),
            #[cfg(feature = "test-support")]
            state_calls: Vec::new(),
            #[cfg(feature = "test-support")]
            noteskin_writes: Vec::new(),
            stateful_message_captures: Vec::new(),
            player_actors: std::array::from_fn(|_| SongLuaCapturedActor::default()),
            song_foreground: SongLuaCapturedActor::default(),
            screen_layers: std::array::from_fn(|_| SongLuaCapturedActor::default()),
            hidden_players: [false; LUA_PLAYERS],
            note_hides: Vec::new(),
            column_offsets: Vec::new(),
            column_splines: Vec::new(),
            column_spline_origin: None,
            info: SongLuaCompileInfo::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaSoundEvent {
    pub beat: f32,
    pub path: PathBuf,
    pub source_message: Option<String>,
    pub queued_startup: bool,
}

pub fn push_startup_message_if_listened<'a>(
    messages: &mut Vec<SongLuaMessageEvent>,
    command_lists: impl IntoIterator<Item = &'a [SongLuaOverlayMessageCommand]>,
) {
    if message_command_lists_have_listener(command_lists, SONG_LUA_STARTUP_MESSAGE) {
        messages.push(SongLuaMessageEvent {
            beat: 0.0,
            message: SONG_LUA_STARTUP_MESSAGE.to_string(),
            persists: false,
        });
    }
}

pub fn sort_compiled_song_lua<OverlayActor>(compiled: &mut CompiledSongLua<OverlayActor>) {
    compiled.beat_mods.sort_by(mod_window_cmp);
    compiled.time_mods.sort_by(mod_window_cmp);
    compiled.eases.sort_by(ease_window_cmp);
    compiled.overlay_eases.sort_by(overlay_ease_cmp);
    compiled.messages.sort_by(message_event_cmp);
}

pub fn runtime_static_overlay_index_by_path<'a>(
    len: usize,
    mut path_at: impl FnMut(usize) -> Option<&'a Path>,
) -> Option<usize> {
    (0..len).position(|index| {
        path_at(index).is_some_and(|path| {
            path.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .eq_ignore_ascii_case("_static 4x1.png")
            })
        })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaOverlayBlendMode {
    Alpha,
    Add,
    Multiply,
    Subtract,
    NoEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaTextGlowMode {
    Inner,
    Stroke,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLuaProxyTarget {
    Player { player_index: usize },
    NoteField { player_index: usize },
    Judgment { player_index: usize },
    Combo { player_index: usize },
    Score { player_index: usize },
    Underlay { hidden: bool },
    Overlay { hidden: bool },
    Actor { overlay_index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaOverlayMeshVertex {
    pub pos: [f32; 2],
    pub z: f32,
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaOverlayModelDraw {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub zoom: [f32; 3],
    pub tint: [f32; 4],
    pub vert_align: f32,
    pub blend_add: bool,
    pub visible: bool,
}

impl SongLuaOverlayModelDraw {
    #[must_use]
    pub const fn new(
        pos: [f32; 3],
        rot: [f32; 3],
        zoom: [f32; 3],
        tint: [f32; 4],
        vert_align: f32,
        blend_add: bool,
        visible: bool,
    ) -> Self {
        Self {
            pos,
            rot,
            zoom,
            tint,
            vert_align,
            blend_add,
            visible,
        }
    }
}

#[derive(Debug)]
pub struct SongLuaOverlayModelLayer<Vertex> {
    pub material: Option<deadsync_noteskin::ModelMaterial>,
    pub texture_key: Arc<str>,
    /// Secondary material source; its frame clock is captured independently.
    pub additive: Option<Arc<str>>,
    pub vertices: Arc<[Vertex]>,
    pub model_size: [f32; 2],
    pub uv_scale: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_tex_shift: [f32; 2],
    pub uv_velocity: [f32; 2],
    pub uv_cycle_seconds: Option<f32>,
    pub texture_frames: Arc<[SongLuaModelTextureFrame]>,
    pub additive_frames: Arc<[SongLuaModelTextureFrame]>,
    pub texture_samples: Arc<[SongLuaTextureSample]>,
    pub draw: SongLuaOverlayModelDraw,
}

impl<Vertex> Clone for SongLuaOverlayModelLayer<Vertex> {
    fn clone(&self) -> Self {
        Self {
            material: self.material,
            texture_key: Arc::clone(&self.texture_key),
            additive: self.additive.clone(),
            vertices: Arc::clone(&self.vertices),
            model_size: self.model_size,
            uv_scale: self.uv_scale,
            uv_offset: self.uv_offset,
            uv_tex_shift: self.uv_tex_shift,
            uv_velocity: self.uv_velocity,
            uv_cycle_seconds: self.uv_cycle_seconds,
            texture_frames: Arc::clone(&self.texture_frames),
            additive_frames: Arc::clone(&self.additive_frames),
            texture_samples: Arc::clone(&self.texture_samples),
            draw: self.draw,
        }
    }
}

impl<Vertex> SongLuaOverlayModelLayer<Vertex> {
    #[must_use]
    pub fn texture_key_at(&self, state: usize) -> &Arc<str> {
        self.texture_frames
            .get(state)
            .and_then(|frame| frame.texture_key.as_ref())
            .unwrap_or(&self.texture_key)
    }

    #[must_use]
    pub fn additive_key_at(&self, state: usize) -> Option<&Arc<str>> {
        self.additive_frames
            .get(state)
            .and_then(|frame| frame.texture_key.as_ref())
            .or(self.additive.as_ref())
    }

    pub fn new(
        texture_key: Arc<str>,
        vertices: Arc<[Vertex]>,
        model_size: [f32; 2],
        uv_scale: [f32; 2],
        uv_offset: [f32; 2],
        uv_tex_shift: [f32; 2],
        uv_velocity: [f32; 2],
        uv_cycle_seconds: Option<f32>,
        draw: SongLuaOverlayModelDraw,
    ) -> Self {
        Self {
            texture_key,
            additive: None,
            material: None,
            vertices,
            model_size,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            uv_velocity,
            uv_cycle_seconds,
            texture_frames: Arc::from([]),
            additive_frames: Arc::from([]),
            texture_samples: Arc::from([]),
            draw,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute> {
    Actor,
    ActorFrame,
    /// Independently animated outer state added by Actor:AddWrapperState.
    WrapperState,
    UpdateTracks {
        tracks: Vec<SongLuaOverlayRuntimeUpdateTrack>,
        draw_frames: Arc<[SongLuaDrawFrame]>,
    },
    ActorFrameTexture {
        capture_name: String,
        alpha_buffer: bool,
        depth_buffer: bool,
        float_buffer: bool,
        /// Integer viewport fixed by the successful Create call.
        capture_size: Option<[f32; 2]>,
    },
    ActorProxy {
        target: SongLuaProxyTarget,
    },
    AftSprite {
        capture_name: String,
    },
    Sprite {
        texture_path: PathBuf,
        texture_key: Arc<str>,
        states: Arc<[SongLuaSpriteState]>,
        textures: Arc<[SongLuaSpriteTexture]>,
    },
    Sound {
        sound_path: PathBuf,
    },
    BitmapText {
        font_name: &'static str,
        font_path: PathBuf,
        text: Arc<str>,
        // Sorted clock/string pairs baked at load; never grown during playback.
        text_changes: Arc<[(f32, Arc<str>)]>,
        text_changes_in_seconds: bool,
        stroke_color: Option<[f32; 4]>,
        attributes: Arc<[TextAttribute]>,
    },
    ActorMultiVertex {
        vertices: Arc<[SongLuaOverlayMeshVertex]>,
        texture_path: Option<PathBuf>,
        texture_key: Option<Arc<str>>,
        capture_name: Option<String>,
    },
    Model {
        layers: Arc<[SongLuaOverlayModelLayer<ModelVertex>]>,
    },
    NoteskinActor {
        slots: Arc<[NoteskinSlot]>,
        texture_samples: Arc<[Arc<[SongLuaTextureSample]>]>,
    },
    SongMeterDisplay {
        stream_width: f32,
        stream_state: SongLuaOverlayState,
        music_length_seconds: f32,
    },
    GraphDisplay {
        size: [f32; 2],
        body_values: Arc<[f32]>,
        body_state: SongLuaOverlayState,
        line_state: Box<SongLuaOverlayState>,
    },
    Quad,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaSpriteState {
    pub frame: u32,
    pub delay: f32,
}

/// Immutable, song-owned bindings recorded during load-time Lua replay. The
/// recorded event count bounds storage; playback selects in O(log n) without
/// I/O, allocation, growth or eviction. All images are prewarmed at the gameplay
/// transition. Bindings are shared read-only and freed with the song at the
/// next transition; the semantic audit checks recorded loads against native Lua.
#[derive(Debug, Clone)]
pub struct SongLuaSpriteTexture {
    pub second: f32,
    pub path: PathBuf,
    pub key: Arc<str>,
    pub states: Arc<[SongLuaSpriteState]>,
    pub animation_epoch: Option<f32>,
    pub frame_sheet: Option<(u32, u32)>,
}

pub fn sprite_texture_at(
    textures: &[SongLuaSpriteTexture],
    second: f32,
) -> Option<&SongLuaSpriteTexture> {
    let end = textures.partition_point(|texture| texture.second <= second);
    end.checked_sub(1).map(|index| &textures[index])
}

/// Select precompiled text without formatting or allocating on a song frame.
pub fn overlay_text_at<'a>(
    initial: &'a Arc<str>,
    changes: &'a [(f32, Arc<str>)],
    time: f32,
) -> &'a Arc<str> {
    let end = changes.partition_point(|(start, _)| *start <= time);
    end.checked_sub(1)
        .map_or(initial, |index| &changes[index].1)
}

#[must_use]
pub const fn parse_overlay_blend_mode(raw: &str) -> Option<SongLuaOverlayBlendMode> {
    if raw.eq_ignore_ascii_case("noeffect") || raw.eq_ignore_ascii_case("blendmode_noeffect") {
        Some(SongLuaOverlayBlendMode::NoEffect)
    } else if raw.eq_ignore_ascii_case("add") || raw.eq_ignore_ascii_case("blendmode_add") {
        Some(SongLuaOverlayBlendMode::Add)
    } else if raw.eq_ignore_ascii_case("multiply") || raw.eq_ignore_ascii_case("blendmode_multiply")
    {
        Some(SongLuaOverlayBlendMode::Multiply)
    } else if raw.eq_ignore_ascii_case("subtract") || raw.eq_ignore_ascii_case("blendmode_subtract")
    {
        Some(SongLuaOverlayBlendMode::Subtract)
    } else if raw.eq_ignore_ascii_case("alpha")
        || raw.eq_ignore_ascii_case("normal")
        || raw.eq_ignore_ascii_case("blendmode_normal")
    {
        Some(SongLuaOverlayBlendMode::Alpha)
    } else {
        None
    }
}

const OVERLAY_PARSE_KEY_STACK_BYTES: usize = 32;

#[inline]
fn with_ascii_lowercase_overlay_key<T>(raw: &str, use_key: impl FnOnce(&str) -> T) -> T {
    let has_uppercase = if raw.len() <= OVERLAY_PARSE_KEY_STACK_BYTES {
        raw.bytes().any(|byte| byte.is_ascii_uppercase())
    } else {
        // Title-case names can use the existing fallback immediately. For
        // long folded names, a full reduction lets LLVM scan bytes in bulk.
        raw.as_bytes()[0].is_ascii_uppercase()
            || raw.bytes().fold(false, |has_uppercase, byte| {
                has_uppercase | byte.is_ascii_uppercase()
            })
    };
    if !has_uppercase {
        return use_key(raw);
    }
    if raw.len() <= OVERLAY_PARSE_KEY_STACK_BYTES {
        let mut key = [0u8; OVERLAY_PARSE_KEY_STACK_BYTES];
        key[..raw.len()].copy_from_slice(raw.as_bytes());
        key[..raw.len()].make_ascii_lowercase();
        let key = std::str::from_utf8(&key[..raw.len()])
            .expect("ASCII case folding preserves valid UTF-8");
        use_key(key)
    } else {
        let key = raw.to_ascii_lowercase();
        use_key(&key)
    }
}

#[must_use]
pub fn parse_overlay_effect_mode(raw: &str) -> Option<EffectMode> {
    with_ascii_lowercase_overlay_key(raw.trim(), parse_overlay_effect_mode_normalized)
}

fn parse_overlay_effect_mode_normalized(key: &str) -> Option<EffectMode> {
    match key {
        "none" => Some(EffectMode::None),
        "diffuseblink" => Some(EffectMode::DiffuseBlink),
        "diffuseramp" => Some(EffectMode::DiffuseRamp),
        "diffuseshift" => Some(EffectMode::DiffuseShift),
        "glowblink" => Some(EffectMode::GlowBlink),
        "glowramp" => Some(EffectMode::GlowRamp),
        "glowshift" => Some(EffectMode::GlowShift),
        "pulse" => Some(EffectMode::Pulse),
        "bob" => Some(EffectMode::Bob),
        "bounce" => Some(EffectMode::Bounce),
        "wag" => Some(EffectMode::Wag),
        "spin" => Some(EffectMode::Spin),
        _ => None,
    }
}

#[must_use]
pub fn parse_overlay_effect_clock(raw: &str) -> Option<EffectClock> {
    let raw = raw.trim().trim_matches('"').trim_matches('\'');
    with_ascii_lowercase_overlay_key(raw, parse_overlay_effect_clock_normalized)
}

fn parse_overlay_effect_clock_normalized(key: &str) -> Option<EffectClock> {
    match key {
        "beat" | "beatnooffset" | "bgm" => Some(EffectClock::Beat),
        "timer" | "timerglobal" | "music" | "musicnooffset" | "time" | "seconds" => {
            Some(EffectClock::Time)
        }
        _ if key.contains("beat") => Some(EffectClock::Beat),
        _ if !key.is_empty() => Some(EffectClock::Time),
        _ => None,
    }
}

#[must_use]
pub fn parse_overlay_text_align(raw: &str) -> Option<TextAlign> {
    let raw = raw.trim().trim_matches('"').trim_matches('\'');
    with_ascii_lowercase_overlay_key(raw, parse_overlay_text_align_normalized)
}

fn parse_overlay_text_align_normalized(key: &str) -> Option<TextAlign> {
    match key {
        "left" | "horizalign_left" => Some(TextAlign::Left),
        "center" | "middle" | "horizalign_center" | "horizalign_middle" => Some(TextAlign::Center),
        "right" | "horizalign_right" => Some(TextAlign::Right),
        _ => None,
    }
}

#[must_use]
pub fn parse_overlay_text_glow_mode(raw: &str) -> Option<SongLuaTextGlowMode> {
    let raw = raw.trim().trim_matches('"').trim_matches('\'');
    with_ascii_lowercase_overlay_key(raw, parse_overlay_text_glow_mode_normalized)
}

fn parse_overlay_text_glow_mode_normalized(key: &str) -> Option<SongLuaTextGlowMode> {
    match key {
        "inner" | "textglowmode_inner" => Some(SongLuaTextGlowMode::Inner),
        "stroke" | "textglowmode_stroke" => Some(SongLuaTextGlowMode::Stroke),
        "both" | "textglowmode_both" => Some(SongLuaTextGlowMode::Both),
        _ => None,
    }
}

#[must_use]
pub const fn input_status_actor_text(actor_type: &str) -> Option<&'static str> {
    if actor_type.eq_ignore_ascii_case("DeviceList") {
        Some("No input devices")
    } else if actor_type.eq_ignore_ascii_case("InputList") {
        Some("No unmapped inputs")
    } else {
        None
    }
}

#[inline(always)]
#[must_use]
pub const fn effect_clock_label(clock: EffectClock) -> &'static str {
    match clock {
        EffectClock::Time => "time",
        EffectClock::Beat => "beat",
    }
}

pub(crate) const fn effect_mode_label(mode: EffectMode) -> &'static str {
    match mode {
        EffectMode::None => "none",
        EffectMode::DiffuseBlink => "diffuseblink",
        EffectMode::DiffuseRamp => "diffuseramp",
        EffectMode::DiffuseShift => "diffuseshift",
        EffectMode::GlowBlink => "glowblink",
        EffectMode::GlowRamp => "glowramp",
        EffectMode::GlowShift => "glowshift",
        EffectMode::Pulse => "pulse",
        EffectMode::Bob => "bob",
        EffectMode::Bounce => "bounce",
        EffectMode::Wag => "wag",
        EffectMode::Spin => "spin",
    }
}

#[inline(always)]
#[must_use]
pub const fn text_glow_mode_label(mode: SongLuaTextGlowMode) -> &'static str {
    match mode {
        SongLuaTextGlowMode::Inner => "inner",
        SongLuaTextGlowMode::Stroke => "stroke",
        SongLuaTextGlowMode::Both => "both",
    }
}

#[inline(always)]
#[must_use]
pub fn song_lua_valid_sprite_state_index(index: Option<u32>) -> Option<u32> {
    index.filter(|&value| value != SONG_LUA_SPRITE_STATE_CLEAR)
}

#[inline(always)]
#[must_use]
pub fn sprite_sheet_rect(index: u32, cols: u32, rows: u32) -> [f32; 4] {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let col = index % cols;
    let row = (index / cols).min(rows.saturating_sub(1));
    let width = 1.0 / cols as f32;
    let height = 1.0 / rows as f32;
    let left = col as f32 * width;
    let top = row as f32 * height;
    [left, top, left + width, top + height]
}

#[must_use]
pub fn sprite_texture_rect(
    custom_rect: Option<[f32; 4]>,
    state_index: Option<u32>,
    sheet_dims: Option<(u32, u32)>,
) -> [f32; 4] {
    if let Some(rect) = custom_rect {
        return rect;
    }
    if let Some(state_index) = song_lua_valid_sprite_state_index(state_index)
        && let Some((cols, rows)) = sheet_dims
    {
        return sprite_sheet_rect(state_index, cols, rows);
    }
    [0.0, 0.0, 1.0, 1.0]
}

#[must_use]
pub fn sprite_texture_rect_with_offset(
    custom_rect: Option<[f32; 4]>,
    state_index: Option<u32>,
    sheet_dims: Option<(u32, u32)>,
    texcoord_offset: Option<[f32; 2]>,
) -> Option<[f32; 4]> {
    let mut rect = custom_rect.or_else(|| {
        let state_index = song_lua_valid_sprite_state_index(state_index)?;
        let (cols, rows) = sheet_dims?;
        Some(sprite_sheet_rect(state_index, cols, rows))
    });
    if rect.is_none() && texcoord_offset.is_some() {
        rect = Some([0.0, 0.0, 1.0, 1.0]);
    }
    if let (Some(base), Some(offset)) = (rect, texcoord_offset) {
        return Some(offset_texture_rect(base, offset));
    }
    rect
}

#[must_use]
pub fn sprite_frame_count(sheet_dims: Option<(u32, u32)>) -> u32 {
    let Some((cols, rows)) = sheet_dims else {
        return 1;
    };
    cols.max(1).saturating_mul(rows.max(1)).max(1)
}

#[must_use]
pub fn sprite_image_frame_size(
    texture_size: Option<(f32, f32)>,
    _animate: bool,
    _state_index: Option<u32>,
    sheet_dims: Option<(u32, u32)>,
) -> Option<(f32, f32)> {
    let (mut width, mut height) = texture_size?;
    if let Some((cols, rows)) = sheet_dims {
        width /= cols.max(1) as f32;
        height /= rows.max(1) as f32;
    }
    Some((width, height))
}

fn borrowed_alignment_token(raw: &str) -> &str {
    let mut token = raw.trim().trim_matches('"').trim_matches('\'');
    // Match the original repeated-prefix stripping order without case-folding
    // the whole string. Lua and numeric-string conversion semantics stay intact.
    for prefix in ["horizalign_", "vertalign_"] {
        while token
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        {
            token = &token[prefix.len()..];
        }
    }
    token
}

#[must_use]
pub fn song_lua_align_token(raw: &str) -> String {
    raw.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase()
        .trim_start_matches("horizalign_")
        .trim_start_matches("vertalign_")
        .to_string()
}

#[must_use]
pub const fn overlay_text_align_label(value: TextAlign) -> &'static str {
    match value {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
    }
}

#[must_use]
pub fn crop_texture_rect(source: [f32; 2], target: [f32; 2]) -> Option<[f32; 4]> {
    if !source.iter().all(|value| value.is_finite() && *value > 0.0) {
        return None;
    }
    let scale = (target[0] / source[0]).max(target[1] / source[1]);
    if !scale.is_finite() || scale <= f32::EPSILON {
        return None;
    }
    let zoomed = [source[0] * scale, source[1] * scale];
    if zoomed[0] > target[0] + 0.01 {
        let cut = ((zoomed[0] - target[0]) / zoomed[0]).max(0.0) * 0.5;
        return Some([cut, 0.0, 1.0 - cut, 1.0]);
    }
    let cut = ((zoomed[1] - target[1]) / zoomed[1]).max(0.0) * 0.5;
    Some([0.0, cut, 1.0, 1.0 - cut])
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaScaleToRectPlan {
    pub pos: [f32; 2],
    pub zoom: f32,
    pub flip_x: bool,
    pub flip_y: bool,
}

#[must_use]
pub fn scale_to_rect_plan(
    rect: [f32; 4],
    base_size: [f32; 2],
    align: [f32; 2],
    cover: bool,
) -> Option<SongLuaScaleToRectPlan> {
    let width = rect[2] - rect[0];
    let height = rect[3] - rect[1];
    if base_size[0].abs() <= f32::EPSILON || base_size[1].abs() <= f32::EPSILON {
        return None;
    }
    let zoom_x = (width / base_size[0]).abs();
    let zoom_y = (height / base_size[1]).abs();
    let zoom = if cover {
        zoom_x.max(zoom_y)
    } else {
        zoom_x.min(zoom_y)
    };
    if !zoom.is_finite() {
        return None;
    }
    Some(SongLuaScaleToRectPlan {
        pos: [
            width.mul_add(align[0], rect[0]),
            height.mul_add(align[1], rect[1]),
        ],
        zoom,
        flip_x: width < 0.0,
        flip_y: height < 0.0,
    })
}

#[inline(always)]
#[must_use]
pub fn offset_texture_rect(rect: [f32; 4], offset: [f32; 2]) -> [f32; 4] {
    [
        rect[0] + offset[0],
        rect[1] + offset[1],
        rect[2] + offset[0],
        rect[3] + offset[1],
    ]
}

#[must_use]
pub fn texture_pixel_offset_rect(
    rect: [f32; 4],
    texture_size: [f32; 2],
    offset: [f32; 2],
) -> Option<[f32; 4]> {
    if texture_size[0] <= f32::EPSILON || texture_size[1] <= f32::EPSILON {
        return None;
    }
    Some(offset_texture_rect(
        rect,
        [offset[0] / texture_size[0], offset[1] / texture_size[1]],
    ))
}

#[must_use]
pub fn sprite_animation_state_at(seconds: f32, delay: f32, frame_count: u32) -> u32 {
    let delay = delay.max(0.0);
    let frame_count = frame_count.max(1);
    if delay <= f32::EPSILON {
        0
    } else {
        ((seconds.max(0.0) / delay).floor() as u32) % frame_count
    }
}

#[must_use]
pub fn sprite_animation_state_from(
    start: u32,
    seconds: f32,
    playback_rate: f32,
    delay: f32,
    frame_count: u32,
    loops: bool,
) -> u32 {
    let frame_count = frame_count.max(1);
    if delay <= f32::EPSILON || frame_count <= 1 {
        return start.min(frame_count - 1);
    }
    let steps = (seconds * playback_rate / delay).floor() as i64;
    let frame = i64::from(start) + steps;
    let frame_count = i64::from(frame_count);
    if loops {
        frame.rem_euclid(frame_count) as u32
    } else {
        frame.clamp(0, frame_count - 1) as u32
    }
}

#[must_use]
pub fn sprite_custom_animation_state_from(
    states: &[SongLuaSpriteState],
    start: u32,
    seconds: f32,
    playback_rate: f32,
    loops: bool,
) -> u32 {
    let len = states.len();
    if len <= 1 {
        return 0;
    }
    let start = (start as usize).min(len - 1);
    let elapsed = seconds * playback_rate;
    if states.iter().all(|state| state.delay > 0.000_1) {
        let cycle = states.iter().map(|state| state.delay).sum::<f32>();
        let start_time = states[..start].iter().map(|state| state.delay).sum::<f32>();
        let time = if loops {
            (start_time + elapsed).rem_euclid(cycle)
        } else {
            (start_time + elapsed).clamp(0.0, cycle - f32::EPSILON)
        };
        let mut end = 0.0;
        return states
            .iter()
            .position(|state| {
                end += state.delay;
                time < end
            })
            .unwrap_or(len - 1) as u32;
    }
    if elapsed <= 0.0 {
        return start as u32;
    }
    let mut remaining = elapsed;
    let mut index = start;
    for _ in 0..len {
        let delay = states[index].delay;
        if delay <= 0.000_1 || remaining < delay {
            return index as u32;
        }
        remaining -= delay;
        if index + 1 == len {
            if !loops {
                return index as u32;
            }
            index = 0;
        } else {
            index += 1;
        }
    }
    index as u32
}

#[inline(always)]
#[must_use]
pub fn song_lua_span_end(start: f32, limit: f32, span_mode: SongLuaSpanMode) -> f32 {
    match span_mode {
        SongLuaSpanMode::Len => start + limit.max(0.0),
        SongLuaSpanMode::End => limit,
    }
}

#[must_use]
pub const fn rolling_numbers_format(metric: &str) -> &'static str {
    if metric.eq_ignore_ascii_case("RollingNumbersEvaluationB") {
        "%03.0f"
    } else if metric.eq_ignore_ascii_case("RollingNumbersEvaluationA")
        || metric.eq_ignore_ascii_case("RollingNumbersEvaluationNoDecentsWayOffs")
        || metric.eq_ignore_ascii_case("RollingNumbersEvaluation")
    {
        "%04.0f"
    } else {
        "%.0f"
    }
}

#[must_use]
pub fn format_rolling_number(format: &str, number: f32) -> String {
    let rounded = number.round().clamp(i64::MIN as f32, i64::MAX as f32) as i64;
    if format.contains("%04") {
        format!("{rounded:04}")
    } else if format.contains("%03") {
        format!("{rounded:03}")
    } else if format.contains("%.2") {
        format!("{number:.2}")
    } else {
        rounded.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaOverlayState {
    pub aux: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub z_bias: f32,
    pub draw_order: i32,
    pub draw_by_z_position: bool,
    pub aft_created: bool,
    pub aft_preserve: bool,
    pub halign: f32,
    pub valign: f32,
    pub text_align: TextAlign,
    pub uppercase: bool,
    pub shadow_len: [f32; 2],
    pub shadow_color: [f32; 4],
    pub glow: [f32; 4],
    pub fov: Option<f32>,
    pub vanishpoint: Option<[f32; 2]>,
    pub diffuse: [f32; 4],
    pub vertex_colors: Option<[[f32; 4]; 4]>,
    pub visible: bool,
    pub hibernating: bool,
    pub cropleft: f32,
    pub cropright: f32,
    pub croptop: f32,
    pub cropbottom: f32,
    pub fadeleft: f32,
    pub faderight: f32,
    pub fadetop: f32,
    pub fadebottom: f32,
    pub mask_source: bool,
    pub mask_dest: bool,
    pub depth_test: bool,
    pub depth_write: bool,
    pub depth_test_on_fail: bool,
    pub cull_mode: deadlib_render_core::CullMode,
    pub zoom: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub zoom_z: f32,
    pub basezoom: f32,
    pub basezoom_x: f32,
    pub basezoom_y: f32,
    pub basezoom_z: f32,
    /// Ancestor and local scales retained in native matrix multiplication order.
    /// Present only when composition can preserve unrotated ancestor scaling.
    pub scale_factors: Option<[[f32; 3]; 2]>,
    pub rot_x_deg: f32,
    pub rot_y_deg: f32,
    pub rot_z_deg: f32,
    pub base_rotation: [f32; 3],
    /// Actor::TweenState quaternion, accumulated by heading/pitch/roll.
    pub quaternion: [f32; 4],
    pub base_alpha: f32,
    pub skew_x: f32,
    pub skew_y: f32,
    pub blend: SongLuaOverlayBlendMode,
    pub vibrate: bool,
    pub effect_magnitude: [f32; 3],
    /// Vibration contributed by composed ancestor ActorFrames.
    pub inherited_vibrate: [f32; 3],
    pub effect_clock: EffectClock,
    pub effect_timer: bool,
    pub effect_no_offset: bool,
    pub effect_mode: EffectMode,
    pub effect_color1: [f32; 4],
    pub effect_color2: [f32; 4],
    pub effect_period: f32,
    pub effect_offset: f32,
    /// Native [source clock, actor clock] sample; avoids subtracting large phases.
    pub effect_time: Option<[f32; 2]>,
    pub effect_timing: Option<[f32; 5]>,
    pub rainbow: bool,
    pub rainbow_scroll: bool,
    pub text_jitter: bool,
    pub text_distortion: f32,
    pub text_glow_mode: SongLuaTextGlowMode,
    pub mult_attrs_with_diffuse: bool,
    // Chronological compilation has already accumulated native spin rotation.
    pub spin_baked: bool,
    // A delayed first binding must not draw its preloaded texture early.
    pub sprite_texture: bool,
    pub sprite_animate: bool,
    pub sprite_loop: bool,
    pub sprite_playback_rate: f32,
    pub sprite_state_delay: f32,
    pub sprite_state_index: Option<u32>,
    pub sprite_animation_epoch: Option<f32>,
    pub decode_movie: bool,
    pub vert_spacing: Option<i32>,
    pub wrap_width_pixels: Option<i32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub max_w_pre_zoom: bool,
    pub max_h_pre_zoom: bool,
    pub max_dimension_uses_zoom: bool,
    pub texture_filtering: bool,
    pub texture_wrapping: bool,
    pub texcoord_offset: Option<[f32; 2]>,
    pub custom_texture_rect: Option<[f32; 4]>,
    pub texcoord_velocity: Option<[f32; 2]>,
    pub size: Option<[f32; 2]>,
    pub stretch_rect: Option<[f32; 4]>,
}

impl SongLuaOverlayState {
    #[must_use]
    pub const fn draw_visible(self) -> bool {
        self.visible && !self.hibernating
    }
}

impl Default for SongLuaOverlayState {
    fn default() -> Self {
        Self {
            aux: 0.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            z_bias: 0.0,
            draw_order: 0,
            draw_by_z_position: false,
            aft_created: false,
            aft_preserve: false,
            halign: 0.5,
            valign: 0.5,
            text_align: TextAlign::Center,
            uppercase: false,
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0, 0.0, 0.0, 0.5],
            glow: [1.0, 1.0, 1.0, 0.0],
            fov: None,
            vanishpoint: None,
            diffuse: [1.0, 1.0, 1.0, 1.0],
            vertex_colors: None,
            visible: true,
            hibernating: false,
            cropleft: 0.0,
            cropright: 0.0,
            croptop: 0.0,
            cropbottom: 0.0,
            fadeleft: 0.0,
            faderight: 0.0,
            fadetop: 0.0,
            fadebottom: 0.0,
            mask_source: false,
            mask_dest: false,
            depth_test: false,
            depth_write: false,
            depth_test_on_fail: false,
            cull_mode: deadlib_render_core::CullMode::None,
            zoom: 1.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            zoom_z: 1.0,
            basezoom: 1.0,
            basezoom_x: 1.0,
            basezoom_y: 1.0,
            basezoom_z: 1.0,
            scale_factors: None,
            rot_x_deg: 0.0,
            rot_y_deg: 0.0,
            rot_z_deg: 0.0,
            base_rotation: [0.0; 3],
            quaternion: [0.0, 0.0, 0.0, 1.0],
            base_alpha: 1.0,
            skew_x: 0.0,
            skew_y: 0.0,
            blend: SongLuaOverlayBlendMode::Alpha,
            vibrate: false,
            effect_magnitude: [0.0, 0.0, 0.0],
            inherited_vibrate: [0.0, 0.0, 0.0],
            effect_clock: EffectClock::Time,
            effect_timer: true,
            effect_no_offset: false,
            effect_mode: EffectMode::None,
            effect_color1: [1.0, 1.0, 1.0, 1.0],
            effect_color2: [1.0, 1.0, 1.0, 1.0],
            effect_period: 1.0,
            effect_offset: 0.0,
            effect_time: None,
            effect_timing: None,
            rainbow: false,
            rainbow_scroll: false,
            text_jitter: false,
            text_distortion: 0.0,
            text_glow_mode: SongLuaTextGlowMode::Both,
            mult_attrs_with_diffuse: false,
            spin_baked: false,
            sprite_texture: true,
            sprite_animate: false,
            sprite_loop: true,
            sprite_playback_rate: 1.0,
            sprite_state_delay: 0.1,
            sprite_state_index: None,
            sprite_animation_epoch: None,
            decode_movie: false,
            vert_spacing: None,
            wrap_width_pixels: None,
            max_width: None,
            max_height: None,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            max_dimension_uses_zoom: false,
            texture_filtering: true,
            texture_wrapping: false,
            texcoord_offset: None,
            custom_texture_rect: None,
            texcoord_velocity: None,
            size: None,
            stretch_rect: None,
        }
    }
}

#[must_use]
pub fn overlay_state_uses_repeat_sampler(state: &SongLuaOverlayState) -> bool {
    state.texture_wrapping
        || state
            .texcoord_offset
            .is_some_and(|[u, v]| u.abs() > f32::EPSILON || v.abs() > f32::EPSILON)
        || state
            .custom_texture_rect
            .is_some_and(|[u0, v0, u1, v1]| u0 < 0.0 || v0 < 0.0 || u1 > 1.0 || v1 > 1.0)
        || state.texcoord_velocity.is_some()
}

#[must_use]
pub const fn overlay_state_uses_nearest_sampler(state: &SongLuaOverlayState) -> bool {
    !state.texture_filtering
}

#[must_use]
pub fn overlay_state_axis_scale(state: SongLuaOverlayState) -> [f32; 2] {
    let basezoom_x = if (state.basezoom_x - 1.0).abs() <= f32::EPSILON {
        state.basezoom
    } else {
        state.basezoom_x
    };
    let basezoom_y = if (state.basezoom_y - 1.0).abs() <= f32::EPSILON {
        state.basezoom
    } else {
        state.basezoom_y
    };
    let zoom_x = if (state.zoom_x - 1.0).abs() <= f32::EPSILON {
        state.zoom
    } else {
        state.zoom_x
    };
    let zoom_y = if (state.zoom_y - 1.0).abs() <= f32::EPSILON {
        state.zoom
    } else {
        state.zoom_y
    };
    [basezoom_x * zoom_x, basezoom_y * zoom_y]
}

#[must_use]
pub fn overlay_state_z_scale(state: SongLuaOverlayState) -> f32 {
    let basezoom_z = if (state.basezoom_z - 1.0).abs() <= f32::EPSILON {
        state.basezoom
    } else {
        state.basezoom_z
    };
    let zoom_z = if (state.zoom_z - 1.0).abs() <= f32::EPSILON {
        state.zoom
    } else {
        state.zoom_z
    };
    basezoom_z * zoom_z
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SongLuaOverlayStateDelta {
    pub aux: Option<f32>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub z: Option<f32>,
    pub z_bias: Option<f32>,
    pub draw_order: Option<i32>,
    pub draw_by_z_position: Option<bool>,
    pub aft_created: Option<bool>,
    pub aft_preserve: Option<bool>,
    pub halign: Option<f32>,
    pub valign: Option<f32>,
    pub text_align: Option<TextAlign>,
    pub uppercase: Option<bool>,
    pub shadow_len: Option<[f32; 2]>,
    pub shadow_color: Option<[f32; 4]>,
    pub glow: Option<[f32; 4]>,
    pub fov: Option<f32>,
    pub vanishpoint: Option<[f32; 2]>,
    pub diffuse: Option<[f32; 4]>,
    pub vertex_colors: Option<[[f32; 4]; 4]>,
    pub visible: Option<bool>,
    pub hibernating: Option<bool>,
    pub cropleft: Option<f32>,
    pub cropright: Option<f32>,
    pub croptop: Option<f32>,
    pub cropbottom: Option<f32>,
    pub fadeleft: Option<f32>,
    pub faderight: Option<f32>,
    pub fadetop: Option<f32>,
    pub fadebottom: Option<f32>,
    pub mask_source: Option<bool>,
    pub mask_dest: Option<bool>,
    pub depth_test: Option<bool>,
    pub depth_write: Option<bool>,
    pub depth_test_on_fail: Option<bool>,
    pub cull_mode: Option<deadlib_render_core::CullMode>,
    pub zoom: Option<f32>,
    pub zoom_x: Option<f32>,
    pub zoom_y: Option<f32>,
    pub zoom_z: Option<f32>,
    pub basezoom: Option<f32>,
    pub basezoom_x: Option<f32>,
    pub basezoom_y: Option<f32>,
    pub basezoom_z: Option<f32>,
    pub rot_x_deg: Option<f32>,
    pub rot_y_deg: Option<f32>,
    pub rot_z_deg: Option<f32>,
    pub base_rotation: Option<[f32; 3]>,
    pub quaternion: Option<[f32; 4]>,
    pub base_alpha: Option<f32>,
    pub skew_x: Option<f32>,
    pub skew_y: Option<f32>,
    pub blend: Option<SongLuaOverlayBlendMode>,
    pub vibrate: Option<bool>,
    pub effect_magnitude: Option<[f32; 3]>,
    pub effect_clock: Option<EffectClock>,
    pub effect_timer: Option<bool>,
    pub effect_no_offset: Option<bool>,
    pub effect_mode: Option<EffectMode>,
    pub effect_color1: Option<[f32; 4]>,
    pub effect_color2: Option<[f32; 4]>,
    pub effect_period: Option<f32>,
    pub effect_offset: Option<f32>,
    pub effect_timing: Option<[f32; 5]>,
    pub rainbow: Option<bool>,
    pub rainbow_scroll: Option<bool>,
    pub text_jitter: Option<bool>,
    pub text_distortion: Option<f32>,
    pub text_glow_mode: Option<SongLuaTextGlowMode>,
    pub mult_attrs_with_diffuse: Option<bool>,
    pub sprite_texture: Option<bool>,
    pub sprite_animate: Option<bool>,
    pub sprite_loop: Option<bool>,
    pub sprite_playback_rate: Option<f32>,
    pub sprite_state_delay: Option<f32>,
    pub sprite_state_index: Option<u32>,
    pub vert_spacing: Option<i32>,
    pub wrap_width_pixels: Option<i32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub max_w_pre_zoom: Option<bool>,
    pub max_h_pre_zoom: Option<bool>,
    pub max_dimension_uses_zoom: Option<bool>,
    pub texture_filtering: Option<bool>,
    pub texture_wrapping: Option<bool>,
    pub texcoord_offset: Option<[f32; 2]>,
    pub custom_texture_rect: Option<[f32; 4]>,
    pub texcoord_velocity: Option<[f32; 2]>,
    pub size: Option<[f32; 2]>,
    pub stretch_rect: Option<[f32; 4]>,
    pub sound_play: Option<bool>,
}

impl SongLuaOverlayStateDelta {
    #[must_use]
    pub const fn has_update_target(&self, target: SongLuaOverlayUpdateTarget) -> bool {
        use SongLuaOverlayUpdateTarget as Target;
        match target {
            Target::Aux => self.aux.is_some(),
            Target::X => self.x.is_some(),
            Target::Y => self.y.is_some(),
            Target::Z => self.z.is_some(),
            Target::ZBias => self.z_bias.is_some(),
            Target::DrawOrder => self.draw_order.is_some(),
            Target::DrawByZPosition => self.draw_by_z_position.is_some(),
            Target::AftPreserve => self.aft_preserve.is_some(),
            Target::AftCreated => self.aft_created.is_some(),
            Target::HAlign => self.halign.is_some(),
            Target::VAlign => self.valign.is_some(),
            Target::TextAlign => self.text_align.is_some(),
            Target::Uppercase => self.uppercase.is_some(),
            Target::ShadowLen => self.shadow_len.is_some(),
            Target::ShadowColor => self.shadow_color.is_some(),
            Target::Glow => self.glow.is_some(),
            Target::Fov => self.fov.is_some(),
            Target::Vanishpoint => self.vanishpoint.is_some(),
            Target::Diffuse => self.diffuse.is_some(),
            Target::VertexColors => self.vertex_colors.is_some(),
            Target::Visible => self.visible.is_some(),
            Target::Hibernating => self.hibernating.is_some(),
            Target::CropLeft => self.cropleft.is_some(),
            Target::CropRight => self.cropright.is_some(),
            Target::CropTop => self.croptop.is_some(),
            Target::CropBottom => self.cropbottom.is_some(),
            Target::FadeLeft => self.fadeleft.is_some(),
            Target::FadeRight => self.faderight.is_some(),
            Target::FadeTop => self.fadetop.is_some(),
            Target::FadeBottom => self.fadebottom.is_some(),
            Target::MaskSource => self.mask_source.is_some(),
            Target::MaskDest => self.mask_dest.is_some(),
            Target::DepthTest => self.depth_test.is_some(),
            Target::DepthWrite => self.depth_write.is_some(),
            Target::DepthTestOnFail => self.depth_test_on_fail.is_some(),
            Target::CullMode => self.cull_mode.is_some(),
            Target::Zoom => self.zoom.is_some(),
            Target::ZoomX => self.zoom_x.is_some(),
            Target::ZoomY => self.zoom_y.is_some(),
            Target::ZoomZ => self.zoom_z.is_some(),
            Target::BaseZoom => self.basezoom.is_some(),
            Target::BaseZoomX => self.basezoom_x.is_some(),
            Target::BaseZoomY => self.basezoom_y.is_some(),
            Target::BaseZoomZ => self.basezoom_z.is_some(),
            Target::RotationX => self.rot_x_deg.is_some(),
            Target::RotationY => self.rot_y_deg.is_some(),
            Target::RotationZ => self.rot_z_deg.is_some(),
            Target::BaseRotation => self.base_rotation.is_some(),
            Target::Quaternion => self.quaternion.is_some(),
            Target::BaseAlpha => self.base_alpha.is_some(),
            Target::SkewX => self.skew_x.is_some(),
            Target::SkewY => self.skew_y.is_some(),
            Target::Blend => self.blend.is_some(),
            Target::Vibrate => self.vibrate.is_some(),
            Target::EffectMagnitude => self.effect_magnitude.is_some(),
            Target::EffectClock => self.effect_clock.is_some(),
            Target::EffectTimer => self.effect_timer.is_some(),
            Target::EffectNoOffset => self.effect_no_offset.is_some(),
            Target::EffectMode => self.effect_mode.is_some(),
            Target::EffectColor1 => self.effect_color1.is_some(),
            Target::EffectColor2 => self.effect_color2.is_some(),
            Target::EffectPeriod => self.effect_period.is_some(),
            Target::EffectOffset => self.effect_offset.is_some(),
            Target::EffectTime => false,
            Target::EffectTiming => self.effect_timing.is_some(),
            Target::Rainbow => self.rainbow.is_some(),
            Target::RainbowScroll => self.rainbow_scroll.is_some(),
            Target::TextJitter => self.text_jitter.is_some(),
            Target::TextDistortion => self.text_distortion.is_some(),
            Target::TextGlowMode => self.text_glow_mode.is_some(),
            Target::MultAttrsWithDiffuse => self.mult_attrs_with_diffuse.is_some(),
            Target::SpriteTexture => self.sprite_texture.is_some(),
            Target::SpriteAnimate => self.sprite_animate.is_some(),
            Target::SpriteLoop => self.sprite_loop.is_some(),
            Target::SpritePlaybackRate => self.sprite_playback_rate.is_some(),
            Target::SpriteStateDelay => self.sprite_state_delay.is_some(),
            Target::SpriteStateIndex => self.sprite_state_index.is_some(),
            Target::VertSpacing => self.vert_spacing.is_some(),
            Target::WrapWidthPixels => self.wrap_width_pixels.is_some(),
            Target::MaxWidth => self.max_width.is_some(),
            Target::MaxHeight => self.max_height.is_some(),
            Target::MaxWPreZoom => self.max_w_pre_zoom.is_some(),
            Target::MaxHPreZoom => self.max_h_pre_zoom.is_some(),
            Target::MaxDimensionUsesZoom => self.max_dimension_uses_zoom.is_some(),
            Target::TextureFiltering => self.texture_filtering.is_some(),
            Target::TextureWrapping => self.texture_wrapping.is_some(),
            Target::TexcoordOffset => self.texcoord_offset.is_some(),
            Target::CustomTextureRect => self.custom_texture_rect.is_some(),
            Target::TexcoordVelocity => self.texcoord_velocity.is_some(),
            Target::Size => self.size.is_some(),
            Target::StretchRect => self.stretch_rect.is_some(),
        }
    }
}

#[must_use]
pub fn overlay_delta_uses_repeat_sampler(delta: &SongLuaOverlayStateDelta) -> bool {
    delta.texture_wrapping == Some(true)
        || delta
            .texcoord_offset
            .is_some_and(|[u, v]| u.abs() > f32::EPSILON || v.abs() > f32::EPSILON)
        || delta
            .custom_texture_rect
            .is_some_and(|[u0, v0, u1, v1]| u0 < 0.0 || v0 < 0.0 || u1 > 1.0 || v1 > 1.0)
        || delta.texcoord_velocity.is_some()
}

#[must_use]
pub fn overlay_delta_uses_nearest_sampler(delta: &SongLuaOverlayStateDelta) -> bool {
    delta.texture_filtering == Some(false)
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayCommandBlock {
    /// Immutable song-load samples [seconds, native time left]. Shared with
    /// cloned commands for the song lifetime; lookup is bounded by the song's
    /// 60 Hz frame count, allocates nothing, and never grows during gameplay.
    pub progress: Option<std::sync::Arc<[[f32; 2]]>>,
    /// Writes from queued commands are replayed at their actual dispatch frames.
    pub queued: bool,
    pub start: f32,
    pub duration: f32,
    pub easing: Option<String>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
    pub delta: SongLuaOverlayStateDelta,
}

#[must_use]
pub fn overlay_block_factor(block: &SongLuaOverlayCommandBlock, elapsed: f32) -> Option<f32> {
    if let Some(samples) = &block.progress {
        return tween_frame_factor(samples, block.duration, elapsed);
    }
    (elapsed >= block.start).then(|| {
        if block.duration <= f32::EPSILON {
            1.0
        } else {
            ((elapsed - block.start) / block.duration).clamp(0.0, 1.0)
        }
    })
}

pub(crate) fn tween_frame_factor(samples: &[[f32; 2]], duration: f32, elapsed: f32) -> Option<f32> {
    let right = samples.partition_point(|sample| sample[0] <= elapsed);
    let [time, left] = *samples.get(right.checked_sub(1)?)?;
    if duration == 0.0 {
        return Some(1.0);
    }
    let left = if let Some(next) = samples.get(right) {
        actor_lerp(left, next[1], (elapsed - time) / (next[0] - time))
    } else {
        // Preserve a partial tween beyond the sampled song horizon.
        (left - (elapsed - time)).max(0.0)
    };
    Some((1.0 - left / duration).clamp(0.0, 1.0))
}

#[must_use]
pub fn overlay_block_end(block: &SongLuaOverlayCommandBlock) -> f32 {
    block
        .progress
        .as_ref()
        .and_then(|samples| samples.last())
        .map_or(block.start + block.duration.max(0.0), |sample| {
            sample[0] + sample[1]
        })
}

/// Resolve native motion macro resets against the actor's state at dispatch.
pub fn motion_restart_epoch(
    mut state: SongLuaOverlayState,
    blocks: &[SongLuaOverlayCommandBlock],
    elapsed: f32,
    origin: f32,
) -> Option<f32> {
    let mut epoch = None;
    for block in blocks {
        if elapsed + f32::EPSILON < block.start {
            break;
        }
        if let Some(mode) = block.delta.effect_mode {
            let reset = match mode {
                EffectMode::Bob => state.effect_mode != mode || state.effect_period != 2.0,
                EffectMode::Wag => state.effect_mode != mode,
                EffectMode::Bounce => true,
                _ => false,
            };
            if reset {
                epoch = Some(origin + block.start);
            }
        }
        apply_overlay_delta(&mut state, &block.delta);
        if elapsed < block.start + block.duration.max(0.0) {
            break;
        }
    }
    epoch
}

fn overlay_command_out_bounce(t: f32) -> f32 {
    const N1: f32 = 7.5625;
    const D1: f32 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        let t = t - 1.5 / D1;
        (N1 * t).mul_add(t, 0.75)
    } else if t < 2.5 / D1 {
        let t = t - 2.25 / D1;
        (N1 * t).mul_add(t, 0.9375)
    } else {
        let t = t - 2.625 / D1;
        (N1 * t).mul_add(t, 0.984_375)
    }
}

const fn bezier_coeff(points: [f32; 4]) -> [f32; 4] {
    let c = 3.0 * (points[1] - points[0]);
    let b = 3.0 * (points[2] - points[1]) - c;
    [points[3] - points[0] - c - b, b, c, points[0]]
}

fn actor_bounce_factor(t: f32, end: bool) -> f32 {
    // _fallback/02 Actor.lua uses these 2D Beziers. Match RageBezier2D's
    // float Newton iteration and error threshold, including the overshoot.
    const BEGIN: [[f32; 4]; 2] = [
        bezier_coeff([0.0, 0.42, 2.0 / 3.0, 1.0]),
        bezier_coeff([0.0, -0.42, 0.3, 1.0]),
    ];
    const END: [[f32; 4]; 2] = [
        bezier_coeff([0.0, 1.0 / 3.0, 0.58, 1.0]),
        bezier_coeff([0.0, 0.7, 1.42, 1.0]),
    ];
    let [x, y] = if end { END } else { BEGIN };
    let evaluate = |v: [f32; 4], t: f32| ((v[0] * t + v[1]) * t + v[2]) * t + v[3];
    let mut parameter = t;
    for _ in 0..100 {
        let error = t - evaluate(x, parameter);
        if error.abs() < 0.0001 {
            break;
        }
        let slope = 3.0 * x[0] * parameter * parameter + 2.0 * x[1] * parameter + x[2];
        parameter += error / slope;
    }
    evaluate(y, parameter)
}

fn overlay_command_ease_factor(easing: Option<&str>, t: f32, opt1: Option<f32>) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if easing.is_some_and(|name| name.starts_with("bezier(")) {
        return deadsync_gameplay::song_lua_ease_factor(easing, t, opt1, None);
    }
    match easing.unwrap_or("linear") {
        "instant" => 1.0,
        "inQuad" => t * t,
        // NativeTween.cpp rounds the scalar multiply before subtraction.
        "outQuad" => 1.0 - (1.0 - t) * (1.0 - t),
        // ITGmania's fallback smooth(t) uses scalar Bezier {0, 0, 1, 1}.
        "smooth" => t * t * (3.0 - 2.0 * t),
        "inOutQuad" => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                (2.0 * (1.0 - t)).mul_add(-(1.0 - t), 1.0)
            }
        }
        "spring" => 1.0 - (t * std::f32::consts::PI * 2.5).cos() / (1.0 + t * 3.0),
        "outElastic" => {
            if t <= 0.0 || t >= 1.0 {
                t
            } else {
                let period = opt1
                    .filter(|value| value.is_finite() && *value > 0.0)
                    .unwrap_or(0.3);
                let tau = std::f32::consts::TAU / period;
                (-10.0 * t)
                    .exp2()
                    .mul_add(((t - period * 0.25) * tau).sin(), 1.0)
            }
        }
        "bouncebegin" => actor_bounce_factor(t, false),
        "bounceend" => actor_bounce_factor(t, true),
        "inBounce" => 1.0 - overlay_command_out_bounce(1.0 - t),
        "outBounce" => overlay_command_out_bounce(t),
        _ => t,
    }
}

#[must_use]
pub fn overlay_state_after_blocks<'a>(
    mut state: SongLuaOverlayState,
    blocks: impl IntoIterator<Item = &'a SongLuaOverlayCommandBlock>,
    elapsed: f32,
) -> SongLuaOverlayState {
    if !elapsed.is_finite() {
        return state;
    }
    for block in blocks {
        let Some(factor) = overlay_block_factor(block, elapsed) else {
            break;
        };
        if factor >= 1.0 {
            apply_overlay_delta(&mut state, &block.delta);
            continue;
        }
        let t = overlay_command_ease_factor(block.easing.as_deref(), factor, block.opt1);
        overlay_state_lerp(&mut state, &block.delta, t);
        return state;
    }
    state
}

/// Move an overlay and its cached stretch bounds together. Actor::StretchTo
/// sets position and scale; later x/y commands still move that geometry.
pub const fn move_overlay(state: &mut SongLuaOverlayState, x: f32, y: f32) {
    if let Some(rect) = &mut state.stretch_rect {
        let dx = x - state.x;
        let dy = y - state.y;
        rect[0] += dx;
        rect[2] += dx;
        rect[1] += dy;
        rect[3] += dy;
    }
    state.x = x;
    state.y = y;
}

/// Applies only the properties written by `delta`, preserving all other state.
///
/// Optional state fields become `Some` when written. Sound playback is a
/// separate event; `sound_play` does not modify visual state.
pub const fn apply_overlay_delta(
    state: &mut SongLuaOverlayState,
    delta: &SongLuaOverlayStateDelta,
) {
    if let Some(value) = delta.aux {
        state.aux = value;
    }
    let x = match delta.x {
        Some(x) => x,
        None => state.x,
    };
    let y = match delta.y {
        Some(y) => y,
        None => state.y,
    };
    move_overlay(state, x, y);
    if let Some(value) = delta.z {
        state.z = value;
    }
    if let Some(value) = delta.z_bias {
        state.z_bias = value;
    }
    if let Some(value) = delta.draw_order {
        state.draw_order = value;
    }
    if let Some(value) = delta.draw_by_z_position {
        state.draw_by_z_position = value;
    }
    if let Some(value) = delta.aft_preserve {
        state.aft_preserve = value;
    }
    if let Some(value) = delta.aft_created {
        state.aft_created = value;
    }
    if let Some(value) = delta.halign {
        state.halign = value;
    }
    if let Some(value) = delta.valign {
        state.valign = value;
    }
    if let Some(value) = delta.text_align {
        state.text_align = value;
    }
    if let Some(value) = delta.uppercase {
        state.uppercase = value;
    }
    if let Some(value) = delta.shadow_len {
        state.shadow_len = value;
    }
    if let Some(value) = delta.shadow_color {
        state.shadow_color = value;
    }
    if let Some(value) = delta.glow {
        state.glow = value;
    }
    if let Some(value) = delta.fov {
        state.fov = Some(value);
    }
    if let Some(value) = delta.vanishpoint {
        state.vanishpoint = Some(value);
    }
    if let Some(value) = delta.diffuse {
        state.diffuse = value;
    }
    if let Some(value) = delta.vertex_colors {
        state.vertex_colors = Some(value);
    }
    if let Some(value) = delta.visible {
        state.visible = value;
    }
    if let Some(value) = delta.hibernating {
        state.hibernating = value;
    }
    if let Some(value) = delta.cropleft {
        state.cropleft = value;
    }
    if let Some(value) = delta.cropright {
        state.cropright = value;
    }
    if let Some(value) = delta.croptop {
        state.croptop = value;
    }
    if let Some(value) = delta.cropbottom {
        state.cropbottom = value;
    }
    if let Some(value) = delta.fadeleft {
        state.fadeleft = value;
    }
    if let Some(value) = delta.faderight {
        state.faderight = value;
    }
    if let Some(value) = delta.fadetop {
        state.fadetop = value;
    }
    if let Some(value) = delta.fadebottom {
        state.fadebottom = value;
    }
    if let Some(value) = delta.mask_source {
        state.mask_source = value;
    }
    if let Some(value) = delta.mask_dest {
        state.mask_dest = value;
    }
    if let Some(value) = delta.depth_test {
        state.depth_test = value;
    }
    if let Some(value) = delta.depth_write {
        state.depth_write = value;
    }
    if let Some(value) = delta.depth_test_on_fail {
        state.depth_test_on_fail = value;
    }
    if let Some(value) = delta.cull_mode {
        state.cull_mode = value;
    }
    if let Some(value) = delta.zoom {
        state.zoom = value;
    }
    if let Some(value) = delta.zoom_x {
        state.zoom_x = value;
    }
    if let Some(value) = delta.zoom_y {
        state.zoom_y = value;
    }
    if let Some(value) = delta.zoom_z {
        state.zoom_z = value;
    }
    if let Some(value) = delta.basezoom {
        state.basezoom = value;
    }
    if let Some(value) = delta.basezoom_x {
        state.basezoom_x = value;
    }
    if let Some(value) = delta.basezoom_y {
        state.basezoom_y = value;
    }
    if let Some(value) = delta.basezoom_z {
        state.basezoom_z = value;
    }
    if let Some(value) = delta.rot_x_deg {
        state.rot_x_deg = value;
    }
    if let Some(value) = delta.rot_y_deg {
        state.rot_y_deg = value;
    }
    if let Some(value) = delta.rot_z_deg {
        state.rot_z_deg = value;
    }
    if let Some(value) = delta.quaternion {
        state.quaternion = value;
    }
    if let Some(value) = delta.base_alpha {
        state.base_alpha = value;
    }
    if let Some(value) = delta.base_rotation {
        state.base_rotation = value;
    }
    if let Some(value) = delta.skew_x {
        state.skew_x = value;
    }
    if let Some(value) = delta.skew_y {
        state.skew_y = value;
    }
    if let Some(value) = delta.blend {
        state.blend = value;
    }
    if let Some(value) = delta.vibrate {
        state.vibrate = value;
    }
    if let Some(value) = delta.effect_magnitude {
        state.effect_magnitude = value;
    }
    if let Some(value) = delta.effect_clock {
        state.effect_clock = value;
    }
    if let Some(value) = delta.effect_timer {
        state.effect_timer = value;
    }
    if let Some(value) = delta.effect_no_offset {
        state.effect_no_offset = value;
    }
    if let Some(value) = delta.effect_mode {
        state.effect_mode = value;
    }
    if let Some(value) = delta.effect_color1 {
        state.effect_color1 = value;
    }
    if let Some(value) = delta.effect_color2 {
        state.effect_color2 = value;
    }
    if let Some(value) = delta.effect_period {
        state.effect_period = value;
    }
    if let Some(value) = delta.effect_offset {
        state.effect_offset = value;
    }
    if let Some(value) = delta.effect_timing {
        state.effect_timing = Some(value);
    }
    if let Some(value) = delta.rainbow {
        state.rainbow = value;
    }
    if let Some(value) = delta.rainbow_scroll {
        state.rainbow_scroll = value;
    }
    if let Some(value) = delta.text_jitter {
        state.text_jitter = value;
    }
    if let Some(value) = delta.text_distortion {
        state.text_distortion = value;
    }
    if let Some(value) = delta.text_glow_mode {
        state.text_glow_mode = value;
    }
    if let Some(value) = delta.mult_attrs_with_diffuse {
        state.mult_attrs_with_diffuse = value;
    }
    if let Some(value) = delta.sprite_texture {
        state.sprite_texture = value;
    }
    if let Some(value) = delta.sprite_animate {
        state.sprite_animate = value;
    }
    if let Some(value) = delta.sprite_loop {
        state.sprite_loop = value;
    }
    if let Some(value) = delta.sprite_playback_rate {
        state.sprite_playback_rate = value;
    }
    if let Some(value) = delta.sprite_state_delay {
        state.sprite_state_delay = value;
    }
    if let Some(value) = delta.sprite_state_index {
        state.sprite_state_index = Some(value);
    }
    if let Some(value) = delta.vert_spacing {
        state.vert_spacing = Some(value);
    }
    if let Some(value) = delta.wrap_width_pixels {
        state.wrap_width_pixels = Some(value);
    }
    if let Some(value) = delta.max_width {
        state.max_width = Some(value);
    }
    if let Some(value) = delta.max_height {
        state.max_height = Some(value);
    }
    if let Some(value) = delta.max_w_pre_zoom {
        state.max_w_pre_zoom = value;
    }
    if let Some(value) = delta.max_h_pre_zoom {
        state.max_h_pre_zoom = value;
    }
    if let Some(value) = delta.max_dimension_uses_zoom {
        state.max_dimension_uses_zoom = value;
    }
    if let Some(value) = delta.texture_filtering {
        state.texture_filtering = value;
    }
    if let Some(value) = delta.texture_wrapping {
        state.texture_wrapping = value;
    }
    if let Some(value) = delta.texcoord_offset {
        state.texcoord_offset = Some(value);
    }
    if let Some(value) = delta.custom_texture_rect {
        state.custom_texture_rect = Some(value);
    }
    if let Some(value) = delta.texcoord_velocity {
        state.texcoord_velocity = Some(value);
    }
    if let Some(value) = delta.size {
        state.size = Some(value);
    }
    if let Some(value) = delta.stretch_rect {
        state.stretch_rect = Some(value);
    }
}

// Actor::TweenState uses RageUtil::lerp, rounding the product before the sum.
// Fusing these operations changes depth enough to distort perspective near W=0.
fn actor_lerp(from: f32, to: f32, factor: f32) -> f32 {
    (to - from) * factor + from
}

/// Interpolates written properties in place, retaining implicit baselines and unwritten state.
///
/// The factor is not clamped, allowing easing curves to overshoot. Discrete
/// properties switch at `t >= 1.0 - f32::EPSILON`. Optional continuous
/// properties interpolate only when their current value exists. Callers apply
/// any explicit starting delta before interpolation; this operation does not
/// change animation-clock bookkeeping or schedule playback events.
pub fn overlay_state_lerp(
    from: &mut SongLuaOverlayState,
    delta: &SongLuaOverlayStateDelta,
    t: f32,
) {
    if let Some(to) = delta.aux {
        from.aux = actor_lerp(from.aux, to, t);
    }
    let x = delta.x.map_or(from.x, |to| actor_lerp(from.x, to, t));
    let y = delta.y.map_or(from.y, |to| actor_lerp(from.y, to, t));
    if delta.stretch_rect.is_some() {
        from.x = x;
        from.y = y;
    } else {
        move_overlay(from, x, y);
    }
    if let Some(to) = delta.z {
        from.z = actor_lerp(from.z, to, t);
    }
    if let Some(to) = delta.z_bias {
        from.z_bias = actor_lerp(from.z_bias, to, t);
    }
    if let Some(to) = delta.draw_order
        && t >= 1.0 - f32::EPSILON
    {
        from.draw_order = to;
    }
    if let Some(to) = delta.draw_by_z_position
        && t >= 1.0 - f32::EPSILON
    {
        from.draw_by_z_position = to;
    }
    if let Some(to) = delta.aft_preserve
        && t >= 1.0 - f32::EPSILON
    {
        from.aft_preserve = to;
    }
    if let Some(to) = delta.aft_created
        && t >= 1.0 - f32::EPSILON
    {
        from.aft_created = to;
    }
    if let Some(to) = delta.halign {
        from.halign = actor_lerp(from.halign, to, t);
    }
    if let Some(to) = delta.valign {
        from.valign = actor_lerp(from.valign, to, t);
    }
    if let Some(to) = delta.text_align
        && t >= 1.0 - f32::EPSILON
    {
        from.text_align = to;
    }
    if let Some(to) = delta.uppercase
        && t >= 1.0 - f32::EPSILON
    {
        from.uppercase = to;
    }
    if let Some(to) = delta.shadow_len {
        from.shadow_len = [
            actor_lerp(from.shadow_len[0], to[0], t),
            actor_lerp(from.shadow_len[1], to[1], t),
        ];
    }
    if let Some(to) = delta.shadow_color {
        for (value, to) in from.shadow_color.iter_mut().zip(to) {
            *value = actor_lerp(*value, to, t);
        }
    }
    if let Some(to) = delta.glow {
        for (value, to) in from.glow.iter_mut().zip(to) {
            *value = actor_lerp(*value, to, t);
        }
    }
    if let (Some(from_fov), Some(to_fov)) = (from.fov, delta.fov) {
        from.fov = Some(actor_lerp(from_fov, to_fov, t));
    }
    if let (Some(from_vanish), Some(to_vanish)) = (from.vanishpoint, delta.vanishpoint) {
        from.vanishpoint = Some([
            actor_lerp(from_vanish[0], to_vanish[0], t),
            actor_lerp(from_vanish[1], to_vanish[1], t),
        ]);
    }
    if let Some(to) = delta.diffuse {
        for (value, to) in from.diffuse.iter_mut().zip(to) {
            *value = actor_lerp(*value, to, t);
        }
    }
    if let Some(to_colors) = delta.vertex_colors {
        let mut from_colors = from.vertex_colors.unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]);
        for corner in 0..4 {
            for channel in 0..4 {
                from_colors[corner][channel] =
                    actor_lerp(from_colors[corner][channel], to_colors[corner][channel], t);
            }
        }
        from.vertex_colors = Some(from_colors);
    }
    if let Some(to) = delta.cropleft {
        from.cropleft = actor_lerp(from.cropleft, to, t);
    }
    if let Some(to) = delta.cropright {
        from.cropright = actor_lerp(from.cropright, to, t);
    }
    if let Some(to) = delta.croptop {
        from.croptop = actor_lerp(from.croptop, to, t);
    }
    if let Some(to) = delta.cropbottom {
        from.cropbottom = actor_lerp(from.cropbottom, to, t);
    }
    if let Some(to) = delta.fadeleft {
        from.fadeleft = actor_lerp(from.fadeleft, to, t);
    }
    if let Some(to) = delta.faderight {
        from.faderight = actor_lerp(from.faderight, to, t);
    }
    if let Some(to) = delta.fadetop {
        from.fadetop = actor_lerp(from.fadetop, to, t);
    }
    if let Some(to) = delta.fadebottom {
        from.fadebottom = actor_lerp(from.fadebottom, to, t);
    }
    if let Some(to) = delta.mask_source
        && t >= 1.0 - f32::EPSILON
    {
        from.mask_source = to;
    }
    if let Some(to) = delta.mask_dest
        && t >= 1.0 - f32::EPSILON
    {
        from.mask_dest = to;
    }
    if let Some(to) = delta.zoom {
        from.zoom = actor_lerp(from.zoom, to, t);
    }
    if let Some(to) = delta.zoom_x {
        from.zoom_x = actor_lerp(from.zoom_x, to, t);
    }
    if let Some(to) = delta.zoom_y {
        from.zoom_y = actor_lerp(from.zoom_y, to, t);
    }
    if let Some(to) = delta.zoom_z {
        from.zoom_z = actor_lerp(from.zoom_z, to, t);
    }
    if let Some(to) = delta.basezoom {
        from.basezoom = actor_lerp(from.basezoom, to, t);
    }
    if let Some(to) = delta.basezoom_x {
        from.basezoom_x = actor_lerp(from.basezoom_x, to, t);
    }
    if let Some(to) = delta.basezoom_y {
        from.basezoom_y = actor_lerp(from.basezoom_y, to, t);
    }
    if let Some(to) = delta.basezoom_z {
        from.basezoom_z = actor_lerp(from.basezoom_z, to, t);
    }
    if let Some(to) = delta.rot_x_deg {
        from.rot_x_deg = actor_lerp(from.rot_x_deg, to, t);
    }
    if let Some(to) = delta.rot_y_deg {
        from.rot_y_deg = actor_lerp(from.rot_y_deg, to, t);
    }
    if let Some(to) = delta.rot_z_deg {
        from.rot_z_deg = actor_lerp(from.rot_z_deg, to, t);
    }
    if let Some(to) = delta.quaternion {
        from.quaternion = crate::actor_quaternion::slerp(from.quaternion, to, t);
    }
    if let Some(to) = delta.base_alpha {
        from.base_alpha = to;
    }
    if let Some(to) = delta.base_rotation {
        from.base_rotation = to;
    }
    if let Some(to) = delta.skew_x {
        from.skew_x = actor_lerp(from.skew_x, to, t);
    }
    if let Some(to) = delta.skew_y {
        from.skew_y = actor_lerp(from.skew_y, to, t);
    }
    if let Some(to) = delta.effect_magnitude {
        for (value, to) in from.effect_magnitude.iter_mut().zip(to) {
            *value = actor_lerp(*value, to, t);
        }
    }
    if let Some(to) = delta.effect_color1 {
        for (value, to) in from.effect_color1.iter_mut().zip(to) {
            *value = actor_lerp(*value, to, t);
        }
    }
    if let Some(to) = delta.effect_color2 {
        for (value, to) in from.effect_color2.iter_mut().zip(to) {
            *value = actor_lerp(*value, to, t);
        }
    }
    if let Some(to) = delta.effect_period {
        from.effect_period = actor_lerp(from.effect_period, to, t);
    }
    if let Some(to) = delta.effect_offset {
        from.effect_offset = actor_lerp(from.effect_offset, to, t);
    }
    if let (Some(from_timing), Some(to_timing)) = (from.effect_timing, delta.effect_timing) {
        from.effect_timing = Some([
            actor_lerp(from_timing[0], to_timing[0], t),
            actor_lerp(from_timing[1], to_timing[1], t),
            actor_lerp(from_timing[2], to_timing[2], t),
            actor_lerp(from_timing[3], to_timing[3], t),
            actor_lerp(from_timing[4], to_timing[4], t),
        ]);
    }
    if let Some(to) = delta.sprite_playback_rate {
        from.sprite_playback_rate = actor_lerp(from.sprite_playback_rate, to, t);
    }
    if let Some(to) = delta.sprite_state_delay {
        from.sprite_state_delay = actor_lerp(from.sprite_state_delay, to, t);
    }
    if let Some(to) = delta.sprite_state_index
        && t >= 1.0 - f32::EPSILON
    {
        from.sprite_state_index = Some(to);
    }
    if let Some(to) = delta.vert_spacing
        && t >= 1.0 - f32::EPSILON
    {
        from.vert_spacing = Some(to);
    }
    if let Some(to) = delta.wrap_width_pixels
        && t >= 1.0 - f32::EPSILON
    {
        from.wrap_width_pixels = Some(to);
    }
    if let (Some(from_width), Some(to_width)) = (from.max_width, delta.max_width) {
        from.max_width = Some(actor_lerp(from_width, to_width, t));
    }
    if let (Some(from_height), Some(to_height)) = (from.max_height, delta.max_height) {
        from.max_height = Some(actor_lerp(from_height, to_height, t));
    }
    if let Some(to) = delta.max_w_pre_zoom
        && t >= 1.0 - f32::EPSILON
    {
        from.max_w_pre_zoom = to;
    }
    if let Some(to) = delta.max_h_pre_zoom
        && t >= 1.0 - f32::EPSILON
    {
        from.max_h_pre_zoom = to;
    }
    if let Some(to) = delta.max_dimension_uses_zoom
        && t >= 1.0 - f32::EPSILON
    {
        from.max_dimension_uses_zoom = to;
    }
    if let (Some(from_offset), Some(to_offset)) = (from.texcoord_offset, delta.texcoord_offset) {
        from.texcoord_offset = Some([
            actor_lerp(from_offset[0], to_offset[0], t),
            actor_lerp(from_offset[1], to_offset[1], t),
        ]);
    }
    if let (Some(from_rect), Some(to_rect)) = (from.custom_texture_rect, delta.custom_texture_rect)
    {
        from.custom_texture_rect = Some([
            actor_lerp(from_rect[0], to_rect[0], t),
            actor_lerp(from_rect[1], to_rect[1], t),
            actor_lerp(from_rect[2], to_rect[2], t),
            actor_lerp(from_rect[3], to_rect[3], t),
        ]);
    }
    if let (Some(from_vel), Some(to_vel)) = (from.texcoord_velocity, delta.texcoord_velocity) {
        from.texcoord_velocity = Some([
            actor_lerp(from_vel[0], to_vel[0], t),
            actor_lerp(from_vel[1], to_vel[1], t),
        ]);
    }
    if let (Some(from_size), Some(to_size)) = (from.size, delta.size) {
        from.size = Some([
            actor_lerp(from_size[0], to_size[0], t),
            actor_lerp(from_size[1], to_size[1], t),
        ]);
    }
    if let (Some(from_rect), Some(to_rect)) = (from.stretch_rect, delta.stretch_rect) {
        from.stretch_rect = Some([
            actor_lerp(from_rect[0], to_rect[0], t),
            actor_lerp(from_rect[1], to_rect[1], t),
            actor_lerp(from_rect[2], to_rect[2], t),
            actor_lerp(from_rect[3], to_rect[3], t),
        ]);
    }
    if let Some(to) = delta.visible
        && t >= 1.0 - f32::EPSILON
    {
        from.visible = to;
    }
    if let Some(to) = delta.hibernating
        && t >= 1.0 - f32::EPSILON
    {
        from.hibernating = to;
    }
    if let Some(to) = delta.blend
        && t >= 1.0 - f32::EPSILON
    {
        from.blend = to;
    }
    if let Some(to) = delta.vibrate
        && t >= 1.0 - f32::EPSILON
    {
        from.vibrate = to;
    }
    if let Some(to) = delta.effect_clock
        && t >= 1.0 - f32::EPSILON
    {
        from.effect_clock = to;
    }
    if let Some(to) = delta.effect_timer
        && t >= 1.0 - f32::EPSILON
    {
        from.effect_timer = to;
    }
    if let Some(to) = delta.effect_no_offset
        && t >= 1.0 - f32::EPSILON
    {
        from.effect_no_offset = to;
    }
    if let Some(to) = delta.effect_mode
        && t >= 1.0 - f32::EPSILON
    {
        from.effect_mode = to;
    }
    if let Some(to) = delta.rainbow
        && t >= 1.0 - f32::EPSILON
    {
        from.rainbow = to;
    }
    if let Some(to) = delta.rainbow_scroll
        && t >= 1.0 - f32::EPSILON
    {
        from.rainbow_scroll = to;
    }
    if let Some(to) = delta.text_jitter
        && t >= 1.0 - f32::EPSILON
    {
        from.text_jitter = to;
    }
    if let Some(to) = delta.text_distortion {
        from.text_distortion = actor_lerp(from.text_distortion, to, t);
    }
    if let Some(to) = delta.text_glow_mode
        && t >= 1.0 - f32::EPSILON
    {
        from.text_glow_mode = to;
    }
    if let Some(to) = delta.mult_attrs_with_diffuse
        && t >= 1.0 - f32::EPSILON
    {
        from.mult_attrs_with_diffuse = to;
    }
    if let Some(to) = delta.sprite_texture
        && t >= 1.0 - f32::EPSILON
    {
        from.sprite_texture = to;
    }
    if let Some(to) = delta.sprite_animate
        && t >= 1.0 - f32::EPSILON
    {
        from.sprite_animate = to;
    }
    if let Some(to) = delta.sprite_loop
        && t >= 1.0 - f32::EPSILON
    {
        from.sprite_loop = to;
    }
    if let Some(to) = delta.texture_wrapping
        && t >= 1.0 - f32::EPSILON
    {
        from.texture_wrapping = to;
    }
    if let Some(to) = delta.texture_filtering
        && t >= 1.0 - f32::EPSILON
    {
        from.texture_filtering = to;
    }
    if let Some(to) = delta.cull_mode {
        from.cull_mode = to;
    }
    if let Some(to) = delta.depth_test
        && t >= 1.0 - f32::EPSILON
    {
        from.depth_test = to;
    }
    if let Some(to) = delta.depth_write
        && t >= 1.0 - f32::EPSILON
    {
        from.depth_write = to;
    }
    if let Some(to) = delta.depth_test_on_fail
        && t >= 1.0 - f32::EPSILON
    {
        from.depth_test_on_fail = to;
    }
}

const fn overlay_delta_is_empty(delta: &SongLuaOverlayStateDelta) -> bool {
    delta.aux.is_none()
        && delta.x.is_none()
        && delta.y.is_none()
        && delta.z.is_none()
        && delta.z_bias.is_none()
        && delta.draw_order.is_none()
        && delta.draw_by_z_position.is_none()
        && delta.aft_preserve.is_none()
        && delta.aft_created.is_none()
        && delta.halign.is_none()
        && delta.valign.is_none()
        && delta.text_align.is_none()
        && delta.uppercase.is_none()
        && delta.shadow_len.is_none()
        && delta.shadow_color.is_none()
        && delta.glow.is_none()
        && delta.fov.is_none()
        && delta.vanishpoint.is_none()
        && delta.diffuse.is_none()
        && delta.vertex_colors.is_none()
        && delta.visible.is_none()
        && delta.hibernating.is_none()
        && delta.cropleft.is_none()
        && delta.cropright.is_none()
        && delta.croptop.is_none()
        && delta.cropbottom.is_none()
        && delta.fadeleft.is_none()
        && delta.faderight.is_none()
        && delta.fadetop.is_none()
        && delta.fadebottom.is_none()
        && delta.mask_source.is_none()
        && delta.mask_dest.is_none()
        && delta.depth_test.is_none()
        && delta.depth_write.is_none()
        && delta.depth_test_on_fail.is_none()
        && delta.cull_mode.is_none()
        && delta.zoom.is_none()
        && delta.zoom_x.is_none()
        && delta.zoom_y.is_none()
        && delta.zoom_z.is_none()
        && delta.basezoom.is_none()
        && delta.basezoom_x.is_none()
        && delta.basezoom_y.is_none()
        && delta.basezoom_z.is_none()
        && delta.rot_x_deg.is_none()
        && delta.rot_y_deg.is_none()
        && delta.rot_z_deg.is_none()
        && delta.quaternion.is_none()
        && delta.base_alpha.is_none()
        && delta.base_rotation.is_none()
        && delta.skew_x.is_none()
        && delta.skew_y.is_none()
        && delta.blend.is_none()
        && delta.vibrate.is_none()
        && delta.effect_magnitude.is_none()
        && delta.effect_clock.is_none()
        && delta.effect_timer.is_none()
        && delta.effect_no_offset.is_none()
        && delta.effect_mode.is_none()
        && delta.effect_color1.is_none()
        && delta.effect_color2.is_none()
        && delta.effect_period.is_none()
        && delta.effect_offset.is_none()
        && delta.effect_timing.is_none()
        && delta.rainbow.is_none()
        && delta.rainbow_scroll.is_none()
        && delta.text_jitter.is_none()
        && delta.text_distortion.is_none()
        && delta.text_glow_mode.is_none()
        && delta.mult_attrs_with_diffuse.is_none()
        && delta.sprite_texture.is_none()
        && delta.sprite_animate.is_none()
        && delta.sprite_loop.is_none()
        && delta.sprite_playback_rate.is_none()
        && delta.sprite_state_delay.is_none()
        && delta.sprite_state_index.is_none()
        && delta.vert_spacing.is_none()
        && delta.wrap_width_pixels.is_none()
        && delta.max_width.is_none()
        && delta.max_height.is_none()
        && delta.max_w_pre_zoom.is_none()
        && delta.max_h_pre_zoom.is_none()
        && delta.max_dimension_uses_zoom.is_none()
        && delta.texture_filtering.is_none()
        && delta.texture_wrapping.is_none()
        && delta.texcoord_offset.is_none()
        && delta.custom_texture_rect.is_none()
        && delta.texcoord_velocity.is_none()
        && delta.size.is_none()
        && delta.stretch_rect.is_none()
        && delta.sound_play.is_none()
}

const fn merge_overlay_delta(into: &mut SongLuaOverlayStateDelta, from: &SongLuaOverlayStateDelta) {
    if from.aux.is_some() {
        into.aux = from.aux;
    }
    if from.x.is_some() {
        into.x = from.x;
    }
    if from.y.is_some() {
        into.y = from.y;
    }
    if from.z.is_some() {
        into.z = from.z;
    }
    if from.z_bias.is_some() {
        into.z_bias = from.z_bias;
    }
    if from.draw_order.is_some() {
        into.draw_order = from.draw_order;
    }
    if from.draw_by_z_position.is_some() {
        into.draw_by_z_position = from.draw_by_z_position;
    }
    if from.aft_preserve.is_some() {
        into.aft_preserve = from.aft_preserve;
    }
    if from.aft_created.is_some() {
        into.aft_created = from.aft_created;
    }
    if from.halign.is_some() {
        into.halign = from.halign;
    }
    if from.valign.is_some() {
        into.valign = from.valign;
    }
    if from.text_align.is_some() {
        into.text_align = from.text_align;
    }
    if from.uppercase.is_some() {
        into.uppercase = from.uppercase;
    }
    if from.shadow_len.is_some() {
        into.shadow_len = from.shadow_len;
    }
    if from.shadow_color.is_some() {
        into.shadow_color = from.shadow_color;
    }
    if from.glow.is_some() {
        into.glow = from.glow;
    }
    if from.fov.is_some() {
        into.fov = from.fov;
    }
    if from.vanishpoint.is_some() {
        into.vanishpoint = from.vanishpoint;
    }
    if from.diffuse.is_some() {
        into.diffuse = from.diffuse;
    }
    if from.visible.is_some() {
        into.visible = from.visible;
    }
    if from.hibernating.is_some() {
        into.hibernating = from.hibernating;
    }
    if from.cropleft.is_some() {
        into.cropleft = from.cropleft;
    }
    if from.cropright.is_some() {
        into.cropright = from.cropright;
    }
    if from.croptop.is_some() {
        into.croptop = from.croptop;
    }
    if from.cropbottom.is_some() {
        into.cropbottom = from.cropbottom;
    }
    if from.fadeleft.is_some() {
        into.fadeleft = from.fadeleft;
    }
    if from.faderight.is_some() {
        into.faderight = from.faderight;
    }
    if from.fadetop.is_some() {
        into.fadetop = from.fadetop;
    }
    if from.fadebottom.is_some() {
        into.fadebottom = from.fadebottom;
    }
    if from.mask_source.is_some() {
        into.mask_source = from.mask_source;
    }
    if from.mask_dest.is_some() {
        into.mask_dest = from.mask_dest;
    }
    if from.depth_test.is_some() {
        into.depth_test = from.depth_test;
    }
    if from.depth_write.is_some() {
        into.depth_write = from.depth_write;
    }
    if from.depth_test_on_fail.is_some() {
        into.depth_test_on_fail = from.depth_test_on_fail;
    }
    if from.cull_mode.is_some() {
        into.cull_mode = from.cull_mode;
    }
    if from.halign.is_some() {
        into.halign = from.halign;
    }
    if from.valign.is_some() {
        into.valign = from.valign;
    }
    if from.text_align.is_some() {
        into.text_align = from.text_align;
    }
    if from.shadow_len.is_some() {
        into.shadow_len = from.shadow_len;
    }
    if from.shadow_color.is_some() {
        into.shadow_color = from.shadow_color;
    }
    if from.glow.is_some() {
        into.glow = from.glow;
    }
    if from.vertex_colors.is_some() {
        into.vertex_colors = from.vertex_colors;
    }
    if from.zoom.is_some() {
        into.zoom = from.zoom;
    }
    if from.zoom_x.is_some() {
        into.zoom_x = from.zoom_x;
    }
    if from.zoom_y.is_some() {
        into.zoom_y = from.zoom_y;
    }
    if from.zoom_z.is_some() {
        into.zoom_z = from.zoom_z;
    }
    if from.basezoom.is_some() {
        into.basezoom = from.basezoom;
    }
    if from.basezoom_x.is_some() {
        into.basezoom_x = from.basezoom_x;
    }
    if from.basezoom_y.is_some() {
        into.basezoom_y = from.basezoom_y;
    }
    if from.basezoom_z.is_some() {
        into.basezoom_z = from.basezoom_z;
    }
    if from.rot_x_deg.is_some() {
        into.rot_x_deg = from.rot_x_deg;
    }
    if from.rot_y_deg.is_some() {
        into.rot_y_deg = from.rot_y_deg;
    }
    if from.rot_z_deg.is_some() {
        into.rot_z_deg = from.rot_z_deg;
    }
    if from.quaternion.is_some() {
        into.quaternion = from.quaternion;
    }
    if from.base_alpha.is_some() {
        into.base_alpha = from.base_alpha;
    }
    if from.base_rotation.is_some() {
        into.base_rotation = from.base_rotation;
    }
    if from.skew_x.is_some() {
        into.skew_x = from.skew_x;
    }
    if from.skew_y.is_some() {
        into.skew_y = from.skew_y;
    }
    if from.blend.is_some() {
        into.blend = from.blend;
    }
    if from.vibrate.is_some() {
        into.vibrate = from.vibrate;
    }
    if from.effect_magnitude.is_some() {
        into.effect_magnitude = from.effect_magnitude;
    }
    if from.effect_clock.is_some() {
        into.effect_clock = from.effect_clock;
    }
    if from.effect_timer.is_some() {
        into.effect_timer = from.effect_timer;
    }
    if from.effect_no_offset.is_some() {
        into.effect_no_offset = from.effect_no_offset;
    }
    if from.effect_mode.is_some() {
        into.effect_mode = from.effect_mode;
    }
    if from.effect_color1.is_some() {
        into.effect_color1 = from.effect_color1;
    }
    if from.effect_color2.is_some() {
        into.effect_color2 = from.effect_color2;
    }
    if from.effect_period.is_some() {
        into.effect_period = from.effect_period;
    }
    if from.effect_offset.is_some() {
        into.effect_offset = from.effect_offset;
    }
    if from.effect_timing.is_some() {
        into.effect_timing = from.effect_timing;
    }
    if from.rainbow.is_some() {
        into.rainbow = from.rainbow;
    }
    if from.rainbow_scroll.is_some() {
        into.rainbow_scroll = from.rainbow_scroll;
    }
    if from.text_jitter.is_some() {
        into.text_jitter = from.text_jitter;
    }
    if from.text_distortion.is_some() {
        into.text_distortion = from.text_distortion;
    }
    if from.text_glow_mode.is_some() {
        into.text_glow_mode = from.text_glow_mode;
    }
    if from.mult_attrs_with_diffuse.is_some() {
        into.mult_attrs_with_diffuse = from.mult_attrs_with_diffuse;
    }
    if from.sprite_texture.is_some() {
        into.sprite_texture = from.sprite_texture;
    }
    if from.sprite_animate.is_some() {
        into.sprite_animate = from.sprite_animate;
    }
    if from.sprite_loop.is_some() {
        into.sprite_loop = from.sprite_loop;
    }
    if from.sprite_playback_rate.is_some() {
        into.sprite_playback_rate = from.sprite_playback_rate;
    }
    if from.sprite_state_delay.is_some() {
        into.sprite_state_delay = from.sprite_state_delay;
    }
    if from.sprite_state_index.is_some() {
        into.sprite_state_index = from.sprite_state_index;
    }
    if from.vert_spacing.is_some() {
        into.vert_spacing = from.vert_spacing;
    }
    if from.wrap_width_pixels.is_some() {
        into.wrap_width_pixels = from.wrap_width_pixels;
    }
    if from.max_width.is_some() {
        into.max_width = from.max_width;
    }
    if from.max_height.is_some() {
        into.max_height = from.max_height;
    }
    if from.max_w_pre_zoom.is_some() {
        into.max_w_pre_zoom = from.max_w_pre_zoom;
    }
    if from.max_h_pre_zoom.is_some() {
        into.max_h_pre_zoom = from.max_h_pre_zoom;
    }
    if from.max_dimension_uses_zoom.is_some() {
        into.max_dimension_uses_zoom = from.max_dimension_uses_zoom;
    }
    if from.texture_filtering.is_some() {
        into.texture_filtering = from.texture_filtering;
    }
    if from.texture_wrapping.is_some() {
        into.texture_wrapping = from.texture_wrapping;
    }
    if from.texcoord_offset.is_some() {
        into.texcoord_offset = from.texcoord_offset;
    }
    if from.custom_texture_rect.is_some() {
        into.custom_texture_rect = from.custom_texture_rect;
    }
    if from.texcoord_velocity.is_some() {
        into.texcoord_velocity = from.texcoord_velocity;
    }
    if from.size.is_some() {
        into.size = from.size;
    }
    if from.stretch_rect.is_some() {
        into.stretch_rect = from.stretch_rect;
    }
    if from.sound_play.is_some() {
        into.sound_play = from.sound_play;
    }
}

#[must_use]
pub fn overlay_delta_from_blocks(
    blocks: &[SongLuaOverlayCommandBlock],
) -> Option<SongLuaOverlayStateDelta> {
    let mut delta = SongLuaOverlayStateDelta::default();
    for block in blocks {
        merge_overlay_delta(&mut delta, &block.delta);
    }
    (!overlay_delta_is_empty(&delta)).then_some(delta)
}

#[must_use]
pub fn overlay_delta_intersection(
    from: &SongLuaOverlayStateDelta,
    to: &SongLuaOverlayStateDelta,
) -> Option<(SongLuaOverlayStateDelta, SongLuaOverlayStateDelta)> {
    let mut out_from = SongLuaOverlayStateDelta::default();
    let mut out_to = SongLuaOverlayStateDelta::default();
    macro_rules! copy_pair {
        ($field:ident) => {
            if let (Some(from_value), Some(to_value)) = (from.$field, to.$field) {
                out_from.$field = Some(from_value);
                out_to.$field = Some(to_value);
            }
        };
    }
    copy_pair!(aux);
    copy_pair!(x);
    copy_pair!(y);
    copy_pair!(z);
    copy_pair!(z_bias);
    copy_pair!(draw_order);
    copy_pair!(draw_by_z_position);
    copy_pair!(aft_preserve);
    copy_pair!(aft_created);
    copy_pair!(halign);
    copy_pair!(valign);
    copy_pair!(text_align);
    copy_pair!(uppercase);
    copy_pair!(shadow_len);
    copy_pair!(shadow_color);
    copy_pair!(glow);
    copy_pair!(fov);
    copy_pair!(vanishpoint);
    copy_pair!(diffuse);
    copy_pair!(vertex_colors);
    copy_pair!(visible);
    copy_pair!(hibernating);
    copy_pair!(cropleft);
    copy_pair!(cropright);
    copy_pair!(croptop);
    copy_pair!(cropbottom);
    copy_pair!(fadeleft);
    copy_pair!(faderight);
    copy_pair!(fadetop);
    copy_pair!(fadebottom);
    copy_pair!(mask_source);
    copy_pair!(mask_dest);
    copy_pair!(depth_test);
    copy_pair!(depth_write);
    copy_pair!(depth_test_on_fail);
    copy_pair!(cull_mode);
    copy_pair!(zoom);
    copy_pair!(zoom_x);
    copy_pair!(zoom_y);
    copy_pair!(zoom_z);
    copy_pair!(basezoom);
    copy_pair!(basezoom_x);
    copy_pair!(basezoom_y);
    copy_pair!(basezoom_z);
    copy_pair!(rot_x_deg);
    copy_pair!(rot_y_deg);
    copy_pair!(rot_z_deg);
    copy_pair!(base_rotation);
    copy_pair!(quaternion);
    copy_pair!(base_alpha);
    copy_pair!(skew_x);
    copy_pair!(skew_y);
    copy_pair!(blend);
    copy_pair!(vibrate);
    copy_pair!(effect_magnitude);
    copy_pair!(effect_clock);
    copy_pair!(effect_timer);
    copy_pair!(effect_no_offset);
    copy_pair!(effect_mode);
    copy_pair!(effect_color1);
    copy_pair!(effect_color2);
    copy_pair!(effect_period);
    copy_pair!(effect_offset);
    copy_pair!(effect_timing);
    copy_pair!(rainbow);
    copy_pair!(rainbow_scroll);
    copy_pair!(text_jitter);
    copy_pair!(text_distortion);
    copy_pair!(text_glow_mode);
    copy_pair!(mult_attrs_with_diffuse);
    copy_pair!(sprite_texture);
    copy_pair!(sprite_animate);
    copy_pair!(sprite_loop);
    copy_pair!(sprite_playback_rate);
    copy_pair!(sprite_state_delay);
    copy_pair!(sprite_state_index);
    copy_pair!(vert_spacing);
    copy_pair!(wrap_width_pixels);
    copy_pair!(max_width);
    copy_pair!(max_height);
    copy_pair!(max_w_pre_zoom);
    copy_pair!(max_h_pre_zoom);
    copy_pair!(max_dimension_uses_zoom);
    copy_pair!(texture_filtering);
    copy_pair!(texture_wrapping);
    copy_pair!(texcoord_offset);
    copy_pair!(custom_texture_rect);
    copy_pair!(texcoord_velocity);
    copy_pair!(size);
    copy_pair!(stretch_rect);
    (!overlay_delta_is_empty(&out_from)).then_some((out_from, out_to))
}

#[derive(Debug, Clone)]
pub struct SongLuaOverlayEaseBuildParams {
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaSpanMode,
    pub easing: Option<String>,
    pub sustain: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

#[must_use]
pub fn overlay_eases_from_captures(
    overlay_count: usize,
    from_blocks: &[(usize, Vec<SongLuaOverlayCommandBlock>)],
    to_blocks: &[(usize, Vec<SongLuaOverlayCommandBlock>)],
    params: SongLuaOverlayEaseBuildParams,
) -> Vec<SongLuaOverlayEase> {
    let mut from_deltas = HashMap::new();
    for (overlay_index, blocks) in from_blocks {
        if let Some(delta) = overlay_delta_from_blocks(blocks) {
            from_deltas.insert(*overlay_index, delta);
        }
    }
    let mut to_deltas = HashMap::new();
    for (overlay_index, blocks) in to_blocks {
        if let Some(delta) = overlay_delta_from_blocks(blocks) {
            to_deltas.insert(*overlay_index, delta);
        }
    }

    let mut out = Vec::new();
    for overlay_index in 0..overlay_count {
        let Some((from_delta, to_delta)) = from_deltas
            .get(&overlay_index)
            .zip(to_deltas.get(&overlay_index))
            .and_then(|(from_delta, to_delta)| overlay_delta_intersection(from_delta, to_delta))
        else {
            continue;
        };
        out.push(SongLuaOverlayEase {
            overlay_index,
            unit: params.unit,
            start: params.start,
            limit: params.limit,
            span_mode: params.span_mode,
            from: from_delta,
            to: to_delta,
            easing: params.easing.clone(),
            sustain: params.sustain,
            opt1: params.opt1,
            opt2: params.opt2,
        });
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayMessageCommand {
    /// Frame delta consumed by a receiver updated after the broadcaster.
    pub frame_advance: f32,
    pub message: String,
    pub blocks: Vec<SongLuaOverlayCommandBlock>,
}

pub fn message_command_lists_have_listener<'a>(
    command_lists: impl IntoIterator<Item = &'a [SongLuaOverlayMessageCommand]>,
    message: &str,
) -> bool {
    command_lists
        .into_iter()
        .any(|commands| commands.iter().any(|command| command.message == message))
}

#[derive(Debug, Clone)]
pub struct SongLuaOverlayActor<Kind> {
    pub kind: Kind,
    pub name: Option<String>,
    pub parent_index: Option<usize>,
    pub initial_state: SongLuaOverlayState,
    pub message_commands: Vec<SongLuaOverlayMessageCommand>,
}

pub fn overlay_actor_uses_repeat_sampler<Kind>(actor: &SongLuaOverlayActor<Kind>) -> bool {
    overlay_state_uses_repeat_sampler(&actor.initial_state)
        || actor
            .message_commands
            .iter()
            .flat_map(|command| command.blocks.iter())
            .any(|block| overlay_delta_uses_repeat_sampler(&block.delta))
}

pub fn overlay_actor_uses_nearest_sampler<Kind>(actor: &SongLuaOverlayActor<Kind>) -> bool {
    overlay_state_uses_nearest_sampler(&actor.initial_state)
        || actor
            .message_commands
            .iter()
            .flat_map(|command| command.blocks.iter())
            .any(|block| overlay_delta_uses_nearest_sampler(&block.delta))
}

pub fn push_song_lua_video_paths<'a, NoteskinSlot, ModelVertex, TextAttribute>(
    overlays: &'a [SongLuaOverlayActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
    seen: &mut HashSet<&'a str>,
    paths: &mut Vec<PathBuf>,
) {
    for overlay in overlays {
        let SongLuaOverlayKind::Sprite {
            texture_path,
            texture_key,
            textures,
            ..
        } = &overlay.kind
        else {
            continue;
        };
        if !overlay.initial_state.decode_movie {
            continue;
        }
        for (texture_path, texture_key) in std::iter::once((texture_path, texture_key))
            .chain(textures.iter().map(|texture| (&texture.path, &texture.key)))
        {
            if is_song_lua_video_path(texture_path) && seen.insert(texture_key.as_ref()) {
                paths.push(texture_path.clone());
            }
        }
    }
}

#[must_use]
pub fn song_lua_video_paths<NoteskinSlot, ModelVertex, TextAttribute>(
    overlays: &[SongLuaOverlayActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    push_song_lua_video_paths(overlays, &mut seen, &mut paths);
    paths
}

pub fn push_unique_song_lua_sound_paths(
    source: &[PathBuf],
    seen: &mut HashSet<String>,
    paths: &mut Vec<PathBuf>,
) {
    for path in source {
        if seen.insert(path.to_string_lossy().into_owned()) {
            paths.push(path.clone());
        }
    }
}

pub fn push_song_lua_overlay_sound_paths<NoteskinSlot, ModelVertex, TextAttribute>(
    overlays: &[SongLuaOverlayActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
    seen: &mut HashSet<String>,
    paths: &mut Vec<PathBuf>,
) {
    for overlay in overlays {
        let SongLuaOverlayKind::Sound { sound_path } = &overlay.kind else {
            continue;
        };
        if seen.insert(sound_path.to_string_lossy().into_owned()) {
            paths.push(sound_path.clone());
        }
    }
}

pub fn compiled_song_lua_sound_paths<'a, OverlayActor: 'a>(
    compiled: impl IntoIterator<Item = &'a CompiledSongLua<OverlayActor>>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for compiled in compiled {
        push_unique_song_lua_sound_paths(&compiled.sound_paths, &mut seen, &mut paths);
    }
    paths
}

pub fn named_overlay_indices_by_name<'a>(
    len: usize,
    mut name_at: impl FnMut(usize) -> Option<&'a str>,
) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    for index in 0..len {
        if let Some(name) = name_at(index) {
            out.insert(name.to_string(), index);
        }
    }
    out
}

pub fn overlay_descendants_by_parent(
    len: usize,
    root_index: usize,
    mut parent_at: impl FnMut(usize) -> Option<usize>,
) -> Vec<usize> {
    let mut out = Vec::new();
    for index in 0..len {
        let mut parent = parent_at(index);
        while let Some(parent_index) = parent {
            if parent_index == root_index {
                out.push(index);
                break;
            }
            parent = parent_at(parent_index);
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayEase {
    pub overlay_index: usize,
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaSpanMode,
    pub from: SongLuaOverlayStateDelta,
    pub to: SongLuaOverlayStateDelta,
    pub easing: Option<String>,
    pub sustain: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SongLuaOverlayUpdateTarget {
    Aux,
    X,
    Y,
    Z,
    ZBias,
    DrawOrder,
    DrawByZPosition,
    HAlign,
    VAlign,
    TextAlign,
    Uppercase,
    ShadowLen,
    ShadowColor,
    Glow,
    Fov,
    Vanishpoint,
    Diffuse,
    VertexColors,
    Visible,
    CropLeft,
    CropRight,
    CropTop,
    CropBottom,
    FadeLeft,
    FadeRight,
    FadeTop,
    FadeBottom,
    MaskSource,
    MaskDest,
    DepthTest,
    CullMode,
    Zoom,
    ZoomX,
    ZoomY,
    ZoomZ,
    BaseZoom,
    BaseZoomX,
    BaseZoomY,
    BaseZoomZ,
    RotationX,
    RotationY,
    RotationZ,
    BaseRotation,
    Quaternion,
    BaseAlpha,
    SkewX,
    SkewY,
    Blend,
    Vibrate,
    EffectMagnitude,
    EffectClock,
    EffectTimer,
    EffectNoOffset,
    EffectMode,
    EffectColor1,
    EffectColor2,
    EffectPeriod,
    EffectOffset,
    EffectTime,
    EffectTiming,
    Rainbow,
    RainbowScroll,
    TextJitter,
    TextDistortion,
    TextGlowMode,
    MultAttrsWithDiffuse,
    SpriteTexture,
    SpriteAnimate,
    SpriteLoop,
    SpritePlaybackRate,
    SpriteStateDelay,
    SpriteStateIndex,
    VertSpacing,
    WrapWidthPixels,
    MaxWidth,
    MaxHeight,
    MaxWPreZoom,
    MaxHPreZoom,
    MaxDimensionUsesZoom,
    TextureFiltering,
    TextureWrapping,
    TexcoordOffset,
    CustomTextureRect,
    TexcoordVelocity,
    Size,
    StretchRect,
    AftCreated,
    AftPreserve,
    Hibernating,
    DepthWrite,
    DepthTestOnFail,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SongLuaOverlayUpdateValue {
    None,
    F32(f32),
    I32(i32),
    U32(u32),
    Bool(bool),
    CullMode(deadlib_render_core::CullMode),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Quaternion([f32; 4]),
    Vec5([f32; 5]),
    VertexColors(Arc<[[f32; 4]; 4]>),
    TextAlign(TextAlign),
    Blend(SongLuaOverlayBlendMode),
    EffectClock(EffectClock),
    EffectMode(EffectMode),
    TextGlowMode(SongLuaTextGlowMode),
}

impl SongLuaOverlayUpdateValue {
    #[must_use]
    pub fn lerp(&self, to: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mix = |from: f32, to: f32| {
            if from.is_finite() && to.is_finite() {
                (to - from).mul_add(t, from)
            } else if t >= 1.0 {
                to
            } else {
                // Non-finite setters are frame targets. Hold the last sample
                // until the next one instead of creating NaN from infinity.
                from
            }
        };
        match (self, to) {
            (Self::F32(from), Self::F32(to)) => Self::F32(mix(*from, *to)),
            (Self::Vec2(from), Self::Vec2(to)) => {
                Self::Vec2(std::array::from_fn(|i| mix(from[i], to[i])))
            }
            (Self::Vec3(from), Self::Vec3(to)) => {
                Self::Vec3(std::array::from_fn(|i| mix(from[i], to[i])))
            }
            (Self::Quaternion(from), Self::Quaternion(to)) => {
                Self::Quaternion(crate::actor_quaternion::slerp(*from, *to, t))
            }
            (Self::Vec4(from), Self::Vec4(to)) => {
                Self::Vec4(std::array::from_fn(|i| mix(from[i], to[i])))
            }
            (Self::Vec5(from), Self::Vec5(to)) => {
                Self::Vec5(std::array::from_fn(|i| mix(from[i], to[i])))
            }
            _ if t >= 1.0 - f32::EPSILON => to.clone(),
            _ => self.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayUpdateSample {
    /// Render tracks use `overlay_update_unit`; raw audit writes use beats.
    pub time: f32,
    pub value: SongLuaOverlayUpdateValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayUpdateTrack {
    pub overlay_index: usize,
    pub target: SongLuaOverlayUpdateTarget,
    pub samples: Vec<SongLuaOverlayUpdateSample>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaStatefulMessageWrite {
    pub overlay_index: usize,
    pub beat: f32,
    pub target: SongLuaOverlayUpdateTarget,
    pub value: SongLuaOverlayUpdateValue,
    pub delay_seconds: f32,
    pub duration_seconds: f32,
    pub easing: Option<String>,
    pub opt1: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaStatefulMessageCapture {
    pub message: String,
    pub overlay_targets: Vec<(usize, Vec<SongLuaOverlayUpdateTarget>)>,
    pub writes: Vec<SongLuaStatefulMessageWrite>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayRuntimeUpdateSample {
    pub second: f32,
    pub value: SongLuaOverlayUpdateValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaOverlayRuntimeUpdateTrack {
    pub overlay_index: usize,
    pub target: SongLuaOverlayUpdateTarget,
    pub samples: Vec<SongLuaOverlayRuntimeUpdateSample>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SongLuaCapturedChildActor {
    pub initial_state: SongLuaOverlayState,
    pub message_commands: Vec<SongLuaOverlayMessageCommand>,
    /// Immutable sheet changes baked on the worker; all sheets are prewarmed.
    pub textures: Vec<SongLuaJudgmentTexture>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaJudgmentTexture {
    pub second: f32,
    pub path: PathBuf,
    pub key: Arc<str>,
    pub frame_size: [f32; 2],
    pub frame_cols: usize,
    pub frame_rows: usize,
}

pub fn judgment_texture_at(
    actor: &SongLuaCapturedChildActor,
    second: f32,
) -> Option<&SongLuaJudgmentTexture> {
    let end = actor
        .textures
        .partition_point(|texture| texture.second <= second);
    end.checked_sub(1).map(|index| &actor.textures[index])
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SongLuaCapturedActor {
    pub initial_state: SongLuaOverlayState,
    pub message_commands: Vec<SongLuaOverlayMessageCommand>,
    pub judgment: SongLuaCapturedChildActor,
    pub combo: SongLuaCapturedChildActor,
    /// The chart directly draws a Judgment or Combo child outside the Player draw.
    pub manual_hud_draw: bool,
    /// NoteField wrappers retain their own pose; they never modify Player.
    /// Baked on the song-loading worker, with no Lua calls during gameplay.
    pub note_field_frames: Vec<SongLuaNoteFieldFrame>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaNoteFieldFrame {
    pub second: f32,
    /// Native draw order: outermost wrapper first.
    pub wrappers: Arc<[SongLuaOverlayState]>,
}

#[derive(Clone, Copy)]
pub enum SongLuaTrackedActorTarget {
    Player(usize),
    PlayerJudgment(usize),
    PlayerCombo(usize),
    SongForeground,
    ScreenLayer(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaNoteHideWindow {
    pub spline_beats_per_t: f32,
    pub spline_size: usize,
    pub player: usize,
    pub column: usize,
    pub start_beat: f32,
    pub end_beat: f32,
}

#[must_use]
pub fn note_hide_window_from_indices(
    player: usize,
    column: usize,
    beats_per_t: f32,
    start_index: usize,
    end_index: usize,
) -> Option<SongLuaNoteHideWindow> {
    if start_index == 0 || end_index < start_index {
        return None;
    }
    let start_beat = (start_index - 1) as f32 * beats_per_t;
    let end_beat = (end_index - 1) as f32 * beats_per_t;
    if !start_beat.is_finite() || !end_beat.is_finite() || end_beat < start_beat {
        return None;
    }
    Some(SongLuaNoteHideWindow {
        spline_beats_per_t: beats_per_t,
        spline_size: end_index,
        player,
        column,
        start_beat,
        end_beat,
    })
}

#[must_use]
pub fn note_hide_windows_from_flags(
    player: usize,
    column: usize,
    beats_per_t: f32,
    hidden: &[bool],
) -> Vec<SongLuaNoteHideWindow> {
    if !beats_per_t.is_finite() || beats_per_t <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut run_start = None::<usize>;
    for (offset, hidden) in hidden.iter().copied().enumerate() {
        let index = offset + 1;
        match (run_start, hidden) {
            (None, true) => run_start = Some(index),
            (Some(start), false) => {
                if let Some(window) =
                    note_hide_window_from_indices(player, column, beats_per_t, start, index - 1)
                {
                    out.push(window);
                }
                run_start = None;
            }
            _ => {}
        }
    }
    if let Some(start) = run_start
        && let Some(window) =
            note_hide_window_from_indices(player, column, beats_per_t, start, hidden.len())
    {
        out.push(window);
    }
    for window in &mut out {
        window.spline_size = hidden.len();
    }
    out
}

#[must_use]
pub fn note_column_zoom_hide_beats_per_t(
    mode: &str,
    subtract_song_beat: bool,
    beats_per_t: f32,
) -> Option<f32> {
    if !mode.eq_ignore_ascii_case("NoteColumnSplineMode_Offset") || subtract_song_beat {
        return None;
    }
    (beats_per_t.is_finite() && beats_per_t > 0.0).then_some(beats_per_t)
}

pub fn sort_note_hide_windows(windows: &mut [SongLuaNoteHideWindow]) {
    windows.sort_by(|left, right| {
        left.player
            .cmp(&right.player)
            .then_with(|| left.column.cmp(&right.column))
            .then_with(|| left.start_beat.total_cmp(&right.start_beat))
            .then_with(|| left.end_beat.total_cmp(&right.end_beat))
    });
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongLuaColumnOffsetWindow {
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaSpanMode,
    pub player: usize,
    pub column: usize,
    pub target: SongLuaColumnTransformTarget,
    pub from_y: f32,
    pub to_y: f32,
    pub easing: Option<String>,
    pub sustain: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SongLuaColumnTransformTarget {
    OffsetX,
    OffsetY,
    Zoom,
    RotationZ,
    PositionPoint { point: u8, axis: u8 },
    PositionEnabled,
    PositionBeatsPerT,
    PositionReceptorT,
    PositionSubtractBeat,
}

impl SongLuaColumnTransformTarget {
    #[inline(always)]
    #[must_use]
    pub const fn baseline(self) -> f32 {
        match self {
            Self::Zoom | Self::PositionBeatsPerT | Self::PositionSubtractBeat => 1.0,
            _ => 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaColumnOffsetSample {
    pub player: usize,
    pub column: usize,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaColumnTransformSample {
    pub player: usize,
    pub column: usize,
    pub target: SongLuaColumnTransformTarget,
    pub value: f32,
}

#[must_use]
pub fn note_column_pos_offset_y_from_points(mode: &str, points: &[[f32; 2]]) -> Option<f32> {
    const EPS: f32 = 0.001;
    if mode.eq_ignore_ascii_case("NoteColumnSplineMode_Disabled") {
        return Some(0.0);
    }
    if !mode.eq_ignore_ascii_case("NoteColumnSplineMode_Offset") {
        return None;
    }
    if points.is_empty() {
        return Some(0.0);
    }
    let mut y = None::<f32>;
    for [x, point_y] in points.iter().copied() {
        if !x.is_finite() || !point_y.is_finite() {
            return None;
        }
        if let Some(y) = y {
            if (point_y - y).abs() > EPS {
                return None;
            }
        } else {
            y = Some(point_y);
        }
    }
    y
}

#[derive(Debug, Clone)]
pub struct SongLuaColumnOffsetBuildParams {
    pub unit: SongLuaTimeUnit,
    pub start: f32,
    pub limit: f32,
    pub span_mode: SongLuaSpanMode,
    pub easing: Option<String>,
    pub sustain: Option<f32>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

#[must_use]
pub fn column_offset_windows_from_samples(
    from_samples: &[SongLuaColumnOffsetSample],
    to_samples: &[SongLuaColumnOffsetSample],
    params: SongLuaColumnOffsetBuildParams,
) -> Vec<SongLuaColumnOffsetWindow> {
    let mut keys = Vec::<(usize, usize)>::new();
    for sample in from_samples.iter().chain(to_samples.iter()) {
        let key = (sample.player, sample.column);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys.sort_unstable();

    let mut out = Vec::new();
    for (player, column) in keys {
        let from_y = column_offset_sample_y(from_samples, player, column);
        let to_y = column_offset_sample_y(to_samples, player, column);
        if from_y.abs() <= f32::EPSILON && to_y.abs() <= f32::EPSILON {
            continue;
        }
        out.push(SongLuaColumnOffsetWindow {
            unit: params.unit,
            start: params.start,
            limit: params.limit,
            span_mode: params.span_mode,
            player,
            column,
            target: SongLuaColumnTransformTarget::OffsetY,
            from_y,
            to_y,
            easing: params.easing.clone(),
            sustain: params.sustain,
            opt1: params.opt1,
            opt2: params.opt2,
        });
    }
    out
}

#[must_use]
pub fn column_transform_windows_from_samples(
    from_samples: &[SongLuaColumnTransformSample],
    to_samples: &[SongLuaColumnTransformSample],
    params: SongLuaColumnOffsetBuildParams,
) -> Vec<SongLuaColumnOffsetWindow> {
    let mut out = Vec::new();
    append_column_transform_windows_from_samples(&mut out, from_samples, to_samples, params);
    out
}

fn column_transform_sample_key(
    sample: &SongLuaColumnTransformSample,
) -> (usize, usize, SongLuaColumnTransformTarget) {
    (sample.player, sample.column, sample.target)
}

fn take_column_transform_value(
    samples: &mut &[SongLuaColumnTransformSample],
    key: (usize, usize, SongLuaColumnTransformTarget),
) -> f32 {
    let Some(first) = samples
        .first()
        .filter(|sample| column_transform_sample_key(sample) == key)
    else {
        return key.2.baseline();
    };
    let value = first.value;
    // The original lookup used the first occurrence, including duplicate keys.
    while let Some((first, rest)) = samples.split_first() {
        if column_transform_sample_key(first) != key {
            break;
        }
        *samples = rest;
    }
    value
}

pub(crate) fn append_column_transform_windows_from_samples(
    out: &mut Vec<SongLuaColumnOffsetWindow>,
    from_samples: &[SongLuaColumnTransformSample],
    to_samples: &[SongLuaColumnTransformSample],
    params: SongLuaColumnOffsetBuildParams,
) {
    // Capture visits players, columns, and targets in key order. Merge those
    // samples directly, without collecting keys or rescanning each input.
    let ordered = |samples: &[SongLuaColumnTransformSample]| {
        samples.is_sorted_by(|left, right| {
            column_transform_sample_key(left) <= column_transform_sample_key(right)
        })
    };
    if ordered(from_samples) && ordered(to_samples) {
        let mut from = from_samples;
        let mut to = to_samples;
        while !from.is_empty() || !to.is_empty() {
            let key = match (from.first(), to.first()) {
                (Some(left), Some(right)) => {
                    column_transform_sample_key(left).min(column_transform_sample_key(right))
                }
                (Some(sample), None) | (None, Some(sample)) => column_transform_sample_key(sample),
                (None, None) => break,
            };
            let from_y = take_column_transform_value(&mut from, key);
            let to_y = take_column_transform_value(&mut to, key);
            let baseline = key.2.baseline();
            if (from_y - baseline).abs() <= f32::EPSILON && (to_y - baseline).abs() <= f32::EPSILON
            {
                continue;
            }
            out.push(SongLuaColumnOffsetWindow {
                unit: params.unit,
                start: params.start,
                limit: params.limit,
                span_mode: params.span_mode,
                player: key.0,
                column: key.1,
                target: key.2,
                from_y,
                to_y,
                easing: params.easing.clone(),
                sustain: params.sustain,
                opt1: params.opt1,
                opt2: params.opt2,
            });
        }
        return;
    }
    // Public callers may supply arbitrary order. Retain their first-write and
    // sorted-output semantics using the existing fallback.
    let mut keys = Vec::<(usize, usize, SongLuaColumnTransformTarget)>::new();
    for sample in from_samples.iter().chain(to_samples.iter()) {
        let key = (sample.player, sample.column, sample.target);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys.sort_unstable();

    for (player, column, target) in keys {
        let from_y = column_transform_sample_value(from_samples, player, column, target);
        let to_y = column_transform_sample_value(to_samples, player, column, target);
        let baseline = target.baseline();
        if (from_y - baseline).abs() <= f32::EPSILON && (to_y - baseline).abs() <= f32::EPSILON {
            continue;
        }
        out.push(SongLuaColumnOffsetWindow {
            unit: params.unit,
            start: params.start,
            limit: params.limit,
            span_mode: params.span_mode,
            player,
            column,
            target,
            from_y,
            to_y,
            easing: params.easing.clone(),
            sustain: params.sustain,
            opt1: params.opt1,
            opt2: params.opt2,
        });
    }
}

fn column_transform_sample_value(
    samples: &[SongLuaColumnTransformSample],
    player: usize,
    column: usize,
    target: SongLuaColumnTransformTarget,
) -> f32 {
    samples
        .iter()
        .find(|sample| {
            sample.player == player && sample.column == column && sample.target == target
        })
        .map_or_else(|| target.baseline(), |sample| sample.value)
}

fn column_offset_sample_y(
    samples: &[SongLuaColumnOffsetSample],
    player: usize,
    column: usize,
) -> f32 {
    samples
        .iter()
        .find(|sample| sample.player == player && sample.column == column)
        .map_or(0.0, |sample| sample.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_deceleration_keeps_scalar_float_rounding() {
        // Retained native Player tween: duration 0.3, Y816 -> Y528.
        let percent = 1.0_f32 - 0.2640790343284607_f32 / 0.3_f32;
        let factor = super::overlay_command_ease_factor(Some("outQuad"), percent, None);
        assert_eq!(factor, 0.22513633966445923_f32);
        let mut pose = super::SongLuaOverlayState {
            y: 816.0,
            ..Default::default()
        };
        let target = super::SongLuaOverlayStateDelta {
            y: Some(528.0),
            ..Default::default()
        };
        super::overlay_state_lerp(&mut pose, &target, factor);
        assert_eq!(pose.y, 751.1607666015625_f32);
    }

    #[test]
    fn theme_metric_number_uses_screen_and_player_count() {
        assert_eq!(
            theme_metric_number_for_screen(
                "Player",
                "DrawDistanceBeforeTargetsPixels",
                1,
                1280.0,
                720.0
            ),
            Some(1080.0)
        );
        assert_eq!(
            theme_metric_number_for_screen("GraphDisplay", "BodyWidth", 1, 640.0, 480.0),
            Some(610.0)
        );
        assert_eq!(
            theme_metric_number_for_screen("GraphDisplay", "BodyWidth", 2, 854.0, 480.0),
            Some(300.0)
        );
        for (width, expected) in [
            (480.0, [80.0, 400.0]),
            (640.0, [160.0, 480.0]),
            (854.0, [213.5, 640.5]),
            (1280.0, [426.5, 853.5]),
        ] {
            for (player, x) in expected.into_iter().enumerate() {
                for style in super::THEME_PLAYER_X_STYLES {
                    let side = matches!(style, "OnePlayerOneSide" | "TwoPlayersTwoSides");
                    assert_eq!(
                        theme_metric_number_for_screen(
                            "ScreenGameplay",
                            &format!("PlayerP{}{style}X", player + 1),
                            2,
                            width,
                            480.0
                        ),
                        Some(if side { x } else { width * 0.5 })
                    );
                }
            }
        }
    }
}

pub(crate) const PLAYER_SONG_POSITIONS_KEY: &str = "__songlua_player_positions";
