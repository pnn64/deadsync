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

#[test]
fn banner_construction_inheritance_and_scrolling_match_production() {
    let files = Files::new();
    for live in [false, true] {
        let mut pair = Pair::new(&files, live);
        pair.script(
            r#"
            assert(getmetatable(Banner).__index==Sprite)
            assert(Banner.Load==Sprite.Load and Banner.GetTexture==Sprite.GetTexture)
            assert(Banner.ScaleToClipped~=nil and Sprite.ScaleToClipped==nil)
            assert(Sprite.LoadFromSortOrder==nil and Sprite.GetScrolling==nil)
            assert(a.SetDrawFunction==nil and a.SetUpdateFunction==nil)
            assert(not a:GetScrolling() and a:GetPercentScrolling()==0)
            assert(a:SetScrolling(0,'0.25')==a and a:GetScrolling())
            log(a:GetPercentScrolling())
            a:SetScrolling(nil,{});assert(not a:GetScrolling() and a:GetPercentScrolling()==0)
            a.__songlua_state_banner_scrolling={};assert(a:GetScrolling())
            a.__songlua_state_banner_scroll_percent='0x10';assert(a:GetPercentScrolling()==16)
            a.__songlua_state_banner_scroll_percent={};assert(not pcall(a.GetPercentScrolling,a))
            a.__songlua_state_banner_scroll_percent=nil
            assert(a:ScaleToClipped(25,19)==a);log(a:GetWidth(),a:GetHeight())
            a:scaletoclipped(31,21);log(a:GetWidth(),a:GetHeight())
            a:ScaleToClipped(false,{});log(a:GetWidth(),a:GetHeight())
            local original=Banner.GetScrolling
            Banner.GetScrolling=function(self) assert(self==a);return 'override' end
            assert(a:GetScrolling()=='override');Banner.GetScrolling=original
            local inherited=Sprite.GetState
            Sprite.GetState=function(self)return 97 end;assert(a:GetState()==97)
            Sprite.GetState=inherited
            assert(type(a.LoadFromSongBanner)=='function' and not pcall(a.LoadFromSongBanner,a,{}))
            a.__songlua_bound_methods.LoadFromSongBanner=function(self,value)assert(self==a and value==7);return 'bound',8 end
            local x,y=a:LoadFromSongBanner(7);assert(x=='bound' and y==8)
            a.__songlua_bound_methods.LoadFromSongBanner=nil
        "#,
        );
    }
}

#[test]
fn banner_path_queries_and_fallbacks_preserve_load_distinctions() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.capture(false);
    pair.script(r#"
        song={HasBanner=function() return true end,GetBannerPath=function() return first end}
        assert(b:LoadFromSong(song)==b);metadata(b:GetTexture())
        b:SetStateProperties{{Frame=1,Delay=.4},{Frame=0,Delay=.8}}
        b:SetScrolling(true,.75):LoadFromCachedBanner(second)
        assert(b:GetScrolling() and b:GetPercentScrolling()==.75)
        log(b:GetWidth(),b:GetHeight(),b:GetAnimationLengthSeconds(),b:GetState());metadata(b:GetTexture())
        course={GetBannerPath=function(self) log('banner',self==course);return '  ' end,
            GetBackgroundPath=function(self) log('background',self==course);return '  '..first..'  ' end}
        b:LoadFromCourse(course);log(b:GetScrolling(),b:GetPercentScrolling());metadata(b:GetTexture())
        course.GetBannerPath=function() return second end
        course.GetBackgroundPath=function() error('must not query fallback') end
        b:LoadFromCourse(course);metadata(b:GetTexture())
        b:LoadFromCourse(false):LoadFromCourse({});metadata(b:GetTexture())
        b:LoadIconFromCharacter({GetIconPath=function() return first end});metadata(b:GetTexture())
        assert(not b:GetScrolling() and b:GetPercentScrolling()==0)
        b:SetScrolling(true,.5):LoadCardFromCharacter({GetCardPath=function() return second end})
        metadata(b:GetTexture());assert(not b:GetScrolling())
        b:LoadBannerFromUnlockEntry({GetBannerFile=function() return first end});metadata(b:GetTexture())
        b:LoadBackgroundFromUnlockEntry({GetBackgroundFile=function() return second end});metadata(b:GetTexture())
        b:LoadIconFromCharacter(nil);log(b.Texture)
        b:LoadCardFromCharacter({GetCardPath=function() return 9 end});log(b.Texture)
        b:LoadBannerFromUnlockEntry({GetBannerFile=function() return '' end});log(b.Texture)
        b:LoadBackgroundFromUnlockEntry({GetBackgroundFile=function() return ' \t' end});log(b.Texture)
        b:LoadFromCachedBanner(nil);assert(b:GetTexture()==nil)
    "#);
}

#[test]
fn banner_song_loader_uses_real_blank_bitmap_and_lossy_song_paths() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.capture(false);
    for source in [
        "nil",
        "{}",
        "{HasBanner=function() return false end,GetBannerPath=function() error('unexpected') end}",
        "{HasBanner=function() return true end,GetBannerPath=function() return '' end}",
        "{HasBanner=function() return true end,GetBannerPath=function() return 19 end}",
    ] {
        pair.script(&format!("b:SetScrolling(true,.5):LoadFromSong({source}); assert(not b:GetScrolling() and b:GetPercentScrolling()==0);metadata(b:GetTexture());log(b:GetWidth(),b:GetHeight())"));
    }
    let texture =
        deadlib_assets::generated_texture("Themes/_fallback/Graphics/_blank.png").unwrap();
    assert_eq!(texture.source_size, [4, 2]);
    pair.script(r#"
        b:LoadFromSong({HasBanner=function() return 0 end,GetBannerPath=function()return string.char(255)..'.png' end})
        log(b.Texture)
        b:LoadFromSong({HasBanner=function() return true end,GetBannerPath=function()return '  ' end})
        assert(b.Texture=='  ');metadata(b:GetTexture())
        assert(not pcall(b.LoadFromSong,b,{HasBanner={}}))
        assert(not pcall(b.LoadFromSong,b,{HasBanner=function()return true end,GetBannerPath=false}))
    "#);
}

#[test]
fn group_sort_and_reentrant_path_getters_match_error_publication() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.script(r#"
        SONGMAN=nil;b:SetScrolling(true,.3):LoadFromSongGroup('x');assert(b:GetScrolling())
        SONGMAN={};b:LoadFromSongGroup('x');assert(b:GetScrolling())
        SONGMAN=false;assert(not pcall(b.LoadFromSongGroup,b,'x'));assert(b:GetScrolling())
        SONGMAN={GetSongGroupBannerPath=false};assert(not pcall(b.LoadFromSongGroup,b,'x'))
        SONGMAN=setmetatable({}, {__index=function(self,key)
            assert(key=='GetSongGroupBannerPath');collectgarbage('collect')
            return function(owner,group) assert(owner==self);log(group);b:x(17);collectgarbage('collect');return second end
        end})
        b:LoadFromSongGroup('group');metadata(b:GetTexture());assert(not b:GetScrolling())
        b:LoadFromSongGroup(23);metadata(b:GetTexture())
        SONGMAN={GetSongGroupBannerPath=function()return false end}
        b:LoadFromSongGroup('x');log(b.Texture)
        SONGMAN.GetSongGroupBannerPath=function()return '  ' end
        b:LoadFromSongGroup('x');log(b.Texture)
        for _,sort in ipairs{'SortOrder_Recent_P1','SortOrder_Title_P2','Invalid','',false,'SortOrder_Group'} do
            b:SetScrolling(true,.9):LoadFromSortOrder(sort);log(b.Texture,b:GetScrolling(),b:GetPercentScrolling())
        end
        local source=setmetatable({}, {__index=function(self,key)
            collectgarbage('collect');b:y(23)
            return function(owner) assert(owner==self);collectgarbage('collect');return first end
        end})
        b:LoadIconFromCharacter(source);metadata(b:GetTexture())
        b:SetScrolling(true,.4)
        assert(not pcall(b.LoadCardFromCharacter,b,{GetCardPath=function()return string.char(255) end}))
        assert(b:GetScrolling() and b:GetPercentScrolling()>.39)
        assert(not pcall(b.LoadFromCourse,b,{GetBannerPath=setmetatable({},{__call=function()return first end})}))
        local old=b.Texture
        assert(not pcall(b.LoadFromSong,b,{HasBanner=function()return true end,GetBannerPath=function()error('path error')end}))
        assert(b.Texture==old and b:GetScrolling())
    "#);
}

#[test]
fn current_background_helper_keeps_dynamic_calls_and_theme_provenance() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.source_probe();
    pair.script(r#"
        assert(type(rawget(a,'LoadFromCurrentSongBackground'))=='function')
        assert(a.LoadFromCurrentSongBackground~=b.LoadFromCurrentSongBackground)
        assert(not pcall(Sprite.LoadFromCurrentSongBackground,b))
        local helper=b.LoadFromCurrentSongBackground
        local public=Sprite.LoadFromCurrentSongBackground
        Sprite.LoadFromCurrentSongBackground=false
        assert(b.LoadFromCurrentSongBackground==helper);Sprite.LoadFromCurrentSongBackground=public
        song={GetBackgroundPath=function(self) assert(self==song);return first end}
        GAMESTATE={GetCurrentSong=function(self) assert(self==GAMESTATE);return song end}
        assert(b:LoadFromCurrentSongBackground()==b);metadata(b:GetTexture())
        local native=Sprite.LoadFromSongBackground
        Sprite.LoadFromSongBackground=function(self,s) assert(self==b and s==song);log('override');return false end
        assert(b:LoadFromCurrentSongBackground()==b);Sprite.LoadFromSongBackground=native
        GAMESTATE.GetCurrentSong=function()return nil end
        GAMESTATE.GetMasterPlayerNumber=function()return 'PlayerNumber_P2' end
        GAMESTATE.GetCurrentTrail=function(self,player)
            assert(player=='PlayerNumber_P2')
            return {GetTrailEntries=function() return {{GetSong=function()return song end},{GetSong=function()error('only first')end}} end}
        end
        b:LoadFromCurrentSongBackground();metadata(b:GetTexture())
        GAMESTATE.GetCurrentTrail=function()return {GetTrailEntries=function()return {} end} end
        assert(b:LoadFromCurrentSongBackground()==b)
        GAMESTATE.GetCurrentTrail=function()return nil end;assert(not pcall(b.LoadFromCurrentSongBackground,b))
        GAMESTATE.GetCurrentSong=function()return song end
        song.GetBackgroundPath=function()
            collectgarbage('collect');assert(source_probe('@theme:/Scripts/02 Sprite.lua'));return second
        end
        b:LoadFromCurrentSongBackground();metadata(b:GetTexture())
        local crop=Sprite.CropTo
        Sprite.CropTo=function(self,w,h)assert(self==b and w==9 and h==7);assert(source_probe('_fallback/Scripts/02 Sprite.lua'));log('crop');return false end
        assert(b:cropto(9,7)==b);Sprite.CropTo=crop
    "#);
}

#[test]
fn banner_live_texture_events_animation_and_retained_handles() {
    let files = Files::new();
    let mut pair = Pair::new(&files, true);
    pair.capture(false);
    pair.script(r#"
        a:SetStateProperties{{Frame=0,Delay=.2},{Frame=1,Delay=.4}}
        assert(a:loop(false):rate('1.5'):position(.3)==a);log(a:GetState())
        a:position(false):rate({}):loop({});log(a:GetState())
        h=a:GetTexture();metadata(h)
        root:SetUpdateFunction(function(self,dt)
            count=(count or 0)+1;collectgarbage('collect')
            if count==1 then a:LoadFromSong({HasBanner=function()return true end,GetBannerPath=function()return second end})
            elseif count==2 then a:Load(first)
            else a:position(.4):rate(2):loop(false) end
            log(a:GetWidth(),a:GetHeight(),a:GetState());metadata(a:GetTexture());metadata(h)
        end)
    "#);
    for delta in [0.1, 0.2, 0.4] {
        pair.advance(delta);
    }
    pair.rust.release_actor_scene(pair.scene).unwrap();
    execute(
        &mut pair.rust,
        "collectgarbage('collect');assert(not pcall(a.LoadFromSong,a,nil));metadata(h)",
    );
}
impl Files {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
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
        .execute(&compile_named(source.as_bytes(), b"@/fixture/banners.lua").unwrap())
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
        reference_test::register_path(std::path::Path::new(&files.dir));
        let source = format!(
            r#"
            directory={:?};first={:?};second={:?};observations={{}}
            function log(...) for i=1,select('#',...) do local v=select(i,...)
                observations[#observations+1]=type(v)=='number' and string.format('%.17g',v) or tostring(v) end end
            function metadata(t) log(t:GetPath(),t:GetSourceWidth(),t:GetSourceHeight(),t:GetTextureWidth(),t:GetTextureHeight(),t:GetImageWidth(),t:GetImageHeight(),t:GetNumFrames()) end
            return Def.ActorFrame{{Def.Banner{{Name='a',Texture=first,_Dir=directory}},Def.Banner{{Name='b',_Dir=directory}},Def.Sprite{{Name='q',_Dir=directory}}}}
        "#,
            files.dir, files.first, files.second
        );
        let mut context = SongLuaCompileContext::new(&files.dir, "banner migration");
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
        reference_test::assert_debug(
            "song/tests/rust_banners.rs::observations",
            &(phase, &self.rust.vm_mut().string_bytes(actual).unwrap()),
        );
        let actual = self
            .rust
            .actor_scene_snapshot(self.scene)
            .unwrap()
            .into_iter()
            .map(|actor| actor.render)
            .collect::<Vec<_>>();
        reference_test::assert_debug("song/tests/rust_banners.rs::render", &(phase, &actual));
        let actual = self
            .rust
            .take_sprite_textures()
            .into_iter()
            .map(|(id, t)| format!("{} {t:?}", self.ids[&id]))
            .collect::<Vec<_>>();
        reference_test::assert_debug("song/tests/rust_banners.rs::textures", &(phase, &actual));
        reference_test::assert_debug(
            "song/tests/rust_banners.rs::judgment_textures",
            &(phase, &self.rust.take_judgment_textures()),
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
    fn source_probe(&mut self) {
        // Inspect actual VM activation records for host caller provenance.

        execute(
            &mut self.rust,
            r#"
            local getinfo=debug.getinfo
            function source_probe(expected)
                for i=1,32 do local info=getinfo(i,'S')
                    if not info then return false end
                    if info.source==expected then return true end
                end
                return false
            end
        "#,
        );
    }
    fn capture(&mut self, queued: bool) {
        self.rust.begin_actor_capture(queued);
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
