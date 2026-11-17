use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {ActorScene, SongActivation, SongLayerPlan, SongRuntime, test_support},
};
use std::{fs, path::Path};

struct Pair {
    rust: SongRuntime,
    scene: ActorScene,
    context: SongLuaCompileContext,
}

fn saved(pair: &mut Pair, key: &str) -> deadlib_lua::TableId {
    let source = format!("return saved.{key}");
    let Value::Table(actor) = pair
        .rust
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"=capture identity").unwrap())
        .unwrap()[0]
    else {
        panic!()
    };
    actor
}

#[test]
fn captured_initial_update_blocks_and_wrapper_defaults_match_compiler() {
    for live in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        activation_snapshot::register_path(dir.path());
        let path = write(
            dir.path(),
            "default.lua",
            r#"
            return Def.ActorFrame{
                InitCommand=function(self)saved.parent=self;self:x(4)end,
                OnCommand=function(self)
                    self:x(9):linear(2):x(40):queuecommand('Later')
                    saved.wrapper=self:AddWrapperState():x(19)
                    self:SetUpdateFunction(function(self)
                        self:x(50);saved.zero_wrapper=self:AddWrapperState():y(27)
                        collectgarbage()
                    end)
                end,
                LaterCommand=function()log('unexpected-future')end,
                Def.ActorFrame{
                    InitCommand=function(self)saved.child=self;self:y(3)end,
                    OnCommand=function(self)
                        self:y(8);self:SetUpdateFunction(function(self)self:y(12)end)
                    end,
                },
            }
        "#,
        );
        let mut context = SongLuaCompileContext::new(dir.path(), "startup capture");
        context.live = live;
        let mut pair = Pair::new(context);
        let rust = pair.activate(&[&path]);
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::captured_initial_update_blocks_and_wrapper_defaults_match_compiler::eq1",
            &(rust.updates.len()),
        );
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::captured_initial_update_blocks_and_wrapper_defaults_match_compiler::eq2",
            &(rust.tweens.len()),
        );
        for key in ["parent", "child"] {
            let b = saved(&mut pair, key);
            reference_test::assert_debug(
                "song/tests/rust_song_activation.rs::captured_initial_update_blocks_and_wrapper_defaults_match_compiler::eq3",
                &(rust.updates[&b]),
            );
            if !live {
                reference_test::assert_debug(
                    "song/tests/rust_song_activation.rs::captured_initial_update_blocks_and_wrapper_defaults_match_compiler::eq4",
                    &(rust.tweens[&b]),
                );
            }
        }
        for key in ["wrapper", "zero_wrapper"] {
            let b = saved(&mut pair, key);

            reference_test::assert_debug(
                "song/tests/rust_song_activation.rs::captured_initial_update_blocks_and_wrapper_defaults_match_compiler::eq5",
                &(&rust.wrapper_initials[&b]),
            );
            assert_eq!(
                &rust.wrapper_initials[&b],
                &deadsync_song_lua::SongLuaOverlayState::default()
            );
            assert!(!rust.updates.contains_key(&b));
            if !live {
                reference_test::assert_debug(
                    "song/tests/rust_song_activation.rs::captured_initial_update_blocks_and_wrapper_defaults_match_compiler::eq7",
                    &(rust.tweens[&b]),
                );
            }
        }
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::captured_initial_update_blocks_and_wrapper_defaults_match_compiler::eq8",
            &(rust.broadcasts),
        );
    }
}

#[test]
fn zero_update_snapshot_keeps_removed_actors_and_strips_only_unchanged_prefixes() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.parent=self end,
            OnCommand=function(self)self:SetUpdateFunction(function(self)self:finishtweening():x(20)end)end,
            Def.ActorFrame{
                InitCommand=function(self)saved.first=self end,
                OnCommand=function(self)
                    self:linear(1):x(5)
                    self:SetUpdateFunction(function(self)
                        saved.parent[2]=nil;collectgarbage();self:linear(2):x(10)
                    end)
                end,
            },
            Def.ActorFrame{
                InitCommand=function(self)saved.gone=self end,
                OnCommand=function(self)self:x(7);self:SetUpdateFunction(function()log('removed-update')end)end,
            },
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "capture retained identities",
    ));
    let rust = pair.activate(&[&path]);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::zero_update_snapshot_keeps_removed_actors_and_strips_only_unchanged_prefixes::eq1",
        &(rust.updates.len()),
    );
    for key in ["parent", "first", "gone"] {
        let b = saved(&mut pair, key);
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::zero_update_snapshot_keeps_removed_actors_and_strips_only_unchanged_prefixes::eq2",
            &(rust.updates[&b]),
        );
    }
    pair.script("collectgarbage();log(saved.gone:GetX())");
    assert_eq!(pair.compare(), "7");
}

#[test]
fn startup_broadcast_capture_preserves_phase_params_and_nested_delivery() {
    for (live, defer) in [(false, false), (true, false), (true, true)] {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        activation_snapshot::register_path(dir.path());
        let path = write(
            dir.path(),
            "default.lua",
            r#"
            return Def.ActorFrame{
                InitCommand=function(self)
                    MESSAGEMAN:Broadcast('InitEvent')
                    self:SetUpdateFunction(function()
                        MESSAGEMAN:Broadcast('UpdateEvent',{value=3})
                        MESSAGEMAN:Broadcast('NilParams',nil)
                    end)
                end,
                BeginCommand=function()MESSAGEMAN:Broadcast('BeginEvent')end,
                OnCommand=function()MESSAGEMAN:Broadcast('OnEvent',{})end,
                UpdateEventMessageCommand=function()MESSAGEMAN:Broadcast('NestedEvent')end,
            }
        "#,
        );
        let mut context = SongLuaCompileContext::new(dir.path(), "startup broadcast capture");
        context.live = live;
        context.defer_initial_live_update = defer;
        let mut pair = Pair::new(context);
        let rust = pair.activate(&[&path]);
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::startup_broadcast_capture_preserves_phase_params_and_nested_delivery::eq1",
            &(rust.broadcasts),
        );
        assert_eq!(rust.updates.is_empty(), defer);
        if !live {
            assert_eq!(
                rust.broadcasts
                    .iter()
                    .map(|(_, name, has_params)| (name.as_str(), *has_params))
                    .collect::<Vec<_>>(),
                [
                    ("BeginEvent", false),
                    ("OnEvent", true),
                    ("UpdateEvent", true),
                    ("NestedEvent", false),
                    ("NilParams", true)
                ]
            );
        }
    }
}
impl Pair {
    fn new(context: SongLuaCompileContext) -> Self {
        let mut rust = SongRuntime::new(&context).unwrap();
        rust.begin_song_compilation().unwrap();
        let (scene, selected) = rust.construct_gameplay_screen(&context).unwrap();
        rust.load_gameplay_theme(scene, &selected).unwrap();
        let mut pair = Self {
            rust,
            scene,
            context,
        };
        pair.script(
            r#"events={}; saved={}
            function log(...)
                local t={}; for i=1,select('#',...)do t[i]=tostring(select(i,...))end
                events[#events+1]=table.concat(t,',')
            end"#,
        );
        pair
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"=activation probe").unwrap())
            .unwrap();
        self.compare();
    }
    fn compare(&mut self) -> String {
        let result = self
            .rust
            .vm_mut()
            .execute(&compile_named(b"return table.concat(events,'|')", b"=events").unwrap())
            .unwrap()[0];
        let Value::String(result) = result else {
            panic!("event string expected")
        };
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare::eq1",
            &(String::from_utf8_lossy(self.rust.vm_mut().string_bytes(result).unwrap())),
        );
        String::from_utf8(self.rust.vm_mut().string_bytes(result).unwrap().to_vec()).unwrap()
    }
    fn activate(&mut self, paths: &[&Path]) -> SongActivation {
        let plan = SongLayerPlan::new(paths, 0, &self.context).unwrap();
        self.rust
            .initialize_song_layers(self.scene, &plan)
            .unwrap()
            .unwrap();
        let rust = self
            .rust
            .activate_song_layers(self.scene, &self.context)
            .unwrap()
            .unwrap();
        self.compare();
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::activate::eq1",
            &(rust.options),
        );
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::activate::eq2",
            &(rust.new_screen),
        );
        rust
    }
}
fn write(dir: &Path, name: &str, source: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, source).unwrap();
    path
}

#[test]
fn startup_order_retained_init_globals_options_and_requests_match_compiler() {
    for live in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        activation_snapshot::register_path(dir.path());
        let path = write(
            dir.path(),
            "default.lua",
            r#"
            return Def.ActorFrame{
                InitCommand=function(self)
                    saved.actor=self
                    log('init',__songlua_screen_loading)
                    mod_actions={tag='actions'};mod_perframes={tag='perframes'}
                    mods={tag='mods'};mods_ease={tag='eases'}
                    SCREENMAN:SetNewScreen('discarded-init')
                    self:queuecommand('Later')
                    self:SetUpdateFunction(function(self,dt)
                        log('update',dt)
                        __songlua_top_screen.__songlua_min_seconds_to_music=99
                        __songlua_top_screen:GetChild('In'):visible(true)
                        SCREENMAN:SetNewScreen('update-target')
                    end)
                end,
                BeginCommand=function(self)log('begin',__songlua_screen_loading)end,
                ScreenChangedMessageCommand=function(self)log('screen-changed')end,
                OnCommand=function(self)
                    log('on');mod_actions={};mod_perframes={};mods={};mods_ease={}
                    __songlua_top_screen.__songlua_min_seconds_to_music=3
                    __songlua_top_screen:GetChild('In'):visible(false)
                    SCREENMAN:SetNewScreen('on-target');collectgarbage()
                end,
                LaterCommand=function(self)log('future-command-must-not-run')end,
                Def.Actor{InitCommand=function(self)log('child-init')end,
                    BeginCommand=function(self)log('child-begin')end,
                    OnCommand=function(self)log('child-on')end},
            }
        "#,
        );
        let mut context = SongLuaCompileContext::new(dir.path(), "activation");
        context.live = live;
        context.screen_loading = true;
        let mut pair = Pair::new(context);
        let rust = pair.activate(&[&path]);
        assert_eq!(rust.options.min_seconds_to_music, Some(3.));
        assert!(rust.options.hide_in);
        assert_eq!(rust.new_screen.as_deref(), Some("update-target"));
        assert_eq!(
            pair.compare(),
            "child-init|init,true|begin,false|child-begin|screen-changed|on|child-on|update,0"
        );
        pair.script(
            "collectgarbage();log('after',__songlua_top_screen.__songlua_min_seconds_to_music)",
        );
        for (i, tag) in ["actions", "perframes", "mods", "eases"]
            .into_iter()
            .enumerate()
        {
            reference_test::assert_debug(
                "song/tests/rust_song_activation.rs::startup_order_retained_init_globals_options_and_requests_match_compiler::eq4",
                &(tag),
            );
            let Value::Table(table) = rust.initialized_globals[i].unwrap() else {
                panic!()
            };
            let key = pair.rust.vm_mut().try_intern(b"tag").unwrap();
            let Value::String(value) = pair.rust.vm_mut().raw_table_get(table, key).unwrap() else {
                panic!()
            };
            assert_eq!(
                pair.rust.vm_mut().string_bytes(value).unwrap(),
                tag.as_bytes()
            );
        }
        assert!(
            pair.rust
                .activate_song_layers(pair.scene, &pair.context)
                .is_err()
        );
    }
}

#[test]
fn deferred_live_update_still_activates_and_retains_transition_request() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)
                SCREENMAN:SetNewScreen('init-only')
                self:SetUpdateFunction(function()log('update')end)
            end,
            BeginCommand=function(self)log('begin')end,
            OnCommand=function(self)
                log('on');__songlua_top_screen.__songlua_min_seconds_to_music=7
                __songlua_top_screen:StartTransitioningScreen('SM_GoToNextScreen')
            end,
        }
    "#,
    );
    let mut context = SongLuaCompileContext::new(dir.path(), "defer update");
    context.live = true;
    context.defer_initial_live_update = true;
    let mut pair = Pair::new(context);
    let rust = pair.activate(&[&path]);
    assert!(rust.new_screen.is_none());
    assert_eq!(rust.options.min_seconds_to_music, Some(7.));
    assert_eq!(pair.compare(), "begin|on");
    assert!(pair.rust.take_screen_requests().0.is_some());
}

#[test]
fn on_and_zero_update_follow_mutated_raw_children_and_preserve_partial_errors() {
    for live in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        activation_snapshot::register_path(dir.path());
        let path = write(
            dir.path(),
            "default.lua",
            r#"
            local late=Def.ActorFrame{
                OnCommand=function(self)log('late-on',self.__songlua_parent==saved.parent)end,
                InitCommand=function(self)
                    self:SetUpdateFunction(function(self,dt)log('late-update',dt)end)
                end,
            }
            return Def.ActorFrame{
                InitCommand=function(self)saved.parent=self end,
                OnCommand=function(self)
                    log('parent-on');self[3]=late
                    self:SetUpdateFunction(function()log('parent-update')end)
                end,
                Def.ActorFrame{
                    InitCommand=function(self)
                        self:SetUpdateFunction(function()
                            log('first-update');saved.parent[2]=late;collectgarbage();error('partial update')
                        end)
                    end,
                    OnCommand=function(self)log('first-on')end,
                },
                Def.ActorFrame{
                    InitCommand=function(self)self:SetUpdateFunction(function()log('removed-update')end)end,
                    OnCommand=function(self)log('second-on')end,
                },
            }
        "#,
        );
        let mut context = SongLuaCompileContext::new(dir.path(), "dynamic startup");
        context.live = live;
        let mut pair = Pair::new(context);
        pair.activate(&[&path]);
        let events = pair.compare();
        assert!(events.contains("late-on,true"));
        assert!(!events.contains("removed-update"));
        assert!(events.ends_with("parent-update"));
    }
}

#[test]
fn scheduled_layers_defer_on_and_updates_but_foreground_receives_begin() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let source = r#"local name=__songlua_current_script_path:match('([^/]+)%.lua$')
        return Def.ActorFrame{
            InitCommand=function(self)
                log(name,'init');self:SetUpdateFunction(function()log(name,'update')end)
            end,
            BeginCommand=function()log(name,'begin')end,
            OnCommand=function()log(name,'on')end,
        }"#;
    let background = write(dir.path(), "background.lua", source);
    let foreground = write(dir.path(), "foreground.lua", source);
    let mut context = SongLuaCompileContext::new(dir.path(), "deferred layers");
    context.live = true;
    context.background_layer_count = 1;
    context.layer_start_beats = vec![8., 16.];
    context.foreground_names = vec!["foreground.lua".into()];
    let mut pair = Pair::new(context);
    pair.activate(&[&background, &foreground]);
    let events = pair.compare();
    assert!(events.contains("foreground,begin"));
    assert!(!events.contains("background,begin"));
    assert!(!events.contains(",on"));
    assert!(!events.contains(",update"));
}

#[test]
fn startup_queue_collection_preserves_enqueue_order_and_includes_zero_update() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            Name='parent',
            OnCommand=function(self)self:queuecommand('Later')end,
            LaterCommand=function()log('future-parent')end,
            Def.ActorFrame{
                Name='child',
                OnCommand=function(self)self:queuecommand('Later')end,
                LaterCommand=function()log('future-child')end,
            },
            Def.ActorFrame{
                Name='update',
                InitCommand=function(self)
                    self:SetUpdateFunction(function(self)self:queuecommand('Later')end)
                end,
                LaterCommand=function()log('future-update')end,
            },
        }
    "#,
    );
    let context = SongLuaCompileContext::new(dir.path(), "queue collection");
    let mut pair = Pair::new(context);
    let _ = pair.activate(&[&path]);
    let queue = test_support::song_startup_queue(&pair.rust);
    let actual = queue
        .into_iter()
        .map(|t| {
            let Value::Table(t) = t else { panic!() };
            let key = pair.rust.vm_mut().try_intern(b"Name").unwrap();
            let Value::String(n) = pair.rust.vm_mut().raw_table_get(t, key).unwrap() else {
                panic!()
            };
            String::from_utf8_lossy(pair.rust.vm_mut().string_bytes(n).unwrap()).into_owned()
        })
        .collect::<Vec<_>>();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::startup_queue_collection_preserves_enqueue_order_and_includes_zero_update::eq1",
        &(actual),
    );
    assert_eq!(pair.compare(), "");
}

#[test]
fn malformed_initialized_globals_stop_at_the_same_request_boundary() {
    for malformed in ["mod_actions", "mod_perframes", "mods", "mods_ease"] {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        activation_snapshot::register_path(dir.path());
        let path = write(
            dir.path(),
            "default.lua",
            &format!(
                r#"
            return Def.ActorFrame{{
                InitCommand=function(self)
                    log('init');SCREENMAN:SetNewScreen('init-request');{malformed}=false
                end,
                BeginCommand=function()log('unexpected-begin')end,
            }}
        "#
            ),
        );
        let context = SongLuaCompileContext::new(dir.path(), "typed read");
        let mut pair = Pair::new(context);

        let plan = SongLayerPlan::new(&[&path], 0, &pair.context).unwrap();
        pair.rust.initialize_song_layers(pair.scene, &plan).unwrap();
        let error = pair
            .rust
            .activate_song_layers(pair.scene, &pair.context)
            .unwrap_err();
        assert!(error.to_string().contains("table"));
        assert_eq!(pair.compare(), "init");
        // The first typed read precedes request consumption; later reads follow it.
        assert_eq!(
            pair.rust.take_screen_requests().0.is_some(),
            malformed == "mod_actions"
        );
    }
}

#[test]
fn reference_clock_is_applied_after_begin_and_before_message_on_and_update() {
    for live in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        activation_snapshot::register_path(dir.path());
        let path = write(
            dir.path(),
            "default.lua",
            r#"
            local function clock(phase)
                local r=__songlua_compile_song_runtime
                log(phase,r.__songlua_song_beat,r.__songlua_music_seconds)
            end
            return Def.ActorFrame{
                InitCommand=function(self)self:SetUpdateFunction(function()clock('update')end)end,
                BeginCommand=function()clock('begin')end,
                ScreenChangedMessageCommand=function()clock('message')end,
                OnCommand=function()clock('on')end,
            }
        "#,
        );
        let mut context = SongLuaCompileContext::new(dir.path(), "startup clock");
        context.live = live;
        context.compile_clock_inputs = vec![[12., 8., 0.]];
        let mut pair = Pair::new(context);
        pair.activate(&[&path]);
        let events = pair.compare();
        assert!(
            events.ends_with("message,8,12|on,8,12|update,8,12"),
            "{events}"
        );
    }
}

#[test]
fn zero_update_visits_wrappers_and_honors_sleep_and_pending_callback_fields() {
    for live in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        activation_snapshot::register_path(dir.path());
        let path = write(
            dir.path(),
            "default.lua",
            r#"
            return Def.ActorFrame{
                OnCommand=function(self)
                    self:AddWrapperState():SetUpdateFunction(function()log('wrapper');collectgarbage()end)
                    self:SetUpdateFunction(function()log('new-callback')end)
                    self.__songlua_update_pending=true
                    self.__songlua_update_previous=function()log('previous-callback')end
                end,
                Def.ActorFrame{
                    OnCommand=function(self)
                        self:hibernate(2):SetUpdateFunction(function()log('sleeping')end)
                        self:AddWrapperState():SetUpdateFunction(function()log('sleeping-wrapper')end)
                    end,
                },
                Def.ActorFrame{
                    OnCommand=function(self)
                        self.__songlua_theme_hibernating=true
                        self:SetUpdateFunction(function()log('hibernating-theme')end)
                    end,
                },
                Def.ActorFrame{
                    OnCommand=function(self)self:SetUpdateFunction(function()log('awake')end)end,
                },
            }
        "#,
        );
        let mut context = SongLuaCompileContext::new(dir.path(), "initial update gates");
        context.live = live;
        let mut pair = Pair::new(context);
        pair.activate(&[&path]);
        assert_eq!(pair.compare(), "wrapper|awake|previous-callback");
    }
}

#[test]
fn failed_on_preserves_partial_state_and_releases_new_constructor_owners() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            OnCommand=function(self)
                saved.extra=Def.ActorFrame{Def.Actor{}};saved.value=42
                log('before-error');collectgarbage();error('activation failure')
            end,
        }
    "#,
    );
    let context = SongLuaCompileContext::new(dir.path(), "activation failure");
    let mut pair = Pair::new(context);

    let plan = SongLayerPlan::new(&[&path], 0, &pair.context).unwrap();
    pair.rust.initialize_song_layers(pair.scene, &plan).unwrap();
    let error = pair
        .rust
        .activate_song_layers(pair.scene, &pair.context)
        .unwrap_err();
    assert!(error.to_string().contains("activation failure"));
    pair.script("assert(saved.value==42 and saved.extra);log('retained')");
    assert_eq!(pair.compare(), "before-error|retained");
    assert!(
        pair.rust
            .activate_song_layers(pair.scene, &pair.context)
            .is_err()
    );
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    let after = test_support::actor_ownership_counts(&mut pair.rust);
    let mut control = Pair::new(pair.context.clone());
    control
        .rust
        .initialize_song_layers(control.scene, &plan)
        .unwrap();
    control.rust.release_actor_scene(control.scene).unwrap();
    assert_eq!(
        after,
        test_support::actor_ownership_counts(&mut control.rust),
        "activation must release every new constructor owner, matching a control stopped after Init"
    );
    assert!(
        pair.rust
            .activate_song_layers(pair.scene, &pair.context)
            .unwrap()
            .is_none()
    );
}

#[test]
fn queued_discovery_rewinds_shared_cells_cycles_and_global_bindings_after_gc() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        local state={value=2}; state.self=state
        local original=state; local counter=7; local optional=nil
        local function read() return counter,optional,state.value,original.value,state==original,original.self==original end
        return Def.ActorFrame{
            InitCommand=function(self) saved.actor=self; self.Read=read end,
            OnCommand=function(self) self:queuecommand('Probe') end,
            ProbeCommand=function(self)
                original.value=20; original.added={}; counter=99; optional='set'
                state={value=30}; brand_new_global={}; stable_global=nil
                log('probe',read()); self:x(41)
                collectgarbage(); collectgarbage()
            end,
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "shared discovery state",
    ));
    pair.script("stable_global='original'");
    let rust = pair.activate(&[&path]);

    pair.rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap();
    assert_eq!(pair.compare(), "probe,99,set,30,20,false,true");
    pair.script("collectgarbage();collectgarbage();log(saved.actor.Read(),brand_new_global~=nil,stable_global==nil)");

    pair.rust.restore_song_startup_locals(pair.scene).unwrap();
    assert_eq!(pair.compare(), "");
    pair.script(
        "log(saved.actor.Read());log(brand_new_global==nil,stable_global,saved.actor:GetX())",
    );
    assert_eq!(pair.compare(), "7,nil,2,2,true,true|true,original,41");
    // A consumed snapshot must not erase subsequent real playback mutations.
    pair.script("stable_global='playback'");

    pair.rust.restore_song_startup_locals(pair.scene).unwrap();
    pair.script("log(stable_global)");
    assert!(pair.compare().ends_with("playback"));
    assert!(
        pair.rust
            .discover_song_startup_commands(pair.scene, &rust)
            .is_err()
    );
}

#[test]
fn queued_discovery_preserves_custom_environment_targets_and_referenced_tables() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        global_state={n=4}
        local local_state={n=3}
        local target={counter=5,bucket={n=6}}
        local environment=setmetatable({__songlua_env_target=target},{__index=function(_,k)
            local value=target[k]; if value~=nil then return value end; return _G[k]
        end,__newindex=target})
        local function read() return target.counter,target.bucket.n,local_state.n,global_state.n,target.extra end
        local probe=setfenv(function(self)
            counter=50; bucket.n=60; local_state.n=30; global_state.n=40; extra='probe'
            self:x(12); collectgarbage()
        end,environment)
        return Def.ActorFrame{
            InitCommand=function(self)saved.actor=self;self.Read=read end,
            OnCommand=function(self)self:queuecommand('Probe')end,
            ProbeCommand=probe,
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "environment discovery state",
    ));
    let rust = pair.activate(&[&path]);

    pair.rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap();
    pair.script("log(saved.actor.Read())");
    assert_eq!(pair.compare(), "50,60,30,40,probe");

    pair.rust.restore_song_startup_locals(pair.scene).unwrap();
    pair.script("collectgarbage();log(saved.actor.Read())");
    // This probe never references log/events, so that unrelated table keeps
    // the diagnostic written after capture in both engines.
    assert_eq!(pair.compare(), "50,60,30,40,probe|5,6,3,4,nil");
}

#[test]
fn live_and_replaced_startup_screens_never_probe_or_rewind() {
    for (live, replaced) in [(true, false), (false, true)] {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        activation_snapshot::register_path(dir.path());
        let path = write(
            dir.path(),
            "default.lua",
            r#"
            return Def.ActorFrame{
                OnCommand=function(self)
                    self:queuecommand('Probe')
                    if replace_screen then SCREENMAN:SetNewScreen('next') end
                end,
                ProbeCommand=function()log('unexpected probe')end,
            }
        "#,
        );
        let mut context = SongLuaCompileContext::new(dir.path(), "skip queued discovery");
        context.live = live;
        let mut pair = Pair::new(context);
        pair.script(&format!("replace_screen={replaced}"));
        let rust = pair.activate(&[&path]);

        pair.rust
            .discover_song_startup_commands(pair.scene, &rust)
            .unwrap();
        pair.script("log('real state')");

        pair.rust.restore_song_startup_locals(pair.scene).unwrap();
        assert_eq!(pair.compare(), "real state");
        assert!(test_support::song_startup_queue(&pair.rust).is_empty());
    }
}

#[test]
fn discovery_error_keeps_partial_mutations_without_a_later_rewind() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        local count=0
        return Def.ActorFrame{
            InitCommand=function(self)saved.actor=self;self.Read=function()return count end end,
            OnCommand=function(self)self:queuecommand('Probe')end,
            ProbeCommand=function(self)count=9;self:x(17);log('partial');error('probe failed')end,
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "failed discovery"));
    let rust = pair.activate(&[&path]);

    let rust_error = pair
        .rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap_err();

    assert!(
        rust_error.to_string().contains("probe failed"),
        "{rust_error}"
    );

    pair.rust.restore_song_startup_locals(pair.scene).unwrap();
    pair.script("collectgarbage();log(saved.actor.Read(),saved.actor:GetX())");
    assert_eq!(pair.compare(), "partial|9,17");
}

#[test]
fn discovery_snapshot_retirement_is_safe_and_foreign_scenes_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        local counter=0
        return Def.ActorFrame{
            InitCommand=function(self)saved.actor=self;self.Read=function()return counter end end,
            OnCommand=function(self)self:queuecommand('Probe')end,
            ProbeCommand=function()counter=1 end,
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "retired discovery"));
    let activation = pair.activate(&[&path]);
    let mut foreign = SongRuntime::new(&pair.context).unwrap();
    assert!(
        !foreign
            .discover_song_startup_commands(pair.scene, &activation)
            .unwrap()
    );
    assert!(!foreign.restore_song_startup_locals(pair.scene).unwrap());
    pair.rust
        .discover_song_startup_commands(pair.scene, &activation)
        .unwrap();
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    assert!(!pair.rust.restore_song_startup_locals(pair.scene).unwrap());
    assert!(
        !pair
            .rust
            .discover_song_startup_commands(pair.scene, &activation)
            .unwrap()
    );
    let value = pair
        .rust
        .vm_mut()
        .execute(
            &compile_named(
                b"collectgarbage();return saved.actor.Read()",
                b"=retired discovery",
            )
            .unwrap(),
        )
        .unwrap()[0];
    assert_eq!(value, Value::Number(1.));
}

#[test]
fn discovery_rewind_handles_more_shared_tables_than_the_native_stack_limit() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        local values={}
        for i=1,8201 do values[i]={n=i} end
        local shared={last=values[8201]}; values.alias=shared; shared.back=values
        return Def.ActorFrame{
            InitCommand=function(self)
                saved.actor=self
                self.Read=function()return values[1].n,values[8201].n,shared.last==values[8201],shared.back==values end
            end,
            OnCommand=function(self)self:queuecommand('Probe')end,
            ProbeCommand=function()
                for i=1,8201 do values[i].n=-i end
                shared.last={}; collectgarbage()
            end,
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "large shared discovery graph",
    ));
    let rust = pair.activate(&[&path]);

    pair.rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap();
    pair.script("collectgarbage();log(saved.actor.Read())");
    assert_eq!(pair.compare(), "-1,-8201,false,true");

    pair.rust.restore_song_startup_locals(pair.scene).unwrap();
    pair.script("collectgarbage();log(saved.actor.Read())");
    assert_eq!(pair.compare(), "-1,-8201,false,true|1,8201,true,true");
}

#[test]
fn queued_actor_captures_separate_startup_writes_and_retain_removed_siblings() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.parent=self end,
            OnCommand=function(self)
                self:x(3):SetUpdateRate(2)
                saved.wrapper=self:AddWrapperState(); saved.wrapper:x(4)
                self:queuecommand('Probe'):linear(2):x(9)
            end,
            ProbeCommand=function(self)
                self:diffusealpha(.4); saved.sibling:y(30); saved.wrapper:rotationz(7)
                self[2]=nil;collectgarbage()
            end,
            Def.Quad{InitCommand=function(self)saved.sibling=self;self:y(6)end},
            Def.Quad{InitCommand=function(self)saved.removed=self;self:z(12)end},
            Def.Quad{InitCommand=function(self)saved.untouched=self;self:x(18)end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "queued actor capture",
    ));
    let activation = pair.activate(&[&path]);
    let before = activation.tweens.clone();

    pair.rust
        .discover_song_startup_commands(pair.scene, &activation)
        .unwrap();
    let rust = pair.rust.song_startup_queued_states(pair.scene).unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::queued_actor_captures_separate_startup_writes_and_retain_removed_siblings::eq1",
        &(rust.len()),
    );
    for key in ["parent", "sibling", "removed", "untouched", "wrapper"] {
        let b = saved(&mut pair, key);
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::queued_actor_captures_separate_startup_writes_and_retain_removed_siblings::eq2",
            &(rust[&b]),
        );
        assert!(
            rust[&b].steps.is_empty()
                && rust[&b].commands.is_empty()
                && rust[&b].block_steps.is_empty()
        );
    }
    let untouched = saved(&mut pair, "untouched");
    assert!(rust[&untouched].blocks.is_empty());
    let removed = saved(&mut pair, "removed");
    assert_eq!(rust[&removed].initial.z, 12.);
    assert_eq!(activation.tweens, before);
    pair.script("collectgarbage();log(saved.removed:GetZ())");

    pair.rust.restore_song_startup_locals(pair.scene).unwrap();
    assert_eq!(
        pair.rust.song_startup_queued_states(pair.scene).unwrap(),
        rust
    );
}

#[test]
fn queued_sprite_bindings_and_startup_text_keep_the_real_zero_frame() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    for name in ["first 2x1.png", "second.png"] {
        image::RgbaImage::from_pixel(8, 4, image::Rgba([255, 255, 255, 255]))
            .save(dir.path().join(name))
            .unwrap();
    }
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            OnCommand=function(self)self:SetUpdateFunction(function()saved.text:settext('zero')end)end,
            Def.Sprite{
                Texture=first,Frames={{Frame=0,Delay=.25},{Frame=1,Delay=.5}},
                FrameExtra={nested={value=6}},DelayExtra={value=7},
                InitCommand=function(self)saved.sprite=self end,
                OnCommand=function(self)self:queuecommand('Probe')end,
                ProbeCommand=function(self)
                    self.Frames[1].Delay=9;self.FrameExtra.nested.value=60;self.DelayExtra.value=70
                    self:Load(second);collectgarbage()
                end,
            },
            Def.BitmapText{
                Font='Common Normal',Text='declared',
                InitCommand=function(self)saved.text=self end,
                OnCommand=function(self)
                    self:settext('on'):queuecommand('Probe')
                end,
                ProbeCommand=function(self)self:settext('future')end,
            },
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "queued media capture",
    ));
    pair.script(&format!(
        "first={:?};second={:?}",
        deadsync_song_lua::file_path_string(&dir.path().join("first 2x1.png")),
        deadsync_song_lua::file_path_string(&dir.path().join("second.png"))
    ));
    let activation = pair.activate(&[&path]);
    pair.script("log(saved.text.__songlua_startup_text)");
    assert_eq!(pair.compare(), "zero");

    pair.rust
        .discover_song_startup_commands(pair.scene, &activation)
        .unwrap();
    let rust = pair.rust.song_startup_queued_states(pair.scene).unwrap();
    for key in ["sprite", "text"] {
        let b = saved(&mut pair, key);
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::queued_sprite_bindings_and_startup_text_keep_the_real_zero_frame::eq2",
            &(rust[&b]),
        );
    }
    pair.script(r#"
        collectgarbage()
        local a=saved.sprite;local s=a.__songlua_startup_sprite
        log(s.Texture==first,a.Texture==second,s.Frames[1].Delay,s.FrameExtra.nested.value,s.DelayExtra.value)
        log(s.Frames~=a.Frames,s.FrameExtra~=a.FrameExtra,saved.text.__songlua_startup_text,saved.text:GetText())
    "#);
    assert_eq!(
        pair.compare(),
        "zero|true,true,0.25,6,7|true,true,zero,future"
    );
}

#[test]
fn startup_media_capture_skips_future_probes_in_live_and_replaced_screens() {
    for (live, replaced) in [(true, false), (false, true)] {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        activation_snapshot::register_path(dir.path());
        let path = write(
            dir.path(),
            "default.lua",
            r#"
            return Def.ActorFrame{
                OnCommand=function()if replaced then SCREENMAN:SetNewScreen('next')end end,
                Def.Sprite{InitCommand=function(self)saved.sprite=self end,OnCommand=function(self)self:queuecommand('Probe')end,ProbeCommand=function()log('future')end},
                Def.RollingNumbers{Font='Common Normal',Text='initial',InitCommand=function(self)saved.text=self end,OnCommand=function(self)self:settext('on')end},
            }
        "#,
        );
        let mut context = SongLuaCompileContext::new(dir.path(), "skipped media capture");
        context.live = live;
        let mut pair = Pair::new(context);
        pair.script(&format!("replaced={replaced}"));
        let activation = pair.activate(&[&path]);

        pair.rust
            .discover_song_startup_commands(pair.scene, &activation)
            .unwrap();

        assert!(
            pair.rust
                .song_startup_queued_states(pair.scene)
                .unwrap()
                .is_empty()
        );
        pair.script(
            "log(saved.text.__songlua_startup_text,saved.sprite.__songlua_startup_sprite==nil)",
        );
        assert_eq!(pair.compare(), "on,true");
    }
}

#[test]
fn malformed_sprite_capture_stops_before_queued_commands_and_releases_the_barrier() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.Sprite{
            InitCommand=function(self)saved.sprite=self end,
            OnCommand=function(self)self:queuecommand('Probe')end,
            ProbeCommand=function()log('probe')end,
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "invalid queued binding",
    ));
    let activation = pair.activate(&[&path]);
    pair.script("saved.sprite[string.char(255)]=1");

    assert!(
        pair.rust
            .discover_song_startup_commands(pair.scene, &activation)
            .is_err()
    );
    assert!(
        pair.rust
            .song_startup_queued_states(pair.scene)
            .unwrap()
            .is_empty()
    );
    assert!(test_support::song_startup_queue(&pair.rust).is_empty());
    assert_eq!(pair.compare(), "");
    pair.script("saved.sprite[string.char(255)]=nil;saved.sprite:queuecommand('Probe')");
    assert_eq!(pair.compare(), "probe|probe");
}

#[test]
fn chronological_replay_rebuilds_startup_queues_after_future_discovery() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        local visits=0
        return Def.ActorFrame{
            InitCommand=function(self)saved.parent=self;self:x(2)end,
            OnCommand=function(self)
                self:x(12):sleep(.5):queuecommand('Later')
                self:SetUpdateRate(2)
            end,
            LaterCommand=function(self)
                visits=visits+1;log('later',visits,self:GetX())
                self:linear(.5):x(32):queuecommand('Done');collectgarbage()
            end,
            DoneCommand=function(self)log('done',self:GetX())end,
            Def.Quad{
                InitCommand=function(self)saved.child=self;self:x(5)end,
                OnCommand=function(self)self:linear(1):x(45):zoom(3):rotationz(90):hibernate(.25)end,
                LaterCommand=function(self)log('child-later',self:GetX());self:addx(8)end,
                DoneCommand=function(self)log('child-done',self:GetX())end,
            },
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "replay startup"));
    let rust = pair.activate(&[&path]);
    let b = saved(&mut pair, "child");

    assert!(
        pair.rust
            .discover_song_startup_commands(pair.scene, &rust)
            .unwrap()
    );
    pair.compare();
    let initial = deadsync_song_lua::SongLuaOverlayState {
        x: 5.,
        ..Default::default()
    };

    assert!(
        pair.rust
            .begin_song_replay(pair.scene, &[(b, initial)], &rust)
            .unwrap()
    );
    pair.script("log('zero',saved.child:GetX(),saved.child:GetDestX(),saved.child:GetZoomX(),saved.child:GetRotationZ(),saved.parent:GetTweenTimeLeft())");
    for (seconds, delta) in [
        (0., 0.),
        (0.125, 0.125),
        (0.25, 0.125),
        (0.5, 0.25),
        (0.75, 0.25),
        (1., 0.25),
    ] {
        pair.rust
            .set_values((seconds * 2.) as f32, seconds as f32)
            .unwrap();

        assert!(pair.rust.advance_actor_scene(pair.scene, delta).unwrap());
        pair.script("log('frame',saved.child:GetX(),saved.child:GetDestX(),saved.child:GetZoomX(),saved.child:GetRotationZ(),saved.parent:GetX(),saved.parent:GetTweenTimeLeft(),saved.child.__songlua_state_x,saved.child.__songlua_current_x);collectgarbage()");
    }
    assert!(
        pair.rust
            .begin_song_replay(pair.scene, &[(b, initial)], &rust)
            .is_err()
    );
}

#[test]
fn replay_boundary_rejects_foreign_retired_live_and_replaced_scenes() {
    for (live, replaced) in [(false, false), (true, false), (false, true)] {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        activation_snapshot::register_path(dir.path());
        let path = write(
            dir.path(),
            "default.lua",
            &format!(
                r#"
            return Def.ActorFrame{{OnCommand=function(self)
                self:sleep(1):queuecommand('Later')
                {}
            end, LaterCommand=function()log('future')end}}
        "#,
                if replaced {
                    "SCREENMAN:SetNewScreen('next')"
                } else {
                    ""
                }
            ),
        );
        let mut context = SongLuaCompileContext::new(dir.path(), "replay validation");
        context.live = live;
        let mut pair = Pair::new(context.clone());
        let activation = pair.activate(&[&path]);
        assert!(
            pair.rust
                .begin_song_replay(pair.scene, &[], &activation)
                .is_err()
        );
        pair.rust
            .discover_song_startup_commands(pair.scene, &activation)
            .unwrap();
        let mut foreign = SongRuntime::new(&context).unwrap();
        assert!(
            !foreign
                .begin_song_replay(pair.scene, &[], &activation)
                .unwrap()
        );
        if live || replaced {
            assert!(
                pair.rust
                    .begin_song_replay(pair.scene, &[], &activation)
                    .is_err()
            );
        }
        pair.rust.retire_actor_scene(pair.scene).unwrap();
        assert!(
            !pair
                .rust
                .begin_song_replay(pair.scene, &[], &activation)
                .unwrap()
        );
    }
}

#[test]
fn replay_restores_sprite_binding_and_only_the_exported_getter_fields() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    for (name, size) in [("first 2x1.png", [8, 4]), ("second.png", [12, 6])] {
        image::RgbaImage::from_pixel(size[0], size[1], image::Rgba([255, 255, 255, 255]))
            .save(dir.path().join(name))
            .unwrap();
    }
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.Sprite{
            Texture=first,Frames={{Frame=0,Delay=.25},{Frame=1,Delay=.5}},FrameExtra={value=7},
            InitCommand=function(self)saved.sprite=self;self:x(3):basezoom(2)end,
            OnCommand=function(self)self:SetStateProperties(self.Frames):queuecommand('Later')end,
            LaterCommand=function(self)
                self:Load(second):visible(false):basezoom(9):diffuse(.25,.5,.75,.5):draworder(17)
                self.FrameExtra.value=70;collectgarbage()
            end,
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "replay media"));
    pair.script(&format!(
        "first={:?};second={:?}",
        deadsync_song_lua::file_path_string(&dir.path().join("first 2x1.png")),
        deadsync_song_lua::file_path_string(&dir.path().join("second.png"))
    ));
    let rust = pair.activate(&[&path]);
    let b = saved(&mut pair, "sprite");

    pair.rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap();
    pair.script("assert(saved.sprite.Texture==second);assert(saved.sprite.FrameExtra.value==70)");
    let initial = deadsync_song_lua::SongLuaOverlayState {
        x: 3.,
        basezoom: 2.,
        basezoom_x: 2.,
        basezoom_y: 2.,
        basezoom_z: 2.,
        sprite_texture: true,
        size: Some([4., 4.]),
        ..Default::default()
    };

    pair.rust
        .begin_song_replay(pair.scene, &[(b, initial)], &rust)
        .unwrap();
    pair.script(r#"
        local a=saved.sprite
        log(a.Texture==first,a.Frames[1].Delay,a:GetNumStates(),a:GetAnimationLengthSeconds(),a:GetWidth(),a:GetHeight())
        log(a:GetX(),a:GetBaseZoomX(),a:GetVisible(),a:GetDiffuseAlpha(),a.FrameExtra.value,a.__songlua_state_draw_order)
        log(a.__songlua_state_diffuse==a.__songlua_diffuse,a.__songlua_capture_cursor,#a.__songlua_capture_blocks)
    "#);
    assert_eq!(
        pair.compare(),
        "true,0.25,2,0.75,4,4|3,2,true,1,70,17|true,0,0"
    );
    for (seconds, delta) in [(0.25, 0.25), (0.5, 0.25), (0.75, 0.25)] {
        pair.rust
            .set_values((seconds * 2.) as f32, seconds as f32)
            .unwrap();

        pair.rust.advance_actor_scene(pair.scene, delta).unwrap();
        pair.script("local a=saved.sprite;log(a.Texture==first,a:GetWidth(),a:GetVisible(),a:GetBaseZoomX(),a:GetDiffuseAlpha());collectgarbage()");
    }
}

#[test]
fn replay_uses_block_step_associations_and_easing_options() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.Quad{InitCommand=function(self)saved.actor=self end,
            OnCommand=function(self)self:linear(1):x(64):linear(1):x(128)end}
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "replay easing"));
    let mut rust = pair.activate(&[&path]);
    let b = saved(&mut pair, "actor");
    // The compiler may annotate captured blocks with easing options while
    // reading declarations. Reconstruction must use those associated blocks.
    for state in [rust.tweens.get_mut(&b).unwrap()] {
        let index = state
            .block_steps
            .iter()
            .position(|step| *step == Some(1))
            .unwrap();
        state.blocks[index].easing = Some("outElastic".into());
        state.blocks[index].opt1 = Some(0.75);
    }

    pair.rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap();
    let initial = deadsync_song_lua::SongLuaOverlayState::default();

    pair.rust
        .begin_song_replay(pair.scene, &[(b, initial)], &rust)
        .unwrap();
    for (seconds, delta) in [
        (0.125, 0.125),
        (0.5, 0.375),
        (1., 0.5),
        (1.5, 0.5),
        (2., 0.5),
    ] {
        pair.rust
            .set_values((seconds * 2.) as f32, seconds as f32)
            .unwrap();

        pair.rust.advance_actor_scene(pair.scene, delta).unwrap();
        pair.script(
            "log(saved.actor:GetX(),saved.actor:GetDestX(),saved.actor:GetTweenTimeLeft())",
        );
    }
}

#[test]
fn replay_color_writes_and_queue_cancellation_publish_native_getters() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.Quad{InitCommand=function(self)saved.actor=self end,
            OnCommand=function(self)self:linear(1):x(40):zoom(2):diffuse(1,0,0,.5)end}
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "replay getters"));
    let rust = pair.activate(&[&path]);
    let b = saved(&mut pair, "actor");

    pair.rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap();
    let initial = deadsync_song_lua::SongLuaOverlayState::default();

    pair.rust
        .begin_song_replay(pair.scene, &[(b, initial)], &rust)
        .unwrap();
    pair.script(r#"
        function show()
            local a=saved.actor
            log(a:GetX(),a:GetDestX(),a:GetZoomX(),a:GetDiffuseAlpha(),a:GetRotationZ(),a.__songlua_state_x,a.__songlua_current_x,a:GetDiffuse()[2])
        end
        show();saved.actor:addx(2);show();saved.actor:diffusealpha(.75);show();saved.actor:stoptweening();show()
        saved.actor:linear(1):x(8):zoom(3):rotationz(90);show()
        saved.actor:finishtweening();show();saved.actor:x(19);show()
        collectgarbage()
    "#);
}

fn compare_tween_progress(
    pair: &mut Pair,
    actors: &[deadlib_lua::TableId],
) -> std::collections::HashMap<deadlib_lua::TableId, deadsync_song_lua::SongTweenReplay> {
    let actual = pair.rust.capture_song_tween_replay(pair.scene).unwrap();
    for b in actors {
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_tween_progress::eq1",
            &(actual.get(b)),
        );
    }
    assert_eq!(
        pair.rust.capture_song_tween_replay(pair.scene).unwrap(),
        actual,
        "sampling twice must not duplicate hold samples"
    );
    actual
}
fn compare_finished_progress(
    pair: &mut Pair,
    actors: &[deadlib_lua::TableId],
    rust: &SongActivation,
) {
    let actual = compare_tween_progress(pair, actors);

    for b in actors {
        let mut blocks = rust.tweens[b].blocks.clone();
        if let Some(replay) = actual.get(b) {
            replay.apply_startup_progress(&mut blocks, &rust.tweens[b]);
        }
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_finished_progress::eq1",
            &(blocks),
        );
    }
}

#[test]
fn tween_progress_preserves_song_time_hibernation_rates_and_wrapper_spans() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.parent=self end,
            OnCommand=function(self)self:SetUpdateRate(2);self:sleep(.5):queuecommand('Later')end,
            LaterCommand=function(self)self:linear(.25):x(20):queuecommand('Done')end,
            DoneCommand=function(self)self:diffusealpha(.5);collectgarbage()end,
            Def.ActorFrame{
                InitCommand=function(self)saved.child=self end,
                OnCommand=function(self)
                    self:SetUpdateRate(3);self:hibernate(.25):linear(1):x(40)
                    saved.wrapper=self:AddWrapperState():hibernate(.125):linear(.5):y(8)
                end,
            },
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "progress spans"));
    let rust = pair.activate(&[&path]);
    let actors = [
        saved(&mut pair, "parent"),
        saved(&mut pair, "child"),
        saved(&mut pair, "wrapper"),
    ];
    assert!(pair.rust.capture_song_tween_replay(pair.scene).is_none());
    assert!(pair.rust.advance_song_replay(pair.scene, 10., 0.).is_err());

    pair.rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap();

    let initial = deadsync_song_lua::SongLuaOverlayState::default();

    let overlays = actors[1..]
        .iter()
        .map(|b| (*b, initial))
        .collect::<Vec<_>>();
    pair.rust
        .begin_song_replay(pair.scene, &overlays, &rust)
        .unwrap();
    for (index, (time, delta)) in [
        (10., 0.),
        (10.25, 0.25),
        (10.5, 0.25),
        (10.75, 0.25),
        (11., 0.25),
        (11.5, 0.5),
        (11.5, 0.),
        (12., 0.5),
    ]
    .into_iter()
    .enumerate()
    {
        pair.rust
            .set_values((time * 2.) as f32, time as f32)
            .unwrap();

        assert!(
            pair.rust
                .advance_song_replay(pair.scene, time, delta)
                .unwrap()
        );
        let capture = compare_tween_progress(&mut pair, &actors);
        if index == 1 {
            assert_eq!(
                capture[&actors[1]].progress[0],
                [[10.125, 1.], [10.25, 0.25]]
            );
            assert_eq!(
                capture[&actors[2]].progress[0],
                [[10.1875, 0.5], [10.25, 0.375]]
            );
            pair.script("saved.parent:hibernate(.75);collectgarbage()");
        }
        if index == 2 {
            assert_eq!(capture[&actors[1]].progress[0].last(), Some(&[10.5, 0.25]));
        }
        if index == 3 {
            pair.script("saved.parent:hurrytweening(.5);saved.child:SetUpdateRate(1)");
        }
    }
    compare_finished_progress(&mut pair, &actors, &rust);
}

#[test]
fn tween_progress_keeps_cancelled_indices_and_tracks_explicit_pose_writes() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.actor=self end,
            OnCommand=function(self)self:linear(1):x(40):linear(2):y(30)end,
            NextCommand=function(self)self:hibernate(.25):linear(.5):x(4);collectgarbage()end,
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "progress cancellation",
    ));
    let rust = pair.activate(&[&path]);
    let actors = [saved(&mut pair, "actor")];

    pair.rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap();

    let initial = deadsync_song_lua::SongLuaOverlayState::default();

    pair.rust
        .begin_song_replay(pair.scene, &[(actors[0], initial)], &rust)
        .unwrap();
    pair.script("saved.actor:stoptweening():diffuse(1,1,1,1):draworder(3):linear(.25):x(1):queuecommand('Next')");
    let capture = compare_tween_progress(&mut pair, &actors);
    assert!(capture[&actors[0]].progress[..2].iter().all(Vec::is_empty));
    assert!(
        capture[&actors[0]]
            .values
            .iter()
            .any(|(target, _)| *target == deadsync_song_lua::SongLuaOverlayUpdateTarget::Diffuse)
    );
    for (time, delta) in [(2., 0.125), (2.25, 0.25), (2.5, 0.25), (3., 0.5)] {
        pair.rust
            .set_values((time * 2.) as f32, time as f32)
            .unwrap();

        pair.rust
            .advance_song_replay(pair.scene, time, delta)
            .unwrap();
        compare_tween_progress(&mut pair, &actors);
    }
    let capture = pair.rust.capture_song_tween_replay(pair.scene).unwrap();
    let mut blocks = rust.tweens[&actors[0]].blocks.clone();
    capture[&actors[0]].apply_startup_progress(&mut blocks, &rust.tweens[&actors[0]]);
    assert!(blocks.iter().all(|block| block.progress.is_none()));
    compare_finished_progress(&mut pair, &actors, &rust);
}

#[test]
fn tween_progress_retains_partial_errors_and_rejects_invalid_scene_handles() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{InitCommand=function(self)saved.actor=self end,
            OnCommand=function(self)self:sleep(.25):queuecommand('Fail')end,
            FailCommand=function(self)self:linear(.5):x(7);error('replay failure')end}
    "#,
    );
    let context = SongLuaCompileContext::new(dir.path(), "progress errors");
    let mut pair = Pair::new(context.clone());
    let rust = pair.activate(&[&path]);
    let actors = [saved(&mut pair, "actor")];

    pair.rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap();

    pair.rust.begin_song_replay(pair.scene, &[], &rust).unwrap();
    pair.rust.set_values(2., 1.).unwrap();

    assert!(pair.rust.advance_song_replay(pair.scene, 1., 0.5).is_err());
    compare_tween_progress(&mut pair, &actors);
    pair.rust.set_values(3., 1.5).unwrap();

    pair.rust.advance_song_replay(pair.scene, 1.5, 0.5).unwrap();
    compare_finished_progress(&mut pair, &actors, &rust);
    let mut foreign = SongRuntime::new(&context).unwrap();
    assert!(foreign.capture_song_tween_replay(pair.scene).is_none());
    assert!(!foreign.advance_song_replay(pair.scene, 2., 0.5).unwrap());
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    assert!(pair.rust.capture_song_tween_replay(pair.scene).is_none());
    assert!(!pair.rust.advance_song_replay(pair.scene, 2., 0.5).unwrap());
}

fn begin_message_replay(pair: &mut Pair, rust: &SongActivation, keys: &[&str]) {
    pair.rust
        .discover_song_startup_commands(pair.scene, rust)
        .unwrap();
    let actors = keys.iter().map(|key| saved(pair, key)).collect::<Vec<_>>();
    let overlays = actors
        .iter()
        .map(|b| (*b, deadsync_song_lua::SongLuaOverlayState::default()))
        .collect::<Vec<_>>();
    assert!(
        pair.rust
            .begin_song_replay(pair.scene, &overlays, rust)
            .unwrap()
    );
}

fn compare_tween_messages(pair: &Pair) -> Vec<deadsync_song_lua::SongLuaStatefulMessageCapture> {
    let actual = pair.rust.song_replay_tween_messages(pair.scene).unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_tween_messages::eq1",
        &(actual),
    );
    assert_eq!(
        actual,
        pair.rust.song_replay_tween_messages(pair.scene).unwrap()
    );
    actual
}

#[test]
fn replay_message_provenance_keeps_nested_cross_actor_and_equal_value_writes() {
    use deadsync_song_lua::SongLuaOverlayUpdateTarget as Target;
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.listener=self end,
            OuterMessageCommand=function(self,p)
                saved.target:x(p.x):x(p.x)
                MESSAGEMAN:Broadcast('Inner',{level=p.level})
                saved.target:y(13)
            end,
            InnerMessageCommand=function(self,p)
                if p.level == 0 then MESSAGEMAN:Broadcast('Outer',{x=4,level=1}) end
                saved.target:zoom(2):zoomx(3):zoomy(4):zoomz(5)
                saved.target:rotationx(6):rotationy(7):rotationz(8):aux(9)
                saved.target:skewx(0.1):skewy(0.2):xy(10,11):z(12)
                saved.target:cropleft(0.1):cropright(0.2):croptop(0.3):cropbottom(0.4)
                saved.target:fadeleft(0.5):faderight(0.6):fadetop(0.7):fadebottom(0.8)
                saved.target:diffuse(1,0.5,0.25,0.75):diffuseupperleft(0.2,0.3,0.4,0.5)
                saved.target:glow(0.1,0.2,0.3,0.4):heading(20)
                __songlua_compile_song_runtime.__songlua_song_beat=17
                saved.target:x(18);collectgarbage()
                saved.untracked:x(100)
            end,
            Def.Quad{
                InitCommand=function(self)saved.target=self end,
                OnCommand=function(self)self:linear(2):x(7)end,
            },
            Def.Quad{
                InitCommand=function(self)saved.untracked=self end,
                OnCommand=function(self)self:linear(1):x(1)end,
            },
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "message provenance"));
    let rust = pair.activate(&[&path]);
    assert!(pair.rust.song_replay_tween_messages(pair.scene).is_none());
    // Shared overlay identities use their last compiler index.
    begin_message_replay(&mut pair, &rust, &["target", "target"]);
    pair.script("MESSAGEMAN:Broadcast('Outer',{x=3,level=0});saved.target:x(99)");
    let captures = compare_tween_messages(&pair);
    assert_eq!(
        captures
            .iter()
            .map(|c| c.message.as_str())
            .collect::<Vec<_>>(),
        ["Inner", "Outer"]
    );
    let inner = &captures[0];
    assert_eq!(inner.overlay_targets[0].0, 1);
    assert_eq!(inner.overlay_targets[0].1.len(), 25);
    assert!(
        inner
            .writes
            .iter()
            .any(|w| w.target == Target::X && w.beat == 17.)
    );
    assert_eq!(
        captures[1]
            .writes
            .iter()
            .filter(|w| w.target == Target::X)
            .count(),
        4
    );
    assert!(
        captures
            .iter()
            .flat_map(|c| &c.writes)
            .all(|w| w.overlay_index == 1)
    );
}

#[test]
fn replay_message_provenance_keeps_tail_delays_hurry_and_queue_dispatch_resets() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.listener=self end,
            OuterMessageCommand=function(self)
                saved.target:hibernate(0.5):linear(0.75):x(10):hurrytweening(0.5):y(20)
                saved.target.__songlua_capture_opt1=0.3
                saved.target:rotationz(30)
                saved.target:linear(0.125):z(31)
                saved.target:sleep(0.25):z(40):queuecommand('Later'):aux(3)
                saved.target:queuemessage('Queued'):x(50)
                collectgarbage()
            end,
            QueuedMessageCommand=function(self)
                saved.target:x(70):linear(0.5):y(80)
            end,
            Def.Quad{
                InitCommand=function(self)saved.target=self end,
                OnCommand=function(self)self:linear(1):x(4)end,
                LaterCommand=function(self)
                    self:x(60);MESSAGEMAN:Broadcast('Nested',{value=61})
                end,
                NestedMessageCommand=function(self,p)self:x(p.value)end,
            },
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "message queue delays",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["target"]);
    pair.script("MESSAGEMAN:Broadcast('Outer')");
    let messages = compare_tween_messages(&pair);
    let writes = &messages[0].writes;
    assert_eq!(
        (writes[0].delay_seconds, writes[0].duration_seconds),
        (1.5, 0.75)
    );
    assert_eq!(
        (writes[1].delay_seconds, writes[1].duration_seconds),
        (0., 0.)
    );
    assert_eq!(writes[1].easing, None);
    assert_eq!(writes[2].opt1, Some(0.3));
    assert_eq!(
        (writes[3].delay_seconds, writes[3].duration_seconds),
        (1.375, 0.125)
    );
    assert_eq!(writes[3].opt1, None);
    assert_eq!(writes[4].duration_seconds, 0.);
    for (time, delta) in [(1., 0.5), (1.5, 0.5), (2., 0.5), (3., 1.)] {
        pair.rust
            .set_values((time * 2.) as f32, time as f32)
            .unwrap();

        pair.rust
            .advance_song_replay(pair.scene, time, delta)
            .unwrap();
        compare_tween_messages(&pair);
    }
    let messages = compare_tween_messages(&pair);
    let nested = messages.iter().find(|m| m.message == "Nested").unwrap();
    assert_eq!(nested.writes[0].duration_seconds, 0.);
    assert_eq!(nested.writes[0].easing, None);
    assert!(messages.iter().any(|m| m.message == "Queued"));
    let actors = [saved(&mut pair, "target")];
    compare_tween_progress(&mut pair, &actors);
}

#[test]
fn replay_message_provenance_survives_errors_and_teardown_without_leaking_scope() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.listener=self end,
            OuterMessageCommand=function(self)
                saved.target:linear(0.5):x(2)
                assert(not pcall(function()MESSAGEMAN:Broadcast('Broken')end))
                saved.target:finishtweening():x(3):linear(1):y(4)
                saved.target:stoptweening():y(5);collectgarbage()
            end,
            BrokenMessageCommand=function(self)
                saved.target:x(6);error('message failure after write')
            end,
            Def.Quad{
                InitCommand=function(self)saved.target=self end,
                OnCommand=function(self)self:linear(1):x(1)end,
            },
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "message scope errors",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["target"]);
    pair.script("MESSAGEMAN:Broadcast('Outer');assert(not pcall(function()MESSAGEMAN:Broadcast('Broken')end));saved.target:z(99)");
    let captured = compare_tween_messages(&pair);
    assert_eq!(captured[0].message, "Broken");
    assert_eq!(captured[0].writes.len(), 2);
    assert_eq!(captured[1].writes.len(), 4);
    let foreign = SongRuntime::new(&pair.context).unwrap();
    assert!(foreign.song_replay_tween_messages(pair.scene).is_none());
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    assert!(pair.rust.song_replay_tween_messages(pair.scene).is_none());
    pair.rust
        .vm_mut()
        .execute(
            &compile_named(b"collectgarbage();saved.target:x(100)", b"=retired capture").unwrap(),
        )
        .unwrap();
    drop(pair);
    assert_eq!(captured[0].writes.len(), 2);
}

fn compare_immediate_updates(
    pair: &mut Pair,
) -> Vec<deadsync_song_lua::SongReplayImmediateUpdates> {
    let actual = pair.rust.drain_song_immediate_updates(pair.scene).unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_immediate_updates::eq1",
        &(actual),
    );
    assert!(
        pair.rust
            .drain_song_immediate_updates(pair.scene)
            .unwrap()
            .is_empty()
    );

    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_immediate_updates::eq2",
        &(pair.rust.song_replay_messages(pair.scene).unwrap()),
    );
    actual
}

#[test]
fn immediate_capture_preserves_base_alignment_size_visibility_and_explicit_writes() {
    use deadsync_song_lua::{
        SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update,
    };
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.listener=self end,
            ChangeMessageCommand=function(self,p)p.apply(saved.target);collectgarbage()end,
            Def.Quad{
                InitCommand=function(self)saved.target=self end,
                OnCommand=function(self)self:linear(1):x(1)end,
            },
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "immediate properties",
    ));
    let rust = pair.activate(&[&path]);
    assert!(pair.rust.drain_song_immediate_updates(pair.scene).is_none());
    begin_message_replay(&mut pair, &rust, &["target"]);
    for source in [
        "a:basezoom(2):basezoomx(3):basezoomy(4):basezoomz(5):basealpha(0.5):basealpha(0.5)",
        "a:baserotationx(10):baserotationy(20):baserotationz(30)",
        "a:halign(0.2):valign(0.8):align(0.3,0.7):horizalign('left'):vertalign('bottom')",
        "a:SetWidth(40):SetHeight(50):SetSize(60,70):shadowlength(4):shadowlengthx(5):shadowlengthy(6):shadowcolor(1,0.5,0.25,0.75)",
        "a:visible(false):visible(false):hibernate(0.25):hibernate(0)",
        "a:basealpha('invalid'):halign({}):SetWidth(false)",
    ] {
        pair.script(&format!(
            "MESSAGEMAN:Broadcast('Change',{{apply=function(a){source}end}})"
        ));
        let output = compare_immediate_updates(&mut pair);
        if source.contains("basealpha(0.5)") {
            assert_eq!(
                output[0]
                    .values
                    .iter()
                    .filter(|(target, _)| *target == Target::BaseAlpha)
                    .count(),
                1
            );
            let messages = pair.rust.song_replay_messages(pair.scene).unwrap();
            assert_eq!(
                messages[0]
                    .writes
                    .iter()
                    .filter(|w| w.target == Target::BaseAlpha && w.value == Update::F32(0.5))
                    .count(),
                2
            );
        }
    }
}

#[test]
fn immediate_effect_capture_preserves_native_preset_order_and_invalid_argument_rules() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            ChangeMessageCommand=function(self,p)p.apply(saved.target)end,
            Def.Quad{InitCommand=function(self)saved.target=self end,
                OnCommand=function(self)self:linear(1):x(2)end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "effect publications",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["target"]);
    for source in [
        "a:effecttiming(0.2,0.3,0.4,0.5,0.6):effectperiod(3)",
        "a:effectcolor1(1,0.2,0.3,0.4):effectcolor2(0.4,0.3,0.2,1):effectmagnitude(2,3,4)",
        "a:effectclock('beatnooffset'):effectclock('timer'):effectoffset(0.125):effect_hold_at_full(0.75)",
        "a:diffuseblink():diffuseshift():diffuseramp():glowblink():glowshift():glowramp()",
        "a:bob():bob():effectperiod(3):bob():bounce():wag():pulse()",
        "a:rainbow():rainbow():spin():vibrate():stopeffect()",
        "a:effecttiming(0,0,0,0):effecttiming(false):effectclock('invalid'):effectperiod(false):effectoffset({})",
        "a:effectmagnitude():effectcolor1():effectcolor2()",
    ] {
        pair.script(&format!(
            "MESSAGEMAN:Broadcast('Change',{{apply=function(a){source};collectgarbage()end}})"
        ));
        compare_immediate_updates(&mut pair);
    }
}

#[test]
fn queued_immediate_capture_preserves_dispatch_precision_final_values_and_cancellation() {
    use deadsync_song_lua::{
        SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update,
    };
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.parent=self end,
            OnCommand=function(self)self:sleep(0.125):queuecommand('Apply')end,
            ApplyCommand=function(self)
                saved.target:basealpha(0.25):basealpha(0.5):align(0.2,0.8)
                MESSAGEMAN:Broadcast('Tint')
                saved.target:stoptweening();collectgarbage()
            end,
            TintMessageCommand=function(self)saved.target:shadowcolor(1,0.5,0.25,1)end,
            Def.Quad{InitCommand=function(self)saved.target=self end,
                OnCommand=function(self)self:linear(1):x(2)end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "queued immediate publications",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["target"]);
    pair.script("saved.target:basealpha(1)");
    let time = 16777216.125;

    pair.rust.set_values(3., 4.).unwrap();

    pair.rust
        .advance_song_replay(pair.scene, time, 0.25)
        .unwrap();
    pair.script("saved.target:basealpha(0.75)");
    let frames = compare_immediate_updates(&mut pair);
    assert_eq!(frames.len(), 1);
    assert!(frames[0].tween_reset);
    let alpha = frames[0]
        .scheduled
        .iter()
        .find(|u| u.target == Target::BaseAlpha)
        .unwrap();
    assert_eq!(alpha.dispatch_seconds, Some(time));
    assert_eq!(alpha.initial_value, Some(Update::F32(1.)));
    assert_eq!(alpha.value, Update::F32(0.5));
    assert_eq!(
        frames[0]
            .final_values
            .iter()
            .find(|(t, _)| *t == Target::BaseAlpha)
            .unwrap()
            .1,
        Update::F32(0.75)
    );
    // Multiple dispatches without a drain retain publication order. Finish
    // promotes only the final destination for each property.
    pair.script("saved.target:queuecommand('Promote');saved.target.PromoteCommand=function(a)a:basealpha(0.2):basealpha(0.4)end");

    pair.rust
        .advance_song_replay(pair.scene, time + 1., 0.25)
        .unwrap();
    pair.script("saved.target:queuecommand('Promote')");

    pair.rust
        .advance_song_replay(pair.scene, time + 2., 0.25)
        .unwrap();
    pair.script("saved.target:finishtweening()");
    let frames = compare_immediate_updates(&mut pair);
    assert!(frames[0].tween_reset && frames[0].scheduled.is_empty());
    assert!(
        frames[0]
            .values
            .contains(&(Target::BaseAlpha, Update::F32(0.4)))
    );
}

#[test]
fn immediate_capture_keeps_partial_queue_errors_wake_writes_and_owned_output_after_teardown() {
    use deadsync_song_lua::{
        SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update,
    };
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.parent=self end,
            OnCommand=function(self)self:sleep(0.125):queuecommand('Fail')end,
            FailCommand=function(self)
                saved.target:basealpha(0.4):visible(false)
                error('partial publication')
            end,
            Def.Quad{InitCommand=function(self)saved.target=self end,
                OnCommand=function(self)self:hibernate(0.5):linear(1):x(1)end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "publication lifetime",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["target", "target"]);

    assert!(pair.rust.advance_song_replay(pair.scene, 1., 0.25).is_err());
    pair.script("saved.target:visible(true);collectgarbage()");
    let frames = compare_immediate_updates(&mut pair);
    assert_eq!(frames[0].overlay_index, 1);
    assert!(
        frames[0]
            .values
            .contains(&(Target::Visible, Update::Bool(true)))
    );
    assert_eq!(frames[0].scheduled.len(), 2);

    pair.rust.advance_song_replay(pair.scene, 2., 0.75).unwrap();
    let wake = compare_immediate_updates(&mut pair);
    assert!(
        wake[0]
            .values
            .contains(&(Target::Hibernating, Update::Bool(false)))
    );
    let mut foreign = SongRuntime::new(&pair.context).unwrap();
    assert!(foreign.drain_song_immediate_updates(pair.scene).is_none());
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    assert!(pair.rust.drain_song_immediate_updates(pair.scene).is_none());
    assert!(pair.rust.song_replay_messages(pair.scene).is_none());
    drop(pair);
    assert_eq!(frames[0].scheduled.len(), 2);
}

#[test]
fn immediate_capture_preserves_actor_order_after_message_pose_writes() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            ChangeMessageCommand=function(self,p)p.apply();collectgarbage()end,
            Def.Quad{InitCommand=function(self)saved.first=self end},
            Def.Quad{InitCommand=function(self)saved.second=self end},
            Def.Quad{InitCommand=function(self)saved.pose_only=self end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "publication order"));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["first", "second", "pose_only"]);
    for (source, indices) in [
        (
            "saved.pose_only:x(5);saved.first:x(1);saved.second:basealpha(0.5);saved.first:basealpha(0.25)",
            vec![0, 1],
        ),
        (
            "saved.second:y(2);saved.first:visible(false);saved.second:visible(true)",
            vec![1, 0],
        ),
        ("saved.pose_only:linear(1):z(3)", vec![]),
        (
            "saved.first:basealpha(1);saved.pose_only:basealpha(0.75)",
            vec![0, 2],
        ),
    ] {
        pair.script(&format!(
            "MESSAGEMAN:Broadcast('Change',{{apply=function(){source}end}})"
        ));
        let frames = compare_immediate_updates(&mut pair);
        assert_eq!(
            frames.iter().map(|f| f.overlay_index).collect::<Vec<_>>(),
            indices
        );
    }
}

#[test]
fn immediate_capture_publishes_sprite_state_culling_and_framebuffer_flags() {
    use deadsync_song_lua::{
        SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update,
    };
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            ChangeMessageCommand=function(self,p)p.apply();collectgarbage()end,
            Def.Sprite{InitCommand=function(self)saved.sprite=self end},
            Def.ActorFrameTexture{InitCommand=function(self)saved.aft=self end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "resource publications",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["sprite", "aft"]);
    for source in [
        "saved.sprite:linear(1):setstate(2):setstate(2):cullmode('front'):backfacecull(false)",
        "saved.aft:SetSize(8,8):EnablePreserveTexture(true):Create();saved.aft:EnablePreserveTexture(false)",
        "saved.sprite:setstate(-1):setstate(1.5):setstate({});assert(not pcall(function()saved.sprite:cullmode('invalid')end));assert(not pcall(function()saved.aft:EnablePreserveTexture(1)end))",
        "saved.aft:Create()",
    ] {
        pair.script(&format!(
            "MESSAGEMAN:Broadcast('Change',{{apply=function(){source}end}})"
        ));
        let frames = compare_immediate_updates(&mut pair);
        if source.contains("setstate(2)") {
            assert!(frames[0].scheduled.is_empty());
            assert!(
                frames[0]
                    .values
                    .contains(&(Target::SpriteStateIndex, Update::U32(2)))
            );
        }
    }
    // Without startup queue steps, queuecommand drains capture immediately.
    // Its publications retain the captured start, with no frame timestamp.
    pair.script("saved.applied=0;saved.sprite.ApplyCommand=function(a)saved.applied=saved.applied+1;a:setstate(3):backfacecull(true);saved.aft:EnablePreserveTexture(true)end;saved.sprite:queuecommand('Apply');assert(saved.applied==1)");

    pair.rust.advance_song_replay(pair.scene, 2., 2.).unwrap();
    let frames = compare_immediate_updates(&mut pair);
    assert!(
        frames
            .iter()
            .flat_map(|f| &f.scheduled)
            .all(|u| u.dispatch_seconds.is_none() && u.delay_seconds == 1.)
    );
    assert_eq!(frames.iter().map(|f| f.scheduled.len()).sum::<usize>(), 3);
    pair.script("assert(saved.applied==1)");
}

#[test]
fn captured_queue_dispatch_keeps_nested_starts_partial_errors_and_scope_restoration() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.parent=self end,
            OuterCommand=function(self)
                log('outer');self:basealpha(0.25):sleep(0.5):queuecommand('Inner')
                saved.other:basealpha(0.5);collectgarbage();log('outer end')
            end,
            InnerCommand=function(self)
                log('inner');saved.other:basealpha(0.75);error('after publication')
            end,
            Def.Quad{InitCommand=function(self)saved.other=self end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "captured queue scopes",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["parent", "other"]);
    pair.script("saved.parent:sleep(1);assert(not pcall(function()saved.parent:queuecommand('Outer')end));saved.other:visible(false);log('recovered')");
    assert_eq!(pair.compare(), "outer|outer end|inner|recovered");
    let frames = compare_immediate_updates(&mut pair);
    assert!(
        frames
            .iter()
            .flat_map(|f| &f.scheduled)
            .all(|u| u.dispatch_seconds.is_none())
    );
    assert!(
        frames
            .iter()
            .flat_map(|f| &f.scheduled)
            .any(|u| u.delay_seconds == 1.5)
    );
    pair.script("saved.parent:queuecommand('Missing');saved.other:basealpha(1);collectgarbage()");
    compare_immediate_updates(&mut pair);
}

#[test]
fn capture_queues_enter_frame_scheduling_in_actual_actor_order() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        local function fire(self)
            log(self:GetName());self:basealpha(0.5);collectgarbage()
        end
        return Def.ActorFrame{
            Def.Quad{Name='early',InitCommand=function(self)saved.early=self end,FireCommand=fire},
            Def.ActorFrame{Name='driver',InitCommand=function(self)saved.driver=self end,
                FireCommand=fire,
                OnCommand=function(self)self:SetUpdateFunction(function(self)
                    if not saved.armed or saved.started then return end
                    saved.started=true;log('queue begin')
                    saved.early:sleep(0.125):queuecommand('Fire')
                    saved.late:sleep(0.125):queuecommand('Fire')
                    self:sleep(0.125):queuecommand('Fire');log('queue end')
                end)end},
            Def.Quad{Name='late',InitCommand=function(self)saved.late=self end,FireCommand=fire},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "capture frame order",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["early", "driver", "late"]);
    let frames: Vec<_> = (0..7)
        .map(|i| (i as f64, if i == 0 { 0. } else { 0.125 }))
        .collect();

    pair.script("saved.armed=true;saved.early:linear(10):basealpha(0.8)");
    compare_immediate_updates(&mut pair);
    for i in 1..frames.len() {
        let seconds = i as f64 * 0.125;
        pair.rust.set_values(i as f32, seconds as f32).unwrap();

        pair.rust
            .advance_song_replay(pair.scene, seconds, 0.125)
            .unwrap();
        pair.compare();
        compare_immediate_updates(&mut pair);
    }
    assert_eq!(pair.compare(), "queue begin|queue end|late|early|driver");
}

fn advance_capture_frame(pair: &mut Pair, index: usize, seconds: f64, delta: f64) {
    pair.rust.set_values(index as f32, seconds as f32).unwrap();

    pair.rust
        .advance_song_replay(pair.scene, seconds, delta)
        .unwrap();
    pair.compare();
    compare_immediate_updates(pair);
}

#[test]
fn recurring_capture_keeps_exact_intervals_hibernation_rates_and_conditional_stop() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.loop=self end,
            LoopCommand=function(self)
                saved.n=(saved.n or 0)+1;log('loop',saved.n);self:basealpha(saved.n/10)
                if saved.n<5 then
                    for i=1,7 do self:sleep('0.1') end
                    self:queuecommand('Loop')
                end
                collectgarbage()
            end,
            OnCommand=function(self)self:SetUpdateFunction(function()log('parent update')end)end,
            Def.ActorFrame{OnCommand=function(self)self:SetUpdateFunction(function()log('child update')end)end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "recurring exact timer",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["loop"]);
    let deltas = [0., 0.0625, 0.0625, 0.35, 0., 0.000001, 0.7, 0.7];

    pair.script("events={};saved.loop:SetUpdateRate(2):hibernate(0.125):playcommand('Loop')");
    compare_immediate_updates(&mut pair);
    let mut seconds = 0.;
    for (i, &dt) in deltas.iter().enumerate() {
        seconds += dt;
        advance_capture_frame(&mut pair, i, seconds, dt);
        if i == 4 {
            pair.script("assert(saved.n==1)");
        }
    }
    pair.script("assert(saved.n==5)");
}

#[test]
fn recurring_capture_zero_intervals_and_per_update_cap_match_native() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            Def.Quad{InitCommand=function(self)saved.zero=self end,
                LoopCommand=function(self)
                    saved.z=(saved.z or 0)+1;log('zero',saved.z);self:basealpha(saved.z/10)
                    if saved.z<4 then self:queuecommand('Loop') end
                end},
            Def.Quad{InitCommand=function(self)saved.fast=self end,
                LoopCommand=function(self)
                    saved.f=(saved.f or 0)+1;log('fast',saved.f);self:basealpha(saved.f/100)
                    if saved.f<70 then self:sleep(0.000001):queuecommand('Loop') end
                end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "recurring update limits",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["zero", "fast"]);
    let deltas = [0., -0.125, 1., 0., 0.001];

    pair.script("saved.zero:playcommand('Loop');saved.fast:playcommand('Loop')");
    compare_immediate_updates(&mut pair);
    let mut seconds = 0.;
    for (i, &dt) in deltas.iter().enumerate() {
        seconds += dt;
        advance_capture_frame(&mut pair, i, seconds, dt);
        if i == 2 || i == 3 {
            pair.script("assert(saved.z==4 and saved.f==65)");
        }
    }
    pair.script("assert(saved.z==4 and saved.f==70)");
}

#[test]
fn recurring_capture_reports_once_preserves_partial_writes_and_restores_dispatch_after_errors() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.bad=self end,
            LoopCommand=function(self)
                saved.n=(saved.n or 0)+1;log('loop',saved.n);self:visible(saved.n%2==0)
                if saved.n<4 then self:sleep(0.125):queuecommand('Loop') end
                collectgarbage();if saved.n>1 then error('recurring partial write') end
            end,
            OnCommand=function(self)self:SetUpdateFunction(function()log('update continues')end)end,
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "recurring errors"));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["bad"]);
    let deltas = [0., 0.125, 0.125, 0.125, 0.125, 0.];

    pair.script("events={};saved.bad:playcommand('Loop')");
    compare_immediate_updates(&mut pair);
    let mut seconds = 0.;
    for (i, &dt) in deltas.iter().enumerate() {
        seconds += dt;
        advance_capture_frame(&mut pair, i, seconds, dt);
        pair.script("saved.bad:basealpha(0.9)");
        let output = compare_immediate_updates(&mut pair);
        assert!(output.iter().all(|f| f.scheduled.is_empty()));
    }
    pair.script("assert(saved.n==4)");
    let _ = saved(&mut pair, "bad");

    let diagnostics = pair.rust.take_actor_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].contains("recurring partial write"));
    assert!(pair.rust.take_actor_diagnostics().is_empty());
}

#[test]
fn recurring_capture_hurry_and_cancellation_preserve_the_stored_interval() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.Quad{
            InitCommand=function(self)saved.loop=self end,
            LoopCommand=function(self)
                saved.n=(saved.n or 0)+1;log('loop',saved.n);self:basealpha(saved.n/10)
                self:sleep(0.25):queuecommand('Loop')
            end,
            ZeroCommand=function(self)
                saved.z=(saved.z or 0)+1;log('zero',saved.z);self:basealpha(saved.z/10)
                self:queuecommand('Zero');collectgarbage()
            end,
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "recurring cancellation",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["loop"]);
    let deltas = [0., 0.125, 0.0625, 0., 0.0625, 0.25, 0.000001, 1.];

    // Before the first update there is no remaining timer to hurry. The stored
    // interval survives both hurry and cancellation, including the zero-sleep
    // command installed after StopTweening.
    pair.script("saved.loop:playcommand('Loop'):hurrytweening(0.5)");
    compare_immediate_updates(&mut pair);
    let mut seconds = 0.;
    for (i, &dt) in deltas.iter().enumerate() {
        seconds += dt;
        advance_capture_frame(&mut pair, i, seconds, dt);
        match i {
            1 => pair.script("assert(saved.n==1);saved.loop:hurrytweening(0.5)"),
            2 | 3 => pair.script("assert(saved.n==1)"),
            4 => pair.script(
                "assert(saved.n==2);saved.loop:stoptweening():playcommand('Zero');assert(saved.z==1)",
            ),
            5 => pair.script("assert(saved.z==1)"),
            6 => pair.script("assert(saved.z==2);saved.loop:finishtweening()"),
            _ => {}
        }
        pair.compare();
        compare_immediate_updates(&mut pair);
    }
    pair.script("assert(saved.n==2 and saved.z==2)");
}

fn compare_property_updates(pair: &mut Pair) -> Vec<deadsync_song_lua::SongReplayPropertyUpdates> {
    let actual = pair.rust.drain_song_property_updates(pair.scene).unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_property_updates::eq1",
        &(actual),
    );
    assert!(
        pair.rust
            .drain_song_property_updates(pair.scene)
            .unwrap()
            .is_empty()
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_property_updates::eq2",
        &(pair.rust.song_replay_messages(pair.scene).unwrap()),
    );
    actual
}

#[test]
fn render_property_capture_preserves_timing_replacement_flags_and_message_order() {
    use deadsync_song_lua::{
        SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update,
    };
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.parent=self end,
            ChangeMessageCommand=function(self,p)p.apply();collectgarbage()end,
            Def.Quad{InitCommand=function(self)saved.first=self end},
            Def.Quad{InitCommand=function(self)saved.second=self end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "render property output",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["parent", "first", "second"]);
    pair.script("MESSAGEMAN:Broadcast('Change',{apply=function()saved.parent:fov(60):vanishpoint(11,12):blend('BlendMode_Add'):zbias(2):draworder(3):SetDrawByZPosition(0):SetTextureFiltering(false):zbuffer(true):ztestmode('WriteOnFail')end})");
    let frames = compare_property_updates(&mut pair);
    assert_eq!(
        frames.iter().map(|f| f.overlay_index).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert!(
        frames[0]
            .values
            .contains(&(Target::DrawByZPosition, Update::Bool(true)))
    );
    assert!(frames[0].values.contains(&(Target::Fov, Update::F32(60.))));
    pair.script("MESSAGEMAN:Broadcast('Change',{apply=function()saved.parent:accelerate(0.5):draworder(4):draworder(5):fov(70):fov(80):sleep(0.25):zbias(3):SetTextureFiltering(true);saved.second:linear(0.5):ztest(false);saved.first:basealpha(0.4)end})");
    let frames = compare_property_updates(&mut pair);
    let draw: Vec<_> = frames[0]
        .scheduled
        .iter()
        .filter(|u| u.target == Target::DrawOrder)
        .collect();
    assert_eq!(draw.len(), 1);
    assert_eq!(draw[0].value, Update::I32(5));
    assert_eq!(draw[0].duration_seconds, 0.5);
    assert_eq!(draw[0].easing.as_deref(), Some("inQuad"));
    pair.script("MESSAGEMAN:Broadcast('Change',{apply=function()saved.parent:draworder('bad'):fov({}):zbias(false):vanishpoint(1):blend('bad');assert(not pcall(function()saved.parent:zbuffer({})end));assert(not pcall(function()saved.parent:ztestmode('bad')end));saved.first:visible(false)end})");
    compare_property_updates(&mut pair);
}

#[test]
fn render_property_capture_keeps_partial_errors_nested_messages_and_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.parent=self end,
            ChangeMessageCommand=function(self,p)p.apply()end,
            NestedMessageCommand=function(self)saved.other:zbias(5);collectgarbage()end,
            FailCommand=function(self)
                self:linear(0.5):fov(90):SetDrawByZPosition(true)
                MESSAGEMAN:Broadcast('Nested');self:blend('BlendMode_Add');error('render partial')
            end,
            Def.Quad{InitCommand=function(self)saved.other=self end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "render property cancellation",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["parent", "other"]);
    for source in [
        "assert(not pcall(function()saved.parent:sleep(0.25):playcommand('Fail')end));saved.other:basealpha(0.7)",
        "saved.parent:stoptweening();saved.other:zwrite(true)",
        "saved.parent:linear(1):fov(45):blend('BlendMode_Normal'):finishtweening();saved.parent:fov(-1)",
        "saved.parent:linear(1):fov(50):hurrytweening(0.5):fov(55)",
    ] {
        pair.script(&format!(
            "MESSAGEMAN:Broadcast('Change',{{apply=function(){source}end}})"
        ));
        compare_property_updates(&mut pair);
    }
}

#[test]
fn render_property_capture_reuses_pending_destinations_across_drains_and_validates_owners() {
    use deadsync_song_lua::{
        SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update,
    };
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            Name='pending',InitCommand=function(self)saved.parent=self end,
            FinishMessageCommand=function(self)self:finishtweening();collectgarbage()end,
        }
    "#,
    );
    let context = SongLuaCompileContext::new(dir.path(), "pending render destinations");
    let mut pair = Pair::new(context.clone());
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["parent"]);
    pair.script("saved.parent:linear(0.5):fov(80)");
    let first = compare_property_updates(&mut pair);
    let pending = vec![(0, Target::Fov, first[0].scheduled[0].value.clone())];

    assert!(
        pair.rust
            .set_song_pending_render_updates(pair.scene, &pending)
    );
    assert!(
        !pair
            .rust
            .set_song_pending_render_updates(pair.scene, &[(99, Target::Fov, Update::F32(1.))])
    );
    assert!(
        !pair
            .rust
            .set_song_pending_render_updates(pair.scene, &[(0, Target::X, Update::F32(1.))])
    );
    let mut foreign = SongRuntime::new(&context).unwrap();
    assert!(!foreign.set_song_pending_render_updates(pair.scene, &pending));
    pair.script("saved.parent:linear(0.25):fov(90)");
    let second = compare_property_updates(&mut pair);
    assert_eq!(second[0].scheduled[0].initial_value, Some(Update::F32(80.)));
    let pending = vec![(0, Target::Fov, second[0].scheduled[0].value.clone())];

    assert!(
        pair.rust
            .set_song_pending_render_updates(pair.scene, &pending)
    );
    pair.script("saved.parent:linear(0.25):fov(100):draworder(7);MESSAGEMAN:Broadcast('Finish')");
    let finish = compare_property_updates(&mut pair);
    assert!(finish[0].tween_reset);
    assert!(finish[0].values.contains(&(Target::Fov, Update::F32(100.))));
    let _ = saved(&mut pair, "parent");
    for source in [
        "collectgarbage()",
        "saved.parent:linear(0.25):fov(-1);MESSAGEMAN:Broadcast('Finish')",
    ] {
        pair.script(source);
        compare_property_updates(&mut pair);
        let snapshot = pair.rust.actor_scene_snapshot(pair.scene).unwrap();
        let actual = snapshot
            .iter()
            .find(|actor| actor.name == b"pending")
            .unwrap();

        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::render_property_capture_reuses_pending_destinations_across_drains_and_validates_owners::eq2",
            &actual.render.fov,
        );
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::render_property_capture_reuses_pending_destinations_across_drains_and_validates_owners::eq3",
            &(actual.draw_order),
        );
    }

    assert!(
        pair.rust
            .set_song_pending_render_updates(pair.scene, &pending)
    );
    pair.script("saved.parent:stoptweening():linear(0.25):fov(95)");
    let stop = compare_property_updates(&mut pair);
    assert!(stop[0].tween_reset);
    assert_eq!(stop[0].scheduled[0].initial_value, None);
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    assert!(
        !pair
            .rust
            .set_song_pending_render_updates(pair.scene, &pending)
    );
    assert!(pair.rust.drain_song_property_updates(pair.scene).is_none());
    drop(pair);
    assert_eq!(second[0].scheduled[0].value, Update::F32(90.));
}

fn install_predicted_frames(pair: &mut Pair, deltas: &[f64], actor_frames: Option<&[[f32; 2]]>) {
    let frames: Vec<_> = deltas
        .iter()
        .enumerate()
        .map(|(i, dt)| (i as f64, *dt))
        .collect();

    assert!(
        pair.rust
            .set_song_replay_frames(pair.scene, &frames, actor_frames)
    );
}
fn advance_predicted_frame(
    pair: &mut Pair,
    index: usize,
    seconds: f64,
    delta: f64,
) -> Vec<deadsync_song_lua::SongReplayPropertyUpdates> {
    pair.rust.set_values(index as f32, seconds as f32).unwrap();

    assert!(
        pair.rust
            .advance_song_replay_at_frame(pair.scene, index, delta)
            .unwrap()
    );
    pair.compare();
    compare_property_updates(pair)
}

#[test]
fn queue_clock_predicts_render_samples_and_cross_actor_frame_advancement() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            Def.Quad{InitCommand=function(self)saved.early=self end},
            Def.ActorFrame{InitCommand=function(self)saved.driver=self end,
                OnCommand=function(self)self:SetUpdateFunction(function(self)
                    if not saved.armed or saved.started then return end
                    saved.started=true;saved.early:linear(0.125):zbias(1):zbias(2)
                    saved.late:linear(0.125):draworder(4)
                    self:linear(0.125):fov(70);collectgarbage()
                end)end},
            Def.Quad{InitCommand=function(self)saved.late=self end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "queue clock render samples",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["early", "driver", "late"]);
    let deltas = [0., 0.125, 0.125, 0.125, 0.125];
    install_predicted_frames(&mut pair, &deltas, None);
    let mut seconds = 0.;
    for (i, &dt) in deltas.iter().enumerate() {
        if i == 1 {
            pair.script("saved.armed=true");
        }
        seconds += dt;
        let frames = advance_predicted_frame(&mut pair, i, seconds, dt);
        if i == 1 {
            assert_eq!(
                frames.iter().map(|f| f.overlay_index).collect::<Vec<_>>(),
                vec![0, 2, 1]
            );
            assert!(frames.iter().all(|f| f.scheduled[0].progress.is_some()));
        }
    }
}

#[test]
fn queue_clock_retains_dispatch_remainders_and_sample_arrays_after_queue_callbacks() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        local function fire(self)
            log(self:GetName());self:linear(0.25):zbias(3):basealpha(0.5);collectgarbage()
        end
        return Def.ActorFrame{
            Def.Quad{Name='early',InitCommand=function(self)saved.early=self end,FireCommand=fire},
            Def.ActorFrame{Name='driver',InitCommand=function(self)saved.driver=self end,FireCommand=fire,
                OnCommand=function(self)self:SetUpdateFunction(function(self)
                    if not saved.armed or saved.started then return end
                    saved.started=true;log('queue begin')
                    saved.early:sleep(0.125):queuecommand('Fire')
                    saved.late:sleep(0.125):queuecommand('Fire')
                    self:sleep(0.125):queuecommand('Fire');log('queue end')
                end)end},
            Def.Quad{Name='late',InitCommand=function(self)saved.late=self end,FireCommand=fire},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "queue clock dispatch samples",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["early", "driver", "late"]);
    let deltas = [0., 0.125, 0.125, 0.125, 0.125, 0.125];
    install_predicted_frames(&mut pair, &deltas, None);
    pair.script("events={}");
    let mut seconds = 0.;
    let mut owned = Vec::new();
    for (i, &dt) in deltas.iter().enumerate() {
        if i == 1 {
            pair.script("saved.armed=true");
        }
        seconds += dt;
        owned.extend(advance_predicted_frame(&mut pair, i, seconds, dt));
    }
    assert_eq!(pair.compare(), "queue begin|queue end|late|early|driver");
    assert!(
        owned
            .iter()
            .flat_map(|f| &f.scheduled)
            .any(|u| u.frame_advance > 0. && u.progress.is_some())
    );
    drop(pair);
    assert!(
        owned
            .iter()
            .flat_map(|f| &f.scheduled)
            .any(|u| u.progress.is_some())
    );
}

#[test]
fn queue_clock_preserves_actor_frame_spans_zero_tails_hurry_and_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{InitCommand=function(self)saved.actor=self end,
            OnCommand=function(self)self:SetUpdateFunction(function(self)
                if not saved.armed or saved.started then return end
                saved.started=true;self:linear(0.1):fov(75):hurrytweening(0.5):fov(80)
                self:sleep(0):draworder(7);collectgarbage();error('clock partial write')
            end)end}
    "#,
    );
    let context = SongLuaCompileContext::new(dir.path(), "queue clock spans and zero tails");
    let mut pair = Pair::new(context.clone());
    let rust = pair.activate(&[&path]);
    assert!(!pair.rust.set_song_replay_frames(pair.scene, &[], None));
    begin_message_replay(&mut pair, &rust, &["actor"]);
    let mut foreign = SongRuntime::new(&context).unwrap();
    assert!(!foreign.set_song_replay_frames(pair.scene, &[], None));
    let deltas = [0., 0.125, 0.125, 0.125];
    let actor_frames = [[0., 0.], [0.0625, 0.125], [0.0625, 0.125], [0.0625, 0.125]];
    install_predicted_frames(&mut pair, &deltas, Some(&actor_frames));
    let mut seconds = 0.;
    for (i, &dt) in deltas.iter().enumerate() {
        if i == 1 {
            pair.script("saved.armed=true");
        }
        seconds += dt;
        advance_predicted_frame(&mut pair, i, seconds, f64::from(actor_frames[i][0]));
    }
    pair.script("saved.actor:stoptweening():linear(0.2):fov(-1)");
    let stop = compare_property_updates(&mut pair);
    assert!(stop[0].tween_reset);
    pair.rust.set_live(true);
    assert!(!pair.rust.set_song_replay_frames(pair.scene, &[], None));
    assert!(
        !pair
            .rust
            .advance_song_replay_at_frame(pair.scene, 10, 1.)
            .unwrap()
    );
    pair.rust.set_live(false);
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    assert!(!pair.rust.set_song_replay_frames(pair.scene, &[], None));
    assert!(
        !pair
            .rust
            .advance_song_replay_at_frame(pair.scene, 10, 1.)
            .unwrap()
    );
}

#[test]
fn queue_clock_keeps_final_frame_zero_states_deferred_until_their_actor_update() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            Def.ActorFrame{InitCommand=function(self)saved.early=self end},
            Def.ActorFrame{OnCommand=function(self)self:SetUpdateFunction(function()
                if not saved.armed or saved.started then return end
                saved.started=true;saved.early:sleep(0):fov(80);saved.late:sleep(0):fov(90)
                collectgarbage()
            end)end},
            Def.ActorFrame{InitCommand=function(self)saved.late=self end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "final queue clock zero states",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["early", "late"]);
    install_predicted_frames(&mut pair, &[0., 0.125], None);
    advance_predicted_frame(&mut pair, 0, 0., 0.);
    pair.script("saved.armed=true");
    let frames = advance_predicted_frame(&mut pair, 1, 0.125, 0.125);
    assert_eq!(frames.len(), 2);
    let early = frames[0].scheduled[0].progress.as_ref().unwrap();
    let late = frames[1].scheduled[0].progress.as_ref().unwrap();
    assert!(early[0][0] > 0.125);
    assert_eq!(late[0][0], 0.125);
    assert_eq!(frames[0].scheduled[0].frame_advance, 0.);
    assert_eq!(frames[1].scheduled[0].frame_advance, 0.125);
}

#[test]
fn fallback_clock_dispatch_preserves_rates_hibernation_and_backdated_publications() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        local function fire(self)
            log(self:GetName());self:basealpha(0.5):linear(0.25):fov(70);collectgarbage()
        end
        return Def.ActorFrame{
            Def.ActorFrame{Name='early',InitCommand=function(self)saved.early=self end,FireCommand=fire},
            Def.ActorFrame{Name='driver',InitCommand=function(self)saved.driver=self end,FireCommand=fire,
                OnCommand=function(self)self:SetUpdateFunction(function(self)
                    if not saved.armed or saved.started then return end
                    saved.started=true
                    saved.early:SetUpdateRate(0.5):sleep(0.125):queuecommand('Fire')
                    saved.late:SetUpdateRate(2):sleep(0.125):queuecommand('Fire')
                    saved.asleep:hibernate(0.375):sleep(0.125):queuecommand('Fire')
                    self:SetUpdateRate(2):sleep(0.125):queuecommand('Fire')
                end)end},
            Def.ActorFrame{Name='late',InitCommand=function(self)saved.late=self end,FireCommand=fire},
            Def.ActorFrame{Name='asleep',InitCommand=function(self)saved.asleep=self end,FireCommand=fire},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "fallback dispatch clock",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["early", "driver", "late", "asleep"]);
    let deltas = [0., 0.125, 0.125, 0.125, 0.125, 0.125, 0.125];
    install_predicted_frames(&mut pair, &deltas, None);
    pair.script("events={}");
    let mut seconds = 0.;
    let mut owned = Vec::new();
    for (i, &dt) in deltas.iter().enumerate() {
        if i == 1 {
            pair.script("saved.armed=true");
        }
        seconds += dt;
        owned.extend(advance_predicted_frame(&mut pair, i, seconds, dt));
    }
    assert!(
        owned
            .iter()
            .flat_map(|f| &f.scheduled)
            .any(|u| u.frame_advance > 0. && u.dispatch_seconds.is_some())
    );
    assert_eq!(pair.compare(), "late|early|driver|asleep");
}

#[test]
fn fallback_clock_hands_only_dispatch_remainder_to_new_recurring_commands() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            Def.ActorFrame{OnCommand=function(self)self:SetUpdateFunction(function()
                if not saved.armed or saved.started then return end
                saved.started=true;saved.loop:sleep(0.125):queuecommand('Loop')
            end)end},
            Def.ActorFrame{InitCommand=function(self)saved.loop=self end,
                LoopCommand=function(self)
                    saved.n=(saved.n or 0)+1;log('loop',saved.n);self:basealpha(saved.n/10)
                    if saved.n<4 then self:sleep(0.125):queuecommand('Loop') end
                    collectgarbage();if saved.n==2 then error('recurring after dispatch') end
                end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "fallback recurring handoff",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["loop"]);
    let deltas = [0., 0.125, 0.125, 0.125, 0.125, 0.125, 0.125];
    install_predicted_frames(&mut pair, &deltas, None);
    let mut seconds = 0.;
    for (i, &dt) in deltas.iter().enumerate() {
        if i == 1 {
            pair.script("saved.armed=true");
        }
        seconds += dt;
        advance_predicted_frame(&mut pair, i, seconds, dt);
        if i == 2 {
            pair.script("assert(saved.n==1)");
        }
    }
    pair.script("assert(saved.n==4)");
    let errors = pair.rust.take_actor_diagnostics();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("recurring after dispatch"));
}

#[test]
fn fallback_clock_queue_errors_report_once_and_restore_dispatch_for_later_frames() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            Def.ActorFrame{OnCommand=function(self)self:SetUpdateFunction(function()
                if not saved.armed or saved.started then return end
                saved.started=true;saved.bad:sleep(0.125):queuecommand('Fail')
            end)end},
            Def.ActorFrame{InitCommand=function(self)saved.bad=self end,
                FailCommand=function(self)
                    saved.n=(saved.n or 0)+1;log('fail',saved.n)
                    self:basealpha(0.4):linear(0.25):fov(80);collectgarbage();error('fallback partial write')
                end},
            Def.Quad{InitCommand=function(self)saved.other=self end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "fallback queue errors",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["bad", "other"]);
    let deltas = [0., 0.125, 0.125, 0.125, 0.125, 0.125, 0.125];
    install_predicted_frames(&mut pair, &deltas, None);
    let mut seconds = 0.;
    for (i, &dt) in deltas.iter().enumerate() {
        if i == 1 {
            pair.script("saved.armed=true");
        }
        seconds += dt;
        advance_predicted_frame(&mut pair, i, seconds, dt);
        pair.script("saved.other:basealpha(0.8)");
        let output = compare_property_updates(&mut pair);
        assert!(output.iter().all(|f| f.scheduled.is_empty()));
        if i == 3 {
            pair.script("saved.bad:stoptweening();saved.started=false");
            compare_property_updates(&mut pair);
        }
    }
    pair.script("assert(saved.n==2)");
    let _ = saved(&mut pair, "bad");

    let errors = pair.rust.take_actor_diagnostics();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("fallback partial write"));
}

#[test]
fn fallback_clock_nested_inline_commands_restore_absent_timestamps() {
    use deadsync_song_lua::SongLuaOverlayUpdateTarget as Target;
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            Def.ActorFrame{OnCommand=function(self)self:SetUpdateFunction(function()
                if not saved.armed or saved.started then return end
                saved.started=true;saved.parent:sleep(0.125):queuecommand('Fail'):queuecommand('Outer')
            end)end},
            Def.ActorFrame{InitCommand=function(self)saved.parent=self end,
                FailCommand=function()log('fail');error('before nested dispatch')end,
                OuterCommand=function(self)
                    log('outer');saved.child:queuecommand('Child');self:basealpha(0.9);collectgarbage()
                end},
            Def.ActorFrame{InitCommand=function(self)saved.child=self end,
                ChildCommand=function(self)log('child');self:basealpha(0.6);collectgarbage()end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "fallback inline scope",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["parent", "child"]);
    let deltas = [0., 0.125, 0.125, 0.125];
    install_predicted_frames(&mut pair, &deltas, None);
    let mut seconds = 0.;
    for (i, &dt) in deltas.iter().enumerate() {
        if i == 1 {
            pair.script("saved.armed=true");
        }
        seconds += dt;
        advance_predicted_frame(&mut pair, i, seconds, dt);
    }
    assert_eq!(pair.compare(), "fail");
    pair.script("saved.parent:queuecommand('Missing')");
    let frames = compare_property_updates(&mut pair);
    assert_eq!(pair.compare(), "fail|outer|child");
    let child = frames
        .iter()
        .find(|f| f.overlay_index == 1)
        .unwrap()
        .scheduled
        .iter()
        .find(|u| u.target == Target::BaseAlpha)
        .unwrap();
    assert_eq!(child.dispatch_seconds, None);
    assert_eq!(child.frame_advance, 0.);
    let parent = frames
        .iter()
        .find(|f| f.overlay_index == 0)
        .unwrap()
        .scheduled
        .iter()
        .find(|u| u.target == Target::BaseAlpha)
        .unwrap();
    assert_eq!(parent.dispatch_seconds, Some(0.25));
    assert_eq!(parent.frame_advance, 0.125);
}

fn compare_pose_output(pair: &mut Pair) -> Vec<deadsync_song_lua::SongReplayPropertyUpdates> {
    let actual = pair.rust.drain_song_overlay_updates(pair.scene).unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_pose_output::eq1",
        &(actual),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_pose_output::eq2",
        &(pair.rust.song_replay_messages(pair.scene).unwrap()),
    );
    assert!(
        pair.rust
            .drain_song_overlay_updates(pair.scene)
            .unwrap()
            .is_empty()
    );
    actual
}

#[test]
fn pose_output_captures_all_fallback_targets_timing_and_ordered_messages() {
    use deadsync_song_lua::SongLuaOverlayUpdateTarget as Target;
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{InitCommand=function(self)saved.actor=self end,
            ChangeMessageCommand=function(self,p)p.apply();collectgarbage()end}
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "fallback pose output",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["actor"]);
    install_predicted_frames(&mut pair, &[0., 0.125, 0.125, 0.125], None);
    let setters = "x(1):y(2):z(3):aux(4):zoom(2):rotationx(5):rotationy(6):rotationz(7):skewx(0.1):skewy(0.2):cropleft(0.1):cropright(0.2):croptop(0.3):cropbottom(0.4):fadeleft(0.1):faderight(0.2):fadetop(0.3):fadebottom(0.4):diffuse(0.2,0.3,0.4,0.5):diffuseupperleft(1,0,0,1):glow(0.5,0.4,0.3,0.2):heading(15)";
    pair.script(&format!(
        "MESSAGEMAN:Broadcast('Change',{{apply=function()saved.actor:{setters}end}})"
    ));
    let direct = compare_pose_output(&mut pair);
    assert_eq!(direct[0].values.len(), 25);
    assert!(direct[0].scheduled.is_empty());
    pair.script(&format!("MESSAGEMAN:Broadcast('Change',{{apply=function()saved.actor:sleep(0.125):accelerate(0.25):{setters}:x(8):x(9)end}})"));
    let timed = compare_pose_output(&mut pair);
    assert_eq!(timed[0].scheduled.len(), 25);
    assert!(
        timed[0]
            .scheduled
            .iter()
            .all(|u| u.delay_seconds == 0.125 && u.duration_seconds == 0.25)
    );
    assert_eq!(
        timed[0]
            .scheduled
            .iter()
            .filter(|u| u.target == Target::X)
            .count(),
        1
    );
}

#[test]
fn pose_output_reconciles_current_destinations_prior_reads_and_sampled_getters() {
    use deadsync_song_lua::{
        SongLuaOverlayState as State, SongLuaOverlayUpdateTarget as Target,
        SongLuaOverlayUpdateValue as Update,
    };
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            Def.ActorFrame{InitCommand=function(self)saved.early=self end},
            Def.ActorFrame{InitCommand=function(self)saved.driver=self end,
                OnCommand=function(self)self:SetUpdateFunction(function(self)
                    if not saved.armed or saved.started then return end
                    saved.started=true;saved.early:linear(0.25):x(8);saved.late:x(9);log('direct late',saved.late:GetX());saved.late:linear(0.25):x(16);self:linear(0.25):x(32)
                    log('driver',self:GetX(),self:GetDestX(),saved.late:GetX(),saved.late:GetDestX());collectgarbage()
                end)end},
            Def.ActorFrame{InitCommand=function(self)saved.late=self end,
                OnCommand=function(self)self:SetUpdateFunction(function(self)
                    if saved.armed then log('late',self:GetX(),self:GetDestX())end
                end)end},
        }
    "#,
    );
    let context = SongLuaCompileContext::new(dir.path(), "pose getter handoff");
    let mut pair = Pair::new(context.clone());
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["early", "driver", "late"]);
    install_predicted_frames(&mut pair, &[0., 0.125, 0.125, 0.125], None);

    let sampled = vec![
        (0, Target::X, Update::F32(1.)),
        (1, Target::X, Update::F32(2.)),
        (2, Target::X, Update::F32(3.)),
    ];

    assert!(
        pair.rust
            .set_song_replay_pose_getters(pair.scene, &sampled)
            .unwrap()
    );
    pair.script("assert(saved.early:GetX()==1 and saved.early:GetDestX()==0);saved.late:x(7)");
    let prior: Vec<_> = [1., 2., 3.]
        .into_iter()
        .map(|x| State {
            x,
            ..State::default()
        })
        .collect();

    assert!(
        pair.rust
            .set_song_replay_prior_positions(pair.scene, &prior)
    );
    pair.script("saved.armed=true");
    pair.rust.set_values(1., 0.125).unwrap();

    pair.rust
        .advance_song_replay_at_frame(pair.scene, 1, 0.125)
        .unwrap();
    pair.compare();
    let frames = compare_pose_output(&mut pair);
    assert!(
        frames
            .iter()
            .flat_map(|f| &f.scheduled)
            .all(|u| u.progress.is_some())
    );
    // A compiler sample replaces raw destination fields outside XYZ/aux.
    // Relative setters must use those fields even with a pending fallback queue.
    let destinations = [
        (1, Target::RotationX, Update::F32(15.)),
        (1, Target::Diffuse, Update::Vec4([0.9, 0.8, 0.7, 0.6])),
        (
            1,
            Target::Quaternion,
            Update::Quaternion([0., 0.5, 0., 0.8660254]),
        ),
    ];

    assert!(
        pair.rust
            .set_song_replay_pose_getters(pair.scene, &destinations)
            .unwrap()
    );
    pair.script("saved.driver:addrotationx(5):diffusealpha(0.25):heading(15);assert(saved.driver:GetRotationX()==20)");
    compare_pose_output(&mut pair);
    assert!(
        !pair
            .rust
            .set_song_replay_pose_getters(pair.scene, &[(99, Target::X, Update::F32(1.))])
            .unwrap()
    );
    let mut foreign = SongRuntime::new(&context).unwrap();
    assert!(
        !foreign
            .set_song_replay_pose_getters(pair.scene, &sampled)
            .unwrap()
    );
}

#[test]
fn pose_output_preserves_pending_finish_stop_partial_errors_and_owned_values() {
    use deadsync_song_lua::{
        SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update,
    };
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{InitCommand=function(self)saved.actor=self end,
            ChangeMessageCommand=function(self,p)p.apply()end}
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "pose cancellation"));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["actor"]);
    install_predicted_frames(&mut pair, &[0., 0.125, 0.125], None);
    pair.script("MESSAGEMAN:Broadcast('Change',{apply=function()saved.actor:linear(0.5):x(8):y(4):rotationz(90):diffuse(0.2,0.3,0.4,0.5):glow(1,0,0,1)end})");
    let first = compare_pose_output(&mut pair);
    let pending: Vec<_> = first
        .iter()
        .flat_map(|f| {
            f.scheduled
                .iter()
                .map(|u| (f.overlay_index, u.target, u.value.clone()))
        })
        .collect();

    assert!(
        pair.rust
            .set_song_pending_overlay_updates(pair.scene, &pending)
    );
    pair.script("MESSAGEMAN:Broadcast('Change',{apply=function()saved.actor:sleep(0.125):linear(0.25):x(16):finishtweening();assert(saved.actor:GetX()==16)end})");
    let finish = compare_pose_output(&mut pair);
    assert!(finish[0].tween_reset);
    assert!(finish[0].values.contains(&(Target::X, Update::F32(16.))));
    pair.script("MESSAGEMAN:Broadcast('Change',{apply=function()assert(not pcall(function()saved.actor:linear(0.25):x(32);collectgarbage();error('partial pose')end));saved.actor:stoptweening()end})");
    let stop = compare_pose_output(&mut pair);
    assert!(stop[0].tween_reset);
    assert!(stop[0].scheduled.iter().all(|u| u.target != Target::X));
    // Stop discards the position destination; a relative write starts at the
    // frozen current position. Keep raw state and destination getters in sync.
    pair.script("assert(saved.actor:GetX()==16 and saved.actor:GetDestX()==16);saved.actor:addx(1);assert(saved.actor:GetDestX()==17)");
    let relative = compare_pose_output(&mut pair);
    assert!(relative[0].values.contains(&(Target::X, Update::F32(17.))));
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    assert!(pair.rust.drain_song_overlay_updates(pair.scene).is_none());
    drop(pair);
    assert!(
        first[0]
            .scheduled
            .iter()
            .any(|u| u.target == Target::Diffuse)
    );
}

#[test]
fn media_output_captures_text_masks_and_sprite_setters_in_native_order() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            ChangeMessageCommand=function(self,p)p.apply();collectgarbage()end,
            Def.BitmapText{Text="x",InitCommand=function(self)saved.text=self end},
            Def.Sprite{InitCommand=function(self)saved.sprite=self end},
            Def.Quad{InitCommand=function(self)saved.mask=self end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "media publications"));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["text", "sprite", "mask"]);
    install_predicted_frames(&mut pair, &[0., 0.125, 0.125, 0.125], None);
    let setters = r#"
        saved.text:uppercase(true):rainbowscroll(true):jitter(true):distort(0.2)
            :textglowmode('TextGlowMode_Inner'):set_mult_attrs_with_diffuse(true)
            :vertspacing(3):wrapwidthpixels(120):maxwidth(80):maxheight(40):max_dimension_use_zoom(true)
        saved.sprite:animate(false):loop(false):rate(2):SetAllStateDelays(0.125)
            :texturewrapping(true):texturetranslate(0.25,0.5):customtexturerect(0,0.1,0.8,0.9):texcoordvelocity(1,2)
        saved.mask:MaskSource():MaskDest()
    "#;
    pair.script(&format!(
        "MESSAGEMAN:Broadcast('Change',{{apply=function(){setters}end}})"
    ));
    let direct = compare_pose_output(&mut pair);
    assert_eq!(direct.iter().map(|f| f.values.len()).sum::<usize>(), 24);
    assert!(direct.iter().all(|f| f.scheduled.is_empty()));
    pair.script(&format!("MESSAGEMAN:Broadcast('Change',{{apply=function()saved.text:sleep(0.125):accelerate(0.25);saved.sprite:sleep(0.125):accelerate(0.25);saved.mask:sleep(0.125):accelerate(0.25);{setters};saved.text:uppercase(true)end}})"));
    let timed = compare_pose_output(&mut pair);
    assert_eq!(timed.iter().map(|f| f.scheduled.len()).sum::<usize>(), 24);
    assert!(
        timed
            .iter()
            .flat_map(|f| &f.scheduled)
            .all(|u| u.delay_seconds == 0.125 && u.duration_seconds == 0.25)
    );
    // The older 41-target view consumes media writes without exposing them.
    pair.script("saved.text:uppercase(false)");
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::media_output_captures_text_masks_and_sprite_setters_in_native_order::eq3",
        &(pair.rust.drain_song_property_updates(pair.scene).unwrap()),
    );
    assert!(
        pair.rust
            .drain_song_overlay_updates(pair.scene)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn media_output_preserves_retained_texture_state_and_geometry_write_order() {
    use deadsync_song_lua::{
        SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update,
    };
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    image::RgbaImage::from_pixel(16, 8, image::Rgba([255, 255, 255, 255]))
        .save(dir.path().join("sheet 2x1.png"))
        .unwrap();
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            ChangeMessageCommand=function(self,p)p.apply()end,
            Def.Sprite{Texture='sheet 2x1.png',InitCommand=function(self)saved.sprite=self end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "media texture capture",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["sprite"]);
    install_predicted_frames(&mut pair, &[0., 0.125, 0.125, 0.125], None);
    pair.script(
        r#"MESSAGEMAN:Broadcast('Change',{apply=function()
        local a=saved.sprite;local t=a:GetTexture();collectgarbage();t:loop(false);t:rate(0.5)
        a:SetStateProperties{{Frame=1,Delay=0.2},{Frame=0,Delay=0.3}}
        a:SetSecondsIntoAnimation(0.21):pause():play():linear(0.4)
            :customtexturerect(0,0,0.5,1):stretchtexcoords(0.1,0.2):addimagecoords(1,2)
            :CropTo(4,4):scaletoclipped(5,6)
        assert(a:GetWidth()==5 and a:GetHeight()==6)
    end})"#,
    );
    let output = compare_pose_output(&mut pair);
    assert!(
        output[0]
            .values
            .contains(&(Target::Size, Update::Vec2([5., 6.])))
    );
    assert!(
        output[0]
            .scheduled
            .iter()
            .any(|u| u.target == Target::CustomTextureRect)
    );
    assert!(
        output[0]
            .scheduled
            .iter()
            .any(|u| u.target == Target::ZoomX)
    );
}

#[test]
fn media_output_retains_partial_writes_and_pending_stop_finish_values() {
    use deadsync_song_lua::{
        SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update,
    };
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            ChangeMessageCommand=function(self,p)p.apply()end,
            Def.BitmapText{InitCommand=function(self)saved.text=self end},
            Def.Sprite{InitCommand=function(self)saved.sprite=self end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "media pending cancellation",
    ));
    let rust = pair.activate(&[&path]);
    begin_message_replay(&mut pair, &rust, &["text", "sprite"]);
    install_predicted_frames(&mut pair, &[0., 0.125, 0.125, 0.125], None);
    pair.script(
        r#"MESSAGEMAN:Broadcast('Change',{apply=function()
        assert(not pcall(function()
            saved.text:linear(0.5):maxwidth(12):jitter(true)
            saved.sprite:linear(0.5):texturetranslate(0.1,0.2):animate(false)
            collectgarbage();error('partial media')
        end))
    end})"#,
    );
    let first = compare_pose_output(&mut pair);
    let pending: Vec<_> = first
        .iter()
        .flat_map(|f| {
            f.scheduled
                .iter()
                .map(|u| (f.overlay_index, u.target, u.value.clone()))
        })
        .collect();

    assert!(
        pair.rust
            .set_song_pending_overlay_updates(pair.scene, &pending)
    );
    pair.script(
        r#"MESSAGEMAN:Broadcast('Change',{apply=function()
        saved.text:linear(0.25):maxwidth(24):jitter(false):stoptweening()
        saved.sprite:linear(0.25):texturetranslate(0.3,0.4):animate(true):stoptweening()
    end})"#,
    );
    let stop = compare_pose_output(&mut pair);
    assert!(stop.iter().all(|f| f.tween_reset));
    assert_eq!(stop.iter().map(|f| f.scheduled.len()).sum::<usize>(), 5);
    let pending: Vec<_> = stop
        .iter()
        .flat_map(|f| {
            f.scheduled
                .iter()
                .map(|u| (f.overlay_index, u.target, u.value.clone()))
        })
        .collect();

    assert!(
        pair.rust
            .set_song_pending_overlay_updates(pair.scene, &pending)
    );
    assert!(!pair.rust.set_song_pending_overlay_updates(
        pair.scene,
        &[(99, Target::Uppercase, Update::Bool(true))]
    ));
    pair.script("MESSAGEMAN:Broadcast('Change',{apply=function()saved.text:finishtweening();saved.sprite:finishtweening()end})");
    let finish = compare_pose_output(&mut pair);
    assert!(
        finish
            .iter()
            .all(|f| f.tween_reset && f.scheduled.is_empty())
    );
    assert!(
        finish
            .iter()
            .flat_map(|f| &f.values)
            .any(|(t, v)| *t == Target::MaxWidth && *v == Update::F32(24.))
    );
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    assert!(pair.rust.drain_song_overlay_updates(pair.scene).is_none());
    drop(pair);
    assert_eq!(first.iter().map(|f| f.scheduled.len()).sum::<usize>(), 5);
}

#[test]
fn media_output_startup_blocks_and_queued_discovery_match_native() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            Def.BitmapText{InitCommand=function(self)saved.text=self;self:uppercase(true):maxwidth(80)end,
                OnCommand=function(self)self:sleep(0.25):maxheight(40):queuecommand('Later')end,
                LaterCommand=function(self)self:jitter(true):distort(0.25);collectgarbage()end},
            Def.Sprite{InitCommand=function(self)saved.sprite=self;self:animate(false):rate(0.5)end,
                OnCommand=function(self)self:sleep(0.25):texturetranslate(0.1,0.2):queuecommand('Later')end,
                LaterCommand=function(self)self:loop(false):texcoordvelocity(1,2)end},
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "media startup capture",
    ));
    let rust = pair.activate(&[&path]);

    assert!(
        pair.rust
            .discover_song_startup_commands(pair.scene, &rust)
            .unwrap()
    );
    let actual = pair.rust.song_startup_queued_states(pair.scene).unwrap();
    for key in ["text", "sprite"] {
        let b = saved(&mut pair, key);

        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::media_output_startup_blocks_and_queued_discovery_match_native::eq1",
            &(rust.updates.get(&b)),
        );
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::media_output_startup_blocks_and_queued_discovery_match_native::eq2",
            &(rust.tweens.get(&b)),
        );
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::media_output_startup_blocks_and_queued_discovery_match_native::eq3",
            &(actual.get(&b)),
        );
        assert!(actual.contains_key(&b), "queued capture missing {key}");
    }
    pair.compare();
}

fn tracked_target_key(target: deadsync_song_lua::SongLuaTrackedActorTarget) -> (u8, usize) {
    use deadsync_song_lua::SongLuaTrackedActorTarget as T;
    match target {
        T::Player(i) => (0, i),
        T::PlayerJudgment(i) => (1, i),
        T::PlayerCombo(i) => (2, i),
        T::SongForeground => (3, 0),
        T::ScreenLayer(i) => (4, i),
    }
}
fn compare_tracked_output(pair: &mut Pair) -> deadsync_song_lua::SongTrackedActors {
    let actual = pair
        .rust
        .read_song_tracked_actors(pair.scene)
        .unwrap()
        .unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_tracked_output::eq1",
        &(actual.actors.len()),
    );
    for a in &actual.actors {
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_tracked_output::eq2",
            &(tracked_target_key(a.target)),
        );
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_tracked_output::eq3",
            &(a.actor),
        );
        assert!(matches!(
            pair.rust.song_tracked_actor_value(a).unwrap(),
            Value::Table(_)
        ));
    }
    pair.compare();
    actual
}

#[test]
fn tracked_output_preserves_order_state_aliases_and_optional_layers() {
    for live in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        activation_snapshot::register_path(dir.path());
        let path = write(dir.path(), "default.lua", "return Def.ActorFrame{}");
        let mut context = SongLuaCompileContext::new(dir.path(), "tracked output");
        context.live = live;
        let mut pair = Pair::new(context);
        pair.activate(&[&path]);
        pair.script(
            r#"
            local p=__songlua_top_screen_player_1
            p:x(11):rotationz(3);p.__songlua_children.Judgment:y(22)
            p.__songlua_children.Combo:aux(4)
            __songlua_top_screen.__songlua_children.SongForeground:z(33)
            collectgarbage()
        "#,
        );
        let mut first = compare_tracked_output(&mut pair);
        assert_eq!(
            first
                .actors
                .iter()
                .map(|a| tracked_target_key(a.target))
                .collect::<Vec<_>>(),
            [
                (0, 0),
                (1, 0),
                (2, 0),
                (0, 1),
                (1, 1),
                (2, 1),
                (3, 0),
                (4, 0),
                (4, 1),
                (4, 2),
                (4, 3)
            ]
        );
        assert!(pair.rust.release_song_tracked_actors(&mut first).unwrap());
        pair.script("__songlua_top_screen_player_2=__songlua_top_screen_player_1;__songlua_top_screen.__songlua_children.Overlay=nil;collectgarbage()");
        let mut alias = compare_tracked_output(&mut pair);
        assert_eq!(
            pair.rust
                .song_tracked_actor_value(&alias.actors[0])
                .unwrap(),
            pair.rust
                .song_tracked_actor_value(&alias.actors[3])
                .unwrap()
        );
        assert!(
            !alias
                .actors
                .iter()
                .any(|a| tracked_target_key(a.target) == (4, 1))
        );
        assert!(pair.rust.release_song_tracked_actors(&mut alias).unwrap());
    }
}

#[test]
fn tracked_output_recreates_missing_children_and_retains_native_partial_errors() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(dir.path(), "default.lua", "return Def.ActorFrame{}");
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "tracked fallback"));
    pair.activate(&[&path]);
    pair.script(
        r#"
        old_foreground=__songlua_top_screen.__songlua_children.SongForeground
        old_judgment=__songlua_top_screen_player_1.__songlua_children.Judgment
        __songlua_top_screen.__songlua_children.SongForeground=nil
        __songlua_top_screen_player_1.__songlua_children.Judgment=nil
        __songlua_top_screen_player_1.__songlua_children.Combo=nil
        GetComboFonts=function()return{}end
    "#,
    );
    let mut output = compare_tracked_output(&mut pair);
    pair.script(
        r#"
        assert(__songlua_top_screen.__songlua_children.SongForeground~=old_foreground)
        assert(__songlua_top_screen_player_1.__songlua_children.Judgment~=old_judgment)
        assert(__songlua_top_screen_player_1.__songlua_children.Combo.Name=='Combo')
        __songlua_top_screen.__songlua_children.SongForeground=nil
        __songlua_top_screen_player_2='invalid tracked player';collectgarbage()
    "#,
    );

    assert!(pair.rust.read_song_tracked_actors(pair.scene).is_err());
    pair.script("assert(type(__songlua_top_screen.__songlua_children.SongForeground)=='table');__songlua_top_screen_player_2=__songlua_top_screen_player_1");
    assert!(pair.rust.release_song_tracked_actors(&mut output).unwrap());
    let mut recovered = compare_tracked_output(&mut pair);
    pair.rust
        .release_song_tracked_actors(&mut recovered)
        .unwrap();
}

#[test]
fn tracked_output_uses_manual_draw_snapshots_without_resurrecting_whole_players() {
    for live in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        activation_snapshot::register_path(dir.path());
        let path = write(
            dir.path(),
            "default.lua",
            r#"
        return Def.ActorFrame{OnCommand=function(self)self:SetDrawFunction(function()
            local p=__songlua_top_screen_player_1
            local j=p.__songlua_children.Judgment
            j:visible(true):x(12):y(13):zoom(2):diffuse(0.1,0.2,0.3,0.4):blend('BlendMode_Subtract'):heading(15)
            j:Draw();j:x(99):zoom(3)
            local p2=__songlua_top_screen_player_2
            p2.__songlua_children.Judgment:visible(false):Draw()
            p2.__songlua_children.Combo:visible(false):Draw()
            p:visible(true):x(80):Draw();p:visible(false):x(6)
            collectgarbage()
        end)end}
    "#,
        );
        let mut context = SongLuaCompileContext::new(dir.path(), "tracked manual draw");
        context.live = live;
        let mut pair = Pair::new(context);
        let _ = pair.activate(&[&path]);

        pair.rust.draw_actor_scene(pair.scene).unwrap();
        let mut output = compare_tracked_output(&mut pair);
        assert!(output.actors[0].actor.manual_hud_draw);
        assert!(!output.actors[3].actor.manual_hud_draw);
        assert!(!output.actors[0].actor.initial_state.visible);
        assert_eq!(output.actors[0].actor.initial_state.x, 6.);
        assert_eq!(output.actors[1].actor.initial_state.x, 12.);
        assert_eq!(output.actors[1].actor.initial_state.zoom_x, 2.);
        pair.rust.release_song_tracked_actors(&mut output).unwrap();
    }
}

#[test]
fn tracked_output_pins_identities_through_retirement_and_rejects_foreign_or_released_roots() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(dir.path(), "default.lua", "return Def.ActorFrame{}");
    let context = SongLuaCompileContext::new(dir.path(), "tracked identity lifetime");
    let mut pair = Pair::new(context.clone());
    pair.activate(&[&path]);
    let mut output = compare_tracked_output(&mut pair);
    let copied = output.actors[0].clone();
    let state = copied.actor.clone();
    let mut foreign = SongRuntime::new(&context).unwrap();
    assert!(
        foreign
            .read_song_tracked_actors(pair.scene)
            .unwrap()
            .is_none()
    );
    assert!(foreign.song_tracked_actor_value(&copied).is_err());
    assert!(foreign.release_song_tracked_actors(&mut output).is_err());
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    pair.rust.vm_mut().execute(&compile_named(b"__songlua_top_screen=nil;__songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil;collectgarbage()",b"=release globals").unwrap()).unwrap();
    assert!(pair.rust.song_tracked_actor_value(&copied).is_ok());
    assert!(
        pair.rust
            .read_song_tracked_actors(pair.scene)
            .unwrap()
            .is_none()
    );
    assert!(pair.rust.release_song_tracked_actors(&mut output).unwrap());
    assert!(!pair.rust.release_song_tracked_actors(&mut output).unwrap());
    assert!(pair.rust.song_tracked_actor_value(&copied).is_err());
    drop(pair);
    assert_eq!(copied.actor, state);
}

#[test]
fn tracked_output_manual_snapshot_keeps_raw_data_and_does_not_repeat_authored_getters() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            Def.Sprite{InitCommand=function(self)saved.sprite=self;saved.draw=self.Draw;saved.settexture=self.SetTexture end},
            OnCommand=function(self)self:SetDrawFunction(function()saved.sprite:Draw()end)end,
        }
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "tracked snapshot getters",
    ));
    let _ = pair.activate(&[&path]);
    pair.script(
        r#"
        local sprite=saved.sprite
        setmetatable(sprite,{__index=function(t,k)
            if k=='Texture'then log('texture getter');collectgarbage();return nil end
        end})
        sprite.Draw=saved.draw;sprite.SetTexture=saved.settexture
        __songlua_top_screen_player_1.__songlua_children.Judgment=sprite
    "#,
    );

    pair.rust.draw_actor_scene(pair.scene).unwrap();
    pair.compare();
    let mut output = compare_tracked_output(&mut pair);
    assert!(output.actors[0].actor.manual_hud_draw);
    assert!(output.actors[1].actor.initial_state.sprite_texture);
    pair.rust.release_song_tracked_actors(&mut output).unwrap();

    // An explicit unbound write survives even though the inferred constructor
    // flag was omitted from the preceding raw semantic snapshot.
    pair.script("saved.sprite:SetTexture(nil)");

    pair.rust.draw_actor_scene(pair.scene).unwrap();
    pair.compare();
    let mut output = compare_tracked_output(&mut pair);
    assert!(!output.actors[1].actor.initial_state.sprite_texture);
    pair.rust.release_song_tracked_actors(&mut output).unwrap();
}

#[test]
fn tracked_output_roots_transient_foregrounds_before_reentrant_global_reads() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(dir.path(), "default.lua", "return Def.ActorFrame{}");
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "tracked transient identity",
    ));
    pair.activate(&[&path]);
    pair.script(
        r#"
        local children=__songlua_top_screen.__songlua_children
        rawset(children,'SongForeground',nil)
        setmetatable(children,{__index=function(t,k)
            if k=='SongForeground'then log('foreground');return {__songlua_state_x=7}end
        end})
        local p1=__songlua_top_screen_player_1
        __songlua_top_screen_player_1=nil
        setmetatable(_G,{__index=function(t,k)
            if k=='__songlua_top_screen_player_1'then log('player');collectgarbage();return p1 end
        end})
    "#,
    );
    let mut output = compare_tracked_output(&mut pair);
    assert_eq!(output.actors[6].actor.initial_state.x, 7.);
    pair.rust
        .vm_mut()
        .execute(&compile_named(b"collectgarbage()", b"=tracked transient GC").unwrap())
        .unwrap();
    assert!(matches!(
        pair.rust
            .song_tracked_actor_value(&output.actors[6])
            .unwrap(),
        Value::Table(_)
    ));
    pair.rust.release_song_tracked_actors(&mut output).unwrap();
}

type OutputKind =
    deadsync_song_lua::SongLuaOverlayKind<u32, u32, deadlib_present::actors::TextAttribute>;
fn output_kind_layer(slot: &u32) -> Option<deadsync_song_lua::SongLuaOverlayModelLayer<u32>> {
    (*slot != 0).then(|| {
        deadsync_song_lua::SongLuaOverlayModelLayer::new(
            std::sync::Arc::from("kind-model"),
            std::sync::Arc::from([*slot]),
            [2., 3.],
            [1.; 2],
            [0.; 2],
            [0.; 2],
            [0.; 2],
            None,
            deadsync_song_lua::SongLuaOverlayModelDraw::new(
                [0.; 3], [0.; 3], [1.; 3], [1.; 4], 0.5, false, true,
            ),
        )
    })
}
fn compare_kind_output(pair: &mut Pair, key: &str) -> Option<OutputKind> {
    let rust = saved(pair, key);
    let field = pair
        .rust
        .vm_mut()
        .try_intern(b"__songlua_actor_type")
        .unwrap();
    let Value::String(class) = pair.rust.vm_mut().raw_table_get(rust, field).unwrap() else {
        panic!()
    };
    let class =
        String::from_utf8(pair.rust.vm_mut().string_bytes(class).unwrap().to_vec()).unwrap();
    let selected = reference_test::source_bool(
        "song/tests/rust_song_activation.rs::kind_selected",
        key.as_bytes(),
    );
    let initial = if selected {
        test_support::read_actor_initial_state(&mut pair.rust, Value::Table(rust)).unwrap()
    } else {
        Default::default()
    };
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::kind_inputs",
        &(&class, &initial),
    );
    let names = std::collections::HashSet::new();
    let actual = pair
        .rust
        .read_song_overlay_kind(
            Value::Table(rust),
            deadsync_song_lua::SongOverlayKindInputs {
                actor_type: &class,
                initial_state: initial,
                context: &pair.context,
                aft_capture_names: &names,
            },
            |_, _, _| Ok(std::sync::Arc::from([7u32, 0u32])),
            output_kind_layer,
        )
        .unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_kind_output::eq1",
        &(activation_snapshot::text(format!("{actual:?}"))),
    );
    pair.compare();
    actual
}

#[test]
fn kind_output_preserves_existing_renderer_variants_in_preparation_and_live() {
    use deadsync_song_lua::SongLuaOverlayKind as K;
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    image::RgbaImage::new(8, 8)
        .save(dir.path().join("tile 2x2.png"))
        .unwrap();
    write(dir.path(), "clip.ogg", "test resource");
    write(dir.path(), "model.txt", "test model resource");
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        local function keep(name) return function(self) saved[name]=self end end
        return Def.ActorFrame{Name='root',InitCommand=keep('root'),
            OnCommand=function(self)local w=self:AddWrapperState();w.Name='wrapper';saved.wrapper=w end,
            Def.Actor{Name='actor',InitCommand=keep('actor')},
            Def.ActorFrame{Name='frame',InitCommand=keep('frame')},
            Def.CourseContentsList{Name='course',InitCommand=keep('course')},
            Def.Quad{Name='quad',InitCommand=keep('quad')},
            Def.ActorFrameTexture{Name='aft',InitCommand=function(self)
                saved.aft=self;self:SetTextureName('capture'):setsize(8,10):EnableDepthBuffer(true):Create()
            end},
            Def.ActorProxy{Name='proxy',InitCommand=function(self)
                saved.proxy=self;self:SetTarget(SCREENMAN:GetTopScreen():GetChild('Underlay'))
            end},
            Def.Sprite{Name='unbound',InitCommand=keep('unbound')},
            Def.Sprite{Name='sprite',Texture='tile 2x2.png',Frames={{Frame=1,Delay=.2},{Frame=2,Delay=.3}},InitCommand=keep('sprite')},
            Def.Sprite{Name='missing',Texture='missing.png',InitCommand=keep('missing')},
            Def.Banner{Name='banner',Texture='tile 2x2.png',InitCommand=keep('banner')},
            Def.Sound{Name='sound',File='clip.ogg',InitCommand=keep('sound')},
            Def.BitmapText{Name='text',Font='Common Normal',Text='initial',InitCommand=keep('text')},
            Def.RollingNumbers{Name='rolling',Font='Common Normal',Text='count',InitCommand=keep('rolling')},
            Def.DeviceList{Name='devices',Font='Common Normal',InitCommand=keep('devices')},
            Def.InputList{Name='inputs',Font='Common Normal',InitCommand=keep('inputs')},
            Def.ActorMultiVertex{Name='mesh',InitCommand=function(self)
                saved.mesh=self;self:SetDrawState{Mode='DrawMode_Triangles'}
                self:SetVertices{{{0,0,0},{1,0,0,1},{0,0}},{{1,0,0},{0,1,0,1},{1,0}},{{0,1,0},{0,0,1,1},{0,1}}}
            end},
            Def.ActorMultiVertex{Name='empty',InitCommand=keep('empty')},
            Def.Model{Name='model',Meshes='model.txt',InitCommand=keep('model')},
            Def.Model{Name='noteskin',Meshes='model.txt',__songlua_noteskin_name='skin',__songlua_noteskin_element='Tap Note',InitCommand=keep('noteskin')},
            Def.SongMeterDisplay{Name='meter',StreamWidth=32,Stream=Def.ActorFrame{},InitCommand=keep('meter')},
            Def.GraphDisplay{Name='graph',InitCommand=keep('graph')},
        }
    "#,
    );
    for live in [false, true] {
        let mut context = SongLuaCompileContext::new(dir.path(), "kind variants");
        context.live = live;
        let mut pair = Pair::new(context);
        pair.activate(&[&path]);
        for key in [
            "root", "actor", "frame", "course", "quad", "wrapper", "aft", "proxy", "unbound",
            "sprite", "banner", "sound", "text", "rolling", "devices", "inputs", "mesh", "model",
            "noteskin", "meter", "graph",
        ] {
            assert!(
                compare_kind_output(&mut pair, key).is_some(),
                "{key},live={live}"
            );
        }
        assert!(matches!(
            compare_kind_output(&mut pair, "unbound"),
            Some(K::Actor)
        ));
        assert!(compare_kind_output(&mut pair, "missing").is_none());
        assert_eq!(compare_kind_output(&mut pair, "empty").is_some(), live);
        let Some(K::Model { layers }) = compare_kind_output(&mut pair, "model") else {
            panic!("model kind")
        };
        assert_eq!(layers.len(), 1);
        let Some(K::NoteskinActor { slots, .. }) = compare_kind_output(&mut pair, "noteskin")
        else {
            panic!("noteskin kind")
        };
        assert_eq!(&*slots, &[7, 0]);
    }
}

#[test]
fn kind_output_preserves_startup_sprite_binding_and_aft_name_rules() {
    use deadsync_song_lua::SongLuaOverlayKind as K;
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    image::RgbaImage::new(4, 4)
        .save(dir.path().join("original.png"))
        .unwrap();
    image::RgbaImage::new(8, 8)
        .save(dir.path().join("later.png"))
        .unwrap();
    let path = write(
        dir.path(),
        "default.lua",
        r#"return Def.ActorFrame{Name='root',InitCommand=function(self)saved.root=self end,
            Def.ActorFrameTexture{InitCommand=function(self)self:SetTextureName('capture'):setsize(4,4):Create()end},
            Def.Sprite{Name='sprite',Texture='original.png',InitCommand=function(self)saved.sprite=self end}}
        "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "kind sprite binding",
    ));
    pair.activate(&[&path]);
    pair.script("saved.sprite.__songlua_startup_sprite={Texture='original.png',Frames={{Frame=3,Delay=.25}}};saved.sprite.Texture='later.png'");
    let Some(K::Sprite {
        texture_path,
        states,
        ..
    }) = compare_kind_output(&mut pair, "sprite")
    else {
        panic!("sprite kind")
    };
    assert_eq!(texture_path, dir.path().join("original.png"));
    assert_eq!(states[0].frame, 3);
    let actor = saved(&mut pair, "sprite");
    let names = std::collections::HashSet::from(["capture".to_owned()]);
    pair.script("saved.sprite.__songlua_startup_sprite.Texture='other/../capture'");
    let kind = pair
        .rust
        .read_song_overlay_kind(
            Value::Table(actor),
            deadsync_song_lua::SongOverlayKindInputs {
                actor_type: "Sprite",
                initial_state: Default::default(),
                context: &pair.context,
                aft_capture_names: &names,
            },
            |_, _, _| Ok(std::sync::Arc::<[u32]>::from([])),
            output_kind_layer,
        )
        .unwrap();
    let _ = saved(&mut pair, "root");
    let _ = saved(&mut pair, "sprite");

    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::kind_output_preserves_startup_sprite_binding_and_aft_name_rules::eq3",
        &(activation_snapshot::text(format!("{kind:?}"))),
    );
    assert!(matches!(kind,Some(K::AftSprite{capture_name})if capture_name=="capture"));
}

#[test]
fn kind_output_keeps_startup_text_and_eager_authored_reads_across_gc() {
    use deadsync_song_lua::SongLuaOverlayKind as K;
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"return Def.BitmapText{Name='label',Font='Common Normal',InitCommand=function(self)saved.label=self end}"#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "kind text getters"));
    pair.activate(&[&path]);
    pair.script(
        r#"
        saved.label.__songlua_startup_text='startup';saved.label.Text=nil
        setmetatable(saved.label,{__index=function(t,k)
            if k=='Text' then log('authored text');collectgarbage();return 'later' end
        end})
    "#,
    );
    let Some(K::BitmapText { text, .. }) = compare_kind_output(&mut pair, "label") else {
        panic!("text kind")
    };
    assert_eq!(&*text, "startup");
    assert_eq!(pair.compare(), "authored text");
}

#[test]
fn kind_output_model_errors_and_missing_resources_keep_native_loader_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    write(dir.path(), "model.txt", "model");
    let path = write(
        dir.path(),
        "default.lua",
        r#"return Def.Model{Name='noteskin',Meshes='model.txt',__songlua_noteskin_name='skin',__songlua_noteskin_element='Tap Note',InitCommand=function(self)saved.model=self end}"#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "kind loader errors"));
    pair.activate(&[&path]);
    let actor = saved(&mut pair, "model");
    let names = std::collections::HashSet::new();

    let actual = pair
        .rust
        .read_song_overlay_kind(
            Value::Table(actor),
            deadsync_song_lua::SongOverlayKindInputs {
                actor_type: "Model",
                initial_state: Default::default(),
                context: &pair.context,
                aft_capture_names: &names,
            },
            |_, _, _| Err::<std::sync::Arc<[u32]>, _>("resource failure".to_owned()),
            output_kind_layer,
        )
        .err()
        .unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::kind_output_model_errors_and_missing_resources_keep_native_loader_boundaries::eq1",
        &(actual),
    );
    assert!(actual.contains("skin Tap Note"));
    pair.script("saved.model.__songlua_noteskin_element='not a tap';saved.model.Meshes=nil;collectgarbage()");
    assert!(compare_kind_output(&mut pair, "model").is_none());
    pair.script("saved.model.Meshes='model.txt';saved.model.__songlua_noteskin_element='Tap Note'");
    assert!(compare_kind_output(&mut pair, "model").is_some());
}

#[test]
fn kind_output_payloads_survive_teardown_and_failed_authored_resources_recover() {
    use deadsync_song_lua::SongLuaOverlayKind as K;
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"return Def.BitmapText{Name='label',Font='Common Normal',Text='owned',InitCommand=function(self)saved.label=self end}"#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "kind ownership"));
    pair.activate(&[&path]);
    pair.script("saved.label.Font={}");
    let actor = saved(&mut pair, "label");
    let names = std::collections::HashSet::new();

    assert!(
        pair.rust
            .read_song_overlay_kind(
                Value::Table(actor),
                deadsync_song_lua::SongOverlayKindInputs {
                    actor_type: "BitmapText",
                    initial_state: Default::default(),
                    context: &pair.context,
                    aft_capture_names: &names
                },
                |_, _, _| Ok(std::sync::Arc::<[u32]>::from([])),
                output_kind_layer
            )
            .is_err()
    );
    pair.script("saved.label.Font='Common Normal';collectgarbage()");
    let kind = compare_kind_output(&mut pair, "label").unwrap();
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    drop(pair);
    let K::BitmapText {
        text, font_name, ..
    } = kind
    else {
        panic!("owned kind")
    };
    assert_eq!(&*text, "owned");
    assert_eq!(font_name, "miso");
}

fn compare_function_probe_state(pair: &mut Pair, expected_error: bool) {
    let actual_function = pair.rust.vm_mut().get_global(b"probe_target").unwrap();
    let Value::Table(arguments) = pair.rust.vm_mut().get_global(b"probe_args").unwrap() else {
        panic!("probe args")
    };
    let key = pair.rust.vm_mut().try_intern(b"n").unwrap();
    let len = match pair.rust.vm_mut().raw_table_get(arguments, key).unwrap() {
        Value::Number(n) => n as usize,
        Value::Nil => pair.rust.vm_mut().raw_table_len(arguments).unwrap(),
        other => panic!("argument length: {other:?}"),
    };
    let actual_args = (1..=len)
        .map(|i| {
            pair.rust
                .vm_mut()
                .raw_table_get(arguments, Value::Number(i as f64))
                .unwrap()
        })
        .collect::<Vec<_>>();
    let owner = pair.rust.actor_scene_root(pair.scene).unwrap();
    let Value::Table(owner_id) = owner else {
        panic!("owner")
    };
    let attachment = pair.rust.vm_mut().table_host_state(owner_id).unwrap();

    let actual = test_support::rust_preserving_function_state(
        &mut pair.rust,
        owner,
        actual_function,
        &actual_args,
    );
    assert_eq!(
        pair.rust.vm_mut().table_host_state(owner_id).unwrap(),
        attachment,
        "restore attachment after success/error"
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_function_probe_state::eq2",
        &(expected_error),
    );
    assert_eq!(actual.is_err(), expected_error, "Rust probe result");
    if expected_error {
        return;
    }

    let actual = actual.unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_function_probe_state::eq4",
        &(actual.len()),
    );

    let table = pair.rust.vm_mut().try_new_table().unwrap();
    let retained = pair.rust.vm_mut().pin(Value::Table(table)).unwrap();
    for (i, root) in actual.iter().enumerate() {
        let value = pair.rust.vm_mut().rooted(*root).unwrap();
        pair.rust
            .vm_mut()
            .raw_table_set(table, Value::Number((i + 1) as f64), value)
            .unwrap()
    }
    let key = pair.rust.vm_mut().try_intern(b"n").unwrap();
    pair.rust
        .vm_mut()
        .raw_table_set(table, key, Value::Number(actual.len() as f64))
        .unwrap();
    pair.rust
        .vm_mut()
        .set_global(b"probe_results", Value::Table(table))
        .unwrap();
    for root in actual {
        pair.rust.vm_mut().unpin(root).unwrap();
        assert!(pair.rust.vm_mut().rooted(root).is_err())
    }
    pair.rust.vm_mut().unpin(retained).unwrap();
    pair.compare();
}

#[test]
fn function_probe_state_restores_shared_cells_cycles_helpers_and_global_bindings() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "command probe script state",
    ));
    pair.script(r#"
        local count=3;local data={value=4};data.self=data;local alias=data
        probe_scalar=8;probe_shared={value=10};probe_actor={__songlua_actor_type='Actor',marker=0}
        local helper_count=1
        function probe_helper()helper_count=helper_count+1;probe_shared.value=99 end
        local dynamic_count=0
        function not_selected()dynamic_count=dynamic_count+1 end
        function probe_target(arg)
            count=count+1;data.value=40;data.self={};data={value=80}
            probe_helper();_G['not_'..'selected']();probe_scalar=100;probe_added=true
            probe_shared={replaced=true};probe_actor.marker=31
            collectgarbage();return count,alias.value,arg
        end
        function probe_check()return count,data==alias,data.self==data,data.value,helper_count,dynamic_count end
        probe_args={'argument'}
    "#);
    compare_function_probe_state(&mut pair, false);
    pair.script("log(probe_check());log(probe_scalar,probe_added,probe_shared.value,probe_actor.marker);log(probe_results.n,probe_results[1],probe_results[2],probe_results[3])");
    assert_eq!(
        pair.compare(),
        "3,true,true,4,1,1|8,nil,10,31|3,4,40,argument"
    );
}

#[test]
fn function_probe_state_restores_custom_environments_and_referenced_helpers() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "command probe environments",
    ));
    pair.script(r#"
        local data={value=2};probe_env={value=5,data=data}
        probe_env.helper=setfenv(function()data.value=90;value=80 end,probe_env)
        local proxy=setmetatable({__songlua_env_target=probe_env},{__index=function(_,k)return probe_env[k]or _G[k]end,__newindex=probe_env})
        probe_target=setfenv(function(arg)helper();value=15;data={value=99};created=true;collectgarbage();return value,arg end,proxy)
        probe_args={17}
    "#);
    compare_function_probe_state(&mut pair, false);
    pair.script("collectgarbage();log(probe_env.value,probe_env.data.value,probe_env.created);log(probe_results.n,probe_results[1],probe_results[2])");
    assert_eq!(pair.compare(), "5,2,nil|2,15,17");
}

#[test]
fn function_probe_state_errors_rewind_removed_functions_and_allow_recovery() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "command probe errors",
    ));
    pair.script(r#"
        local count=6;local data={value=12}
        function probe_check()return count,data.value end
        function probe_target()count=90;data.value=30;probe_new='temporary';probe_target=nil;collectgarbage();error('probe failure')end
        probe_args={}
    "#);
    compare_function_probe_state(&mut pair, true);
    pair.script("log(probe_check());log(type(probe_target),probe_new);probe_target=function()return 'recovered'end");
    compare_function_probe_state(&mut pair, false);
    pair.script("collectgarbage();log(probe_results[1])");
    assert_eq!(pair.compare(), "6,12|function,nil|recovered");
}

#[test]
fn function_probe_state_roots_new_return_values_and_retains_aliases_after_gc() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "command probe return roots",
    ));
    pair.script(
        r#"
        local data={value=7};probe_existing=data
        function probe_target(arg)
            data.value=25;local fresh={value=80,arg=arg};collectgarbage()
            return data,fresh,nil,function()return fresh.value end
        end
        probe_args={{value=31}}
    "#,
    );
    compare_function_probe_state(&mut pair, false);
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    pair.script("collectgarbage();log(probe_results.n,probe_results[1]==probe_existing,probe_results[1].value,probe_results[2].value,probe_results[2].arg==probe_args[1],probe_results[3],probe_results[4]())");
    assert_eq!(pair.compare(), "4,true,7,80,true,nil,80");
}

#[test]
fn function_probe_state_handles_large_shared_graphs_and_native_function_roots() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "command probe large state",
    ));
    pair.script(r#"
        local nodes={};for i=1,4200 do nodes[i]={value=i}end;nodes.self=nodes
        function probe_target()
            for i=1,#nodes do nodes[i].value=-1;nodes[i].added={}end
            nodes[1]=nil;nodes.self={};collectgarbage();return 'large'
        end
        function probe_check()return #nodes,nodes[1].value,nodes[4200].value,nodes.self==nodes,nodes[2].added end
        probe_args={}
    "#);
    compare_function_probe_state(&mut pair, false);
    pair.script("log(probe_check());log(probe_results[1]);probe_target=string.upper;probe_args={'host root'}");
    compare_function_probe_state(&mut pair, false);
    pair.script("collectgarbage();log(probe_results[1])");
    assert_eq!(pair.compare(), "4200,1,4200,true,nil|large|HOST ROOT");
}

#[test]
fn function_probe_state_retains_actor_sources_without_global_or_closure_references() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "startup snapshot source roots",
    ));
    let owner = pair.rust.actor_scene_root(pair.scene).unwrap();
    let actor = pair
        .rust
        .vm_mut()
        .execute(
            &compile_named(
                b"return {__songlua_actor_type='Actor',Callback=function()return 4 end}",
                b"=unlinked actor source",
            )
            .unwrap(),
        )
        .unwrap()[0];
    assert!(
        test_support::rust_function_snapshot_retains_actor_source(&mut pair.rust, owner, actor)
            .unwrap()
    );
}

fn compare_indexed_actor_capture(
    pair: &mut Pair,
    selected: &[(usize, &str)],
    arg: Option<f32>,
    beat: Option<f32>,
    error: bool,
    emptied: bool,
) -> Option<deadsync_song_lua::SongActorFunctionBlocks> {
    let mut rust = Vec::new();
    for &(index, key) in selected {
        let b = saved(pair, key);
        rust.push((index, Value::Table(b)));
    }

    let g = pair.rust.vm_mut().get_global(b"indexed_probe").unwrap();

    let actual = pair.rust.capture_song_actor_function(&rust, g, arg, beat);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_indexed_actor_capture::eq1",
        &(error),
    );
    assert_eq!(actual.is_err(), error, "Rust capture error: {actual:?}");
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_indexed_actor_capture::eq3",
        &(pair.rust.values().unwrap()),
    );
    if emptied {
        for (_, b) in &rust {
            assert!(
                pair.rust
                    .actor_command_capture(*b)
                    .unwrap()
                    .blocks
                    .is_empty()
            );
        }
    }
    pair.compare();
    if error {
        None
    } else {
        let actual = actual.unwrap();
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_indexed_actor_capture::eq4",
            &(actual),
        );
        Some(actual)
    }
}
fn capture_metadata(pair: &Pair, names: &[&str]) -> Vec<deadsync_song_lua::ActorSnapshot> {
    pair.rust
        .actor_scene_snapshot(pair.scene)
        .unwrap()
        .into_iter()
        .filter(|a| names.iter().any(|name| a.name == name.as_bytes()))
        .collect()
}

#[test]
fn indexed_actor_capture_preserves_order_duplicates_state_and_runtime_clock() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{
            Def.Quad{Name='capture-a',InitCommand=function(self)saved.a=self;self:x(3):zoom(2)end},
            Def.Quad{Name='capture-b',InitCommand=function(self)saved.b=self;self:y(6)end}}
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "indexed actor capture",
    ));
    pair.activate(&[&path]);

    pair.rust.set_values(2., 3.).unwrap();
    let before = capture_metadata(&pair, &["capture-a", "capture-b"]);
    pair.script(
        r#"indexed_probe=function(value)
        log(value,GAMESTATE:GetSongBeat());saved.a:x(value):linear(.5):x(30):diffusealpha(.25)
        saved.b:sleep(.25):linear(.75):y(20):visible(false)
    end"#,
    );
    let blocks = compare_indexed_actor_capture(
        &mut pair,
        &[(8, "a"), (2, "b"), (8, "a")],
        Some(12.),
        Some(9.),
        false,
        true,
    )
    .unwrap();
    assert_eq!(
        blocks.iter().map(|row| row.0).collect::<Vec<_>>(),
        vec![8, 2, 8]
    );
    assert_eq!(blocks[0].1, blocks[2].1);
    assert_eq!(capture_metadata(&pair, &["capture-a", "capture-b"]), before);
    assert_eq!(pair.rust.values().unwrap(), (2., 3.));
    pair.script("log(saved.a:GetX(),saved.a:GetZoom(),saved.b:GetY(),saved.b:GetVisible())");
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    drop(pair);
    assert_eq!(blocks[0].1[0].delta.x, Some(12.));
}

#[test]
fn indexed_actor_capture_restores_media_effects_raw_tables_and_wrapper_membership() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    image::RgbaImage::new(8, 8)
        .save(dir.path().join("one.png"))
        .unwrap();
    image::RgbaImage::new(4, 4)
        .save(dir.path().join("two.png"))
        .unwrap();
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{Name='capture-frame',InitCommand=function(self)saved.frame=self;self:zoom(2)end,
            Def.BitmapText{Name='capture-text',Font='Common Normal',Text='original',InitCommand=function(self)saved.text=self end},
            Def.Sprite{Name='capture-sprite',Texture='one.png',InitCommand=function(self)saved.sprite=self end}}
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "indexed actor media",
    ));
    pair.activate(&[&path]);
    let before = capture_metadata(&pair, &["capture-frame", "capture-text", "capture-sprite"]);
    pair.script(r#"math.randomseed(1234);indexed_probe=function()
        saved.frame:zoom(5):diffusealpha(.2):hibernate(2):SetUpdateRate(.5)
        probe_wrapper=saved.frame:AddWrapperState():x(42)
        saved.text:settext('temporary'):strokecolor{.2,.3,.4,.5}:diffuse{.1,.2,.3,.4}
        saved.text:AddAttribute(0,{Length=1,Diffuse={.5,.4,.3,.2}})
        saved.sprite:Load('two.png'):animate(false):rate(2):setstate(3):customtexturerect(.1,.2,.8,.9)
        saved.sprite:glowshift():effectperiod(2);saved.frame.probe_marker=19;probe_random=math.random()
        collectgarbage()
    end"#);
    compare_indexed_actor_capture(
        &mut pair,
        &[(0, "frame"), (1, "text"), (2, "sprite")],
        None,
        None,
        false,
        true,
    )
    .unwrap();
    assert_eq!(
        capture_metadata(&pair, &["capture-frame", "capture-text", "capture-sprite"]),
        before
    );
    pair.script("log(saved.frame:GetZoom(),saved.frame:GetNumWrapperStates(),saved.frame.probe_marker,probe_wrapper:GetX(),saved.text:GetText(),saved.sprite:GetState(),math.random())");
}

#[test]
fn indexed_actor_capture_keeps_nested_queue_output_and_outer_script_mutations() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        local count=0
        return Def.ActorFrame{Name='capture-root',InitCommand=function(self)saved.a=self;self:x(4)end,
            LaterCommand=function(self)count=count+1;self:linear(.5):x(28):playcommand('Inner')end,
            InnerCommand=function(self)self:diffusealpha(.4);probe_count=count end}
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "indexed actor queues",
    ));
    let rust = pair.activate(&[&path]);

    pair.rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap();
    let before = capture_metadata(&pair, &["capture-root"]);
    pair.script(
        "indexed_probe=function()saved.a:sleep(.25):queuecommand('Later');probe_global='kept'end",
    );
    let first =
        compare_indexed_actor_capture(&mut pair, &[(5, "a")], None, Some(7.), false, true).unwrap();
    assert!(!first[0].1.is_empty());
    assert_eq!(capture_metadata(&pair, &["capture-root"]), before);
    compare_indexed_actor_capture(&mut pair, &[(5, "a")], None, None, false, true).unwrap();
    pair.script("log(probe_count,probe_global,saved.a:GetX())");
    assert_eq!(pair.compare(), "2,kept,4");
}

#[test]
fn indexed_actor_capture_errors_rewind_state_and_restore_reentrant_properties() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.BitmapText{Name='capture-text',Font='Common Normal',Text='original',InitCommand=function(self)saved.a=self;self:x(3)end}
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "indexed actor errors",
    ));
    pair.activate(&[&path]);
    let before = capture_metadata(&pair, &["capture-text"]);
    pair.script(r#"local a=saved.a;local mt=getmetatable(a);local set=a.settext
        setmetatable(a,{__index=mt.__index,__newindex=function(t,k,v)
            if k=='Text'then log('restore',v);collectgarbage()end;rawset(t,k,v)
        end});rawset(a,'settext',set)
        indexed_probe=function()a:settext('temporary'):x(31);probe_partial=true;error('capture failure')end
    "#);
    compare_indexed_actor_capture(&mut pair, &[(0, "a")], None, Some(8.), true, true);
    assert_eq!(capture_metadata(&pair, &["capture-text"]), before);
    pair.script("log(saved.a:GetText(),saved.a:GetX(),probe_partial);indexed_probe=function()saved.a:x(44)end");
    compare_indexed_actor_capture(&mut pair, &[(0, "a")], None, None, false, true).unwrap();
}

#[test]
fn indexed_actor_capture_snapshot_errors_keep_native_clock_boundary_and_recover() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"return Def.Quad{Name='capture-a',InitCommand=function(self)saved.a=self;self:x(3)end}"#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "indexed actor snapshot errors",
    ));
    pair.activate(&[&path]);
    pair.script("rawset(saved.a,string.char(255),true);indexed_probe=function()probe_called=true;saved.a:x(99)end");
    compare_indexed_actor_capture(&mut pair, &[(0, "a")], None, Some(7.), true, false);
    pair.script("assert(probe_called==nil);rawset(saved.a,string.char(255),nil)");
    compare_indexed_actor_capture(&mut pair, &[(0, "a")], None, Some(9.), false, true).unwrap();
    pair.script("log(probe_called,saved.a:GetX())");
    let mut foreign = SongRuntime::new(&pair.context).unwrap();
    foreign.begin_actor_preparation().unwrap();
    let actor = saved(&mut pair, "a");
    let f = pair.rust.vm_mut().get_global(b"indexed_probe").unwrap();
    assert!(
        foreign
            .capture_song_actor_function(&[(0, Value::Table(actor))], f, None, None)
            .is_err()
    );
}

#[test]
fn indexed_actor_capture_limit_errors_restore_metadata_and_allow_later_capture() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"return Def.Quad{Name='capture-a',InitCommand=function(self)saved.a=self;self:x(3)end}"#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "indexed actor limits",
    ));
    pair.activate(&[&path]);
    let before = capture_metadata(&pair, &["capture-a"]);
    pair.script("indexed_probe=function()saved.a:x(99);while true do end end");
    let actor = saved(&mut pair, "a");
    let f = pair.rust.vm_mut().get_global(b"indexed_probe").unwrap();
    let clock = pair.rust.values().unwrap();
    pair.rust.vm_mut().set_instruction_limit(Some(50_000));
    let error = pair
        .rust
        .capture_song_actor_function(&[(0, Value::Table(actor))], f, None, Some(7.))
        .unwrap_err();
    pair.rust.vm_mut().set_instruction_limit(None);
    assert!(error.contains("limit"), "{error}");
    assert_eq!(pair.rust.values().unwrap(), clock);
    assert_eq!(capture_metadata(&pair, &["capture-a"]), before);
    pair.script("indexed_probe=function()saved.a:x(22)end");
    compare_indexed_actor_capture(&mut pair, &[(0, "a")], None, None, false, true).unwrap();
}

#[test]
fn indexed_actor_capture_preserves_preexisting_tween_queue_steps() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{Name='capture-queued',InitCommand=function(self)saved.a=self;self:x(3)end,
            OnCommand=function(self)self:linear(1):x(40)end}
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "indexed actor queue state",
    ));
    pair.activate(&[&path]);
    let actor = saved(&mut pair, "a");
    let before = pair
        .rust
        .actor_command_capture(Value::Table(actor))
        .unwrap();

    assert!(!before.steps.is_empty());
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::indexed_actor_capture_preserves_preexisting_tween_queue_steps::eq1",
        &(pair
            .rust
            .actor_command_capture(Value::Table(actor))
            .unwrap()
            .steps),
    );
    pair.script("indexed_probe=function()saved.a:x(21)end");
    compare_indexed_actor_capture(&mut pair, &[(0, "a")], None, None, false, true).unwrap();

    let after = pair
        .rust
        .actor_command_capture(Value::Table(actor))
        .unwrap();
    assert_eq!(after.steps, before.steps);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::indexed_actor_capture_preserves_preexisting_tween_queue_steps::eq3",
        &(pair
            .rust
            .actor_command_capture(Value::Table(actor))
            .unwrap()
            .steps),
    );
}

#[test]
fn indexed_actor_capture_roots_first_return_through_restoration_then_releases_it() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.BitmapText{Name='capture-return',Font='Common Normal',Text='original',InitCommand=function(self)saved.a=self end}
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "indexed actor return roots",
    ));
    pair.activate(&[&path]);
    pair.script(
        r#"
        local a=saved.a; local mt=getmetatable(a); local set=a.settext
        probe_weak=setmetatable({}, {__mode='v'})
        setmetatable(a, {__index=mt.__index, __newindex=function(t,k,v)
            if k=='Text' then
                collectgarbage();collectgarbage()
                assert(probe_weak[1] and probe_weak[1].marker=='first')
                assert(probe_weak[2]==nil)
                log('retained',v)
            end
            rawset(t,k,v)
        end});rawset(a,'settext',set)
        indexed_probe=function()
            a:settext('temporary')
            local first={marker='first'}; local extra={marker='extra'}
            probe_weak[1]=first;probe_weak[2]=extra
            return first,extra
        end
    "#,
    );
    compare_indexed_actor_capture(&mut pair, &[(0, "a")], None, Some(8.), false, true).unwrap();
    pair.script(
        "collectgarbage();collectgarbage();assert(probe_weak[1]==nil);log(saved.a:GetText())",
    );
    assert_eq!(pair.compare(), "retained,original|original");
}

fn command_scope_pair(source: &str) -> (tempfile::TempDir, Pair) {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(dir.path(), "default.lua", source);
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "nested command locals",
    ));
    let rust = pair.activate(&[&path]);

    pair.rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap();
    (dir, pair)
}
fn rust_command_script(
    runtime: &mut SongRuntime,
    source: &str,
) -> Result<String, deadlib_lua::Error> {
    let code = compile_named(source.as_bytes(), b"=command scope")?;
    let value = runtime.vm_mut().execute(&code)?[0];
    let Value::String(id) = value else {
        panic!("scope string result")
    };
    Ok(String::from_utf8(runtime.vm_mut().string_bytes(id).unwrap().to_vec()).unwrap())
}
fn compare_command_scope(pair: &mut Pair, source: &str, error: bool) -> Option<String> {
    let actual = test_support::rust_preserving_command_locals(&mut pair.rust, |runtime| {
        rust_command_script(runtime, source)
    });
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_command_scope::eq1",
        &(error),
    );
    assert_eq!(actual.is_err(), error, "{actual:?}");
    match actual {
        Ok(actual) => {
            reference_test::assert_debug(
                "song/tests/rust_song_activation.rs::compare_command_scope::eq3",
                &(actual),
            );
            Some(actual)
        }
        Err(_) => None,
    }
}

#[test]
fn command_local_scopes_capture_commands_reached_by_play_queue_and_broadcast() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        local function counter(key)
            local count=0
            return function(self)count=count+1;_G[key]=count end
        end
        return Def.ActorFrame{InitCommand=function(self)saved.a=self end,
            OuterCommand=function(self)
                self:playcommand('Hidden'):sleep(.1):queuecommand('Queued')
                MESSAGEMAN:Broadcast('Nested')
            end,
            HiddenCommand=counter('last_hidden'),QueuedCommand=counter('last_queued'),
            NestedMessageCommand=counter('last_message')}
    "#,
    );
    let source = "saved.a:playcommand('Outer');saved.a:playcommand('Outer');return table.concat({last_hidden,last_queued,last_message},',')";
    assert_eq!(
        compare_command_scope(&mut pair, source, false).unwrap(),
        "2,2,2"
    );
    // Scalar global assignments are not a shallow global snapshot in this scope.
    pair.script("assert(last_hidden==2 and last_queued==2 and last_message==2)");
    assert_eq!(
        compare_command_scope(&mut pair, source, false).unwrap(),
        "2,2,2"
    );
    pair.script("saved.a:playcommand('Hidden');assert(last_hidden==1)");
}

#[test]
fn command_local_scopes_keep_custom_environment_bindings_and_restore_referenced_tables() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        local env=setmetatable({scalar=0,payload={n=0}}, {__index=_G});probe_env=env
        local fn=assert(loadstring('return function(self)scalar=scalar+1;payload.n=payload.n+1 end'))
        setfenv(fn,env);fn=fn()
        return Def.ActorFrame{InitCommand=function(self)saved.a=self end,HiddenCommand=fn}
    "#,
    );
    let source = "saved.a:playcommand('Hidden');return probe_env.scalar..','..probe_env.payload.n";
    assert_eq!(
        compare_command_scope(&mut pair, source, false).unwrap(),
        "1,1"
    );
    pair.script("assert(probe_env.scalar==1 and probe_env.payload.n==0)");
    assert_eq!(
        compare_command_scope(&mut pair, source, false).unwrap(),
        "2,1"
    );
    pair.script("assert(probe_env.scalar==2 and probe_env.payload.n==0)");
}

#[test]
fn command_local_scopes_restore_shared_cells_cycles_and_detached_commands_after_gc() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        local n=0;local tab={n=0};tab.self=tab;local alias=tab
        probe_peek=function()return n,tab.n,tab==alias,tab.self==tab end
        local function helper()n=n+1;tab.n=tab.n+2 end
        return Def.ActorFrame{InitCommand=function(self)saved.a=self end,
            HiddenCommand=function(self)
                helper();rawset(self,'HiddenCommand',nil);tab={n=99};collectgarbage()
            end}
    "#,
    );
    compare_command_scope(
        &mut pair,
        "saved.a:playcommand('Hidden');collectgarbage();return 'detached'",
        false,
    )
    .unwrap();
    pair.script("local a,b,c,d=probe_peek();assert(a==0 and b==0 and c and d);assert(saved.a.HiddenCommand==nil)");
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    pair.script("collectgarbage();local a,b,c,d=probe_peek();assert(a==0 and b==0 and c and d)");
}

#[test]
fn command_local_scopes_restore_after_callback_errors_and_instruction_limits() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        local n=0;probe_peek=function()return n end
        return Def.ActorFrame{InitCommand=function(self)saved.a=self end,
            HiddenCommand=function(self)n=n+1;error('nested failure')end,
            LimitCommand=function(self)n=n+10;while true do end end,
            GoodCommand=function(self)n=n+1;last_good=n end}
    "#,
    );
    compare_command_scope(
        &mut pair,
        "saved.a:playcommand('Hidden');return 'unexpected'",
        true,
    );
    pair.script("assert(probe_peek()==0)");
    pair.rust.vm_mut().set_instruction_limit(Some(50_000));
    let error = test_support::rust_preserving_command_locals(&mut pair.rust, |runtime| {
        rust_command_script(runtime, "saved.a:playcommand('Limit');return 'unexpected'")
    })
    .unwrap_err();
    pair.rust.vm_mut().set_instruction_limit(None);
    assert!(error.to_string().contains("limit"));
    pair.script("assert(probe_peek()==0)");
    assert_eq!(
        compare_command_scope(
            &mut pair,
            "saved.a:playcommand('Good');return tostring(last_good)",
            false
        )
        .unwrap(),
        "1"
    );
}

#[test]
fn command_local_nested_scopes_defer_locals_but_restore_rng_at_each_boundary() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        local n=0;probe_peek=function()return n end
        return Def.ActorFrame{InitCommand=function(self)saved.a=self end,
            HiddenCommand=function(self)n=n+1;last_count=n end}
    "#,
    );
    pair.script("math.randomseed(1234)");

    let rust=test_support::rust_preserving_command_locals(&mut pair.rust,|runtime| {
        rust_command_script(runtime,"saved.a:playcommand('Hidden');math.random();return ''")?;
        let inner=test_support::rust_preserving_command_locals(runtime,|runtime|rust_command_script(runtime,"saved.a:playcommand('Hidden');math.randomseed(999);math.random();return tostring(probe_peek())"))?;
        let after=rust_command_script(runtime,"saved.a:playcommand('Hidden');return tostring(probe_peek())..','..tostring(math.random())")?;
        Ok((inner,after))
    }).unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::command_local_nested_scopes_defer_locals_but_restore_rng_at_each_boundary::eq1",
        &(rust),
    );
    assert_eq!(rust.0, "2");
    assert!(rust.1.starts_with("3,"));
    pair.script("assert(probe_peek()==0);log(math.random())");
}

fn compare_touched_actor_capture(
    pair: &mut Pair,
    selected: &[(usize, &str)],
    arg: Option<f32>,
    beat: Option<f32>,
    error: bool,
    emptied: bool,
) -> Option<deadsync_song_lua::SongActorFunctionBlocks> {
    let mut rust = Vec::new();
    for &(index, key) in selected {
        let b = saved(pair, key);
        rust.push((index, Value::Table(b)));
    }

    let g = pair.rust.vm_mut().get_global(b"indexed_probe").unwrap();

    let actual = pair
        .rust
        .capture_song_touched_actor_function(&rust, g, arg, beat);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_touched_actor_capture::eq1",
        &(error),
    );
    assert_eq!(actual.is_err(), error, "Rust capture error: {actual:?}");
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_touched_actor_capture::eq3",
        &(pair.rust.values().unwrap()),
    );
    if emptied {
        for (_, b) in &rust {
            assert!(
                pair.rust
                    .actor_command_capture(*b)
                    .unwrap()
                    .blocks
                    .is_empty()
            );
        }
    }
    pair.compare();
    if error {
        None
    } else {
        let actual = actual.unwrap();
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_touched_actor_capture::eq4",
            &(actual),
        );
        Some(actual)
    }
}

#[test]
fn touched_actor_capture_filters_readonly_actors_preserves_order_and_restores_unselected_writes() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        return Def.ActorFrame{
            Def.Quad{Name='touch-a',InitCommand=function(self)saved.a=self;self:x(3)end},
            Def.Quad{Name='touch-b',InitCommand=function(self)saved.b=self;self:y(6)end},
            Def.Quad{Name='touch-other',InitCommand=function(self)saved.other=self;self:x(4)end},
            Def.BitmapText{Font='Common Normal',Text='original',InitCommand=function(self)saved.text=self end}}
    "#,
    );
    let before = capture_metadata(&pair, &["touch-a", "touch-b", "touch-other"]);
    pair.script("rawset(saved.b,string.char(255),true);indexed_probe=function(v) saved.text:settext('kept');saved.b:x(false);log(saved.b:GetY());saved.a:x(v);saved.other:x(99);log(#__songlua_capture_scope_actors) end");
    let blocks = compare_touched_actor_capture(
        &mut pair,
        &[(8, "a"), (2, "b"), (8, "a"), (3, "text")],
        Some(12.),
        Some(9.),
        false,
        false,
    )
    .unwrap();
    assert_eq!(blocks.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [8, 8]);
    assert_eq!(blocks[0].1, blocks[1].1);
    assert_eq!(
        capture_metadata(&pair, &["touch-a", "touch-b", "touch-other"]),
        before
    );
    pair.script("assert(saved.text:GetText()=='kept');rawset(saved.b,string.char(255),nil);assert(__songlua_capture_scope_actors==nil)");
}

#[test]
fn touched_actor_capture_snapshots_at_first_write_and_restores_command_locals_and_rng() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        local n=0;probe_peek=function()return n end
        return Def.BitmapText{Name='touch-text',Font='Common Normal',Text='original',
            InitCommand=function(self)saved.a=self;self:x(3)end,
            HiddenCommand=function(self)n=n+1;last_count=n;self:x(55)end}
    "#,
    );
    pair.script("math.randomseed(1234);indexed_probe=function()saved.a:settext('before'):x(31):settext('after'):playcommand('Hidden');math.randomseed(999);math.random();probe_global=11 end");
    compare_touched_actor_capture(&mut pair, &[(0, "a")], None, Some(8.), false, true).unwrap();
    pair.script("assert(saved.a:GetText()=='before' and saved.a:GetX()==3 and probe_peek()==0 and last_count==1 and probe_global==11);log(math.random())");
}

#[test]
fn touched_actor_capture_nested_queues_rewind_all_reached_actors_and_keep_script_bindings() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        local n=0;probe_peek=function()return n end
        return Def.ActorFrame{Name='touch-a',InitCommand=function(self)saved.a=self;self:x(3)end,
            LaterCommand=function(self)n=n+1;self:linear(.5):x(28);saved.b:y(14);rawset(self,'LaterCommand',nil);collectgarbage()end,
            Def.Quad{Name='touch-b',InitCommand=function(self)saved.b=self;self:y(4)end}}
    "#,
    );
    let before = capture_metadata(&pair, &["touch-a", "touch-b"]);
    pair.script(
        "indexed_probe=function()saved.a:sleep(.25):queuecommand('Later');probe_global='kept'end",
    );
    let blocks =
        compare_touched_actor_capture(&mut pair, &[(7, "b"), (2, "a")], None, None, false, true)
            .unwrap();
    assert_eq!(blocks.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [7, 2]);
    assert_eq!(capture_metadata(&pair, &["touch-a", "touch-b"]), before);
    pair.script("assert(probe_peek()==0 and probe_global=='kept' and saved.a.LaterCommand==nil)");
}

#[test]
fn touched_actor_capture_snapshot_and_callback_errors_clean_scope_and_allow_recovery() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        return Def.ActorFrame{
            Def.Quad{Name='touch-a',InitCommand=function(self)saved.a=self;self:x(3)end},
            Def.Quad{Name='touch-b',InitCommand=function(self)saved.b=self;self:x(4)end}}
    "#,
    );
    let before = capture_metadata(&pair, &["touch-a", "touch-b"]);
    pair.script("rawset(saved.b,string.char(255),true);indexed_probe=function()saved.a:x(19);saved.b:x(21);probe_after=true end");
    compare_touched_actor_capture(&mut pair, &[(0, "a"), (1, "b")], None, Some(9.), true, true);
    assert_eq!(capture_metadata(&pair, &["touch-a", "touch-b"]), before);
    pair.script("assert(probe_after==nil and __songlua_capture_scope_actors==nil);rawset(saved.b,string.char(255),nil);indexed_probe=function()saved.a:x(25);error('touch failure')end");
    compare_touched_actor_capture(&mut pair, &[(0, "a")], None, Some(11.), true, true);
    assert_eq!(capture_metadata(&pair, &["touch-a", "touch-b"]), before);
    pair.script("indexed_probe=function()saved.a:x(37)end");
    compare_touched_actor_capture(&mut pair, &[(0, "a")], None, None, false, true).unwrap();
}

#[test]
fn touched_actor_capture_includes_render_effect_and_media_writes() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        return Def.ActorFrame{
            Def.Quad{Name='touch-a',InitCommand=function(self)saved.a=self end},
            Def.Quad{Name='touch-b',InitCommand=function(self)saved.b=self end},
            Def.BitmapText{Name='touch-text',Font='Common Normal',Text='original',InitCommand=function(self)saved.text=self end}}
    "#,
    );
    pair.script("indexed_probe=function()saved.a:diffusealpha(.4);saved.b:glowshift():effectperiod(2);saved.text:rainbowscroll(true)end");
    let blocks = compare_touched_actor_capture(
        &mut pair,
        &[(0, "a"), (1, "b"), (2, "text")],
        None,
        None,
        false,
        true,
    )
    .unwrap();
    assert_eq!(blocks.len(), 3);
}

#[test]
fn touched_actor_capture_covers_existing_setter_families_independently() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    image::RgbaImage::new(8, 8)
        .save(dir.path().join("one.png"))
        .unwrap();
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{InitCommand=function(self)saved.frame=self end,
            Def.Sprite{Texture='one.png',InitCommand=function(self)saved.sprite=self end},
            Def.BitmapText{Font='Common Normal',Text='original',InitCommand=function(self)saved.text=self end},
            Def.ActorFrameTexture{Name='touch-aft',Width=4,Height=4,InitCommand=function(self)saved.aft=self end},
            Def.ActorMultiVertex{Texture='one.png',InitCommand=function(self)saved.mesh=self end}}
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "touched setter families",
    ));
    let rust = pair.activate(&[&path]);

    pair.rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap();
    let frame = [
        "x(1)",
        "y(2)",
        "z(3)",
        "aux(4)",
        "zoom(2)",
        "rotationx(5)",
        "rotationy(6)",
        "rotationz(7)",
        "skewx(.1)",
        "skewy(.2)",
        "cropleft(.1)",
        "cropright(.2)",
        "croptop(.3)",
        "cropbottom(.4)",
        "fadeleft(.1)",
        "faderight(.2)",
        "fadetop(.3)",
        "fadebottom(.4)",
        "diffuse(.2,.3,.4,.5)",
        "diffuseupperleft(1,0,0,1)",
        "glow(.5,.4,.3,.2)",
        "heading(15)",
        "visible(false)",
        "hibernate(.1)",
        "halign(.2)",
        "valign(.7)",
        "horizalign('left')",
        "basezoom(2)",
        "basezoomx(3)",
        "basezoomy(4)",
        "basezoomz(5)",
        "basealpha(.3)",
        "baserotationz(8)",
        "shadowlength(2)",
        "shadowcolor(.1,.2,.3,.4)",
        "setsize(10,20)",
        "vibrate()",
        "rainbow()",
        "glowshift()",
        "effectperiod(2)",
        "effecttiming(1,2,3,4,5)",
        "effectmagnitude(1,2,3)",
        "effectcolor1(.1,.2,.3,.4)",
        "effectcolor2(.4,.3,.2,.1)",
        "effectoffset(.2)",
        "effectclock('timer')",
        "cullmode('back')",
        "zbias(.1)",
        "draworder(3)",
        "SetDrawByZPosition(true)",
        "fov(60)",
        "fov(-1)",
        "vanishpoint(10,20)",
        "blend('add')",
        "ztest(true)",
        "ztestmode('writeonfail')",
        "zwrite(true)",
        "SetTextureFiltering(false)",
        "MaskSource()",
        "MaskDest()",
        "diffusealpha(.4)",
        "xy(10,20)",
        "addx(2)",
        "SetWidth(12)",
        "SetHeight(13)",
        "align(.2,.3)",
        "vertalign('top')",
        "Center()",
        "FullScreen()",
        "zoomto(10,20)",
        "stretchto(0,0,10,20)",
        "scaletofit(0,0,10,20)",
        "scaletocover(0,0,10,20)",
        "setsize(0,0):scaletofit(0,0,10,20)",
        "setsize(0,0):scaletocover(0,0,10,20)",
    ];
    let sprite = [
        "SetTexture(nil)",
        "setstate(0)",
        "animate(false)",
        "loop(false)",
        "rate(2)",
        "SetAllStateDelays(.125)",
        "texturewrapping(true)",
        "texturetranslate(.25,.5)",
        "customtexturerect(0,.1,.8,.9)",
        "texcoordvelocity(1,2)",
        "play()",
        "pause()",
        "Load('one.png')",
        "SetStateProperties({{Frame=0,Delay=.2},{Frame=1,Delay=.3}})",
        "SetSecondsIntoAnimation(.2)",
        "stretchtexcoords(.1,.2)",
        "CropTo(4,4)",
        "scaletoclipped(5,6)",
    ];
    let text = [
        "uppercase(true)",
        "rainbowscroll(true)",
        "jitter(true)",
        "distort(.2)",
        "textglowmode('TextGlowMode_Inner')",
        "set_mult_attrs_with_diffuse(true)",
        "vertspacing(3)",
        "wrapwidthpixels(120)",
        "maxwidth(80)",
        "maxheight(40)",
        "max_dimension_use_zoom(true)",
    ];
    let aft = ["EnablePreserveTexture(true)", "Create()"];
    for (actor, setters) in [
        ("frame", frame.as_slice()),
        ("sprite", sprite.as_slice()),
        ("text", text.as_slice()),
        ("aft", aft.as_slice()),
        (
            "mesh",
            [
                "LoadTexture(nil)",
                "LoadTexture('one.png')",
                "SetTexture(nil)",
            ]
            .as_slice(),
        ),
    ] {
        for setter in setters {
            for prefix in ["", "sleep(.125):linear(.25):"] {
                eprintln!("touched setter {actor}:{prefix}{setter}");
                pair.script(&format!("indexed_probe=function()saved.{actor}:{prefix}{setter};log(#__songlua_capture_scope_actors)end"));
                compare_touched_actor_capture(
                    &mut pair,
                    &[
                        (0, "frame"),
                        (1, "sprite"),
                        (2, "text"),
                        (3, "aft"),
                        (4, "mesh"),
                    ],
                    None,
                    None,
                    false,
                    true,
                )
                .unwrap();
            }
        }
    }
}

#[test]
fn touched_actor_capture_restores_wrapper_membership_and_detached_wrapper_state() {
    let (_dir, mut pair) =
        command_scope_pair(r#"return Def.ActorFrame{InitCommand=function(self)saved.a=self end}"#);
    pair.script(
        "indexed_probe=function()probe_wrapper=saved.a:AddWrapperState():x(42);collectgarbage()end",
    );
    compare_touched_actor_capture(&mut pair, &[(0, "a")], None, None, false, true).unwrap();
    pair.script("assert(saved.a:GetNumWrapperStates()==0 and probe_wrapper:GetX()==0)");
}

#[test]
fn touched_actor_capture_instruction_limits_restore_state_clock_and_allow_later_capture() {
    let (_dir, mut pair) = command_scope_pair(
        r#"return Def.Quad{Name='touch-limit',InitCommand=function(self)saved.a=self;self:x(3)end}"#,
    );
    let before = capture_metadata(&pair, &["touch-limit"]);
    pair.script("indexed_probe=function()saved.a:x(99);while true do end end");
    let actor = saved(&mut pair, "a");
    let f = pair.rust.vm_mut().get_global(b"indexed_probe").unwrap();
    let clock = pair.rust.values().unwrap();
    pair.rust.vm_mut().set_instruction_limit(Some(50_000));
    let error = pair
        .rust
        .capture_song_touched_actor_function(&[(0, Value::Table(actor))], f, None, Some(7.))
        .unwrap_err();
    pair.rust.vm_mut().set_instruction_limit(None);
    assert!(error.contains("limit"));
    assert_eq!(pair.rust.values().unwrap(), clock);
    assert_eq!(capture_metadata(&pair, &["touch-limit"]), before);
    pair.script(
        "assert(__songlua_capture_scope_actors==nil);indexed_probe=function()saved.a:x(22)end",
    );
    compare_touched_actor_capture(&mut pair, &[(0, "a")], None, None, false, true).unwrap();
}

#[test]
fn touched_actor_capture_roots_previous_globals_and_first_return_through_gc_then_releases() {
    let (_dir, mut pair) = command_scope_pair(
        r#"return Def.BitmapText{Font='Common Normal',Text='original',InitCommand=function(self)saved.a=self end}"#,
    );
    pair.script(r#"
        probe_weak=setmetatable({}, {__mode='v'})
        do local old={marker='previous'};probe_weak[3]=old;__songlua_capture_scope_actors=old end
        local a=saved.a;local mt=getmetatable(a);local set=a.settext
        setmetatable(a,{__index=mt.__index,__newindex=function(t,k,v)
            if k=='Text' then collectgarbage();collectgarbage();assert(probe_weak[1] and probe_weak[1].marker=='first');assert(probe_weak[2]==nil);log('retained',v)end
            rawset(t,k,v)
        end});rawset(a,'settext',set)
        indexed_probe=function()
            collectgarbage();assert(probe_weak[3].marker=='previous')
            a:x(12):settext('temporary');local first={marker='first'};local extra={marker='extra'}
            probe_weak[1]=first;probe_weak[2]=extra;return first,extra
        end
    "#);
    compare_touched_actor_capture(&mut pair, &[(0, "a")], None, Some(8.), false, true).unwrap();
    pair.script("assert(__songlua_capture_scope_actors==probe_weak[3]);__songlua_capture_scope_actors=nil;collectgarbage();collectgarbage();assert(probe_weak[1]==nil and probe_weak[3]==nil);assert(saved.a:GetText()=='original')");
}

#[test]
fn touched_actor_capture_preserves_structural_sort_and_following_updates() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{InitCommand=function(self)saved.parent=self end,
            Def.ActorFrame{InitCommand=function(self)saved.a=self end,OnCommand=function(self)self:SetUpdateFunction(function()log('a')end)end},
            Def.ActorFrame{InitCommand=function(self)saved.b=self end,OnCommand=function(self)self:SetUpdateFunction(function()log('b')end)end}}
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "touched structural ordering",
    ));
    let rust = pair.activate(&[&path]);

    pair.rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap();

    pair.rust.advance_actor_scene(pair.scene, 0.1).unwrap();
    pair.compare();
    pair.script(
        "events={};saved.b:draworder(-1);indexed_probe=function()saved.parent:SortByDrawOrder()end",
    );
    compare_touched_actor_capture(&mut pair, &[(0, "parent")], None, None, false, false).unwrap();

    pair.rust.advance_actor_scene(pair.scene, 0.1).unwrap();
    // This pre-replay native path rebuilds traversal after the structural sort.
    assert_eq!(pair.compare(), "b|a");
}

#[test]
fn touched_actor_capture_preserves_preexisting_tween_queue_steps() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"
        return Def.ActorFrame{Name='capture-queued',InitCommand=function(self)saved.a=self;self:x(3)end,
            OnCommand=function(self)self:linear(1):x(40)end}
    "#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "indexed actor queue state",
    ));
    pair.activate(&[&path]);
    let actor = saved(&mut pair, "a");
    let before = pair
        .rust
        .actor_command_capture(Value::Table(actor))
        .unwrap();

    assert!(!before.steps.is_empty());
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::touched_actor_capture_preserves_preexisting_tween_queue_steps::eq1",
        &(pair
            .rust
            .actor_command_capture(Value::Table(actor))
            .unwrap()
            .steps),
    );
    pair.script("indexed_probe=function()saved.a:x(21)end");
    compare_touched_actor_capture(&mut pair, &[(0, "a")], None, None, false, true).unwrap();

    let after = pair
        .rust
        .actor_command_capture(Value::Table(actor))
        .unwrap();
    assert_eq!(after.steps, before.steps);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::touched_actor_capture_preserves_preexisting_tween_queue_steps::eq3",
        &(pair
            .rust
            .actor_command_capture(Value::Table(actor))
            .unwrap()
            .steps),
    );
}

#[test]
fn message_probe_defaults_match_all_existing_parameter_shapes_and_retained_methods() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "message defaults"));
    pair.script(
        r#"
        function dump_defaults(v)
            if type(v)~='table' then return type(v)=='function' and 'function' or tostring(v) end
            local keys={};for k in pairs(v) do keys[#keys+1]=k end
            table.sort(keys,function(a,b)return tostring(a)<tostring(b)end)
            local parts={};for _,k in ipairs(keys)do parts[#parts+1]=tostring(k)..'='..dump_defaults(v[k])end
            return '{'..table.concat(parts,';')..'}'
        end
        "#,
    );
    for name in [
        "JudgmentMessageCommand",
        "EarlyHitMessageCommand",
        "LifeChangedMessageCommand",
        "HealthStateChangedMessageCommand",
        "ExCountsChangedMessageCommand",
        "PlayerOptionsChangedMessageCommand",
        "OtherMessageCommand",
        "JudgmentCommand",
        "judgmentMessageCommand",
        "JudgmentMessageCommandMessageCommand",
        "",
    ] {
        let root = pair.rust.default_song_message_command_params(name).unwrap();
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::message_probe_defaults_match_all_existing_parameter_shapes_and_retained_methods::eq1",
            &(root.is_some()),
        );

        let value = root
            .map(|r| pair.rust.vm_mut().rooted(r).unwrap())
            .unwrap_or(Value::Nil);
        pair.rust
            .vm_mut()
            .set_global(b"probe_params", value)
            .unwrap();
        pair.script("collectgarbage();collectgarbage();log(dump_defaults(probe_params))");
        if name == "JudgmentMessageCommand" {
            pair.script(
                r#"
                local p=probe_params;local n=p.Notes[1];local r=n:GetTapNoteResult()
                assert(r==n:GetHoldNoteResult() and r==n.TapNoteResult and r==n.HoldNoteResult)
                assert(n:GetTapNoteType()=='TapNoteType_Tap' and n:GetPlayerNumber()=='PlayerNumber_P1')
                local names={'GetTapNoteType','GetTapNoteSource','GetTapNoteSubType','GetPlayerNumber',
                    'GetAttackModifiers','GetHoldDuration','GetAttackDuration','GetKeysoundIndex'}
                for _,k in ipairs(names)do log(k,n[k](),n[k](false,{},'ignored'))end
                for _,k in ipairs({'GetHeld','GetHidden','GetTapNoteOffset','GetTapNoteScore'})do log(k,r[k](),r[k](false,{}))end
                saved.result_method=n.GetTapNoteResult;probe_params=nil;collectgarbage();collectgarbage()
                assert(saved.result_method()==r);log(saved.result_method():GetTapNoteScore())
                "#,
            );
        } else if name == "LifeChangedMessageCommand" {
            pair.script(
                r#"
                local m=probe_params.LifeMeter
                for _,k in ipairs({'GetLife','IsFailing','IsHot','IsInDanger'})do log(k,m[k](),m[k](false,{},7))end
                saved.meter=m.GetLife;probe_params=nil;collectgarbage();log(saved.meter())
                "#,
            );
        }
        if let Some(root) = root {
            pair.rust.vm_mut().unpin(root).unwrap();
        }
    }
}

fn install_message_probe(pair: &mut Pair, actor: &str, field: &str) -> deadlib_lua::Root {
    let b = saved(pair, actor);

    let vm = pair.rust.vm_mut();
    let key = vm.try_intern(field.as_bytes()).unwrap();
    let command = vm.raw_table_get(b, key).unwrap();
    let root = pair
        .rust
        .create_song_message_capture_runner(Value::Table(b), field, command)
        .unwrap();
    let vm = pair.rust.vm_mut();
    vm.set_global(b"indexed_probe", vm.rooted(root).unwrap())
        .unwrap();
    root
}

#[test]
fn message_probe_runner_keeps_selected_handler_defaults_guard_and_nested_queues() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        local calls=0
        return Def.ActorFrame{
            Def.Quad{Name='message-a',InitCommand=function(self)saved.a=self;self:x(3)end,
                JudgmentMessageCommand=function(self,p)
                    calls=calls+1;p.calls=(p.calls or 0)+1
                    log(calls,p.calls,p.Player,p.Notes[1]:GetTapNoteResult():GetTapNoteScore())
                    assert(p.Player=='PlayerNumber_P1' and p.Notes[1]:GetTapNoteResult():GetTapNoteScore()=='TapNoteScore_W3')
                    probe_events=probe_events..calls..','..p.calls..'|'
                    self:x(calls+10):playcommand('JudgmentMessage'):queuecommand('After')
                    saved.b:queuecommand('Other');collectgarbage()
                end,
                AfterCommand=function(self)self:linear(.25):y(12);log('after');probe_events=probe_events..'after|'end},
            Def.Quad{Name='message-b',InitCommand=function(self)saved.b=self end,
                OtherCommand=function(self)self:z(14);log('other');probe_events=probe_events..'other|'end},
        }
        "#,
    );
    let root = install_message_probe(&mut pair, "a", "JudgmentMessageCommand");
    pair.script("probe_events='';saved.a.JudgmentMessageCommand=function()error('replacement must not run')end;collectgarbage();collectgarbage()");
    for _ in 0..2 {
        let blocks = compare_touched_actor_capture(
            &mut pair,
            &[(8, "b"), (3, "a")],
            None,
            Some(6.),
            false,
            true,
        )
        .unwrap();
        assert_eq!(blocks.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [8, 3]);
    }
    assert_eq!(pair.compare(), "");
    pair.script("assert(probe_events=='1,1|other|after|1,2|other|after|')");
    pair.rust.vm_mut().unpin(root).unwrap();
}

#[test]
fn message_probe_runner_without_defaults_preserves_arity_directory_and_unrelated_queues() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        return Def.ActorFrame{
            Def.Quad{Name='message-a',InitCommand=function(self)saved.a=self end,
                CustomMessageCommand=function(...)
                    local self=...;assert(select('#',...)==1)
                    assert(__songlua_script_dir==self.__songlua_script_dir)
                    log('custom');probe_events=probe_events..'custom|';self:x(9):queuecommand('After')
                end,
                AfterCommand=function(self)self:y(10);log('after');probe_events=probe_events..'after|'end},
            Def.Quad{Name='message-unrelated',InitCommand=function(self)saved.unrelated=self end,
                OnCommand=function(self)self:sleep(1):queuecommand('Pending')end,
                PendingCommand=function(self)self:z(21);log('unrelated');probe_events=probe_events..'unrelated|'end},
        }
        "#,
    );
    pair.script("probe_events='';__songlua_script_dir='outside'");
    let id = saved(&mut pair, "unrelated");
    let before = pair.rust.actor_command_capture(Value::Table(id)).unwrap();

    assert!(!before.queued_commands.is_empty());
    let root = install_message_probe(&mut pair, "a", "CustomMessageCommand");
    compare_touched_actor_capture(
        &mut pair,
        &[(0, "a"), (1, "unrelated")],
        None,
        None,
        false,
        false,
    )
    .unwrap();
    pair.script("assert(__songlua_script_dir=='outside');assert(saved.unrelated:GetZ()==0);assert(probe_events=='custom|after|')");
    assert_eq!(pair.compare(), "");
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::message_probe_runner_without_defaults_preserves_arity_directory_and_unrelated_queues::eq2",
        &(before),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::message_probe_runner_without_defaults_preserves_arity_directory_and_unrelated_queues::eq3",
        &(pair.rust.actor_command_capture(Value::Table(id)).unwrap()),
    );
    pair.rust.vm_mut().unpin(root).unwrap();
}

#[test]
fn message_probe_runner_errors_release_command_guard_and_recover_after_instruction_abort() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        return Def.Quad{Name='message-errors',InitCommand=function(self)saved.a=self;self:x(3)end,
            CustomMessageCommand=function(self)
                self:x(15)
                if probe_failure=='limit' then while true do end end
                if probe_failure=='runtime' then error('message probe failure')end
                self:queuecommand('After')
            end,
            AfterCommand=function(self)self:y(12)end}
        "#,
    );
    let root = install_message_probe(&mut pair, "a", "CustomMessageCommand");
    pair.script("probe_failure='runtime'");
    compare_touched_actor_capture(&mut pair, &[(0, "a")], None, Some(8.), true, true);
    pair.script("probe_failure=nil");
    compare_touched_actor_capture(&mut pair, &[(0, "a")], None, None, false, true).unwrap();
    rust_command_script(&mut pair.rust, "probe_failure='limit';return ''").unwrap();
    let actor = saved(&mut pair, "a");
    let runner = pair.rust.vm_mut().rooted(root).unwrap();
    pair.rust.vm_mut().set_instruction_limit(Some(50_000));
    let error = pair
        .rust
        .capture_song_touched_actor_function(&[(0, Value::Table(actor))], runner, None, Some(9.))
        .unwrap_err();
    pair.rust.vm_mut().set_instruction_limit(None);
    assert!(error.contains("limit"), "{error}");
    rust_command_script(&mut pair.rust, "probe_failure=nil;return ''").unwrap();
    compare_touched_actor_capture(&mut pair, &[(0, "a")], None, None, false, true).unwrap();
    pair.rust.vm_mut().unpin(root).unwrap();
}

#[test]
fn message_probe_explicit_roots_hold_defaults_and_detached_handler_until_release() {
    let (_dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self end}");

    let root = pair
        .rust
        .default_song_message_command_params("LifeChangedMessageCommand")
        .unwrap()
        .unwrap();

    let vm = pair.rust.vm_mut();
    vm.set_global(b"probe_params", vm.rooted(root).unwrap())
        .unwrap();
    pair.script("probe_weak=setmetatable({}, {__mode='v'});probe_weak[1]=probe_params;probe_weak[2]=probe_params.LifeMeter;probe_params=nil;collectgarbage();collectgarbage();assert(probe_weak[2]:GetLife()==0.5)");

    pair.rust.vm_mut().unpin(root).unwrap();
    pair.script(
        "collectgarbage();collectgarbage();assert(probe_weak[1]==nil and probe_weak[2]==nil)",
    );
    pair.script(r#"
        do local payload={value=17};probe_weak[3]=payload
            rawset(saved.a,'CustomMessageCommand',function(self)assert(payload.value==17);self:x(7)end)
        end
    "#);
    let root = install_message_probe(&mut pair, "a", "CustomMessageCommand");

    pair.script("rawset(saved.a,'CustomMessageCommand',nil);indexed_probe=nil;collectgarbage();collectgarbage();assert(probe_weak[3].value==17)");
    pair.rust.retire_actor_scene(pair.scene).unwrap();

    let vm = pair.rust.vm_mut();
    vm.call(vm.rooted(root).unwrap(), &[]).unwrap();
    pair.script("assert(saved.a:GetX()==7)");

    pair.rust.vm_mut().unpin(root).unwrap();
    pair.script("collectgarbage();collectgarbage();assert(probe_weak[3]==nil)");
}

fn compare_function_action(
    pair: &mut Pair,
    overlays: &[(usize, &str)],
    tracked: &[&str],
    beat: f32,
) -> Result<deadsync_song_lua::SongLuaFunctionActionCapture, (String, String)> {
    let mut rust = Vec::new();
    for &(index, key) in overlays {
        let b = saved(pair, key);
        rust.push((index, Value::Table(b)));
    }
    let mut rust_tracked = Vec::new();
    for key in tracked {
        let b = saved(pair, key);
        rust_tracked.push(Value::Table(b));
    }

    let g = pair.rust.vm_mut().get_global(b"action_probe").unwrap();

    let actual = pair
        .rust
        .capture_song_function_action(&rust, &rust_tracked, g, beat);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_function_action::eq1",
        &(actual.is_err()),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_function_action::eq2",
        &(pair.rust.values().unwrap()),
    );
    pair.compare();
    match actual {
        Ok(actual) => {
            reference_test::assert_debug(
                "song/tests/rust_song_activation.rs::compare_function_action::eq3",
                &(actual),
            );
            Ok(actual)
        }
        Err(b) => Err((
            String::from_utf8(reference_test::source_bytes(
                "song/tests/rust_song_activation.rs::compare_function_action::error",
                b"",
            ))
            .unwrap(),
            b,
        )),
    }
}

#[test]
fn function_action_collects_overlay_tracked_broadcast_sound_and_rewinds_shared_state() {
    let (dir, mut pair) = command_scope_pair(
        r#"
        local commands=0
        command_peek=function()return commands end
        return Def.ActorFrame{
            Def.Quad{Name='action-a',InitCommand=function(self)saved.a=self;self:x(3)end,
                MoveCommand=function(self)commands=commands+1;self:y(commands+10)end},
            Def.Quad{Name='action-b',InitCommand=function(self)saved.b=self end,
                SuppressedMessageCommand=function()error('suppressed handler ran')end,
                DeliveredMessageCommand=function(self,p)assert(p.marker==7);self:diffusealpha(.4)end},
            Def.Quad{Name='action-c',InitCommand=function(self)saved.c=self end},
        }
    "#,
    );
    let sound = dir.path().join("probe.wav");
    fs::write(&sound, b"captured path").unwrap();
    pair.script(&format!(
        "probe_sound={:?}",
        deadsync_song_lua::file_path_string(&sound)
    ));
    pair.script(
        r#"
        local n=0;local data={value=3};data.self=data
        action_check=function()return n,data.value,data.self==data end
        probe_scalar='before'
        __songlua_broadcast_messages={marker='old-broadcasts'}
        __songlua_sound_calls={marker='old-sounds'}
        __songlua_suppress_broadcast_dispatch='old-suppress'
        function action_probe()
            n=n+1;data.value=91;probe_scalar='changed';probe_created={}
            saved.a:sleep(.125):linear(.25):x(21):queuecommand('Move')
            saved.c:z(32)
            MESSAGEMAN:Broadcast('Suppressed')
            MESSAGEMAN:Broadcast('Delivered',{marker=7})
            SOUND:PlayOnce(probe_sound);SOUND:PlayOnce(probe_sound)
            math.random();collectgarbage()
        end
    "#,
    );
    for _ in 0..2 {
        let output = compare_function_action(
            &mut pair,
            &[(7, "a"), (7, "b"), (2, "a")],
            &["c", "a", "c"],
            8.,
        )
        .unwrap();
        assert_eq!(
            output
                .overlay_blocks
                .iter()
                .map(|(i, _)| *i)
                .collect::<Vec<_>>(),
            [7, 7, 2]
        );
        assert_eq!(
            output
                .tracked_blocks
                .iter()
                .map(|(i, _)| *i)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(
            output.broadcasts,
            [("Suppressed".into(), false), ("Delivered".into(), true)]
        );
        assert_eq!(output.sound_paths, [sound.clone(), sound.clone()]);
        assert_eq!(output.side_effects, 4);
        pair.script(r#"
            local n,v,same=action_check();assert(n==0 and v==3 and same)
            assert(command_peek()==0 and probe_scalar=='before' and probe_created==nil)
            assert(__songlua_broadcast_messages.marker=='old-broadcasts' and __songlua_sound_calls.marker=='old-sounds')
            assert(__songlua_suppress_broadcast_dispatch=='old-suppress' and saved.a:GetX()==3 and saved.c:GetZ()==0)
            log(math.random())
        "#);
    }
}

#[test]
fn function_action_restores_column_membership_and_cloned_spline_values_after_replacement() {
    let (_dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self end}");
    pair.script(r#"
        probe_player=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
        probe_field=probe_player:GetChild('NoteField');probe_columns=probe_field:GetColumnActors()
        probe_handler=probe_columns[1]:GetPosHandler();probe_spline=probe_handler:GetSpline()
        probe_spline:SetSize(2);probe_spline:SetPoint(1,{5,6,7});probe_spline:SetPoint(2,{8,9,10})
        probe_points=probe_spline.__songlua_spline_points
        function action_probe()
            local player=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
            local field=player:GetChild('NoteField');local c=field:GetColumnActors()[1]
            local h=c:GetPosHandler();local s=h:GetSpline()
            h:SetSplineMode('NoteColumnSplineMode_Position'):SetReceptorT(9):SetBeatsPerT(3):SetSubtractSongBeat(false)
            s:SetPoint(1,{99,98,97});s:SetSize(1);s:SetLoop(true)
            field.__songlua_note_columns=nil;assert(field:GetColumnActors()[1]~=c)
            rawset(player.__songlua_children,'NoteField',{marker='replacement'})
            collectgarbage()
        end
    "#);
    compare_function_action(&mut pair, &[], &[], 12.).unwrap();
    pair.script(r#"
        assert(probe_player:GetChild('NoteField')==probe_field)
        assert(probe_field:GetColumnActors()==probe_columns and probe_columns[1]:GetPosHandler()==probe_handler)
        assert(probe_handler.__songlua_spline_mode=='NoteColumnSplineMode_Disabled')
        assert(probe_handler.__songlua_receptor_t==0 and probe_handler.__songlua_beats_per_t==1 and probe_handler.__songlua_subtract_song_beat)
        assert(probe_spline:GetSize()==2 and not probe_spline:get_loop())
        assert(probe_spline.__songlua_spline_points~=probe_points)
        assert(probe_points[1][1]==99 and probe_spline.__songlua_spline_points[1][1]==5 and probe_spline.__songlua_spline_points[2][3]==10)
    "#);
}

#[test]
fn function_action_cleanup_order_retains_first_return_and_counts_late_column_side_effects() {
    let (_dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self;self:x(3)end}");
    pair.script(r#"
        local weak=setmetatable({}, {__mode='v'});saved.a.action_weak=weak
        local handler=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):GetColumnActors()[1]:GetPosHandler()
        local restored=0
        probe_cleanup_check=function()return restored end
        setmetatable(handler,{__newindex=function(t,k,v)
            if k=='__songlua_spline_mode' then
                collectgarbage();collectgarbage();assert(weak[1].marker=='first' and weak[2]==nil)
                assert(saved.a:GetX()==91)
                restored=restored+1
                MESSAGEMAN:Broadcast('LateCleanup')
            end
            rawset(t,k,v)
        end})
        function action_probe()
            saved.a:x(91)
            local h=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):GetColumnActors()[1]:GetPosHandler()
            rawset(h,'__songlua_spline_mode',nil)
            local first={marker='first'};local extra={marker='extra'}
            local weak=saved.a.action_weak;weak[1]=first;weak[2]=extra
            return first,extra
        end
    "#);
    let output = compare_function_action(&mut pair, &[(0, "a")], &[], 9.).unwrap();
    assert!(output.broadcasts.is_empty());
    assert_eq!(output.side_effects, 1);
    pair.script("assert(probe_cleanup_check()==1 and saved.a:GetX()==3);collectgarbage();collectgarbage();assert(saved.a.action_weak[1]==nil and saved.a.action_weak[2]==nil)");
}

#[test]
fn function_action_output_errors_preserve_cleanup_and_precede_callback_errors() {
    let (_dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self;self:x(3)end}");
    for malformed in [
        "__songlua_broadcast_messages[1]={message={}}",
        "__songlua_sound_calls[1]={}",
        "__songlua_broadcast_messages[1]=false",
    ] {
        pair.script(&format!(
            "function action_probe()saved.a:x(90);{malformed};error('body-marker')end"
        ));
        let (a, b) = compare_function_action(&mut pair, &[(0, "a")], &[], 7.).unwrap_err();
        assert!(
            !a.contains("body-marker") && !b.contains("body-marker"),
            "{a};{b}"
        );
        pair.script("assert(saved.a:GetX()==3 and __songlua_broadcast_messages==nil and __songlua_sound_calls==nil and __songlua_suppress_broadcast_dispatch==nil)");
    }
    pair.script("function action_probe()saved.a:x(31);error('body-marker')end");
    let (a, b) = compare_function_action(&mut pair, &[(0, "a")], &[], 7.).unwrap_err();
    assert!(a.contains("body-marker") && b.contains("body-marker"));
    pair.script("action_probe=function()saved.a:x(22)end");
    compare_function_action(&mut pair, &[(0, "a")], &[], 7.).unwrap();
}

#[test]
fn function_action_instruction_abort_restores_actor_clock_globals_and_allows_later_capture() {
    let (_dir, mut pair) = command_scope_pair(
        "return Def.Quad{Name='action-limit',InitCommand=function(self)saved.a=self;self:x(3)end}",
    );
    pair.script("action_probe=function()saved.a:x(99);probe_created={};MESSAGEMAN:Broadcast('LimitEvent');while true do end end");
    let before = capture_metadata(&pair, &["action-limit"]);
    let clock = pair.rust.values().unwrap();
    let actor = saved(&mut pair, "a");
    let f = pair.rust.vm_mut().get_global(b"action_probe").unwrap();
    pair.rust.vm_mut().set_instruction_limit(Some(50_000));
    let error = pair
        .rust
        .capture_song_function_action(&[(0, Value::Table(actor))], &[], f, 11.)
        .unwrap_err();
    pair.rust.vm_mut().set_instruction_limit(None);
    assert!(error.contains("limit"), "{error}");
    assert_eq!(pair.rust.values().unwrap(), clock);
    assert_eq!(capture_metadata(&pair, &["action-limit"]), before);
    pair.script("assert(probe_created==nil and __songlua_broadcast_messages==nil and __songlua_sound_calls==nil and __songlua_suppress_broadcast_dispatch==nil);action_probe=function()saved.a:x(12)end");
    let output = compare_function_action(&mut pair, &[(0, "a")], &[], 8.).unwrap();
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    drop(pair);
    assert_eq!(output.overlay_blocks[0].1[0].delta.x, Some(12.));
}

#[test]
fn function_action_sound_manager_keeps_existing_methods_lookup_rules_and_return_identity() {
    let (dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self end}");
    fs::create_dir(dir.path().join("sounds")).unwrap();
    for file in ["same.wav", "sounds/same.wav", "wrong.txt"] {
        fs::write(dir.path().join(file), b"captured resource").unwrap();
    }
    pair.script(&format!(
        "sound_base={:?};sound_sub={:?};sound_absolute={:?};sound_wrong={:?};sound_expected_sub={:?}",
        dir.path().to_string_lossy(),
        dir.path().join("sounds").to_string_lossy(),
        dir.path().join("same.wav").to_string_lossy(),
        dir.path().join("wrong.txt").to_string_lossy(),
        dir.path().join("sounds").join("same.wav").to_string_lossy(),
    ));
    pair.script(r#"
        __songlua_sound_paths={};__songlua_sound_calls={}
        local before=__songlua_side_effect_count or 0
        assert(SOUND:DimMusic()==SOUND and SOUND.StopMusic(false)==SOUND and SOUND:PlayAnnouncer()==SOUND)
        assert(SOUND:GetPlayerBalance(false,{})==0 and SOUND.IsTimingDelayed()==false)
        assert(__songlua_side_effect_count==before+3)
        __songlua_script_dir=sound_sub
        assert(SOUND:PlayMusicPart('same.wav')==SOUND)
        assert(__songlua_sound_paths[1]==sound_expected_sub and #__songlua_sound_calls==0, 'actual path='..tostring(__songlua_sound_paths[1])..'; expected='..sound_expected_sub..'; calls='..#__songlua_sound_calls)
        assert(SOUND.PlayOnce({},'same.wav')==SOUND)
        __songlua_script_dir=nil;assert(SOUND.PlayOnce('same.wav')==SOUND)
        assert(SOUND:PlayOnce(sound_absolute)==SOUND)
        assert(#__songlua_sound_paths==2 and #__songlua_sound_calls==3)
        assert(SOUND:PlayOnce(sound_wrong)==SOUND and SOUND:PlayOnce('missing.wav')==SOUND)
        assert(SOUND:PlayOnce(false)==SOUND and SOUND:PlayOnce(string.char(255))==SOUND and SOUND.PlayOnce({})==SOUND)
        assert(#__songlua_sound_calls==3 and #__songlua_sound_paths==2)
        __songlua_script_dir=false;assert(SOUND:PlayOnce('same.wav')==SOUND and #__songlua_sound_calls==3)
        __songlua_script_dir=nil
        local mt=getmetatable(_G);local looked_up=false
        setmetatable(_G,{__index=function(t,k)if k=='__songlua_script_dir'then looked_up=true end end})
        assert(SOUND:PlayOnce(77)==SOUND and not looked_up);setmetatable(_G,mt)
        local old=SOUND;local retained=SOUND.StopMusic
        SOUND={replacement=true};collectgarbage();assert(retained()==old);SOUND=old
        log(table.concat(__songlua_sound_calls,'|'),table.concat(__songlua_sound_paths,'|'))
    "#);
}

#[test]
fn function_action_sound_paths_preserve_authored_and_native_separator_spellings() {
    let (dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self end}");
    fs::create_dir(dir.path().join("sounds")).unwrap();
    for path in [
        dir.path().join("same.wav"),
        dir.path().join("sounds").join("same.wav"),
    ] {
        fs::write(path, b"captured resource").unwrap();
    }
    let authored_sub = deadsync_song_lua::file_path_string(&dir.path().join("sounds"));
    let authored_absolute = deadsync_song_lua::file_path_string(&dir.path().join("same.wav"));
    let calls = [
        std::path::Path::new(&authored_sub)
            .join("same.wav")
            .to_string_lossy()
            .into_owned(),
        dir.path().join("same.wav").to_string_lossy().into_owned(),
        authored_absolute.clone(),
    ];
    let mut paths = Vec::new();
    for path in &calls {
        if !paths.contains(path) {
            paths.push(path.clone());
        }
    }
    let lua_list = |values: &[String]| {
        values
            .iter()
            .map(|v| format!("{v:?}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    pair.script(&format!(
        "sound_sub={authored_sub:?};sound_absolute={authored_absolute:?};expected_calls={{{}}};expected_paths={{{}}}",
        lua_list(&calls), lua_list(&paths),
    ));
    pair.script(
        r#"
        __songlua_sound_paths={};__songlua_sound_calls={}
        __songlua_script_dir=sound_sub;assert(SOUND:PlayOnce('same.wav')==SOUND)
        __songlua_script_dir=nil;assert(SOUND:PlayOnce('same.wav')==SOUND)
        assert(SOUND:PlayOnce(sound_absolute)==SOUND)
        assert(#__songlua_sound_calls==#expected_calls and #__songlua_sound_paths==#expected_paths)
        for i,v in ipairs(expected_calls)do assert(__songlua_sound_calls[i]==v)end
        for i,v in ipairs(expected_paths)do assert(__songlua_sound_paths[i]==v)end
        log(table.concat(__songlua_sound_calls,'|'),table.concat(__songlua_sound_paths,'|'))
    "#,
    );
}

#[test]
fn function_action_sound_manager_preserves_partial_errors_gc_and_runtime_message_context() {
    let (dir, mut pair) = command_scope_pair(
        r#"
        return Def.Quad{InitCommand=function(self)saved.a=self end,
            CaptureMessageCommand=function()SOUND:PlayOnce(sound_file)end}
    "#,
    );
    let sound = dir.path().join("runtime.wav");
    fs::write(&sound, b"captured resource").unwrap();
    pair.script(&format!(
        "sound_file={:?}",
        deadsync_song_lua::file_path_string(&sound)
    ));
    pair.script(r#"
        __songlua_sound_paths={false};__songlua_sound_calls={}
        assert(not pcall(SOUND.PlayOnce,SOUND,sound_file))
        assert(__songlua_sound_calls[1]==sound_file)
        local prior=getmetatable(_G);local weak=setmetatable({}, {__mode='v'})
        __songlua_sound_paths=nil;__songlua_sound_calls=nil
        setmetatable(_G,{__index=function(t,k)
            if k=='__songlua_sound_calls' then local v={};weak[1]=v;collectgarbage();return v end
            if k=='__songlua_sound_paths' then collectgarbage();collectgarbage();assert(weak[1]==nil);return {} end
        end})
        assert(SOUND:PlayOnce(sound_file)==SOUND);setmetatable(_G,prior)
        __songlua_sound_paths={};__songlua_sound_calls={}
    "#);

    pair.rust.set_values(4., 2.).unwrap();

    pair.rust.begin_actor_capture(true);
    pair.script("SOUND:PlayOnce(sound_file);MESSAGEMAN:Broadcast('Capture');SOUND:PlayMusicPart(sound_file)");

    let actual = pair.rust.take_prepared_sounds();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::function_action_sound_manager_preserves_partial_errors_gc_and_runtime_message_context::eq1",
        &(actual),
    );
    assert_eq!(actual.len(), 2);
    assert!(
        actual
            .iter()
            .all(|e| e.beat == 4. && e.queued_startup && e.path == sound)
    );
    assert_eq!(actual[0].source_message, None);
    assert_eq!(actual[1].source_message.as_deref(), Some("Capture"));

    pair.rust.end_actor_capture();
}

#[test]
fn function_action_column_restore_errors_keep_native_partial_state_and_release_scope() {
    let (_dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self;self:x(3)end}");
    pair.script(r#"
        local h=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):GetColumnActors()[1]:GetPosHandler()
        setmetatable(h,{__newindex=function(t,k,v)
            if k=='__songlua_spline_mode' then error('column-restore-marker')end
            rawset(t,k,v)
        end})
        function action_probe()
            saved.a:x(91);probe_created=true
            local h=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):GetColumnActors()[1]:GetPosHandler()
            rawset(h,'__songlua_spline_mode',nil)
        end
    "#);
    let (a, b) = compare_function_action(&mut pair, &[(0, "a")], &[], 9.).unwrap_err();
    assert!(a.contains("column-restore-marker") && b.contains("column-restore-marker"));
    pair.script(r#"
        assert(probe_created and saved.a:GetX()==91)
        assert(__songlua_capture_scope_actors==nil and type(__songlua_broadcast_messages)=='table')
        assert(type(__songlua_sound_calls)=='table' and __songlua_suppress_broadcast_dispatch==true)
        local h=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):GetColumnActors()[1]:GetPosHandler()
        setmetatable(h,nil);rawset(h,'__songlua_spline_mode','NoteColumnSplineMode_Disabled')
        action_probe=function()saved.a:x(12)end
    "#);
    compare_function_action(&mut pair, &[(0, "a")], &[], 12.).unwrap();
    pair.script("assert(saved.a:GetX()==91)");
}

fn compare_actor_command(
    pair: &mut Pair,
    key: &str,
    name: &str,
) -> Result<Vec<deadsync_song_lua::SongLuaOverlayCommandBlock>, (String, String)> {
    let b = saved(pair, key);

    let actual = pair.rust.capture_song_actor_command(Value::Table(b), name);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_actor_command::eq1",
        &(actual.is_err()),
    );
    pair.compare();
    match actual {
        Ok(b) => {
            reference_test::assert_debug(
                "song/tests/rust_song_activation.rs::compare_actor_command::eq2",
                &(b),
            );
            Ok(b)
        }
        Err(b) => Err((
            String::from_utf8(reference_test::source_bytes(
                "song/tests/rust_song_activation.rs::compare_actor_command::error",
                b"",
            ))
            .unwrap(),
            b,
        )),
    }
}
fn compare_actor_messages(
    pair: &mut Pair,
    key: &str,
) -> deadsync_song_lua::SongLuaCapturedMessageCommands {
    let b = saved(pair, key);

    let actual = pair
        .rust
        .capture_song_actor_message_commands(Value::Table(b))
        .unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_actor_messages::eq1",
        &(actual.commands),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_actor_messages::eq2",
        &(actual.sounds),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_actor_messages::eq3",
        &(actual.skipped.len()),
    );
    pair.compare();
    actual
}

#[test]
fn actor_messages_command_defaults_and_missing_handlers_match_native() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        return Def.Quad{InitCommand=function(self)saved.a=self;self:x(3)end,
            JudgmentMessageCommand=function(self,p)assert(p.Player=='PlayerNumber_P1' and p.Notes[1]:GetTapNoteResult():GetTapNoteScore()=='TapNoteScore_W3');self:x(1)end,
            EarlyHitMessageCommand=function(self,p)assert(p.Early and p.TapNoteOffset<0);self:x(2)end,
            LifeChangedMessageCommand=function(self,p)assert(p.LifeMeter:GetLife()==.5);self:x(3)end,
            HealthStateChangedMessageCommand=function(self,p)assert(p.HealthState=='HealthState_Alive' and p.PlayerNumber=='PlayerNumber_P1');self:x(4)end,
            ExCountsChangedMessageCommand=function(self,p)assert(p.ExCounts.W0==0 and p.ActualPossible==1);self:x(5)end,
            PlayerOptionsChangedMessageCommand=function(self,p)assert(p.PlayerNumber=='PlayerNumber_P1');self:x(6)end,
            OtherMessageCommand=function(...)assert(select('#',...)==1);local self=...;self:x(7)end,
            DirectCommand=function(...)assert(select('#',...)==1);local self=...;self:x(8)end}
    "#,
    );
    for (i, name) in [
        "JudgmentMessageCommand",
        "EarlyHitMessageCommand",
        "LifeChangedMessageCommand",
        "HealthStateChangedMessageCommand",
        "ExCountsChangedMessageCommand",
        "PlayerOptionsChangedMessageCommand",
        "OtherMessageCommand",
        "DirectCommand",
    ]
    .into_iter()
    .enumerate()
    {
        let blocks = compare_actor_command(&mut pair, "a", name).unwrap();
        assert_eq!(blocks[0].delta.x, Some((i + 1) as f32));
        pair.script("assert(saved.a:GetX()==3)");
    }
    assert!(
        compare_actor_command(&mut pair, "a", "MissingMessageCommand")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn actor_messages_command_preserves_prior_capture_history_queues_and_mutable_alias_boundary() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        return Def.Quad{InitCommand=function(self)saved.a=self end,
            OnCommand=function(self)self:linear(1):x(40)end,
            ProbeMessageCommand=function(self)self.__songlua_capture_custom.value=99;self:z(31)end}
    "#,
    );
    pair.script("saved.a:x(5):sleep(.25):linear(.5):y(10);saved.a.__songlua_capture_custom={value=7};probe_capture_alias=saved.a.__songlua_capture_custom");
    compare_actor_command(&mut pair, "a", "ProbeMessageCommand").unwrap();
    pair.script("saved.a:z(11);assert(saved.a.__songlua_capture_custom.value==7 and probe_capture_alias.value==99)");
    let b = saved(&mut pair, "a");

    let actual = pair.rust.actor_command_capture(Value::Table(b)).unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::actor_messages_command_preserves_prior_capture_history_queues_and_mutable_alias_boundary::eq1",
        &(actual),
    );
    assert!(actual.steps.len() >= 3);
    assert!(actual.blocks.iter().any(|b| b.delta.x == Some(5.)));
    assert!(actual.blocks.iter().any(|b| b.delta.y == Some(10.)));
    assert!(actual.blocks.iter().any(|b| b.delta.z == Some(11.)));
}

#[test]
fn actor_messages_command_errors_restore_reached_actors_locals_rng_and_keep_structural_edits() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        local n=0;local data={value=0};data.self=data
        command_check=function()return n,data.value,data.self==data end
        return Def.ActorFrame{
            Def.Quad{InitCommand=function(self)saved.a=self;self:x(3)end,
                ProbeMessageCommand=function(self)
                    n=n+1;data.value=9;probe_new_scalar=3
                    self:x(90);saved.b:queuecommand('Other');self.created_wrapper=self:AddWrapperState():x(7)
                    math.random();rawset(self,'OtherField',true)
                    if probe_failure then error('actor-command-marker')end
                end},
            Def.Quad{InitCommand=function(self)saved.b=self end,OtherCommand=function(self)self:y(13);rawset(self,'OtherCommand',nil)end}}
    "#,
    );
    pair.script("probe_failure=true");
    let (a, b) = compare_actor_command(&mut pair, "a", "ProbeMessageCommand").unwrap_err();
    assert!(a.contains("actor-command-marker") && b.contains("actor-command-marker"));
    pair.script(r#"
        local n,v,same=command_check();assert(n==0 and v==0 and same)
        assert(probe_new_scalar==nil and saved.a:GetX()==3 and saved.b:GetY()==0)
        assert(saved.a.OtherField and saved.b.OtherCommand==nil and saved.a:GetNumWrapperStates()==0)
        assert(saved.a.created_wrapper:GetX()==0);log(math.random());probe_failure=nil
    "#);
    compare_actor_command(&mut pair, "a", "ProbeMessageCommand").unwrap();
}

#[test]
fn actor_messages_scan_freezes_sorted_names_restores_history_and_extracts_sounds() {
    let (dir, mut pair) = command_scope_pair(
        r#"
        return Def.Quad{InitCommand=function(self)saved.a=self end,
            AlphaMessageCommand=function(self)
                rawset(self,'ZLastMessageCommand',nil)
                rawset(self,'BetaMessageCommand',function(self)self:x(8)end);self:x(1)
            end,
            EchoMessageCommandMessageCommand=function(self)self:x(2)end,
            ToneMessageCommand=function()SOUND:PlayOnce(probe_sound);SOUND:PlayOnce(probe_sound)end,
            ZLastMessageCommand=function()error('removed handler ran')end,
            IgnoredMessageCommand=false}
    "#,
    );
    let path = dir.path().join("message.wav");
    fs::write(&path, b"captured path").unwrap();
    pair.script(&format!(
        "probe_sound={:?};saved.a.HostMessageCommand=SOUND.DimMusic",
        deadsync_song_lua::file_path_string(&path)
    ));
    let b = saved(&mut pair, "a");

    assert!(test_support::rust_seed_message_sound_block(
        &mut pair.rust,
        Value::Table(b),
        false
    ));
    pair.script("saved.a:linear(1)");

    assert!(test_support::rust_seed_message_sound_block(
        &mut pair.rust,
        Value::Table(b),
        true
    ));
    let output = compare_actor_messages(&mut pair, "a");
    assert!(output.skipped.is_empty());
    assert_eq!(
        output
            .commands
            .iter()
            .map(|c| c.message.as_str())
            .collect::<Vec<_>>(),
        ["Alpha", "Echo", deadsync_song_lua::SONG_LUA_STARTUP_MESSAGE]
    );
    assert!(
        output
            .commands
            .last()
            .unwrap()
            .blocks
            .iter()
            .all(|b| b.delta.sound_play == Some(true))
    );
    assert_eq!(
        output.sounds,
        [("Tone".into(), path.clone()), ("Tone".into(), path)]
    );
    let history = pair.rust.actor_command_capture(Value::Table(b)).unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::actor_messages_scan_freezes_sorted_names_restores_history_and_extracts_sounds::eq3",
        &(history),
    );
    assert!(
        history
            .blocks
            .iter()
            .any(|b| b.delta.sound_play == Some(false))
    );
    pair.script(
        "assert(saved.a.ZLastMessageCommand==nil and type(saved.a.BetaMessageCommand)=='function')",
    );
}

#[test]
fn actor_messages_failures_defer_once_and_keep_actor_roots_through_retirement() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let context = SongLuaCompileContext::new(dir.path(), "deferred actor lifetime");
    let source = "return Def.Quad{Name='deferred-source',FailMessageCommand=function(self)self:x(91);error(probe_failure_text)end}";

    let mut rust = SongRuntime::new(&context).unwrap();
    rust.begin_song_compilation().unwrap();
    rust.begin_actor_preparation().unwrap();
    let declaration = rust
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/actors.lua").unwrap())
        .unwrap()[0];
    let scene = rust.instantiate_actor(declaration).unwrap();
    let root = rust.actor_scene_root(scene).unwrap();
    rust.vm_mut().set_global(b"root", root).unwrap();
    let mut pair = Pair {
        rust,
        scene,
        context,
    };
    pair.script("events={};saved={a=root};function log(...)local t={};for i=1,select('#',...)do t[i]=tostring(select(i,...))end;events[#events+1]=table.concat(t,',')end");
    pair.script("probe_failure_text='first-marker'");
    let first = compare_actor_messages(&mut pair, "a");
    assert!(
        first.skipped[0].contains("first-marker")
            && first.skipped[0].contains("FailMessageCommand")
    );
    pair.script("probe_failure_text='second-marker'");
    let second = compare_actor_messages(&mut pair, "a");
    assert_eq!(first.skipped, second.skipped);

    let rust = test_support::rust_deferred_actor_messages(&pair.rust);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::actor_messages_failures_defer_once_and_keep_actor_roots_through_retirement::eq2",
        &(1),
    );
    assert_eq!(rust.len(), 1);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::actor_messages_failures_defer_once_and_keep_actor_roots_through_retirement::eq4",
        &(rust[0].1),
    );
    assert!(!rust[0].3);
    pair.script("message_weak=setmetatable({saved.a},{__mode='v'});saved.a=nil;root=nil");
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    rust_command_script(
        &mut pair.rust,
        "collectgarbage();collectgarbage();assert(message_weak[1]);return ''",
    )
    .unwrap();
    test_support::rust_clear_deferred_actor_messages(&mut pair.rust).unwrap();
    rust_command_script(
        &mut pair.rust,
        "collectgarbage();collectgarbage();assert(message_weak[1]==nil);return ''",
    )
    .unwrap();
}

#[test]
fn actor_messages_key_errors_preserve_native_partial_state_and_allow_recovery() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        return Def.Quad{InitCommand=function(self)saved.a=self;self:x(3)end,
            ProbeMessageCommand=function(self)self:x(91);probe_new_scalar=7 end}
    "#,
    );
    pair.script("rawset(saved.a,string.char(255),{})");
    assert!(
        compare_actor_command(&mut pair, "a", "MissingMessageCommand")
            .unwrap()
            .is_empty()
    );
    compare_actor_command(&mut pair, "a", "ProbeMessageCommand").unwrap_err();
    pair.script("rawset(saved.a,string.char(255),nil);rawset(_G,string.char(255),{})");
    compare_actor_command(&mut pair, "a", "ProbeMessageCommand").unwrap_err();
    pair.script("assert(saved.a:GetX()==3 and probe_new_scalar==7);rawset(_G,string.char(255),nil);probe_new_scalar=nil");
    compare_actor_command(&mut pair, "a", "ProbeMessageCommand").unwrap();
}

#[test]
fn actor_messages_discard_command_returns_before_reentrant_mutable_restore_gc() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        return Def.BitmapText{Font='Common Normal',Text='original',InitCommand=function(self)saved.a=self end,
            ProbeMessageCommand=function(self)
                self:settext('temporary'):x(9)
                local a={marker='first'};local b={marker='extra'};local weak=self.message_weak
                weak[1]=a;weak[2]=b;return a,b
            end}
    "#,
    );
    pair.script(r#"
        local a=saved.a;local mt=getmetatable(a);local restored=0
        a.message_weak=setmetatable({}, {__mode='v'});probe_restored=function()return restored end
        setmetatable(a,{__index=mt.__index,__newindex=function(t,k,v)
            if k=='Text' then collectgarbage();collectgarbage();assert(a.message_weak[1]==nil and a.message_weak[2]==nil);restored=restored+1 end
            rawset(t,k,v)
        end})
    "#);
    compare_actor_command(&mut pair, "a", "ProbeMessageCommand").unwrap();
    pair.script("assert(saved.a:GetText()=='original' and probe_restored()==2)");
}

#[test]
fn actor_messages_instruction_limits_restore_history_and_live_mode_skips_message_probes() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        return Def.Quad{Name='message-limit',InitCommand=function(self)saved.a=self;self:x(3)end,
            ProbeMessageCommand=function(self)self:x(99);probe_new_scalar=7;while true do end end}
    "#,
    );
    let b = saved(&mut pair, "a");
    let before = pair.rust.actor_command_capture(Value::Table(b)).unwrap();
    pair.rust.vm_mut().set_instruction_limit(Some(50_000));
    let error = pair
        .rust
        .capture_song_actor_command(Value::Table(b), "ProbeMessageCommand")
        .unwrap_err();
    pair.rust.vm_mut().set_instruction_limit(None);
    assert!(error.contains("limit"));
    assert_eq!(
        pair.rust.actor_command_capture(Value::Table(b)).unwrap(),
        before
    );
    pair.script("assert(probe_new_scalar==nil and saved.a:GetX()==3);saved.a.ProbeMessageCommand=function(self)self:x(12)end");
    compare_actor_command(&mut pair, "a", "ProbeMessageCommand").unwrap();

    pair.rust.set_live(true);

    assert_eq!(
        pair.rust
            .capture_song_actor_message_commands(Value::Table(b))
            .unwrap(),
        Default::default()
    );
}

type MessageRows = Vec<Vec<deadsync_song_lua::SongLuaOverlayMessageCommand>>;
fn compare_stable_message_pass(pair: &mut Pair, keys: &[&str]) -> (MessageRows, Vec<String>) {
    let mut b = Vec::new();
    for key in keys {
        let y = saved(pair, key);
        b.push(Value::Table(y));
    }
    let mut actual = vec![Vec::new(); keys.len()];
    let mut rb = Vec::new();

    let ar = pair.rust.capture_song_stable_cross_actor_message_commands(
        &mut b
            .into_iter()
            .zip(actual.iter_mut())
            .map(|(actor, commands)| deadsync_song_lua::SongMessageTarget { actor, commands })
            .collect::<Vec<_>>(),
        |d| rb.push(d),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_stable_message_pass::eq1",
        &(ar.is_err()),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_stable_message_pass::eq2",
        &(actual),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_stable_message_pass::eq3",
        &(rb.len()),
    );
    pair.compare();
    (actual, rb)
}
struct DeferredPass {
    overlays: MessageRows,
    tracked: MessageRows,
    sounds: Vec<(usize, String, std::path::PathBuf)>,
    skipped: Vec<String>,
    failed: bool,
}
fn compare_deferred_message_pass(
    pair: &mut Pair,
    keys: &[&str],
    tracked: &[&str],
    runtime: &[deadsync_song_lua::SongLuaStatefulMessageCapture],
) -> DeferredPass {
    let mut rust_skipped = test_support::rust_deferred_actor_messages(&pair.rust)
        .into_iter()
        .map(|r| r.2)
        .collect::<Vec<_>>();
    rust_skipped.push("unrelated detail".into());

    let mut rust = Vec::new();
    for key in keys.iter().chain(tracked) {
        let b = saved(pair, key);
        rust.push(Value::Table(b));
    }
    let mut actual = vec![Vec::new(); rust.len()];

    let mut rust_targets = rust
        .into_iter()
        .zip(actual.iter_mut())
        .map(|(actor, commands)| deadsync_song_lua::SongMessageTarget { actor, commands })
        .collect::<Vec<_>>();

    let (a, b) = rust_targets.split_at_mut(keys.len());
    let ar = pair
        .rust
        .capture_song_deferred_messages(a, b, runtime, &mut rust_skipped);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_deferred_message_pass::eq1",
        &(ar.is_err()),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_deferred_message_pass::eq2",
        &(actual),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_deferred_message_pass::eq3",
        &(rust_skipped.len()),
    );
    let failed = ar.is_err();
    let sounds = ar.unwrap_or_default();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_deferred_message_pass::eq4",
        &(sounds),
    );
    assert!(test_support::rust_deferred_actor_messages(&pair.rust).is_empty());

    pair.compare();
    let tracked = actual.split_off(keys.len());
    DeferredPass {
        overlays: actual,
        tracked,
        sounds,
        skipped: rust_skipped,
        failed,
    }
}

#[test]
fn message_retry_stable_cross_actor_scope_freezes_handlers_and_preserves_alias_indices() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        local n=0;peek_message_locals=function()return n end
        return Def.ActorFrame{
            Def.Quad{InitCommand=function(self)saved.a=self end,PulseMessageCommand=function(self)
                n=n+1;self:x(n);saved.b:y(7):linear(.25):z(8)
                self.PulseMessageCommand=nil;saved.b.OtherMessageCommand=nil;collectgarbage()
            end},
            Def.Quad{InitCommand=function(self)saved.b=self end,OtherMessageCommand=function()saved.c:x(19);collectgarbage()end},
            Def.Quad{InitCommand=function(self)saved.c=self end}}
    "#,
    );
    let (rows, dynamic) = compare_stable_message_pass(&mut pair, &["a", "b", "b", "c"]);
    assert!(dynamic.is_empty());
    assert!(rows[0].is_empty());
    assert_eq!(rows[1].len(), 1);
    assert_eq!(rows[2].len(), 1);
    assert_eq!(rows[3].len(), 2);
    pair.script("assert(peek_message_locals()==0);assert(saved.a.PulseMessageCommand==nil and saved.b.OtherMessageCommand==nil);assert(saved.a:GetX()==0 and saved.b:GetY()==0 and saved.c:GetX()==0)");
}

#[test]
fn message_retry_unstable_targets_and_second_run_errors_defer_with_original_details() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        local n=0;local m=0;peek_message_locals=function()return n,m end
        return Def.ActorFrame{
            Def.Quad{InitCommand=function(self)saved.a=self end,FlipMessageCommand=function()
                n=n+1;if n%2==1 then saved.c:x(5)else saved.d:x(5)end
            end},
            Def.Quad{InitCommand=function(self)saved.b=self end,FailMessageCommand=function()
                m=m+1;if m==2 then error('second attempt')end;saved.c:y(6)
            end},
            Def.Quad{InitCommand=function(self)saved.c=self end},Def.Quad{InitCommand=function(self)saved.d=self end}}
    "#,
    );
    let (rows, dynamic) = compare_stable_message_pass(&mut pair, &["a", "b", "c", "d"]);
    assert!(rows.iter().all(Vec::is_empty));
    assert_eq!(dynamic.len(), 2);
    assert!(
        dynamic
            .iter()
            .any(|d| d.contains("changes cross-actor targets or effects"))
    );
    assert!(dynamic.iter().any(
        |d| d.contains("cannot be replayed for stable cross-actor capture")
            && d.contains("second attempt")
    ));
    pair.script("local n,m=peek_message_locals();assert(n==0 and m==0)");
    let first = test_support::rust_deferred_actor_messages(&pair.rust);
    compare_stable_message_pass(&mut pair, &["a", "b", "c", "d"]);
    assert_eq!(
        test_support::rust_deferred_actor_messages(&pair.rust),
        first
    );
    let pass = compare_deferred_message_pass(&mut pair, &["a", "b", "c", "d"], &[], &[]);
    assert!(!pass.failed);
    assert_eq!(pass.skipped.len(), 3);
    assert!(pass.overlays.iter().all(Vec::is_empty));
}

#[test]
fn message_retry_deferred_success_captures_source_cross_tracked_and_repeated_sounds() {
    let (dir, mut pair) = command_scope_pair(
        r#"
        local n=0;peek_message_locals=function()return n end
        return Def.ActorFrame{
            Def.Quad{InitCommand=function(self)saved.a=self end,LaterMessageCommand=function(self)
                assert(ready,'not ready');n=n+1;self:x(9);saved.b:y(10);saved.c:z(11)
                SOUND:PlayOnce(probe_sound);SOUND:PlayOnce(probe_sound);collectgarbage()
            end},
            Def.Quad{InitCommand=function(self)saved.b=self end},Def.Quad{InitCommand=function(self)saved.c=self end}}
    "#,
    );
    assert_eq!(compare_actor_messages(&mut pair, "a").skipped.len(), 1);
    let path = dir.path().join("retry.wav");
    fs::write(&path, b"existing sound path").unwrap();
    pair.script(&format!(
        "ready=true;probe_sound={:?}",
        deadsync_song_lua::file_path_string(&path)
    ));
    pair.rust.set_beat(7.).unwrap();

    let pass = compare_deferred_message_pass(&mut pair, &["a", "b"], &["c"], &[]);
    assert!(!pass.failed);
    assert_eq!(pass.skipped, ["unrelated detail"]);
    assert_eq!(pass.sounds.len(), 2);
    assert_eq!(pass.sounds[0], (0, "Later".into(), path));
    assert_eq!(pass.sounds[0], pass.sounds[1]);
    assert_eq!(
        pass.overlays.iter().map(Vec::len).collect::<Vec<_>>(),
        [1, 1]
    );
    assert_eq!(pass.tracked[0].len(), 1);
    pair.script("assert(peek_message_locals()==0);assert(saved.a:GetX()==0 and saved.b:GetY()==0 and saved.c:GetZ()==0)");
}

#[test]
fn message_retry_deferred_rejects_unrepresentable_sound_music_broadcast_and_state_changes() {
    for body in [
        "SOUND:PlayOnce('missing-retry.wav')",
        "SOUND:PlayMusicPart('missing-retry.wav')",
        "MESSAGEMAN:Broadcast('Unsubscribed')",
        "n=n+1;self:x(n)",
    ] {
        let source = format!(
            r#"local n=0;return Def.Quad{{InitCommand=function(self)saved.a=self end,LaterMessageCommand=function(self)assert(ready,'not ready');{body} end}}"#
        );
        let (_dir, mut pair) = command_scope_pair(&source);
        assert_eq!(compare_actor_messages(&mut pair, "a").skipped.len(), 1);
        pair.script("ready=true");
        let pass = compare_deferred_message_pass(&mut pair, &["a"], &[], &[]);
        assert!(!pass.failed);
        assert!(pass.sounds.is_empty());
        assert!(pass.overlays[0].is_empty());
        assert_eq!(pass.skipped.len(), 2, "{body}");
    }
}

#[test]
fn message_retry_runtime_capture_coverage_and_missing_overlay_consume_without_retry() {
    for covered in [false, true] {
        let (_dir, mut pair) = command_scope_pair(
            r#"return Def.Quad{InitCommand=function(self)saved.a=self end,LaterMessageCommand=function()error('must remain deferred')end}"#,
        );
        compare_actor_messages(&mut pair, "a");
        let captures = vec![deadsync_song_lua::SongLuaStatefulMessageCapture {
            message: "Later".into(),
            overlay_targets: vec![(0, Vec::new())],
            writes: Vec::new(),
        }];
        let keys = if covered { vec!["a"] } else { Vec::new() };
        let pass = compare_deferred_message_pass(&mut pair, &keys, &[], &captures);
        assert!(!pass.failed);
        assert!(pass.overlays.iter().all(Vec::is_empty));
        assert_eq!(pass.skipped.len(), if covered { 1 } else { 2 });
    }
}

#[test]
fn message_retry_removed_handler_preserves_prior_publication_and_consumes_records_on_error() {
    let (_dir, mut pair) = command_scope_pair(
        r#"return Def.ActorFrame{
        Def.Quad{InitCommand=function(self)saved.a=self end,LaterMessageCommand=function(self)assert(ready,'not ready');self:x(9)end},
        Def.Quad{InitCommand=function(self)saved.b=self end,LaterMessageCommand=function(self)assert(ready,'not ready');self:y(10)end}}
    "#,
    );
    compare_actor_messages(&mut pair, "a");
    compare_actor_messages(&mut pair, "b");
    pair.script("ready=true;saved.b.LaterMessageCommand=nil");
    let pass = compare_deferred_message_pass(&mut pair, &["a", "b"], &[], &[]);
    assert!(pass.failed);
    assert_eq!(pass.overlays[0].len(), 1);
    assert!(pass.overlays[1].is_empty());
    assert_eq!(pass.skipped.len(), 2);
    let again = compare_deferred_message_pass(&mut pair, &["a", "b"], &[], &[]);
    assert!(!again.failed);
    assert!(again.overlays.iter().all(Vec::is_empty));
}

#[test]
fn message_retry_successful_chronological_delivery_marks_observed_even_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(
        dir.path(),
        "default.lua",
        r#"return Def.Quad{InitCommand=function(self)saved.a=self end,LaterMessageCommand=function()assert(ready,'not ready');deliveries=(deliveries or 0)+1 end}"#,
    );
    let mut pair = Pair::new(SongLuaCompileContext::new(
        dir.path(),
        "observed deferred delivery",
    ));
    let rust = pair.activate(&[&path]);

    pair.rust
        .discover_song_startup_commands(pair.scene, &rust)
        .unwrap();
    compare_actor_messages(&mut pair, "a");
    pair.script("ready=true;MESSAGEMAN:Broadcast('Later')");

    assert!(!test_support::rust_deferred_actor_messages(&pair.rust)[0].3);
    let b = saved(&mut pair, "a");

    pair.rust
        .begin_song_replay(pair.scene, &[(b, Default::default())], &rust)
        .unwrap();

    assert!(
        pair.rust
            .set_song_replay_frames(pair.scene, &[(0., 0.)], None)
    );
    pair.script("ready=false;pcall(function()MESSAGEMAN:Broadcast('Later')end)");

    assert!(!test_support::rust_deferred_actor_messages(&pair.rust)[0].3);
    pair.script("ready=true;MESSAGEMAN:Broadcast('Later');ready=false");

    assert!(test_support::rust_deferred_actor_messages(&pair.rust)[0].3);
    let pass = compare_deferred_message_pass(&mut pair, &["a"], &[], &[]);
    assert!(!pass.failed);
    assert!(pass.overlays[0].is_empty());
    assert_eq!(pass.skipped, ["unrelated detail"]);
}

#[test]
fn message_retry_consumption_releases_retired_deferred_actor_roots() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let context = SongLuaCompileContext::new(dir.path(), "consumed deferred roots");
    let mut runtime = SongRuntime::new(&context).unwrap();
    runtime.begin_song_compilation().unwrap();
    runtime.begin_actor_preparation().unwrap();
    let declaration = runtime
        .vm_mut()
        .execute(
            &compile_named(
                b"return Def.Quad{LaterMessageCommand=function()error('deferred')end}",
                b"=deferred lifetime",
            )
            .unwrap(),
        )
        .unwrap()[0];
    let scene = runtime.instantiate_actor(declaration).unwrap();
    let actor = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"root", actor).unwrap();
    assert_eq!(
        runtime
            .capture_song_actor_message_commands(actor)
            .unwrap()
            .skipped
            .len(),
        1
    );
    rust_command_script(
        &mut runtime,
        "message_weak=setmetatable({root},{__mode='v'});root=nil;return ''",
    )
    .unwrap();
    runtime.retire_actor_scene(scene).unwrap();
    rust_command_script(
        &mut runtime,
        "collectgarbage();collectgarbage();assert(message_weak[1]);return ''",
    )
    .unwrap();
    let mut skipped = test_support::rust_deferred_actor_messages(&runtime)
        .into_iter()
        .map(|r| r.2)
        .collect();
    assert!(
        runtime
            .capture_song_deferred_messages(&mut [], &mut [], &[], &mut skipped)
            .unwrap()
            .is_empty()
    );
    rust_command_script(
        &mut runtime,
        "collectgarbage();collectgarbage();assert(message_weak[1]==nil);return ''",
    )
    .unwrap();
}

fn overlay_ease_params() -> deadsync_song_lua::SongLuaOverlayEaseBuildParams {
    deadsync_song_lua::SongLuaOverlayEaseBuildParams {
        unit: deadsync_song_lua::SongLuaTimeUnit::Second,
        start: 3.,
        limit: 2.,
        span_mode: deadsync_song_lua::SongLuaSpanMode::Len,
        easing: Some("smooth".into()),
        sustain: Some(0.25),
        opt1: Some(0.4),
        opt2: Some(0.6),
    }
}
fn compare_overlay_ease(
    pair: &mut Pair,
    overlays: &[(usize, &str)],
    probes: &[&str],
    params: deadsync_song_lua::SongLuaOverlayEaseBuildParams,
) -> Result<Vec<deadsync_song_lua::SongLuaOverlayEase>, (String, String)> {
    let (mut b, mut y) = (Vec::new(), Vec::new());
    for &(i, key) in overlays {
        let r = saved(pair, key);
        b.push((i, Value::Table(r)));
    }
    for key in probes {
        let r = saved(pair, key);
        y.push(Value::Table(r));
    }

    let g = pair.rust.vm_mut().get_global(b"ease_probe").unwrap();

    let actual = pair
        .rust
        .capture_song_overlay_function_eases(&b, g, params, [2., 6.], &y);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_overlay_ease::eq1",
        &(actual.is_err()),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_overlay_ease::eq2",
        &(pair.rust.values().unwrap()),
    );
    pair.compare();
    match actual {
        Ok(b) => {
            reference_test::assert_debug(
                "song/tests/rust_song_activation.rs::compare_overlay_ease::eq3",
                &(b),
            );
            Ok(b)
        }
        Err(b) => Err((
            String::from_utf8(reference_test::source_bytes(
                "song/tests/rust_song_activation.rs::compare_overlay_ease::error",
                b"",
            ))
            .unwrap(),
            b,
        )),
    }
}
#[test]
fn overlay_compiler_ease_endpoints_keep_intersection_metadata_and_clamped_clocks() {
    let (_dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self end}");
    pair.script("ease_probe=function(v)local beat=GAMESTATE:GetSongBeat();saved.a:x(v+beat):diffusealpha(v/10);if v==2 then saved.a:y(8)end end");
    for mode in [
        deadsync_song_lua::SongLuaSpanMode::Len,
        deadsync_song_lua::SongLuaSpanMode::End,
    ] {
        let mut p = overlay_ease_params();
        p.span_mode = mode;
        p.limit = -2.;
        let out = compare_overlay_ease(&mut pair, &[(0, "a")], &["a"], p).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].from.x, Some(5.));
        assert_eq!(out[0].to.x, Some(9.));
        assert_eq!(out[0].from.y, None);
        assert_eq!(out[0].sustain, Some(0.25));
    }
    pair.script("assert(saved.a:GetX()==0 and saved.a:GetY()==0)");
}
#[test]
fn overlay_compiler_ease_known_and_unknown_probes_keep_native_rewind_boundaries() {
    for indexed in [false, true] {
        let (_dir, mut pair) = command_scope_pair(
            "return Def.ActorFrame{Def.Quad{InitCommand=function(self)saved.a=self end},Def.Quad{InitCommand=function(self)saved.b=self end},Def.Quad{InitCommand=function(self)saved.other=self end}}",
        );
        pair.script("local n=0;ease_peek=function()return n end;ease_probe=function(v)n=n+1;saved.a:x(v);saved.b:y(n)end");
        let out = compare_overlay_ease(
            &mut pair,
            &[(0, "a"), (1, "b")],
            if indexed { &["a"] } else { &["other"] },
            overlay_ease_params(),
        )
        .unwrap();
        assert_eq!(out.len(), if indexed { 1 } else { 2 });
        pair.script(if indexed {
            "assert(ease_peek()==2 and saved.a:GetX()==0 and saved.b:GetY()==2)"
        } else {
            "assert(ease_peek()==2 and saved.a:GetX()==0 and saved.b:GetY()==0)"
        });
    }
}
#[test]
fn overlay_compiler_ease_duplicate_indices_empty_output_and_error_recovery_match_native() {
    let (_dir, mut pair) = command_scope_pair(
        "return Def.ActorFrame{Def.Quad{InitCommand=function(self)saved.a=self end},Def.Quad{InitCommand=function(self)saved.b=self end}}",
    );
    pair.script("ease_probe=function(v)saved.a:x(v);saved.b:x(v*10)end");
    let rows = compare_overlay_ease(
        &mut pair,
        &[(0, "a"), (0, "b"), (8, "a")],
        &[],
        overlay_ease_params(),
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].from.x, Some(20.));
    pair.script("calls=0;ease_probe=function(v)calls=calls+1 end");
    assert!(
        compare_overlay_ease(&mut pair, &[(0, "a")], &[], overlay_ease_params())
            .unwrap()
            .is_empty()
    );
    pair.script("assert(calls==2)");
    for fail in [2, 6] {
        pair.script(&format!("calls=0;ease_probe=function(v)calls=calls+1;saved.a:x(99);if v=={fail} then error('endpoint failure')end end"));
        assert!(
            compare_overlay_ease(&mut pair, &[(0, "a")], &["a"], overlay_ease_params()).is_err()
        );
        pair.script(&format!(
            "assert(calls=={} and saved.a:GetX()==0)",
            if fail == 2 { 1 } else { 2 }
        ));
    }
    pair.script("ease_probe=function(v)saved.a:x(v)end");
    assert_eq!(
        compare_overlay_ease(&mut pair, &[(0, "a")], &[], overlay_ease_params())
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn overlay_compiler_ease_retains_detached_function_through_both_endpoints_and_releases_it() {
    let (_dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self end}");
    pair.script("local retained={v=7};ease_probe=function(v)assert(retained.v==7);ease_probe=nil;collectgarbage();saved.a:x(v)end;ease_weak=setmetatable({ease_probe,retained},{__mode='v'})");
    assert_eq!(
        compare_overlay_ease(&mut pair, &[(0, "a")], &[], overlay_ease_params())
            .unwrap()
            .len(),
        1
    );
    pair.script(
        "collectgarbage();collectgarbage();assert(ease_weak[1]==nil and ease_weak[2]==nil)",
    );
}

fn standalone_overlay_pair(source: &str) -> (tempfile::TempDir, Pair) {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let context = SongLuaCompileContext::new(dir.path(), "overlay compiler tree");

    let mut rust = SongRuntime::new(&context).unwrap();
    rust.begin_song_compilation().unwrap();
    rust.begin_actor_preparation().unwrap();
    let declaration = rust
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"=overlay tree").unwrap())
        .unwrap()[0];
    let scene = rust.instantiate_actor(declaration).unwrap();
    let root = rust.actor_scene_root(scene).unwrap();
    rust.vm_mut().set_global(b"root", root).unwrap();
    let mut pair = Pair {
        rust,
        scene,
        context,
    };
    pair.script("prepared_root=nil;__songlua_top_screen=nil;events={};saved={root=root};function log(...)local t={};for i=1,select('#',...)do t[i]=tostring(select(i,...))end;events[#events+1]=table.concat(t,',')end");
    (dir, pair)
}
fn compare_overlay_tree(
    pair: &mut Pair,
    hide_root: bool,
) -> Result<deadsync_song_lua::SongOverlayActors<OutputKind>, (String, String)> {
    let b = saved(pair, "root");
    if hide_root {
        pair.script("saved.root=nil;root=nil");
    }
    let context = pair.context.clone();
    let mut rs = Vec::new();

    let actual = pair.rust.read_song_overlay_actors(
        Value::Table(b),
        &context,
        |_, _, _| Ok(std::sync::Arc::from([7u32, 0u32])),
        output_kind_layer,
        |d| rs.push(d),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_overlay_tree::eq1",
        &(actual.is_err()),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_overlay_tree::eq2",
        &(rs.len()),
    );
    pair.compare();
    match actual {
        Ok(b) => {
            reference_test::assert_debug(
                "song/tests/rust_song_activation.rs::compare_overlay_tree::eq3",
                &(b.actors.len()),
            );
            for b in &b.actors {
                reference_test::assert_debug(
                    "song/tests/rust_song_activation.rs::compare_overlay_tree::eq4",
                    &(activation_snapshot::text(format!("{:?}", b.actor))),
                );
                reference_test::assert_debug(
                    "song/tests/rust_song_activation.rs::compare_overlay_tree::eq5",
                    &(b.message_sounds),
                );
            }
            Ok(b)
        }
        Err(b) => Err((
            String::from_utf8(reference_test::source_bytes(
                "song/tests/rust_song_activation.rs::compare_overlay_tree::error",
                b"",
            ))
            .unwrap(),
            b,
        )),
    }
}
#[test]
fn overlay_compiler_tree_keeps_wrapper_order_parents_and_excludes_wrapper_children() {
    let (_dir, mut pair) = standalone_overlay_pair(
        "return Def.ActorFrame{Name='root',Def.Quad{Name='body'},Def.ActorFrame{Name='nested',Def.Quad{Name='leaf'}}}",
    );
    pair.script("local a=root[1];local inner=a:AddWrapperState():x(1);inner.Name='inner';local outer=a:AddWrapperState():y(2);outer.Name='outer';local hidden=Def.Quad{Name='wrapper-child'};a:GetWrapperState(1)[1]=hidden");
    let mut rows = compare_overlay_tree(&mut pair, false).unwrap();
    assert_eq!(
        rows.actors
            .iter()
            .filter_map(|r| r.actor.name.as_deref())
            .collect::<Vec<_>>(),
        ["root", "outer", "inner", "body", "nested", "leaf"]
    );
    assert_eq!(rows.actors[3].actor.parent_index, Some(2));
    pair.rust.release_song_overlay_actors(&mut rows).unwrap();
}
#[test]
fn overlay_compiler_tree_plain_elision_uses_only_two_global_table_levels_and_values() {
    let (_dir, mut pair) = standalone_overlay_pair(
        "return Def.ActorFrame{Def.Actor{},Def.Actor{},Def.Actor{},Def.Actor{},Def.Actor{},Def.ActorFrame{Def.Quad{Name='child'}}}",
    );
    pair.script("direct=root[1];one={value=root[2]};deep={one={value=root[3]}};key_only={[root[4]]=true};root[5]:x(4);root[6]:x(1);root[6][1]:y(2)");
    let mut rows = compare_overlay_tree(&mut pair, true).unwrap();
    assert_eq!(rows.actors.len(), 5);
    assert_eq!(rows.actors[0].actor.name, None);
    assert_eq!(rows.actors[2].actor.initial_state.x, 4.);
    assert_eq!(rows.actors[3].actor.parent_index, None);
    assert_eq!(rows.actors[4].actor.parent_index, Some(3));
    pair.rust.release_song_overlay_actors(&mut rows).unwrap();
}
#[test]
fn overlay_compiler_tree_forward_aft_names_and_generic_proxy_targets_remap_after_elision() {
    use deadsync_song_lua::{SongLuaOverlayKind as K, SongLuaProxyTarget as P};
    let (_dir, mut pair) = standalone_overlay_pair(
        "return Def.ActorFrame{Name='root',Def.Actor{},Def.Sprite{Name='capture',Texture='tree-capture'},Def.ActorFrameTexture{Name='aft'},Def.Quad{Name='target'},Def.ActorProxy{Name='proxy'},Def.ActorProxy{Name='missing'}}",
    );
    pair.script("root[3]:SetTextureName('tree-capture'):setsize(8,9):EnableAlphaBuffer(true):Create();root[5]:SetTarget(root[4]);root[6]:SetTarget(root[1])");
    let mut rows = compare_overlay_tree(&mut pair, false).unwrap();
    assert!(
        matches!(&rows.actors[1].actor.kind,K::AftSprite{capture_name}if capture_name=="tree-capture")
    );
    let target = rows
        .actors
        .iter()
        .position(|r| r.actor.name.as_deref() == Some("target"))
        .unwrap();
    assert!(rows.actors.iter().any(|r|r.actor.name.as_deref()==Some("proxy")&&matches!(r.actor.kind,K::ActorProxy{target:P::Actor{overlay_index}}if overlay_index==target)));
    assert!(
        rows.actors
            .iter()
            .any(|r| r.actor.name.as_deref() == Some("missing")
                && matches!(
                    r.actor.kind,
                    K::ActorProxy {
                        target: P::Actor {
                            overlay_index: usize::MAX
                        }
                    }
                ))
    );
    pair.rust.release_song_overlay_actors(&mut rows).unwrap();
}
#[test]
fn overlay_compiler_tree_messages_keep_preprobe_state_postprobe_name_and_changed_children() {
    let (_dir, mut pair) = standalone_overlay_pair(
        "return Def.ActorFrame{Name='before',RenameMessageCommand=function(self)self.Name='after';self:name('semantic name');self:x(99);self[2]=nil end,Def.Actor{FailMessageCommand=function()error('deferred tree source')end},Def.Quad{Name='removed'}}",
    );
    pair.script("root:x(4)");
    let mut rows = compare_overlay_tree(&mut pair, false).unwrap();
    assert_eq!(rows.actors.len(), 2);
    assert_eq!(rows.actors[0].actor.name.as_deref(), Some("after"));
    assert_eq!(rows.actors[0].actor.initial_state.x, 4.);
    assert_eq!(rows.actors[0].actor.message_commands.len(), 1);
    assert_eq!(
        test_support::rust_deferred_actor_messages(&pair.rust).len(),
        1
    );
    pair.rust.release_song_overlay_actors(&mut rows).unwrap();
}
#[test]
fn overlay_compiler_tree_getter_errors_keep_partial_state_release_roots_and_allow_recovery() {
    let (_dir, mut pair) =
        standalone_overlay_pair("return Def.ActorFrame{Name='root',Def.Quad{Name='child'}}");
    pair.script("root[1].Name=nil;setmetatable(root[1],{__index=function(t,k)if k=='Name'then log('name error');collectgarbage();error('tree name failure')end end})");
    assert!(compare_overlay_tree(&mut pair, false).is_err());
    pair.script("setmetatable(root[1],nil);root[1].Name='recovered'");
    let mut rows = compare_overlay_tree(&mut pair, false).unwrap();
    assert_eq!(rows.actors.len(), 2);
    pair.rust.release_song_overlay_actors(&mut rows).unwrap();
}
#[test]
fn overlay_compiler_tree_batch_roots_survive_retirement_and_reject_foreign_or_released_rows() {
    let (_dir, mut pair) = standalone_overlay_pair("return Def.Quad{Name='owned'}");
    let mut rows = compare_overlay_tree(&mut pair, false).unwrap();
    let copy = rows.actors[0].clone();
    let mut foreign = SongRuntime::new(&pair.context).unwrap();
    assert!(foreign.song_overlay_actor_value(&copy).is_err());
    assert!(foreign.release_song_overlay_actors(&mut rows).is_err());
    pair.script("tree_weak=setmetatable({root},{__mode='v'});root=nil;saved.root=nil");
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    rust_command_script(
        &mut pair.rust,
        "collectgarbage();collectgarbage();assert(tree_weak[1]);return ''",
    )
    .unwrap();
    assert!(matches!(
        pair.rust.song_overlay_actor_value(&copy).unwrap(),
        Value::Table(_)
    ));
    assert!(pair.rust.release_song_overlay_actors(&mut rows).unwrap());
    assert!(!pair.rust.release_song_overlay_actors(&mut rows).unwrap());
    assert!(pair.rust.song_overlay_actor_value(&copy).is_err());
    rust_command_script(
        &mut pair.rust,
        "collectgarbage();collectgarbage();assert(tree_weak[1]==nil);return ''",
    )
    .unwrap();
    assert_eq!(copy.actor.name.as_deref(), Some("owned"));
}

#[test]
fn overlay_compiler_tree_live_mode_keeps_unnamed_default_actors_without_message_probes() {
    let (_dir, mut pair) = standalone_overlay_pair(
        "return Def.Actor{ProbeMessageCommand=function()error('live extraction must skip this')end}",
    );
    let rust = saved(&mut pair, "root");
    pair.context.live = true;
    pair.rust.set_live(true);

    let mut rows = compare_overlay_tree(&mut pair, true).unwrap();
    assert_eq!(rows.actors.len(), 1);
    assert!(rows.actors[0].actor.message_commands.is_empty());
    assert!(test_support::rust_deferred_actor_messages(&pair.rust).is_empty());
    assert!(pair.rust.release_song_overlay_actors(&mut rows).unwrap());

    pair.rust
        .vm_mut()
        .set_global(b"root", Value::Table(rust))
        .unwrap();
    pair.script("saved.root=root;root.ProbeMessageCommand=nil");
    pair.context.live = false;
    pair.rust.set_live(false);

    let mut rows = compare_overlay_tree(&mut pair, true).unwrap();
    assert!(rows.actors.is_empty());
    pair.rust.release_song_overlay_actors(&mut rows).unwrap();
}

#[test]
fn overlay_compiler_tree_detached_transient_child_survives_getter_gc_only_until_batch_release() {
    let (_dir, mut pair) = standalone_overlay_pair("return Def.ActorFrame{Name='root'}");
    pair.script("root[1]=setmetatable({__songlua_actor_type='Quad',__songlua_state_x=12},{__index=function(t,k)if k=='Name'then root[1]=nil;collectgarbage();return 'detached'end end});tree_weak=setmetatable({root[1]},{__mode='v'})");
    let mut rows = compare_overlay_tree(&mut pair, false).unwrap();
    assert_eq!(rows.actors.len(), 2);
    assert_eq!(rows.actors[1].actor.name.as_deref(), Some("detached"));
    assert_eq!(rows.actors[1].actor.initial_state.x, 12.);
    rust_command_script(
        &mut pair.rust,
        "collectgarbage();collectgarbage();assert(tree_weak[1]);return ''",
    )
    .unwrap();
    pair.rust.release_song_overlay_actors(&mut rows).unwrap();
    pair.script("collectgarbage();collectgarbage();assert(tree_weak[1]==nil)");
}
#[test]
fn overlay_compiler_tree_authored_registry_membership_controls_reference_elision() {
    let (_dir, mut pair) = standalone_overlay_pair(
        "return Def.ActorFrame{Name='root',Def.Actor{},Def.Quad{Name='quad'}}",
    );
    pair.script("external=root[1];local a=root[1];registry_keep=function()__songlua_actor_registry={a}end;__songlua_actor_registry={}");
    let mut rows = compare_overlay_tree(&mut pair, false).unwrap();
    assert_eq!(rows.actors.len(), 2);
    pair.rust.release_song_overlay_actors(&mut rows).unwrap();
    pair.script("registry_keep()");
    let mut rows = compare_overlay_tree(&mut pair, false).unwrap();
    assert_eq!(rows.actors.len(), 3);
    pair.rust.release_song_overlay_actors(&mut rows).unwrap();
}

#[derive(Clone, Debug, Default)]
struct ActionPublished {
    counter: usize,
    messages: Vec<deadsync_song_lua::SongLuaMessageEvent>,
    sounds: Vec<deadsync_song_lua::SongLuaSoundEvent>,
    info: deadsync_song_lua::SongLuaCompileInfo,
    overlays: MessageRows,
    tracked: MessageRows,
}
impl ActionPublished {
    fn seed() -> Self {
        Self {
            counter: 7,
            messages: vec![deadsync_song_lua::SongLuaMessageEvent {
                beat: -1.,
                message: "before".into(),
                persists: false,
            }],
            ..Default::default()
        }
    }
    fn output(&mut self) -> deadsync_song_lua::SongActionOutput<'_> {
        deadsync_song_lua::SongActionOutput {
            counter: &mut self.counter,
            messages: &mut self.messages,
            sound_events: &mut self.sounds,
            info: &mut self.info,
        }
    }
}
fn compare_compiled_actions(
    pair: &mut Pair,
    keys: &[&str],
    tracked: &[&str],
    reader: bool,
    mut state: ActionPublished,
) -> (ActionPublished, Result<Option<bool>, (String, String)>) {
    let mut rust = Vec::new();
    for key in keys.iter().chain(tracked) {
        let b = saved(pair, key);
        rust.push(Value::Table(b));
    }
    if state.overlays.is_empty() {
        for r in rust.iter().take(keys.len()) {
            let b = pair.rust.capture_song_actor_message_commands(*r).unwrap();
            reference_test::assert_debug(
                "song/tests/rust_song_activation.rs::compare_compiled_actions::eq1",
                &(b.commands),
            );
            state.overlays.push(b.commands);
        }
    }
    if state.tracked.is_empty() {
        for r in rust.iter().skip(keys.len()) {
            let b = pair.rust.capture_song_actor_message_commands(*r).unwrap();
            reference_test::assert_debug(
                "song/tests/rust_song_activation.rs::compare_compiled_actions::eq2",
                &(b.commands),
            );
            state.tracked.push(b.commands);
        }
    }

    let mut actual = state;

    let mut ar = std::mem::take(&mut actual.overlays);
    ar.extend(std::mem::take(&mut actual.tracked));

    let mut rt = rust
        .into_iter()
        .zip(ar.iter_mut())
        .map(|(actor, commands)| deadsync_song_lua::SongMessageTarget { actor, commands })
        .collect::<Vec<_>>();

    let (a, b) = rt.split_at_mut(keys.len());
    let rr = if reader {
        let table = pair.rust.vm_mut().get_global(b"actions_probe").unwrap();
        pair.rust
            .read_song_overlay_actions(Some(table), a, b, &mut actual.output())
            .map(|()| None)
    } else {
        let function = pair.rust.vm_mut().get_global(b"compile_probe").unwrap();
        pair.rust
            .compile_song_function_action(a, b, function, 4., true, &mut actual.output())
            .map(Some)
    };

    actual.tracked = ar.split_off(keys.len());
    actual.overlays = ar;
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_compiled_actions::eq3",
        &(rr.is_err()),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_compiled_actions::eq4",
        &(activation_snapshot::text(format!("{actual:?}"))),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_compiled_actions::eq5",
        &(pair.rust.values().unwrap()),
    );
    pair.compare();
    let result = match rr {
        Ok(b) => {
            reference_test::assert_debug(
                "song/tests/rust_song_activation.rs::compare_compiled_actions::eq6",
                &(b),
            );
            Ok(b)
        }
        Err(b) => Err((
            String::from_utf8(reference_test::source_bytes(
                "song/tests/rust_song_activation.rs::compare_compiled_actions::error",
                b"",
            ))
            .unwrap(),
            b,
        )),
    };
    (actual, result)
}
#[test]
fn action_compiler_publication_keeps_broadcast_order_direct_tracked_sounds_and_counter() {
    let (dir, mut pair) = command_scope_pair(
        r#"return Def.ActorFrame{
        Def.Quad{InitCommand=function(self)saved.a=self end},
        Def.Quad{InitCommand=function(self)saved.b=self end,FlashMessageCommand=function(self)self:y(5)end},
        Def.Quad{InitCommand=function(self)saved.c=self end}}"#,
    );
    let path = dir.path().join("action.wav");
    fs::write(&path, b"existing sound").unwrap();
    pair.script(&format!(
        "action_sound={:?}",
        deadsync_song_lua::file_path_string(&path)
    ));
    pair.script("compile_probe=function()MESSAGEMAN:Broadcast('Flash');MESSAGEMAN:Broadcast('Missing');MESSAGEMAN:Broadcast('Flash');saved.a:x(9);saved.c:z(10);SOUND:PlayOnce(action_sound);SOUND:PlayOnce(action_sound)end");
    let (out, result) = compare_compiled_actions(
        &mut pair,
        &["a", "b"],
        &["c"],
        false,
        ActionPublished::seed(),
    );
    assert_eq!(result.unwrap(), Some(true));
    assert_eq!(out.counter, 8);
    assert_eq!(
        out.messages
            .iter()
            .map(|m| m.message.as_str())
            .collect::<Vec<_>>(),
        ["before", "Flash", "Flash", "__songlua_overlay_fn_action_7"]
    );
    assert_eq!(out.overlays[0][0].message, "__songlua_overlay_fn_action_7");
    assert_eq!(out.tracked[0][0].message, "__songlua_overlay_fn_action_7");
    assert_eq!(out.overlays[1].len(), 1);
    assert_eq!(out.sounds.len(), 2);
    assert!(out.sounds.iter().all(|s| s.beat == 4.
        && s.path == path
        && s.source_message.is_none()
        && !s.queued_startup));
    pair.script("assert(saved.a:GetX()==0 and saved.b:GetY()==0 and saved.c:GetZ()==0)");
}
#[test]
fn action_compiler_publication_keeps_native_empty_side_effect_listener_and_error_results() {
    for (body, handled, names) in [
        ("return 7", Some(false), 0),
        ("MESSAGEMAN:Broadcast('Flash')", Some(true), 1),
        ("MESSAGEMAN:Broadcast('Missing')", Some(true), 0),
        ("SOUND:StopMusic()", Some(true), 0),
        ("saved.a:x(91);error('action publication failure')", None, 0),
    ] {
        let (_dir, mut pair) = command_scope_pair(
            "return Def.Quad{InitCommand=function(self)saved.a=self end,FlashMessageCommand=function(self)self:y(5)end}",
        );
        pair.script(&format!("compile_probe=function(){body} end"));
        let (out, result) =
            compare_compiled_actions(&mut pair, &["a"], &[], false, ActionPublished::seed());
        if let Some(handled) = handled {
            assert_eq!(result.unwrap(), Some(handled));
        } else {
            assert!(result.is_err());
        }
        assert_eq!(out.counter, 7);
        assert_eq!(out.messages.len(), 1 + names);
        assert!(out.sounds.is_empty());
        assert_eq!(out.info.unsupported_function_actions, 0);
        pair.script("assert(saved.a:GetX()==0 and saved.a:GetY()==0)");
    }
}
#[test]
fn action_compiler_mixed_parameter_broadcasts_preserve_all_or_none_named_publication() {
    for writes in [false, true] {
        let source = format!(
            r#"return Def.ActorFrame{{Def.Quad{{InitCommand=function(self)saved.a=self end,FlashMessageCommand=function(self)self:y(5)end}},Def.Quad{{InitCommand=function(self)saved.b=self end,ParamMessageCommand=function(self,p)assert(p.marker==3);{} end}}}}"#,
            if writes { "self:z(11)" } else { "" }
        );
        let (_dir, mut pair) = command_scope_pair(&source);
        pair.script("compile_probe=function()MESSAGEMAN:Broadcast('Flash');MESSAGEMAN:Broadcast('Param',{marker=3})end");
        let (out, result) =
            compare_compiled_actions(&mut pair, &["a", "b"], &[], false, ActionPublished::seed());
        assert_eq!(result.unwrap(), Some(true));
        assert_eq!(out.messages.len(), if writes { 2 } else { 1 });
        assert!(out.messages.iter().all(|m| m.message != "Flash"));
        assert_eq!(out.counter, if writes { 8 } else { 7 });
    }
}
#[test]
fn action_compiler_reader_uses_raw_fields_native_float_rules_truthiness_and_nil_boundary() {
    let (_dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self end}");
    pair.script(r#"actions_probe=setmetatable({false,{false,'skip'},{1,'one'},{' 2.5 ','two',0},{3,'three',''},{4,'four',false},{'NaN','nan',{}},{'inf','infinite',true},{math.huge,'skip'},{1e300,'skip'},{'0x10','skip'},{string.char(255),'skip'},setmetatable({},{__index=function()error('entry meta must not run')end}),setmetatable({5,'raw',false},{__index=function()error('entry meta must not run')end}),{6,29},{7,{}}},{__index=function()error('sequence meta must not run')end});actions_probe[18]={8,'after-hole'}"#);
    let (out, result) =
        compare_compiled_actions(&mut pair, &["a"], &[], true, ActionPublished::seed());
    result.unwrap();
    assert_eq!(out.counter, 7);
    assert_eq!(
        out.messages
            .iter()
            .map(|m| m.message.as_str())
            .collect::<Vec<_>>(),
        [
            "before", "one", "two", "three", "four", "nan", "infinite", "raw"
        ]
    );
    assert!(out.messages[2].persists && out.messages[3].persists && !out.messages[4].persists);
    assert!(out.messages[5].beat.is_nan());
    assert!(out.messages[6].beat.is_infinite());
    assert_eq!(out.info.unsupported_function_actions, 0);
    pair.script("actions_probe=nil");
    let (next, result) = compare_compiled_actions(&mut pair, &["a"], &[], true, out.clone());
    result.unwrap();
    assert_eq!(format!("{next:?}"), format!("{out:?}"));
}
#[test]
fn action_compiler_reader_continues_failed_or_unhandled_functions_and_deduplicates_details() {
    let (dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self end}");
    let sound = dir.path().join("discarded.wav");
    fs::write(&sound, b"existing").unwrap();
    pair.script(&format!(
        "action_sound={:?}",
        deadsync_song_lua::file_path_string(&sound)
    ));
    pair.script("actions_probe={{1,function()end,true},{1,function()error('body failure')end,true},{1,function()saved.a:x(91);SOUND:PlayOnce(action_sound);error('failed sound')end,true},{2,'after',true},{3,function()saved.a:x(7)end,false}}");
    let (out, result) =
        compare_compiled_actions(&mut pair, &["a"], &[], true, ActionPublished::seed());
    result.unwrap();
    assert_eq!(out.info.unsupported_function_actions, 3);
    assert_eq!(
        out.info.unsupported_function_action_captures,
        ["function action beat=1.000 persists=true"]
    );
    assert_eq!(
        out.messages
            .iter()
            .map(|m| m.message.as_str())
            .collect::<Vec<_>>(),
        ["before", "after", "__songlua_overlay_fn_action_7"]
    );
    assert_eq!(out.counter, 8);
    assert!(out.sounds.is_empty());
    assert!(!out.messages.last().unwrap().persists);
    pair.script("assert(saved.a:GetX()==0)");
}
#[test]
fn action_compiler_reader_hard_string_errors_preserve_prior_publications_and_recover() {
    let (_dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self end}");
    pair.script(
        "actions_probe={{1,function()saved.a:x(4)end},{2,string.char(255)},{3,'unreached'}}",
    );
    let (out, result) =
        compare_compiled_actions(&mut pair, &["a"], &[], true, ActionPublished::seed());
    assert!(result.is_err());
    assert_eq!(out.counter, 8);
    assert_eq!(out.messages.len(), 2);
    assert_eq!(out.overlays[0].len(), 1);
    assert_eq!(out.info.unsupported_function_actions, 0);
    pair.script("actions_probe={{4,'recovered'}}");
    let (out, result) = compare_compiled_actions(&mut pair, &["a"], &[], true, out);
    result.unwrap();
    assert_eq!(out.counter, 8);
    assert_eq!(out.messages.len(), 3);
    assert_eq!(out.messages[2].message, "recovered");
    assert_eq!(out.overlays[0].len(), 1);
}
#[test]
fn action_compiler_reader_keeps_original_table_and_reads_mutated_next_entry_after_cleanup_error_gc()
{
    let (_dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self;self:x(3)end}");
    pair.script(r#"
        local h=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):GetColumnActors()[1]:GetPosHandler()
        actions_probe={{1,function()saved.a:x(91);rawset(h,'__songlua_spline_mode',nil)end},{2,'old-second'},{3,'old-third'}}
        raw_actions={original=actions_probe};action_weak=setmetatable({actions_probe},{__mode='v'})
        setmetatable(h,{__newindex=function(t,k,v)
            if k=='__songlua_spline_mode'then
                raw_actions.original[2]={9,'after-failure',true};raw_actions.original[3]=nil;raw_actions.original=nil
                actions_probe={{20,'replacement-global'}};collectgarbage();error('action column cleanup')
            end
            rawset(t,k,v)
        end})
    "#);
    let (out, result) =
        compare_compiled_actions(&mut pair, &["a"], &[], true, ActionPublished::seed());
    result.unwrap();
    assert_eq!(out.info.unsupported_function_actions, 1);
    assert_eq!(out.counter, 7);
    assert_eq!(
        out.messages
            .iter()
            .map(|m| m.message.as_str())
            .collect::<Vec<_>>(),
        ["before", "after-failure"]
    );
    assert_eq!(out.messages[1].beat, 9.);
    pair.script("collectgarbage();collectgarbage();assert(action_weak[1]==nil);assert(saved.a:GetX()==91);local h=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):GetColumnActors()[1]:GetPosHandler();setmetatable(h,nil);rawset(h,'__songlua_spline_mode','NoteColumnSplineMode_Disabled')");
    let (out, result) = compare_compiled_actions(&mut pair, &["a"], &[], true, out);
    result.unwrap();
    assert_eq!(out.messages.last().unwrap().message, "replacement-global");
}
#[test]
fn action_compiler_reader_accepts_existing_native_functions_and_records_unhandled_gc() {
    let (_dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self end}");
    pair.script(
        "actions_probe={{1,SOUND.StopMusic,true},{2,collectgarbage,false},{3,'after-native'}}",
    );
    let (out, result) =
        compare_compiled_actions(&mut pair, &["a"], &[], true, ActionPublished::seed());
    result.unwrap();
    assert_eq!(out.counter, 7);
    assert_eq!(out.info.unsupported_function_actions, 1);
    assert_eq!(
        out.info.unsupported_function_action_captures,
        ["function action beat=2.000 persists=false"]
    );
    assert_eq!(out.messages.last().unwrap().message, "after-native");
}

#[test]
fn action_compiler_reader_instruction_abort_records_unsupported_continues_and_recovers() {
    let (_dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self;self:x(3)end}");
    pair.script("actions_probe={{1,function()saved.a:x(99);probe_created={};while true do end end},{2,'after-limit'}}");
    let actor = saved(&mut pair, "a");
    let table = pair.rust.vm_mut().get_global(b"actions_probe").unwrap();
    let mut commands = Vec::new();
    let mut out = ActionPublished::seed();
    let clock = pair.rust.values().unwrap();
    pair.rust.vm_mut().set_instruction_limit(Some(50_000));
    let result = pair.rust.read_song_overlay_actions(
        Some(table),
        &mut [deadsync_song_lua::SongMessageTarget {
            actor: Value::Table(actor),
            commands: &mut commands,
        }],
        &mut [],
        &mut out.output(),
    );
    pair.rust.vm_mut().set_instruction_limit(None);
    result.unwrap();
    assert_eq!(out.info.unsupported_function_actions, 1);
    assert_eq!(out.messages.last().unwrap().message, "after-limit");
    assert_eq!(out.counter, 7);
    assert!(commands.is_empty());
    assert_eq!(pair.rust.values().unwrap(), clock);
    pair.script("assert(saved.a:GetX()==3 and probe_created==nil);assert(__songlua_capture_scope_actors==nil and __songlua_broadcast_messages==nil and __songlua_sound_calls==nil and __songlua_suppress_broadcast_dispatch==nil);actions_probe={{3,function()saved.a:x(12)end}} ");
    let (out, result) = compare_compiled_actions(&mut pair, &["a"], &[], true, out);
    result.unwrap();
    assert_eq!(out.counter, 8);
    assert_eq!(out.info.unsupported_function_actions, 1);
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    drop(pair);
    assert_eq!(out.overlays[0][0].blocks[0].delta.x, Some(12.));
}

fn easing_reader_names(pair: &mut Pair) -> deadsync_song_lua::SongEasingNames {
    let mut names = pair.rust.new_song_easing_names().unwrap();
    pair.rust
        .register_loaded_song_easing_names(&mut names)
        .unwrap();
    names
}

fn easing_result_text(out: &deadsync_song_lua::SongLuaReadEasesResult) -> String {
    format!(
        "{:?}",
        (
            &out.eases,
            &out.overlay_eases,
            &out.column_offsets,
            &out.info,
            out.stats
        )
    )
}
macro_rules! ease_reader_metadata {
    ($i:ident) => {
        format!(
            "{:?}",
            (
                $i.unit,
                $i.start,
                $i.limit,
                $i.span_mode,
                $i.from,
                $i.to,
                &$i.easing,
                $i.sustain,
                $i.opt1,
                $i.opt2
            )
        )
    };
}
fn compare_easing_reader(
    pair: &mut Pair,
    names: &deadsync_song_lua::SongEasingNames,
    unit: deadsync_song_lua::SongLuaTimeUnit,
    choose: impl Fn(f32) -> deadsync_song_lua::SongLuaFunctionEaseDecision,
) -> Result<deadsync_song_lua::SongLuaReadEasesResult, String> {
    let mut rust_inputs = Vec::new();
    let table = pair.rust.vm_mut().get_global(b"eases_probe").unwrap();
    let actual = pair.rust.read_song_eases_with_function_capture(
        Some(table),
        unit,
        &names,
        |runtime, input, info| {
            rust_inputs.push(ease_reader_metadata!(input));
            runtime
                .vm_mut()
                .call(input.function, &[Value::Number(f64::from(input.from))])
                .map_err(|e| e.to_string())?;
            info.unsupported_function_eases += 1;
            Ok(deadsync_song_lua::SongLuaFunctionEaseResult {
                decision: choose(input.start),
                stats: deadsync_song_lua::SongLuaReadEasesStats {
                    entry_count: 100,
                    function_targets: 200,
                    overlay_capture_attempts: 3,
                    overlay_capture_outputs: 4,
                    probe_ms: 0.25,
                    overlay_capture_ms: 0.5,
                },
            })
        },
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_easing_reader::eq1",
        &(rust_inputs),
    );
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_easing_reader::eq2",
        &(actual.is_ok()),
    );
    if let Ok(a) = &actual {
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_easing_reader::eq3",
            &(easing_result_text(a)),
        );
    }
    pair.compare();
    actual
}

#[test]
fn easing_reader_raw_fields_preserve_span_float_player_and_nil_rules() {
    use deadsync_song_lua::{SongLuaFunctionEaseDecision as Decision, SongLuaTimeUnit};
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    pair.script(
        r#"
        ease=nil;xero=nil
        eases_probe=setmetatable({false, {}, {false,1,2,3,'bad'},
            {1,' 2.5 ',3,4,'drunk','eNd','linear',1.9,' 3 ','NaN','inf'},
            {2,3,4,5,'tipsy',' END ',2,6,7,8},
            {'NaN',1,2,3,'nan','LEN','curve','2',false,{},''},
            {math.huge,1,2,3,'skip'}, {1e300,1,2,3,'skip'},
            {1,2,math.huge,3,'skip'}, {1,2,3,string.char(255),'skip'},
            {1,2,3,4,29}, {1,2,3,4,{}},
            setmetatable({9,4,0,1,'raw','len',false,2},{__index=function()error('raw entry')end})
        },{__index=function()error('raw sequence')end})
        eases_probe[15]={30,1,0,1,'after-hole'}
    "#,
    );
    let names = easing_reader_names(&mut pair);
    for unit in [SongLuaTimeUnit::Beat, SongLuaTimeUnit::Second] {
        let out = compare_easing_reader(&mut pair, &names, unit, |_| Decision::Skip).unwrap();
        assert_eq!(out.eases.len(), 4);
        assert_eq!(out.stats.entry_count, 12);
        assert_eq!(out.stats.function_targets, 0);
        assert_eq!(out.eases[0].player, Some(1));
        assert_eq!(out.eases[1].easing.as_deref(), Some(" END "));
        assert_eq!(out.eases[1].sustain, Some(6.));
        assert!(out.eases[2].start.is_nan());
        assert!(out.eases[0].opt1.unwrap().is_nan());
        assert!(out.eases[0].opt2.unwrap().is_infinite());
        assert!(out.eases[2].player.is_none());
        assert!(out.eases[3].easing.is_none());
    }
    pair.script("eases_probe=nil");
    assert!(
        compare_easing_reader(&mut pair, &names, SongLuaTimeUnit::Beat, |_| {
            Decision::Skip
        })
        .unwrap()
        .eases
        .is_empty()
    );
    pair.rust.release_song_easing_names(&mut { names }).unwrap();
}

#[test]
fn easing_reader_loaded_aliases_table_tags_native_functions_and_registry_roots() {
    use deadsync_song_lua::{SongLuaFunctionEaseDecision as Decision, SongLuaTimeUnit};
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    pair.script(r#"
        local f=function()end;local t=setmetatable({},{__newindex=function()error('raw tag')end})
        ease={instant=f,linear=t,inQuad=f,outQuad=collectgarbage};xero={outCubic=f,inCubic=t}
        ease_weak=setmetatable({f,t},{__mode='v'})
        eases_probe={{1,1,0,1,'a',f},{2,1,0,1,'b',t},{3,1,0,1,'c',collectgarbage},
            {4,1,0,1,'d',{__songlua_easing_name=42}},
            {5,1,0,1,'e',setmetatable({},{__index=function()error('raw tag read')end,__tostring=function()error('coercion')end})},
            {6,1,0,1,'f',{__songlua_easing_name=string.char(255)}},
            {7,1,0,1,'g',string.char(255)}, {8,1,0,1,'h',42},
            {9,1,0,1,'i',{__songlua_easing_name=false}}}
    "#);
    let mut names = easing_reader_names(&mut pair);
    pair.script("ease=nil;xero=nil;rawset(eases_probe[2][6],'__songlua_easing_name','changed')");
    let out = compare_easing_reader(&mut pair, &names, SongLuaTimeUnit::Beat, |_| Decision::Skip)
        .unwrap();
    assert_eq!(
        out.eases
            .iter()
            .map(|e| e.easing.as_deref())
            .collect::<Vec<_>>(),
        [
            Some("outCubic"),
            Some("inCubic"),
            Some("outQuad"),
            Some("42"),
            None,
            None,
            None,
            None,
            None
        ]
    );
    // Native's pointer map does not own handles. Only the Rust host registry is
    // under test after removing the last authored strong references.
    pair.rust.vm_mut().execute(&compile_named(b"eases_probe=nil;collectgarbage();collectgarbage();assert(ease_weak[1] and ease_weak[2])", b"=rooted names").unwrap()).unwrap();
    assert!(pair.rust.release_song_easing_names(&mut names).unwrap());
    assert!(!pair.rust.release_song_easing_names(&mut names).unwrap());
    pair.rust.vm_mut().execute(&compile_named(b"collectgarbage();collectgarbage();assert(ease_weak[1]==nil and ease_weak[2]==nil)", b"=released names").unwrap()).unwrap();
}

#[test]
fn easing_reader_decisions_preserve_callback_metadata_output_order_and_stats_merging() {
    use deadsync_song_lua::{
        SongLuaEaseTarget as Target, SongLuaFunctionEaseDecision as Decision, SongLuaTimeUnit,
    };
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    pair.script("ease=nil;xero=nil;eases_probe={};for i=1,6 do eases_probe[i]={i,2,3,4,function(v)log('ease',v)end,'end','linear',2,5,6,7}end");
    let names = easing_reader_names(&mut pair);
    let out =
        compare_easing_reader(&mut pair, &names, SongLuaTimeUnit::Second, |start| {
            match start as usize {
                1 => Decision::Target(Target::PlayerX),
                2 => Decision::ColumnOffsets(vec![deadsync_song_lua::SongLuaColumnOffsetWindow {
                    unit: SongLuaTimeUnit::Second,
                    start: 2.,
                    limit: 4.,
                    span_mode: deadsync_song_lua::SongLuaSpanMode::End,
                    player: 1,
                    column: 3,
                    target: deadsync_song_lua::SongLuaColumnTransformTarget::OffsetY,
                    from_y: 2.,
                    to_y: 9.,
                    easing: Some("linear".into()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                }]),
                3 => Decision::OverlayEases(deadsync_song_lua::overlay_eases_from_captures(
                    1,
                    &[(
                        0,
                        vec![deadsync_song_lua::SongLuaOverlayCommandBlock {
                            delta: deadsync_song_lua::SongLuaOverlayStateDelta {
                                x: Some(1.),
                                ..Default::default()
                            },
                            progress: None,
                            queued: false,
                            start: 0.,
                            duration: 0.,
                            easing: None,
                            opt1: None,
                            opt2: None,
                        }],
                    )],
                    &[(
                        0,
                        vec![deadsync_song_lua::SongLuaOverlayCommandBlock {
                            delta: deadsync_song_lua::SongLuaOverlayStateDelta {
                                x: Some(8.),
                                ..Default::default()
                            },
                            progress: None,
                            queued: false,
                            start: 0.,
                            duration: 0.,
                            easing: None,
                            opt1: None,
                            opt2: None,
                        }],
                    )],
                    overlay_ease_params(),
                )),
                4 => Decision::Target(Target::Function),
                5 => Decision::Skip,
                _ => Decision::Target(Target::Mod("function-mod".into())),
            }
        })
        .unwrap();
    assert_eq!(out.eases.len(), 2);
    assert_eq!(out.column_offsets.len(), 1);
    assert_eq!(out.overlay_eases.len(), 1);
    assert_eq!(out.info.unsupported_function_eases, 6);
    assert_eq!(out.stats.entry_count, 6);
    assert_eq!(out.stats.function_targets, 6);
    assert_eq!(out.stats.overlay_capture_attempts, 18);
    assert_eq!(out.stats.overlay_capture_outputs, 25);
    assert_eq!(out.stats.probe_ms, 1.5);
    assert_eq!(out.stats.overlay_capture_ms, 3.);
    assert_eq!(out.eases[1].start, 6.);
}

#[test]
fn easing_reader_callback_mutation_keeps_original_sequence_and_function_alive_through_gc() {
    use deadsync_song_lua::{
        SongLuaEaseTarget as Target, SongLuaFunctionEaseDecision as Decision, SongLuaTimeUnit,
    };
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    pair.script(
        r#"
        ease=nil;xero=nil
        eases_probe={{1,1,0,1,function(v)
            local t=ease_weak[1];t[1]=nil;t[2]={8,9,10,11,'changed'};t[3]=nil
            eases_probe={{20,1,0,1,'replacement'}};collectgarbage();log('mutated',v)
        end},{2,1,0,1,'old-second'},{3,1,0,1,'old-third'}}
        ease_weak=setmetatable({eases_probe,eases_probe[1],eases_probe[1][5]},{__mode='v'})
    "#,
    );
    let names = easing_reader_names(&mut pair);
    let out = compare_easing_reader(&mut pair, &names, SongLuaTimeUnit::Beat, |_| {
        Decision::Target(Target::PlayerY)
    })
    .unwrap();
    assert_eq!(
        out.eases.iter().map(|e| e.start).collect::<Vec<_>>(),
        [1., 8.]
    );
    assert_eq!(out.stats.entry_count, 2);
    pair.script("collectgarbage();collectgarbage();assert(ease_weak[1]==nil and ease_weak[2]==nil and ease_weak[3]==nil)");
    let out = compare_easing_reader(&mut pair, &names, SongLuaTimeUnit::Beat, |_| Decision::Skip)
        .unwrap();
    assert_eq!(out.eases[0].start, 20.);
}

#[test]
fn easing_reader_hard_function_and_utf8_errors_stop_the_sequence_and_allow_recovery() {
    use deadsync_song_lua::{SongLuaFunctionEaseDecision as Decision, SongLuaTimeUnit};
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    pair.script("ease=nil;xero=nil");
    let names = easing_reader_names(&mut pair);
    for target in [
        "function()log('failed');error('ease body failure')end",
        "string.char(255)",
    ] {
        pair.script(&format!("eases_probe={{{{1,1,0,1,'before'}},{{2,1,0,1,{target}}},{{3,1,0,1,function()error('unreachable')end}}}}"));
        assert!(
            compare_easing_reader(&mut pair, &names, SongLuaTimeUnit::Beat, |_| {
                Decision::Skip
            })
            .is_err()
        );
    }
    pair.script("eases_probe={{4,1,0,1,'recovered'},{5,1,0,1,math.abs}}");
    let out = compare_easing_reader(&mut pair, &names, SongLuaTimeUnit::Beat, |_| Decision::Skip)
        .unwrap();
    assert_eq!(out.eases[0].start, 4.);
    assert_eq!(out.stats.function_targets, 1);
}

#[test]
fn easing_reader_registration_getters_preserve_order_raw_tags_and_partial_errors() {
    use deadsync_song_lua::{SongLuaFunctionEaseDecision as Decision, SongLuaTimeUnit};
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    pair.script(
        r#"
        local f=function()end
        ease=setmetatable({instant=f},{__index=function(t,k)
            log('lookup',k)
            if k=='linear'then ease=nil;collectgarbage();return {__songlua_easing_name='old'}end
            if k=='outQuad'then error('registration stopped')end
        end});xero={instant=function()error('must not register')end}
        eases_probe={{1,1,0,1,'a',f}}
        real_G=_G;_G={ease={}}
    "#,
    );

    let mut rust = pair.rust.new_song_easing_names().unwrap();

    assert!(
        pair.rust
            .register_loaded_song_easing_names(&mut rust)
            .is_err()
    );
    assert_eq!(pair.compare(), "lookup,linear|lookup,inQuad|lookup,outQuad");
    let mut names = rust;
    let out = compare_easing_reader(&mut pair, &names, SongLuaTimeUnit::Beat, |_| Decision::Skip)
        .unwrap();
    assert_eq!(out.eases[0].easing.as_deref(), Some("instant"));
    pair.script("_G=real_G;real_G=nil;ease={};xero=nil");

    pair.rust
        .register_loaded_song_easing_names(&mut names)
        .unwrap();
    let out = compare_easing_reader(&mut pair, &names, SongLuaTimeUnit::Beat, |_| Decision::Skip)
        .unwrap();
    assert_eq!(out.eases[0].easing.as_deref(), Some("instant"));
}

#[test]
fn easing_reader_registry_rejects_foreign_or_released_owners_and_abort_releases_iteration_roots() {
    use deadsync_song_lua::{SongLuaFunctionEaseDecision as Decision, SongLuaTimeUnit};
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    pair.script("ease=nil;xero=nil;eases_probe={{1,1,0,1,function()eases_probe=nil;while true do end end}};ease_weak=setmetatable({eases_probe,eases_probe[1]},{__mode='v'})");
    let mut names = pair.rust.new_song_easing_names().unwrap();
    let mut other = SongRuntime::new(&pair.context).unwrap();
    assert!(other.register_loaded_song_easing_names(&mut names).is_err());
    assert!(other.release_song_easing_names(&mut names).is_err());
    let table = pair.rust.vm_mut().get_global(b"eases_probe").unwrap();
    pair.rust.vm_mut().set_instruction_limit(Some(40_000));
    let result = pair.rust.read_song_eases_with_function_capture(
        Some(table),
        SongLuaTimeUnit::Beat,
        &names,
        |runtime, input, _| {
            runtime
                .vm_mut()
                .call(input.function, &[])
                .map_err(|e| e.to_string())?;
            Ok(deadsync_song_lua::SongLuaFunctionEaseResult {
                decision: Decision::Skip,
                stats: Default::default(),
            })
        },
    );
    pair.rust.vm_mut().set_instruction_limit(None);
    assert!(result.is_err());
    pair.rust.vm_mut().execute(&compile_named(b"collectgarbage();collectgarbage();assert(ease_weak[1]==nil and ease_weak[2]==nil)", b"=reader abort cleanup").unwrap()).unwrap();
    assert!(pair.rust.release_song_easing_names(&mut names).unwrap());
    assert!(
        pair.rust
            .register_loaded_song_easing_names(&mut names)
            .is_err()
    );
    let table = pair.rust.vm_mut().try_new_table().unwrap();
    assert!(
        pair.rust
            .read_song_eases_with_function_capture(
                Some(Value::Table(table)),
                SongLuaTimeUnit::Beat,
                &names,
                |_, _, _| unreachable!()
            )
            .is_err()
    );
}

#[test]
fn easing_reader_runs_existing_overlay_endpoints_and_keeps_owned_output_after_teardown() {
    use deadsync_song_lua::{SongLuaFunctionEaseDecision as Decision, SongLuaTimeUnit};
    let (_dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self end}");
    pair.script("ease=nil;xero=nil;ease_probe=function(v)saved.a:x(v+GAMESTATE:GetSongBeat()):diffusealpha(v/10)end;eases_probe={{4,1,2,6,ease_probe,'end','linear',1,9,8,7},{5,2,1,3,'drunk'}}");
    let mut names = easing_reader_names(&mut pair);
    let b = saved(&mut pair, "a");

    let table = pair.rust.vm_mut().get_global(b"eases_probe").unwrap();
    let rust = pair
        .rust
        .read_song_eases_with_function_capture(
            Some(table),
            SongLuaTimeUnit::Beat,
            &names,
            |runtime, i, _| {
                let rows = runtime.capture_song_overlay_function_eases(
                    &[(0, Value::Table(b))],
                    i.function,
                    deadsync_song_lua::SongLuaOverlayEaseBuildParams {
                        unit: i.unit,
                        start: i.start,
                        limit: i.limit,
                        span_mode: i.span_mode,
                        easing: i.easing,
                        sustain: i.sustain,
                        opt1: i.opt1,
                        opt2: i.opt2,
                    },
                    [i.from, i.to],
                    &[Value::Table(b)],
                )?;
                Ok(deadsync_song_lua::SongLuaFunctionEaseResult {
                    decision: Decision::OverlayEases(rows),
                    stats: deadsync_song_lua::SongLuaReadEasesStats {
                        overlay_capture_attempts: 1,
                        ..Default::default()
                    },
                })
            },
        )
        .unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::easing_reader_runs_existing_overlay_endpoints_and_keeps_owned_output_after_teardown::eq1",
        &(easing_result_text(&rust)),
    );
    assert_eq!(rust.stats.function_targets, 1);
    assert_eq!(rust.stats.overlay_capture_outputs, 1);
    assert_eq!(rust.eases.len(), 1);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::easing_reader_runs_existing_overlay_endpoints_and_keeps_owned_output_after_teardown::eq5",
        &(pair.rust.values().unwrap()),
    );
    pair.script("assert(saved.a:GetX()==0)");
    pair.rust.release_song_easing_names(&mut names).unwrap();
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    drop(pair);
    assert_eq!(rust.overlay_eases.len(), 1);
    assert_eq!(rust.eases[0].start, 5.);
}

#[test]
fn easing_reader_nested_capture_retains_detached_function_and_entry_until_callback_returns() {
    use deadsync_song_lua::{
        SongLuaEaseTarget as Target, SongLuaFunctionEaseDecision as Decision, SongLuaTimeUnit,
    };
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    pair.script(
        r#"
        ease=nil;xero=nil
        eases_probe={{1,2,3,4,function(v)
            if eases_probe then eases_probe[1][5]=nil;eases_probe[1]=nil;eases_probe=nil end
            log('nested outer',v)
        end},{2,1,0,1,'outer-second'}}
        inner_eases={{10,1,0,1,'inner'}}
        ease_weak=setmetatable({eases_probe,eases_probe[1],eases_probe[1][5]},{__mode='v'})
    "#,
    );
    let names = easing_reader_names(&mut pair);

    let table = pair.rust.vm_mut().get_global(b"eases_probe").unwrap();
    let actual=pair.rust.read_song_eases_with_function_capture(Some(table),SongLuaTimeUnit::Beat,&names,
        |runtime,i,_| {
            runtime.vm_mut().call(i.function,&[Value::Number(f64::from(i.from))]).map_err(|e|e.to_string())?;
            let inner=runtime.vm_mut().get_global(b"inner_eases").map_err(|e|e.to_string())?;
            let inner=runtime.read_song_eases_with_function_capture(Some(inner),SongLuaTimeUnit::Second,&names,|_,_,_|unreachable!())?;
            assert_eq!(inner.eases[0].start,10.);
            runtime.vm_mut().execute(&compile_named(b"collectgarbage();collectgarbage();assert(ease_weak[1] and ease_weak[2] and ease_weak[3])",b"=nested reader roots").unwrap()).unwrap();
            runtime.vm_mut().call(i.function,&[Value::Number(f64::from(i.to))]).map_err(|e|e.to_string())?;
            Ok(deadsync_song_lua::SongLuaFunctionEaseResult {decision:Decision::Target(Target::PlayerZ),stats:Default::default()})
        }).unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::easing_reader_nested_capture_retains_detached_function_and_entry_until_callback_returns::eq3",
        &(easing_result_text(&actual)),
    );
    assert_eq!(pair.compare(), "nested outer,3|nested outer,4");
    pair.script("collectgarbage();collectgarbage();assert(ease_weak[1]==nil and ease_weak[2]==nil and ease_weak[3]==nil)");
}

fn compare_column_samples(
    pair: &mut Pair,
    globals: bool,
) -> Result<Vec<deadsync_song_lua::SongLuaColumnTransformSample>, String> {
    let rust = if globals {
        pair.rust.read_song_note_column_transform_samples()
    } else {
        let Value::Table(list) = pair.rust.vm_mut().get_global(b"fields_probe").unwrap() else {
            panic!()
        };
        let mut fields = Vec::new();
        for i in 1usize.. {
            let v = pair
                .rust
                .vm_mut()
                .raw_table_get(list, Value::Number(i as f64))
                .unwrap();
            if v == Value::Nil {
                break;
            }
            fields.push(v);
        }
        pair.rust
            .read_song_note_column_transform_samples_for_fields(&fields)
    };
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_column_samples::eq1",
        &(rust.is_ok()),
    );
    if let Ok(a) = &rust {
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_column_samples::eq2",
            &(a),
        );
    }
    pair.compare();
    rust
}
fn column_sample_fixture(pair: &mut Pair) {
    pair.script(
        r#"
        function column_handler(mode,size,points)
            return {__songlua_spline_mode='NoteColumnSplineMode_'..mode,
                __songlua_beats_per_t=1,__songlua_receptor_t=0,__songlua_subtract_song_beat=true,
                __songlua_spline={__songlua_spline_size=size,__songlua_spline_points=points or {}}}
        end
        function sample_column(player,index,pos,zoom,rot)
            return {__songlua_player_index=player,__songlua_column_index=index,
                __songlua_pos_handler=pos,__songlua_zoom_handler=zoom,__songlua_rot_handler=rot}
        end
        sample_points={{2,3,4},{2,3,4},{2,3,4}}
        sample_pos=column_handler('Offset',3,sample_points)
        sample_zoom=column_handler('Offset',3,{{.5},{.5},{.5}})
        sample_rot=column_handler('Position',3,{{0,0,30},{0,0,90},{0,0,180}})
        sample_col=sample_column(0,0,sample_pos,sample_zoom,sample_rot)
        sample_field={__songlua_note_columns={sample_col}}
        fields_probe={sample_field}
    "#,
    );
}
fn column_ease_params() -> deadsync_song_lua::SongLuaColumnOffsetBuildParams {
    deadsync_song_lua::SongLuaColumnOffsetBuildParams {
        unit: deadsync_song_lua::SongLuaTimeUnit::Beat,
        start: 4.,
        limit: 2.,
        span_mode: deadsync_song_lua::SongLuaSpanMode::Len,
        easing: Some("inQuad".into()),
        sustain: Some(3.),
        opt1: Some(4.),
        opt2: Some(5.),
    }
}
fn compare_column_eases(
    pair: &mut Pair,
    params: deadsync_song_lua::SongLuaColumnOffsetBuildParams,
) -> Result<Vec<deadsync_song_lua::SongLuaColumnOffsetWindow>, String> {
    let f = pair.rust.vm_mut().get_global(b"column_ease_probe").unwrap();
    let rust = pair
        .rust
        .capture_song_note_column_function_eases(f, params, [2., 6.]);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_column_eases::eq1",
        &(rust.is_ok()),
    );
    if let Ok(a) = &rust {
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_column_eases::eq2",
            &(a),
        );
    }
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_column_eases::eq3",
        &(pair.rust.values().unwrap()),
    );
    pair.compare();
    rust
}
fn column_endpoint_fixture(pair: &mut Pair) {
    pair.script(
        r#"
        column_field=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField')
        column_list=column_field:GetColumnActors();column_actor=column_list[1]
        column_pos=column_actor:GetPosHandler();column_spline=column_pos:GetSpline()
        column_before_mode=column_pos.__songlua_spline_mode
    "#,
    );
}

#[test]
fn column_eases_samples_keep_native_mode_baselines_numeric_coercion_and_target_order() {
    use deadsync_song_lua::SongLuaColumnTransformTarget as T;
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    column_sample_fixture(&mut pair);
    let out = compare_column_samples(&mut pair, false).unwrap();
    assert_eq!(
        out.iter().map(|v| (v.target, v.value)).collect::<Vec<_>>(),
        [
            (T::OffsetX, 2.),
            (T::OffsetY, 3.),
            (T::Zoom, 1.5),
            (T::RotationZ, 30.),
            (T::PositionEnabled, 0.)
        ]
    );
    for change in [
        "sample_pos.__songlua_spline_mode='notecolumnsplinemode_disabled';sample_zoom.__songlua_spline_mode='NoteColumnSplineMode_Position'",
        "sample_pos.__songlua_spline_mode='NoteColumnSplineMode_Position';sample_rot.__songlua_spline_mode='NoteColumnSplineMode_Disabled'",
        "sample_pos.__songlua_spline_mode='unknown';sample_zoom.__songlua_spline_mode=nil;sample_rot.__songlua_spline=nil",
        "sample_pos=column_handler('Offset',0);sample_col.__songlua_pos_handler=sample_pos;sample_col.__songlua_zoom_handler=nil;sample_col.__songlua_rot_handler=nil",
        "sample_pos.__songlua_spline.__songlua_spline_size='3.9';sample_pos.__songlua_spline.__songlua_spline_points={{' 0x2 ','3.5'},{'2','3.5'},{'2','3.5'}};sample_col.__songlua_player_index='1.9';sample_col.__songlua_column_index='2.8'",
        "sample_col.__songlua_player_index=-1",
        "sample_col.__songlua_player_index=nil",
    ] {
        pair.script(change);
        compare_column_samples(&mut pair, false).unwrap();
    }
}

#[test]
fn column_eases_samples_validate_two_point_splines_and_preserve_sparse_raw_points() {
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    column_sample_fixture(&mut pair);
    pair.script("sample_pos.__songlua_spline.__songlua_spline_size=2;sample_pos.__songlua_beats_per_t='-2';sample_pos.__songlua_receptor_t='3.5';sample_pos.__songlua_subtract_song_beat=0;sample_points[1]=setmetatable({1,nil,3},{__index=function()error('raw axis')end});sample_points[2]=nil;setmetatable(sample_points,{__index=function()error('raw point')end})");
    let out = compare_column_samples(&mut pair, false).unwrap();
    assert_eq!(out.len(), 14);
    for change in [
        "sample_pos.__songlua_beats_per_t=0",
        "sample_pos.__songlua_beats_per_t=math.huge",
        "sample_pos.__songlua_beats_per_t=1;sample_pos.__songlua_receptor_t=0/0",
        "sample_pos.__songlua_receptor_t=0;sample_points[1]={0/0,4,0}",
        "sample_points[1]={1,2,3};sample_points[2]=false",
        "sample_points[2]={true,2,3}",
    ] {
        pair.script(change);
        let result = compare_column_samples(&mut pair, false);
        if change.ends_with("false") || change.contains("{true") {
            assert!(result.is_err());
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn column_eases_samples_recheck_nonuniform_witnesses_and_keep_later_y_errors() {
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    column_sample_fixture(&mut pair);
    pair.script("sample_points[2]={7,3,4}");
    compare_column_samples(&mut pair, false).unwrap();
    pair.script(
        "assert(sample_pos.__songlua_spline.__songlua_nonuniform_x==2);sample_points[2]={2,3,4}",
    );
    let out = compare_column_samples(&mut pair, false).unwrap();
    assert_eq!(out[0].value, 2.);
    pair.script("sample_points[2]={7,8,4};sample_points[3]=false");
    assert!(compare_column_samples(&mut pair, false).is_err());
    pair.script("sample_points[3]={2,3,4};sample_pos.__songlua_spline.__songlua_nonuniform_x=-1");
    assert!(compare_column_samples(&mut pair, false).is_err());
    pair.script("sample_pos.__songlua_spline.__songlua_nonuniform_x=1e19");
    compare_column_samples(&mut pair, false).unwrap();
}

#[test]
fn column_eases_samples_pin_transient_columns_handlers_and_points_during_witness_setters() {
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    column_sample_fixture(&mut pair);
    pair.script(r#"
        sample_points[2]={7,3,4}
        sample_weak=setmetatable({sample_col,sample_pos,sample_pos.__songlua_spline,sample_points},{__mode='v'})
        setmetatable(sample_pos.__songlua_spline,{__newindex=function(t,k,v)
            if k=='__songlua_nonuniform_x'then
                sample_field.__songlua_note_columns[1]=nil
                sample_col=nil;sample_pos=nil;sample_points=nil;collectgarbage();collectgarbage()
                assert(sample_weak[1] and sample_weak[2] and sample_weak[3] and sample_weak[4]);log('witness',v)
            end
            rawset(t,k,v)
        end})
    "#);
    let out = compare_column_samples(&mut pair, false).unwrap();
    assert!(out.iter().any(|s| s.target
        == deadsync_song_lua::SongLuaColumnTransformTarget::OffsetY
        && s.value == 3.));
    pair.script("collectgarbage();collectgarbage();assert(sample_weak[1]==nil and sample_weak[2]==nil and sample_weak[3]==nil and sample_weak[4]==nil)");
}

#[test]
fn column_eases_global_collection_roots_both_fields_before_reading_columns() {
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    column_sample_fixture(&mut pair);
    pair.script(r#"
        local second={__songlua_note_columns={sample_column(1,0,column_handler('Disabled',0))}}
        local player={__songlua_children={NoteField=second}}
        fields_weak=setmetatable({sample_field,second,player},{__mode='v'})
        __songlua_top_screen_player_1={__songlua_children={NoteField=sample_field}}
        __songlua_top_screen_player_2=nil
        setmetatable(_G,{__index=function(t,k)
            if k=='__songlua_top_screen_player_2'then
                log('global second');local result=player;player=nil;setmetatable(t,nil);return result
            end
        end})
        local columns=sample_field.__songlua_note_columns
        sample_field.__songlua_note_columns=nil
        setmetatable(sample_field,{__index=function(t,k)
            if k=='__songlua_note_columns'then
                collectgarbage();collectgarbage();assert(fields_weak[2] and fields_weak[3]==nil)
                log('columns first');return columns
            end
        end})
    "#);
    let out = compare_column_samples(&mut pair, true).unwrap();
    assert_eq!(out.len(), 8);
    assert_eq!(pair.compare(), "global second|columns first");
    pair.script("collectgarbage();collectgarbage();assert(fields_weak[2]==nil)");
}

#[test]
fn column_eases_endpoint_capture_keeps_metadata_clocks_shared_cells_rng_and_original_splines() {
    let (_dir, mut pair) =
        command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self end}");
    column_endpoint_fixture(&mut pair);
    pair.script(r#"
        local count=0
        column_ease_probe=function(v)
            count=count+1;column_count=count;column_random=math.random();saved.a:x(v)
            column_pos:SetSplineMode('NoteColumnSplineMode_Offset'):SetBeatsPerT(2):SetReceptorT(3):SetSubtractSongBeat(false)
            column_spline:SetSize(2);column_spline:SetPoint(1,{v,GAMESTATE:GetSongBeat(),0});column_spline:SetPoint(2,{v+1,GAMESTATE:GetSongBeat()+2,0})
            log('column endpoint',v,GAMESTATE:GetSongBeat());collectgarbage()
        end
    "#);
    for unit in [
        deadsync_song_lua::SongLuaTimeUnit::Beat,
        deadsync_song_lua::SongLuaTimeUnit::Second,
    ] {
        let mut params = column_ease_params();
        params.unit = unit;
        params.span_mode = deadsync_song_lua::SongLuaSpanMode::End;
        params.limit = 1.;
        let out = compare_column_eases(&mut pair, params.clone()).unwrap();
        assert!(!out.is_empty());
        assert!(out.iter().all(|w| w.unit == unit
            && w.start == 4.
            && w.limit == 1.
            && w.span_mode == params.span_mode
            && w.easing == params.easing
            && w.sustain == params.sustain
            && w.opt1 == params.opt1
            && w.opt2 == params.opt2));
        pair.script("assert(column_pos.__songlua_spline_mode==column_before_mode and column_spline.__songlua_spline_size==0);assert(column_actor:GetPosHandler()==column_pos and column_pos:GetSpline()==column_spline and column_field:GetColumnActors()==column_list);assert(saved.a:GetX()==6 and column_count%2==0);log('random after',column_random,math.random())");
    }
    assert!(pair.compare().contains("column endpoint,6,4"));
}

#[test]
fn column_eases_endpoint_errors_keep_native_body_sample_and_restore_priority() {
    for (body, marker) in [
        (
            "column_pos:SetSplineMode('NoteColumnSplineMode_Position');error('column body error')",
            "column body error",
        ),
        (
            "column_pos:SetSplineMode('NoteColumnSplineMode_Offset');rawset(column_spline,'__songlua_spline_size',2);rawset(column_spline,'__songlua_spline_points',false)",
            "table",
        ),
        (
            "column_pos:SetSplineMode('NoteColumnSplineMode_Offset');rawset(column_spline,'__songlua_spline_size',2);rawset(column_spline,'__songlua_spline_points',false);error('body before invalid samples')",
            "body before invalid samples",
        ),
        (
            "rawset(column_pos,'__songlua_spline_mode',nil);setmetatable(column_pos,{__newindex=function(t,k,v)if k=='__songlua_spline_mode'then error('column restore error')end;rawset(t,k,v)end});error('lower priority body error')",
            "column restore error",
        ),
    ] {
        let (_dir, mut pair) =
            command_scope_pair("return Def.Quad{InitCommand=function(self)saved.a=self end}");
        column_endpoint_fixture(&mut pair);
        pair.script(&format!(
            "column_ease_probe=function()saved.a:x(91);{body} end"
        ));
        let error = compare_column_eases(&mut pair, column_ease_params()).unwrap_err();
        assert!(error.contains(marker), "{error}");
        pair.script("assert(saved.a:GetX()==91)");
        if marker != "column restore error" {
            pair.script("assert(column_pos.__songlua_spline_mode==column_before_mode and column_spline.__songlua_spline_size==0)");
        } else {
            pair.script("assert(GAMESTATE:GetSongBeat()==4)");
        }
    }
}

#[test]
fn column_eases_endpoint_first_return_stays_alive_during_authored_restoration() {
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    column_endpoint_fixture(&mut pair);
    pair.script(r#"
        column_ease_probe=function(v)
            rawset(column_pos,'__songlua_spline_mode',nil)
            setmetatable(column_pos,{__newindex=function(t,k,v)
                if k=='__songlua_spline_mode'then
                    collectgarbage();collectgarbage();assert(column_return_weak[1] and column_return_weak[2]==nil);log('restore returned')
                end;rawset(t,k,v)
            end})
            local first,second={},{};column_return_weak=setmetatable({first,second},{__mode='v'});return first,second
        end
    "#);
    // Sampling fails on the removed required mode, but restoration still runs
    // with only the first return retained, then restores the previous clock.
    assert!(compare_column_eases(&mut pair, column_ease_params()).is_err());
    assert_eq!(pair.compare(), "restore returned");
    pair.script("collectgarbage();collectgarbage();assert(column_return_weak[1]==nil and column_return_weak[2]==nil)");
}

#[test]
fn column_eases_snapshot_failure_preserves_selected_clock_and_does_not_call_body() {
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    column_endpoint_fixture(&mut pair);
    pair.script("column_field.__songlua_note_columns={false};column_ease_probe=function()error('unreached body')end");
    let error = compare_column_eases(&mut pair, column_ease_params()).unwrap_err();
    assert!(!error.contains("unreached body"));
    pair.script("assert(GAMESTATE:GetSongBeat()==4)");
}

#[test]
fn column_eases_instruction_abort_restores_columns_clocks_and_releases_detached_values() {
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    column_endpoint_fixture(&mut pair);
    pair.script("column_ease_probe=function()column_pos:SetSplineMode('NoteColumnSplineMode_Offset');column_spline:SetSize(2);column_spline:SetPoint(1,{3,4,5});column_spline:SetPoint(2,{6,7,8});column_ease_probe=nil;while true do end end;column_function_weak=setmetatable({column_ease_probe},{__mode='v'})");
    let f = pair.rust.vm_mut().get_global(b"column_ease_probe").unwrap();
    let previous = pair.rust.values().unwrap();
    pair.rust.vm_mut().set_instruction_limit(Some(60_000));
    let result =
        pair.rust
            .capture_song_note_column_function_eases(f, column_ease_params(), [2., 6.]);
    pair.rust.vm_mut().set_instruction_limit(None);
    assert!(result.is_err());
    assert_eq!(pair.rust.values().unwrap(), previous);
    pair.rust.vm_mut().execute(&compile_named(b"assert(column_pos.__songlua_spline_mode==column_before_mode and column_spline.__songlua_spline_size==0);collectgarbage();collectgarbage();assert(column_function_weak[1]==nil);column_ease_probe=function()end",b"=column abort recovery").unwrap()).unwrap();
    let f = pair.rust.vm_mut().get_global(b"column_ease_probe").unwrap();
    assert!(
        pair.rust
            .capture_song_note_column_function_eases(f, column_ease_params(), [2., 6.])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn column_eases_samples_release_each_processed_field_before_the_next_getter() {
    for globals in [false, true] {
        let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
        column_sample_fixture(&mut pair);
        pair.script(r#"
            local first=sample_field
            local second={}
            local columns1=first.__songlua_note_columns
            local columns2={sample_column(1,0,column_handler('Disabled',0))}
            processed_weak=setmetatable({first,second},{__mode='v'})
            first.__songlua_note_columns=nil
            setmetatable(first,{__index=function(t,k)
                if k=='__songlua_note_columns'then
                    fields_probe=nil;sample_field=nil
                    __songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil
                    return columns1
                end
            end})
            setmetatable(second,{__index=function(t,k)
                if k=='__songlua_note_columns'then
                    collectgarbage();collectgarbage();assert(processed_weak[1]==nil and processed_weak[2]);log('processed field released')
                    return columns2
                end
            end})
            fields_probe={first,second}
            __songlua_top_screen_player_1={__songlua_children={NoteField=first}}
            __songlua_top_screen_player_2={__songlua_children={NoteField=second}}
        "#);
        let out = compare_column_samples(&mut pair, globals).unwrap();
        assert_eq!(out.len(), 8);
        assert_eq!(pair.compare(), "processed field released");
        pair.script("collectgarbage();collectgarbage();assert(processed_weak[1]==nil and processed_weak[2]==nil)");
    }
}

#[test]
fn column_eases_reader_publishes_real_column_endpoints_and_keeps_owned_rows_after_teardown() {
    use deadsync_song_lua::{SongLuaFunctionEaseDecision as Decision, SongLuaTimeUnit};
    let (_dir, mut pair) = command_scope_pair("return Def.Actor{}");
    column_endpoint_fixture(&mut pair);
    pair.script(
        r#"
        ease=nil;xero=nil
        column_ease_probe=function(v)
            column_pos:SetSplineMode('NoteColumnSplineMode_Offset')
            column_spline:SetSize(3)
            for i=1,3 do column_spline:SetPoint(i,{v,v+GAMESTATE:GetSongBeat(),0})end
        end
        eases_probe={{4,2,2,6,column_ease_probe,'len','linear',1,9,8,7},{9,1,0,1,'drunk'}}
    "#,
    );
    let mut names = easing_reader_names(&mut pair);

    let table = pair.rust.vm_mut().get_global(b"eases_probe").unwrap();
    let rust = pair
        .rust
        .read_song_eases_with_function_capture(
            Some(table),
            SongLuaTimeUnit::Second,
            &names,
            |runtime, i, _| {
                let rows = runtime.capture_song_note_column_function_eases(
                    i.function,
                    deadsync_song_lua::SongLuaColumnOffsetBuildParams {
                        unit: i.unit,
                        start: i.start,
                        limit: i.limit,
                        span_mode: i.span_mode,
                        easing: i.easing,
                        sustain: i.sustain,
                        opt1: i.opt1,
                        opt2: i.opt2,
                    },
                    [i.from, i.to],
                )?;
                Ok(deadsync_song_lua::SongLuaFunctionEaseResult {
                    decision: Decision::ColumnOffsets(rows),
                    stats: Default::default(),
                })
            },
        )
        .unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::column_eases_reader_publishes_real_column_endpoints_and_keeps_owned_rows_after_teardown::eq1",
        &(easing_result_text(&rust)),
    );
    assert_eq!(rust.column_offsets.len(), 2);
    assert_eq!(rust.stats.function_targets, 1);
    assert_eq!(rust.stats.overlay_capture_attempts, 0);
    pair.script("assert(column_pos.__songlua_spline_mode==column_before_mode and column_spline.__songlua_spline_size==0)");
    pair.rust.release_song_easing_names(&mut names).unwrap();
    pair.rust.retire_actor_scene(pair.scene).unwrap();
    drop(pair);
    assert_eq!(rust.column_offsets[0].from_y, 2.);
    assert_eq!(rust.column_offsets[0].to_y, 6.);
    assert!(
        rust.column_offsets
            .iter()
            .all(|w| w.unit == SongLuaTimeUnit::Second
                && w.easing.as_deref() == Some("linear")
                && w.sustain == Some(9.))
    );
}

fn function_ease_pair() -> (tempfile::TempDir, Pair) {
    let (dir, mut pair) = command_scope_pair(
        "return Def.ActorFrame{Def.Quad{InitCommand=function(self)saved.a=self end},Def.Quad{InitCommand=function(self)saved.b=self end}}",
    );
    pair.script("saved.p=SCREENMAN:GetTopScreen():GetChild('PlayerP1');saved.p2=SCREENMAN:GetTopScreen():GetChild('PlayerP2');saved.n=saved.p:GetChild('NoteField');saved.j=saved.p:GetChild('Judgment');probe_ids={saved.a,saved.b,saved.p,saved.p2,saved.n,saved.j}");
    (dir, pair)
}
fn compare_function_ease_probe(
    pair: &mut Pair,
) -> Result<
    (
        Option<deadsync_song_lua::SongLuaEaseTarget>,
        Vec<String>,
        Vec<usize>,
    ),
    String,
> {
    let Value::Table(ids) = pair.rust.vm_mut().get_global(b"probe_ids").unwrap() else {
        panic!()
    };
    let ids = (1..=pair.rust.vm_mut().raw_table_len(ids).unwrap())
        .map(|i| {
            pair.rust
                .vm_mut()
                .raw_table_get(ids, Value::Number(i as f64))
                .unwrap()
        })
        .collect::<Vec<_>>();

    let f = pair.rust.vm_mut().get_global(b"probe_fn").unwrap();
    let rust = pair.rust.probe_song_function_ease_target(f);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_function_ease_probe::eq1",
        &(rust.is_ok()),
    );
    let rust = rust.map(|mut r| {
        let actors = pair
            .rust
            .song_function_ease_probe_actors(&r)
            .unwrap()
            .into_iter()
            .map(|v| ids.iter().position(|id| *id == v).unwrap_or(usize::MAX))
            .collect::<Vec<_>>();
        let out = (r.target.clone(), r.methods.clone(), actors);
        pair.rust.release_song_function_ease_probe(&mut r).unwrap();
        out
    });
    if let Ok(a) = &rust {
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_function_ease_probe::eq2",
            &(a),
        );
    }
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_function_ease_probe::eq3",
        &(pair.rust.values().unwrap()),
    );
    pair.compare();
    rust
}
fn compare_function_eases(
    pair: &mut Pair,
    keys: &[&str],
) -> Result<deadsync_song_lua::SongLuaReadEasesResult, String> {
    let mut values = Vec::new();
    for key in keys {
        let b = saved(pair, key);
        values.push((values.len(), Value::Table(b)));
    }
    let mut names = easing_reader_names(pair);

    let table = pair.rust.vm_mut().get_global(b"eases_probe").unwrap();
    let rust = pair.rust.read_song_eases_for_overlay_actors(
        Some(table),
        deadsync_song_lua::SongLuaTimeUnit::Beat,
        &names,
        &values,
    );
    pair.rust.release_song_easing_names(&mut names).unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_function_eases::eq1",
        &(rust.is_ok()),
    );
    if let Ok(a) = &rust {
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_function_eases::eq2",
            &((&a.eases, &a.overlay_eases, &a.column_offsets, &a.info)),
        );
        assert!(a.stats.probe_ms >= 0. && a.stats.overlay_capture_ms >= 0.);
    }
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_function_eases::eq3",
        &(pair.rust.values().unwrap()),
    );
    pair.compare();
    rust
}

#[test]
fn function_ease_probe_classifies_all_existing_player_targets_and_repeated_setters() {
    use deadsync_song_lua::SongLuaEaseTarget as T;
    let (_dir, mut pair) = function_ease_pair();
    for (method, target) in [
        ("x", T::PlayerX),
        ("y", T::PlayerY),
        ("z", T::PlayerZ),
        ("rotationx", T::PlayerRotationX),
        ("rotationy", T::PlayerRotationY),
        ("rotationz", T::PlayerRotationZ),
        ("skewx", T::PlayerSkewX),
        ("skewy", T::PlayerSkewY),
        ("zoom", T::PlayerZoom),
        ("zoomx", T::PlayerZoomX),
        ("zoomy", T::PlayerZoomY),
        ("zoomz", T::PlayerZoomZ),
    ] {
        pair.script(&format!(
            "probe_fn=function(v)saved.p:{method}(v);saved.p:{method}(v+1)end"
        ));
        let out = compare_function_ease_probe(&mut pair).unwrap();
        assert_eq!(out.0, Some(target));
        assert_eq!(out.1, vec![format!("player.{method}"); 2]);
        assert_eq!(out.2, [2]);
    }
}

#[test]
fn function_ease_probe_tracks_write_order_separately_from_named_setter_calls() {
    let (_dir, mut pair) = function_ease_pair();
    for (body, methods, actors) in [
        (
            "saved.b:diffusealpha(.3);saved.a:x(2);saved.b:y(3)",
            vec!["overlay.x", "overlay.y"],
            vec![1, 0],
        ),
        (
            "saved.p:x(2);saved.p:y(3)",
            vec!["player.x", "player.y"],
            vec![2],
        ),
        ("saved.n:x(2)", vec!["notefield.x"], vec![4]),
        ("saved.j:rotationz(2)", vec!["judgment.rotationz"], vec![5]),
        (
            "saved.p:baserotationx(3)",
            vec!["player.baserotationx"],
            vec![2],
        ),
        (
            "saved.a:diffusealpha(.3);saved.b:skewx(4)",
            vec!["overlay.skewx"],
            vec![0, 1],
        ),
    ] {
        pair.script(&format!("probe_fn=function(){body} end"));
        let out = compare_function_ease_probe(&mut pair).unwrap();
        assert!(out.0.is_none());
        assert_eq!(out.1, methods);
        assert_eq!(out.2, actors);
    }
}

#[test]
fn function_ease_probe_keeps_native_empty_tween_ownership_and_skips_nonwriting_operations() {
    let (_dir, mut pair) = function_ease_pair();
    for (body, methods, actors) in [
        (
            "saved.a:linear(1):queuecommand('Empty');saved.b:AddWrapperState()",
            vec![],
            vec![0],
        ),
        ("saved.a:GetX();saved.b:GetY()", vec![], vec![]),
        ("saved.a:xy(false,false)", vec![], vec![]),
        ("saved.a:diffusealpha(false)", vec![], vec![]),
        ("saved.a:x(false)", vec!["overlay.x"], vec![0]),
        ("saved.a:visible(false)", vec![], vec![0]),
        ("saved.a:play()", vec![], vec![0]),
        ("saved.a:effectmagnitude(1,2,3)", vec![], vec![0]),
        ("saved.a:hibernate(.2)", vec![], vec![0]),
    ] {
        pair.script(&format!("probe_fn=function(){body} end"));
        let out = compare_function_ease_probe(&mut pair).unwrap();
        assert!(out.0.is_none());
        assert_eq!(out.1, methods);
        assert_eq!(out.2, actors);
    }
}

#[test]
fn function_ease_probe_restores_rng_clocks_globals_and_keeps_shared_writes_on_body_errors() {
    let (_dir, mut pair) = function_ease_pair();
    pair.script("math.randomseed(4321);__songlua_probe_methods='old methods';__songlua_probe_actors=17;__songlua_probe_actor_set=false;local count=0;probe_fn=function(v)count=count+1;probe_count=count;probe_random=math.random();saved.p:x(40+v);error('probe body failure')end");
    let out = compare_function_ease_probe(&mut pair).unwrap();
    assert!(out.0.is_none());
    assert_eq!(out.1, ["player.x"]);
    assert_eq!(out.2, [2]);
    pair.script("assert(probe_count==1 and saved.p:GetX()==41);assert(__songlua_probe_methods=='old methods' and __songlua_probe_actors==17 and __songlua_probe_actor_set==false);assert(math.random()==probe_random)");
}

#[test]
fn function_ease_probe_uses_original_observation_tables_and_validates_corrupt_sequences() {
    use deadsync_song_lua::SongLuaEaseTarget as T;
    let (_dir, mut pair) = function_ease_pair();
    pair.script("probe_fn=function()__songlua_probe_methods[1]='player.y';__songlua_probe_methods={};__songlua_probe_actors={};saved.p:x(9)end");
    let out = compare_function_ease_probe(&mut pair).unwrap();
    assert_eq!(out.0, Some(T::PlayerY));
    assert_eq!(out.1, ["player.y"]);
    assert!(out.2.is_empty());
    for body in [
        "__songlua_probe_methods[1]=false",
        "__songlua_probe_actors[1]=false",
    ] {
        pair.script(&format!("probe_fn=function(){body} end"));
        assert!(compare_function_ease_probe(&mut pair).is_err());
        pair.script("saved.p:y(18);__songlua_probe_methods=nil;__songlua_probe_actors=nil;__songlua_probe_actor_set=nil");
    }
}

#[test]
fn function_ease_probe_keeps_first_return_through_authored_global_restoration() {
    let (_dir, mut pair) = function_ease_pair();
    pair.script(r#"
        __songlua_probe_methods=17
        setmetatable(_G,{__newindex=function(t,k,v)
            if k=='__songlua_probe_methods' and probe_return_weak then
                collectgarbage();collectgarbage();assert(probe_return_weak[1] and probe_return_weak[2]==nil);log('probe cleanup')
            end;rawset(t,k,v)
        end})
        probe_fn=function()
            rawset(_G,'__songlua_probe_methods',nil)
            local first,second={},{};probe_return_weak=setmetatable({first,second},{__mode='v'});return first,second
        end
    "#);
    let out = compare_function_ease_probe(&mut pair).unwrap();
    assert!(out.0.is_none());
    assert!(out.1.is_empty());
    assert_eq!(pair.compare(), "probe cleanup");
    pair.script("setmetatable(_G,nil);collectgarbage();collectgarbage();assert(probe_return_weak[1]==nil and probe_return_weak[2]==nil)");
}

#[test]
fn function_ease_probe_root_batch_keeps_unknown_tables_until_release_and_rejects_foreign_vms() {
    let (_dir, mut pair) = function_ease_pair();
    pair.script("probe_fn=function()local a={marker=7};__songlua_probe_actors[1]=a;probe_weak=setmetatable({a},{__mode='v'})end");
    let f = pair.rust.vm_mut().get_global(b"probe_fn").unwrap();
    let mut probe = pair.rust.probe_song_function_ease_target(f).unwrap();
    let mut other = SongRuntime::new(&pair.context).unwrap();
    assert!(other.song_function_ease_probe_actors(&probe).is_err());
    assert!(other.release_song_function_ease_probe(&mut probe).is_err());
    pair.rust
        .vm_mut()
        .execute(
            &compile_named(
                b"collectgarbage();collectgarbage();assert(probe_weak[1].marker==7)",
                b"=probe actor roots",
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        pair.rust
            .song_function_ease_probe_actors(&probe)
            .unwrap()
            .len(),
        1
    );
    assert!(
        pair.rust
            .release_song_function_ease_probe(&mut probe)
            .unwrap()
    );
    assert!(
        !pair
            .rust
            .release_song_function_ease_probe(&mut probe)
            .unwrap()
    );
    assert!(pair.rust.song_function_ease_probe_actors(&probe).is_err());
    pair.rust
        .vm_mut()
        .execute(
            &compile_named(
                b"collectgarbage();collectgarbage();assert(probe_weak[1]==nil)",
                b"=probe actor release",
            )
            .unwrap(),
        )
        .unwrap();
}

#[test]
fn function_ease_probe_instruction_abort_still_cleans_up_and_allows_a_later_probe() {
    let (_dir, mut pair) = function_ease_pair();
    pair.script("probe_fn=function()saved.p:x(33);while true do end end");
    let f = pair.rust.vm_mut().get_global(b"probe_fn").unwrap();
    let clock = pair.rust.values().unwrap();
    pair.rust.vm_mut().set_instruction_limit(Some(50_000));
    let mut out = pair.rust.probe_song_function_ease_target(f).unwrap();
    pair.rust.vm_mut().set_instruction_limit(None);
    assert!(out.target.is_none());
    assert_eq!(out.methods, ["player.x"]);
    assert_eq!(pair.rust.values().unwrap(), clock);
    pair.rust
        .release_song_function_ease_probe(&mut out)
        .unwrap();
    pair.rust.vm_mut().execute(&compile_named(b"assert(saved.p:GetX()==33);assert(__songlua_probe_methods==nil and __songlua_probe_actors==nil and __songlua_probe_actor_set==nil);probe_fn=function()saved.p:y(12)end",b"=probe abort cleanup").unwrap()).unwrap();
    let f = pair.rust.vm_mut().get_global(b"probe_fn").unwrap();
    let mut out = pair.rust.probe_song_function_ease_target(f).unwrap();
    assert_eq!(
        out.target,
        Some(deadsync_song_lua::SongLuaEaseTarget::PlayerY)
    );
    pair.rust
        .release_song_function_ease_probe(&mut out)
        .unwrap();
}

#[test]
fn function_ease_pipeline_compiles_player_targets_after_column_endpoints_and_probe() {
    let (_dir, mut pair) = function_ease_pair();
    pair.script("ease=nil;xero=nil;local count=0;eases_probe={{4,2,2,6,function(v)count=count+1;function_ease_count=count;saved.p:x(v);function_ease_random=math.random()end,'end','linear',1.9,3,4,5},{9,1,0,1,'drunk'}}");
    let out = compare_function_eases(&mut pair, &["a", "b"]).unwrap();
    assert_eq!(out.eases.len(), 2);
    assert!(out.overlay_eases.is_empty());
    assert_eq!(out.stats.function_targets, 1);
    assert_eq!(out.stats.overlay_capture_attempts, 0);
    assert_eq!(
        out.eases[0].target,
        deadsync_song_lua::SongLuaEaseTarget::PlayerX
    );
    pair.script(
        "assert(function_ease_count==3 and saved.p:GetX()==1);log('rng after ease',math.random())",
    );
}

#[test]
fn function_ease_pipeline_compiles_all_written_overlays_with_named_and_unnamed_setters() {
    let (_dir, mut pair) = function_ease_pair();
    pair.script("ease=nil;xero=nil;local count=0;eases_probe={{4,2,2,6,function(v)count=count+1;function_ease_count=count;saved.a:x(v);saved.b:diffusealpha(v/10)end,'len','linear',1,9,8,7}}");
    let out = compare_function_eases(&mut pair, &["a", "b"]).unwrap();
    assert!(out.eases.is_empty());
    assert_eq!(out.overlay_eases.len(), 2);
    assert_eq!(out.stats.overlay_capture_attempts, 1);
    assert_eq!(out.stats.overlay_capture_outputs, 2);
    assert_eq!(out.info.unsupported_function_eases, 0);
    pair.script("assert(function_ease_count==5 and saved.a:GetX()==1 and math.abs(saved.b:GetDiffuseAlpha()-.1)<.0001)");
}

#[test]
fn function_ease_pipeline_keeps_empty_and_unknown_probe_selections_on_touched_fallback() {
    for unknown in [false, true] {
        let (_dir, mut pair) = function_ease_pair();
        pair.script(if unknown {"ease=nil;xero=nil;eases_probe={{4,2,2,6,function(v)saved.n:x(v);saved.n:diffusealpha(v/10)end}}"} else {"ease=nil;xero=nil;eases_probe={{4,2,2,6,function(v)if v~=1 then saved.a:x(v)end end}}"});
        let out = compare_function_eases(&mut pair, &["a", "b"]).unwrap();
        if unknown {
            assert!(out.eases.is_empty());
            assert_eq!(out.info.unsupported_function_eases, 1);
            assert_eq!(out.stats.overlay_capture_attempts, 1);
        } else {
            assert_eq!(out.overlay_eases.len(), 1);
            assert_eq!(out.stats.overlay_capture_attempts, 1);
        }
    }
}

#[test]
fn function_ease_pipeline_column_windows_take_priority_over_overlay_writes() {
    let (_dir, mut pair) = function_ease_pair();
    column_endpoint_fixture(&mut pair);
    pair.script("ease=nil;xero=nil;local count=0;eases_probe={{4,2,2,6,function(v)count=count+1;function_ease_count=count;saved.a:x(v);column_pos:SetSplineMode('NoteColumnSplineMode_Offset');column_spline:SetSize(3);for i=1,3 do column_spline:SetPoint(i,{v,v,0})end end,'len','linear'}}");
    let out = compare_function_eases(&mut pair, &["a", "b"]).unwrap();
    assert_eq!(out.column_offsets.len(), 2);
    assert!(out.overlay_eases.is_empty());
    assert_eq!(out.stats.overlay_capture_attempts, 0);
    pair.script("assert(function_ease_count==2 and saved.a:GetX()==6 and column_spline.__songlua_spline_size==0)");
}

#[test]
fn function_ease_pipeline_continues_failed_endpoints_and_deduplicates_unsupported_details() {
    let (_dir, mut pair) = function_ease_pair();
    pair.script("ease=nil;xero=nil;local function bad(v)saved.a:x(v);if v==2 then error('endpoint failure')end end;eases_probe={{4,2,2,6,bad},{4,2,2,6,bad},{8,1,0,1,'after'}}");
    let out = compare_function_eases(&mut pair, &["a", "b"]).unwrap();
    assert_eq!(out.info.unsupported_function_eases, 2);
    assert_eq!(out.info.unsupported_function_ease_captures.len(), 1);
    assert_eq!(out.stats.overlay_capture_attempts, 2);
    assert_eq!(out.eases.len(), 1);
    pair.script("assert(saved.a:GetX()==1)");
}

#[test]
fn function_ease_pipeline_propagates_corrupt_probe_reads_and_accepts_existing_native_functions() {
    let (_dir, mut pair) = function_ease_pair();
    pair.script("ease=nil;xero=nil;eases_probe={{1,1,0,1,'before'},{4,2,2,6,function(v)saved.a:x(v);if type(__songlua_probe_methods)=='table'then __songlua_probe_methods[1]=false end end},{8,1,0,1,'unreached'}}");
    assert!(compare_function_eases(&mut pair, &["a", "b"]).is_err());
    pair.script("__songlua_probe_methods=nil;__songlua_probe_actors=nil;__songlua_probe_actor_set=nil;eases_probe={{4,2,2,6,math.abs},{8,1,0,1,'after-native'}}");
    let out = compare_function_eases(&mut pair, &["a", "b"]).unwrap();
    assert_eq!(out.info.unsupported_function_eases, 1);
    assert_eq!(out.eases.len(), 1);
    assert_eq!(out.stats.function_targets, 1);
}

#[test]
fn function_ease_probe_preserves_setter_error_preflight_before_semantic_writes() {
    for body in [
        "__songlua_probe_actors=false;saved.a:diffusealpha(.2)",
        "__songlua_probe_actor_set=false;saved.a:diffusealpha(.2)",
        "__songlua_probe_methods=false;saved.p:x(9)",
        "saved.p.__songlua_player_index=true;saved.p:x(9)",
    ] {
        let (_dir, mut pair) = function_ease_pair();
        pair.script("probe_before_alpha=saved.a:GetDiffuseAlpha();probe_before_x=saved.p:GetX();probe_before_player_index=saved.p.__songlua_player_index");
        pair.script(&format!("probe_fn=function(){body} end"));
        let out = compare_function_ease_probe(&mut pair).unwrap();
        assert!(out.0.is_none());
        assert!(out.1.is_empty());
        pair.script("saved.p.__songlua_player_index=probe_before_player_index;assert(saved.a:GetDiffuseAlpha()==probe_before_alpha and saved.p:GetX()==probe_before_x)");
    }
}

#[test]
fn function_ease_probe_preserves_partial_global_restoration_when_a_setter_fails() {
    let (_dir, mut pair) = function_ease_pair();
    pair.script(r#"
        __songlua_probe_methods='old methods';__songlua_probe_actors='old actors';__songlua_probe_actor_set='old set'
        setmetatable(_G,{__newindex=function(t,k,v)
            if k=='__songlua_probe_actors'then error('probe restore failure')end
            rawset(t,k,v)
        end})
        probe_fn=function()rawset(_G,'__songlua_probe_methods',nil);rawset(_G,'__songlua_probe_actors',nil)end
    "#);

    pair.rust.set_beat(7.).unwrap();
    assert!(compare_function_ease_probe(&mut pair).is_err());
    pair.script("assert(__songlua_probe_methods=='old methods' and __songlua_probe_actors==nil and type(__songlua_probe_actor_set)=='table');assert(GAMESTATE:GetSongBeat()==0);setmetatable(_G,nil);__songlua_probe_methods=nil;__songlua_probe_actor_set=nil;probe_fn=function()saved.p:y(3)end");
    assert_eq!(
        compare_function_ease_probe(&mut pair).unwrap().0,
        Some(deadsync_song_lua::SongLuaEaseTarget::PlayerY)
    );
}

#[test]
fn function_ease_probe_follows_native_lifecycle_capture_ownership() {
    for body in [
        "saved.a:AddWrapperState()",
        "saved.a:queuecommand('Empty')",
        "saved.a:sleep(.1)",
        "saved.a:linear(.1)",
        "saved.a:linear(.1):stoptweening()",
        "saved.a:linear(.1):finishtweening()",
        "saved.a:linear(.1):hurrytweening(.5)",
        "saved.a:stoptweening()",
        "saved.a:finishtweening()",
    ] {
        let (_dir, mut pair) = function_ease_pair();
        pair.script(&format!(
            "probe_case={body:?};probe_fn=function(){body} end"
        ));
        let out = compare_function_ease_probe(&mut pair).unwrap();
        assert!(out.0.is_none());
        assert!(out.1.is_empty());
    }
}

fn callback_tables_pair(path: &Path, label: &str) -> Pair {
    let entry = write(path, "default.lua", "return Def.ActorFrame{}");
    let mut pair = Pair::new(SongLuaCompileContext::new(path, label));
    pair.activate(&[&entry]);
    pair
}
fn callback_table_tags(
    runtime: &mut SongRuntime,
    batch: &deadsync_song_lua::SongCallbackTables,
) -> Vec<String> {
    let values = runtime.song_callback_tables(batch).unwrap();
    let key = runtime.vm_mut().try_intern(b"tag").unwrap();
    values
        .into_iter()
        .map(|v| {
            let Value::Table(t) = v else { panic!() };
            let Value::String(id) = runtime.vm_mut().raw_table_get(t, key).unwrap() else {
                panic!()
            };
            String::from_utf8(runtime.vm_mut().string_bytes(id).unwrap().to_vec()).unwrap()
        })
        .collect()
}
fn compare_callback_table_discovery(
    pair: &mut Pair,
    names: &[&str],
    mode: u8,
) -> Result<Vec<String>, String> {
    let root = pair.rust.vm_mut().get_global(b"callback_root").unwrap();
    let rust = match mode {
        0 => pair.rust.read_song_update_function_tables(root, names),
        1 => pair
            .rust
            .read_song_update_function_nested_tables(root, names),
        _ => {
            pair.rust
                .read_song_global_function_nested_tables("xero", &["definemod", "node"], names)
        }
    };
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_callback_table_discovery::eq1",
        &(rust.is_ok()),
    );
    let output = match rust {
        Ok(mut rust) => {
            let actual = callback_table_tags(&mut pair.rust, &rust);
            reference_test::assert_debug(
                "song/tests/rust_song_activation.rs::compare_callback_table_discovery::eq2",
                &(actual),
            );
            pair.rust.release_song_callback_tables(&mut rust).unwrap();
            Ok(actual)
        }
        Err(e) => Err(e.to_string()),
    };
    pair.compare();
    output
}

#[test]
fn callback_tables_keep_direct_nested_recurring_and_raw_child_discovery_order() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let mut pair = callback_tables_pair(dir.path(), "callback table discovery");
    pair.script(r#"
        local mods={tag='A'};local actions={tag='B'}
        local function leaf()return actions end
        local loop;loop=function()return mods,leaf,loop end
        local function child(tag)local mods={tag=tag};return {__songlua_update_function=function()return mods end}end
        callback_root={false,child('C'),nil,child('unreached')}
        callback_root.__songlua_update_function=function()return loop end
        callback_root.__songlua_recurring_update_command='Tick'
        callback_root.Tick=function()return actions end
    "#);
    assert_eq!(
        compare_callback_table_discovery(&mut pair, &["mods", "actions"], 0).unwrap(),
        ["B", "C"]
    );
    assert_eq!(
        compare_callback_table_discovery(&mut pair, &["mods", "actions"], 1).unwrap(),
        ["A", "B", "C"]
    );
}

#[test]
fn callback_tables_keep_original_global_namespace_and_authored_debug_getter() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let mut pair = callback_tables_pair(dir.path(), "global callback table discovery");
    pair.script(r#"
        local nodes={tag='first'};local function helper()return nodes end
        local function first()return helper end
        local function second()local unused=helper;return nodes,unused end
        xero=setmetatable({}, {__index=function(_,key)log('function',key);return key=='definemod' and first or second end})
        local original=debug.getupvalue
        debug.getupvalue=function(fn,i)
            log('getupvalue',i);xero=nil;collectgarbage()
            return original(fn,i)
        end
    "#);
    assert_eq!(
        compare_callback_table_discovery(&mut pair, &["nodes"], 2).unwrap(),
        ["first"]
    );
}

#[test]
fn callback_tables_preserve_optional_debug_lookup_and_typed_metadata_errors() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let mut pair = callback_tables_pair(dir.path(), "callback table lookup errors");
    pair.script("callback_debug=debug;callback_root={__songlua_update_function=true}");
    for mode in [0, 1] {
        pair.script("debug=nil");
        assert!(
            compare_callback_table_discovery(&mut pair, &["mods"], mode)
                .unwrap()
                .is_empty()
        );
        pair.script("debug=false");
        assert!(compare_callback_table_discovery(&mut pair, &["mods"], mode).is_err());
        pair.script("debug={getupvalue=false}");
        assert!(compare_callback_table_discovery(&mut pair, &["mods"], mode).is_err());
        pair.script("debug=callback_debug");
        assert!(compare_callback_table_discovery(&mut pair, &["mods"], mode).is_err());
        pair.script("callback_root={__songlua_recurring_update_command=42,['42']=false}");
        assert!(compare_callback_table_discovery(&mut pair, &["mods"], mode).is_err());
        pair.script("callback_root={__songlua_update_function=true}");
    }
    pair.script("xero=false;debug=nil");
    assert!(compare_callback_table_discovery(&mut pair, &["nodes"], 2).is_err());
    pair.script("callback_root=false;debug=false");
    assert!(
        compare_callback_table_discovery(&mut pair, &["mods"], 0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn callback_tables_global_name_predicate_keeps_literals_short_circuit_and_meter_stream() {
    let (_dir, mut pair) = command_scope_pair(
        r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.a=self end,
            Def.SongMeterDisplay{
                InitCommand=function(self)saved.meter=self end,
                Stream=Def.ActorFrame{OnCommand=function(self)self:SetUpdateFunction(function()return 'mod_perframes' end)end},
            },
        }
    "#,
    );
    pair.script(
        r#"
        callback_root={saved.meter,__songlua_recurring_update_command='Bad',Bad=true}
        local function nested()return 'prefix_globals' end
        callback_root.__songlua_update_function=function()return nested end
    "#,
    );
    for name in ["prefix_globals", "mod_perframes", "not_referenced_678"] {
        let root = pair.rust.vm_mut().get_global(b"callback_root").unwrap();
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::callback_tables_global_name_predicate_keeps_literals_short_circuit_and_meter_stream::eq1",
            &(pair.rust.song_update_tree_reads_global(root, name).unwrap()),
        );
    }
    pair.script("callback_root={__songlua_update_function=math.abs,__songlua_recurring_update_command='Bad',Bad=true}");

    let root = pair.rust.vm_mut().get_global(b"callback_root").unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::callback_tables_global_name_predicate_keeps_literals_short_circuit_and_meter_stream::eq2",
        &(pair
            .rust
            .song_update_tree_reads_global(root, "mods")
            .unwrap()),
    );

    pair.compare();
    for (source, expected) in [
        (
            "callback_root=setmetatable({__songlua_update_function=function()return 'mods' end},{__index=function()error('short circuit failed')end})",
            true,
        ),
        (
            "callback_root=setmetatable({__songlua_update_function=math.abs,__songlua_recurring_update_command='Bad'},{__index=function(_,key)if key=='Bad'then error('ignored handler lookup')end end})",
            false,
        ),
    ] {
        pair.script(source);

        let root = pair.rust.vm_mut().get_global(b"callback_root").unwrap();
        let actual = pair
            .rust
            .song_update_tree_reads_global(root, "mods")
            .unwrap();
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::callback_tables_global_name_predicate_keeps_literals_short_circuit_and_meter_stream::eq3",
            &actual,
        );
        assert_eq!(actual, expected);

        pair.compare();
    }
}

#[test]
fn callback_tables_root_detached_results_across_getters_gc_and_foreign_release() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let mut pair = callback_tables_pair(dir.path(), "callback result roots");
    pair.script(
        r#"
        local a,b=1,2
        callback_root={__songlua_update_function=function()return a,b end}
        callback_weak=setmetatable({}, {__mode='v'})
        debug.getupvalue=function(_,i)
            collectgarbage();collectgarbage()
            local result={tag=tostring(i)};callback_weak[i]=result
            return 'mods',result
        end
    "#,
    );

    let root = pair.rust.vm_mut().get_global(b"callback_root").unwrap();
    let mut batch = pair
        .rust
        .read_song_update_function_tables(root, &["mods"])
        .unwrap();
    pair.script("callback_root=nil;debug=nil;collectgarbage();collectgarbage();assert(callback_weak[1] and callback_weak[2])");
    assert_eq!(callback_table_tags(&mut pair.rust, &batch), ["1", "2"]);
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::callback_tables_root_detached_results_across_getters_gc_and_foreign_release::eq2",
        &(["1", "2"]),
    );
    let mut foreign = SongRuntime::new(&pair.context).unwrap();
    assert!(foreign.song_callback_tables(&batch).is_err());
    assert!(foreign.release_song_callback_tables(&mut batch).is_err());
    assert!(pair.rust.release_song_callback_tables(&mut batch).unwrap());
    assert!(!pair.rust.release_song_callback_tables(&mut batch).unwrap());
    assert!(pair.rust.song_callback_tables(&batch).is_err());

    pair.script(
        "collectgarbage();collectgarbage();assert(callback_weak[1]==nil and callback_weak[2]==nil)",
    );
}

#[test]
fn callback_tables_mod_windows_keep_native_coercions_holes_spans_and_time_units() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "mod window tables"));
    pair.script(
        r#"
        callback_mods={false,{'1.5','-2','50% drunk','END',1.75},{3,4,'100% dizzy',' end','2'},
            {'inf','NaN','nonfinite strings'}, {1,2,99}, {'bad',2,'bad'},
            {1,2,string.char(255)}, nil, {20,4,'unreached'}}
    "#,
    );
    for unit in [
        deadsync_song_lua::SongLuaTimeUnit::Beat,
        deadsync_song_lua::SongLuaTimeUnit::Second,
        deadsync_song_lua::SongLuaTimeUnit::BeatClock,
    ] {
        let table = pair.rust.vm_mut().get_global(b"callback_mods").unwrap();
        let rust = pair.rust.read_song_mod_windows(Some(table), unit).unwrap();
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::callback_tables_mod_windows_keep_native_coercions_holes_spans_and_time_units::eq1",
            &(activation_snapshot::text(format!("{rust:?}"))),
        );
        assert_eq!(rust.len(), 3);
    }
}

#[test]
fn callback_tables_runtime_mods_keep_first_unique_order_player_state_and_static_selection() {
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let mut pair = Pair::new(SongLuaCompileContext::new(dir.path(), "runtime mod tables"));
    pair.script(
        r#"
        ease={linear=function(x)return x end}
        local same={2,1,ease.linear,0.5,'zoom',add=true}
        callback_mods={same,same,{1,3,'linear',0.25,'ZOOM',plr=1,startVal=2,opt1='1.5',opt2=4},
            {10,12,'linear',0.4,'static',timing='END'}, {20,1,'linear',0.1,'static',plr=2,add=0},
            {30,1,'linear',4,'x'}, {40,2,'linear',0.75,'Drunk',plr=0,pn=2},
            {50,2,'linear',2,'rotationx',plr='2'}, {'NaN',1,'linear',2,'bad'},
            {1,0,'linear','inf','bad'}, {1,2,{},4,'bad'}}
        for _,name in ipairs{'z','rotationy','rotationz','zoomx','zoomy','zoomz','y'}do
            callback_mods[#callback_mods+1]={60,1,'linear',3,name,plr=1}
        end
    "#,
    );
    pair.context.players[0].enabled = false;
    pair.context.players[1].enabled = true;
    let mut names = easing_reader_names(&mut pair);

    let table = pair.rust.vm_mut().get_global(b"callback_mods").unwrap();
    let context = pair.context.clone();
    let rust = pair
        .rust
        .read_song_runtime_mod_eases(Some(table), &names, Some(7), &context)
        .unwrap();
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::callback_tables_runtime_mods_keep_first_unique_order_player_state_and_static_selection::eq1",
        &(rust),
    );
    assert_eq!(rust.1.len(), 2);
    pair.rust.release_song_easing_names(&mut names).unwrap();
}

fn compare_static_schedule(
    source: &str,
    expect_error: bool,
) -> (
    tempfile::TempDir,
    Pair,
    deadsync_song_lua::SongStaticSchedule,
) {
    use deadsync_song_lua::{SongMessageTarget, SongStaticInputs};
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(dir.path(), "default.lua", source);
    let context = SongLuaCompileContext::new(dir.path(), "static schedule compiler");
    let mut pair = Pair::new(context.clone());

    let plan = SongLayerPlan::new(&[&path], 0, &context).unwrap();
    let root = pair
        .rust
        .initialize_song_layers(pair.scene, &plan)
        .unwrap()
        .unwrap();
    let pin = pair.rust.vm_mut().pin(root).unwrap();
    let activation = pair
        .rust
        .activate_song_layers(pair.scene, &context)
        .unwrap()
        .unwrap();
    pair.rust
        .discover_song_startup_commands(pair.scene, &activation)
        .unwrap();
    let mut output =
        test_support::static_schedule_startup_output(&mut pair.rust, &activation, &context);
    pair.rust.end_actor_capture();
    let mut names = pair.rust.new_song_easing_names().unwrap();
    pair.rust
        .register_loaded_song_easing_names(&mut names)
        .unwrap();
    let mut skipped = Vec::new();
    let mut rows = pair
        .rust
        .read_song_preparation_overlay_actors::<(), ()>(
            root,
            &context,
            |_, _, _| Ok(std::sync::Arc::from([])),
            |_| None,
            |s| skipped.push(s),
        )
        .unwrap();
    for detail in skipped {
        deadsync_song_lua::push_unique_compile_detail(
            &mut output.info.skipped_message_command_captures,
            detail,
        );
    }
    let mut values = rows
        .actors
        .iter()
        .map(|r| pair.rust.song_overlay_actor_value(r).unwrap())
        .collect::<Vec<_>>();
    let screen = pair
        .rust
        .vm_mut()
        .get_global(b"__songlua_top_screen")
        .unwrap();
    values.push(screen);
    let mut commands = rows
        .actors
        .iter()
        .map(|r| r.actor.message_commands.clone())
        .collect::<Vec<_>>();
    commands.push(Vec::new());
    {
        let mut targets = values
            .iter()
            .zip(&mut commands)
            .map(|(&actor, commands)| SongMessageTarget { actor, commands })
            .collect::<Vec<_>>();
        pair.rust
            .capture_song_stable_cross_actor_message_commands(&mut targets, |s| {
                deadsync_song_lua::push_unique_compile_detail(
                    &mut output.info.skipped_message_command_captures,
                    s,
                )
            })
            .unwrap();
    }
    let mut tracked = pair
        .rust
        .read_song_tracked_actors(pair.scene)
        .unwrap()
        .unwrap();
    let tracked_values = tracked
        .actors
        .iter()
        .map(|r| pair.rust.song_tracked_actor_value(r).unwrap())
        .collect::<Vec<_>>();
    let mut tracked_commands = tracked
        .actors
        .iter()
        .map(|r| r.actor.message_commands.clone())
        .collect::<Vec<_>>();
    let mut stages = Vec::new();
    let result = {
        let mut targets = values
            .iter()
            .zip(&mut commands)
            .map(|(&actor, commands)| SongMessageTarget { actor, commands })
            .collect::<Vec<_>>();
        let mut tracked = tracked_values
            .iter()
            .zip(&mut tracked_commands)
            .map(|(&actor, commands)| SongMessageTarget { actor, commands })
            .collect::<Vec<_>>();
        pair.rust.compile_song_static_schedule(
            SongStaticInputs {
                root,
                initialized_globals: activation.initialized_globals,
                names: &names,
                context: &context,
                static_overlay: None,
            },
            &mut targets,
            &mut tracked,
            &mut output,
            |s| stages.push(s),
        )
    };
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_static_schedule::eq1",
        &(result.is_err()),
    );
    assert_eq!(result.is_err(), expect_error);
    if let Ok(mut tables) = result {
        let check = pair
            .rust
            .vm_mut()
            .get_global(b"__migration_static_boundary")
            .unwrap();
        if check != Value::Nil {
            pair.rust.vm_mut().call(check, &[]).unwrap();
        }
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_static_schedule::eq3",
            &(activation_snapshot::text(format!("{output:?}"))),
        );
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_static_schedule::eq4",
            &(activation_snapshot::text(format!("{commands:?}"))),
        );
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_static_schedule::eq5",
            &(activation_snapshot::text(format!("{tracked_commands:?}"))),
        );
        reference_test::assert_debug(
            "song/tests/rust_song_activation.rs::compare_static_schedule::eq6",
            &([
                tables.prefix_is_runtime,
                tables.global_perframes_are_runtime,
                tables.global_mods_are_runtime,
                tables.global_mod_eases_are_runtime,
                tables.global_actions_are_runtime,
            ]),
        );
        let perframes = pair.rust.song_static_perframe_tables(&tables).unwrap();
        let key = pair.rust.vm_mut().try_intern(b"tag").unwrap();
        for rust in perframes {
            let rust = rust.map(|v| {
                let Value::Table(t) = v else { panic!() };
                let Value::String(id) = pair.rust.vm_mut().raw_table_get(t, key).unwrap() else {
                    panic!()
                };
                String::from_utf8(pair.rust.vm_mut().string_bytes(id).unwrap().to_vec()).unwrap()
            });
            reference_test::assert_debug(
                "song/tests/rust_song_activation.rs::compare_static_schedule::eq7",
                &(rust),
            );
        }
        let mut foreign = SongRuntime::new(&context).unwrap();
        assert!(foreign.song_static_perframe_tables(&tables).is_err());
        assert!(foreign.release_song_static_tables(&mut tables).is_err());
        assert!(pair.rust.release_song_static_tables(&mut tables).unwrap());
        assert!(!pair.rust.release_song_static_tables(&mut tables).unwrap());
        assert!(pair.rust.song_static_perframe_tables(&tables).is_err());
        assert_eq!(stages.last(), Some(&"global_actions"));
    }
    pair.compare();
    pair.rust.release_song_easing_names(&mut names).unwrap();
    pair.rust.release_song_overlay_actors(&mut rows).unwrap();
    pair.rust.release_song_tracked_actors(&mut tracked).unwrap();
    pair.rust.vm_mut().unpin(pin).unwrap();
    (dir, pair, output)
}

#[test]
fn static_schedule_composes_prefix_global_mods_eases_actions_and_perframe_inputs() {
    let (_dir, mut pair, output) = compare_static_schedule(
        r#"
        ease={linear=function(v)return v end}
        prefix_globals={perframes={tag='prefix'},mods={{1,2,'50% drunk'}},
            ease={{2,1,0,1,'dizzy','linear'}},actions={{3,'Prefix',true}}}
        mod_perframes={tag='global'}
        mods={{4,2,'100% stealth'},{5,2,ease.linear,2,'zoom',plr=2}}
        mod_time={{6,9,'25% dizzy','end',1}}
        mods_ease={{7,2,0,1,'drunk','linear',2}}
        mod_actions={{8,'Global',false},{9,function()saved.a:x(12)end,true}}
        return Def.ActorFrame{Def.Quad{InitCommand=function(self)saved.a=self end}}
    "#,
        false,
    );
    assert_eq!(output.beat_mods.len(), 2);
    assert_eq!(output.time_mods.len(), 1);
    assert_eq!(output.counter, 1);
    pair.script("assert(saved.a:GetX()==0)");
}

#[test]
fn static_schedule_skips_reader_owned_tables_by_identity_and_initialized_presence() {
    let (_dir, _pair, output) = compare_static_schedule(
        r#"
        local mods_ease={{1,1,0,1,function()error('must not probe')end}}
        local perframes={tag='runtime'}
        return Def.ActorFrame{
            InitCommand=function(self)
                mods={{1,2,'must not compile'}};mod_actions={{2,'must not emit'}}
                _G.mods_ease=mods_ease
                self:SetUpdateFunction(function()
                    local a=mods;local b=mod_actions
                    return mods_ease,perframes,a,b
                end)
            end,
            OnCommand=function()mods={{9,1,'replacement'}};mod_perframes=perframes end,
        }
    "#,
        false,
    );
    assert!(output.beat_mods.is_empty());
    assert!(output.eases.is_empty());
    assert_eq!(output.info.unsupported_function_eases, 0);
    let (_dir, _pair, output) = compare_static_schedule(
        r#"
        prefix_globals={mods={{1,1,'must skip'}},actions={{1,'must skip'}},perframes={tag='unused'}}
        return Def.ActorFrame{InitCommand=function(self)self:SetUpdateFunction(function()return prefix_globals end)end}
    "#,
        false,
    );
    assert!(output.beat_mods.is_empty());
    assert!(!output.messages.iter().any(|e| e.message == "must skip"));
}

#[test]
fn static_schedule_keeps_prefix_lookup_order_soft_perframe_errors_and_fresh_globals() {
    let (_dir, mut pair, output) = compare_static_schedule(
        r#"
        events={};function trace(k)events[#events+1]=k end
        local prefix=setmetatable({}, {__index=function(_,key)
            trace(key)
            if key=='perframes'then error('optional perframes')end
            if key=='mods'then return {{1,2,'50% drunk'}}end
            if key=='ease'then return {{2,1,0,1,'dizzy'}}end
            if key=='actions'then mods={{3,2,'25% stealth'}};return {{4,'Prefix'}}end
        end})
        rawset(_G,'prefix_globals',nil)
        setmetatable(_G,{
            __index=function(_,key)if key=='prefix_globals'then return prefix end end,
            __newindex=function(t,key,value)if key~='prefix_globals'then rawset(t,key,value)end end,
        })
        mod_perframes={tag='global'}
        return Def.ActorFrame{}
    "#,
        false,
    );
    assert_eq!(output.beat_mods.len(), 2);
    assert_eq!(pair.compare(), "perframes|mods|ease|actions");
    let (_dir, _pair, output) = compare_static_schedule(
        r#"
        local frame={tag='raw'}
        prefix_globals=setmetatable({mods={{1,2,'50% drunk'}},perframes=frame},{__index=function()error('metatable must be dropped')end})
        mod_perframes=frame
        __migration_static_boundary=function()
            assert(getmetatable(prefix_globals)==nil)
            assert(prefix_globals.perframes~=mod_perframes)
        end
        return Def.ActorFrame{}
    "#,
        false,
    );
    assert_eq!(output.beat_mods.len(), 1);
}

#[test]
fn static_schedule_xero_nodes_keep_first_winner_sparse_pairs_state_and_capture_dedup() {
    let (_dir, mut pair, output) = compare_static_schedule(
        r#"
        local calls=0
        local nodes={{{'custom'},nil,function(v)calls=calls+1;saved.a:x(v);xero_calls=calls end},
            {{'CUSTOM'},nil,function()error('later node must not win')end}}
        local eases={{2,1,'linear',.25,'drunk',3,'custom',false,'bad',4,'unknown'},
            {4,1,'linear',1,'custom',relative=true,plr=2,time=0},
            {6,2,'linear',.5,'zoomx',plr=1}, {8,1,'linear',2,'custom'}}
        xero={definemod=function()return nodes end}
        return Def.ActorFrame{
            InitCommand=function(self)self:SetUpdateFunction(function()return eases,nodes end)end,
            Def.Quad{InitCommand=function(self)saved.a=self end},
        }
    "#,
        false,
    );
    assert!(!output.eases.is_empty());
    assert_eq!(output.overlay_eases.len(), 4);
    pair.script("assert(xero_calls==12)");
}

#[test]
fn static_schedule_xero_captures_retain_future_functions_and_release_parsed_tables() {
    let (_dir, _pair, output) = compare_static_schedule(
        r#"
        weak=setmetatable({}, {__mode='v'});local nodes,eases=1,2;local round=0
        prefix_globals=setmetatable({}, {__index=function(_,key)if key=='perframes'then round=0 end end})
        return Def.ActorFrame{
            InitCommand=function(self)self:SetUpdateFunction(function()return nodes,eases end)end,
            OnCommand=function()
                debug.getupvalue=function(_,i)
                    if i==1 then round=round+1 end
                    if round==5 and i==1 then
                        local unused=function()end;weak.unused=unused
                        local first=function(v)
                            collectgarbage();collectgarbage()
                            assert(weak.unused==nil and weak.nodes==nil and weak.eases==nil)
                            assert(weak.future~=nil);saved.a:x(v)
                        end
                        local future=function(v)saved.a:y(v)end;weak.future=future
                        local result={{{'first'},nil,first},{{'future'},nil,future},{{'unused'},nil,unused}}
                        weak.nodes=result;return 'nodes',result
                    end
                    if round==6 and i==2 then
                        local result={{1,1,'linear',3,'first'},{2,1,'linear',4,'future'}}
                        weak.eases=result;return 'eases',result
                    end
                    return nil,nil
                end
            end,
            Def.Quad{InitCommand=function(self)saved.a=self end},
        }
    "#,
        false,
    );
    assert_eq!(output.overlay_eases.len(), 2);
}

#[test]
fn static_schedule_retains_selected_perframe_tables_after_global_replacement() {
    let (_dir, _pair, _) = compare_static_schedule(
        r#"
        local frames={tag='selected-prefix'};local global={tag='selected-global'}
        prefix_globals={perframes=frames};mod_perframes=global
        __migration_static_boundary=function()collectgarbage();collectgarbage()end
        setmetatable(_G,{__index=function(_,key)
            if key=='mods_ease'then prefix_globals=nil;mod_perframes=nil;collectgarbage()end
        end})
        return Def.ActorFrame{}
    "#,
        false,
    );
}

#[test]
fn static_schedule_hard_errors_preserve_partial_script_effects_and_clean_roots() {
    let (_dir, mut pair, _) = compare_static_schedule(
        r#"
        events={}
        local prefix=setmetatable({mods={{1,2,'50% drunk'}}},{__index=function(_,key)
            events[#events+1]=key
            if key=='perframes'then return nil end
            if key=='ease'then return false end
        end})
        rawset(_G,'prefix_globals',nil)
        setmetatable(_G,{
            __index=function(_,key)if key=='prefix_globals'then return prefix end end,
            __newindex=function(t,key,value)if key~='prefix_globals'then rawset(t,key,value)end end,
        })
        return Def.ActorFrame{}
    "#,
        true,
    );
    assert_eq!(pair.compare(), "perframes|ease");
    pair.script("collectgarbage();collectgarbage();log('after error')");
}

type PerframeOutput = (
    Vec<deadsync_song_lua::SongLuaEaseWindow>,
    Vec<deadsync_song_lua::SongLuaOverlayEase>,
    deadsync_song_lua::SongLuaCompileInfo,
);
fn compare_sampled_perframes(
    source: &str,
    commands: Vec<Vec<deadsync_song_lua::SongLuaOverlayMessageCommand>>,
    messages: Vec<deadsync_song_lua::SongLuaMessageEvent>,
    expect_error: bool,
) -> (tempfile::TempDir, Pair, Option<PerframeOutput>) {
    use deadsync_song_lua::SongMessageTarget;
    let dir = tempfile::tempdir().unwrap();
    reference_test::register_path(dir.path());
    activation_snapshot::register_path(dir.path());
    let path = write(dir.path(), "default.lua", source);
    let mut context = SongLuaCompileContext::new(dir.path(), "sampled perframes");
    context.song_music_rate = 1.5;
    let mut pair = Pair::new(context.clone());
    drop(pair.activate(&[&path]));
    pair.rust.end_actor_capture();

    let mut rust_values = Vec::new();
    for key in ["actor", "quad"] {
        let rust = saved(&mut pair, key);
        rust_values.push(Value::Table(rust));
    }
    let mut tracked = pair
        .rust
        .read_song_tracked_actors(pair.scene)
        .unwrap()
        .unwrap();

    let rust_tables = pair
        .rust
        .vm_mut()
        .execute(
            &compile_named(
                b"return prefix_globals and prefix_globals.perframes, mod_perframes",
                b"=perframe selection",
            )
            .unwrap(),
        )
        .unwrap()
        .to_vec();

    let mut rust_commands = commands;

    let rust = {
        let targets = rust_values
            .iter()
            .zip(&mut rust_commands)
            .map(|(&actor, commands)| SongMessageTarget { actor, commands })
            .collect::<Vec<_>>();
        test_support::compile_perframes_for_tables(
            &mut pair.rust,
            [0, 1].map(|i| (rust_tables[i] != Value::Nil).then_some(rust_tables[i])),
            &context,
            &targets,
            &tracked,
            &messages,
        )
    };
    reference_test::assert_debug(
        "song/tests/rust_song_activation.rs::compare_sampled_perframes::eq1",
        &(rust.is_err()),
    );
    assert_eq!(rust.is_err(), expect_error);
    let output = match rust {
        Ok(rust) => {
            reference_test::assert_debug(
                "song/tests/rust_song_activation.rs::compare_sampled_perframes::eq3",
                &(activation_snapshot::text(format!("{rust:?}"))),
            );
            Some(rust)
        }
        Err(rust) => {
            assert!(rust.contains("perframe failure"), "{rust}");
            None
        }
    };
    pair.compare();
    pair.rust.release_song_tracked_actors(&mut tracked).unwrap();
    (dir, pair, output)
}
fn perframe_source(body: &str) -> String {
    format!(
        r#"
        {body}
        return Def.ActorFrame{{
            InitCommand=function(self)saved.actor=self end,
            Def.Quad{{InitCommand=function(self)saved.quad=self end}},
        }}
    "#
    )
}

#[test]
fn sampled_perframes_keep_prefix_global_order_overlaps_raw_holes_and_player_curves() {
    let source = perframe_source(
        r#"
        local first=true
        prefix_globals={perframes={
            {0,0.25,function(beat,dt)
                saved.actor:x(beat*10)
                SCREENMAN:GetTopScreen():GetChild('PlayerP1'):x(beat*20):zoomx(1+beat)
                    :rotationx(beat):rotationy(beat*2):rotationz(beat*3)
                if first then log('prefix',beat,dt);first=false end
            end},
            false,{2,1,function()error('invalid span')end},
        }}
        mod_perframes={
            {'0.1','0.35',function(beat,dt)
                saved.quad:y(saved.actor:GetX()+beat)
                SCREENMAN:GetTopScreen():GetChild('PlayerP2'):z(beat*3):y(beat*5)
                    :zoom(1+beat/2):zoomz(1-beat):skewx(beat):skewy(-beat)
            end},
            {0,0,function()error('zero span')end},
            {0/0,1,function()error('nan')end},
            {0,1,false},
            [6]={0,1,function()error('past hole')end},
        }
    "#,
    );
    let (_, _, output) = compare_sampled_perframes(&source, vec![vec![], vec![]], vec![], false);
    let output = output.unwrap();
    assert!(!output.0.is_empty());
    assert!(!output.1.is_empty());
    assert_eq!(output.2.unsupported_perframes, 0);
    let source = perframe_source(
        r#"
        mod_perframes={{0,0.02,function()
            SCREENMAN:GetTopScreen():GetChild('PlayerP1'):x(0):y(0):z(0)
                :rotationx(0):rotationy(0):rotationz(0):zoom(1):skewx(0):skewy(0)
        end}}
    "#,
    );
    compare_sampled_perframes(&source, vec![vec![], vec![]], vec![], false);
}

#[test]
fn sampled_perframes_keep_gap_holds_and_side_effect_only_diagnostics() {
    let source = perframe_source(
        r#"
        mod_perframes={
            {0,0.1,function(beat)saved.quad:x(beat*30)end},
            {0.2,0.3,function(beat)saved.quad:y(beat*20)end},
        }
    "#,
    );
    let (_, _, output) = compare_sampled_perframes(&source, vec![vec![], vec![]], vec![], false);
    assert!(
        output
            .unwrap()
            .1
            .iter()
            .any(|e| e.start == 0.1 && e.limit == 0.1)
    );
    for (body, unsupported) in [
        (
            "mod_perframes={{0,0.1,function()end},{0,0.1,function()end}}",
            2,
        ),
        (
            "mod_perframes={{0,0.1,function()SCREENMAN:SystemMessage('perframe')end}}",
            0,
        ),
        ("mod_perframes={false,{0,0,false}}", 0),
    ] {
        let (_, _, output) =
            compare_sampled_perframes(&perframe_source(body), vec![vec![], vec![]], vec![], false);
        assert_eq!(output.unwrap().2.unsupported_perframes, unsupported);
    }
}

#[test]
fn sampled_perframes_replay_messages_in_stable_order_and_filter_queued_blocks() {
    use deadsync_song_lua::{
        SongLuaMessageEvent as Event, SongLuaOverlayCommandBlock as Block,
        SongLuaOverlayMessageCommand as Command, SongLuaOverlayStateDelta as Delta,
    };
    let command = |message: &str, x: f32, queued: bool| Command {
        message: message.into(),
        frame_advance: 0.,
        blocks: vec![Block {
            progress: None,
            queued,
            start: 0.,
            duration: 0.5,
            easing: Some("linear".into()),
            opt1: None,
            opt2: None,
            delta: Delta {
                x: Some(x),
                ..Default::default()
            },
        }],
    };
    let source = perframe_source(
        r#"
        mod_perframes={{0,0.4,function()saved.quad:x(saved.actor:GetX()*2)end}}
    "#,
    );
    let (_, mut pair, output) = compare_sampled_perframes(
        &source,
        vec![
            vec![
                command("First", 20., false),
                command("Second", 30., false),
                command("Ignored", 99., true),
            ],
            vec![],
        ],
        vec![
            Event {
                beat: 0.,
                message: "First".into(),
                persists: false,
            },
            Event {
                beat: 0.,
                message: "Second".into(),
                persists: false,
            },
            Event {
                beat: 0.2,
                message: "Ignored".into(),
                persists: false,
            },
        ],
        false,
    );
    assert!(!output.unwrap().1.is_empty());
    pair.script("log(saved.actor:GetX(),saved.quad:GetX())");
    pair.compare();

    let source = r#"
        mod_perframes={{0,0.1,function()saved.quad:x(rawget(saved.actor,'__songlua_state_x') or 0)end}}
        return Def.ActorFrame{
            InitCommand=function()saved.actor={__songlua_actor_type='Actor'}end,
            Def.Quad{InitCommand=function(self)saved.quad=self end},
        }
    "#;
    let (_, _, output) = compare_sampled_perframes(
        source,
        vec![vec![command("First", 20., false)], vec![]],
        vec![Event {
            beat: 0.,
            message: "First".into(),
            persists: false,
        }],
        false,
    );
    assert!(!output.unwrap().1.is_empty());

    for clock in ["timer", "beat"] {
        let source = perframe_source(
            r#"
            mod_perframes={{0,0.2,function()
                saved.quad:x(saved.actor:GetSecsIntoEffect()):y(saved.actor:GetEffectDelta())
            end}}
        "#,
        )
        .replace(
            "saved.actor=self end",
            &format!("saved.actor=self;self:bob():effectclock('{clock}') end"),
        );
        let mut restart = command("Restart", 0., false);
        restart.blocks[0].duration = 0.;
        restart.blocks[0].delta = Delta {
            effect_mode: Some(deadlib_present::anim::EffectMode::Bounce),
            ..Default::default()
        };
        compare_sampled_perframes(
            &source,
            vec![vec![restart], vec![]],
            vec![Event {
                beat: 0.05,
                message: "Restart".into(),
                persists: false,
            }],
            false,
        );
    }
}

#[test]
fn sampled_perframes_restore_clocks_after_errors_and_keep_partial_script_writes() {
    let source = perframe_source(
        r#"
        mod_perframes={{0,0.1,function(beat,dt)
            saved.actor:x(17);saved.partial={beat,dt};collectgarbage()
            log('callback',GAMESTATE:GetSongBeat(),GAMESTATE:GetCurMusicSeconds(),beat,dt)
            error('perframe failure')
        end}}
    "#,
    );
    let (_, mut pair, _) = compare_sampled_perframes(&source, vec![vec![], vec![]], vec![], true);
    pair.script("collectgarbage();log(saved.actor:GetX(),saved.partial[1],saved.partial[2],GAMESTATE:GetSongBeat(),GAMESTATE:GetCurMusicSeconds())");
    pair.compare();
}

#[test]
fn sampled_perframes_root_future_callbacks_and_release_consumed_inputs_after_gc() {
    let source = r#"
        return Def.ActorFrame{
            InitCommand=function(self)saved.actor=self end,
            OnCommand=function()
                weak=setmetatable({}, {__mode='v'})
                local first=true
                local future=function(beat)saved.quad:x(beat)end
                weak.future=future
                mod_perframes={{0,0.1,function()
                    mod_perframes=nil;prefix_globals=nil;collectgarbage()
                    assert(weak.inputs==nil,'consumed input table retained')
                    assert(weak.future,'future callback lost')
                    if first then log('retained');first=false end
                end},{0,0.1,future}}
                weak.inputs=mod_perframes
            end,
            Def.Quad{InitCommand=function(self)saved.quad=self end},
        }
    "#;
    let (_, mut pair, output) =
        compare_sampled_perframes(source, vec![vec![], vec![]], vec![], false);
    assert!(!output.unwrap().1.is_empty());
    pair.script("collectgarbage();collectgarbage();assert(weak.future==nil,'consumed callback retained');log('released')");
    pair.compare();
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;

#[path = "support/activation_snapshot.rs"]
mod activation_snapshot;
