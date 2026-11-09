use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime},
};

struct Pair {
    rust: RustSongRuntime,
    scene: ActorScene,
}

fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/render.lua").unwrap())
        .unwrap()
        .to_vec()
}

impl Pair {
    fn new(definition: &str) -> Self {
        let source = format!(
            r#"
            observations={{}}
            function log(...) for i=1,select('#',...) do
                local v=select(i,...)
                observations[#observations+1]=type(v)=='number' and string.format('%.17g',v) or tostring(v)
            end end
            function observe(a)
                log(a:GetDiffuseAlpha(),a:GetBaseZoomX(),a:GetBaseZoomY(),a:GetBaseZoomZ(),a:GetHAlign(),a:GetVAlign())
                log(a:GetZoomedWidth(),a:GetZoomedHeight(),a:GetX(),a:GetY(),a:GetDestX(),a:GetDestY())
                log(a:getrotation())
                log(unpack(a:GetDiffuse()));log(unpack(a:GetGlow()))
            end
            {definition}
        "#
        );

        let mut rust = RustSongRuntime::new(&SongLuaCompileContext::new(
            std::path::Path::new("."),
            "render migration",
        ))
        .unwrap();
        let declaration = execute(&mut rust, &source)[0];
        let scene = rust.instantiate_actor(declaration).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        let mut pair = Self { rust, scene };
        pair.compare("construction");
        pair
    }
    fn compare(&mut self, phase: &str) {
        let actual = self
            .rust
            .actor_scene_snapshot(self.scene)
            .unwrap()
            .into_iter()
            .map(|actor| actor.render)
            .collect::<Vec<_>>();
        reference_test::assert_debug("song/tests/rust_render.rs::render", &(phase, &actual));
        let Value::String(actual) =
            execute(&mut self.rust, "return table.concat(observations,'|')")[0]
        else {
            panic!("observation string")
        };
        reference_test::assert_debug(
            "song/tests/rust_render.rs::observations",
            &(phase, &self.rust.vm_mut().string_bytes(actual).unwrap()),
        );
    }
    fn script(&mut self, script: &str) {
        execute(&mut self.rust, script);
        self.compare(script);
    }
    fn advance(&mut self, delta: f64) {
        self.rust.advance_actor_scene(self.scene, delta).unwrap();
        self.compare(&format!("advance {delta}"));
        self.script("observe(root)");
    }
}

#[test]
fn declared_camera_fov_and_command_override_match_production() {
    for value in ["0", "'60'", "-1"] {
        let mut pair = Pair::new(&format!("return Def.ActorFrame{{FOV={value}}}"));
        pair.script("root:fov(45)");
        pair.script("root:fov(-1)");
        pair.script("root:fov(0)");
    }
    Pair::new("return Def.ActorFrame{FOV=60,InitCommand=function(self)self:fov(-1)end}");
}

#[test]
fn existing_set_fov_frame_alias_matches_command_and_class_calls() {
    let mut pair = Pair::new("return Def.ActorFrame{InitCommand=function(self)self:SetFOV(45)end}");
    for source in [
        "assert(root:SetFOV(75)==root)",
        "assert(ActorFrame.SetFOV(root,'60')==root)",
        "assert(root:SetFOV(-1)==root)",
        "assert(root:SetFOV(0)==root)",
    ] {
        pair.script(source);
    }
}

#[test]
fn colors_gradients_and_getter_ownership_match_production() {
    let mut pair = Pair::new("return Def.Quad{}");
    for script in [
        "observe(root); assert(root:diffuse(.1,.2,.3,.4)==root); root:glow(.5,.6,.7); observe(root)",
        "root:diffuseupperleft(.1,.2,.3,.4):diffuseupperright(.8,.7,.6,.5):diffuselowerleft('#ff800040'):diffuselowerright({.3,.5,.8}); observe(root)",
        "root:diffuseleftedge('#12345678'):diffuserightedge(.2,.4,.6,.8):diffusetopedge('0.2,0.5,0.8,0.9'):diffusebottomedge({1,2,3,4}); observe(root)",
        "saved=root:GetDiffuse(); assert(saved~=root:GetDiffuse()); saved[1]=99; root:diffusealpha(.25):diffusecolor(.8,.7,.6,0):glow('#aabbccdd'); collectgarbage('collect'); assert(saved[1]==99); observe(root)",
        "root:diffuse('garbage'):glow('0.25'); observe(root)",
        "root:diffuse(1,nil,3):glow(false):diffusecolor({1,2}):diffusealpha(false); observe(root)",
        "root:diffuse({[1]='0x1',[2]='0x2',[3]='0x3'}); root:glow(setmetatable({1,2,3},{__index=function() error('raw read required') end})); observe(root)",
        "root:diffuse({1,2,3,false}):glow(string.char(255)); observe(root)",
    ] {
        pair.script(script);
    }
    pair.rust.release_actor_scene(pair.scene).unwrap();
    execute(
        &mut pair.rust,
        "collectgarbage('collect'); assert(saved[1]==99)",
    );
}

#[test]
fn color_crop_and_quaternion_tweens_match_production() {
    let mut pair = Pair::new(
        "return Def.Quad{InitCommand=function(self) self:diffuse(.2,.3,.4,.5):glow(.1,.2,.3,.4) end}",
    );
    pair.script("root:heading(17):pitch(-31):roll(52):linear(.5):diffuse(1,.8,.6,.4):diffusealpha(.2):glow(1,.5,0,1):diffuseupperleft(.5,.5,.5,.5):cropleft(.2):cropright(.3):croptop(.4):cropbottom(.5):fadeleft(.1):faderight(.2):fadetop(.3):fadebottom(.4):skewx(.25):skewy(-.3):heading(60):pitch(21):roll(-75); observe(root)");
    for delta in [0.0, 0.125, 0.25, 0.125] {
        pair.advance(delta);
    }
    pair.script("root:smooth(.5):diffuse(0,0,0,0):glow(0,0,0,0):diffuselowerleft(0,0,0,0):heading(-30):skewy(1); observe(root)");
    pair.advance(0.2);
    pair.script("root:stoptweening(); observe(root)");
    pair.script("root:linear(.5):diffusecolor(.2,.3,.4):roll(90):queuecommand('Again'); root.AgainCommand=function(self) self:glow(1,0,0,1) end");
    pair.script("root:finishtweening(); observe(root)");
    pair.advance(0.01);
}

#[test]
fn immediate_base_alignment_shadow_and_frame_properties_match_production() {
    let mut pair = Pair::new("return Def.ActorFrame{Def.Quad{}}");
    for script in [
        "root:linear(2):x(20):basezoom(2):basezoomy(3):basezoomz(4):basealpha(.25):baserotationx(20):baserotationy(30):baserotationz(40):shadowlength(5):shadowcolor(.1,.2,.3,.4):horizalign('HorizAlign_Left'):vertalign('VertAlign_Bottom'):fov(60):vanishpoint(20,40); observe(root)",
        "root:shadowlengthx(7):shadowlengthy(-8):halign(.25):valign(.75):basezoomx(-3); observe(root)",
        "root:horizalign(' HorizAlign_HorizAlign_RIGHT '):vertalign('VertAlign_middle'):shadowcolor('#ffccaa55'); observe(root)",
        "root:halign('0.125'):valign('1.25'):horizalign('bad'):vertalign(false):basezoom(false):shadowlength(false):fov(false):vanishpoint(8,false); observe(root)",
        "root:basezoom(5):shadowlength(-3):valign(-2):basealpha(2); observe(root)",
    ] {
        pair.script(script);
    }
    pair.advance(0.5);
    pair.script("root:stoptweening(); observe(root)");
}

#[test]
fn geometry_dimensions_alignment_and_flip_order_match_production() {
    let mut pair = Pair::new("return Def.Quad{}");
    for script in [
        "root:setsize(-4,8):basezoom(2):zoomto(12,16); observe(root)",
        "root:linear(.5):zoomtowidth(20):zoomtoheight(-24); observe(root)",
        "root:stretchto(1,2,41,82); observe(root)",
        "root:halign(0):valign(1):scaletofit(0,0,100,50); observe(root)",
        "root:scaletocover(100,50,0,0); observe(root)",
    ] {
        pair.script(script);
        pair.advance(0.125);
    }
    pair.script("root:zoomto(2,false):zoomtowidth(false):scaletofit(1,2,3,false):stretchto(false,2,nil,4); observe(root)");
    pair.advance(0.25);
    pair.script("root:setsize(0,0):zoomto(4,8); observe(root)");
    pair.script("root:scaletofit(0,0,10,10); observe(root)");
}

#[test]
fn render_flags_propagate_in_call_order_and_keep_strict_arguments() {
    let mut pair = Pair::new("return Def.ActorFrame{Def.ActorFrame{Def.Quad{}},Def.Quad{}}");
    for script in [
        "root:zbuffer(true):backfacecull(true):blend('BlendMode_Add'):SetDrawByZPosition(true):zbias(.25); root[1][1]:zwrite(false)",
        "root[1]:ztestmode('ZTestMode_WriteOnFail'); root[2]:ztest(0.9):cullmode('CullMode_Front'):blend('multiply')",
        "root:ztestmode('off'):zwrite(-1.2); root:backfacecull(0.5):SetTextureFiltering(nil); root[2]:cullmode(2)",
        "root:SetTextureFiltering():SetDrawByZPosition(0):clearzbuffer(); root[1]:SetTextureFiltering(false); root:zbuffer(false)",
        "for _,name in ipairs{'backfacecull','ztest','zwrite','zbuffer','cullmode','ztestmode'} do assert(not pcall(root[name],root,'bad')); assert(not pcall(root[name],root)) end",
    ] {
        pair.script(script);
    }
    pair.advance(0.25);
}

#[test]
fn render_numeric_edge_cases_match_production() {
    let mut pair = Pair::new("return Def.Quad{}");
    for script in [
        "root:diffusealpha(1/0):basealpha(-1/0):cropleft(1/0):croptop(1/0):fadeleft(0/0):skewx(0/0); observe(root)",
        "root:diffusealpha(0/0):basezoom(1/0):baserotationx(1/0):shadowlength(1/0):cropbottom(-1/0); observe(root)",
        "root:diffuse({1/0,-1/0,0/0,1e300}):glow(1/0,0,0,1); observe(root)",
        "root:diffusealpha('inf'):basezoom('inf'):shadowlength('NaN'):cropright('inf'):faderight('-inf'); observe(root)",
        "root:diffuse(1,'0x1',3):glow({1,'0x2',3}); observe(root)",
    ] {
        pair.script(script);
    }
}

#[test]
fn render_callbacks_reenter_force_gc_and_clone_template_state() {
    let mut pair = Pair::new(
        r#"
        NullActor:diffuse(.1,.2,.3,.4):linear(2):diffuse(1,.8,.6,.4):basezoom(2):shadowlength(4):cullmode('front')
        return Def.ActorFrame{NullActor,NullActor,
            PulseMessageCommand=function(self,p)
                self:diffuse(p.color):linear(.5):diffusealpha(.1)
                collectgarbage('collect')
                self[1]:glow(self:GetDiffuse())
                if not p.again then p.again=true; MESSAGEMAN:Broadcast('Pulse',p) end
            end}
    "#,
    );
    pair.script("MESSAGEMAN:Broadcast('Pulse',{color={.3,.4,.5,.6}}); root[1]:basezoomx(5):shadowlength(7); observe(root[1]); observe(root[2])");
    pair.script("root:SetUpdateFunction(function(self,dt) self[2]:diffusecolor(dt,.2,.3):roll(5); collectgarbage('collect') end)");
    for delta in [0.1, 0.2, 0.3] {
        pair.advance(delta);
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
