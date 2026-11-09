use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime},
};

fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/framebuffers.lua").unwrap())
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"))
        .to_vec()
}
struct Pair {
    rust: RustSongRuntime,
    scene: ActorScene,
    _files: tempfile::TempDir,
}
impl Pair {
    fn new(live: bool) -> Self {
        let files = tempfile::tempdir().unwrap();
        reference_test::register_path(files.path());
        let path = files.path().join("existing.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([255; 4]))
            .save(&path)
            .unwrap();
        let source = format!(
            r#"
            existing={:?};observations={{}}
            function log(...) for i=1,select('#',...) do observations[#observations+1]=tostring(select(i,...)) end end
            function texture(t) if not t then log('nil texture');return end
                log(t:GetPath(),t:GetSourceWidth(),t:GetSourceHeight(),t:GetTextureWidth(),t:GetTextureHeight(),t:GetImageWidth(),t:GetImageHeight(),t:GetNumFrames())
            end
            return Def.ActorFrame{{ScriptErrorMessageCommand=function(self,p) log(p.message) end,
                Def.ActorFrameTexture{{Name='a',Def.Quad{{Name='child',InitCommand=function(self) log('child Init') end}}}},
                Def.ActorFrameTexture{{Name='b'}},Def.Sprite{{Name='sprite',Texture=existing}}}}
        "#,
            deadsync_song_lua::file_path_string(&path)
        );
        let mut context = SongLuaCompileContext::new(files.path(), "framebuffer migration");
        context.live = live;

        let mut rust = RustSongRuntime::new(&context).unwrap();
        let definition = execute(&mut rust, &source)[0];
        let scene = rust.instantiate_actor(definition).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        let mut pair = Self {
            rust,
            scene,
            _files: files,
        };
        pair.script("a=root[1];b=root[2];sprite=root[3]", false);
        pair
    }
    fn script(&mut self, source: &str, error: bool) {
        execute(&mut self.rust, source);
        self.compare(source, error);
    }
    fn compare(&mut self, phase: &str, error: bool) {
        for name in ["a", "b"] {
            let actor = self.rust.vm_mut().get_global(name.as_bytes()).unwrap();
            let actual = self.rust.actor_frame_texture(actor);
            if error && name == "a" {
                assert!(actual.is_err(), "{phase}: Rust read must fail");
            } else {
                reference_test::assert_debug(
                    "song/tests/rust_framebuffers.rs::payload",
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
        reference_test::assert_debug("song/tests/rust_framebuffers.rs::render", &(phase, &actual));
        let probe =
            "return table.concat(observations,'|')..'/'..tostring(__songlua_side_effect_count)";
        let Value::String(actual) = execute(&mut self.rust, probe)[0] else {
            panic!("observations")
        };
        reference_test::assert_debug(
            "song/tests/rust_framebuffers.rs::observations",
            &(phase, &self.rust.vm_mut().string_bytes(actual).unwrap()),
        );
    }
    fn advance(&mut self, delta: f64) {
        self.rust.advance_actor_scene(self.scene, delta).unwrap();
        self.compare("advance", false);
    }
}

#[test]
fn lua_error_service_preserves_message_reentry_gc_and_failure_cleanup() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        pair.script(
            r#"
            local report=lua.ReportScriptError
            root.ScriptErrorMessageCommand=function(self,p)
                collectgarbage();log(p.message);report('nested error suppressed')
            end
            report('outer error')
            assert(__songlua_reporting_script_error==false)
            root.ScriptErrorMessageCommand=function()error('handler failure')end
            assert(not pcall(report,'failed report'))
            assert(__songlua_reporting_script_error==false)
            root.ScriptErrorMessageCommand=function(self,p)log(p.message)end
            lua=nil;collectgarbage()
            assert(not pcall(report,false,'invalid arguments use no method receiver'))
            report('retained report')
        "#,
            false,
        );
    }
}

#[test]
fn creation_freezes_settings_and_retained_texture_metadata() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        pair.script("assert(a:GetTexture()==nil and b:GetTexture()==nil);assert(a:GetNumChildren()==1 and a:GetChild('child')==a[1]);log(a.__songlua_aft_capture_name,b.__songlua_aft_capture_name)",false);
        pair.script("a:SetWidth(13.9):SetHeight(9.8):EnableAlphaBuffer(true):EnableDepthBuffer(true):EnableFloat(true):EnablePreserveTexture(true):SetTextureName('dir//sub/../capture'):Create();saved=a:GetTexture();texture(saved);assert(saved==a:GetTexture() and not rawequal(saved,a:GetTexture()))",false);
        pair.script("a:SetWidth(100):SetHeight(200):SetTextureName('new name'):EnableAlphaBuffer(false):EnableDepthBuffer(false):EnableFloat(false);texture(saved);texture(a:GetTexture());assert(saved==a:GetTexture())",false);
        pair.script("sprite:SetTexture(saved);texture(saved);assert(sprite.__songlua_aft_capture_name=='dir/capture');a:EnablePreserveTexture(false)",false);
        pair.script("a:linear(1):x(40):EnablePreserveTexture(true);b:SetWidth(12):SetHeight(7):sleep(.5):Create();texture(b:GetTexture())",false);
        pair.advance(0.25);
        pair.advance(0.8);
    }
}

#[test]
fn creation_errors_dispatch_script_error_and_allow_retry() {
    let mut pair = Pair::new(true);
    pair.script("a:SetWidth(0):Create();assert(a:GetTexture()==nil)", false);
    pair.script(
        "a:SetWidth(12):SetHeight(8):SetTextureName(existing):Create();assert(a:GetTexture()==nil)",
        false,
    );
    pair.script("a:SetTextureName('shared'):Create();b:SetTextureName('shared'):Create();assert(b:GetTexture()==nil);a:Create()",false);
    pair.script("b:SetTextureName('second'):Create();root.ScriptErrorMessageCommand=function(self,p) log(p.message);a:Create();b:Create() end;a:Create();assert(__songlua_reporting_script_error==false)",false);
    pair.script("root.ScriptErrorMessageCommand=function() error('report failure') end;assert(not pcall(a.Create,a));assert(__songlua_reporting_script_error==false);root.ScriptErrorMessageCommand=function(self,p) log(p.message) end;a:Create()",false);
}

#[test]
fn dimensions_and_strict_buffer_arguments_match_production() {
    for dimensions in [
        "{13.9,8.4}",
        "{.9,7}",
        "{-2,7}",
        "{0/0,7}",
        "{1/0,7}",
        "{'0x10','2.75'}",
        "{}",
    ] {
        let mut pair = Pair::new(false);
        pair.script(
            &format!("a.__songlua_state_size={dimensions};a:Create();texture(a:GetTexture())"),
            false,
        );
    }
    let mut pair = Pair::new(true);
    for name in [
        "EnableAlphaBuffer",
        "EnableDepthBuffer",
        "EnableFloat",
        "EnablePreserveTexture",
    ] {
        pair.script(&format!("for _,v in ipairs{{0,1,'true',{{}}}} do assert(not pcall(a.{name},a,v)) end;assert(not pcall(a.{name},a));assert(a:{name}(true)==a);assert(a:{name}(false)==a)"),false);
    }
    pair.script("assert(not pcall(a.SetTextureName,a));assert(not pcall(a.SetTextureName,a,false));a:SetTextureName(17):Create();texture(a:GetTexture())",false);
}

#[test]
fn collecting_allocation_getters_preserve_partial_publication_and_retry() {
    let mut pair = Pair::new(true);
    pair.script(r#"
        local mt=getmetatable(a);local lookup=mt.__index
        mt.__index=function(self,key)
            if key=='__songlua_aft_alpha_buffer' then collectgarbage();b:aux(7);log('alpha');return true end
            if key=='__songlua_aft_depth_buffer' then log('depth');error('flag failure') end
            return lookup(self,key)
        end
        assert(not pcall(a.Create,a));assert(a:GetTexture()==nil);mt.__index=lookup;a:Create();texture(a:GetTexture())
    "#,false);
    pair.script("local allocation=a.__songlua_aft_allocation;a.__songlua_aft_allocation=nil;local mt=getmetatable(a);local lookup=mt.__index;mt.__index=function(self,key) if key=='__songlua_aft_allocation' then collectgarbage();return allocation end;return lookup(self,key) end;assert(a:GetTexture()==nil);mt.__index=lookup;a.__songlua_aft_allocation=allocation",false);
}

#[test]
fn texture_handle_equality_and_collection_keep_captured_metadata() {
    let mut pair = Pair::new(true);
    pair.script("a:Create();b:Create();local x=a:GetTexture();local y=a:GetTexture();assert(x==y,'same allocation');assert(x~=b:GetTexture(),'different allocations');saved=x;weak=setmetatable({y},{__mode='v'});y=nil;collectgarbage();collectgarbage();texture(saved)",false);
    // Unreferenced handles and their captured callbacks must be collected.
    execute(
        &mut pair.rust,
        "assert(weak[1]==nil,'unreferenced handle collected')",
    );
    pair.script("a.__songlua_aft_allocation.name='changed';a.__songlua_aft_allocation.size={17,9};texture(saved);texture(a:GetTexture());assert(saved~=a:GetTexture());saved:loop(false):rate(2)",false);
    let actor = pair.rust.vm_mut().get_global(b"a").unwrap();
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    assert!(pair.rust.actor_frame_texture(actor).is_err());
    execute(
        &mut pair.rust,
        "weak=setmetatable({a},{__mode='v'});a=nil;b=nil;root=nil;sprite=nil;collectgarbage();assert(weak[1]);texture(saved);saved=nil;collectgarbage();collectgarbage();assert(weak[1]==nil)",
    );
}

#[test]
fn malformed_allocation_reads_recover_without_stale_output() {
    let mut pair = Pair::new(false);
    pair.script("a:Create();allocation=a.__songlua_aft_allocation", false);
    pair.script(
        "a.__songlua_aft_allocation=false;assert(not pcall(a.GetTexture,a))",
        true,
    );
    pair.script("a.__songlua_aft_allocation=allocation", false);
    for source in [
        "allocation.name=false",
        "allocation.size=false",
        "allocation.size=nil",
    ] {
        pair.script(source, true);
        pair.script("allocation.name='restored';allocation.size={4,5}", false);
    }
    pair.script("allocation.size={false,5}", false);
    pair.script(
        "assert(not pcall(a.GetTexture,a));allocation.size={4,5};texture(a:GetTexture())",
        false,
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
