use super::*;
use deadlib_lua::{Value, compile_named};
use deadlib_render_core::{DrawOp, RenderFrame};
use deadsync_song_lua::{ActorScene, SongRuntime};

fn rgba(bytes: [u8; 4]) -> [f32; 4] {
    bytes.map(|byte| f32::from(byte) / 255.0)
}

fn colors(frame: &RenderFrame) -> Vec<[f32; 4]> {
    assert_eq!(
        frame.ops.len(),
        1,
        "unexpected extra diffuse/glow pass: {:?}",
        frame.ops
    );
    match frame.ops[0] {
        DrawOp::TexturedMesh(run) => {
            let tint = frame.tmesh_instances[run.instance_start as usize].tint;
            frame.tmesh_geometries[run.geometry as usize]
                .vertices
                .iter()
                .map(|vertex| std::array::from_fn(|channel| vertex.color[channel] * tint[channel]))
                .collect()
        }
        DrawOp::Mesh(_) => frame
            .mesh_vertices
            .iter()
            .map(|vertex| vertex.color)
            .collect(),
        ref op => panic!("expected native mesh submission, got {op:?}"),
    }
}

#[test]
fn compiled_manual_amv_draws_inherit_only_normal_frame_colors() {
    crate::tests::init_paths();
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("default.lua");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([255; 4]))
        .save(dir.path().join("white.png"))
        .unwrap();
    std::fs::write(
        &entry,
        r#"
local vertices = {
    {{0,0,0},{.1234,.501,.8761,.1234},{0,0}},
    {{40,0,0},{-.125,1.25,.501,.499},{1,0}},
    {{0,30,0},{.1234,.501,.8761,.1234},{0,1}},
}
local function group(name, custom)
    return Def.ActorFrame {
        Name=name,
        InitCommand=function(self)
            self:xy(120,160):diffuse{.875,.875,.75,.875}
            if custom then self:SetDrawFunction(function(frame) frame[1]:Draw() end) end
        end,
        Def.ActorMultiVertex {
            Name=name..'Mesh',
            InitCommand=function(self)
                self:SetDrawState{Mode='DrawMode_Triangles'}:SetVertices(vertices)
                    :diffuse{.8,.6,.75,.8}
            end,
        },
    }
end
return Def.ActorFrame {
    InitCommand=function(self) self:diffuse{.25,.5,.75,.25}:glow{.8,.5,.3,.5} end,
    OnCommand=function(self)
        local texture=self:GetChild('Image'):GetTexture()
        self[1][1]:SetTexture(texture);self[2][1]:SetTexture(texture)
        self:SetDrawFunction(function(frame)
            for i=1,4 do frame[i]:Draw() end
        end)
        self:Draw()
    end,
    group('NormalTextured',false),group('CustomTextured',true),
    group('NormalPlain',false),group('CustomPlain',true),
    Def.Sprite{Name='Image',Texture='white.png',InitCommand=function(self) self:visible(false) end},
}
"#,
    )
    .unwrap();
    let mut context =
        deadsync_song_lua::SongLuaCompileContext::new(dir.path(), "manual inherited colors");
    context.music_length_seconds = 0.1;
    context.song_timing_bpms = vec![(0.0, 120.0)];
    let compiled = compile_song_lua(&entry, &context).unwrap();
    assert_eq!(compiled.info.unsupported_perframes, 0);
    let states = compiled
        .overlays
        .iter()
        .map(|actor| actor.initial_state)
        .collect::<Vec<_>>();
    let mut composer = actor_conformance::WholeSongComposer::new(&compiled.overlays);
    for immediate in [true, false] {
        let frame = compiled
            .draw_frames
            .iter()
            .find(|frame| frame.immediate == immediate && !frame.ops.is_empty())
            .unwrap();
        assert_eq!(frame.ops.len(), 4);
        for (index, op) in frame.ops.iter().enumerate() {
            let SongLuaDrawOp::Draw {
                inherited_diffuse,
                inherited_glow,
                vertices,
                ..
            } = op
            else {
                panic!("expected recorded mesh")
            };
            assert_eq!(
                *inherited_diffuse,
                if index % 2 == 0 {
                    [0.875, 0.875, 0.75, 0.875]
                } else {
                    [1.0; 4]
                }
            );
            // Native actors start with white glow RGB and zero alpha. Normal
            // groups forward that local color; custom callbacks reset the
            // outgoing scope. Neither may inherit the custom root's glow.
            assert_eq!(
                *inherited_glow,
                if index % 2 == 0 {
                    [1.0, 1.0, 1.0, 0.0]
                } else {
                    [0.0; 4]
                },
                "custom root glow must not leak"
            );
            let vertices = vertices.as_ref().unwrap();
            assert_eq!(
                vertices.iter().map(|vertex| vertex.pos).collect::<Vec<_>>(),
                [[0.0, 0.0], [40.0, 0.0], [0.0, 30.0]]
            );
            assert_eq!(
                vertices.iter().map(|vertex| vertex.uv).collect::<Vec<_>>(),
                [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]
            );
        }
        let rendered = composer.render_recorded_frame(
            &compiled.overlays,
            &states,
            frame,
            [640.0, 480.0],
            frame.second,
            frame.second * 2.0,
        );
        assert_eq!(rendered.len(), 4);
        let [DrawOp::TexturedMesh(normal)] = rendered[0].1.ops.as_slice() else {
            panic!("expected normal textured draw")
        };
        let [DrawOp::TexturedMesh(custom)] = rendered[1].1.ops.as_slice() else {
            panic!("expected callback textured draw")
        };
        assert_eq!(
            rendered[0].1.tmesh_instances[normal.instance_start as usize].transform(),
            rendered[1].1.tmesh_instances[custom.instance_start as usize].transform(),
            "color boundaries must preserve the captured transform stack",
        );
        for (frame, run) in [(&rendered[0].1, normal), (&rendered[1].1, custom)] {
            let geometry = &frame.tmesh_geometries[run.geometry as usize];
            assert_eq!(
                geometry.vertices.iter().map(|v| v.pos).collect::<Vec<_>>(),
                [[0.0, 0.0, 0.0], [40.0, 0.0, 0.0], [0.0, 30.0, 0.0]]
            );
            assert_eq!(
                geometry.vertices.iter().map(|v| v.uv).collect::<Vec<_>>(),
                [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]
            );
        }
        assert_eq!(
            rendered[2]
                .1
                .mesh_vertices
                .iter()
                .map(|v| v.pos)
                .collect::<Vec<_>>(),
            rendered[3]
                .1
                .mesh_vertices
                .iter()
                .map(|v| v.pos)
                .collect::<Vec<_>>(),
        );
        for (index, (_, submitted)) in rendered.iter().enumerate() {
            // Fresh native control, authored FTOC then native final diffuse.
            let expected = if index % 2 == 0 {
                [
                    rgba([21, 67, 126, 21]),
                    rgba([0, 133, 72, 88]),
                    rgba([21, 67, 126, 21]),
                ]
            } else {
                [
                    rgba([24, 76, 168, 24]),
                    rgba([0, 153, 96, 101]),
                    rgba([24, 76, 168, 24]),
                ]
            };
            assert_eq!(
                colors(submitted),
                expected,
                "immediate={immediate}, mesh={index}"
            );
        }
    }
}

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/manual-color.lua").unwrap())
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"))
        .to_vec()
}

fn runtime(source: &str) -> (SongRuntime, ActorScene) {
    let mut context = deadsync_song_lua::SongLuaCompileContext::new(".", "native draw color scope");
    context.live = true;
    let mut runtime = SongRuntime::new(&context).unwrap();
    let definition = execute(&mut runtime, source)[0];
    let scene = runtime.instantiate_actor(definition).unwrap();
    let root = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"root", root).unwrap();
    assert!(runtime.start_actor_scene(scene).unwrap());
    runtime.begin_actor_playback();
    (runtime, scene)
}

fn take(runtime: &mut SongRuntime, names: &[&str]) -> Vec<SongLuaDrawOp> {
    let sources = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            (
                runtime.vm_mut().get_global(name.as_bytes()).unwrap(),
                SongLuaDrawSource::Overlay(index),
            )
        })
        .collect::<Vec<_>>();
    runtime
        .take_actor_draw_frames(&sources)
        .unwrap()
        .into_iter()
        .flat_map(|(_, frame)| frame.ops)
        .collect()
}

fn incoming(op: &SongLuaDrawOp) -> ([f32; 4], [f32; 4]) {
    let SongLuaDrawOp::Draw {
        inherited_diffuse,
        inherited_glow,
        ..
    } = op
    else {
        panic!("expected recorded leaf: {op:?}")
    };
    (*inherited_diffuse, *inherited_glow)
}

fn close_color(actual: [f32; 4], expected: [f32; 4]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
    }
}

#[test]
fn manual_draw_nested_callback_resets_color_after_its_opacity_gate() {
    let (mut runtime, scene) =
        runtime("return Def.ActorFrame{Def.ActorFrame{Def.Quad{},Def.ActorFrame{Def.Quad{}}}}");
    execute(
        &mut runtime,
        r#"
        group=root[1];leaf=group[1];nested=group[2];nested_leaf=nested[1]
        root:diffuse{.5,.25,.75,.25}:glow{.1,.2,.3,.4}
        group:diffuse{.25,.5,1,.5}:glow{.2,.1,.4,.2}
        nested:diffuse{.875,.5,.25,.75}:glow{.25,.125,.5,.25}
        group:SetDrawFunction(function() leaf:Draw();nested:Draw() end)
        root:Draw()
    "#,
    );
    let names = ["root", "group", "leaf", "nested", "nested_leaf"];
    let first = take(&mut runtime, &names);
    assert_eq!(first.len(), 2);
    assert_eq!(incoming(&first[0]), ([1.0; 4], [0.0; 4]));
    assert_eq!(
        incoming(&first[1]),
        ([0.875, 0.5, 0.25, 0.75], [0.25, 0.125, 0.5, 0.25])
    );

    execute(&mut runtime, "root:diffusealpha(0):glow{0,0,0,0}:Draw()");
    assert!(
        take(&mut runtime, &names).is_empty(),
        "native gate precedes the callback"
    );
    execute(&mut runtime, "root:glow{1,1,1,.5}:Draw()");
    let glowing_owner = take(&mut runtime, &names);
    assert_eq!(glowing_owner.len(), 2);
    assert_eq!(incoming(&glowing_owner[0]), ([1.0; 4], [0.0; 4]));
    assert_eq!(incoming(&glowing_owner[1]), incoming(&first[1]));

    execute(&mut runtime, "group:SetDrawFunction(nil);root:Draw()");
    let ordinary = take(&mut runtime, &names);
    assert_eq!(ordinary.len(), 2);
    assert_eq!(incoming(&ordinary[0]).0[3], 0.0);
    close_color(incoming(&ordinary[0]).1, [1.0, 1.0, 1.0, 0.6]);
    assert!(runtime.take_actor_diagnostics().is_empty());
    runtime.end_actor_scene(scene).unwrap();
    runtime.release_actor_scene(scene).unwrap();
}

#[test]
fn manual_draw_direct_wrappers_replace_normal_parent_colors() {
    let (mut runtime, scene) = runtime("return Def.ActorFrame{Def.Quad{}}");
    execute(
        &mut runtime,
        r#"
        leaf=root[1];root:diffuse{.1,.2,.3,.1}:glow{1,1,1,.9}
        inner=leaf:AddWrapperState():diffuse{.5,.25,.75,.5}:glow{.25,.125,0,.25}
        outer=leaf:AddWrapperState():diffuse{.875,.5,.25,.25}:glow{0,.25,.5,.5}
        outer:AddWrapperState():diffusealpha(0):visible(false)
        root:Draw()
    "#,
    );
    let names = ["root", "leaf"];
    let first = take(&mut runtime, &names);
    assert_eq!(
        first.len(),
        1,
        "native owner draw visits direct wrappers only"
    );
    assert_eq!(
        incoming(&first[0]),
        ([0.4375, 0.125, 0.1875, 0.125], [0.25, 0.34375, 0.5, 0.625])
    );
    let SongLuaDrawOp::Draw { parents, .. } = &first[0] else {
        unreachable!()
    };
    assert_eq!(
        parents.len(),
        3,
        "normal parent and two direct wrappers remain geometry ancestors"
    );
    execute(
        &mut runtime,
        "outer:diffusealpha(0):glow{0,0,0,0};root:Draw()",
    );
    assert!(
        take(&mut runtime, &names).is_empty(),
        "normal-parent glow cannot rescue a transparent wrapper"
    );

    // Every diffuse corner participates in the owner's gate, but only corner
    // zero becomes normal child inheritance. A wrapper replaces that zero.
    execute(
        &mut runtime,
        r#"
        leaf:RemoveWrapperState(2):RemoveWrapperState(1)
        root:glow{0,0,0,0}:diffuse{1,1,1,1}:diffuseupperleft{1,1,1,0}
        root:Draw()
    "#,
    );
    assert!(take(&mut runtime, &names).is_empty());
    execute(&mut runtime, "leaf:AddWrapperState();root:Draw()");
    let wrapped = take(&mut runtime, &names);
    assert_eq!(wrapped.len(), 1);
    // A fresh wrapper replaces the parent's colors with its native defaults,
    // including white glow RGB with zero alpha.
    assert_eq!(incoming(&wrapped[0]), ([1.0; 4], [1.0, 1.0, 1.0, 0.0]));
    assert!(runtime.take_actor_diagnostics().is_empty());
    runtime.end_actor_scene(scene).unwrap();
    runtime.release_actor_scene(scene).unwrap();
}

#[test]
fn manual_draw_color_scope_freezes_each_actor_effect_phase() {
    let (mut runtime, scene) = runtime("return Def.ActorFrame{Def.ActorFrame{Def.Quad{}}}");
    execute(
        &mut runtime,
        r#"
        group=root[1];leaf=group[1];root:SetUpdateRate(2)
        group:diffuseramp():effectperiod(1):effectcolor1(1,.5,.25,.5):effectcolor2(.5,.25,1,.75)
    "#,
    );
    let names = ["root", "group", "leaf"];
    runtime.set_values(100.0, 12.0).unwrap();
    assert!(runtime.advance_actor_scene(scene, 0.125).unwrap());
    execute(&mut runtime, "root:Draw()");
    let frozen = take(&mut runtime, &names);
    assert_eq!(frozen.len(), 1);
    let quarter = [0.625, 0.3125, 0.8125, 0.6875];
    close_color(incoming(&frozen[0]).0, quarter);

    execute(&mut runtime, "group:hibernate(.5)");
    runtime.set_values(200.0, 30.0).unwrap();
    assert!(runtime.advance_actor_scene(scene, 0.125).unwrap());
    execute(&mut runtime, "group:hibernate(0);root:Draw()");
    let sleeping = take(&mut runtime, &names);
    close_color(incoming(&sleeping[0]).0, quarter);
    runtime.set_values(20.0, 3.0).unwrap();
    assert!(runtime.advance_actor_scene(scene, 0.0).unwrap());
    execute(&mut runtime, "root:Draw()");
    let moved_clock = take(&mut runtime, &names);
    close_color(incoming(&moved_clock[0]).0, quarter);

    assert!(runtime.advance_actor_scene(scene, 0.125).unwrap());
    execute(
        &mut runtime,
        "root:Draw();group:stopeffect():diffuse{.1,.2,.3,.4};root:Draw()",
    );
    let changed = take(&mut runtime, &names);
    assert_eq!(changed.len(), 2);
    close_color(incoming(&changed[0]).0, [0.75, 0.375, 0.625, 0.625]);
    close_color(incoming(&changed[1]).0, [0.1, 0.2, 0.3, 0.4]);
    close_color(incoming(&frozen[0]).0, quarter);
    assert!(runtime.take_actor_diagnostics().is_empty());
    runtime.end_actor_scene(scene).unwrap();
    runtime.release_actor_scene(scene).unwrap();
}

#[test]
fn manual_aft_preserves_inherited_color_and_restores_the_outer_scope() {
    let (mut runtime, scene) =
        runtime("return Def.ActorFrame{Def.ActorFrameTexture{Def.Quad{}},Def.Quad{}}");
    execute(
        &mut runtime,
        r#"
        aft=root[1];captured=aft[1];sibling=root[2]
        root:diffuse{.875,.5,.25,.75}:glow{.25,.125,.5,.25}:xy(30,40)
        aft:SetSize(8,8):Create():diffuse{.5,.25,.75,.5}:glow{0,.25,.5,.5}
        root:Draw()
    "#,
    );
    let ops = take(&mut runtime, &["root", "aft", "captured", "sibling"]);
    assert_eq!(ops.len(), 4);
    assert!(matches!(ops[0], SongLuaDrawOp::Begin { capture: 1, .. }));
    assert!(matches!(ops[2], SongLuaDrawOp::Finish { capture: 1 }));
    assert_eq!(
        incoming(&ops[1]),
        ([0.4375, 0.125, 0.1875, 0.375], [0.25, 0.34375, 0.75, 0.625])
    );
    assert_eq!(
        incoming(&ops[3]),
        ([0.875, 0.5, 0.25, 0.75], [0.25, 0.125, 0.5, 0.25])
    );
    let SongLuaDrawOp::Draw {
        parents: captured_parents,
        ..
    } = &ops[1]
    else {
        unreachable!()
    };
    let SongLuaDrawOp::Draw {
        parents: sibling_parents,
        ..
    } = &ops[3]
    else {
        unreachable!()
    };
    assert!(captured_parents.is_empty());
    assert_eq!(sibling_parents.len(), 1);
    assert_eq!([sibling_parents[0].x, sibling_parents[0].y], [30.0, 40.0]);
    assert!(runtime.take_actor_diagnostics().is_empty());
    runtime.end_actor_scene(scene).unwrap();
    runtime.release_actor_scene(scene).unwrap();
}

fn external_source_frame(program: SongLuaDrawFrame) -> RenderFrame {
    let metrics = deadlib_present::space::Metrics::centered(640.0, 480.0);
    deadlib_present::space::with_current_metrics(metrics, || {
        let overlays: Vec<SongLuaOverlayActor> = vec![SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorFrame,
            name: Some("external draw owner".into()),
            parent_index: None,
            initial_state: Default::default(),
            message_commands: Vec::new(),
        }];
        let child = |origin: [f32; 2]| -> Arc<[Actor]> {
            Arc::from([Actor::Frame {
                align: [0.0; 2],
                offset: [origin[0] + 1.0, origin[1] + 2.0],
                size: [SizeSpec::Px(4.0), SizeSpec::Px(2.0)],
                children: Vec::new(),
                background: Some(deadlib_present::actors::Background::Color([1.0; 4])),
                z: 0,
            }])
        };
        let player = SongLuaManualPlayerSource {
            field: None,
            combo: None,
            judgment: Some([child([320.0, 240.0])]),
            origin: [320.0, 240.0],
            camera: NotefieldCameraCache::default(),
        };
        let underlay = [child([0.0; 2])];
        let sources = SongLuaScreenProxySources {
            manual_players: [Some(&player), None],
            underlay: Some(&underlay),
            ..Default::default()
        };
        let topology = SongLuaOverlayTopologyIndex::new(&overlays);
        let mut scratch = SongLuaManualScratch::with_frames(&overlays, Arc::from([program]));
        let mut actors = Vec::new();
        song_lua_draw_owner(
            &mut actors,
            &mut Vec::new(),
            &overlays,
            &[Default::default()],
            &topology,
            &mut scratch,
            0,
            0,
            &sources,
            &AssetManager::new(),
            [640.0, 480.0],
            [0.0; 2],
            0.0,
            0,
        );
        deadlib_present::compose::build_passes(
            std::iter::once(ActorSegment::new(&actors)),
            &[],
            [0.0; 4],
            &metrics,
            &font::FontMap::default(),
            0.0,
            &mut TextLayoutCache::default(),
            &mut ComposeScratch::default(),
            &CaptureTextureContext,
            None,
        )
    })
}

#[test]
fn manual_external_sources_use_recorded_glow_for_entry_only() {
    crate::tests::init_paths();
    for live in [false, true] {
        for effect in [false, true] {
            let mut context =
                deadsync_song_lua::SongLuaCompileContext::new(".", "external draw color gates");
            context.live = live;
            context.players[0].enabled = true;
            let mut runtime = SongRuntime::new(&context).unwrap();
            let screen_scene =
                deadsync_song_lua::test_support::gameplay_screen(&mut runtime, &context).unwrap();
            let definition = execute(&mut runtime, "return Def.ActorFrame{}")[0];
            let scene = runtime.instantiate_actor(definition).unwrap();
            let root = runtime.actor_scene_root(scene).unwrap();
            runtime.vm_mut().set_global(b"root", root).unwrap();
            assert!(runtime.start_actor_scene(scene).unwrap());
            runtime.begin_actor_playback();
            execute(
                &mut runtime,
                r#"
                player=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
                layer=SCREENMAN:GetTopScreen():GetChild('Underlay')
                root:xy(17,19)
                player:xy(100,200);layer:xy(20,30)
                for _,actor in ipairs{player,layer} do
                    actor:visible(true):diffuse{.2,.3,.4,0}:glow{0,0,0,0}
                    -- Preserve the outer fractional glow through native Screen
                    -- mixing; the default local glow RGB is white, not black.
                    actor:AddWrapperState():xy(11,13):diffusealpha(0):glow{0,0,0,0}
                    actor:AddWrapperState():glow{.4375,.25,.125,.5}
                end
                root:SetDrawFunction(function() player:Draw();layer:Draw() end)
            "#,
            );
            if effect {
                execute(
                    &mut runtime,
                    r#"
                    for _,actor in ipairs{player,layer} do
                        actor:GetWrapperState(2):glow{0,0,0,0}:glowshift():effectperiod(1)
                            :effectcolor1(.4375,.25,.125,.5):effectcolor2(.4375,.25,.125,.5)
                    end
                "#,
                );
            }
            execute(&mut runtime, "root:Draw()");
            let sources = [
                (root, SongLuaDrawSource::Overlay(0)),
                (
                    runtime.vm_mut().get_global(b"player").unwrap(),
                    SongLuaDrawSource::Player(0),
                ),
                (
                    runtime.vm_mut().get_global(b"layer").unwrap(),
                    SongLuaDrawSource::ScreenLayer(0),
                ),
            ];
            let frames = runtime.take_actor_draw_frames(&sources).unwrap();
            assert_eq!(frames.len(), 1);
            let program = frames.into_iter().next().unwrap().1;
            assert_eq!(program.ops.len(), 2, "live={live}, effect={effect}");
            for op in &program.ops {
                let SongLuaDrawOp::Draw { state, parents, .. } = op else {
                    unreachable!()
                };
                assert_eq!(state.diffuse[3], 0.0);
                assert_eq!(state.glow[3], 0.0);
                assert_eq!(parents.last().unwrap().diffuse[3], 0.0);
                assert_eq!(parents.last().unwrap().glow[3], 0.0);
                assert_eq!(incoming(op).0[3], 0.0);
                close_color(incoming(op).1, [0.4375, 0.25, 0.125, 0.5]);
                if effect {
                    assert_eq!(
                        parents[1].glow[3], 0.0,
                        "raw glow is not the evaluated glow"
                    );
                    assert_eq!(
                        parents[1].effect_mode,
                        deadlib_present::anim::EffectMode::GlowShift
                    );
                }
            }
            let mut opaque = program.clone();
            for op in &mut opaque.ops {
                let SongLuaDrawOp::Draw {
                    state,
                    parents,
                    inherited_diffuse,
                    inherited_glow,
                    ..
                } = op
                else {
                    unreachable!()
                };
                state.diffuse = [1.0; 4];
                *inherited_diffuse = [1.0; 4];
                *inherited_glow = [0.0; 4];
                for parent in Arc::make_mut(parents) {
                    parent.diffuse = [1.0; 4];
                    parent.glow = [0.0; 4];
                    parent.effect_mode = deadlib_present::anim::EffectMode::None;
                }
            }
            let expected = external_source_frame(opaque);
            let actual = external_source_frame(program);
            assert_eq!(
                actual.sprite_instances.len(),
                2,
                "live={live}, effect={effect}"
            );
            assert_eq!(
                compare_render_frames_semantic(&expected, &actual),
                Ok(()),
                "entry colors must not restyle shared source children or change their basis"
            );
            let mut pixels = Vec::new();
            for op in &actual.ops {
                let DrawOp::Sprite(run) = op else {
                    panic!("expected unchanged source sprite")
                };
                for instance in &actual.sprite_instances[run.instance_start as usize
                    ..(run.instance_start + run.instance_count) as usize]
                {
                    assert_eq!(instance.tint, [1.0; 4]);
                    assert_eq!(instance.size, [4.0, 2.0]);
                    let clip = actual.cameras[run.camera as usize]
                        * Vector4::new(
                            instance.center[0],
                            instance.center[1],
                            instance.center[2],
                            1.0,
                        );
                    pixels.push([
                        (clip.x / clip.w + 1.0) * 320.0,
                        (1.0 - clip.y / clip.w) * 240.0,
                    ]);
                }
            }
            assert_eq!(pixels.len(), 2);
            for (pixel, expected_pixel) in pixels.into_iter().zip([[131.0, 235.0], [51.0, 65.0]]) {
                for (actual, expected) in pixel.into_iter().zip(expected_pixel) {
                    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
                }
            }
            execute(
                &mut runtime,
                r#"
                for _,actor in ipairs{player,layer} do
                    actor:GetWrapperState(2):stopeffect():glow{0,0,0,0}
                end
                root:Draw()
            "#,
            );
            for (_, frame) in runtime.take_actor_draw_frames(&sources).unwrap() {
                assert!(
                    external_source_frame(frame).ops.is_empty(),
                    "transparent sources without glow remain gated in preparation and live replay"
                );
            }
            assert!(runtime.take_actor_diagnostics().is_empty());
            runtime.end_actor_scene(scene).unwrap();
            runtime.release_actor_scene(scene).unwrap();
            runtime.end_actor_scene(screen_scene).unwrap();
            runtime.release_actor_scene(screen_scene).unwrap();
        }
    }
}
