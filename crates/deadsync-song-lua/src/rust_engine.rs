//! Song host integration with the shared Rust Lua engine.
//!
//! This portion owns song clock, RNG, file loading, actor declarations and the
//! Actor/ActorFrame/ActorFrameTexture/Quad/Sprite/Banner/Sound/BitmapText/ActorMultiVertex/ActorProxy,
//! GraphDisplay/SongMeterDisplay/CourseContentsList/Model and text-display/wrapper bindings.
//! Production routing uses this engine for preparation and live scenes.
//! Legacy implementation isolation and final application validation are tracked
//! by the fixed production migration objective.
use crate::{
    LUA_PLAYERS, SONG_LUA_RUNTIME_BEAT_KEY as BEAT, SONG_LUA_RUNTIME_BPS_KEY as BPS,
    SONG_LUA_RUNTIME_DELTA_BEAT_KEY as DELTA_BEAT,
    SONG_LUA_RUNTIME_DELTA_SECONDS_KEY as DELTA_SECONDS, SONG_LUA_RUNTIME_KEY,
    SONG_LUA_RUNTIME_RATE_KEY as RATE, SONG_LUA_RUNTIME_SECONDS_KEY as SECONDS,
    SongLuaCompileContext, song_display_bps, song_elapsed_seconds_at, song_music_rate,
};
use deadlib_lua::{Error, HostCall, Root, TableId, Value, Vm};
mod actors;
#[path = "rust_engine/input/program/mod.rs"]
pub mod baked_program;
mod compiler;
pub use compiler::{compile_prepared_song_lua_layers, compile_song_lua_layers, reload_song_lua};
mod compiler_debug;
mod definitions;
mod function_snapshot;
mod gameplay;
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
pub use crate::live::SongLuaLivePlayerWrite;
pub use options::PlayerStateStatus;
mod random;
pub use random::RandomSnapshot;
#[cfg(feature = "test-support")]
pub mod test_support;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

const FREEZE: &str = "__songlua_freeze";
const DELAY: &str = "__songlua_delay";
const BPMS: &str = "__songlua_timing_bpms";
const PLAYERS: &str = crate::runtime::PLAYER_SONG_POSITIONS_KEY;

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
pub struct RustSongRuntime {
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

impl RustSongRuntime {
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
