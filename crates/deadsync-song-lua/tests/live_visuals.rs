use deadlib_lua::compile_named;
use deadlib_present::actors::TextAttribute;
use deadsync_song_lua::{self as song, SongLuaLiveTarget as Target};
use std::{path::Path, sync::Arc};

type Kind = song::SongLuaOverlayKind<(), (), TextAttribute>;
type Actor = song::SongLuaOverlayActor<Kind>;
type Visuals<A = Actor> = deadsync_gameplay::SongLuaRuntimeVisuals<
    A,
    song::SongLuaCapturedActor,
    song::gameplay::SongLuaRuntimeOverlayStateDelta,
>;
fn slots(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from([]))
}
fn layer(_: &()) -> Option<song::SongLuaOverlayModelLayer<()>> {
    None
}

const SOURCE: &str = r#"
return Def.ActorFrame {
    InitCommand=function(self) ROOT=self end,
    OnCommand=function(self) self:sleep(1000) end,
    Def.BitmapText {Name='Text',Font='Common Normal',Text='before',
        InitCommand=function(self) TXT=self;self:AddAttribute(0,{Length=2,Diffuse={1,.5,.25,1}}) end},
    Def.Sprite {Name='Sprite',Texture='art.png',Frames={{Frame=0,Delay=.125},{Frame=1,Delay=.25}},
        InitCommand=function(self) IMG=self end},
    Def.ActorMultiVertex {Name='Mesh',InitCommand=function(self)
        MESH=self;self:SetDrawState{Mode='DrawMode_Triangles'}
        self:SetVertices{{{0,0},{1,0,0,1}},{{10,0},{0,1,0,1}},{{0,10},{0,0,1,1}}}
        self:LoadTexture('art.png')
    end},
    Def.Quad {Name='Target',InitCommand=function(self) QUAD=self end},
    Def.ActorProxy {Name='Proxy',InitCommand=function(self) PRX=self;self:SetTarget(QUAD) end},
}
"#;

fn visuals<A>(out: song::CompiledSongLua<A>) -> Visuals<A> {
    Visuals {
        overlays: out.overlays,
        foreground_beats: out.foreground_beats,
        screen_overlay_index: out.screen_overlay_index,
        overlay_eases: Vec::new(),
        overlay_ease_ranges: Vec::new(),
        overlay_events: Vec::new(),
        background_visual_layers: Vec::new(),
        foreground_visual_layers: Vec::new(),
        player_actors: out.player_actors,
        player_events: Default::default(),
        player_judgment_events: Default::default(),
        player_combo_events: Default::default(),
        song_foreground: out.song_foreground,
        song_foreground_events: Vec::new(),
        hidden_players: out.hidden_players,
        screen_layers: out.screen_layers,
        screen_layer_events: Default::default(),
        note_hides: Default::default(),
        column_offsets: Default::default(),
        column_splines: Default::default(),
        screen_width: out.screen_width,
        screen_height: out.screen_height,
    }
}

#[test]
fn column_splines_match_between_baked_and_runtime_and_clear_on_reset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.lua");
    std::fs::write(&path, r#"
        local function update()
            local column=SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):GetColumnActors()[1]
            local beat=GAMESTATE:GetSongBeat()
            for index,handler in ipairs{column:GetPosHandler(),column:GetRotHandler(),column:GetZoomHandler()} do
                if beat>=2 then
                    handler:SetSplineMode('NoteColumnSplineMode_Disabled')
                else
                    handler:SetSplineMode(index==3 and 'NoteColumnSplineMode_Position' or 'NoteColumnSplineMode_Offset')
                    handler:SetBeatsPerT(.5):SetReceptorT(.25)
                    local spline=handler:GetSpline()
                    spline:SetSize(3):SetPoint(1,{1,2,3}):SetPoint(2,{3,4,5}):SetPoint(3,{7,8,9}):Solve()
                    -- SetPoint changes the constant terms without refitting the solved slopes.
                    if beat>=1 then spline:SetPoint(1,{11,12,13}) end
                end
            end
        end
        return Def.ActorFrame{InitCommand=function(self) self:SetUpdateFunction(update) end,
            OnCommand=function(self) self:sleep(1000) end}
    "#).unwrap();
    let mut context = song::SongLuaCompileContext::new(dir.path(), "column spline publication");
    context.players[1].enabled = false;
    context.music_length_seconds = 3.0;
    context.compile_clock_inputs = vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [2.0, 2.0, 1.0]];
    let bake = song::compile_song_lua_layers(
        &[&path],
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    )
    .unwrap()
    .remove(0);
    let track = &bake.column_splines[0];
    assert_eq!(track.player, 0);
    assert_eq!(track.column, 0);

    context.live = true;
    let mut compiled = song::compile_song_lua_layers(
        &[&path],
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    )
    .unwrap()
    .remove(0);
    let mut scene = compiled.live_scene.take().unwrap();
    let mut out = visuals(compiled);
    let mut position_at_start = None;
    for beat in [0.0, 1.0, 2.0] {
        let frame = song::SongLuaLiveFrame {
            beat,
            music_seconds: beat,
            screen_seconds: beat,
            delta_seconds: if beat == 0.0 { 0.0 } else { 1.0 },
            ..Default::default()
        };
        assert!(scene.update(frame).unwrap().errors.is_empty());
        scene.write_visuals(&mut out).unwrap();
        let baked = track.at_second(beat as f32).unwrap();
        if beat < 2.0 {
            let live = out.column_splines[0][0].at_second(-1.0).unwrap();
            assert_eq!(baked.position, live.position);
            assert_eq!(baked.rotation, live.rotation);
            assert_eq!(baked.zoom, live.zoom);
            let position = live.position.as_ref().unwrap();
            assert!(!position.absolute);
            assert!(!live.rotation.as_ref().unwrap().absolute);
            assert!(live.zoom.as_ref().unwrap().absolute);
            assert_eq!(position.coefficients.len(), 3);
            if let Some(initial) = position_at_start.take() {
                let initial: [[[f32; 4]; 3]; 3] = initial;
                assert_eq!(position.coefficients[0][0][0], 11.0);
                assert_eq!(&position.coefficients[0][0][1..], &initial[0][0][1..]);
                assert_ne!(position.coefficients[0][0][1], 0.0);
            } else {
                position_at_start = Some(position.coefficients.as_ref().try_into().unwrap());
            }
        } else {
            assert!(out.column_splines.iter().all(Vec::is_empty));
            assert!(baked.position.is_none() && baked.rotation.is_none() && baked.zoom.is_none());
        }
    }
}

#[test]
#[ignore = "requires the retained Peach Summer Splash native fixture"]
fn peach_sandbox_column_splines_follow_native_wrap_and_reset() {
    let fixture = std::env::var_os("DEADSYNC_PEACH_FIXTURE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/itgmania-song-lua-micro/peach-summer-splash")
        });
    let read = |file| std::fs::read(fixture.join(file)).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&read("manifest.json")).unwrap();
    let native: serde_json::Value = serde_json::from_slice(
        &zstd::stream::decode_all(read("sandbox-offset-native.json.zst").as_slice()).unwrap(),
    )
    .unwrap();
    let coverage = &manifest["fixtures"]["sandbox_offset"]["coverage"];
    let dir = fixture.join("inputs/sandbox");
    let path = dir.join("lua/default.lua");
    let updates = native["update_frames"].as_array().unwrap();
    let clocks: Vec<[f64; 3]> = updates
        .iter()
        .map(|f| ["seconds", "song_beat", "delta"].map(|key| f[key].as_f64().unwrap()))
        .collect();
    let option_deltas = |index: usize| {
        let options = &updates[index]["option_deltas"];
        [&options["song"], &options["players"][0], &options["players"][1]]
            .map(|value| value.as_f64().unwrap() as f32)
    };
    let wrap = coverage["offset_wrap"]["samples"].as_array().unwrap();
    let reset = coverage["first_tap_after_reset"]["samples"]
        .as_array()
        .unwrap();
    let hold = coverage["hold_crossing_bend"]["body_samples"]
        .as_array()
        .unwrap();
    let samples = [wrap, reset, hold].map(|rows| {
        std::array::from_fn::<_, 2, _>(|i| {
            rows[i]["update_sequence"].as_u64().unwrap() as usize - 1
        })
    });
    let mut context = song::SongLuaCompileContext::new(dir, "Peach Summer Splash sandbox2");
    context.players[1].enabled = false;
    context.players[0].speedmod = song::SongLuaSpeedMod::X(2.0);
    context.players[0].noteskin_name = "cel".into();
    context.screen_width = 854.0;
    context.display_width = 854.0;
    context.song_timing_bpms = vec![(0.0, 125.0)];
    context.song_display_bpms = [125.0; 2];
    context.music_length_seconds = clocks.last().unwrap()[0] as f32;
    // The foreground is activated at beat zero. The capture's earlier
    // ScreenGameplay updates belong to the audio pre-roll.
    context.compile_clock_inputs.push([0.0; 3]);
    context.compile_screen_actor_deltas.push(0.0);
    context.compile_option_clock_deltas.push([0.0; 3]);
    for (index, &clock) in clocks.iter().enumerate().filter(|(_, frame)| frame[1] > 0.0) {
        context.compile_clock_inputs.push(clock);
        context.compile_screen_actor_deltas.push(clock[2] as f32);
        context.compile_option_clock_deltas.push(option_deltas(index));
    }
    let bake = song::compile_song_lua_layers(
        &[&path],
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    )
    .unwrap()
    .remove(0);
    context.live = true;
    let mut live = song::compile_song_lua_layers(
        &[&path],
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    )
    .unwrap()
    .remove(0);
    let mut scene = live.live_scene.take().unwrap();
    let mut out = visuals(live);
    let mut tap_y = [[0.0; 2]; 2];
    for (index, &[seconds, beat, delta]) in clocks.iter().enumerate() {
        if beat <= 0.0 {
            continue;
        }
        scene.set_option_clock_deltas(option_deltas(index));
        let frame = song::SongLuaLiveFrame {
            beat,
            music_seconds: seconds,
            delta_seconds: delta,
            screen_seconds: seconds + 2.0,
            ..Default::default()
        };
        assert!(
            scene.update(frame).unwrap().errors.is_empty(),
            "frame {index}"
        );
        for (probe, frames) in samples.iter().enumerate() {
            let Some(side) = frames.iter().position(|&i| i == index) else {
                continue;
            };
            scene.write_visuals(&mut out).unwrap();
            let column = usize::from(probe == 2);
            let baked = bake
                .column_splines
                .iter()
                .find(|t| t.column == column)
                .unwrap()
                .at_second(seconds as f32)
                .unwrap();
            let live = out.column_splines[0]
                .iter()
                .find(|t| t.column == column)
                .unwrap()
                .at_second(seconds as f32)
                .unwrap();
            assert_eq!(baked.position, live.position, "position at frame {index}");
            assert_eq!(baked.zoom, live.zoom, "zoom at frame {index}");
            let position = live.position.as_ref().unwrap();
            assert!(!position.absolute);
            assert_eq!(position.coefficients.len(), 75);
            let beat = beat as f32;
            match probe {
                0 | 1 => {
                    let note = if probe == 0 { 34.0 } else { 36.0 };
                    let [_, y, _] = position.view().sample(beat, note).0;
                    tap_y[probe][side] = (note - beat) * 128.0 + y;
                }
                _ => {
                    let tangent = position.view().sample(beat, 32.0).1;
                    assert!(
                        tangent[2].abs() > 0.0,
                        "the hold carries the spline tangent"
                    );
                }
            }
        }
    }
    for (ys, native) in tap_y.into_iter().zip([wrap, reset]) {
        let y = |i: usize| native[i]["first_anchor_y_ndc"].as_f64().unwrap() as f32;
        let native_travel = (y(0) - y(1)) * 240.0;
        assert!(
            (ys[1] - ys[0] - native_travel).abs() < 0.25,
            "{ys:?}, native {native_travel}"
        );
    }
}

struct Pair {
    rust: Option<song::SongLiveScene>,
    r: Visuals,
    bindings: Vec<(usize, Target)>,
    dir: tempfile::TempDir,
}
impl Drop for Pair {
    fn drop(&mut self) {
        if let Some(s) = &mut self.rust {
            let _ = s.end();
        }
    }
}
impl Pair {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("default.lua");
        // Exact files precede their directory so its prefix cannot consume
        // their portable snapshot tokens.
        reference_test::register_path(&path);
        reference_test::register_path(&dir.path().join("art.png"));
        reference_test::register_path(&dir.path().join("second.png"));
        reference_test::register_path(dir.path());
        std::fs::write(&path, SOURCE).unwrap();
        for name in ["art.png", "second.png"] {
            image::RgbaImage::new(2, 2)
                .save(dir.path().join(name))
                .unwrap();
        }
        let mut context = song::SongLuaCompileContext::new(dir.path(), "live visual publication");
        context.live = true;
        context.music_length_seconds = 10.;
        context.players[1].enabled = false;

        let mut r = song::compile_song_lua_layers(
            &[&path],
            0,
            &context,
            Default::default(),
            slots,
            layer,
            |_, _| None,
        )
        .unwrap()
        .remove(0);
        reference_test::assert_debug("song/tests/rust_live_visuals.rs::compiled", &r);
        let bindings: Vec<_> = r
            .live_actor_indices()
            .iter()
            .copied()
            .enumerate()
            .map(|(local, global)| (global, Target::Root(local)))
            .collect();

        let mut rust = r.live_scene.take().unwrap();

        rust.bind_targets(bindings.clone()).unwrap();
        let pair = Self {
            rust: Some(rust),
            r: visuals(r),
            bindings,
            dir,
        };
        pair.compare("prepared");
        pair
    }
    fn script(&mut self, source: &str) {
        let program = compile_named(source.as_bytes(), b"@/live-visual-check.lua").unwrap();
        let result = {
            let mut runtime = self.rust.as_mut().unwrap().runtime_mut();
            runtime.vm_mut().execute(&program).map(|_| ())
        };
        result.unwrap();
    }
    fn compare(&self, phase: &str) {
        reference_test::assert_debug(
            "song/tests/rust_live_visuals.rs::visuals",
            &(phase, &self.r),
        );
    }
    fn publish(&mut self, success: bool) {
        let r = self.rust.as_mut().unwrap().write_visuals(&mut self.r);

        assert_eq!(r.is_ok(), success, "rust: {r:?}");
        self.compare("publication including partial failure");
    }
    fn update(&mut self) {
        let frame = song::SongLuaLiveFrame {
            music_seconds: 0.125,
            screen_seconds: 0.125,
            beat: 0.25,
            delta_seconds: 0.125,
            ..Default::default()
        };

        let r = self.rust.as_mut().unwrap().update(frame).unwrap();

        assert!(r.errors.is_empty(), "{:?}", r.errors);
        reference_test::assert_debug(
            "song/tests/rust_live_visuals.rs::states",
            &("update", &self.rust.as_ref().unwrap().states()),
        );
    }
    fn index(&self, name: &str) -> usize {
        self.r
            .overlays
            .iter()
            .position(|a| a.name.as_deref() == Some(name))
            .unwrap()
    }
    fn bind(&mut self, bindings: Vec<(usize, Target)>) {
        self.rust.as_mut().unwrap().bind_targets(bindings).unwrap();
        reference_test::assert_debug(
            "song/tests/rust_live_visuals.rs::draw_bindings",
            &(
                "bindings",
                &self.rust.as_ref().unwrap().visual_draw_bindings(),
            ),
        );
        reference_test::assert_debug(
            "song/tests/rust_live_visuals.rs::bindings",
            &(
                "bindings",
                &self.rust.as_ref().unwrap().visual_bindings().unwrap(),
            ),
        );
    }
}
fn text(v: &Visuals, index: usize) -> (&Arc<str>, &Arc<[TextAttribute]>) {
    let Kind::BitmapText {
        text, attributes, ..
    } = &v.overlays[index].kind
    else {
        panic!("text")
    };
    (text, attributes)
}
fn states(v: &Visuals, index: usize) -> &Arc<[song::SongLuaSpriteState]> {
    let Kind::Sprite { states, .. } = &v.overlays[index].kind else {
        panic!("sprite")
    };
    states
}

#[test]
fn live_poses_content_and_tracked_visibility_reach_prepared_slots() {
    let mut p = Pair::new();
    p.publish(true);
    let index = p.index("Target");
    let old = p.r.overlays[index].initial_state;
    p.script("TXT:settext('updated');IMG.Frames[1].Delay=.75;QUAD:x(37);SCREENMAN:GetTopScreen():GetChild('PlayerP1'):visible(false)");
    p.publish(true);
    assert_eq!(
        format!("{:?}", p.r.overlays[index].initial_state),
        format!("{old:?}"),
        "poses remain frozen until update"
    );
    assert_eq!(text(&p.r, p.index("Text")).0.as_ref(), "updated");
    p.update();
    p.publish(true);
    assert_eq!(p.r.overlays[index].initial_state.x, 37.);
    assert!(p.r.hidden_players[0]);
    p.bind(p.bindings.clone());
}

#[test]
fn unchanged_payload_arcs_are_retained_and_in_place_edits_replace_only_changed_values() {
    let mut p = Pair::new();
    p.publish(true);
    let t = p.index("Text");
    let s = p.index("Sprite");
    let (old_text, old_attributes) = text(&p.r, t);
    let old_text = old_text.clone();
    let old_attributes = old_attributes.clone();
    let old_states = states(&p.r, s).clone();
    p.publish(true);
    assert!(Arc::ptr_eq(&old_text, text(&p.r, t).0));
    assert!(Arc::ptr_eq(&old_attributes, text(&p.r, t).1));
    assert!(Arc::ptr_eq(&old_states, states(&p.r, s)));
    p.script("TXT.__songlua_text_attributes[1].color[1]=-0;IMG.Frames[1].Delay=-0");
    p.publish(true);
    assert!(!Arc::ptr_eq(&old_attributes, text(&p.r, t).1));
    assert!(!Arc::ptr_eq(&old_states, states(&p.r, s)));
    assert_eq!(old_attributes[0].color[0], 1.);
    assert_eq!(old_states[0].delay, 0.125);
    p.script("TXT.__songlua_text_attributes[1].color[1]=0/0;IMG.Frames[1].Delay=0/0");
    p.publish(true);
    let attributes = text(&p.r, t).1.clone();
    let frames = states(&p.r, s).clone();
    p.publish(true);
    assert!(Arc::ptr_eq(&attributes, text(&p.r, t).1));
    assert!(Arc::ptr_eq(&frames, states(&p.r, s)));
    p.script("TXT.__songlua_text_attributes[2]={start=4,length=0};IMG.Frames[2]=nil");
    p.publish(true);
    assert!(Arc::ptr_eq(&attributes, text(&p.r, t).1));
    assert_eq!(states(&p.r, s).len(), 1);
    p.script("TXT.__songlua_text_attributes={};IMG.Frames={}");
    p.publish(true);
    assert!(text(&p.r, t).1.is_empty() && states(&p.r, s).is_empty());
}

#[test]
fn text_and_attribute_publication_preserve_the_native_error_commit_boundaries() {
    let mut p = Pair::new();
    p.publish(true);
    let t = p.index("Text");
    let attributes = text(&p.r, t).1.clone();
    p.script("TXT.Text='committed';TXT.__songlua_text_attributes[2]=false;TXT.__songlua_stroke_color={1,0,0,1}");
    p.publish(false);
    assert_eq!(text(&p.r, t).0.as_ref(), "committed");
    assert!(Arc::ptr_eq(&attributes, text(&p.r, t).1));
    p.script("TXT.__songlua_text_attributes[2]=nil;TXT.__songlua_text_attributes[1].length=7;TXT.__songlua_stroke_color=false");
    p.publish(false);
    assert_eq!(text(&p.r, t).1[0].length, 7);
    p.script("TXT.__songlua_stroke_color=nil;TXT.Text=string.char(255)");
    p.publish(false);
    assert_eq!(text(&p.r, t).0.as_ref(), "committed");
    p.script("TXT.Text=true");
    p.publish(true);
    assert_eq!(text(&p.r, t).0.as_ref(), "true");
}

#[test]
fn sprite_paths_commit_before_frame_errors_and_unresolved_paths_keep_the_previous_texture() {
    let mut p = Pair::new();
    p.publish(true);
    let s = p.index("Sprite");
    let original = states(&p.r, s).clone();
    p.script("IMG.Texture='second.png';IMG.Frames[2].Frame=false");
    p.publish(false);
    let Kind::Sprite { texture_path, .. } = &p.r.overlays[s].kind else {
        panic!("sprite")
    };
    assert!(texture_path.ends_with("second.png"));
    assert!(Arc::ptr_eq(&original, states(&p.r, s)));
    p.script("IMG.Frames={{Frame=2,Delay=.5}};IMG.Texture='missing.png'");
    p.publish(true);
    let Kind::Sprite { texture_path, .. } = &p.r.overlays[s].kind else {
        panic!("sprite")
    };
    assert!(texture_path.ends_with("second.png"));
    assert_eq!(states(&p.r, s)[0].frame, 2);
    p.script("IMG.Frames=nil;IMG.Frame0000=3;IMG.Delay0000=.25;IMG.Frame0001=7;IMG.Delay0001=.5");
    p.publish(true);
    assert_eq!(
        states(&p.r, s).iter().map(|s| s.frame).collect::<Vec<_>>(),
        [3, 7]
    );
}

#[test]
fn mesh_vertices_publish_before_texture_errors_and_empty_meshes_still_read_the_texture() {
    let mut p = Pair::new();
    p.publish(true);
    let m = p.index("Mesh");
    p.script("MESH:SetVertices{{{0,0}},{{50,0}},{{0,50}}};MESH.Texture=false");
    p.publish(false);
    let Kind::ActorMultiVertex { vertices, .. } = &p.r.overlays[m].kind else {
        panic!("mesh")
    };
    assert_eq!(vertices.len(), 3);
    p.script("MESH:SetVertices{}");
    p.publish(false);
    let Kind::ActorMultiVertex { vertices, .. } = &p.r.overlays[m].kind else {
        panic!("mesh")
    };
    assert!(vertices.is_empty());
    p.script("MESH.Texture='second.png'");
    p.publish(true);
}

fn add_layers(v: &mut Visuals) {
    let layer = deadsync_gameplay::SongLuaVisualLayerRuntime {
        start_second: 0.,
        foreground_beats: None,
        screen_width: v.screen_width,
        screen_height: v.screen_height,
        overlays: v.overlays.clone(),
        overlay_eases: Vec::new(),
        overlay_ease_ranges: Vec::new(),
        overlay_events: Vec::new(),
        song_foreground: v.song_foreground.clone(),
        song_foreground_events: Vec::new(),
    };
    v.background_visual_layers = vec![layer.clone(), layer.clone()];
    v.foreground_visual_layers = vec![layer.clone(), layer];
}
#[test]
fn proxy_actor_identity_resolves_within_each_layer_and_duplicate_bindings_use_the_last_target() {
    let mut p = Pair::new();

    add_layers(&mut p.r);
    let proxy = p.index("Proxy");
    let target = p.index("Target");
    let global = |local: usize| p.bindings[local].0;
    let proxy_global = global(proxy);
    let target_global = global(target);
    for (owner, other, expected) in [
        (Target::Root(proxy), Target::Root(target), target),
        (
            Target::Background(0, proxy),
            Target::Background(0, target),
            target,
        ),
        (
            Target::Foreground(1, proxy),
            Target::Foreground(1, target),
            target,
        ),
        (
            Target::Background(0, proxy),
            Target::Foreground(0, target),
            usize::MAX,
        ),
        (
            Target::Foreground(1, proxy),
            Target::Foreground(0, target),
            usize::MAX,
        ),
    ] {
        p.bind(vec![(proxy_global, owner), (target_global, other)]);
        p.publish(true);
        let actor = match owner {
            Target::Root(i) => &p.r.overlays[i],
            Target::Background(l, i) => &p.r.background_visual_layers[l].overlays[i],
            Target::Foreground(l, i) => &p.r.foreground_visual_layers[l].overlays[i],
        };
        assert!(
            matches!(actor.kind,Kind::ActorProxy { target:song::SongLuaProxyTarget::Actor { overlay_index }} if overlay_index==expected)
        );
    }
    p.bind(vec![
        (proxy_global, Target::Root(proxy)),
        (target_global, Target::Root(target)),
        (target_global, Target::Background(1, target)),
    ]);
    p.publish(true);
    assert!(matches!(
        p.r.overlays[proxy].kind,
        Kind::ActorProxy {
            target: song::SongLuaProxyTarget::Actor {
                overlay_index: usize::MAX
            }
        }
    ));
    p.bind(p.bindings.clone());
    for script in [
        "PRX:SetTarget(SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'))",
        "PRX:SetTarget(SCREENMAN:GetTopScreen():GetChild('Underlay'):visible(false))",
        "PRX.__songlua_proxy_target_kind=nil",
    ] {
        p.script(script);
        p.publish(true);
    }
}

#[test]
fn invalid_global_bindings_leave_previous_bindings_and_missing_slots_stop_after_prior_writes() {
    let mut p = Pair::new();
    p.publish(true);
    let old = format!("{:?}", p.rust.as_ref().unwrap().visual_draw_bindings());

    assert!(
        p.rust
            .as_mut()
            .unwrap()
            .bind_targets(vec![(usize::MAX, Target::Root(0))])
            .is_err()
    );
    assert_eq!(
        format!("{:?}", p.rust.as_ref().unwrap().visual_draw_bindings()),
        old
    );
    let t = p.index("Text");
    let s = p.index("Sprite");
    p.bind(vec![
        (p.bindings[t].0, Target::Root(t)),
        (p.bindings[s].0, Target::Root(usize::MAX)),
    ]);
    p.script("TXT.Text='prior slot committed';SCREENMAN:GetTopScreen():GetChild('PlayerP1'):visible(false)");
    p.update();
    p.publish(false);
    assert_eq!(text(&p.r, t).0.as_ref(), "prior slot committed");
    assert!(!p.r.hidden_players[0]);
    p.bind(p.bindings.clone());
    p.publish(true);
    assert!(p.r.hidden_players[0]);
}

#[test]
fn collecting_getters_are_read_once_in_native_order_without_font_or_stroke_fallback_reads() {
    let mut p = Pair::new();
    p.publish(true);
    let q = p.index("Target");
    let old = p.r.overlays[q].initial_state;
    p.script(r#"
        LOG={};TXT.Text=nil;TXT.Font=nil;TXT.__songlua_text_attributes=nil;TXT.__songlua_stroke_color=nil
        local original=getmetatable(TXT);local mt={};for k,v in pairs(original)do mt[k]=v end
        local index=original.__index
        mt.__index=function(t,k)
            if k=='Font' or k=='StrokeColor' then error('publication read a preparation field') end
            if k=='Text' then collectgarbage('collect');LOG[#LOG+1]='text';QUAD:x(123);return 'getter' end
            if k=='__songlua_text_attributes' then collectgarbage('collect');LOG[#LOG+1]='attrs';return {{start=0,length=1,color={.2,.4,.6,1}}} end
            if k=='__songlua_stroke_color' then collectgarbage('collect');LOG[#LOG+1]='stroke';return nil end
            if type(index)=='function' then return index(t,k)end;return index[k]
        end
        setmetatable(TXT,mt)
        IMG.Frames={setmetatable({}, {__index=function(_,k)collectgarbage('collect');LOG[#LOG+1]=k;if k=='Frame'then return 4 elseif k=='Delay'then return .5 end end})}
    "#);
    p.publish(true);
    p.script("assert(table.concat(LOG,',')=='text,attrs,stroke,Frame,Delay')");
    assert_eq!(
        format!("{:?}", p.r.overlays[q].initial_state),
        format!("{old:?}")
    );
}

#[test]
fn immutable_visual_snapshots_survive_gc_and_session_teardown() {
    let mut p = Pair::new();
    p.publish(true);
    let t = p.index("Text");
    let s = p.index("Sprite");
    let old = p.r.clone();

    p.script("TXT:settext('new');TXT:ClearAttributes();IMG.Frames={{Frame=7,Delay=1}};collectgarbage('collect')");
    p.publish(true);
    reference_test::assert_debug("song/tests/rust_live_visuals.rs::retained", &old);
    assert_eq!(text(&old, t).0.as_ref(), "before");
    assert_eq!(states(&old, s)[0].frame, 0);

    p.rust.as_mut().unwrap().end().unwrap();

    p.rust.take();
    reference_test::assert_debug("song/tests/rust_live_visuals.rs::retained", &old);
    assert!(p.dir.path().exists());
}

#[test]
fn single_use_publication_callbacks_reject_recursive_and_saved_lua_calls() {
    let mut p = Pair::new();
    let result = {
        let mut runtime = p.rust.as_mut().unwrap().runtime_mut();
        runtime
            .vm_mut()
            .set_global(b"RUST_PUBLICATION_CHECK", deadlib_lua::Value::Boolean(true))
    };
    result.unwrap();
    p.script(r#"
        if RUST_PUBLICATION_CHECK then
            local constructor=Def.Actor
            for i=1,32 do
                local name,value=debug.getupvalue(constructor,i)
                if not name then break end
                if name=='getinfo' then ORIGINAL_GETINFO=value;break end
            end
            assert(type(ORIGINAL_GETINFO)=='function')
        end
        ORIGINAL_TXT_META=getmetatable(TXT)
        local mt={};for k,v in pairs(ORIGINAL_TXT_META)do mt[k]=v end
        local index=ORIGINAL_TXT_META.__index
        mt.__index=function(t,k)
            if k=='Text' then
                if RUST_PUBLICATION_CHECK then
                    local info=ORIGINAL_GETINFO(2,'fS')
                    local frames={}
                    for level=0,8 do local v=ORIGINAL_GETINFO(level,'fS');frames[#frames+1]=level..':'..tostring(v and v.what)..':'..type(v and v.func) end
                    assert(info and info.what=='C' and type(info.func)=='function',table.concat(frames,','))
                    SAVED_PUBLICATION=info.func
                    local ok,err=pcall(SAVED_PUBLICATION)
                    assert(not ok and string.find(tostring(err),'live content publication is no longer active',1,true))
                end
                collectgarbage('collect')
                return 'guarded'
            end
            if type(index)=='function' then return index(t,k) end
            return index[k]
        end
        TXT.Text=nil;setmetatable(TXT,mt)
    "#);
    p.publish(true);
    p.script(r#"
        if RUST_PUBLICATION_CHECK then
            assert(type(SAVED_PUBLICATION)=='function')
            local ok,err=pcall(SAVED_PUBLICATION)
            assert(not ok and string.find(tostring(err),'live content publication is no longer active',1,true))
        end
        SAVED_PUBLICATION=nil;collectgarbage('collect')
    "#);
    p.publish(true);
    p.script("setmetatable(TXT,ORIGINAL_TXT_META);TXT.Text='guarded';SAVED_PUBLICATION=nil;ORIGINAL_TXT_META=nil;ORIGINAL_GETINFO=nil;collectgarbage('collect')");
}

#[test]
fn late_wrapper_visuals_keep_owned_slots_and_publish_all_layers() {
    let dir = tempfile::tempdir().unwrap();
    let paths = ["background.lua", "foreground.lua", "main.lua"]
        .map(|name| dir.path().join(name));
    std::fs::write(&paths[0], r#"
        return Def.ActorFrame {Name='Background',
            Def.Quad {Name='Whoosh', InitCommand=function(self) WHOOSH=self end,
                OnCommand=function(self)
                    self:setsize(128,128):rotationz(90):rotationx(45)
                    self:AddWrapperState():y(-100):zoom(4):zoomz(1)
                    self:AddWrapperState():x(6)
                end}}
    "#).unwrap();
    for (path, name) in paths[1..].iter().zip(["Foreground", "Main"]) {
        std::fs::write(path, format!(r#"
            return Def.ActorFrame {{Name='{name}',
                Def.Quad {{Name='{name}Leaf', OnCommand=function(self)
                    self:AddWrapperState():x(7)
                end}}}}
        "#)).unwrap();
    }
    let mut context = song::SongLuaCompileContext::new(dir.path(), "late wrapper publication");
    context.live = true;
    context.defer_initial_live_update = true;
    context.music_length_seconds = 10.;
    context.background_layer_count = 1;
    context.foreground_names = vec!["foreground.lua".into(), "main.lua".into()];
    context.layer_start_beats = vec![0., 0., 0.];
    context.players[0].step_statistics_enabled = false;
    context.players[1].enabled = false;
    let refs = paths.iter().map(|path| path.as_path()).collect::<Vec<_>>();
    let mut outputs = song::compile_song_lua_layers(
        &refs, 2, &context, Default::default(), slots, layer, |_, _| None,
    ).unwrap();
    let mut scene = outputs.iter_mut().find_map(|output| output.live_scene.take()).unwrap();
    let bindings = outputs.iter().enumerate().flat_map(|(layer, output)| {
        output.live_actor_indices().iter().enumerate().map(move |(local, &global)|
            (global, match layer {
                0 => Target::Background(0, local),
                1 => Target::Foreground(0, local),
                _ => Target::Root(local),
            }))
    }).collect();
    scene.bind_targets(bindings).unwrap();
    let main = outputs.pop().unwrap();
    let foreground = outputs.pop().unwrap();
    let background = outputs.pop().unwrap();
    let make_layer = |output: song::CompiledSongLua<Actor>| {
        deadsync_gameplay::SongLuaVisualLayerRuntime {
            start_second: 0., screen_width: output.screen_width, screen_height: output.screen_height,
            foreground_beats: None,
            overlays: output.overlays, overlay_eases: Vec::new(), overlay_ease_ranges: Vec::new(),
            overlay_events: Vec::new(), song_foreground: output.song_foreground,
            song_foreground_events: Vec::new(),
        }
    };
    let mut visual = visuals(main);
    visual.background_visual_layers.push(make_layer(background));
    visual.foreground_visual_layers.push(make_layer(foreground));
    let before = [visual.overlays.len(), visual.background_visual_layers[0].overlays.len(),
        visual.foreground_visual_layers[0].overlays.len()];
    let whoosh = visual.background_visual_layers[0].overlays.iter()
        .position(|actor| actor.name.as_deref() == Some("Whoosh")).unwrap();
    let original_parent = visual.background_visual_layers[0].overlays[whoosh].parent_index;
    let screen = visual.screen_overlay_index;
    scene.write_visuals(&mut visual).unwrap();
    assert!(!visual.overlays.iter().any(|actor| matches!(actor.kind, Kind::WrapperState)));
    assert!(!visual.foreground_visual_layers[0].overlays.iter()
        .any(|actor| matches!(actor.kind, Kind::WrapperState)));
    assert!(!visual.background_visual_layers[0].overlays.iter()
        .any(|actor| matches!(actor.kind, Kind::WrapperState)));
    let update = |scene: &mut song::SongLiveScene, seconds, delta| {
        let output = scene.update(song::SongLuaLiveFrame {
            music_seconds: seconds, screen_seconds: seconds, beat: seconds * 2.,
            delta_seconds: delta, ..Default::default()
        }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
    };
    update(&mut scene, 0.1, 0.1);
    scene.write_visuals(&mut visual).unwrap();
    assert_eq!(visual.screen_overlay_index, screen);
    assert_eq!(visual.overlays.len(), before[0] + 1);
    assert_eq!(visual.foreground_visual_layers[0].overlays.len(), before[2] + 1);
    let background = &visual.background_visual_layers[0].overlays;
    assert_eq!(background.len(), before[1] + 2);
    assert_eq!(background[whoosh].name.as_deref(), Some("Whoosh"));
    let inner = background[whoosh].parent_index.unwrap();
    let outer = background[inner].parent_index.unwrap();
    assert_eq!((inner, outer), (before[1], before[1] + 1));
    assert_eq!(background[outer].parent_index, original_parent);
    let pose = background[inner].initial_state;
    assert_eq!((pose.y, pose.zoom_x, pose.zoom_y, pose.zoom_z), (-100., 4., 4., 1.));
    assert_eq!(song::overlay_state_axis_scale(pose), [4., 4.]);
    assert_eq!(song::overlay_state_z_scale(pose), 1.);
    assert_eq!(background[outer].initial_state.x, 6.);
    assert_eq!(song::overlay_state_axis_scale(background[whoosh].initial_state), [1., 1.]);
    let paths = scene.visual_bindings().unwrap();
    let owner_path = paths.iter().find_map(|(path, target)| match target {
        Target::Background(0, index) if *index == whoosh => Some(path.clone()),
        _ => None,
    }).unwrap();
    for (index, ordinal) in [(inner, 1), (outer, 2)] {
        assert!(paths.iter().any(|(path, target)| path == &format!("{owner_path}/WrapperState{ordinal}")
            && matches!(target, Target::Background(0, local) if *local == index)));
    }
    let count = scene.visual_draw_bindings().len();
    scene.write_visuals(&mut visual).unwrap();
    assert_eq!(scene.visual_draw_bindings().len(), count, "publication is idempotent");
    {
        let mut runtime = scene.runtime_mut();
        let program = compile_named(br#"
            WHOOSH:GetWrapperState(1):linear(.5):y(-80)
            WHOOSH:GetWrapperState(1):AddWrapperState():x(999)
            collectgarbage('collect')
        "#, b"@/late-wrapper-update.lua").unwrap();
        runtime.vm_mut().execute(&program).unwrap();
    }
    update(&mut scene, 0.35, 0.25);
    scene.write_visuals(&mut visual).unwrap();
    assert_eq!(visual.background_visual_layers[0].overlays[inner].initial_state.y, -90.);
    assert_eq!(scene.visual_draw_bindings().len(), count,
        "Actor::Draw does not recurse through a wrapper's own wrapper list");
    {
        let mut runtime = scene.runtime_mut();
        let program = compile_named(br#"WHOOSH:AddWrapperState():z(12)"#,
            b"@/late-wrapper-second-attachment.lua").unwrap();
        runtime.vm_mut().execute(&program).unwrap();
    }
    update(&mut scene, 0.5, 0.15);
    scene.write_visuals(&mut visual).unwrap();
    let background = &visual.background_visual_layers[0].overlays;
    let newest = before[1] + 2;
    assert_eq!(background.len(), newest + 1);
    assert_eq!(background[outer].parent_index, Some(newest));
    assert_eq!(background[newest].parent_index, original_parent);
    assert_eq!(background[newest].initial_state.z, 12.);
    assert_eq!(scene.visual_draw_bindings().len(), count + 1,
        "a later wrapper in the same scene triggers another publication");
    assert!(scene.visual_bindings().unwrap().iter().any(|(path, target)|
        path == &format!("{owner_path}/WrapperState3")
            && matches!(target, Target::Background(0, local) if *local == newest)));
    scene.end().unwrap();
}

#[test]
fn scheduled_foreground_on_load_publishes_initially_textureless_sprite() {
    use deadsync_assets::song_lua::SongLuaOverlayKind as RenderKind;
    use song::playback::actor_conformance::WholeSongComposer;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("foreground.lua");
    let texture = dir.path().join("header.png");
    image::RgbaImage::new(854, 85).save(&texture).unwrap();
    std::fs::write(&path, r#"
        LOAD_COUNT=0
        return Def.ActorFrame {
            OnCommand=function(self) self:sleep(1000) end,
            Def.Sprite {Name='Header', InitCommand=function(self) self:visible(false) end,
                OnCommand=function(self)
                    LOAD_COUNT=LOAD_COUNT+1
                    self:Load('header.png'):x(427):y(42):SetWidth(854):visible(true)
                end},
        }
    "#).unwrap();
    let mut context = song::SongLuaCompileContext::new(dir.path(), "scheduled sprite load");
    context.live = true;
    context.defer_initial_live_update = true;
    context.layer_start_beats = vec![1.];
    context.music_length_seconds = 10.;
    context.players[0].step_statistics_enabled = false;
    context.players[1].enabled = false;
    let mut output: deadsync_assets::song_lua::CompiledSongLua = song::compile_song_lua_layers(
        &[&path], 0, &context, Default::default(),
        |_, _, _| Ok(Arc::<[deadsync_assets::noteskin::SpriteSlot]>::from([])),
        |_| None, |_, _| None,
    ).unwrap().remove(0);
    let mut scene = output.live_scene.take().unwrap();
    scene.bind_targets(output.live_actor_indices().iter().copied().enumerate()
        .map(|(local, global)| (global, Target::Root(local))).collect()).unwrap();
    let mut visual = visuals(output);
    let index = visual.overlays.iter()
        .position(|actor| actor.name.as_deref() == Some("Header")).unwrap();
    scene.write_visuals(&mut visual).unwrap();
    assert!(matches!(visual.overlays[index].kind, RenderKind::Actor));
    assert!(!visual.overlays[index].initial_state.sprite_texture);
    let mut composer = WholeSongComposer::new(&visual.overlays);
    for (seconds, beat, delta) in [(0., 0., 0.), (0.6, 1.2, 0.6)] {
        let updated = scene.update(song::SongLuaLiveFrame {
            music_seconds: seconds, screen_seconds: seconds, beat,
            delta_seconds: delta, ..Default::default()
        }).unwrap();
        assert!(updated.errors.is_empty(), "{:?}", updated.errors);
        scene.write_visuals(&mut visual).unwrap();
        if beat < 1. {
            assert!(matches!(visual.overlays[index].kind, RenderKind::Actor));
        }
    }
    let RenderKind::Sprite { texture_path, texture_key, .. } = &visual.overlays[index].kind else {
        panic!("OnCommand's texture did not reach the prepared visual: {:?}", visual.overlays[index]);
    };
    assert_eq!(texture_path, &texture);
    assert_eq!(Path::new(texture_key.as_ref()), texture_path);
    let state = visual.overlays[index].initial_state;
    assert!(state.visible && state.sprite_texture);
    assert_eq!(state.size, Some([854., 85.]));
    assert_eq!([state.x, state.y], [427., 42.]);
    let states = visual.overlays.iter().map(|actor| actor.initial_state).collect::<Vec<_>>();
    let frame = composer.render_overlay(&visual.overlays, &states, index, [854., 480.], 0.6, 1.2);
    let [sprite] = frame.sprite_instances.as_slice() else {
        panic!("expected one loaded Sprite: {:?}", frame.ops);
    };
    assert_eq!(sprite.size, [854., 85.]);
    let [deadlib_render_core::DrawOp::Sprite(draw)] = frame.ops.as_slice() else {
        panic!("expected one Sprite draw: {:?}", frame.ops);
    };
    assert_eq!(composer.texture_key(draw.texture_handle), texture_key.as_ref());
    scene.write_visuals(&mut visual).unwrap();
    {
        let mut runtime = scene.runtime_mut();
        let check = compile_named(b"assert(LOAD_COUNT==1)", b"@/scheduled-sprite-check.lua").unwrap();
        runtime.vm_mut().execute(&check).unwrap();
    }
    scene.end().unwrap();
}

#[test]
fn mixed_case_texture_resolves_on_linux_and_preserves_its_logical_path() {
    use deadsync_assets::song_lua::SongLuaOverlayKind as RenderKind;
    use song::playback::actor_conformance::WholeSongComposer;

    let dir = tempfile::tempdir().unwrap();
    let physical_dir = dir.path().join("MixedCase");
    std::fs::create_dir(&physical_dir).unwrap();
    let physical = physical_dir.join("Shivers 3x2.png");
    // Cold's actual 3x2 sheet has a nonintegral default frame width.
    image::RgbaImage::new(1750, 1050).save(&physical).unwrap();
    let logical = dir.path().join("mixedcase/shivers 3x2.png");
    #[cfg(target_os = "linux")]
    assert!(!logical.exists(), "the regression requires a case-sensitive filesystem");
    let script = dir.path().join("default.lua");
    std::fs::write(&script, format!(r#"
        return Def.ActorFrame {{ Def.Sprite {{
            Name='MixedCaseTexture', Texture='mixedcase/shivers 3x2.png',
            InitCommand=function(self)
                local texture=self:GetTexture()
                assert(self:GetWidth()==583 and self:GetHeight()==525)
                assert(texture:GetSourceWidth()==1750 and texture:GetSourceHeight()==1050)
                assert(texture:GetNumFrames()==6)
                assert(texture:GetPath()=={:?}, texture:GetPath())
                self:animate(false):SetStateProperties{{{{Frame=1,Delay=1}}}}
                assert(self:GetWidth()==583 and self:GetHeight()==525)
            end,
            OnCommand=function(self) self:sleep(1000) end,
        }}, Def.Sprite {{
            Name='DeclaredFrames', Texture='mixedcase/shivers 3x2.png',
            Frames={{{{Frame=0,Delay=1}},{{Frame=1,Delay=1}}}},
            InitCommand=function(self)
                DECLARED_FRAMES=self
                assert(math.abs(self:GetWidth()-1750/3)<0.0001 and self:GetHeight()==525)
                self:animate(false):SetStateProperties{{{{Frame=1,Delay=1}}}}
                assert(math.abs(self:GetWidth()-1750/3)<0.0001 and self:GetHeight()==525)
            end,
            OnCommand=function(self) self:sleep(1000) end,
        }} }}
    "#, song::file_path_string(&logical))).unwrap();
    let mut context = song::SongLuaCompileContext::new(dir.path(), "case-insensitive native media");
    context.live = true;
    context.music_length_seconds = 10.;
    context.players[1].enabled = false;
    let mut compiled: deadsync_assets::song_lua::CompiledSongLua = song::compile_song_lua_layers(
        &[&script], 0, &context, Default::default(),
        |_, _, _| Ok(Arc::<[deadsync_assets::noteskin::SpriteSlot]>::from([])),
        |_| None, |_, _| None,
    ).unwrap().remove(0);
    let mut scene = compiled.live_scene.take().unwrap();
    scene.bind_targets(compiled.live_actor_indices().iter().copied().enumerate()
        .map(|(local, global)| (global, Target::Root(local))).collect()).unwrap();
    let mut visual = visuals(compiled);
    scene.write_visuals(&mut visual).unwrap();
    let states = visual.overlays.iter().map(|actor| actor.initial_state).collect::<Vec<_>>();
    let mut composer = WholeSongComposer::new(&visual.overlays);
    for (name, expected) in [
        ("MixedCaseTexture", [583., 525.]),
        ("DeclaredFrames", [1750. / 3., 525.]),
    ] {
        let index = visual.overlays.iter()
            .position(|actor| actor.name.as_deref() == Some(name)).unwrap();
        let actor = &visual.overlays[index];
        let RenderKind::Sprite { texture_path, .. } = &actor.kind else {
            panic!("mixed-case texture did not produce a live visual: {:?}", actor.kind);
        };
        assert!(texture_path.is_file());
        #[cfg(target_os = "linux")]
        assert_eq!(texture_path, &physical);
        let frame = composer.render_overlay(&visual.overlays, &states, index, [854., 480.], 0., 0.);
        let [sprite] = frame.sprite_instances.as_slice() else {
            panic!("expected one resolved Sprite draw: {:?}", frame.ops);
        };
        assert_eq!(sprite.size, expected, "{name}");
    }
    {
        let mut runtime = scene.runtime_mut();
        let check = compile_named(br#"
            local a=DECLARED_FRAMES
            a:SetTexture(a:GetTexture())
            assert(a:GetWidth()==583 and a:GetHeight()==525)
            a:SetStateProperties{{Frame=0,Delay=1}}
            assert(a:GetWidth()==583 and a:GetHeight()==525)
        "#, b"@/declared-frame-reload.lua").unwrap();
        runtime.vm_mut().execute(&check).unwrap();
    }
    let outcome = scene.update(song::SongLuaLiveFrame {
        delta_seconds: 0.,
        ..Default::default()
    }).unwrap();
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    scene.write_visuals(&mut visual).unwrap();
    let index = visual.overlays.iter()
        .position(|actor| actor.name.as_deref() == Some("DeclaredFrames")).unwrap();
    let states = visual.overlays.iter().map(|actor| actor.initial_state).collect::<Vec<_>>();
    let frame = composer.render_overlay(&visual.overlays, &states, index, [854., 480.], 0., 0.);
    assert_eq!(frame.sprite_instances[0].size, [583., 525.]);
    scene.end().unwrap();
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;

#[test]
fn scheduled_amv_aft_publication_tracks_binding_changes_without_replacing_slots() {
    use deadlib_render_core::{DrawOp, TextureHandle, render_target_base_handle};
    use deadsync_assets::song_lua::SongLuaOverlayKind as RenderKind;
    use song::playback::actor_conformance::WholeSongComposer;

    type RenderActor = deadsync_assets::song_lua::SongLuaOverlayActor;
    fn script(scene: &mut song::SongLiveScene, source: &str) {
        let code = compile_named(source.as_bytes(), b"@/live-aft-binding.lua").unwrap();
        let result = {
            let mut runtime = scene.runtime_mut();
            runtime.vm_mut().execute(&code).map(|_| ())
        };
        result.unwrap();
    }
    fn publish(scene: &mut song::SongLiveScene, visual: &mut Visuals<RenderActor>, seconds: f64) {
        let output = scene.update(song::SongLuaLiveFrame {
            music_seconds: seconds, screen_seconds: seconds, beat: seconds * 2.,
            delta_seconds: 0., ..Default::default()
        }).unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        scene.write_visuals(visual).unwrap();
    }
    #[track_caller]
    fn bound_texture(composer: &mut WholeSongComposer, visual: &Visuals<RenderActor>, index: usize) -> TextureHandle {
        let states = visual.overlays.iter().map(|actor| actor.initial_state).collect::<Vec<_>>();
        let frame = composer.render_overlay(&visual.overlays, &states, index, [640., 480.], 0.6, 1.2);
        let [DrawOp::TexturedMesh(draw)] = frame.ops.as_slice() else {
            panic!("expected one textured AMV submission for {:?}: {:?}; kind={:?}; state={:?}",
                visual.overlays[index].name, frame.ops, visual.overlays[index].kind, states[index]);
        };
        draw.texture_handle
    }

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("foreground.lua");
    let ordinary = dir.path().join("ordinary.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([255; 4])).save(&ordinary).unwrap();
    std::fs::write(&path, r#"
        CREATE_COUNT=0
        local vertices={{{-8,-8,0},{1,1,1,1},{0,0}},{{8,-8,0},{1,1,1,1},{1,0}},{{0,8,0},{1,1,1,1},{.5,1}}}
        return Def.ActorFrame {
            OnCommand=function(self)
                self:sleep(1000)
                CREATE_COUNT=CREATE_COUNT+1
                A:SetWidth(32.9):SetHeight(16.9):SetTextureName('capture-a')
                    :EnableAlphaBuffer(true):EnableDepthBuffer(true):EnableFloat(true):Create()
                M:SetTexture(A:GetTexture())
            end,
            Def.ActorFrameTexture {Name='A',InitCommand=function(self) A=self end,
                Def.Quad {InitCommand=function(self) self:setsize(8,8) end}},
            Def.ActorFrameTexture {Name='B',InitCommand=function(self)
                B=self;self:SetWidth(64):SetHeight(32):SetTextureName('capture-b'):Create()
            end},
            Def.ActorMultiVertex {Name='Mesh',InitCommand=function(self)
                M=self;self:SetDrawState{Mode='DrawMode_Triangles'}:SetVertices(vertices)
            end},
            Def.Sprite {Name='Ordinary',Texture='ordinary.png',InitCommand=function(self) IMAGE=self end},
            Def.ActorMultiVertex {Name='Alias',Texture='capture-b',InitCommand=function(self)
                ALIAS=self;self:SetDrawState{Mode='DrawMode_Triangles'}:SetVertices(vertices)
            end},
        }
    "#).unwrap();
    let mut context = song::SongLuaCompileContext::new(dir.path(), "late AMV AFT binding");
    context.live = true;
    context.defer_initial_live_update = true;
    context.layer_start_beats = vec![1.];
    context.music_length_seconds = 10.;
    context.players[0].step_statistics_enabled = false;
    context.players[1].enabled = false;
    let mut output: deadsync_assets::song_lua::CompiledSongLua = song::compile_song_lua_layers(
        &[&path], 0, &context, Default::default(),
        |_, _, _| Ok(Arc::<[deadsync_assets::noteskin::SpriteSlot]>::from([])),
        |_| None, |_, _| None,
    ).unwrap().remove(0);
    let mut scene = output.live_scene.take().unwrap();
    scene.bind_targets(output.live_actor_indices().iter().copied().enumerate()
        .map(|(local, global)| (global, Target::Root(local))).collect()).unwrap();
    let mut visual = visuals(output);
    let index = |name| visual.overlays.iter().position(|actor| actor.name.as_deref() == Some(name)).unwrap();
    let (a, b, mesh, alias) = (index("A"), index("B"), index("Mesh"), index("Alias"));
    let count = visual.overlays.len();
    scene.write_visuals(&mut visual).unwrap();
    assert!(matches!(visual.overlays[a].kind, RenderKind::ActorFrameTexture { capture_size: None, .. }));
    assert!(matches!(visual.overlays[mesh].kind, RenderKind::ActorMultiVertex { capture_name: None, .. }));
    assert!(matches!(&visual.overlays[alias].kind, RenderKind::ActorMultiVertex { capture_name: Some(name), .. } if name == "capture-b"),
        "unchanged declarative registered-texture binding remains valid");
    let mut composer = WholeSongComposer::new(&visual.overlays);
    let a_handle = composer.capture_handle(a).unwrap();
    let b_handle = composer.capture_handle(b).unwrap();
    publish(&mut scene, &mut visual, 0.);
    assert!(matches!(visual.overlays[a].kind, RenderKind::ActorFrameTexture { capture_size: None, .. }));
    publish(&mut scene, &mut visual, 0.6);
    let RenderKind::ActorFrameTexture { capture_name, capture_size, alpha_buffer, depth_buffer, float_buffer } = &visual.overlays[a].kind else { panic!("AFT slot changed kind") };
    assert_eq!(capture_name, "capture-a");
    assert_eq!(*capture_size, Some([32., 16.]));
    assert_eq!((*alpha_buffer, *depth_buffer, *float_buffer), (true, true, true));
    assert!(visual.overlays[a].initial_state.aft_created);
    assert_eq!(visual.overlays.len(), count);
    assert!(matches!(&visual.overlays[mesh].kind, RenderKind::ActorMultiVertex { capture_name: Some(name), texture_path: None, texture_key: None, .. } if name == "capture-a"));
    assert_eq!(render_target_base_handle(bound_texture(&mut composer, &visual, mesh)), a_handle);
    assert_eq!(render_target_base_handle(bound_texture(&mut composer, &visual, alias)), b_handle);

    script(&mut scene, "saved=B:GetTexture();M:SetTexture(saved);collectgarbage('collect');assert(M:GetTexture()==saved)");
    publish(&mut scene, &mut visual, 0.7);
    assert!(matches!(&visual.overlays[mesh].kind, RenderKind::ActorMultiVertex { capture_name: Some(name), .. } if name == "capture-b"));
    assert_eq!(render_target_base_handle(bound_texture(&mut composer, &visual, mesh)), b_handle);
    script(&mut scene, "M:SetTexture(M:GetTexture());A:SetWidth(128):SetHeight(256):SetTextureName('later-request'):EnableAlphaBuffer(false):EnableDepthBuffer(false):EnableFloat(false)");
    publish(&mut scene, &mut visual, 0.8);
    assert_eq!(render_target_base_handle(bound_texture(&mut composer, &visual, mesh)), b_handle);
    let RenderKind::ActorFrameTexture { capture_name, capture_size, alpha_buffer, depth_buffer, float_buffer } = &visual.overlays[a].kind else { panic!("AFT slot changed kind") };
    assert_eq!(capture_name, "capture-a");
    assert_eq!(*capture_size, Some([32., 16.]));
    assert_eq!((*alpha_buffer, *depth_buffer, *float_buffer), (true, true, true),
        "Create freezes allocation metadata independently of later request setters");

    // The file and its binding did not exist when this composer was constructed.
    // This exercises late resource publication on every platform, independently
    // of Windows separator differences in the earlier prepared Sprite's handle.
    let late = dir.path().join("late-ordinary.png");
    assert!(!late.exists());
    image::RgbaImage::from_pixel(9, 5, image::Rgba([255; 4])).save(&late).unwrap();
    script(&mut scene, &format!("M:LoadTexture({:?})", song::file_path_string(&late)));
    publish(&mut scene, &mut visual, 0.85);
    let RenderKind::ActorMultiVertex {
        capture_name: None, texture_path: Some(late_path), texture_key: Some(late_key), ..
    } = &visual.overlays[mesh].kind else { panic!("late ordinary texture retained the AFT binding") };
    assert_eq!(late_path, &late);
    let late_key = late_key.clone();
    let late_handle = bound_texture(&mut composer, &visual, mesh);
    assert!(!deadlib_render_core::is_render_target_texture(late_handle));
    assert_eq!(composer.texture_key(late_handle), late_key.as_ref());
    assert_eq!((composer.capture_handle(a), composer.capture_handle(b)), (Some(a_handle), Some(b_handle)));
    assert_eq!(visual.overlays.len(), count);

    script(&mut scene, "saved=IMAGE:GetTexture();M:SetTexture(saved);ALIAS:SetTexture(saved);collectgarbage('collect');assert(M:GetTexture()==saved);assert(ALIAS:GetTexture()==saved)");
    publish(&mut scene, &mut visual, 0.9);
    let RenderKind::ActorMultiVertex { capture_name: None, texture_path: Some(path), texture_key: Some(key), .. } = &visual.overlays[mesh].kind else { panic!("ordinary texture did not replace AFT") };
    assert_eq!(path, &ordinary);
    let key = key.clone();
    let handle = bound_texture(&mut composer, &visual, mesh);
    assert!(!deadlib_render_core::is_render_target_texture(handle));
    assert_eq!(composer.texture_key(handle), key.as_ref());
    let RenderKind::ActorMultiVertex { capture_name: None, texture_path: Some(alias_path), texture_key: Some(alias_key), .. } = &visual.overlays[alias].kind else { panic!("declarative alias retained a stale AFT after ordinary SetTexture") };
    assert_eq!(alias_path, &ordinary);
    assert_eq!(alias_key, &key);
    let alias_handle = bound_texture(&mut composer, &visual, alias);
    assert!(!deadlib_render_core::is_render_target_texture(alias_handle));
    assert_eq!(composer.texture_key(alias_handle), key.as_ref());

    script(&mut scene, "M:SetTexture(A:GetTexture());ALIAS:LoadTexture(nil)");
    publish(&mut scene, &mut visual, 1.);
    assert_eq!(render_target_base_handle(bound_texture(&mut composer, &visual, mesh)), a_handle);
    assert!(matches!(visual.overlays[alias].kind, RenderKind::ActorMultiVertex { capture_name: None, .. }));
    let alias_handle = bound_texture(&mut composer, &visual, alias);
    assert!(!deadlib_render_core::is_render_target_texture(alias_handle));
    assert_eq!(composer.texture_key(alias_handle), "__songlua_native_default_texture.png");
    script(&mut scene, "M:LoadTexture(nil);saved=B:GetTexture();ALIAS:SetTexture(saved);collectgarbage('collect');assert(ALIAS:GetTexture()==saved);assert(M:GetTexture():GetPath()=='');assert(CREATE_COUNT==1)");
    publish(&mut scene, &mut visual, 1.1);
    let RenderKind::ActorMultiVertex { capture_name: None, texture_key: Some(key), .. } = &visual.overlays[mesh].kind else { panic!("default texture retained a stale AFT") };
    let key = key.clone();
    let handle = bound_texture(&mut composer, &visual, mesh);
    assert!(!deadlib_render_core::is_render_target_texture(handle));
    assert_eq!(composer.texture_key(handle), key.as_ref());
    assert_eq!(key.as_ref(), "__songlua_native_default_texture.png");
    assert!(matches!(&visual.overlays[alias].kind, RenderKind::ActorMultiVertex { capture_name: Some(name), texture_path: None, texture_key: None, .. } if name == "capture-b"));
    assert_eq!(render_target_base_handle(bound_texture(&mut composer, &visual, alias)), b_handle);
    assert_eq!((composer.capture_handle(a), composer.capture_handle(b)), (Some(a_handle), Some(b_handle)));
    assert_eq!(visual.overlays.len(), count);
    scene.end().unwrap();
}
