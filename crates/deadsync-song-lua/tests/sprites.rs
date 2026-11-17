use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext, {ActorScene, SongRuntime},
};
use std::collections::HashMap;

struct Files {
    _dir: tempfile::TempDir,
    dir: String,
    first: String,
    second: String,
}
impl Files {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        reference_test::register_path(dir.path());
        let a = dir.path().join("sheet 3x2.png");
        let b = dir.path().join("other 2x1 (doubleres).png");
        image::RgbaImage::from_pixel(39, 26, image::Rgba([150, 80, 255, 255]))
            .save(&a)
            .unwrap();
        image::RgbaImage::from_pixel(40, 18, image::Rgba([255, 255, 255, 255]))
            .save(&b)
            .unwrap();
        Self {
            dir: song::file_path_string(dir.path()) + "/",
            first: song::file_path_string(&a),
            second: song::file_path_string(&b),
            _dir: dir,
        }
    }
}
fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/sprites.lua").unwrap())
        .unwrap()
        .to_vec()
}
struct Pair {
    rust: SongRuntime,
    scene: ActorScene,
    ids: HashMap<u64, String>,
    seconds: f32,
}
impl Pair {
    fn new(files: &Files, live: bool) -> Self {
        let source = format!(
            r#"
            directory={:?};first={:?};second={:?};observations={{}}
            function log(...) for i=1,select('#',...) do local v=select(i,...)
                observations[#observations+1]=type(v)=='number' and string.format('%.17g',v) or tostring(v) end end
            function metadata(t) log(t:GetPath(),t:GetSourceWidth(),t:GetSourceHeight(),t:GetTextureWidth(),t:GetTextureHeight(),t:GetImageWidth(),t:GetImageHeight(),t:GetNumFrames()) end
            return Def.ActorFrame{{Def.Sprite{{Name='a',Texture=first,_Dir=directory}},Def.Sprite{{Name='b',_Dir=directory}},Def.Quad{{Name='q',_Dir=directory}}}}
        "#,
            files.dir, files.first, files.second
        );
        let mut context = SongLuaCompileContext::new(&files.dir, "sprite migration");
        context.live = live;
        let mut rust = SongRuntime::new(&context).unwrap();
        let def = execute(&mut rust, &source)[0];
        let scene = rust.instantiate_actor(def).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        execute(&mut rust, "a=root[1];b=root[2];q=root[3]");
        let mut ids = HashMap::new();
        for name in ["root", "a", "b", "q"] {
            let actor = rust.vm_mut().get_global(name.as_bytes()).unwrap();
            ids.insert(
                rust.vm_mut().object_pointer(actor).unwrap() as u64,
                name.to_owned(),
            );
        }
        let mut pair = Self {
            rust,
            scene,
            ids,
            seconds: 0.0,
        };
        pair.compare("construction");
        pair
    }
    fn compare(&mut self, phase: &str) {
        let Value::String(actual) =
            execute(&mut self.rust, "return table.concat(observations,'|')")[0]
        else {
            panic!("string")
        };
        let observations =
            String::from_utf8_lossy(self.rust.vm_mut().string_bytes(actual).unwrap());
        reference_test::assert_source_debug(
            "song/tests/rust_sprites.rs::compare::observations",
            phase,
            &observations,
        );
        let actual = self.rust.actor_scene_snapshot(self.scene).unwrap();
        let render = actual
            .into_iter()
            .map(|actor| actor.render)
            .collect::<Vec<_>>();
        reference_test::assert_source_debug(
            "song/tests/rust_sprites.rs::compare::render",
            phase,
            &render,
        );
        let actual = self
            .rust
            .take_sprite_textures()
            .into_iter()
            .map(|(id, t)| format!("{} {t:?}", self.ids[&id]))
            .collect::<Vec<_>>();
        reference_test::assert_source_debug(
            "song/tests/rust_sprites.rs::compare::textures",
            phase,
            &actual,
        );
        reference_test::assert_source_debug(
            "song/tests/rust_sprites.rs::compare::judgments",
            phase,
            &self.rust.take_judgment_textures(),
        );
    }
    fn script(&mut self, source: &str) {
        execute(&mut self.rust, source);
        self.compare(source);
    }
    fn advance(&mut self, delta: f64) {
        self.seconds += delta as f32;
        self.rust
            .set_values(self.seconds * 2.0, self.seconds)
            .unwrap();
        self.rust.advance_actor_scene(self.scene, delta).unwrap();
        self.compare("advance");
    }
    fn capture(&mut self, queued: bool) {
        self.rust.begin_actor_capture(queued);
    }
}

#[test]
fn sprite_loading_and_retained_texture_metadata_match_production() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.script(r#"
        assert(b:GetTexture()==nil)
        local blank=q:GetTexture()
        assert(blank:GetPath()=='' and blank:GetSourceWidth()==1 and blank:GetSourceHeight()==1)
        assert(blank:GetNumFrames()==1);metadata(blank)
        h=a:GetTexture(); assert(h~=a:GetTexture()); metadata(h)
        log(a:GetWidth(),a:GetHeight(),a:GetNumStates(),a:GetState(),a:GetDecodeMovie())
        local path=h.GetPath;assert(path()==first)
        h.__songlua_texture_path='authored';assert(h:GetPath()==first);h.__songlua_texture_path=first
        a:Load(second);metadata(a:GetTexture());metadata(h)
        assert(b:SetTexture(h)==b);log(b:GetWidth(),b:GetHeight(),b:GetNumStates())
        assert(h:loop(false):rate('1.25')==h)
        local rate=h.rate;assert(rate(-2)==h);h:loop({});collectgarbage('collect')
    "#);
    pair.script(
        "a.Texture=first;log(a:GetWidth(),a:GetHeight());metadata(a:GetTexture());metadata(h)",
    );
}

#[test]
fn load_and_settexture_preserve_their_distinct_state_and_return_rules() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    for source in [
        "a:setstate(5);a:SetTexture(first);log(a:GetState())",
        "a:SetTexture(second);log(a:GetState(),a:GetWidth(),a:GetHeight())",
        "a:SetTexture(nil);assert(a.Texture==second);a:Load(nil);assert(a:GetTexture()==nil)",
        "a:Load('');metadata(a:GetTexture());a:Load(false);assert(a.Texture=='')",
        "a:LoadFromCached('Background',first);metadata(a:GetTexture())",
        "assert(a:LoadBanner(second,'last')=='last');assert(a:LoadBackground(first,nil)==nil)",
        "assert(not pcall(a.LoadBanner,a,false));a:Load('missing 4x2.png');metadata(a:GetTexture());log(a:GetNumStates())",
        "a:SetTexture({__songlua_default_texture=true});metadata(a:GetTexture());b:SetTexture(a:GetTexture());metadata(b:GetTexture())",
        "a:Load(first);q:Load(second);log(q:GetWidth(),q:GetHeight());metadata(q:GetTexture())",
    ] {
        pair.script(source);
    }
}

#[test]
fn sprite_animation_and_uv_properties_remain_immediate_across_tweens() {
    let files = Files::new();
    for live in [true, false] {
        let mut pair = Pair::new(&files, live);
        pair.script("a:sleep(1):setstate(4):animate(false):texturewrapping('yes'):texcoordvelocity(.1,.2):texturetranslate(.25,.5);log(a:GetState(),a:GetAnimationLengthSeconds())");
        pair.script("a:customtexturerect(.2,.3,.8,.9);log(a:GetState());a:SetSecondsIntoAnimation(.45);log(a:GetState())");
        pair.script("a:SetStateProperties{{Delay=.2},{Frame='3.5',Delay=.35},{Frame=1,Delay=-1}};log(a:GetState(),a:GetAnimationLengthSeconds());a:SetAllStateDelays(.4);log(a:GetAnimationLengthSeconds())");
        pair.script("a:SetCustomImageRect(0,0,1,1):stretchtexcoords(.1,-.2):addimagecoords(2,3);a:pause();a:play();a:SetDecodeMovie(true);log(a:GetDecodeMovie())");
        pair.advance(0.25);
        pair.advance(1.0);
        pair.script("for _,v in ipairs{-1,1.5,'2',true,{},0/0,1/0} do a:setstate(v) end;log(a:GetState());assert(not pcall(a.SetCustomPosCoords,a,{}))");
    }
}

#[test]
fn sprite_geometry_uses_image_frames_and_retained_handle_dimensions() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    for source in [
        "log(a:GetWidth(),a:GetHeight());a:zoomto(100,50);log(a:GetZoomX(),a:GetZoomY())",
        "a:CropTo(20,10);log(a:GetWidth(),a:GetHeight());a:CropTo(-1,2)",
        "a:scaletoclipped(-7,9);log(a:GetWidth(),a:GetHeight());a:Load(second):SetWidth(31);log(a:GetWidth(),a:GetHeight())",
        "a:scaletofit(10,20,110,220);log(a:GetZoomedWidth(),a:GetZoomedHeight());a:stretchto(0,0,100,150)",
        "h=a:GetTexture();a:Load(first);b:SetTexture(h);log(b:GetWidth(),b:GetHeight());metadata(h)",
        "a:zoom(3):linear(.5):CropTo(40,20);log(a:GetZoomX(),a:GetZoomY(),a:GetWidth(),a:GetHeight())",
    ] {
        pair.script(source);
    }
    pair.advance(0.25);
    pair.advance(0.25);
}

#[test]
fn texture_capture_preserves_epochs_frame_sheets_states_and_startup_suppression() {
    let files = Files::new();
    let mut pair = Pair::new(&files, false);
    pair.capture(false);
    pair.script("a.Frames={{Frame=1,Delay=.2},{Delay=.3}};a.__songlua_judgment_sprite_player=1;a:SetTexture(second)");
    pair.advance(0.25);
    pair.script("a:setstate(1);a:SetTexture(second);a:Load(first):setstate(2);log(__songlua_side_effect_count)");
    pair.capture(true);
    pair.script("a:Load(second);b:Load(first)");
    pair.capture(false);
    pair.script("b:Load(second);a:Load(first)");

    pair.rust.end_actor_capture();
    pair.script("a:Load(second)");
}

#[test]
fn texture_lookups_survive_reentry_gc_and_recover_after_errors() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.script(r#"
        a.Texture=nil;local mt=getmetatable(a);local lookup=mt.__index
        mt.__index=function(self,key)
            if key=='Texture' then b:aux(b:getaux()+1);collectgarbage('collect');return first end
            return lookup(self,key)
        end
        h=a:GetTexture();metadata(h);log(a:GetWidth(),a:GetHeight());a:addimagecoords(1,2);mt.__index=lookup
        a:Load(second);metadata(h);metadata(a:GetTexture())
        assert(not pcall(a.Load,a,string.char(255)))
        a.Texture=false;assert(not pcall(a.GetTexture,a));a.Texture=first
        local states={setmetatable({}, {__index=function(self,key)collectgarbage('collect');if key=='Frame' then return '2.9' else return '.25' end end})}
        a:SetStateProperties(states);log(a:GetState(),a:GetAnimationLengthSeconds());collectgarbage('collect')
    "#);
}

#[test]
fn texture_capture_errors_keep_prior_mutations_and_recover() {
    let files = Files::new();
    let mut pair = Pair::new(&files, false);
    pair.capture(false);
    pair.script("a.__songlua_state_sprite_frame_sheet={-1,2};assert(not pcall(a.SetTexture,a,second));log(a:GetState(),a:GetWidth(),__songlua_side_effect_count)");
    pair.script("a.__songlua_state_sprite_frame_sheet={3,'2.5'};a:SetTexture(first)");
    pair.script("a.Frames={{Frame=false,Delay=.2}};assert(not pcall(a.SetTexture,a,second));log(a:GetState(),a:GetWidth(),__songlua_side_effect_count)");
    pair.script("a.Frames={{Frame=2,Delay=.2}};a:SetTexture(first)");
    pair.script("a.Texture=false;assert(pcall(a.SetWidth,a,false));assert(not pcall(a.SetWidth,a,10));a.Texture=first");
}

#[test]
fn retained_texture_metadata_survives_owner_retirement_and_releases_cycles() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.script("h=a:GetTexture();weak=setmetatable({a},{__mode='v'});metadata(h)");
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    execute(
        &mut pair.rust,
        r#"
        root=nil;a=nil;b=nil;q=nil;collectgarbage('collect');assert(weak[1] and h:GetSourceWidth()==39)
        assert(h:GetNumFrames()==6 and h:GetPath()==first)
        h=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil)
    "#,
    );
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
