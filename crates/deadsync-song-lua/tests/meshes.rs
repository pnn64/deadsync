use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, {ActorScene, SongRuntime},
};

#[test]
fn host_buffer_provenance_is_weak() {
    fn allocate(call: &mut deadlib_lua::HostCall<'_>) -> Result<(), deadlib_lua::Error> {
        let value = call.try_new_userdata(216)?;
        call.with_roots(&[value], |call| {
            let Value::Table(registry) = call.captures()[0] else {
                unreachable!()
            };
            call.raw_table_set(registry, value, Value::Boolean(true))?;
            call.push(value)
        })
    }
    let mut vm = deadlib_lua::Vm::new();
    let registry = vm
        .execute(
            &compile_named(
                b"return setmetatable({}, {__mode='k'})",
                b"=buffer provenance",
            )
            .unwrap(),
        )
        .unwrap()[0];
    let allocate = vm.new_host_function(allocate, &[registry]).unwrap();
    vm.set_global(b"allocate", allocate).unwrap();
    vm.execute(&compile_named(b"local value=allocate(); local weak=setmetatable({value},{__mode='v'});value=nil;collectgarbage();collectgarbage();assert(weak[1]==nil)", b"=buffer collection").unwrap()).unwrap();
}

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/meshes.lua").unwrap())
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"))
        .to_vec()
}
struct Pair {
    rust: SongRuntime,
    scene: ActorScene,
    seconds: f32,
    _files: tempfile::TempDir,
}
impl Pair {
    fn new(live: bool) -> Self {
        let files = tempfile::tempdir().unwrap();
        reference_test::register_path(files.path());
        let path = files.path().join("mesh 2x1.png");
        image::RgbaImage::from_pixel(12, 8, image::Rgba([255; 4]))
            .save(&path)
            .unwrap();
        let source = format!(
            r#"
            texture={:?};observations={{}}
            function log(...) for i=1,select('#',...) do local v=select(i,...)
                observations[#observations+1]=type(v)=='number' and string.format('%.17g',v) or tostring(v) end end
            vertices={{{{{{0,0,1}},{{1,0,0,.5}},{{0,0}}}},{{{{10,0,2}},{{0,1,0,.75}},{{1,0}}}},
                      {{{{10,10,3}},{{0,0,1,1}},{{1,1}}}},{{{{0,10,4}},{{1,1,0,1}},{{0,1}}}},
                      {{{{-5,5,5}},{{1,0,1,1}},{{.5,.5}}}},{{{{-5,-5,6}},{{0,1,1,1}},{{.5,0}}}}}}
            return Def.ActorFrame{{Def.ActorMultiVertex{{Name='a',Texture=texture}},Def.ActorMultiVertex{{Name='b'}}}}
        "#,
            deadsync_song_lua::file_path_string(&path)
        );
        let mut context = SongLuaCompileContext::new(files.path(), "mesh migration");
        context.live = live;

        let mut rust = SongRuntime::new(&context).unwrap();
        let definition = execute(&mut rust, &source)[0];
        let scene = rust.instantiate_actor(definition).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();

        execute(&mut rust, "a=root[1];b=root[2]");
        let mut pair = Self {
            rust,
            scene,
            seconds: 0.0,
            _files: files,
        };
        pair.compare("construction", false);
        pair
    }
    fn compare(&mut self, phase: &str, read_error: bool) {
        for name in ["a", "b"] {
            let actor = self.rust.vm_mut().get_global(name.as_bytes()).unwrap();
            let actual = self.rust.actor_multi_vertex_mesh(actor);
            if read_error && name == "a" {
                assert!(actual.is_err(), "{phase}: Rust read must fail");
            } else {
                reference_test::assert_debug(
                    "song/tests/rust_meshes.rs::payload",
                    &(&format!("{phase} / {name}"), &actual.unwrap()),
                );
            }
        }
        let actual = self
            .rust
            .actor_scene_snapshot(self.scene)
            .unwrap()
            .into_iter()
            .map(|actor| actor.render)
            .collect::<Vec<_>>();
        reference_test::assert_debug("song/tests/rust_meshes.rs::render", &(phase, &actual));
        let Value::String(actual) =
            execute(&mut self.rust, "return table.concat(observations,'|')")[0]
        else {
            panic!("string")
        };
        reference_test::assert_debug(
            "song/tests/rust_meshes.rs::observations",
            &(phase, &self.rust.vm_mut().string_bytes(actual).unwrap()),
        );
    }
    fn script(&mut self, source: &str) {
        self.script_with_error(source, false);
    }
    fn script_with_error(&mut self, source: &str, read_error: bool) {
        execute(&mut self.rust, source);
        self.compare(source, read_error);
    }
    fn advance(&mut self, delta: f64) {
        self.seconds += delta as f32;

        self.rust
            .set_values(self.seconds * 2.0, self.seconds)
            .unwrap();

        self.rust.advance_actor_scene(self.scene, delta).unwrap();
        self.compare("advance", false);
    }
}

#[test]
fn vertex_uploads_copy_scalars_and_survive_collection() {
    let mut pair = Pair::new(true);
    pair.script("assert(a:GetTexture():GetPath()=='');assert(a:GetTexture():GetSourceWidth()==1);assert(a:SetVertices(vertices)==a);assert(type(a.__songlua_vertices)=='userdata');assert(a:GetNumVertices()==6);log(a:GetNumVertices(),a:GetDestDrawMode())");
    let actor = pair.rust.vm_mut().get_global(b"a").unwrap();
    let mesh = pair.rust.actor_multi_vertex_mesh(actor).unwrap().unwrap();
    assert_eq!(mesh.vertices.len(), 6);
    assert_eq!(mesh.vertices[0].pos, [0.0, 0.0]);
    assert_eq!(mesh.vertices[1].pos, [10.0, 0.0]);
    assert_eq!(mesh.vertices[0].color[3], 0.5);
    pair.script("for key in pairs(MESSAGEMAN) do assert(type(key)~='userdata') end");
    pair.script("vertices[1][1][1]=900;vertices[1][2][4]=0;vertices[2]=false;collectgarbage('collect');log(a:GetNumVertices())");
    pair.script("saved=a.__songlua_vertices;b.__songlua_vertices=saved;b:SetNumVertices(99);a:SetVertices({});collectgarbage('collect');log(a:GetNumVertices(),b:GetNumVertices())");
    pair.script("a.__songlua_vertices=saved;a.__songlua_vertex_count=nil;assert(not pcall(a.GetNumVertices,a));a.__songlua_vertex_count=6;log(a:GetNumVertices())");
    pair.script("weak=setmetatable({saved},{__mode='v'});saved=nil;a.__songlua_vertices=nil;b.__songlua_vertices=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil)");
    pair.script(r#"
        local mt=getmetatable(a);local assign=mt.__newindex
        mt.__newindex=function(self,key,value)
            if key=='__songlua_vertices' then
                weak[2]=value;collectgarbage('collect');assert(weak[2]==value);error('rejected upload')
            end
            rawset(self,key,value)
        end
        assert(not pcall(a.SetVertices,a,vertices));assert(a:GetNumVertices()==6)
        mt.__newindex=assign;collectgarbage('collect');collectgarbage('collect');assert(weak[2]==nil)
    "#);
}

#[test]
fn uploaded_buffer_collection_after_successful_and_failed_queries() {
    for query in ["none", "success", "failure"] {
        let mut pair = Pair::new(true);
        if query == "none" {
            pair.script("a:SetVertices(vertices);weak=setmetatable({a.__songlua_vertices},{__mode='v'});a.__songlua_vertices=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil,'without mesh query')");
            continue;
        }
        pair.script(
            "a:SetVertices(vertices);weak=setmetatable({a.__songlua_vertices},{__mode='v'})",
        );
        if query == "failure" {
            pair.script("a.__songlua_vertex_count=nil;assert(not pcall(a.GetNumVertices,a));a.__songlua_vertex_count=6");
        }
        pair.script(&format!("a.__songlua_vertices=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil,'after {query} query')"));
    }
}

#[test]
fn mesh_modes_ranges_and_line_joins_match_the_production_reader() {
    let mut pair = Pair::new(true);
    pair.script("a:SetVertices(vertices)");
    for mode in [
        "DrawMode_Triangles",
        "DrawMode_Quads",
        "DrawMode_QuadStrip",
        "DrawMode_Fan",
        "DrawMode_LineStrip",
        "drawmode_quads",
        "other",
    ] {
        pair.script(&format!("a:SetDrawState{{Mode='{mode}',First=1,Num=-1}}:SetLineWidth(3);log(a:GetDestDrawMode())"));
        for (first, num) in [(2, 3), (0, 0), (-3, 99), (10, -1), (1, -1)] {
            pair.script(&format!("a:SetDrawState{{First={first},Num={num}}}"));
        }
    }
    pair.script("a:SetVertices{{{0,0}},{{0,0}},{{10,0}},{{0,0}},{{-10,.001}},{{-10,.001}}};a:SetDrawState{Mode='DrawMode_LineStrip'}:SetLineWidth(2)");
    pair.script("a:SetLineWidth(0);a:SetLineWidth(-2)");
    pair.script("a:SetLineWidth('nan');a:SetLineWidth('inf')");
}

#[test]
fn mesh_upload_conversion_and_count_rules_keep_existing_fallbacks() {
    let mut pair = Pair::new(false);
    for vertices in [
        "{}",
        "{false,{{1,2,3},{1,0,0},{0,1}},{{3,4}},{{5,6}}}",
        "{{{'1','2','3'},{'0x1','0.5','1',nil},{'.1','.2'}},{{4,5},'#ff00ff',{0/0,1}},{{8,9,1/0},string.char(255)}}",
        "{{{0/0,2,-3},{1,1,1,1},{0,1}},{{3,4,5}},{{6,7,8}}}",
        "{{{1,2}},{{3,4}},{{5,6}},[8]={{10,12}}}",
    ] {
        pair.script(&format!(
            "a:SetVertices({vertices});log(a:GetNumVertices())"
        ));
    }
    for count in ["3.5", "'2.5'", "false", "-7", "'inf'", "'nan'", "1e50"] {
        pair.script(&format!(
            "a:SetNumVertices({count});log(a:GetNumVertices())"
        ));
    }
    pair.script("a:SetVertices(vertices);a:SetVertices(false);a.__songlua_vertex_count='4.9';log(a:GetNumVertices());a.__songlua_vertex_count=1/0;assert(not pcall(a.GetNumVertices,a));a.__songlua_vertex_count=nil;a.__songlua_vertices=vertices;log(a:GetNumVertices())");
}

#[test]
fn mesh_updates_and_texture_loads_remain_immediate_during_tweens() {
    for live in [true, false] {
        let mut pair = Pair::new(live);
        pair.script("a:sleep(.5):SetVertices(vertices):SetDrawState{Mode='DrawMode_Quads'}:SetLineWidth(5);assert(a:LoadTexture(texture)==a);log(a:GetTexture():GetPath(),a:GetWidth(),a:GetHeight())");
        pair.script("a:SetTexture(a:GetTexture());log(a:GetWidth(),a:GetHeight());a:LoadTexture(nil);log(a:GetTexture():GetPath(),a:GetWidth(),a:GetHeight())");
        pair.script("a:LoadTexture(123);log(a:GetTexture():GetPath());assert(not pcall(a.LoadTexture,a,false));assert(not pcall(a.LoadTexture,a,string.char(255)));a:LoadTexture(nil)");
        pair.advance(0.25);
        pair.advance(0.5);
        pair.script("assert(a.InitState==nil);log(a:GetNumVertices(),a:GetDestDrawMode());assert(a:GetTexture():GetPath()=='')");
    }
}

#[test]
fn draw_state_getters_reenter_collect_and_keep_partial_writes() {
    let mut pair = Pair::new(true);
    pair.script(r#"
        a:SetVertices(vertices)
        local reads=0
        a:SetDrawState(setmetatable({}, {__index=function(self,key)
            b:aux(b:getaux()+1);collectgarbage('collect')
            if key=='Mode' then reads=reads+1;return reads==1 and 'DrawMode_Quads' or 'DrawMode_Fan' end
            return key=='First' and 2 or 4
        end}));log(a:GetDestDrawMode(),a.__songlua_draw_state.Mode)
        local original=a.__songlua_draw_state
        assert(not pcall(a.SetDrawState,a,setmetatable({}, {__index=function(self,key)
            collectgarbage('collect');if key=='Mode' then return 'DrawMode_Triangles' end;error('range failure')
        end})))
        assert(a.__songlua_draw_state==original);log(original.Mode,a:GetDestDrawMode())
    "#);
    pair.script("a.__songlua_draw_state=setmetatable({}, {__index=function(self,key) b:aux(b:getaux()+1);collectgarbage('collect');error('ignored range failure') end})");
    pair.script("a.__songlua_draw_state=nil;assert(not pcall(a.SetDrawState,a,{Mode=false,First=2}));assert(a.__songlua_draw_state.Mode==false);log(a:GetDestDrawMode())");
    pair.script("a.__songlua_draw_state=nil;a:SetDrawState{Mode='DrawMode_Triangles'};a.__songlua_vertices=nil;local mt=getmetatable(a);local lookup=mt.__index;mt.__index=function(self,key) if key=='__songlua_vertices' then collectgarbage('collect');return vertices end;return lookup(self,key) end");
}

#[test]
fn mesh_reader_rejects_invalid_buffers_and_recovers_without_stale_output() {
    let mut pair = Pair::new(true);
    pair.script("a:SetVertices(vertices);saved=a.__songlua_vertices");
    pair.script_with_error("a.__songlua_vertices=false", true);
    pair.script_with_error("a.__songlua_vertices=newproxy(true)", true);
    pair.script("a.__songlua_vertices=saved");
    pair.script_with_error("a.__songlua_draw_state=false", true);
    pair.script("a.__songlua_draw_state=nil");
    pair.script_with_error("a.__songlua_draw_state_mode=false", true);
    pair.script("a.__songlua_draw_state_mode=nil");
    pair.script_with_error("a.__songlua_line_width=false", true);
    pair.script("a.__songlua_line_width=nil;a:SetDrawState{Num=0}");
    pair.script("a:SetDrawState{Num=-1}");
    pair.script("for _,name in ipairs{'SetVertex','GetCurrDrawMode','GetDestFirstToDraw','GetSpline','SetTextureMode','SetState','AddState'} do assert(type(a[name])=='function');assert(not pcall(a[name],a)) end;assert(a.Load==nil and a.CropTo==nil)");
    let actor = pair.rust.vm_mut().get_global(b"a").unwrap();
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    assert!(pair.rust.actor_multi_vertex_mesh(actor).is_err());
}

#[test]
fn copied_texture_handle_retains_identity_and_metadata_across_gc_and_reload() {
    let mut pair = Pair::new(true);
    pair.script(
        r#"
        copied=a:GetTexture()
        b:SetTexture(copied)
        assert(b:GetTexture() == copied)
        collectgarbage('collect')
        log(b:GetTexture():GetPath(),b:GetTexture():GetTextureWidth(),b:GetTexture():GetTextureHeight())
        a:LoadTexture(nil)
        assert(b:GetTexture() == copied)
        assert(a:GetTexture() ~= copied)
        log(copied:GetNumFrames(),b:GetTexture():GetNumFrames())
        b:LoadTexture(nil)
        assert(b:GetTexture() ~= copied and b:GetTexture():GetPath() == '')
        log(b:GetTexture():GetTextureWidth(),b:GetTexture():GetTextureHeight())
        "#,
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
