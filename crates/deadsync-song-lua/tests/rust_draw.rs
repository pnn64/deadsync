use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaDrawFrame, SongLuaDrawOp, SongLuaDrawSource,
    rust_engine::{ActorScene, RustSongRuntime},
};

fn execute(runtime: &mut RustSongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/draw.lua").unwrap())
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"))
        .to_vec()
}
const NAMES: &[&str] = &[
    "root", "q", "nested", "nq", "a", "aq", "b", "bq", "proxy", "logic", "mesh",
];
struct Pair {
    rust: RustSongRuntime,
    scene: ActorScene,
}
impl Pair {
    fn new(live: bool) -> Self {
        let source = r#"
            observations={};function log(s) observations[#observations+1]=s end
            return Def.ActorFrame {Name='root',
                Def.Quad{Name='q'},
                Def.ActorFrame{Name='nested', Def.Quad{Name='nq'}},
                Def.ActorFrameTexture{Name='a',Def.Quad{Name='aq'}},
                Def.ActorFrameTexture{Name='b',Def.Quad{Name='bq'}},
                Def.ActorProxy{Name='proxy'},Def.Actor{Name='logic'},Def.ActorMultiVertex{Name='mesh'}
            }
        "#;
        let mut context = SongLuaCompileContext::new(std::path::Path::new("."), "draw migration");
        context.live = live;

        let mut rust = RustSongRuntime::new(&context).unwrap();
        let definition = execute(&mut rust, source)[0];
        let scene = rust.instantiate_actor(definition).unwrap();
        let root = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"root", root).unwrap();
        let mut pair = Self { rust, scene };
        pair.script("q=root[1];nested=root[2];nq=nested[1];a=root[3];aq=a[1];b=root[4];bq=b[1];proxy=root[5];logic=root[6];mesh=root[7]");
        pair
    }
    fn script(&mut self, source: &str) {
        execute(&mut self.rust, source);
    }
    fn draw(&mut self, expected_error: Option<&str>) {
        let actual = self.rust.draw_actor_scene(self.scene);
        if let Some(message) = expected_error {
            assert!(actual.unwrap_err().to_string().contains(message));
        } else {
            assert!(actual.unwrap());
        }
        self.script("assert(__songlua_draw_callback_active==nil)");
        let probe = "return table.concat(observations,'|')";
        let Value::String(actual) = execute(&mut self.rust, probe)[0] else {
            panic!()
        };
        reference_test::assert_debug(
            "song/tests/rust_draw.rs::observations",
            &self.rust.vm_mut().string_bytes(actual).unwrap(),
        );
    }
    fn take(&mut self) -> Vec<(usize, SongLuaDrawFrame)> {
        let names: Vec<_> = NAMES
            .iter()
            .enumerate()
            .map(|(i, &name)| (name, SongLuaDrawSource::Overlay(i)))
            .collect();
        let sources: Vec<_> = names
            .iter()
            .map(|(name, source)| {
                (
                    self.rust.vm_mut().get_global(name.as_bytes()).unwrap(),
                    *source,
                )
            })
            .collect();
        let actual = self.rust.take_actor_draw_frames(&sources).unwrap();
        reference_test::assert_debug("song/tests/rust_draw.rs::frames", &actual);
        actual
    }
}

#[test]
fn nested_targets_restore_caller_stacks_and_freeze_each_submission() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        pair.script(r#"
            root:x(10):fov(70);q:SetSize(9,7);a:SetSize(16,8):Create();b:Create();ta=a:GetTexture();tb=b:GetTexture()
            root:SetDrawFunction(function(self)
                q:x(1):Draw();assert(ta:BeginRenderingTo(true)==ta);q:x(2):Draw()
                tb.BeginRenderingTo(false);q:x(3):Draw();tb.FinishRenderingTo()
                q:x(4):Draw();assert(ta:FinishRenderingTo()==ta);q:x(5):Draw()
            end)
        "#);
        pair.draw(None);
        let frames = pair.take();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].1.ops.len(), 9);
        let SongLuaDrawOp::Draw {
            state,
            parents,
            camera,
            ..
        } = &frames[0].1.ops[0]
        else {
            panic!()
        };
        assert_eq!(state.x, 1.0);
        assert_eq!(parents.len(), 1);
        assert!(camera.is_some());
        let SongLuaDrawOp::Draw {
            state,
            parents,
            camera,
            ..
        } = &frames[0].1.ops[2]
        else {
            panic!()
        };
        assert_eq!(state.x, 2.0);
        assert!(parents.is_empty());
        assert!(camera.is_none());
    }
}

#[test]
fn frame_traversal_wrappers_visibility_and_proxy_leaves_match() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        pair.script(r#"
            nested:x(3):fov(50);nq:x(4);nq:AddWrapperState():y(5);nq:AddWrapperState():z(6)
            a:Create():EnablePreserveTexture(true);aq:diffusealpha(.7)
            proxy:SetTarget(q)
            root:SetDrawFunction(function() nested:Draw();a:Draw();b:Draw();logic:Draw();proxy:Draw() end)
        "#);
        pair.draw(None);
        let frames = pair.take();
        assert_eq!(frames[0].1.ops.len(), 5);
        for source in [
            "nq:visible(false)",
            "nq:visible(true);nested:hibernate(2)",
            "nested:hibernate(0);nq:GetWrapperState(1):visible(false)",
            "nq:GetWrapperState(1):visible(true);root:diffusealpha(0)",
            "root:glow{1,1,1,.5}",
        ] {
            pair.script(source);
            pair.draw(None);
            pair.take();
        }
    }
}

#[test]
fn immediate_draws_preserve_call_time_pose_and_live_frames_replace_only_ordinary_frames() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        pair.script("q:x(7):Draw():x(9);root:SetDrawFunction(function() q:Draw() end)");
        pair.draw(None);
        pair.script("q:x(11)");
        pair.draw(None);
        let frames = pair.take();
        assert_eq!(frames.len(), if live { 2 } else { 3 });
        assert!(frames[0].1.immediate);
        let SongLuaDrawOp::Draw { state, .. } = frames[0].1.ops[0] else {
            panic!()
        };
        assert_eq!(state.x, 7.0);
        pair.script("root:SetDrawFunction(nil)");
        pair.draw(None);
        pair.take();
    }
}

#[test]
fn callbacks_keep_partial_draws_report_once_and_defer_queued_commands() {
    let mut pair = Pair::new(true);
    pair.script(r#"
        root:addcommand('Later',function() log('queued');q:x(20) end)
        root:SetDrawFunction(function(self) log('draw');q:Draw();self:queuecommand('Later');error('draw failure') end)
    "#);
    for _ in 0..2 {
        pair.draw(None);
        pair.take();
    }
    assert_eq!(pair.rust.take_actor_diagnostics().len(), 1);
    pair.script("assert(#observations==2);assert(root.__songlua_draw_error_reported)");

    pair.rust.set_values(0.2, 0.1).unwrap();
    pair.rust.advance_actor_scene(pair.scene, 0.1).unwrap();
    pair.script("assert(#observations==4 and observations[3]=='queued');root:SetDrawFunction(function() q:Draw() end)");
    pair.draw(None);
    pair.take();
}

#[test]
fn unbalanced_and_mismatched_targets_keep_errors_and_recover_next_live_frame() {
    let mut pair = Pair::new(true);
    pair.script("a:Create();b:Create();ta=a:GetTexture();tb=b:GetTexture();root:SetDrawFunction(function() ta:BeginRenderingTo();q:Draw();tb:FinishRenderingTo() end)");
    pair.draw(Some("left a render target active"));
    pair.take();
    assert_eq!(pair.rust.take_actor_diagnostics().len(), 1);
    pair.script("root:SetDrawFunction(function() ta:BeginRenderingTo();q:Draw();ta:FinishRenderingTo() end)");
    pair.draw(None);
    pair.take();
    pair.script(
        "root:SetDrawFunction(function() assert(not pcall(ta.FinishRenderingTo));q:Draw() end)",
    );
    pair.draw(None);
    pair.take();
}

#[test]
fn traced_callbacks_and_textures_survive_reentry_then_collect() {
    let mut pair = Pair::new(true);
    pair.script(
        r#"
        a:Create();ta=a:GetTexture();root:SetDrawFunction(function(self)
            self:SetDrawFunction(nil);collectgarbage();assert(ta:BeginRenderingTo(0)==ta)
            q:Draw();collectgarbage();ta:FinishRenderingTo();nested:playcommand('Again')
        end)
        nested:addcommand('Again',function() collectgarbage();q:x(12):Draw() end)
    "#,
    );
    pair.draw(None);
    pair.take();
    pair.script("assert(root:GetDrawFunction()==nil);ta=nil");
    assert!(pair.rust.release_actor_scene(pair.scene).unwrap());
    execute(
        &mut pair.rust,
        "weak=setmetatable({root},{__mode='v'});root=nil;q=nil;nested=nil;nq=nil;a=nil;aq=nil;b=nil;bq=nil;proxy=nil;logic=nil;mesh=nil;collectgarbage();collectgarbage();assert(weak[1]==nil)",
    );
    assert!(!pair.rust.draw_actor_scene(pair.scene).unwrap());
}

#[test]
fn preparation_operation_limit_and_live_high_draw_count_match() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        pair.script("root:SetDrawFunction(function() for i=1,300 do q:x(i):Draw() end end)");
        pair.draw(if live {
            None
        } else {
            Some("exceeds its song budget")
        });
        let frames = pair.take();
        assert_eq!(frames[0].1.ops.len(), if live { 300 } else { 256 });
    }
}

#[test]
fn draw_function_binding_validation_and_mesh_payloads_match() {
    let mut pair = Pair::new(true);
    pair.script(r#"
        assert(root:GetDrawFunction()==nil)
        for _,v in ipairs{false,1,'bad',{}} do assert(not pcall(root.SetDrawFunction,root,v)) end
        local f=function() mesh:Draw() end;assert(root:SetDrawFunction(f)==root and root:GetDrawFunction()==f)
        mesh:SetVertices{{{0,0,0},{1,0,0,1},{0,0}},{{10,0,0},{0,1,0,.5},{1,0}},{{0,10,0},{0,0,1,1},{0,1}}}
        mesh:SetDrawState{Mode='DrawMode_Triangles',First=1,Num=3}
    "#);
    pair.draw(None);
    let frames = pair.take();
    let SongLuaDrawOp::Draw { vertices, .. } = &frames[0].1.ops[0] else {
        panic!()
    };
    assert_eq!(vertices.as_ref().unwrap().len(), 3);
}

#[test]
fn preparation_preserves_all_owners_and_quiet_tails() {
    let mut pair = Pair::new(false);
    pair.script("root:SetDrawFunction(function() q:Draw() end)");
    pair.draw(None);
    pair.script("root:SetDrawFunction(nil);nested:SetDrawFunction(function() nq:Draw() end)");
    pair.draw(None);
    pair.script("nested:SetDrawFunction(nil)");
    pair.draw(None);
    let frames = pair.take();
    assert_eq!(frames.len(), 6);
    assert_eq!(
        frames
            .iter()
            .map(|(_, frame)| frame.ops.len())
            .sum::<usize>(),
        2
    );
    assert!(frames[4..].iter().all(|(_, frame)| frame.ops.is_empty()));
}

#[test]
fn unmapped_sources_fail_conversion_then_allow_new_capture() {
    let mut pair = Pair::new(true);
    pair.script("root:SetDrawFunction(function() q:Draw() end)");
    pair.draw(None);
    let root = pair.rust.actor_scene_root(pair.scene).unwrap();
    let actual = pair
        .rust
        .take_actor_draw_frames(&[(root, SongLuaDrawSource::Overlay(0))])
        .unwrap_err();
    reference_test::assert_debug("song/tests/rust_draw.rs::conversion_error", &actual);
    assert!(actual.contains("uncompiled actor"));
    pair.draw(None);
    pair.take();
}

#[test]
fn transition_coercion_and_binary_sprite_predicates_preserve_native_reads() {
    let mut pair = Pair::new(false);
    pair.script("root:SetDrawFunction(function() q:Draw() end)");
    for value in ["0", "-0.5", "1.9", "'2'", "255.9"] {
        pair.script(&format!("q.__songlua_transition_state={value}"));
        pair.draw(None);
        pair.take();
    }
    for value in ["-1", "256", "0/0", "1/0"] {
        pair.script(&format!("q.__songlua_transition_state={value}"));
        pair.draw(Some("out of range"));
        pair.take();
    }
    pair.script("q.__songlua_transition_state=nil;q.__songlua_actor_type='Sprite';q.__songlua_state_sprite_texture=true;q.Texture=string.char(255)");
    pair.draw(None);
    pair.take();
}

#[test]
fn draw_samples_tween_and_spin_state_at_each_update_boundary() {
    for live in [false, true] {
        let mut pair = Pair::new(live);
        pair.script("q:x(2):linear(2):x(42):zoom(3);nq:spin():effectmagnitude(0,0,40);root:SetDrawFunction(function() q:Draw();nested:Draw() end)");
        pair.draw(None);
        pair.take();
        let mut seconds = 0.0;
        for delta in [0.25, 0.5, 1.5] {
            seconds += delta;

            pair.rust
                .set_values(seconds as f32 * 2.0, seconds as f32)
                .unwrap();

            pair.rust.advance_actor_scene(pair.scene, delta).unwrap();
            pair.draw(None);
            pair.take();
        }
    }
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
