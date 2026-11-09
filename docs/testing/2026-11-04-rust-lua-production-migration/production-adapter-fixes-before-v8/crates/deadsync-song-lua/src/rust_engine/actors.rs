//! Owned actor instances and command execution over the Rust host API.
use crate::tween_pose::TweenPose;
mod assets;
mod banners;
mod capture;
mod initial_state;
mod input_snapshot;
pub use input_snapshot::{CompiledActorInputs, CompiledActorInteractions};
pub(super) mod perframes;
mod scalar_methods;
mod song_activation;
mod song_capture;
mod song_clock;
mod song_getters;
mod song_immediate;
mod song_layers;
mod song_messages;
mod song_multitap;
pub(super) mod update_options;
#[cfg(feature = "test-support")]
pub(super) use song_immediate::TARGETS as IMMEDIATE_CAPTURE_TARGETS;
#[cfg(feature = "test-support")]
pub(super) use song_properties::TARGETS as SCHEDULED_RENDER_TARGETS;
mod song_replay;
pub use capture::ActorCommandCapture;
mod courses;
mod diagnostics;
mod draw;
mod effects;
mod framebuffers;
pub(super) mod gameplay_theme;
mod generated_players;
mod graphs;
mod host_compat;
mod loading;
mod native_children;
mod notefields;
mod offline;
mod player_parts;
mod screen_assembly;
mod screen_parts;
mod song_meters;
mod splines;
mod steps_displays;
mod top_screen;
pub use framebuffers::FrameTexture;
pub use graphs::GraphDisplay;
pub use song_meters::SongMeterDisplay;
mod gameplay_bitmaps;
mod lifecycle;
mod live_epoch;
mod live_feedback;
mod live_frames;
pub(super) mod live_gameplay;
pub(super) mod live_layers;
#[cfg(feature = "test-support")]
mod live_observation;
pub(super) mod live_scene;
mod live_sources;
mod live_visuals;
mod meshes;
mod messages;
mod models;
mod proxies;
mod transitions;
mod underlay;
pub use meshes::ActorMesh;
mod recurring;
mod render;
mod retained;
mod retirement;
mod song_media;
#[cfg(feature = "test-support")]
pub(super) use song_media::TARGETS as MEDIA_CAPTURE_TARGETS;
mod action_capture;
mod column_output;
mod command_locals;
pub(super) mod compiler;
pub(super) mod compiler_tables;
pub(super) mod ease_probe;
mod function_action;
mod function_capture;
mod song_columns;
pub(super) mod song_kinds;
mod song_output;
mod song_pose;
mod song_properties;
pub(super) mod song_tree;
mod sound;
mod sound_manager;
mod sprites;
mod state_snapshot;
mod text;
mod text_displays;
mod textures;
pub use text::BitmapText;
pub(in crate::rust_engine) mod chronological;
mod wrappers;
use deadlib_lua::{Error, ErrorKind, HostCall, Root, TableId, Value, Vm, compile_named};
use lifecycle::{Lifecycle, TweenStep};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};

/// A scene owned by one Rust song runtime. Release it or transfer it to Lua
/// ownership with `retire_actor_scene` after its host lifecycle ends.
/// Copies identify the same scene; they do not duplicate its actors or callbacks.
#[derive(Clone, Copy, Debug)]
pub struct ActorScene {
    // The permanent constructor root includes VM identity and generation.
    owner: Root,
    id: u64,
}

/// Native fields currently implemented by the actor host. Lua instance fields
/// are separate: assigning `self.Name` or `self.x` does not change these values.
#[derive(Clone, Debug, PartialEq)]
pub struct ActorSnapshot {
    pub class: &'static str,
    pub name: Vec<u8>,
    /// Index within this scene; an external foreground parent has no local index.
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// Wrapper membership in creation order, separate from ActorFrame children.
    pub wrappers: Vec<usize>,
    pub position: [f32; 3],
    pub zoom: [f32; 3],
    pub rotation: [f32; 3],
    pub size: [f32; 2],
    pub visible: bool,
    pub draw_order: i32,
    pub aux: f32,
    /// Current local render properties, before ancestor composition or effects.
    pub render: crate::SongLuaOverlayState,
}
struct Actor {
    scene: u64,
    raw_live_epoch: bool,
    state: ActorSnapshot,
    base_transform: crate::base_transform::BaseTransform,
    pose_exported: bool,
    current: TweenPose,
    queue: VecDeque<TweenStep>,
    replay: Option<song_replay::State>,
    capture: Option<capture::Capture>,
    preparation_getters: Option<[f32; 4]>,
    hibernate: f32,
    update_rate: f32,
    started: bool,
    update_error_reported: bool,
    load_name: Option<Vec<u8>>,
    parent: Option<TableId>,
    children: Vec<TableId>,
    wrappers: Vec<TableId>,
    active_commands: HashSet<Vec<u8>>,
    propagate: bool,
    effects: effects::State,
    sound: Option<TableId>,
    offline_template: bool,
    offline_instance: bool,
    initialized: bool,
    pending_init: bool,
    registered: bool,
    native_children: bool,
    raw_instance: bool,
    model_materials: Vec<Option<crate::model_texture::Clock>>,
}
struct TemplateState {
    state: ActorSnapshot,
    base_transform: crate::base_transform::BaseTransform,
    pose_exported: bool,
    pose: TweenPose,
    hibernate: f32,
    update_rate: f32,
    propagate: bool,
    update_error_reported: bool,
    effects: effects::State,
    capture: Option<capture::Capture>,
    preparation_getters: Option<[f32; 4]>,
}
impl From<&Actor> for TemplateState {
    fn from(actor: &Actor) -> Self {
        Self {
            state: actor.state.clone(),
            base_transform: actor.base_transform,
            pose_exported: actor.pose_exported,
            pose: *actor.destination(),
            hibernate: actor.hibernate,
            update_rate: actor.update_rate,
            propagate: actor.propagate,
            update_error_reported: actor.update_error_reported,
            effects: actor.effects,
            capture: actor.capture.clone(),
            preparation_getters: actor.preparation_getters,
        }
    }
}
impl Actor {
    fn new(
        scene: u64,
        class: &'static str,
        parent: Option<TableId>,
        load_name: Option<Vec<u8>>,
    ) -> Self {
        Self {
            scene,
            raw_live_epoch: false,
            base_transform: Default::default(),
            pose_exported: false,
            load_name,
            current: crate::SongLuaOverlayState::default().into(),
            queue: VecDeque::new(),
            replay: None,
            capture: None,
            preparation_getters: None,
            hibernate: 0.0,
            update_rate: 1.0,
            started: false,
            update_error_reported: false,
            state: ActorSnapshot {
                class,
                name: Vec::new(),
                parent: None,
                children: Vec::new(),
                wrappers: Vec::new(),
                position: [0.; 3],
                zoom: [1.; 3],
                rotation: [0.; 3],
                size: [1.; 2],
                visible: true,
                draw_order: 0,
                aux: 0.,
                render: crate::SongLuaOverlayState::default(),
            },
            parent,
            children: Vec::new(),
            wrappers: Vec::new(),
            active_commands: HashSet::new(),
            propagate: false,
            effects: effects::State::default(),
            sound: None,
            offline_template: false,
            offline_instance: false,
            initialized: false,
            pending_init: false,
            registered: true,
            native_children: false,
            raw_instance: false,
            model_materials: Vec::new(),
        }
    }
    fn sync_rotation_getters(&mut self) {
        self.pose_exported = true;
        if self.capture.is_none() {
            self.preparation_getters = None;
            if let Some(replay) = &mut self.replay {
                replay.getters = None;
            }
        }
        let pose = self.destination();
        self.state.rotation = [pose.rot_x_deg, pose.rot_y_deg, pose.rot_z_deg];
    }
    fn getter_destination(&self) -> &TweenPose {
        self.replay
            .as_ref()
            .and_then(|replay| replay.getters.as_ref())
            .unwrap_or_else(|| self.destination())
    }
    fn destination(&self) -> &TweenPose {
        self.queue.back().map_or(&self.current, |step| &step.to)
    }
    fn destination_mut(&mut self) -> &mut TweenPose {
        self.queue
            .back_mut()
            .map_or(&mut self.current, |step| &mut step.to)
    }
}
struct Scene {
    actors: Vec<TableId>,
    root: Option<TableId>,
    ended: bool,
    sounds_retired: bool,
    initial_states: Option<super::song_layers::SongInitialStates>,
    song_session: Option<song_activation::Session>,
}
#[derive(Default)]
struct Store {
    clock: Arc<super::Clock>,
    playback: bool,
    replay_frame: Option<(u64, f64)>,
    replay_dispatch: Option<song_immediate::Dispatch>,
    replay_advancing: bool,
    replay_clock: Option<song_clock::State>,
    constructing_capture: bool,
    defer_registration: bool,
    preparing: bool,
    startup_queues: Option<Vec<TableId>>,
    startup_discovery: bool,
    capture_scope: Option<f32>,
    ease_probe_active: bool,
    command_locals: Option<command_locals::Scope>,
    action_capture: Option<action_capture::State>,
    deferred_messages: Vec<messages::Deferred>,
    song_broadcasts: Option<song_capture::Broadcasts>,
    live_sources: Option<Vec<crate::SongLuaLiveBroadcast>>,
    song_callback_depth: usize,
    trace_dynamic_commands: bool,
    next: u64,
    actors: HashMap<TableId, Actor>,
    actor_order: Vec<TableId>,
    scenes: HashMap<u64, Scene>,
    retired_scenes: HashMap<u64, TableId>,
    retired_subscriptions: HashSet<TableId>,
    collected_scenes: Arc<Mutex<Vec<(u64, TableId)>>>,
    constructor_scenes: HashSet<u64>,
    offline_definitions: bool,
    song_dir: std::path::PathBuf,
    diagnostics: Vec<String>,
    callback_errors: Vec<String>,
    #[cfg(feature = "test-support")]
    observations: super::observations::Observations,
    #[cfg(feature = "test-support")]
    audit_update_command: bool,
    #[cfg(feature = "test-support")]
    audit_queued_depth: usize,
    sounds: Vec<crate::SongLuaActorSound>,
    prepared_sounds: Vec<crate::SongLuaSoundEvent>,
    sprite_textures: Vec<(u64, crate::SongLuaSpriteTexture)>,
    judgment_textures: Vec<(usize, crate::SongLuaJudgmentTexture)>,
    sound_capture: Option<bool>,
    active_message: Option<String>,
    mesh_output: Option<ActorMesh>,
    text_output: Option<BitmapText>,
    graph_output: Option<GraphDisplay>,
    meter_output: Option<SongMeterDisplay>,
    model_paths_output: Option<[std::path::PathBuf; 3]>,
    graph_size: [f32; 2],
    proxy_output: Option<crate::SongLuaProxyTarget>,
    framebuffer_output: Option<FrameTexture>,
    draw_capture: Option<crate::draw_commands::Capture>,
    multitap_plan: Option<Arc<song_multitap::Plan>>,
    update_plans: HashMap<TableId, Arc<lifecycle::Plan>>,
    scalar_methods: scalar_methods::Methods,
    #[cfg(feature = "test-support")]
    message_ranks: HashMap<TableId, usize>,
    #[cfg(feature = "test-support")]
    message_paths: HashMap<String, usize>,
    #[cfg(feature = "test-support")]
    message_coverage: Vec<String>,
    #[cfg(feature = "test-support")]
    message_coverage_seen: HashSet<String>,
}
type Shared = Arc<Mutex<Store>>;

#[derive(Clone, Copy)]
enum Op {
    PrepareDefinitions,
    LoadPreparationActor,
    OfflineConstructor(&'static str),
    Instantiate,
    Construct,
    Initialize,
    SetParent,
    Append,
    DiscoverStartup,
    Template,
    LoadFont,
    CloneTemplate,
    Release,
    Retire,
    Start,
    Advance,
    End,
    Lifecycle(Lifecycle),
    Message(messages::MessageOp),
    Render(&'static str),
    Effect(&'static str),
    Wrapper(&'static str),
    Sound(&'static str),
    Sprite(&'static str),
    Banner(&'static str),
    Mesh(&'static str),
    ReadMesh,
    Text(&'static str),
    TextDisplay(&'static str),
    ReadText,
    Graph(&'static str),
    ReadGraph,
    SongMeter(&'static str),
    Course(&'static str),
    ReadSongMeter,
    ReadModelPaths,
    ReadProxy,
    Framebuffer(&'static str),
    ReadFramebuffer,
    Draw(&'static str),
    DrawScene,
    Name,
    GetName,
    GetParent,
    GetChildren,
    GetChild,
    GetNumChildren,
    Sort,
    AddCommand,
    GetCommand,
    PlayCommand,
    PropagateCommand,
    Recursive,
    OnChildren,
    OnLeaves,
    NamedChildren,
    NamedLeaves,
    Propagate,
    Set(&'static str),
    Get(&'static str),
}
#[derive(Clone)]
struct Binding {
    store: Shared,
    op: Op,
}

pub(super) struct Actors {
    registry: TableId,
    helpers: TableId,
    store: Shared,
    prepare_definitions: Root,
    instantiate: Root,
    construct: Root,
    initialize: Root,
    set_parent: Root,
    append: Root,
    template: Root,
    release: Root,
    retire: Root,
    start: Root,
    advance: Root,
    end: Root,
    broadcast: Root,
    read_mesh: Root,
    read_proxy: Root,
    read_framebuffer: Root,
    draw_scene: Root,
    read_text: Root,
    read_graph: Root,
    read_meter: Root,
    read_model_paths: Root,
    discover_startup: Root,
}
impl Actors {
    pub(super) fn install(
        vm: &mut Vm,
        clock: Arc<super::Clock>,
        players: usize,
        #[cfg(feature = "test-support")] observations: super::observations::Observations,
    ) -> Result<Self, Error> {
        let store = Arc::new(Mutex::new(Store {
            clock,
            graph_size: crate::graph_display_body_size(players),
            #[cfg(feature = "test-support")]
            observations,
            ..Store::default()
        }));
        let registry = vm.try_new_table()?;
        let registry_root = vm.pin(Value::Table(registry))?;
        // Every callback captures this private table; it owns all Lua actor
        // values until release, including commands stored on the instances.
        // Rust metadata contains no untraced owner.
        let helpers = vm.try_new_table()?;
        let helpers_root = vm.pin(Value::Table(helpers))?;
        let captures = [Value::Table(registry), Value::Table(helpers)];
        let methods = vm.try_new_table()?;
        let methods_root = vm.pin(Value::Table(methods))?;
        let actor = vm.try_new_table()?;
        super::put(vm, methods, "Actor", Value::Table(actor))?;
        let frame = vm.try_new_table()?;
        super::put(vm, methods, "ActorFrame", Value::Table(frame))?;
        let sprite = vm.try_new_table()?;
        super::put(vm, methods, "Sprite", Value::Table(sprite))?;
        let banner = vm.try_new_table()?;
        super::put(vm, methods, "Banner", Value::Table(banner))?;
        let mesh = vm.try_new_table()?;
        super::put(vm, methods, "ActorMultiVertex", Value::Table(mesh))?;
        let bitmap = vm.try_new_table()?;
        super::put(vm, methods, "BitmapText", Value::Table(bitmap))?;
        let framebuffer = vm.try_new_table()?;
        super::put(vm, methods, "ActorFrameTexture", Value::Table(framebuffer))?;
        scalar_methods::install_metatable(vm, methods, &store)?;
        for (name, op) in [
            ("name", Op::Name),
            ("GetName", Op::GetName),
            ("GetParent", Op::GetParent),
            ("addcommand", Op::AddCommand),
            ("GetCommand", Op::GetCommand),
            ("playcommand", Op::PlayCommand),
            ("RunCommandsRecursively", Op::Recursive),
        ] {
            install_method(vm, actor, name, op, &store, &captures)?;
        }
        for name in [
            "x",
            "y",
            "z",
            "xy",
            "addx",
            "addy",
            "addz",
            "zoom",
            "zoomx",
            "zoomy",
            "zoomz",
            "rotationx",
            "rotationy",
            "rotationz",
            "addrotationx",
            "addrotationy",
            "addrotationz",
            "SetWidth",
            "SetHeight",
            "setsize",
            "aux",
            "visible",
            "draworder",
        ] {
            if let Some(setter) = scalar_methods::Setter::from_name(name) {
                scalar_methods::install(vm, actor, setter, &store, &captures)?;
            } else {
                install_method(vm, actor, name, Op::Set(name), &store, &captures)?;
            }
        }
        for name in [
            "GetX",
            "GetY",
            "GetZ",
            "GetDestX",
            "GetDestY",
            "GetDestZ",
            "GetZoom",
            "GetZoomX",
            "GetZoomY",
            "GetZoomZ",
            "GetRotationX",
            "GetRotationY",
            "GetRotationZ",
            "GetWidth",
            "GetHeight",
            "GetZoomedWidth",
            "GetZoomedHeight",
            "getaux",
            "GetVisible",
        ] {
            install_method(vm, actor, name, Op::Get(name), &store, &captures)?;
        }
        install_method(vm, actor, "SetSize", Op::Set("setsize"), &store, &captures)?;
        for (name, op) in [
            ("GetChild", Op::GetChild),
            ("GetChildren", Op::GetChildren),
            ("GetNumChildren", Op::GetNumChildren),
            ("SortByDrawOrder", Op::Sort),
            ("RunCommandsOnChildren", Op::OnChildren),
            ("runcommandsonleaves", Op::OnLeaves),
            ("playcommandonchildren", Op::NamedChildren),
            ("playcommandonleaves", Op::NamedLeaves),
            ("propagate", Op::Propagate),
            ("propagatecommand", Op::PropagateCommand),
        ] {
            install_method(vm, frame, name, op, &store, &captures)?;
        }
        for &(name, operation) in lifecycle::ACTOR_METHODS {
            install_method(vm, actor, name, Op::Lifecycle(operation), &store, &captures)?;
        }
        for &(name, operation) in lifecycle::FRAME_METHODS {
            install_method(vm, frame, name, Op::Lifecycle(operation), &store, &captures)?;
        }
        install_method(vm, actor, "Draw", Op::Draw("Draw"), &store, &captures)?;
        for name in ["SetDrawFunction", "GetDrawFunction"] {
            install_method(vm, frame, name, Op::Draw(name), &store, &captures)?;
        }
        for &name in render::ACTOR_METHODS {
            install_method(vm, actor, name, Op::Render(name), &store, &captures)?;
        }
        for &name in render::FRAME_METHODS {
            install_method(vm, frame, name, Op::Render(name), &store, &captures)?;
        }
        install_method(vm, frame, "SetFOV", Op::Render("fov"), &store, &captures)?;
        for &name in effects::METHODS {
            install_method(vm, actor, name, Op::Effect(name), &store, &captures)?;
        }
        for name in ["AddWrapperState", "GetWrapperState", "GetNumWrapperStates"] {
            install_method(vm, actor, name, Op::Wrapper(name), &store, &captures)?;
        }
        for name in ["play", "pause"] {
            install_method(vm, actor, name, Op::Sound(name), &store, &captures)?;
        }
        for &name in sprites::ACTOR_METHODS {
            install_method(vm, actor, name, Op::Sprite(name), &store, &captures)?;
        }
        for &name in sprites::SPRITE_METHODS {
            install_method(vm, sprite, name, Op::Sprite(name), &store, &captures)?;
        }
        for &name in banners::METHODS {
            install_method(vm, banner, name, Op::Banner(name), &store, &captures)?;
        }
        for &name in banners::SPRITE_METHODS {
            install_method(vm, sprite, name, Op::Banner(name), &store, &captures)?;
        }
        for &name in meshes::METHODS {
            install_method(vm, mesh, name, Op::Mesh(name), &store, &captures)?;
        }
        for &name in framebuffers::METHODS {
            install_method(
                vm,
                framebuffer,
                name,
                Op::Framebuffer(name),
                &store,
                &captures,
            )?;
        }
        for &name in text::METHODS {
            install_method(vm, bitmap, name, Op::Text(name), &store, &captures)?;
        }
        let displays = vm.try_new_table()?;
        super::put(vm, helpers, "text_display_methods", Value::Table(displays))?;
        for &name in text::METHODS {
            install_method(vm, displays, name, Op::Text(name), &store, &captures)?;
        }
        for &name in text_displays::METHODS {
            install_method(vm, displays, name, Op::TextDisplay(name), &store, &captures)?;
        }
        let graph_methods = vm.try_new_table()?;
        super::put(vm, helpers, "graph_methods", Value::Table(graph_methods))?;
        for name in ["Load", "Set"] {
            install_method(vm, graph_methods, name, Op::Graph(name), &store, &captures)?;
        }
        let meter_methods = vm.try_new_table()?;
        super::put(vm, helpers, "meter_methods", Value::Table(meter_methods))?;
        for name in ["Load", "SetStreamWidth", "GetStreamWidth"] {
            install_method(
                vm,
                meter_methods,
                name,
                Op::SongMeter(name),
                &store,
                &captures,
            )?;
        }
        let course_methods = vm.try_new_table()?;
        super::put(vm, helpers, "course_methods", Value::Table(course_methods))?;
        for &name in courses::METHODS {
            install_method(
                vm,
                course_methods,
                name,
                Op::Course(name),
                &store,
                &captures,
            )?;
        }
        let model_methods = vm.try_new_table()?;
        super::put(vm, helpers, "model_methods", Value::Table(model_methods))?;
        for &name in sprites::SPRITE_METHODS {
            install_method(vm, model_methods, name, Op::Sprite(name), &store, &captures)?;
        }
        for (name, op) in [
            ("GetChild", Op::GetChild),
            ("GetChildren", Op::GetChildren),
            ("GetNumChildren", Op::GetNumChildren),
        ] {
            install_method(vm, model_methods, name, op, &store, &captures)?;
        }
        install_method(
            vm,
            methods,
            "CloneTemplate",
            Op::CloneTemplate,
            &store,
            &captures,
        )?;
        let load_font = vm.new_host_function_with_data(
            callback,
            &captures,
            Binding {
                store: store.clone(),
                op: Op::LoadFont,
            },
        )?;
        vm.set_global(b"LoadFont", load_font)?;
        let color = vm.new_host_function(text::global_color, &captures)?;
        vm.set_global(b"color", color)?;
        let current_background = compile_named(
            banners::CURRENT_BACKGROUND,
            b"@theme:/Scripts/02 Sprite.lua",
        )?;
        let current_background = vm.execute(&current_background)?[0];
        super::put(vm, methods, "CurrentSongBackground", current_background)?;
        let cropto = compile_named(
            format!(
                "{}return function(self,w,h) self:CropTo(w,h); return self end",
                "\n".repeat(74)
            )
            .as_bytes(),
            b"_fallback/Scripts/02 Sprite.lua",
        )?;
        let cropto = vm.execute(&cropto)?[0];
        super::put(vm, methods, "CropToHelper", cropto)?;
        let setup = compile_named(
            include_bytes!("actors.lua"),
            b"=DeadSync actor instance bindings",
        )?;
        let setup = vm.execute(&setup)?[0];
        let values = vm.call(setup, &[Value::Table(methods)])?.to_vec();
        for (i, value) in values.into_iter().enumerate() {
            if i == 7 {
                // Numeric slot eight belongs to MessageManager. New private
                // resources use named entries so its installation cannot replace them.
                super::put(vm, helpers, meshes::BUFFERS_KEY, value)?;
            } else if i == 8 {
                super::put(vm, helpers, "setmetatable", value)?;
            } else {
                vm.raw_table_set(helpers, Value::Number((i + 1) as f64), value)?;
            }
        }
        native_children::install_helpers(vm, actor, helpers)?;
        state_snapshot::install(vm, helpers)?;
        function_action::install(vm, helpers)?;
        song_columns::install(vm, helpers)?;
        let broadcast = messages::install(vm, &store, &captures)?;
        let instantiate = vm.new_host_function_with_data(
            callback,
            &captures,
            Binding {
                store: store.clone(),
                op: Op::Instantiate,
            },
        )?;
        let instantiate = vm.pin(instantiate)?;
        let template = vm.new_host_function_with_data(
            callback,
            &captures,
            Binding {
                store: store.clone(),
                op: Op::Template,
            },
        )?;
        let template = vm.pin(template)?;
        let release = vm.new_host_function_with_data(
            callback,
            &captures,
            Binding {
                store: store.clone(),
                op: Op::Release,
            },
        )?;
        let release = vm.pin(release)?;
        let retire = vm.new_host_function_with_data(
            callback,
            &captures,
            Binding {
                store: store.clone(),
                op: Op::Retire,
            },
        )?;
        let retire = vm.pin(retire)?;
        let mut phases = Vec::new();
        for op in [
            Op::Start,
            Op::Advance,
            Op::End,
            Op::ReadMesh,
            Op::ReadProxy,
            Op::ReadFramebuffer,
            Op::DrawScene,
            Op::ReadText,
            Op::ReadGraph,
            Op::ReadSongMeter,
            Op::ReadModelPaths,
            Op::DiscoverStartup,
            Op::Construct,
            Op::Initialize,
            Op::SetParent,
            Op::Append,
            Op::PrepareDefinitions,
        ] {
            let function = vm.new_host_function_with_data(
                callback,
                &captures,
                Binding {
                    store: store.clone(),
                    op,
                },
            )?;
            phases.push(vm.pin(function)?);
        }
        vm.unpin(methods_root)?;
        vm.unpin(helpers_root)?;
        vm.unpin(registry_root)?;
        let actors = Self {
            registry,
            helpers,
            store,
            instantiate,
            template,
            release,
            retire,
            start: phases[0],
            advance: phases[1],
            end: phases[2],
            read_mesh: phases[3],
            read_proxy: phases[4],
            read_framebuffer: phases[5],
            draw_scene: phases[6],
            read_text: phases[7],
            read_graph: phases[8],
            read_meter: phases[9],
            read_model_paths: phases[10],
            discover_startup: phases[11],
            construct: phases[12],
            initialize: phases[13],
            set_parent: phases[14],
            append: phases[15],
            prepare_definitions: phases[16],
            broadcast,
        };
        // The existing production host owns a live, reusable NullActor before
        // loading authored scripts. Each use in a declaration is instantiated
        // separately, while the template itself remains a message subscriber.
        let declaration = vm.get_global(b"NullActor")?;
        let null = actors.instantiate(vm, declaration)?;
        vm.set_global(b"NullActor", actors.root(null).expect("owned null actor"))?;
        Ok(actors)
    }
    /// Match the native registry replacement at a screen reconstruction boundary.
    /// Existing scene owners remain valid until the session releases them.
    pub(in crate::rust_engine) fn clear_message_subscriptions(
        &self,
        vm: &mut Vm,
    ) -> Result<(), Error> {
        retirement::sweep(&self.store);
        // A wrapper created through a retained old actor joins the current
        // native registry. Keep that root until this replacement boundary.
        let late: Vec<_> = self
            .store
            .lock()
            .unwrap()
            .retired_subscriptions
            .iter()
            .copied()
            .collect();
        for actor in late {
            let active_owner = {
                let guard = self.store.lock().unwrap();
                guard.scenes.iter().any(|(id, scene)| {
                    !guard.retired_scenes.contains_key(id) && scene.actors.contains(&actor)
                })
            };
            if !active_owner {
                vm.raw_table_set(self.registry, Value::Table(actor), Value::Nil)?;
            }
            self.store
                .lock()
                .unwrap()
                .retired_subscriptions
                .remove(&actor);
        }
        self.store.lock().unwrap().actor_order.clear();
        Ok(())
    }

    pub(super) fn bind_source_loader(
        &self,
        vm: &mut Vm,
        function: Value,
        song_dir: &std::path::Path,
    ) -> Result<(), Error> {
        super::put(vm, self.helpers, "source_loader", function)?;
        self.store.lock().unwrap().song_dir = song_dir.to_owned();
        Ok(())
    }
    pub(super) fn instantiate(&self, vm: &mut Vm, declaration: Value) -> Result<ActorScene, Error> {
        self.materialize(vm, declaration, self.instantiate)
    }
    pub(super) fn construct(&self, vm: &mut Vm, declaration: Value) -> Result<ActorScene, Error> {
        self.materialize(vm, declaration, self.construct)
    }
    fn materialize(
        &self,
        vm: &mut Vm,
        declaration: Value,
        function: Root,
    ) -> Result<ActorScene, Error> {
        let function = vm.rooted(function)?;
        let Value::Number(id) = vm.call(function, &[declaration])?[0] else {
            unreachable!()
        };
        Ok(ActorScene {
            owner: self.instantiate,
            id: id as u64,
        })
    }
    pub(super) fn root(&self, scene: ActorScene) -> Option<Value> {
        if scene.owner != self.instantiate {
            return None;
        }
        retirement::sweep(&self.store);
        let store = self.store.lock().unwrap();
        if store.retired_scenes.contains_key(&scene.id) {
            return None;
        }
        store
            .scenes
            .get(&scene.id)
            .map(|s| s.root.map_or(Value::Nil, Value::Table))
    }
    pub(super) fn release(&self, vm: &mut Vm, scene: ActorScene) -> Result<bool, Error> {
        if scene.owner != self.instantiate {
            return Ok(false);
        }
        let function = vm.rooted(self.release)?;
        let released =
            vm.call(function, &[Value::Number(scene.id as f64)])?[0] == Value::Boolean(true);
        if released {
            self.clear_replay_clock(scene);
        }
        Ok(released)
    }
    pub(super) fn retire(&self, vm: &mut Vm, scene: ActorScene) -> Result<bool, Error> {
        if scene.owner != self.instantiate {
            return Ok(false);
        }
        let function = vm.rooted(self.retire)?;
        let retired =
            vm.call(function, &[Value::Number(scene.id as f64)])?[0] == Value::Boolean(true);
        if retired {
            self.clear_replay_clock(scene);
        }
        Ok(retired)
    }

    fn clear_replay_clock(&self, scene: ActorScene) {
        let mut store = self.store.lock().unwrap();
        if store
            .replay_frame
            .is_some_and(|(owner, _)| owner == scene.id)
        {
            store.replay_clock = None;
        }
    }

    pub(super) fn initialize(&self, vm: &mut Vm, scene: ActorScene) -> Result<bool, Error> {
        self.phase(vm, scene, self.initialize, 0.0)
    }
    pub(super) fn parent(
        &self,
        vm: &mut Vm,
        scene: ActorScene,
        parent: Option<Value>,
        append: bool,
    ) -> Result<bool, Error> {
        if scene.owner != self.instantiate {
            return Ok(false);
        }
        let function = vm.rooted(if append { self.append } else { self.set_parent })?;
        Ok(vm.call(
            function,
            &[Value::Number(scene.id as f64), parent.unwrap_or(Value::Nil)],
        )?[0]
            == Value::Boolean(true))
    }
    pub(super) fn start(&self, vm: &mut Vm, scene: ActorScene) -> Result<bool, Error> {
        self.phase(vm, scene, self.start, 0.0)
    }
    pub(super) fn end(&self, vm: &mut Vm, scene: ActorScene) -> Result<bool, Error> {
        self.phase(vm, scene, self.end, 0.0)
    }
    pub(super) fn advance(
        &self,
        vm: &mut Vm,
        scene: ActorScene,
        delta: f64,
    ) -> Result<bool, Error> {
        self.phase(vm, scene, self.advance, delta)
    }
    pub(super) fn draw(&self, vm: &mut Vm, scene: ActorScene) -> Result<bool, Error> {
        self.phase(vm, scene, self.draw_scene, 0.0)
    }
    pub(super) fn take_draw_frames(
        &self,
        vm: &mut Vm,
        sources: &[(Value, crate::SongLuaDrawSource)],
    ) -> Result<Vec<(usize, crate::SongLuaDrawFrame)>, String> {
        let bindings = self.draw_sources(vm, sources)?;
        let Some(capture) = self.store.lock().unwrap().draw_capture.take() else {
            return Ok(Vec::new());
        };
        crate::draw_commands::convert(capture.frames, &capture.labels, &bindings)
    }
    pub(in crate::rust_engine) fn take_live_draw_frames(
        &self,
        vm: &mut Vm,
        sources: &[(Value, crate::SongLuaDrawSource)],
    ) -> Result<Vec<crate::SongLuaDrawFrame>, String> {
        if self
            .store
            .lock()
            .unwrap()
            .draw_capture
            .as_ref()
            .is_none_or(|capture| {
                !capture.frames.iter().any(|frame| frame.immediate) || capture.context.is_some()
            })
        {
            return Ok(Vec::new());
        }
        let bindings = self.draw_sources(vm, sources)?;
        let mut capture = self.store.lock().unwrap().draw_capture.take().unwrap();
        let immediate = capture
            .frames
            .extract_if(.., |frame| frame.immediate)
            .collect();
        let result = crate::draw_commands::convert(immediate, &capture.labels, &bindings);
        if capture.frames.is_empty() && capture.calls.is_empty() && capture.targets.is_empty() {
            capture.bytes = 0;
            capture.frame_ops = 0;
            capture.labels.clear();
        }
        self.store.lock().unwrap().draw_capture = Some(capture);
        result.map(|frames| frames.into_iter().map(|(_, frame)| frame).collect())
    }
    fn draw_sources(
        &self,
        vm: &mut Vm,
        sources: &[(Value, crate::SongLuaDrawSource)],
    ) -> Result<rustc_hash::FxHashMap<usize, crate::SongLuaDrawSource>, String> {
        let mut bindings = rustc_hash::FxHashMap::default();
        for &(value, source) in sources {
            let Value::Table(actor) = value else {
                return Err("draw source must be an owned actor".into());
            };
            if !self.store.lock().unwrap().actors.contains_key(&actor) {
                return Err("draw source is foreign or released".into());
            }
            let pointer = vm
                .object_pointer(value)
                .ok_or("draw source has no identity")?;
            bindings.insert(pointer, source);
        }
        Ok(bindings)
    }
    fn phase(
        &self,
        vm: &mut Vm,
        scene: ActorScene,
        function: Root,
        delta: f64,
    ) -> Result<bool, Error> {
        if scene.owner != self.instantiate {
            return Ok(false);
        }
        let function = vm.rooted(function)?;
        Ok(vm.call(
            function,
            &[Value::Number(scene.id as f64), Value::Number(delta)],
        )?[0]
            == Value::Boolean(true))
    }
    pub(super) fn snapshot(&self, scene: ActorScene) -> Option<Vec<ActorSnapshot>> {
        if scene.owner != self.instantiate {
            return None;
        }
        retirement::sweep(&self.store);
        let store = self.store.lock().unwrap();
        if store.retired_scenes.contains_key(&scene.id) {
            return None;
        }
        let scene = store.scenes.get(&scene.id)?;
        let ids: HashMap<_, _> = scene
            .actors
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        Some(
            scene
                .actors
                .iter()
                .map(|id| {
                    let actor = &store.actors[id];
                    let mut state = actor.state.clone();
                    state.position = [actor.current.x, actor.current.y, actor.current.z];
                    state.zoom = [
                        actor.current.zoom_x,
                        actor.current.zoom_y,
                        actor.current.zoom_z,
                    ];
                    state.rotation = [
                        actor.current.rot_x_deg,
                        actor.current.rot_y_deg,
                        actor.current.rot_z_deg,
                    ];
                    state.aux = actor.current.aux;
                    actor.current.write_to(&mut state.render);
                    state.render.visible = state.visible;
                    state.render.draw_order = state.draw_order;
                    state.render.hibernating = actor.hibernate > 0.0;
                    state.parent = actor.parent.and_then(|id| ids.get(&id).copied());
                    state.children = actor.children.iter().map(|id| ids[id]).collect();
                    state.wrappers = actor.wrappers.iter().map(|id| ids[id]).collect();
                    state
                })
                .collect(),
        )
    }
    pub(super) fn mesh(&self, vm: &mut Vm, actor: Value) -> Result<Option<ActorMesh>, Error> {
        let function = vm.rooted(self.read_mesh)?;
        self.store.lock().unwrap().mesh_output = None;
        let result = vm.call(function, &[actor]);
        let mesh = self.store.lock().unwrap().mesh_output.take();
        result?;
        Ok(mesh)
    }
    pub(super) fn take_diagnostics(&self) -> Vec<String> {
        std::mem::take(&mut self.store.lock().unwrap().diagnostics)
    }

    pub(super) fn proxy(
        &self,
        vm: &mut Vm,
        actor: Value,
    ) -> Result<Option<crate::SongLuaProxyTarget>, Error> {
        let function = vm.rooted(self.read_proxy)?;
        self.store.lock().unwrap().proxy_output = None;
        let result = vm.call(function, &[actor]);
        let target = self.store.lock().unwrap().proxy_output.take();
        result?;
        Ok(target)
    }

    pub(super) fn take_sounds(&self) -> Vec<crate::SongLuaActorSound> {
        std::mem::take(&mut self.store.lock().unwrap().sounds)
    }
    pub(super) fn bitmap_text(&self, vm: &mut Vm, actor: Value) -> Result<BitmapText, Error> {
        let function = vm.rooted(self.read_text)?;
        self.store.lock().unwrap().text_output = None;
        let result = vm.call(function, &[actor]);
        let output = self.store.lock().unwrap().text_output.take();
        result?;
        Ok(output.expect("successful text read"))
    }
    pub(super) fn graph(&self, vm: &mut Vm, actor: Value) -> Result<GraphDisplay, Error> {
        let function = vm.rooted(self.read_graph)?;
        self.store.lock().unwrap().graph_output = None;
        let result = vm.call(function, &[actor]);
        let output = self.store.lock().unwrap().graph_output.take();
        result?;
        Ok(output.expect("successful graph read"))
    }
    pub(super) fn song_meter(
        &self,
        vm: &mut Vm,
        actor: Value,
    ) -> Result<Option<SongMeterDisplay>, Error> {
        let function = vm.rooted(self.read_meter)?;
        self.store.lock().unwrap().meter_output = None;
        let result = vm.call(function, &[actor]);
        let output = self.store.lock().unwrap().meter_output.take();
        result?;
        Ok(output)
    }
    pub(super) fn framebuffer(&self, vm: &mut Vm, actor: Value) -> Result<FrameTexture, Error> {
        let function = vm.rooted(self.read_framebuffer)?;
        self.store.lock().unwrap().framebuffer_output = None;
        let result = vm.call(function, &[actor]);
        let output = self.store.lock().unwrap().framebuffer_output.take();
        result?;
        Ok(output.expect("successful framebuffer read"))
    }
    pub(super) fn begin_sound_capture(&self, queued_startup: bool) {
        let mut store = self.store.lock().unwrap();
        store.prepared_sounds.clear();
        store.sprite_textures.clear();
        store.judgment_textures.clear();
        store.sound_capture = Some(queued_startup);
    }
    pub(super) fn take_prepared_sounds(&self) -> Vec<crate::SongLuaSoundEvent> {
        std::mem::take(&mut self.store.lock().unwrap().prepared_sounds)
    }
    pub(super) fn end_sound_capture(&self) {
        self.store.lock().unwrap().sound_capture = None;
    }
    pub(super) fn take_sprite_textures(&self) -> Vec<(u64, crate::SongLuaSpriteTexture)> {
        std::mem::take(&mut self.store.lock().unwrap().sprite_textures)
    }
    pub(super) fn take_judgment_textures(&self) -> Vec<(usize, crate::SongLuaJudgmentTexture)> {
        std::mem::take(&mut self.store.lock().unwrap().judgment_textures)
    }

    pub(super) fn begin_playback(&self) {
        let mut store = self.store.lock().unwrap();
        store.preparing = false;
        store.constructing_capture = false;
        store.startup_queues = None;
        store.startup_discovery = false;
        let live = store.clock.live.load(std::sync::atomic::Ordering::Relaxed);
        for actor in store.actors.values_mut() {
            if actor.capture.take().is_some() {
                // Newly registered replays begin at the captured destination
                // with an empty queue, exactly as ensure_tween_replay does.
                let getters = actor.preparation_getters.unwrap_or([
                    actor.current.x,
                    actor.current.y,
                    actor.current.z,
                    actor.current.aux,
                ]);
                actor.current = *actor.destination();
                actor.queue.clear();
                actor.sync_rotation_getters();
                // In preparation, native position/aux getters still read the
                // captured current fields until the next getter synchronization.
                // Replay itself already owns the captured destination pose.
                actor.preparation_getters = (!live).then_some(getters);
            }
        }
        store.playback = true;
    }

    pub(super) fn render_states(
        &self,
        scene: ActorScene,
        frame_clock: [f32; 2],
    ) -> Option<Vec<crate::SongLuaOverlayState>> {
        let snapshots = self.snapshot(scene)?;
        retirement::sweep(&self.store);
        let store = self.store.lock().unwrap();
        if store.retired_scenes.contains_key(&scene.id) {
            return None;
        }
        let scene = store.scenes.get(&scene.id)?;
        Some(
            snapshots
                .into_iter()
                .zip(&scene.actors)
                .map(|(snapshot, id)| {
                    let mut render = snapshot.render;
                    effects::render_time(&store.actors[id], frame_clock, &mut render);
                    render
                })
                .collect(),
        )
    }

    pub(super) fn host_messages(&self, vm: &Vm) -> Result<Value, Error> {
        vm.rooted(self.broadcast)
    }

    pub(super) fn host_template(&self, vm: &Vm) -> Result<Value, Error> {
        vm.rooted(self.template)
    }

    pub(super) fn broadcast(
        &self,
        vm: &mut Vm,
        name: &str,
        params: Option<Value>,
        normalize: bool,
    ) -> Result<(), Error> {
        let name = vm.try_intern(name.as_bytes())?;
        vm.call(
            vm.rooted(self.broadcast)?,
            &[
                name,
                Value::Boolean(params.is_some()),
                params.unwrap_or(Value::Nil),
                Value::Boolean(normalize),
            ],
        )?;
        Ok(())
    }

    #[cfg(feature = "test-support")]
    pub(super) fn ownership_counts(&self) -> [usize; 3] {
        retirement::sweep(&self.store);
        let guard = self.store.lock().unwrap();
        [
            guard.actors.len(),
            guard.scenes.len(),
            guard.retired_scenes.len(),
        ]
    }

    #[cfg(feature = "test-support")]
    pub(super) fn set_message_ranks(&self, scene: ActorScene, ranks: &[usize]) -> bool {
        if scene.owner != self.instantiate {
            return false;
        }
        let mut store = self.store.lock().unwrap();
        let Some(scene) = store.scenes.get(&scene.id) else {
            return false;
        };
        if ranks.len() != scene.actors.len()
            || ranks.contains(&0)
            || ranks.iter().copied().collect::<HashSet<_>>().len() != ranks.len()
        {
            return false;
        }
        let actors = scene.actors.clone();
        store
            .message_ranks
            .extend(actors.into_iter().zip(ranks.iter().copied()));
        true
    }

    #[cfg(feature = "test-support")]
    pub(super) fn set_message_identity(&self, actor: Value, rank: usize) -> bool {
        let Value::Table(actor) = actor else {
            return false;
        };
        let mut store = self.store.lock().unwrap();
        if rank == 0 || !store.actors.contains_key(&actor) {
            return false;
        }
        store.message_ranks.insert(actor, rank);
        true
    }
}
fn install_method(
    vm: &mut Vm,
    table: TableId,
    name: &str,
    op: Op,
    store: &Shared,
    captures: &[Value],
) -> Result<(), Error> {
    let function = vm.new_host_function_with_data(
        callback,
        captures,
        Binding {
            store: store.clone(),
            op,
        },
    )?;
    super::put(vm, table, name, function)
}
fn table(call: &HostCall<'_>, value: Value) -> Result<TableId, Error> {
    if let Value::Table(id) = value {
        Ok(id)
    } else {
        Err(call.error(b"actor table expected"))
    }
}
fn get(call: &mut HostCall<'_>, t: TableId, key: &[u8]) -> Result<Value, Error> {
    let key = call.try_intern(key)?;
    call.raw_table_get(t, key)
}
fn bytes(call: &mut HostCall<'_>, value: Value) -> Result<Vec<u8>, Error> {
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"string expected"))?;
    Ok(call.string_bytes(id).unwrap().to_vec())
}
fn arg(call: &HostCall<'_>, i: usize) -> Value {
    call.arguments().get(i).copied().unwrap_or(Value::Nil)
}
fn helper(call: &HostCall<'_>, index: usize) -> Result<Value, Error> {
    let Value::Table(helpers) = call.captures()[1] else {
        unreachable!()
    };
    call.raw_table_get(helpers, Value::Number(index as f64))
}
fn declared_children(store: &Shared, actor: TableId) -> Vec<TableId> {
    store.lock().unwrap().actors[&actor].children.clone()
}
fn children(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
) -> Result<Vec<TableId>, Error> {
    if has_native_children(store, actor) {
        native_children::children(call, store, actor)
    } else {
        Ok(declared_children(store, actor))
    }
}
fn has_native_children(store: &Shared, actor: TableId) -> bool {
    let store = store.lock().unwrap();
    let actor = &store.actors[&actor];
    actor.native_children
        || matches!(
            actor.state.class,
            "GraphDisplay"
                | "SongMeterDisplay"
                | "CourseContentsList"
                | "Model"
                | "PlayerActor"
                | "HoldJudgment"
                | "NoteField"
        )
}
fn is_frame(store: &Shared, actor: TableId) -> bool {
    matches!(
        store.lock().unwrap().actors[&actor].state.class,
        "ActorFrame"
            | "PlayerActor"
            | "HoldJudgment"
            | "PercentageDisplay"
            | "StepsDisplay"
            | "TopScreen"
            | "NoteField"
            | "ActorFrameTexture"
            | "WrapperState"
            | "GraphDisplay"
            | "SongMeterDisplay"
            | "CourseContentsList"
    )
}
fn sort(call: &mut HostCall<'_>, store: &Shared, actor: TableId) -> Result<(), Error> {
    for child in declared_children(store, actor) {
        if store.lock().unwrap().actors[&child].raw_instance {
            let value = get(call, child, b"__songlua_state_draw_order")?;
            let n = sprites::number(call, value).unwrap_or(0.);
            let order = if n > -2147483649. && n < 2147483648. {
                n as i32
            } else {
                0
            };
            store
                .lock()
                .unwrap()
                .actors
                .get_mut(&child)
                .unwrap()
                .state
                .draw_order = order;
        }
    }
    let list = {
        let mut store = store.lock().unwrap();
        let mut list = store.actors[&actor].children.clone();
        list.sort_by_key(|id| store.actors[id].state.draw_order);
        store.actors.get_mut(&actor).unwrap().children = list.clone();
        list
    };
    for (index, child) in list.into_iter().enumerate() {
        call.raw_table_set(
            actor,
            Value::Number((index + 1) as f64),
            Value::Table(child),
        )?;
    }
    Ok(())
}
fn command(
    call: &mut HostCall<'_>,
    actor: TableId,
    name: &[u8],
) -> Result<Option<(Value, Vec<u8>)>, Error> {
    let mut field = [name, b"Command"].concat();
    for fallback in [false, true] {
        if fallback {
            if name.ends_with(b"Message") {
                break;
            }
            field = [name, b"MessageCommand"].concat();
        }
        let key = call.try_intern(&field)?;
        let value = call.table_get(Value::Table(actor), key)?;
        match value {
            Value::Function(_) | Value::Native(_) => return Ok(Some((value, field))),
            Value::Nil => {}
            _ => return Err(call.error(b"actor command must be a function")),
        }
    }
    Ok(None)
}
fn run(
    call: &mut HostCall<'_>,
    actor: TableId,
    function: Value,
    params: Option<Value>,
) -> Result<(), Error> {
    match params {
        Some(params) => call.call(function, &[Value::Table(actor), params])?,
        None => call.call(function, &[Value::Table(actor)])?,
    };
    Ok(())
}
fn run_actor_function(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    function: Value,
    params: Option<Value>,
) -> Result<(), Error> {
    command_locals::capture(call, store, function)?;
    run_scoped(call, actor, function, params)
}
fn theme_callback(call: &mut HostCall<'_>, function: Value) -> Result<bool, Error> {
    // The enclosing host callback can already own results or a stack window.
    // Use the existing inspector in its own call frame rather than opening a
    // second native stack on that callback.
    Ok(call.call(helper(call, 4)?, &[function])?[0] == Value::Boolean(true))
}
fn command_observed(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    field: &[u8],
    function: Value,
) -> Result<bool, Error> {
    let dynamic = match get(call, actor, b"__songlua_dynamic_commands")? {
        Value::Nil => Value::Nil,
        value => {
            let commands = table(call, value)?;
            get(call, commands, field)?
        }
    };
    Ok(store.lock().unwrap().trace_dynamic_commands || dynamic != function)
}
fn run_callback(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    function: Value,
    params: Option<Value>,
    observed: bool,
) -> Result<(), Error> {
    let capture_sources = store.lock().unwrap().live_sources.is_some();
    let song_callback = observed && capture_sources && !theme_callback(call, function)?;
    if song_callback {
        store.lock().unwrap().song_callback_depth += 1;
    }
    let result = run_actor_function(call, store, actor, function, params);
    if song_callback {
        store.lock().unwrap().song_callback_depth -= 1;
    }
    result
}
fn run_scoped(
    call: &mut HostCall<'_>,
    actor: TableId,
    function: Value,
    params: Option<Value>,
) -> Result<(), Error> {
    let key = call.try_intern(b"__songlua_script_dir")?;
    let directory = call.table_get(Value::Table(actor), key)?;
    if directory == Value::Nil {
        return run(call, actor, function, params);
    }
    let directory = bytes(call, directory)?;
    let text = std::str::from_utf8(&directory)
        .map_err(|_| call.error(b"actor script directory must be UTF-8"))?;
    if text.trim().is_empty() {
        return run(call, actor, function, params);
    }
    let directory = call.try_intern(&directory)?;
    call.with_roots(&[directory, key], |call| {
        let globals = helper(call, 5)?;
        let previous = call.table_get(globals, key)?;
        call.with_roots(&[previous], |call| {
            call.table_set(globals, key, directory)?;
            let result = run(call, actor, function, params);
            // Restore the caller's directory even when a callback errors or
            // exhausts its instruction budget. This runs outside authored Lua.
            let globals_table = table(call, globals)?;
            if call.raw_table_get(globals_table, key)? != Value::Nil {
                // An existing field bypasses __newindex. Restore it without
                // entering the VM again after an exhausted instruction budget.
                call.raw_table_set(globals_table, key, previous)?;
            } else {
                call.table_set(globals, key, previous)?;
            }
            result
        })
    })
}
fn run_named(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &[u8],
    params: Option<Value>,
) -> Result<(), Error> {
    run_named_with_drain(call, store, actor, name, params, true)
}
fn run_named_with_drain(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &[u8],
    params: Option<Value>,
    drain: bool,
) -> Result<(), Error> {
    let Some((function, field)) = command(call, actor, name)? else {
        return Ok(());
    };
    run_guarded_command(call, store, actor, field, function, params, drain)
}
fn run_guarded_command(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    field: Vec<u8>,
    function: Value,
    params: Option<Value>,
    drain: bool,
) -> Result<(), Error> {
    let inserted = store
        .lock()
        .unwrap()
        .actors
        .get_mut(&actor)
        .unwrap()
        .active_commands
        .insert(field.clone());
    if !inserted {
        return Ok(());
    }
    let previous_capture_start = capture::command_start(store, actor);
    #[cfg(feature = "test-support")]
    let observed_command = song_messages::update_command(store, &field);
    // Keep the selected function alive if a callback replaces its own field or
    // forces GC. Never retain the store lock while entering authored Lua.
    let result = call.with_roots(&[function, params.unwrap_or(Value::Nil)], |call| {
        let theme = theme_callback(call, function)?;
        let observed = command_observed(call, store, actor, &field, function)?;
        match run_callback(call, store, actor, function, params, observed) {
            Err(error) if theme && error.kind() == ErrorKind::Runtime => {
                store.lock().unwrap().diagnostics.push(error.to_string());
                Ok(())
            }
            result => result,
        }
    });
    #[cfg(feature = "test-support")]
    song_messages::restore_command(store, observed_command);
    let result = result.and_then(|()| {
        if drain && previous_capture_start.is_some() {
            capture::drain_commands(call, store, actor)?;
        }
        Ok(())
    });
    store
        .lock()
        .unwrap()
        .actors
        .get_mut(&actor)
        .unwrap()
        .active_commands
        .remove(&field);
    capture::command_end(store, actor, previous_capture_start);
    result
}
fn play(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &[u8],
    params: Option<Value>,
    recurse: bool,
) -> Result<(), Error> {
    run_named(call, store, actor, name, params)?;
    if recurse {
        for child in children(call, store, actor)? {
            play(call, store, child, name, params, true)?;
        }
    }
    Ok(())
}
fn leaves(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    action: &mut impl FnMut(&mut HostCall<'_>, TableId) -> Result<(), Error>,
) -> Result<(), Error> {
    let children = children(call, store, actor)?;
    if children.is_empty() {
        action(call, actor)?;
    } else {
        for child in children {
            leaves(call, store, child, action)?;
        }
    }
    Ok(())
}
fn release(call: &mut HostCall<'_>, store: &Shared, id: u64) -> Result<bool, Error> {
    if store.lock().unwrap().retired_scenes.contains_key(&id) {
        return Ok(false);
    }
    store.lock().unwrap().constructor_scenes.remove(&id);
    let scene = store.lock().unwrap().scenes.remove(&id);
    let Some(scene) = scene else { return Ok(false) };
    lifecycle::invalidate_plans(call, store)?;
    // Course displays can be shared tables. A scene retires only its last
    // ownership of those actors, including their callbacks and registry roots.
    let mut retired = HashSet::new();
    let mut transferred = Vec::new();
    {
        let mut guard = store.lock().unwrap();
        for &actor in &scene.actors {
            let other = guard
                .scenes
                .iter()
                .filter(|(_, scene)| scene.actors.contains(&actor))
                .min_by_key(|(id, _)| guard.retired_scenes.contains_key(id))
                .map(|(id, _)| *id);
            if let Some(other) = other {
                if guard.retired_scenes.contains_key(&other) {
                    transferred.push((actor, other));
                }
                if let Some(instance) = guard.actors.get_mut(&actor)
                    && instance.scene == id
                {
                    instance.scene = other;
                }
            } else {
                retired.insert(actor);
            }
        }
    }
    store
        .lock()
        .unwrap()
        .actor_order
        .retain(|actor| !retired.contains(actor));
    if let Some(queued) = &mut store.lock().unwrap().startup_queues {
        queued.retain(|actor| !retired.contains(actor));
    }
    let registry = table(call, call.captures()[0])?;
    for (actor, other) in transferred {
        retirement::retain(call, store, other, actor, registry)?;
        call.raw_table_set(registry, Value::Table(actor), Value::Nil)?;
        store.lock().unwrap().retired_subscriptions.remove(&actor);
        store
            .lock()
            .unwrap()
            .actor_order
            .retain(|value| *value != actor);
    }
    for actor in scene.actors {
        if !retired.contains(&actor) {
            continue;
        }
        if !scene.sounds_retired {
            sound::retire(call, store, actor);
        }
        call.raw_table_set(registry, Value::Table(actor), Value::Nil)?;
        let mut store = store.lock().unwrap();
        store.actors.remove(&actor);
        store.retired_subscriptions.remove(&actor);
        #[cfg(feature = "test-support")]
        store.message_ranks.remove(&actor);
    }
    Ok(true)
}
fn build(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    node: TableId,
    parent: Option<TableId>,
    check_condition: bool,
    internal: bool,
) -> Result<Option<TableId>, Error> {
    let attrs = table(call, call.raw_table_get(node, Value::Number(1.0))?)?;
    if check_condition && get(call, attrs, b"Condition")? == Value::Boolean(false) {
        return Ok(None);
    }
    let template = match call.raw_table_get(node, Value::Number(3.0))? {
        Value::Table(source) => store
            .lock()
            .unwrap()
            .actors
            .get(&source)
            .map(TemplateState::from),
        _ => None,
    };
    let class = if let Some(template) = &template {
        template.state.class.as_bytes().to_vec()
    } else {
        let mut kind = get(call, attrs, b"Class")?;
        if kind == Value::Nil {
            kind = get(call, attrs, b"Type")?;
        }
        if kind == Value::Nil {
            Vec::new()
        } else {
            bytes(call, kind)?
        }
    };
    let dummy = class.is_empty();
    let class = match class.as_slice() {
        b"" | b"Actor" => "Actor",
        b"ActorFrame" => "ActorFrame",
        b"ActorFrameTexture" => "ActorFrameTexture",
        b"WrapperState" => "WrapperState",
        b"Quad" => "Quad",
        b"Sprite" => "Sprite",
        b"Banner" => "Banner",
        b"GraphDisplay" => "GraphDisplay",
        b"SongMeterDisplay" => "SongMeterDisplay",
        b"CourseContentsList" => "CourseContentsList",
        b"Model" => "Model",
        b"PlayerActor" if internal => "PlayerActor",
        b"HoldJudgment" if internal => "HoldJudgment",
        b"PercentageDisplay" if internal => "PercentageDisplay",
        b"StepsDisplay" if internal => "StepsDisplay",
        b"TopScreen" if internal => "TopScreen",
        b"ChildActor" if internal => "ChildActor",
        b"OptionRow" if internal => "OptionRow",
        b"OptionRowFrame" if internal => "OptionRowFrame",
        b"Timer" if internal => "Timer",
        b"NoteField" if internal => "NoteField",
        b"NoteColumnRenderer" if internal => "NoteColumnRenderer",
        b"LifeMeter" if internal => "LifeMeter",
        b"GraphDisplayBody" if internal => "GraphDisplayBody",
        b"GraphDisplayLine" if internal => "GraphDisplayLine",
        b"BitmapText" => "BitmapText",
        b"RollingNumbers" => "RollingNumbers",
        b"BPMDisplay" => "BPMDisplay",
        b"DeviceList" => "DeviceList",
        b"InputList" => "InputList",
        b"ActorMultiVertex" => "ActorMultiVertex",
        b"ActorProxy" => "ActorProxy",
        b"Sound" => "Sound",
        _ => {
            return Err(call.error(
                format!(
                    "actor class '{}' has no Rust instance implementation yet",
                    String::from_utf8_lossy(&class)
                )
                .as_bytes(),
            ));
        }
    };
    let create = helper(call, 1)?;
    let kind = call.try_intern(class.as_bytes())?;
    let empty_attrs = if dummy {
        Value::Table(call.try_new_table()?)
    } else {
        Value::Table(attrs)
    };
    let actor = call.call(
        create,
        &[
            kind,
            parent.map_or(Value::Nil, Value::Table),
            empty_attrs,
            Value::Boolean(template.is_some()),
        ],
    )?[0];
    let actor = table(call, actor)?;
    if matches!(class, "Quad" | "ActorMultiVertex") && template.is_none() {
        let key = call.try_intern(b"__songlua_default_texture")?;
        call.raw_table_set(actor, key, Value::Boolean(true))?;
    }
    let mut name = get(call, attrs, b"Name")?;
    // Constructors can name a raw CourseContentsList Display before the same
    // declaration is materialized as an ordinary child.
    if name == Value::Nil
        && let Value::Table(source) = call.raw_table_get(node, Value::Number(3.))?
    {
        name = get(call, source, b"Name")?;
        if name != Value::Nil {
            sprites::set(call, actor, "Name", name)?;
        }
    }
    let load_name = if dummy || name == Value::Nil {
        None
    } else {
        Some(bytes(call, name)?)
    };
    let registry = table(call, call.captures()[0])?;
    retirement::retain(call, store, scene, actor, registry)?;
    {
        let mut store = store.lock().unwrap();
        store.scenes.get_mut(&scene).unwrap().actors.push(actor);
        let deferred = store.defer_registration && !internal && !dummy;
        let mut instance = Actor::new(
            scene,
            class,
            if deferred { None } else { parent },
            load_name,
        );
        instance.pending_init = deferred;
        instance.registered = !deferred;
        if deferred {
            instance.state.name = instance.load_name.clone().unwrap_or_default();
        } else {
            store.actor_order.push(actor);
        }
        if store.constructing_capture {
            instance.capture = Some(capture::Capture::default());
        }
        if let Some(template) = &template {
            // Production clones native fields into a fresh actor at its last
            // destination. Its existing tween queue does not advance or become
            // the clone's queue; independent hibernation and immediate fields
            // are retained. Init still runs normally on the new instance.
            instance.current = template.pose;
            instance.hibernate = template.hibernate;
            instance.update_rate = template.update_rate;
            instance.propagate = template.propagate;
            instance.update_error_reported = template.update_error_reported;
            instance.state.size = template.state.size;
            instance.state.visible = template.state.visible;
            instance.state.draw_order = template.state.draw_order;
            instance.state.render = template.state.render;
            instance.base_transform = template.base_transform;
            instance.pose_exported = template.pose_exported;
            instance.state.rotation = template.state.rotation;
            instance.effects = template.effects;
            if store.preparing {
                instance.capture = template.capture.clone().or(instance.capture);
                instance.preparation_getters = template.preparation_getters;
                instance.offline_instance = true;
            }
        }
        store.actors.insert(actor, instance);
    }
    initialize_resources(
        call,
        store,
        scene,
        actor,
        class,
        if template.is_some() {
            ResourceMode::Template
        } else {
            ResourceMode::Constructor
        },
    )?;
    if dummy {
        return Ok(Some(actor));
    }
    if matches!(
        class,
        "ActorFrame"
            | "PlayerActor"
            | "HoldJudgment"
            | "PercentageDisplay"
            | "StepsDisplay"
            | "TopScreen"
            | "NoteField"
            | "ActorFrameTexture"
            | "GraphDisplay"
            | "SongMeterDisplay"
            | "CourseContentsList"
            | "Model"
    ) {
        let nodes = table(call, call.raw_table_get(node, Value::Number(2.))?)?;
        let mut selected = nodes;
        let mut array_only = true;
        for i in 1..=call.raw_table_len(nodes)? {
            let entry = table(call, call.raw_table_get(nodes, Value::Number(i as f64))?)?;
            let key = call.raw_table_get(entry, Value::Number(1.))?;
            if bytes(call, key)? == b"children" {
                let node = table(call, call.raw_table_get(entry, Value::Number(2.))?)?;
                selected = table(call, call.raw_table_get(node, Value::Number(2.))?)?;
                array_only = false;
                break;
            }
        }
        for i in 1..=call.raw_table_len(selected)? {
            let entry = table(call, call.raw_table_get(selected, Value::Number(i as f64))?)?;
            let name = call.raw_table_get(entry, Value::Number(1.))?;
            let name = bytes(call, name)?;
            if array_only && (name.is_empty() || !name.iter().all(u8::is_ascii_digit)) {
                continue;
            }
            let child = table(call, call.raw_table_get(entry, Value::Number(2.))?)?;
            if let Some(child) = build(call, store, scene, child, Some(actor), true, false)? {
                store
                    .lock()
                    .unwrap()
                    .actors
                    .get_mut(&actor)
                    .unwrap()
                    .children
                    .push(child);
                let index = store.lock().unwrap().actors[&actor].children.len();
                call.raw_table_set(actor, Value::Number(index as f64), Value::Table(child))?;
            }
        }
    }
    Ok(Some(actor))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResourceMode {
    Constructor,
    Template,
    OfflineClone,
    Dummy,
}

fn initialize_resources(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    actor: TableId,
    class: &'static str,
    mode: ResourceMode,
) -> Result<(), Error> {
    let template = mode != ResourceMode::Constructor;
    let bind_only = matches!(mode, ResourceMode::OfflineClone | ResourceMode::Dummy);
    if mode == ResourceMode::Constructor {
        host_compat::initialize_base(call, store, actor)?;
    }
    if class == "Sound" {
        sound::install(call, store, actor)?;
    }
    proxies::install(call, actor, class == "ActorProxy")?;
    textures::install_getter(call, store, actor)?;
    if !template {
        render::initialize(call, store, actor)?;
    }
    if class == "Model" {
        models::initialize(call, store, actor, template)?;
    }
    if text_displays::is_display(class) {
        text_displays::install(call, actor)?;
        if !bind_only && let Some(text) = crate::input_status_actor_text(class) {
            let value = call.try_intern(text.as_bytes())?;
            sprites::set(call, actor, "Text", value)?;
        }
    }
    if matches!(
        class,
        "GraphDisplay" | "GraphDisplayBody" | "GraphDisplayLine"
    ) {
        graphs::install(call, actor)?;
    }
    if class == "GraphDisplay" && !bind_only {
        graphs::initialize(call, store, scene, actor)?;
    }
    if class == "CourseContentsList" {
        courses::install(call, actor)?;
        if !bind_only {
            courses::initialize(call, store, scene, actor)?;
        }
    }
    if class == "SongMeterDisplay" {
        song_meters::install(call, actor)?;
        if !bind_only {
            song_meters::initialize(call, store, scene, actor)?;
        }
    }
    if class == "ActorFrameTexture" && !bind_only {
        framebuffers::initialize(call, actor)?;
    }
    if matches!(class, "Sprite" | "Banner") && !bind_only {
        sprites::initialize(call, store, actor, template)?;
    }
    if mode == ResourceMode::Constructor {
        host_compat::initialize_pose(call, store, actor)?;
    }
    Ok(())
}

fn new_scene(call: &HostCall<'_>, store: &Shared) -> Result<u64, Error> {
    let mut guard = store.lock().unwrap();
    if guard.next >= (1u64 << 53) - 1 {
        return Err(call.error(b"actor scene identity exhausted"));
    }
    guard.next += 1;
    let id = guard.next;
    guard.scenes.insert(
        id,
        Scene {
            actors: Vec::new(),
            root: None,
            ended: false,
            sounds_retired: false,
            initial_states: None,
            song_session: None,
        },
    );
    Ok(id)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Materialization {
    Template,
    Construct,
    Initialize,
}

// Declaration construction and template cloning share scene ownership. A
// compiler can retain the constructed graph across initial-state capture/Init.
fn materialize(
    call: &mut HostCall<'_>,
    store: &Shared,
    declaration: TableId,
    mode: Materialization,
) -> Result<u64, Error> {
    let offline = {
        let guard = store.lock().unwrap();
        matches!(
            mode,
            Materialization::Construct | Materialization::Initialize
        ) && guard.preparing
            && guard.actors.contains_key(&declaration)
    };
    if offline {
        return offline::own_root(
            call,
            store,
            declaration,
            mode == Materialization::Initialize,
        );
    }
    let node = call.call(helper(call, 2)?, &[Value::Table(declaration)])?[0];
    call.with_roots(&[node], |call| {
        let id = new_scene(call, store)?;
        let previous = {
            let mut guard = store.lock().unwrap();
            std::mem::replace(
                &mut guard.defer_registration,
                mode != Materialization::Template,
            )
        };
        // MakeActor ignores Condition on the root. Only the child loader tests it.
        let result = build(call, store, id, table(call, node)?, None, false, false);
        store.lock().unwrap().defer_registration = previous;
        let result = result.and_then(|root| {
            if mode == Materialization::Initialize
                && let Some(actor) = root
            {
                prepare_init(store);
                initialize(call, store, actor)?;
            }
            Ok(root)
        });
        match result {
            Ok(root) => {
                let mut guard = store.lock().unwrap();
                guard.scenes.get_mut(&id).unwrap().root = root;
                if mode == Materialization::Template
                    && let Some(actor) = root
                {
                    let offline = !guard.clock.live.load(std::sync::atomic::Ordering::Relaxed)
                        && !guard.playback;
                    guard.actors.get_mut(&actor).unwrap().offline_template = offline;
                }
                Ok(id)
            }
            Err(error) => {
                release(call, store, id)?;
                Err(error)
            }
        }
    })
}

fn prepare_init(store: &Shared) {
    let mut guard = store.lock().unwrap();
    if guard.preparing {
        guard.startup_queues.get_or_insert_with(Vec::new);
    }
}

fn initialize(call: &mut HostCall<'_>, store: &Shared, actor: TableId) -> Result<(), Error> {
    if !store.lock().unwrap().actors.contains_key(&actor) {
        return native_children::group_lifecycle(call, store, actor, false);
    }
    if store.lock().unwrap().actors[&actor].offline_instance {
        offline::prepare_init(call, store, actor)?;
    }
    courses::prepare_raw_init(call, store, actor)?;
    if store.lock().unwrap().actors[&actor].initialized {
        return Ok(());
    }
    let original = get(call, actor, b"Name")?;
    call.with_roots(&[original], |call| {
        initialize_body(call, store, actor, original)
    })
}
fn initialize_body(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    original: Value,
) -> Result<(), Error> {
    let raw = {
        let guard = store.lock().unwrap();
        let a = &guard.actors[&actor];
        a.raw_instance || a.offline_instance
    };
    let name = if raw {
        // Raw CourseContentsList displays preserve malformed attributes until
        // a typed getter reads them. prepare_raw_init already applies that rule.
        store.lock().unwrap().actors[&actor].load_name.clone()
    } else if original == Value::Nil {
        None
    } else {
        Some(bytes(call, original)?)
    };
    {
        let mut store = store.lock().unwrap();
        if !store.actor_order.contains(&actor) {
            store.actor_order.push(actor);
        }
        let actor = store.actors.get_mut(&actor).unwrap();
        actor.state.name.clear();
        actor.load_name = name;
        actor.pending_init = false;
        actor.registered = true;
    }
    // The production host materializes the complete tree before Init. Every
    // child is observable while child-first Init applies names and draw order.
    for child in declared_children(store, actor) {
        native_children::adopt_declared(call, store, child, actor)?;
        initialize(call, store, child)?;
    }
    sort(call, store, actor)?;
    {
        let mut store = store.lock().unwrap();
        let actor = store.actors.get_mut(&actor).unwrap();
        if let Some(name) = actor.load_name.take() {
            actor.state.name = name;
        }
    }
    if original != Value::Nil {
        let key = call.try_intern(b"Name")?;
        call.raw_table_set(actor, key, original)?;
    }
    courses::publish_raw_name(call, store, actor, original)?;
    play(call, store, actor, b"Init", None, false)?;
    if let Some(stream) = song_meters::stream(call, store, actor)? {
        if store
            .lock()
            .unwrap()
            .actors
            .get(&stream)
            .is_some_and(|a| !a.initialized)
        {
            let name = get(call, stream, b"Name")?;
            if name != Value::Nil {
                let name = bytes(call, name)?;
                store
                    .lock()
                    .unwrap()
                    .actors
                    .get_mut(&stream)
                    .unwrap()
                    .load_name = Some(name);
            }
        }
        initialize(call, store, stream)?;
    }
    store
        .lock()
        .unwrap()
        .actors
        .get_mut(&actor)
        .unwrap()
        .initialized = true;
    courses::publish_raw_flag(call, store, actor, "__songlua_init_commands_ran")?;
    Ok(())
}
fn group(call: &mut HostCall<'_>, ids: &[TableId]) -> Result<Value, Error> {
    match ids {
        [] => Ok(Value::Nil),
        [actor] => Ok(Value::Table(*actor)),
        _ => {
            let list = call.try_new_table()?;
            for (i, id) in ids.iter().enumerate() {
                call.raw_table_set(list, Value::Number((i + 1) as f64), Value::Table(*id))?;
            }
            Ok(call.call(helper(call, 3)?, &[Value::Table(list)])?[0])
        }
    }
}
fn invoke_bound(call: &mut HostCall<'_>) -> Result<(), Error> {
    let name = *call.function_data::<&'static str>().unwrap();
    // Production's own adapters close over the receiver. Both colon and dot
    // invocation therefore address this actor, including calls with another
    // table in the receiver slot. settext uniquely always reads argument two.
    let skip = if name == "settext" {
        1
    } else {
        usize::from(matches!(call.arguments().first(), Some(Value::Table(_))))
    };
    let mut args = Vec::with_capacity(call.arguments().len() + 1);
    args.push(call.captures()[2]);
    args.extend(call.arguments().iter().skip(skip).copied());
    call.call_and_return(call.captures()[3], &args)
}

fn actor_name(call: &mut HostCall<'_>, store: &Shared, actor: TableId) -> Result<Vec<u8>, Error> {
    let (raw, name) = {
        let guard = store.lock().unwrap();
        let actor = &guard.actors[&actor];
        (
            actor.raw_instance || actor.pending_init || actor.offline_instance,
            actor.state.name.clone(),
        )
    };
    if raw {
        for key in [
            b"__songlua_load_name".as_slice(),
            b"__songlua_native_name",
            b"Name",
        ] {
            let value = get(call, actor, key)?;
            if value != Value::Nil {
                return Ok(assets::text(call, value)?.into_bytes());
            }
        }
        return Ok(Vec::new());
    }
    Ok(name)
}

fn parent_scene(
    call: &mut HostCall<'_>,
    store: &Shared,
    root: TableId,
    parent: Value,
    append: bool,
) -> Result<(), Error> {
    let parent = match parent {
        Value::Nil if !append => None,
        Value::Table(parent) if store.lock().unwrap().actors.contains_key(&parent) => Some(parent),
        _ => return Err(call.error(b"parent must be an actor owned by this runtime")),
    };
    if let Some(parent) = parent {
        {
            let guard = store.lock().unwrap();
            let mut ancestor = Some(parent);
            let mut visited = HashSet::new();
            while let Some(id) = ancestor {
                if id == root || !visited.insert(id) {
                    return Err(call.error(b"actor parent would create a cycle"));
                }
                ancestor = guard.actors.get(&id).and_then(|actor| actor.parent);
            }
            // Before Init, child links exist even though GetParent is still nil.
            let mut descendants = guard.actors[&root].children.clone();
            let mut visited = HashSet::new();
            while let Some(id) = descendants.pop() {
                if id == parent {
                    return Err(call.error(b"actor parent would create a cycle"));
                }
                if visited.insert(id)
                    && let Some(actor) = guard.actors.get(&id)
                {
                    descendants.extend_from_slice(&actor.children);
                }
            }
        }
        if append {
            if !is_frame(store, parent) {
                return Err(call.error(b"ActorFrame parent expected"));
            }
            courses::share_child(call, store, parent, root)?;
            if !store.lock().unwrap().actors[&parent]
                .children
                .contains(&root)
            {
                screen_parts::append(call, store, parent, root)?;
                lifecycle::invalidate_plans(call, store)?;
                return Ok(());
            }
        }
        native_children::adopt(call, store, root, parent)?;
    } else {
        sprites::set(call, root, "__songlua_parent", Value::Nil)?;
        store.lock().unwrap().actors.get_mut(&root).unwrap().parent = None;
    }
    Ok(())
}

fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Binding { store, op } = call.function_data::<Binding>().unwrap().clone();
    retirement::sweep(&store);
    if matches!(op, Op::Retire) {
        let id = call.number(0)? as u64;
        let retired = retirement::retire(call, &store, id)?;
        return call.push(Value::Boolean(retired));
    }
    if matches!(op, Op::PrepareDefinitions) {
        return offline::install(call, &store);
    }
    if matches!(op, Op::LoadPreparationActor) {
        return loading::load_actor(call, &store);
    }
    if let Op::OfflineConstructor(class) = op {
        return offline::construct(call, &store, class);
    }

    if matches!(op, Op::DiscoverStartup) {
        return capture::discover_startup(call, &store);
    }
    if let Op::Message(op) = op {
        return messages::callback(call, &store, op);
    }
    if matches!(op, Op::End) {
        let id = call.number(0)? as u64;
        if store.lock().unwrap().retired_scenes.contains_key(&id) {
            return call.push(Value::Boolean(false));
        }
        let root = {
            let mut store = store.lock().unwrap();
            let Some(scene) = store.scenes.get_mut(&id) else {
                return call.push(Value::Boolean(false));
            };
            if scene.ended {
                return call.push(Value::Boolean(false));
            }
            // An errored End is still final. The caller observes its partial
            // effects; it must not retry callbacks or advance the outgoing tree.
            scene.ended = true;
            scene.root
        };
        if let Some(root) = root {
            lifecycle::end(call, &store, root)?;
        }
        let actors = store.lock().unwrap().scenes[&id].actors.clone();
        for actor in actors {
            sound::retire(call, &store, actor);
        }
        store
            .lock()
            .unwrap()
            .scenes
            .get_mut(&id)
            .unwrap()
            .sounds_retired = true;
        return call.push(Value::Boolean(true));
    }
    if matches!(
        op,
        Op::Start | Op::Advance | Op::DrawScene | Op::Initialize | Op::SetParent | Op::Append
    ) {
        let id = call.number(0)? as u64;
        if store.lock().unwrap().retired_scenes.contains_key(&id) {
            return call.push(Value::Boolean(false));
        }
        let root = store
            .lock()
            .unwrap()
            .scenes
            .get(&id)
            .map(|scene| (scene.root, scene.ended));
        let Some((root, ended)) = root else {
            return call.push(Value::Boolean(false));
        };
        if ended {
            return Err(call.error(b"cannot advance an ended song Lua scene"));
        }
        if let Some(root) = root {
            if matches!(op, Op::Initialize) {
                prepare_init(&store);
                initialize(call, &store, root)?;
            } else if matches!(op, Op::SetParent | Op::Append) {
                parent_scene(call, &store, root, arg(call, 1), matches!(op, Op::Append))?;
            } else if matches!(op, Op::Start) {
                lifecycle::start(call, &store, root)?;
            } else if matches!(op, Op::DrawScene) {
                draw::scene(call, &store, root)?;
            } else {
                let delta = call.number(1)? as f32;
                lifecycle::advance(call, &store, root, delta)?;
            }
        }
        return call.push(Value::Boolean(true));
    }
    if matches!(op, Op::LoadFont) {
        return text::load_font(call, &store);
    }
    if matches!(op, Op::Template) {
        let declaration = table(call, arg(call, 0))?;
        let globals = table(call, helper(call, 5)?)?;
        for key in ["__songlua_script_dir", "__songlua_song_dir"] {
            if let Some(directory) = assets::field(call, globals, key.as_bytes())? {
                let value = call.try_intern(directory.as_bytes())?;
                sprites::set(call, declaration, key, value)?;
            }
        }
        let id = materialize(call, &store, declaration, Materialization::Template)?;
        let actor = store.lock().unwrap().scenes[&id]
            .root
            .expect("actor template");
        let key = call.try_intern(b"Class")?;
        call.raw_table_set(actor, key, Value::Nil)?;
        return call.push(Value::Table(actor));
    }
    if matches!(op, Op::Instantiate | Op::Construct) {
        let declaration = table(call, arg(call, 0))?;
        let mode = if matches!(op, Op::Instantiate) {
            Materialization::Initialize
        } else {
            Materialization::Construct
        };
        let id = materialize(call, &store, declaration, mode)?;
        return call.push(Value::Number(id as f64));
    }
    if matches!(op, Op::Release) {
        let id = call.number(0)? as u64;
        let removed = release(call, &store, id)?;
        return call.push(Value::Boolean(removed));
    }
    let actor = table(call, arg(call, 0))?;
    if !store.lock().unwrap().actors.contains_key(&actor) {
        return Err(call.error(b"actor instance is not owned by this runtime or has been released"));
    }
    let raw_epoch = store.lock().unwrap().actors[&actor].raw_live_epoch;
    if let Op::Get(name) | Op::Render(name) = op
        && raw_epoch
        && live_epoch::getter(call, &store, actor, name)?
    {
        return Ok(());
    }
    let frame_method = matches!(
        op,
        Op::GetChildren
            | Op::GetChild
            | Op::GetNumChildren
            | Op::Sort
            | Op::OnChildren
            | Op::OnLeaves
            | Op::NamedChildren
            | Op::NamedLeaves
            | Op::Propagate
            | Op::PropagateCommand
    ) || matches!(op, Op::Render(name) if render::FRAME_METHODS.contains(&name))
        || matches!(op, Op::Draw("SetDrawFunction" | "GetDrawFunction"));
    let actor_class = store.lock().unwrap().actors[&actor].state.class;
    let private_child_query = matches!(op, Op::GetChild | Op::GetChildren | Op::GetNumChildren)
        && matches!(
            actor_class,
            "Model" | "OptionRow" | "OptionRowFrame" | "LifeMeter"
        );
    if frame_method && !is_frame(&store, actor) && !private_child_query {
        return Err(call.error(b"ActorFrame expected"));
    }
    if let Op::Set(name) | Op::Render(name) = op {
        ease_probe::method(call, &store, actor, name)?;
    }
    match op {
        Op::Lifecycle(operation) => return lifecycle::method(call, &store, actor, operation),
        Op::Render(name) => return render::method(call, &store, actor, name),
        Op::Effect(name) => return effects::method(call, &store, actor, name),
        Op::Wrapper(name) => return wrappers::method(call, &store, actor, name),
        Op::Sound(name) => return sound::method(call, &store, actor, name),
        Op::Sprite(name) => return sprites::method(call, &store, actor, name),
        Op::Banner(name) => return banners::method(call, &store, actor, name),
        Op::Mesh(name) => return meshes::method(call, &store, actor, name),
        Op::CloneTemplate => {
            if store.lock().unwrap().actors[&actor].offline_instance {
                let clone = offline::clone_actor(call, &store, actor, false)?;
                return call.push(Value::Table(clone));
            }
            let id = materialize(call, &store, actor, Materialization::Template)?;
            let root = store.lock().unwrap().scenes[&id].root;
            return call.push(root.map_or(Value::Nil, Value::Table));
        }
        Op::Text(name) => return text::method(call, &store, actor, name),
        Op::Graph(name) => return graphs::method(call, &store, actor, name),
        Op::SongMeter(name) => return song_meters::method(call, &store, actor, name),
        Op::Course(name) => return courses::method(call, &store, actor, name),
        Op::ReadModelPaths => {
            let paths = models::paths(call, actor)?;
            store.lock().unwrap().model_paths_output = paths;
            return Ok(());
        }
        Op::ReadSongMeter => {
            let meter = song_meters::read(call, &store, actor)?;
            store.lock().unwrap().meter_output = meter;
            return Ok(());
        }
        Op::ReadGraph => {
            let graph = graphs::read(call, &store, actor)?;
            store.lock().unwrap().graph_output = Some(graph);
            return Ok(());
        }
        Op::TextDisplay(name) => return text_displays::method(call, &store, actor, name),
        Op::ReadText => {
            let output = text::read(call, actor)?;
            store.lock().unwrap().text_output = Some(output);
            return Ok(());
        }
        Op::Framebuffer(name) => return framebuffers::method(call, &store, actor, name),
        Op::ReadFramebuffer => {
            let framebuffer = framebuffers::read(call, actor)?;
            store.lock().unwrap().framebuffer_output = Some(framebuffer);
            return Ok(());
        }
        Op::Draw(name) => return draw::method(call, &store, actor, name),
        Op::ReadMesh => {
            let mesh = meshes::read(call, actor)?;
            store.lock().unwrap().mesh_output = mesh;
            return Ok(());
        }
        Op::ReadProxy => {
            let target = proxies::read(call, actor)?;
            store.lock().unwrap().proxy_output = target;
            return Ok(());
        }
        Op::GetName if actor_class == "OptionRow" => {
            let bound = get(call, actor, b"__songlua_bound_methods")?;
            let function = get(call, table(call, bound)?, b"GetName")?;
            let args = call.arguments().to_vec();
            return call.call_and_return(function, &args);
        }
        Op::GetName => {
            let name = actor_name(call, &store, actor)?;
            let name = call.try_intern(&name)?;
            return call.push(name);
        }
        Op::Name => {
            let name = bytes(call, arg(call, 1))?;
            let publish = {
                let guard = store.lock().unwrap();
                let actor = &guard.actors[&actor];
                actor.raw_instance || actor.pending_init || actor.offline_instance
            };
            if publish {
                let value = call.try_intern(&name)?;
                let key = call.try_intern(b"__songlua_load_name")?;
                if call.raw_table_get(actor, key)? != Value::Nil {
                    call.raw_table_set(actor, key, value)?;
                }
                let key = call.try_intern(b"__songlua_native_name")?;
                call.raw_table_set(actor, key, value)?;
            }
            store
                .lock()
                .unwrap()
                .actors
                .get_mut(&actor)
                .unwrap()
                .state
                .name = name;
        }
        Op::GetParent => {
            let parent = store.lock().unwrap().actors[&actor].parent;
            return call.push(parent.map_or(Value::Nil, Value::Table));
        }
        Op::GetNumChildren => {
            let count = children(call, &store, actor)?.len();
            return call.push(Value::Number(count as f64));
        }
        Op::GetChild if actor_class == "TopScreen" => {
            let bound = get(call, actor, b"__songlua_bound_methods")?;
            let function = get(call, table(call, bound)?, b"GetChild")?;
            if function != Value::Nil {
                let args = call.arguments().to_vec();
                return call.call_and_return(function, &args);
            }
            let key = bytes(call, arg(call, 1))?;
            let map = native_children::named_children(call, actor)?;
            let value = call.with_roots(&[map], |call| {
                let key = call.try_intern(&key)?;
                call.table_get(map, key)
            })?;
            return call.push(value);
        }
        Op::GetChild if has_native_children(&store, actor) => {
            let key = arg(call, 1);
            let key = bytes(call, key)?;
            let map = native_children::named_children(call, actor)?;
            let value = call.with_roots(&[map], |call| {
                let key = call.try_intern(&key)?;
                call.table_get(map, key)
            })?;
            return call.push(value);
        }
        Op::GetChildren if has_native_children(&store, actor) => {
            let result = native_children::named_children(call, actor)?;
            return call.push(result);
        }
        Op::GetChild => {
            let name = bytes(call, arg(call, 1))?;
            let mut ids = Vec::new();
            for id in children(call, &store, actor)? {
                if actor_name(call, &store, id)? == name {
                    ids.push(id);
                }
            }
            let value = group(call, &ids)?;
            return call.push(value);
        }
        Op::GetChildren => {
            let result = Value::Table(call.try_new_table()?);
            return call.with_roots(&[result], |call| {
                let mut names: Vec<(Vec<u8>, Vec<TableId>)> = Vec::new();
                for id in children(call, &store, actor)? {
                    let name = actor_name(call, &store, id)?;
                    if let Some((_, ids)) = names.iter_mut().find(|(n, _)| *n == name) {
                        ids.push(id)
                    } else {
                        names.push((name, vec![id]));
                    }
                }
                for (name, ids) in names {
                    let value = group(call, &ids)?;
                    let key = call.try_intern(&name)?;
                    call.raw_table_set(table(call, result)?, key, value)?;
                }
                call.push(result)
            });
        }
        Op::Sort => sort(call, &store, actor)?,
        Op::Propagate => {
            store
                .lock()
                .unwrap()
                .actors
                .get_mut(&actor)
                .unwrap()
                .propagate = !matches!(arg(call, 1), Value::Nil | Value::Boolean(false));
        }
        Op::AddCommand | Op::GetCommand => {
            let name = arg(call, 1);
            if matches!(op, Op::GetCommand) {
                let Some(name) = command_name(call, name) else {
                    return call.push(Value::Nil);
                };
                let value = command(call, actor, &name)?;
                return call.push(value.map_or(Value::Nil, |(function, _)| function));
            }
            let name = bytes(call, name)?;
            std::str::from_utf8(&name).map_err(|_| call.error(b"command name must be UTF-8"))?;
            let value = arg(call, 2);
            if !matches!(value, Value::Function(_) | Value::Native(_)) {
                return Err(call.error(b"function expected"));
            }
            let key = call.try_intern(&[name.as_slice(), b"Command"].concat())?;
            if store
                .lock()
                .unwrap()
                .clock
                .live
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                let commands = match get(call, actor, b"__songlua_dynamic_commands")? {
                    Value::Nil => {
                        let commands = call.try_new_table()?;
                        let field = call.try_intern(b"__songlua_dynamic_commands")?;
                        call.raw_table_set(actor, field, Value::Table(commands))?;
                        commands
                    }
                    value => table(call, value)?,
                };
                call.raw_table_set(commands, key, value)?;
            }
            call.table_set(Value::Table(actor), key, value)?;
        }
        Op::PlayCommand | Op::PropagateCommand | Op::NamedChildren | Op::NamedLeaves => {
            let Some(name) = command_name(call, arg(call, 1)) else {
                return call.push(Value::Table(actor));
            };
            let params = call.arguments().get(2).copied();
            match op {
                Op::NamedLeaves => leaves(call, &store, actor, &mut |call, actor| {
                    run_named(call, &store, actor, &name, params)
                })?,
                Op::NamedChildren | Op::PropagateCommand => {
                    for child in children(call, &store, actor)? {
                        play(
                            call,
                            &store,
                            child,
                            &name,
                            params,
                            matches!(op, Op::PropagateCommand),
                        )?;
                    }
                }
                _ => {
                    let propagate = store.lock().unwrap().actors[&actor].propagate;
                    if !propagate {
                        run_named(call, &store, actor, &name, params)?;
                    }
                    for child in children(call, &store, actor)? {
                        play(call, &store, child, &name, params, true)?;
                    }
                }
            }
        }
        Op::Recursive => {
            // The existing production class exposes this name but has no
            // semantic implementation. Migration must preserve that error.
            return Err(call.error(
                b"native actor method RunCommandsRecursively has no semantic implementation",
            ));
        }
        Op::OnChildren | Op::OnLeaves => {
            let function = arg(call, 1);
            if matches!(function, Value::Function(_) | Value::Native(_)) {
                let params = call.arguments().get(2).copied();
                match op {
                    Op::OnLeaves => leaves(call, &store, actor, &mut |call, actor| {
                        run(call, actor, function, None)
                    })?,
                    _ => {
                        for child in children(call, &store, actor)? {
                            run(call, child, function, params)?;
                        }
                    }
                }
            }
        }
        Op::Set(name) => set_scalar(call, &store, actor, name)?,
        Op::Get(name) => return get_scalar(call, &store, actor, name),
        Op::PrepareDefinitions
        | Op::LoadPreparationActor
        | Op::OfflineConstructor(_)
        | Op::Instantiate
        | Op::Construct
        | Op::Initialize
        | Op::SetParent
        | Op::Append
        | Op::DiscoverStartup
        | Op::Template
        | Op::LoadFont
        | Op::Release
        | Op::Retire
        | Op::Start
        | Op::Advance
        | Op::End
        | Op::DrawScene
        | Op::Message(_) => {
            unreachable!()
        }
    }
    call.push(Value::Table(actor))
}
fn command_name(call: &HostCall<'_>, value: Value) -> Option<Vec<u8>> {
    let Value::String(id) = value else {
        return None;
    };
    std::str::from_utf8(call.string_bytes(id)?)
        .ok()
        .map(|name| name.as_bytes().to_vec())
}
fn set_scalar(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
) -> Result<(), Error> {
    if name == "draworder" {
        if let Some(value) = integer_scalar(call, arg(call, 1)) {
            action_capture::prepare_write(call, store, actor)?;
            store
                .lock()
                .unwrap()
                .actors
                .get_mut(&actor)
                .unwrap()
                .state
                .draw_order = value;
            let key = call.try_intern(b"__songlua_state_draw_order")?;
            call.raw_table_set(actor, key, Value::Number(value as f64))?;
            capture::property(call, store, actor, name);
            song_properties::record(
                call,
                store,
                actor,
                crate::SongLuaOverlayUpdateTarget::DrawOrder,
                crate::SongLuaOverlayUpdateValue::I32(value),
            );
        }
        return Ok(());
    }
    let allows_nonfinite = matches!(
        name,
        "x" | "y" | "zoom" | "zoomx" | "zoomy" | "zoomz" | "rotationx" | "rotationy" | "rotationz"
    );
    if name == "xy" {
        let store_ref = store;
        let x = float_scalar(call, arg(call, 1), false);
        let y = float_scalar(call, arg(call, 2), false);
        if x.is_some() || y.is_some() {
            action_capture::prepare_write(call, store, actor)?;
        }
        let mut store = store.lock().unwrap();
        let position = store.actors.get_mut(&actor).unwrap().destination_mut();
        if let Some(x) = x {
            position.x = x;
        }
        if let Some(y) = y {
            position.y = y;
        }
        drop(store);
        capture::write(
            call,
            store_ref,
            actor,
            crate::SongLuaOverlayStateDelta {
                x,
                y,
                ..Default::default()
            },
            false,
        );
        song_replay::publish_scalar(call, store_ref, actor, name)?;
        return Ok(());
    }
    let first = if name == "visible" {
        let Some(value) = boolish(call, arg(call, 1)) else {
            return Ok(());
        };
        value as u8 as f32
    } else {
        let Some(value) = float_scalar(call, arg(call, 1), allows_nonfinite) else {
            return Ok(());
        };
        value
    };
    let second = if name == "setsize" {
        let Some(value) = float_scalar(call, arg(call, 2), false) else {
            return Ok(());
        };
        value
    } else {
        0.
    };
    if matches!(name, "SetWidth" | "SetHeight") {
        sprites::refresh_size(call, store, actor)?;
    }
    if matches!(name, "SetWidth" | "SetHeight" | "setsize" | "visible") {
        action_capture::prepare_write(call, store, actor)?;
    }
    if matches!(name, "SetWidth" | "SetHeight" | "setsize") {
        let size = {
            let store = store.lock().unwrap();
            let actor = &store.actors[&actor];
            (actor.state.class == "ActorFrameTexture").then(|| match name {
                "SetWidth" => [first, actor.state.size[1].abs()],
                "SetHeight" => [actor.state.size[0].abs(), first],
                _ => [first, second],
            })
        };
        if let Some(size) = size {
            let value = sprites::vector_value(call, &size)?;
            let key = call.try_intern(b"__songlua_state_size")?;
            call.raw_table_set(actor, key, value)?;
        }
    }
    if name == "visible" {
        sprites::set(
            call,
            actor,
            "__songlua_visible",
            Value::Boolean(first != 0.),
        )?;
    }
    let preparation = {
        let store = store.lock().unwrap();
        !store.playback || store.actors[&actor].replay.is_some()
    };
    let raw_epoch = store.lock().unwrap().actors[&actor].raw_live_epoch;
    let captured_position = if raw_epoch && matches!(name, "addx" | "addy" | "addz") {
        Some(live_epoch::destination(call, actor, &name[3..])?)
    } else if preparation && matches!(name, "addx" | "addy" | "addz") {
        text::captured_position(call, actor, &name[3..])?
    } else {
        None
    };
    action_capture::prepare_write(call, store, actor)?;
    let id = actor;
    let mut guard = store.lock().unwrap();
    let actor = guard.actors.get_mut(&id).unwrap();
    let captured_position = captured_position.or_else(|| {
        actor.replay.as_ref().and_then(|_| {
            let pose = actor.getter_destination();
            match name {
                "addx" => Some(pose.x),
                "addy" => Some(pose.y),
                "addz" => Some(pose.z),
                _ => None,
            }
        })
    });
    let immediate = &mut actor.state;
    match name {
        "SetWidth" => immediate.size = [first, immediate.size[1].abs()],
        "SetHeight" => immediate.size = [immediate.size[0].abs(), first],
        "setsize" => immediate.size = [first, second],
        "visible" => immediate.visible = first != 0.,
        _ => {}
    }
    if matches!(name, "SetWidth" | "SetHeight" | "setsize") {
        immediate.render.size = Some(immediate.size);
    }
    let s = actor.destination_mut();
    match name {
        "x" => s.x = first,
        "y" => s.y = first,
        "z" => s.z = first,
        "addx" => s.x = captured_position.unwrap_or(s.x) + first,
        "addy" => s.y = captured_position.unwrap_or(s.y) + first,
        "addz" => s.z = captured_position.unwrap_or(s.z) + first,
        "zoom" => {
            s.zoom = first;
            s.zoom_x = first;
            s.zoom_y = first;
            s.zoom_z = first;
        }
        "zoomx" => s.zoom_x = first,
        "zoomy" => s.zoom_y = first,
        "zoomz" => s.zoom_z = first,
        "rotationx" => s.rot_x_deg = first,
        "rotationy" => s.rot_y_deg = first,
        "rotationz" => s.rot_z_deg = first,
        "addrotationx" => s.rot_x_deg += first,
        "addrotationy" => s.rot_y_deg += first,
        "addrotationz" => s.rot_z_deg += first,
        "aux" => s.aux = first,
        "SetWidth" | "SetHeight" | "setsize" | "visible" => {}
        _ => unreachable!(),
    }
    match name {
        "rotationx" | "addrotationx" => actor.state.rotation[0] = actor.destination().rot_x_deg,
        "rotationy" | "addrotationy" => actor.state.rotation[1] = actor.destination().rot_y_deg,
        "rotationz" | "addrotationz" => actor.state.rotation[2] = actor.destination().rot_z_deg,
        _ => {}
    }
    let template_position = if actor.offline_template {
        match name {
            "x" | "addx" => Some(("x", actor.destination().x)),
            "y" | "addy" => Some(("y", actor.destination().y)),
            "z" | "addz" => Some(("z", actor.destination().z)),
            _ => None,
        }
    } else {
        None
    };
    drop(guard);
    capture::property(call, store, id, name);
    if let Some((axis, value)) = template_position {
        text::capture_template_position(call, id, axis, value)?;
    }
    if matches!(name, "zoom" | "zoomx" | "zoomy") {
        text::pre_zoom(call, store, id, name != "zoomy", name != "zoomx")?;
    }
    song_replay::publish_scalar(call, store, id, name)
}
fn float_scalar(call: &HostCall<'_>, value: Value, allows_nonfinite: bool) -> Option<f32> {
    match value {
        Value::Number(value) => {
            let value = value as f32;
            (allows_nonfinite || value.is_finite()).then_some(value)
        }
        Value::String(id) => std::str::from_utf8(call.string_bytes(id)?)
            .ok()?
            .trim()
            .parse()
            .ok(),
        _ => None,
    }
}
fn integer_scalar(call: &HostCall<'_>, value: Value) -> Option<i32> {
    match value {
        Value::Number(value) if value.is_finite() => Some(value.round() as i32),
        Value::String(id) => {
            let text = std::str::from_utf8(call.string_bytes(id)?).ok()?.trim();
            text.parse()
                .ok()
                .or_else(|| text.parse::<f32>().ok().map(|value| value.round() as i32))
        }
        _ => None,
    }
}
fn boolish(call: &HostCall<'_>, value: Value) -> Option<bool> {
    match value {
        Value::Boolean(value) => Some(value),
        Value::Number(value) => Some(value != 0.0),
        Value::String(id) => {
            let text = std::str::from_utf8(call.string_bytes(id)?).ok()?.trim();
            if ["true", "yes", "on"]
                .iter()
                .any(|word| text.eq_ignore_ascii_case(word))
            {
                Some(true)
            } else if ["false", "no", "off"]
                .iter()
                .any(|word| text.eq_ignore_ascii_case(word))
            {
                Some(false)
            } else {
                text.parse::<f32>().ok().map(|value| value != 0.0)
            }
        }
        _ => None,
    }
}
fn get_scalar(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
) -> Result<(), Error> {
    if matches!(
        name,
        "GetWidth" | "GetHeight" | "GetZoomedWidth" | "GetZoomedHeight"
    ) {
        sprites::refresh_size(call, store, actor)?;
    }
    let store = store.lock().unwrap();
    let prior = song_pose::prior_position(&store, actor);
    let actor = &store.actors[&actor];
    let s = &actor.state;
    let current = &actor.current;
    let getters =
        actor
            .preparation_getters
            .unwrap_or([current.x, current.y, current.z, current.aux]);
    let dest = actor.getter_destination();
    let value = match name {
        "GetX" => prior.map_or(getters[0], |p| p[0]),
        "GetY" => prior.map_or(getters[1], |p| p[1]),
        "GetZ" => prior.map_or(getters[2], |p| p[2]),
        "GetDestX" => dest.x,
        "GetDestY" => dest.y,
        "GetDestZ" => dest.z,
        "GetZoom" | "GetZoomX" => dest.zoom_x,
        "GetZoomY" => dest.zoom_y,
        "GetZoomZ" => dest.zoom_z,
        "GetRotationX" => s.rotation[0],
        "GetRotationY" => s.rotation[1],
        "GetRotationZ" => s.rotation[2],
        "GetWidth" => s.size[0].abs(),
        "GetHeight" => s.size[1].abs(),
        "GetZoomedWidth" => s.size[0].abs() * (s.render.basezoom_x * dest.zoom_x),
        "GetZoomedHeight" => s.size[1].abs() * (s.render.basezoom_y * dest.zoom_y),
        "getaux" => getters[3],
        "GetVisible" => return call.push(Value::Boolean(s.visible)),
        _ => unreachable!(),
    };
    call.push(Value::Number(f64::from(value)))
}
