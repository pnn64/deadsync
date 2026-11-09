//! Existing DeepCopy's live pose export, separate from declaration cloning.
use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    self as song, SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime},
};

struct Pair {
    rust: RustSongRuntime,
    scene: ActorScene,
    seconds: f64,
}

impl Pair {
    fn new(definition: &str) -> Self {
        Self::with_context(true, definition)
    }
    fn with_context(live: bool, definition: &str) -> Self {
        let mut context =
            SongLuaCompileContext::new(std::path::Path::new("."), "existing live actor copy");
        context.live = live;

        let mut rust = RustSongRuntime::new(&context).unwrap();
        if !live {
            rust.begin_actor_preparation().unwrap();
        }
        let definition = rust
            .vm_mut()
            .execute(&compile_named(definition.as_bytes(), b"@/fixture/copy.lua").unwrap())
            .unwrap()[0];
        let scene = rust.instantiate_actor(definition).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        let mut pair = Self {
            rust,
            scene,
            seconds: 0.,
        };
        pair.run(OBSERVE);
        pair
    }
    fn run(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/copy-control.lua").unwrap())
            .unwrap_or_else(|e| panic!("Rust: {source}\n{e:?}"));
    }
    fn promote(&mut self) {
        self.rust.begin_actor_playback();
    }
    fn compare(&mut self) {
        self.compare_string("return take_copy()");
    }
    fn compare_string(&mut self, source: &str) {
        let output = self
            .rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"=copied fields").unwrap())
            .unwrap()
            .to_vec();
        let [Value::String(id)] = output.as_slice() else {
            panic!("copy snapshot")
        };
        let actual = std::str::from_utf8(self.rust.vm_mut().string_bytes(*id).unwrap()).unwrap();
        reference_test::assert_debug(
            "song/tests/rust_actor_copy.rs::observations",
            &actual.as_bytes(),
        );
    }
    fn advance(&mut self, delta: f64) {
        self.seconds += delta;

        self.rust.advance_actor_scene(self.scene, delta).unwrap();
        self.compare();
    }
}

#[test]
fn copied_actor_fields_match_before_live_promotion() {
    Pair::new("return Def.Quad{}").compare();
    let mut pair = Pair::new(
        "return Def.Quad{InitCommand=function(self)self:x(12):basezoomx(1):glow(1,.5,.25,.75):diffuse(.25,.5,.75,1):cropleft(.1)end}",
    );
    pair.compare();
}

#[test]
fn preparation_copy_keeps_authored_pose_and_explicit_default_base_fields() {
    Pair::with_context(false, "return Def.Quad{}").compare();
    let mut pair = Pair::with_context(
        false,
        "return Def.Quad{InitCommand=function(self)self:x(12):basezoomx(1):glow(1,.5,.25,.75):diffuse(.25,.5,.75,1):cropleft(.1)end}",
    );
    pair.compare();
}

#[test]
fn immediate_flags_keep_their_raw_presence_before_playback() {
    for live in [false, true] {
        let mut pair = Pair::with_context(live, "return Def.Quad{}");
        for source in [
            "root:ztest(false)",
            "root:zwrite(true)",
            "root:SetTextureFiltering(true)",
            "root:MaskSource():MaskDest()",
        ] {
            pair.run(source);
            pair.compare();
        }
        pair.run("assert(root.SetDrawByZPosition==nil)");
        let mut frame = Pair::with_context(live, "return Def.ActorFrame{}");
        frame.run("root:SetDrawByZPosition(false)");
        frame.compare();
    }
}

#[test]
fn sampled_base_getters_keep_copy_presence_and_values() {
    let mut pair = Pair::new("return Def.Quad{}");
    pair.promote();
    for (target, value) in [
        (song::SongLuaOverlayUpdateTarget::BaseZoomX, 1.),
        (song::SongLuaOverlayUpdateTarget::BaseZoom, 2.),
        (song::SongLuaOverlayUpdateTarget::HAlign, 0.5),
    ] {
        let values = [(0, target, song::SongLuaOverlayUpdateValue::F32(value))];

        let actor = pair.rust.actor_scene_root(pair.scene).unwrap();
        pair.rust
            .set_song_overlay_getter_values(pair.scene, &[actor], &values)
            .unwrap();
        pair.compare();
    }
}

#[test]
fn copied_pose_materializes_as_an_independent_actor_declaration() {
    let mut pair = Pair::new("return Def.Quad{}");
    pair.promote();
    pair.run(
        "root:linear(1):x(23):zoomy(3):basezoom(2):basezoomx(1):halign(.25):diffuse(.25,.5,.75,1)",
    );
    pair.compare();

    let declaration = pair.rust.vm_mut().get_global(b"copied").unwrap();
    let scene = pair.rust.instantiate_actor(declaration).unwrap();
    let cloned = pair.rust.actor_scene_root(scene).unwrap();
    pair.rust.vm_mut().set_global(b"cloned", cloned).unwrap();
    pair.compare_string("return table.concat({cloned:GetX(),cloned:GetDestX(),cloned:GetZoomY(),cloned:GetBaseZoomX(),cloned:GetBaseZoomY(),cloned:GetHAlign(),unpack(cloned:GetDiffuse())},'|')");
    pair.run("root:x(99):basezoom(4):diffuse(0,0,0,0);collectgarbage()");
    pair.compare_string("return table.concat({cloned:GetX(),cloned:GetDestX(),cloned:GetZoomY(),cloned:GetBaseZoomX(),cloned:GetBaseZoomY(),cloned:GetHAlign(),unpack(cloned:GetDiffuse())},'|')");
    pair.rust.release_actor_scene(scene).unwrap();
}

const OBSERVE: &str = r#"
local keys={
    '__songlua_diffuse','__songlua_state_diffuse','__songlua_state_glow','__songlua_state_vertex_colors',
    '__songlua_state_aux','__songlua_state_x','__songlua_state_y','__songlua_state_z',
    '__songlua_state_zoom','__songlua_state_zoom_x','__songlua_state_zoom_y','__songlua_state_zoom_z',
    '__songlua_state_skew_x','__songlua_state_skew_y',
    '__songlua_state_cropleft','__songlua_state_cropright','__songlua_state_croptop','__songlua_state_cropbottom',
    '__songlua_state_fadeleft','__songlua_state_faderight','__songlua_state_fadetop','__songlua_state_fadebottom',
    '__songlua_aux','__songlua_current_x','__songlua_current_y','__songlua_current_z',
    '__songlua_state_draw_by_z_position','__songlua_state_mask_source','__songlua_state_mask_dest',
    '__songlua_state_depth_test','__songlua_state_depth_write','__songlua_state_depth_test_on_fail',
    '__songlua_state_texture_filtering','__songlua_state_texture_wrapping',
    '__songlua_state_basezoom','__songlua_state_basezoom_x','__songlua_state_basezoom_y','__songlua_state_basezoom_z',
    '__songlua_state_halign','__songlua_state_valign'
}
local function encode(v)
    if type(v)=='table' then
        local out={};for i,item in ipairs(v)do out[i]=encode(item)end;return '{'..table.concat(out,',')..'}'
    elseif type(v)=='number' then return v~=v and 'nan' or string.format('%.17g',v)
    else return tostring(v)end
end
function take_copy()
    copied=DeepCopy(root);assert(copied~=root and getmetatable(copied)==nil)
    collectgarbage('collect')
    local out={};for _,key in ipairs(keys)do out[#out+1]=key..'='..encode(rawget(copied,key))end
    return table.concat(out,'\n')
end
"#;

#[test]
fn promoted_copy_exports_exact_pose_flags_and_absent_base_fields() {
    let mut pair = Pair::new("return Def.Quad{}");
    pair.promote();
    pair.compare();
    pair.run("assert(copied.__songlua_diffuse~=copied.__songlua_state_diffuse);assert(copied.__songlua_state_basezoom_x==nil)");
}

#[test]
fn live_copy_keeps_tween_destination_current_pose_and_independent_colors() {
    let mut pair = Pair::new("return Def.Quad{InitCommand=function(self) self:xy(3,4):aux(5) end}");
    pair.promote();
    pair.run(
        r#"
        root:linear(1):xy(23,44):z(7):aux(9):zoomy(3):skewx(.25):fadeleft(.5)
        root:diffuse(.25,.5,.75,1):glow(1,.75,.5,.25):diffuseupperleft(.1,.2,.3,.4)
        root:ztest(true):SetTextureFiltering(false):basezoom(2):basezoomx(1):halign(.25)
    "#,
    );
    pair.compare();
    pair.advance(0.25);
    pair.run(
        r#"
        saved=copied;root:diffuse(0,0,0,0):x(99):ztest(false):basezoomx(4)
        collectgarbage();assert(saved.__songlua_state_x==23 and saved.__songlua_state_basezoom_x==1)
        assert(saved.__songlua_state_depth_test==true)
        assert(saved.__songlua_state_glow[1]==1 and saved.__songlua_state_glow[4]==.25)
    "#,
    );
    pair.compare();
    pair.advance(0.8);
}

#[test]
fn explicit_default_base_fields_and_alignment_aliases_keep_their_presence() {
    let mut pair = Pair::new("return Def.Quad{}");
    pair.promote();
    for source in [
        "root:basezoomx(1)",
        "root:align(.5,.5)",
        "root:horizalign('left'):vertalign('bottom')",
        "root:halign(.5):valign(.5):basezoom(1)",
        "root:basezoomy(-0.0):basezoomz(-2)",
    ] {
        pair.run(source);
        pair.compare();
    }
}

#[test]
fn nested_copy_and_retained_function_keep_actor_pose_after_gc() {
    let mut pair = Pair::new("return Def.Quad{}");
    pair.promote();
    pair.run(r#"
        root:basezoomx(1):x(12):diffuse(.25,.5,.75,1)
        retained_copy=DeepCopy;DeepCopy=nil;collectgarbage()
        local out=retained_copy({first=root,second=root})
        assert(out.first~=out.second and out.first.__songlua_state_x==12 and out.second.__songlua_state_x==12)
        assert(out.first.__songlua_state_diffuse~=out.second.__songlua_state_diffuse)
        root:x(24):diffuse(0,0,0,0);collectgarbage()
        assert(out.first.__songlua_state_x==12 and out.first.__songlua_state_diffuse[1]==.25)
        DeepCopy=retained_copy
    "#);
    pair.compare();
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
