use deadlib_lua::compile_named;
use deadlib_present::actors::TextAttribute;
use deadsync_song_lua::{self as song, SongLuaLiveTarget as Target, rust_engine as rust};
use std::{path::Path, sync::Arc};

type Kind = song::SongLuaOverlayKind<(), (), TextAttribute>;
type Actor = song::SongLuaOverlayActor<Kind>;
type Visuals = deadsync_gameplay::SongLuaRuntimeVisuals<
    Actor,
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

fn visuals<L>(out: song::CompiledSongLuaData<Actor, L>) -> Visuals {
    Visuals {
        overlays: out.overlays,
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

struct Pair {
    rust: Option<rust::SongLiveScene>,
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
        reference_test::register_path(dir.path());
        let path = dir.path().join("default.lua");
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

        let mut r = rust::compile_song_lua_layers(
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
        let r = self.rust.as_ref().unwrap().write_visuals(&mut self.r);

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

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
