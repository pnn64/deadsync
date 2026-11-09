use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime, SongLayerPlan, test_support},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Pair {
    rust: RustSongRuntime,
    scene: ActorScene,
    context: SongLuaCompileContext,
}
impl Pair {
    fn new(context: SongLuaCompileContext) -> Self {
        reference_test::register_path(&context.song_dir);

        let mut rust = RustSongRuntime::new(&context).unwrap();
        assert!(rust.begin_song_compilation().unwrap());
        let (scene, selected) = rust.construct_gameplay_screen(&context).unwrap();
        assert!(rust.load_gameplay_theme(scene, &selected).unwrap());
        let mut p = Self {
            rust,
            scene,
            context,
        };
        p.script(
            r#"events={}; saved={}; mod_actions=false
            function log(...)
                local t={};for i=1,select('#',...)do t[i]=tostring(select(i,...))end
                events[#events+1]=table.concat(t,',')
            end"#,
        );
        p
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(
                &compile_named(source.as_bytes(), b"@/fixture/song-layer-observation.lua").unwrap(),
            )
            .unwrap();
        self.compare();
    }
    fn compare(&mut self) {
        let values = self
            .rust
            .vm_mut()
            .execute(&compile_named(b"return table.concat(events,'|')", b"=observations").unwrap())
            .unwrap()
            .to_vec();
        let Value::String(value) = values[0] else {
            panic!()
        };
        reference_test::assert_debug(
            "song/tests/rust_song_layers.rs::observations",
            &self.rust.vm_mut().string_bytes(value).unwrap(),
        );
    }
    fn load(&mut self, paths: &[PathBuf]) -> Value {
        let paths = paths.iter().map(PathBuf::as_path).collect::<Vec<_>>();
        let plan = SongLayerPlan::new(&paths, 0, &self.context).unwrap();
        let value = self
            .rust
            .initialize_song_layers(self.scene, &plan)
            .unwrap()
            .unwrap();
        self.compare();
        value
    }
}
fn write(dir: &Path, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, source).unwrap();
    path
}
#[test]
fn authored_sources_parenting_init_and_foreground_attachment_match_production() {
    for live in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let source = r#"
            local source=debug.getinfo(1,'S').source
            log('load',source,__songlua_current_script_path,__songlua_script_dir)
            local count=(loaded_count or 0)+1;loaded_count=count
            return Def.ActorFrame{
                Name='authored',
                InitCommand=function(self)
                    saved[count]=self
                    local p=self:GetParent()
                    log('init',count,self:GetName(),p and p:GetName(),p and #p,SCREENMAN:GetTopScreen()==nil)
                    log('provenance',self._Source,self._Dir,self._Line)
                    self.Name='from-init';self:queuecommand('Later')
                    collectgarbage()
                end,
                OnCommand=function(self) log('unexpected-on') end,
                LaterCommand=function(self) log('unexpected-later') end,
            }
        "#;
        let background = write(dir.path(), "background.lua", source);
        write(dir.path(), "front/default.lua", source);
        let last = write(dir.path(), " last.lua ", source);
        let mut context = SongLuaCompileContext::new(dir.path(), "authored layer construction");
        context.live = live;
        context.screen_loading = true;
        context.background_layer_count = 1;
        context.foreground_names = vec!["./front".into(), " last.lua ".into()];
        context.layer_start_beats = vec![-8., 4., 8.];
        let mut pair = Pair::new(context);
        let roots = pair.load(&[background, dir.path().join("front"), last]);
        assert_eq!(
            pair.rust
                .vm_mut()
                .raw_table_len(match roots {
                    Value::Table(t) => t,
                    _ => panic!(),
                })
                .unwrap(),
            3
        );
        pair.script(r#"
            local top=__songlua_top_screen
            local fg=top:GetChild('SongForeground')
            log('children',#fg,fg[1]==saved[2],fg[2]==saved[3])
            for i,a in ipairs(saved)do log('after',i,a:GetName(),a:GetVisible(),a:GetParent() and a:GetParent():GetName(),a.__songlua_defer_on,a.__songlua_live_inactive)end
            assert(not table.concat(events,'|'):find('unexpected',1,true))
        "#);
    }
}
#[test]
fn live_load_errors_and_scalar_returns_keep_prior_side_effects_and_dummy_layers() {
    let dir = tempfile::tempdir().unwrap();
    let bad = write(
        dir.path(),
        "bad.lua",
        "load_count=(load_count or 0)+1; log('failed-file',load_count); error('expected load failure')",
    );
    let scalar = write(
        dir.path(),
        "scalar.lua",
        "log('scalar-file',load_count); return 41",
    );
    let mut context = SongLuaCompileContext::new(dir.path(), "live fallback");
    context.live = true;
    let mut pair = Pair::new(context);
    let roots = pair.load(&[bad, scalar]);
    let Value::Table(roots) = roots else { panic!() };
    for i in 1..=2 {
        assert!(matches!(
            pair.rust
                .vm_mut()
                .raw_table_get(roots, Value::Number(i as f64))
                .unwrap(),
            Value::Table(_)
        ));
    }
    pair.script("local fg=__songlua_top_screen:GetChild('SongForeground'); log('fallbacks',#fg,fg[1]:GetName(),fg[2]:GetName(),load_count)");
}
#[test]
fn background_upper_left_wraps_before_init_and_composes_existing_commands() {
    for live in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "background.lua",
            r#"
            return Def.Sprite{
                Name='image',InitCommand=function(self)
                    saved[1]=self;log('background-init',self:GetParent():GetName(),self:GetParent():GetParent())
                end,
                OnCommand=function(self) log('authored-on');self:x(17) end,
            }
        "#,
        );
        let mut context = SongLuaCompileContext::new(dir.path(), "upper left");
        context.live = live;
        context.background_layer_count = 1;
        context.background_upper_left_colors = vec![Some([0.25, 0.5, 0.75, 1.])];
        let mut pair = Pair::new(context);
        pair.load(&[path]);
        pair.script("local a=saved[1]; local ok,err=pcall(function()a:playcommand('On')end); assert(not ok and tostring(err):find('UpperLeft.lua:6',1,true)); log('on-result',ok); log('after-on',a:GetX(),a:GetParent():GetParent(),#__songlua_top_screen:GetChild('SongForeground'))");
    }
}
#[test]
fn invalid_plans_reject_entries_before_any_lua_execution() {
    let dir = tempfile::tempdir().unwrap();
    let p = write(dir.path(), "entry.lua", "error('must not execute')");
    let mut c = SongLuaCompileContext::new(dir.path(), "invalid layer inputs");
    c.background_layer_count = 2;
    assert_eq!(SongLayerPlan::new(&[], 9, &c).unwrap().paths().len(), 0);
    assert_eq!(
        SongLayerPlan::new(&[&p], 0, &c).unwrap_err(),
        "song lua background count exceeds its session entries"
    );
    c.background_layer_count = 0;
    c.foreground_names = vec!["one".into(), "two".into()];
    assert_eq!(
        SongLayerPlan::new(&[&p], 0, &c).unwrap_err(),
        "song lua foreground names do not match its session entries"
    );
    c.foreground_names.clear();
    c.layer_start_beats = vec![f32::NAN];
    assert_eq!(
        SongLayerPlan::new(&[&p], 0, &c).unwrap_err(),
        "song lua layer beats do not match its session entries"
    );
    c.layer_start_beats.clear();
    assert!(
        SongLayerPlan::new(&[&p], 1, &c)
            .unwrap_err()
            .contains("primary index 1")
    );
    assert!(
        SongLayerPlan::new(&[&dir.path().join("missing")], 0, &c)
            .unwrap_err()
            .contains("does not exist")
    );
}

#[test]
fn source_alias_selects_xero_environment_and_nested_file_directory() {
    for live in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "template/child.lua",
            "counter=counter+3; return counter",
        );
        let actual = write(
            dir.path(),
            "physical.lua",
            r#"
            counter=counter+2
            local child=assert(loadfile('child.lua'))()
            log('aliased-environment',counter,child,__songlua_current_script_path,debug.getinfo(1,'S').source)
            return Def.Actor{InitCommand=function(self) saved[1]=self;log('aliased-init',counter,child)end}
        "#,
        );
        let mut c = SongLuaCompileContext::new(dir.path(), "source alias environment");
        c.live = live;
        c.foreground_names = vec!["template/std.lua".into()];
        let mut p = Pair::new(c);
        p.script("xero={counter=20}");
        p.load(&[actual]);
        p.script("log('after-environment',xero.counter,counter,__songlua_script_dir,__songlua_current_script_path,__songlua_current_chunk_env)");
    }
}
#[test]
fn failed_init_preserves_partial_state_and_all_constructor_owners_retire() {
    for live in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "bad-init.lua",
            r#"
            return Def.ActorFrame{InitCommand=function(self)
                saved[1]=self; self:x(9); self:AddWrapperState():x(12)
                local retained=self
                TEMP=Def.Actor{ProbeCommand=function()return retained end}
                log('failed-init',self:GetX(),self:GetParent():GetName(),#self:GetParent())
                error('intentional init failure')
            end}
        "#,
        );
        let mut c = SongLuaCompileContext::new(dir.path(), "partial layer failure");
        c.live = live;
        let mut p = Pair::new(c);

        let plan = SongLayerPlan::new(&[&path], 0, &p.context).unwrap();
        let error = p.rust.initialize_song_layers(p.scene, &plan).unwrap_err();
        assert!(
            error.to_string().contains("intentional init failure"),
            "{error}"
        );
        p.compare();
        p.script("log('partial-child-count',#__songlua_top_screen:GetChild('SongForeground')); weak=setmetatable({saved[1],TEMP},{__mode='v'})");
        assert!(p.rust.retire_actor_scene(p.scene).unwrap());
        p.rust.reconstruct_gameplay_screen(&p.context).unwrap();
        p.script(
            "log('saved-after-retirement',saved[1]:GetX(),saved[1]:GetWrapperState(1):GetX())",
        );
        assert_eq!(test_support::actor_ownership_counts(&p.rust)[2], 1);
        p.rust
            .vm_mut()
            .execute(
                &compile_named(
                    b"saved={};TEMP=nil;collectgarbage();collectgarbage();assert(next(weak)==nil)",
                    b"=retired layer cleanup",
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(test_support::actor_ownership_counts(&p.rust)[2], 0);
    }
}

#[test]
fn compiler_debug_keeps_context_policy_lua_upvalues_and_author_replacements() {
    let dir = tempfile::tempdir().unwrap();
    let path = write(
        dir.path(),
        "debug.lua",
        r#"
        local function observe(info)
            log('info',info.source,info.short_src,info.what,info.currentline,info.linedefined,info.lastlinedefined,info.namewhat)
        end
        observe(debug.getinfo());observe(debug.getinfo(nil));observe(debug.getinfo(999,'n'))
        local item={tag=24}; local function closure() return item end
        local name,value=debug.getupvalue(closure,1.75);log('upvalue',name,value==item,value.tag)
        for _,index in ipairs{0,-1,2,'1'}do log('absent',select('#',debug.getupvalue(closure,index)),debug.getupvalue(closure,index))end
        log('trace',debug.traceback(12),debug.traceback(false),debug.traceback('message',20),debug.traceback(string.char(255)))
        local n=0;for _ in pairs(debug)do n=n+1 end;log('debug-fields',n)
        return Def.Actor{InitCommand=function(self) saved[1]=self;observe(debug.getinfo(1)) end}
    "#,
    );
    let c = SongLuaCompileContext::new(dir.path(), "compiler debug");
    let mut p = Pair::new(c);
    p.load(&[path]);
    p.script(
        "log('outside-file',debug.getinfo(1).source); old_debug=debug;debug={replacement=true}",
    );
    assert!(!p.rust.begin_song_compilation().unwrap());
    p.script("assert(debug.replacement and debug~=old_debug)");
}

fn compare_initial(pair: &mut Pair) {
    let rust = pair.rust.song_initial_states(pair.scene).unwrap();
    for actual in [rust.actors, rust.screen_layers] {
        let mut actual = actual
            .into_iter()
            .map(|(actor, state)| {
                let vm = pair.rust.vm_mut();
                let key = vm.try_intern(b"Name").unwrap();
                let name = match vm.raw_table_get(actor, key).unwrap() {
                    Value::String(id) => {
                        String::from_utf8_lossy(vm.string_bytes(id).unwrap()).into_owned()
                    }
                    Value::Nil => String::new(),
                    other => panic!("unexpected actor name: {other:?}"),
                };
                (name, format!("{state:?}"))
            })
            .collect::<Vec<_>>();
        actual.sort();
        reference_test::assert_debug("song/tests/rust_song_layers.rs::initial_states", &actual);
    }
}
#[test]
fn compiler_initial_states_precede_layer_init_and_keep_player_and_screen_boundaries() {
    for live in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let first = write(
            dir.path(),
            "first.lua",
            r#"
            local player=__songlua_top_screen_player_1
            player:x(111)
            return Def.ActorFrame{
                Name='frame', FOV=63,
                InitCommand=function(self)
                    saved[1]=self; self:x(90); self[1]:y(80)
                    player:x(222)
                    __songlua_top_screen:GetChild('Underlay'):x(333)
                    collectgarbage()
                end,
                Def.Quad{Name='leaf', InitCommand=function(self) self:zoom(3):diffusealpha(0.2) end},
            }
        "#,
        );
        let second = write(
            dir.path(),
            "second.lua",
            r#"
            saved[1]:x(444)
            return Def.Actor{Name='second', InitCommand=function(self) self:x(555) end}
        "#,
        );
        let mut context = SongLuaCompileContext::new(dir.path(), "initial state timing");
        context.live = live;
        let mut pair = Pair::new(context);
        pair.script("local a=__songlua_top_screen:GetChild('Underlay'); a.FOV=75; a:fov(-1)");
        pair.load(&[first, second]);
        compare_initial(&mut pair);
        let before = format!("{:?}", pair.rust.song_initial_states(pair.scene).unwrap());
        pair.script("saved[1]:x(999); collectgarbage('collect')");
        assert_eq!(
            format!("{:?}", pair.rust.song_initial_states(pair.scene).unwrap()),
            before
        );
        compare_initial(&mut pair);
        assert!(pair.rust.retire_actor_scene(pair.scene).unwrap());
        assert!(pair.rust.song_initial_states(pair.scene).is_none());
    }
}
#[test]
fn compiler_initial_states_preserve_authored_raw_render_properties() {
    for live in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "raw.lua",
            r#"
            return Def.ActorFrame{
                Name='raw', FOV=75,
                __songlua_state_x=' 12.5 ', __songlua_state_y=-8,
                __songlua_state_zoom_x=2, __songlua_state_rot_z_deg=37,
                __songlua_state_fov=-1, __songlua_state_draw_order=9.75,
                __songlua_visible=0, __songlua_theme_hibernating='',
                __songlua_state_diffuse={0.2,'0.4',0.6,0.8},
                __songlua_state_vertex_colors={{1,0,0,1},{0,1,0,1},{0,0,1,1},{1,1,1,1}},
                __songlua_state_glow={1,0.5,0.2,0.1},
                __songlua_state_size={27,39}, __songlua_state_shadow_len={2,3},
                __songlua_state_base_rotation={5,6,7},
                __songlua_state_quaternion={0.1,0.2,0.3,0.4},
                __songlua_state_effect_magnitude={9,8,7},
                __songlua_state_effect_clock='music', __songlua_state_effect_mode='pulse',
                __songlua_state_effect_timing={1,2,3,4,5},
                __songlua_state_effect_timer=false, __songlua_state_effect_no_offset=true,
                __songlua_state_blend='BlendMode_Add',
                __songlua_state_text_align='left', __songlua_state_text_glow_mode='both',
                __songlua_state_max_width=123, __songlua_state_wrap_width_pixels=45,
                __songlua_state_texture_filtering=false, __songlua_state_texture_wrapping=true,
                __songlua_state_texcoord_offset={0.1,0.2},
                __songlua_state_custom_texture_rect={0,0,1,1},
                InitCommand=function(self) self:x(700):zoom(4) end,
                Def.Sprite{Name='empty-sprite'},
                Def.Actor{Name='incomplete-color', __songlua_state_diffuse={1,2,3}},
            }
        "#,
        );
        let mut context = SongLuaCompileContext::new(dir.path(), "raw initial state");
        context.live = live;
        let mut pair = Pair::new(context);
        pair.load(&[path]);
        compare_initial(&mut pair);
    }
}
#[test]
fn malformed_initial_render_state_errors_before_init_in_both_modes() {
    for live in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "bad-state.lua",
            r#"
            log('loaded')
            return Def.Actor{Name='bad', __songlua_state_x={}, InitCommand=function(self) log('unexpected-init') end}
        "#,
        );
        let mut context = SongLuaCompileContext::new(dir.path(), "initial state validation");
        context.live = live;
        let mut pair = Pair::new(context);

        let plan = SongLayerPlan::new(&[path.as_path()], 0, &pair.context).unwrap();
        let error = pair
            .rust
            .initialize_song_layers(pair.scene, &plan)
            .unwrap_err();
        assert!(error.to_string().contains("number"), "{error}");
        pair.compare();
        pair.script("assert(table.concat(events,'|')=='loaded'); collectgarbage()");
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
