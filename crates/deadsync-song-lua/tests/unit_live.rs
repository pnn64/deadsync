//! Existing live microfixtures exercising the public Rust host facade.
use deadsync_song_lua::{self as song, *};
use std::{fs, path::Path, sync::Arc};
type Compiled = CompiledSongLua<
    SongLuaOverlayActor<SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>>,
>;
fn compile(name: &str, source: &str) -> Compiled {
    compile_with_context(name, source, |_| {})
}
fn compile_with_context(
    name: &str,
    source: &str,
    configure: impl FnOnce(&mut SongLuaCompileContext),
) -> Compiled {
    let dir = std::env::temp_dir().join(format!("deadsync-live-{name}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("test directory");
    let entry = dir.join("default.lua");
    fs::write(&entry, source).expect("test script");
    let mut context = SongLuaCompileContext::new(&dir, name);
    context.live = true;
    context.music_length_seconds = 60.0;
    configure(&mut context);
    song::compile_song_lua(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("live preparation")
}
fn write_interactive_sound(dir: &Path, name: &str) {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // One silent PCM sample: real, independently named audio assets.
    let mut wav = b"RIFF\x26\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0".to_vec();
    wav.extend_from_slice(&8000_u32.to_le_bytes());
    wav.extend_from_slice(&16000_u32.to_le_bytes());
    wav.extend_from_slice(b"\x02\0\x10\0data\x02\0\0\0\0\0");
    fs::write(path, wav).unwrap();
}

#[cfg(feature = "test-support")]
#[test]
fn live_overlay_stage_observation_precedes_foreground_mutations() {
    let mut compiled = compile_with_context(
        "overlay-stage-phase",
        r#"
return Def.Actor {
    InitCommand = function(self) self:sleep(1000) end,
    OnCommand = function(self)
        GAMESTATE:SetCurrentSteps(PLAYER_1, GAMESTATE:GetCurrentSong():GetAllSteps()[2])
        SCREENMAN:GetTopScreen():GetChild('PlayerP1'):SetLife(0.2)
        STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1):FailPlayer()
    end,
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![-24.0];
            context.players[0].enabled = true;
            context.players[1].enabled = false;
            context.song_steps = [SongLuaDifficulty::Challenge, SongLuaDifficulty::Edit]
                .into_iter()
                .map(|difficulty| song::SongLuaStep {
                    steps_type: "StepsType_Dance_Single".into(),
                    difficulty,
                    description: format!("{difficulty:?}"),
                    chart_name: String::new(),
                    credit: String::new(),
                    meter: 9,
                    music_file: String::new(),
                    possible_grade_points: 100,
                })
                .collect();
            context.player_step_indices = [Some(0), Some(0)];
        },
    );
    let mut scene = compiled.live_scene.take().unwrap();
    scene.enable_overlay_stage_observer().unwrap();
    let mut frame = SongLuaLiveFrame {
        beat: -4.0,
        music_seconds: -2.0,
        screen_seconds: 0.0,
        delta_seconds: 0.0,
        ..Default::default()
    };
    frame.players[0] = SongLuaLivePlayer {
        possible_points: 100,
        actual_points: 17,
        current_possible_points: 25,
        life: 0.7,
        fail_enabled: true,
        ..Default::default()
    };
    frame.players[0].tap_scores[4] = 3;
    frame.players[0].tap_scores[8] = 1;
    frame.players[0].tap_scores[9] = 2;
    let output = scene.advance(frame).unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert_eq!(
        scene.selected_context().unwrap().player_step_indices[0],
        Some(1)
    );
    assert!(
        output
            .player_writes
            .contains(&SongLuaLivePlayerWrite::Fail { player: 0 })
    );
    let observed = scene
        .take_overlay_stage_observation()
        .expect("overlay stage observation must precede foreground mutations");
    assert_eq!(observed.player_step_indices[0], Some(0));
    assert_eq!(observed.music_seconds, -2.0);
    assert_eq!(observed.beat, -4.0);
    let player = observed.players[0];
    assert_eq!(
        (
            player.possible_points,
            player.actual_points,
            player.current_possible_points
        ),
        (100, 17, 25)
    );
    assert_eq!(player.life, 0.7);
    assert!(!player.failed);
    assert_eq!(
        (
            player.tap_scores[4],
            player.tap_scores[8],
            player.tap_scores[9]
        ),
        (3, 1, 2)
    );
    assert!(
        scene.take_overlay_stage_observation().is_none(),
        "observations drain once"
    );
}

#[test]
fn live_versus_children_retain_native_player_construction_groups() {
    let mut compiled = compile_with_context(
        "versus-screen-child-order",
        r#"
return Def.Actor {
 OnCommand=function(self)
  local selected = {PlayerP1=true, PlayerP2=true, OniGameOverP1=true, OniGameOverP2=true}
  local names = {}
  SCREENMAN:GetTopScreen():RunCommandsOnChildren(function(child)
   local name=child:GetName()
   if selected[name] then names[#names+1]=name end
  end)
  assert(table.concat(names, ',') == 'PlayerP1,PlayerP2,OniGameOverP1,OniGameOverP2',
   'native Versus child traversal: '..table.concat(names, ','))
 end
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
            context.players[0].enabled = true;
            context.players[1].enabled = true;
        },
    );
    let mut scene = compiled.live_scene.take().expect("owned Versus screen");
    let output = scene.advance(SongLuaLiveFrame::default()).unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
}

#[test]
fn stage_failure_is_visible_without_life_failure() {
    let mut player = deadsync_gameplay::init_player_runtime();
    assert!(!song::SongLuaLivePlayer::from_player(&player, 100).failed);
    player.stage_failed = true;
    let frame = song::SongLuaLivePlayer::from_player(&player, 100);
    assert!(frame.failed);
    assert_eq!(frame.life, 0.5);
    assert_eq!(frame.actual_points, 0);
    assert_eq!(player.fail_time, None);
    assert!(!player.is_failing);
}

#[test]
fn interactive_runtime_stops_only_the_authored_sound_actor() {
    let mut compiled = compile_with_context(
        "independent-sounds",
        r#"
local a,b
return Def.ActorFrame {
    Def.Sound { File='same.wav', InitCommand=function(self) a=self end },
    Def.Sound { File='same.wav', InitCommand=function(self) b=self end },
    OnCommand=function(self) a:play(); b:play(); a:stop() end,
}
"#,
        |context| write_interactive_sound(&context.song_dir, "same.wav"),
    );
    let output = compiled.live_scene.as_mut().unwrap().take_startup_output();
    let [
        SongLuaActorSound::Play { owner: a, path: pa },
        SongLuaActorSound::Play { owner: b, path: pb },
        SongLuaActorSound::Stop { owner: stop },
    ] = output.actor_sounds.as_slice()
    else {
        panic!("unexpected audio sequence: {:?}", output.actor_sounds);
    };
    assert_ne!(a, b);
    assert_eq!(pa, pb);
    assert_eq!(a, stop);
}

#[test]
fn live_foreground_on_observes_updated_underlay() {
    let mut compiled = compile_with_context(
        "foreground-underlay-order",
        r#"
local underlay = SCREENMAN:GetTopScreen():GetChild('Underlay')
return Def.ActorFrame {
    InitCommand = function(self) underlay:sleep(0.5) end,
    OnCommand = function(self)
        self:aux(underlay:GetTweenTimeLeft()):sleep(10)
    end,
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
        },
    );
    let mut scene = compiled.live_scene.take().expect("scene");
    finish_note_field_intro_fixture(&scene);
    scene
        .advance(SongLuaLiveFrame {
            music_seconds: -0.5,
            beat: -1.0,
            screen_seconds: 0.125,
            delta_seconds: 0.125,
            ..Default::default()
        })
        .expect("dormant foreground");
    scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.0625,
            beat: 0.125,
            screen_seconds: 0.1875,
            delta_seconds: 0.0625,
            ..Default::default()
        })
        .expect("foreground enters");
    assert_eq!(
        scene.states()[0].aux,
        0.3125,
        "Foreground::Update sends On after this frame's underlay update"
    );
}

#[test]
fn live_gameplay_lead_in_metrics_evaluate_current_theme_rate() {
    let compiled = compile_with_context(
        "gameplay-lead-in-metrics",
        r#"
local MusicLengthSeconds = GAMESTATE:GetCurrentSong():MusicLengthSeconds()
local MinSecondsToStep = THEME:GetMetric('ScreenGameplay', 'MinSecondsToStep')
local MinSecondsToMusic = THEME:GetMetric('ScreenGameplay', 'MinSecondsToMusic')
local TotalSeconds = MusicLengthSeconds + math.max(MinSecondsToStep, MinSecondsToMusic)
assert(TotalSeconds == 69, 'Numujika lead-in calculation with 1.5x theme rate')
GAMESTATE:GetSongOptionsObject('ModsLevel_Song'):MusicRate(2.5)
SL.Global.ActiveModifiers.MusicRate = 1.03
assert(THEME:GetMetric('ScreenGameplay', 'MinSecondsToStep') == 6.18)
assert(THEME:GetMetric('ScreenGameplay', 'MinSecondsToMusic') == 2.06)
assert(THEME:GetMetric('ScreenGameplay', 'MinSecondsToStepNextSong') == 4.12)
local names = {}
for _, name in ipairs(THEME:GetMetricNamesInGroup('ScreenGameplay')) do names[name] = true end
SL.Global.ActiveModifiers.MusicRate = nil
for _, name in ipairs({'MinSecondsToStep', 'MinSecondsToMusic', 'MinSecondsToStepNextSong'}) do
    assert(THEME:HasMetric('ScreenGameplay', name) and names[name])
    assert(not pcall(function() return THEME:GetMetric('ScreenGameplay', name) end))
end
SL.Global.ActiveModifiers.MusicRate = 0
assert(THEME:GetMetric('ScreenGameplay', 'MinSecondsToStep') == 0)
SL.Global.ActiveModifiers.MusicRate = 1.5
return Def.Actor{}
"#,
        |context| context.song_music_rate = 1.5,
    );
    assert!(compiled.live_scene.is_some());
}

#[test]
fn live_percentage_display_inherits_actor_frame_class_methods() {
    compile(
        "percentage-display-inheritance",
        r#"
return Def.ActorFrame {
    OnCommand = function()
        local screen = SCREENMAN:GetTopScreen()
        local first = screen:GetChild('ScoreP1'):GetChild('ScoreDisplayPercentage Percent')
        local second = screen:GetChild('ScoreP2'):GetChild('ScoreDisplayPercentage Percent')
        local native_count = ActorFrame.GetNumChildren
        local native_children = ActorFrame.GetChildren
        local counts, children = 0, 0
        ActorFrame.GetNumChildren = function(self)
            if self == first or self == second then counts = counts + 1 end
            return native_count(self)
        end
        ActorFrame.GetChildren = function(self)
            if self == first or self == second then children = children + 1 end
            return native_children(self)
        end
        for index, percent in ipairs{first, second} do
            assert(percent:GetNumChildren() == 1)
            assert(percent:GetChildren()['PercentP' .. index] == percent:GetChild('PercentP' .. index))
        end
        assert(counts == 2 and children == 2,
            'PercentageDisplay must inherit ActorFrame class method overrides')
        ActorFrame.GetNumChildren = native_count
        ActorFrame.GetChildren = native_children
        assert(first.SetUpdateFunction == ActorFrame.SetUpdateFunction)
        first:SetUpdateFunction(function() end)
        first:SetUpdateFunction(nil)
    end,
}
"#,
    );
}

#[test]
fn live_score_display_loads_unnamed_blank_frame_before_percent() {
    compile(
        "score-display-frame",
        r#"
return Def.ActorFrame {
    OnCommand = function()
        local screen = SCREENMAN:GetTopScreen()
        for _, player in ipairs{'P1', 'P2'} do
            local score = screen:GetChild('Score' .. player)
            assert(score:GetNumChildren() == 2, 'ScoreDisplayPercentage must include its blank frame')
            local frame = assert(score:GetChild(''))
            local percent = assert(score:GetChild('ScoreDisplayPercentage Percent'))
            assert(frame:GetName() == '' and frame:GetParent() == score)
            assert(percent:GetParent() == score)
            assert(score:GetChild('') == frame and score:GetChildren()[''] == frame)
            local texture = assert(frame:GetTexture(), 'score frame must load its inherited bitmap')
            assert(texture:GetPath() == 'Themes/_fallback/Graphics/_blank.png')
            assert(texture:GetSourceWidth() == 4 and texture:GetSourceHeight() == 2)
            assert(texture:GetImageWidth() == 8 and texture:GetImageHeight() == 8)
            assert(texture:GetTextureWidth() == 8 and texture:GetTextureHeight() == 8)
            assert(texture:GetNumFrames() == 1)
            assert(frame:GetWidth() == 4 and frame:GetHeight() == 2)
            assert(percent:GetChild('Percent' .. player):GetParent() == percent)
        end
    end,
}
"#,
    );
}

#[test]
fn live_oni_game_over_loads_inherited_blank_bitmap() {
    compile(
        "oni-game-over-bitmap",
        r#"
local sprite
local function check(texture)
    assert(texture, 'OniGameOver must load the inherited blank bitmap')
    assert(texture:GetPath() == 'Themes/_fallback/Graphics/_blank.png')
    assert(texture:GetSourceWidth() == 4 and texture:GetSourceHeight() == 2)
    assert(texture:GetImageWidth() == 8 and texture:GetImageHeight() == 8)
    assert(texture:GetTextureWidth() == 8 and texture:GetTextureHeight() == 8)
    assert(texture:GetNumFrames() == 1)
end
return Def.ActorFrame {
    OnCommand = function()
        local screen = SCREENMAN:GetTopScreen()
        local first = screen:GetChild('OniGameOverP1')
        local second = screen:GetChild('OniGameOverP2')
        assert(first:GetName() == 'OniGameOverP1' and first:GetParent() == screen)
        first:zoomto(320, 180)
        local retained = first:GetTexture()
        check(retained)
        check(second:GetTexture())
        assert(sprite:GetTexture() == nil, 'an unloaded Sprite must remain unloaded')
        sprite:SetTexture(retained)
        check(sprite:GetTexture())
        assert(sprite:GetWidth() == 4 and sprite:GetHeight() == 2)
        sprite:Load(nil)
        assert(sprite:GetTexture() == nil)
        second:Load(nil)
        assert(second:GetTexture() == nil)
        second:SetTexture(retained)
        check(second:GetTexture())
    end,
    Def.Sprite { InitCommand = function(self) sprite = self end },
}
"#,
    );
    let key = "Themes/_fallback/Graphics/_blank.png";
    let bitmap = deadlib_assets::generated_texture(key).expect("actual prepared blank bitmap");
    assert_eq!(bitmap.image.dimensions(), (8, 8));
    assert!(bitmap.image.pixels().all(|pixel| pixel.0 == [0, 0, 0, 0]));
    let source = deadlib_assets::texture_dims(key).expect("native bitmap source dimensions");
    assert_eq!((source.w, source.h), (4, 2));
}

#[test]
fn live_vertex_upload_copies_values_before_source_table_mutation() {
    let compiled = compile(
        "live-vertex-value-copy",
        r#"
return Def.ActorMultiVertex {
 InitCommand=function(self)
  local vertices = {
   {{1,2,-3},{0.25,0.5,0.75,1},{0.1,0.2}},
   {{4,5,-6},{1,1,1,1},{0,1}},
   {{7,8,-9},{1,1,1,1},{1,0}},
  }
  self:SetDrawState{Mode='DrawMode_Triangles'}:SetVertices(vertices)
  assert(self:GetNumVertices()==3)
  vertices[1][1][1]=999
  vertices[1][2][4]=0
  vertices[1][3][1]=999
 end,
}
"#,
    );
    let vertices = compiled
        .overlays
        .iter()
        .find_map(|actor| match &actor.kind {
            SongLuaOverlayKind::ActorMultiVertex { vertices, .. } => Some(vertices),
            _ => None,
        })
        .expect("uploaded mesh remains drawable");
    assert_eq!(vertices.len(), 3);
    assert_eq!(vertices[0].pos, [1.0, 2.0]);
    assert_eq!(vertices[0].z, -3.0);
    assert_eq!(vertices[0].color, [0.25, 0.5, 0.75, 1.0]);
    assert_eq!(vertices[0].uv, [0.1, 0.2]);
}

#[test]
fn live_quad_default_texture_survives_geometry_and_explicit_rebinding() {
    compile(
        "quad-default-texture",
        r#"
local quad, sprite
local function check(texture)
    assert(texture, 'Quad must load its native default texture')
    assert(texture:GetPath() == '', 'default texture resource has an empty filename')
    for _, method in ipairs{'GetSourceWidth', 'GetSourceHeight', 'GetTextureWidth',
        'GetTextureHeight', 'GetImageWidth', 'GetImageHeight', 'GetNumFrames'} do
        assert(texture[method](texture) == 1, method .. ' must describe the native 1x1 default')
    end
end
return Def.ActorFrame {
    OnCommand = function()
        quad:zoomto(320, 180)
        local original = quad:GetTexture()
        check(original)
        assert(sprite:GetTexture() == nil, 'an unloaded Sprite has no texture')
        sprite:SetTexture(original)
        check(sprite:GetTexture())
        quad:Load(nil)
        assert(quad:GetTexture() == nil, 'Load(nil) unloads the current Quad texture')
        quad:SetTexture(original)
        check(quad:GetTexture())
    end,
    Def.Quad { InitCommand = function(self) quad = self end },
    Def.Sprite { InitCommand = function(self) sprite = self end },
}
"#,
    );
    compile(
        "quad-ignores-node-texture",
        r#"
return Def.Quad {
    Texture = 'ignored 4x2.png',
    OnCommand = function(self)
        local texture = assert(self:GetTexture(), 'Quad default exists despite a Texture node attribute')
        assert(texture:GetPath() == '' and texture:GetNumFrames() == 1)
        assert(texture:GetSourceWidth() == 1 and texture:GetSourceHeight() == 1)
    end,
}
"#,
    );
}

#[test]
fn live_banner_uses_declared_artwork_and_native_fallback() {
    for (name, selected) in [("declared", true), ("fallback", false)] {
        let source = r#"
local song = GAMESTATE:GetCurrentSong()
local expected = $SELECTED
local function check(banner)
    local texture = assert(banner:GetTexture(), 'Banner must load a real selected or fallback texture')
    if expected then
        assert(texture:GetPath():find('chosen artwork.png', 1, true))
        assert(texture:GetSourceWidth() == 16 and texture:GetSourceHeight() == 8)
    else
        assert(texture:GetPath():find('Themes/_fallback/Graphics/_blank.png', 1, true))
        assert(texture:GetSourceWidth() == 4 and texture:GetSourceHeight() == 2)
    end
end
return Def.Banner {
    OnCommand=function(self)
        if expected then
            assert(song:HasBanner())
            assert(song:GetBannerPath():find('chosen artwork.png', 1, true),
                'Song banner API must use the parsed declared artwork')
        else assert(not song:HasBanner()) end
        self:SetScrolling(true, 0.25):LoadFromSong(song)
        check(self)
        assert(not self:GetScrolling())
        local pane = SCREENMAN:GetTopScreen():GetChild('Underlay'):GetChild('StepStatsPaneP1')
        check(pane:GetChild('BannerAndData'):GetChild('')[1])
        self:SetScrolling(true, 0.5):LoadFromSong(nil)
        local fallback = assert(self:GetTexture())
        assert(fallback:GetPath():find('Themes/_fallback/Graphics/_blank.png', 1, true))
        assert(fallback:GetSourceWidth() == 4 and fallback:GetSourceHeight() == 2)
        assert(not self:GetScrolling())
    end,
}
"#.replace("$SELECTED", if selected { "true" } else { "false" });
        let compiled =
            compile_with_context(&format!("declared-banner-{name}"), &source, |context| {
                context.screen_width = 854.0;
                if selected {
                    let art = context.song_dir.join("art");
                    fs::create_dir_all(&art).unwrap();
                    let declared = art.join("chosen artwork.png");
                    image::RgbaImage::from_pixel(16, 8, image::Rgba([11, 22, 33, 255]))
                        .save(&declared)
                        .unwrap();
                    image::RgbaImage::from_pixel(8, 8, image::Rgba([99, 88, 77, 255]))
                        .save(context.song_dir.join("banner.png"))
                        .unwrap();
                    context.song_banner_path = Some(declared);
                }
            });
        assert!(
            compiled.live_scene.is_some(),
            "banner case must retain its real live scene"
        );
    }
}

#[test]
fn live_queue() {
    let mut compiled = std::thread::spawn(|| {
        compile(
            "queue",
            r#"
local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local count = 0
return Def.ActorFrame {
    Name = 'Moving',
    OnCommand = function(self) self:x(0):linear(1):x(100):queuecommand('Later') end,
    LaterCommand = function(self)
        count = count + 1
        self:aux(stats:GetCurrentPossibleDancePoints())
        MESSAGEMAN:Broadcast('ActualTick', { count = count })
    end,
    UnsolicitedMessageCommand = function(self) error('speculative listener called') end,
}
"#,
        )
    })
    .join()
    .expect("worker transfers the initialized VM");
    let global = compiled.live_actor_indices()[compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Moving"))
        .expect("moving actor")];
    let mut scene = compiled.live_scene.take().expect("owned scene");
    assert_eq!(
        scene.states()[global].x,
        0.0,
        "startup must retain current pose, not destination"
    );
    assert!(scene.take_startup_output().errors.is_empty());
    let mut player = deadsync_gameplay::init_player_runtime();
    player.current_possible_points = 5;
    let mut frame = SongLuaLiveFrame {
        screen_seconds: 0.5,
        delta_seconds: 0.5,
        players: [
            SongLuaLivePlayer::from_player(&player, 10),
            SongLuaLivePlayer::default(),
        ],
        ..SongLuaLiveFrame::default()
    };
    let output = scene.advance(frame.clone()).expect("first half frame");
    assert!(output.errors.is_empty() && output.broadcasts.is_empty());
    assert!((scene.states()[global].x - 50.0).abs() < 1e-5);
    player.current_possible_points = 10;
    frame.players[0] = SongLuaLivePlayer::from_player(&player, 10);
    frame.screen_seconds = 1.0;
    let output = scene.advance(frame.clone()).expect("queue completion");
    assert!(output.errors.is_empty() && output.broadcasts.is_empty());
    assert_eq!(scene.states()[global].x, 100.0);
    // Actor.cpp::UpdateTweening stops when the positive delta is exhausted.
    // A trailing zero-time command runs on the next positive update.
    frame.screen_seconds = 1.1;
    frame.delta_seconds = 0.1;
    let output = scene.advance(frame.clone()).expect("trailing command");
    assert!(output.errors.is_empty());
    assert_eq!(output.broadcasts.len(), 1);
    assert_eq!(output.broadcasts[0].1, "ActualTick");
    assert_eq!(scene.states()[global].aux, 10.0);
    frame.screen_seconds = 1.2;
    assert!(
        scene
            .advance(frame.clone())
            .expect("later frame")
            .broadcasts
            .is_empty(),
        "drain events exactly once"
    );
}

#[test]
fn live_deprecated_hidden_preserves_lua_truthiness_and_reports_warning() {
    compile(
        "deprecated-hidden",
        r#"
local warnings = 0
return Def.ActorFrame { Name='hidden-check',
    ScriptErrorMessageCommand=function(self, params)
        assert(params.message == 'hidden is deprecated, use visible instead. (used on hidden-check)')
        warnings = warnings + 1
    end,
    OnCommand=function(self)
        assert(self:hidden(true) == nil and not self:GetVisible())
        assert(self:hidden(false) == nil and self:GetVisible())
        assert(self:hidden(0) == nil and not self:GetVisible())
        assert(self:hidden(nil) == nil and self:GetVisible())
        assert(warnings == 4)
    end,
}
"#,
    );
}

#[test]
fn live_source_broadcasts() {
    let mut compiled = compile(
        "broadcast-origin",
        r#"
            return Def.ActorFrame {
                OnCommand=function(self)
                    GAMESTATE:SetCurrentSteps(PLAYER_1, GAMESTATE:GetCurrentSteps(PLAYER_1))
                    MESSAGEMAN:Broadcast('CurrentStepsP1Changed')
                end,
                CurrentStepsP1ChangedMessageCommand=function(self)
                    MESSAGEMAN:Broadcast('NestedSource')
                end,
            }
        "#,
    );
    let output = compiled
        .live_scene
        .as_mut()
        .expect("owned scene")
        .take_startup_output();
    assert_eq!(
        output
            .source_broadcasts
            .iter()
            .map(|event| event.name.as_str())
            .collect::<Vec<_>>(),
        ["NestedSource", "CurrentStepsP1Changed", "NestedSource"]
    );
    assert_eq!(
        output
            .broadcasts
            .iter()
            .filter(|(_, name, _)| name == "CurrentStepsP1Changed")
            .count(),
        2
    );
    assert_eq!(
        output
            .broadcasts
            .iter()
            .filter(|(_, name, _)| name == "ScreenChanged")
            .count(),
        1
    );
}

#[test]
fn live_queued_broadcast_origin() {
    let mut compiled = compile(
        "queued-broadcast-origin",
        r#"
            return Def.ActorFrame {
                OnCommand=function(self) self:sleep(0.25):queuemessage('Queued') end,
                QueuedMessageCommand=function(self, params)
                    assert(type(params) == 'table' and next(params) == nil)
                    MESSAGEMAN:Broadcast('NestedSource', params)
                end,
            }
        "#,
    );
    let scene = compiled.live_scene.as_mut().expect("owned scene");
    assert!(scene.take_startup_output().source_broadcasts.is_empty());
    let output = scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.3125,
            delta_seconds: 0.3125,
            ..Default::default()
        })
        .expect("queued native message");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert_eq!(
        output
            .broadcasts
            .iter()
            .map(|(_, name, params)| (name.as_str(), *params))
            .collect::<Vec<_>>(),
        [("Queued", true), ("NestedSource", true)]
    );
    assert_eq!(
        output
            .source_broadcasts
            .iter()
            .map(|event| event.name.as_str())
            .collect::<Vec<_>>(),
        ["NestedSource"]
    );
    assert_eq!(
        output.source_broadcasts[0]
            .params
            .as_ref()
            .expect("empty listener table"),
        &Some(serde_json::json!([]))
    );
}

#[test]
fn live_sprite_cropto_alias() {
    let mut compiled = compile_with_context(
        "sprite-cropto-alias",
        r#"
return Def.ActorFrame {
    Def.Sprite {
        Texture='wide.png',
        OnCommand=function(self)
            self:xy(17,29)
            assert(self:cropto(100,100)==self, 'cropto returns its receiver')
            assert(self:GetWidth()==100 and self:GetHeight()==100 and self:GetZoomedWidth()==100)
            assert(self:GetX()==17 and self:GetY()==29)
            local original=Sprite.CropTo
            local marker={}; local calls=0
            Sprite.CropTo=function(receiver,w,h)
                assert(receiver==self and w==marker and h==false, 'cropto forwards original arguments')
                calls=calls+1
            end
            assert(self:cropto(marker,false)==self and calls==1, 'cropto uses current CropTo method')
            Sprite.CropTo=original
            MESSAGEMAN:Broadcast('CropAliasVerified', {kind='wide', width=self:GetWidth(), height=self:GetHeight()})
        end,
    },
    Def.Sprite {
        Texture='tall.png',
        OnCommand=function(self)
            assert(self:cropto(100,100)==self)
            assert(self:GetWidth()==100 and self:GetHeight()==100 and self:GetZoomedHeight()==100)
            MESSAGEMAN:Broadcast('CropAliasVerified', {kind='tall', width=self:GetWidth(), height=self:GetHeight()})
        end,
    },
}
"#,
        |context| {
            image::RgbaImage::new(200, 100)
                .save(context.song_dir.join("wide.png"))
                .unwrap();
            image::RgbaImage::new(100, 200)
                .save(context.song_dir.join("tall.png"))
                .unwrap();
        },
    );
    let output = compiled
        .live_scene
        .as_mut()
        .expect("owned scene")
        .take_startup_output();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert_eq!(output.source_broadcasts.len(), 2);
    for (event, kind) in output.source_broadcasts.iter().zip(["wide", "tall"]) {
        assert_eq!(event.name, "CropAliasVerified");
        assert_eq!(
            event.params.as_ref().unwrap(),
            &Some(serde_json::json!({"kind":kind,"width":100,"height":100}))
        );
    }
}

#[cfg(feature = "test-support")]
#[test]
fn live_native_raw_clocks_apply_to_constructors_and_same_frame_reads() {
    let mut compiled = compile_with_context(
        "raw-host-clock",
        "local started=GetTimeSinceStart()\nreturn Def.Quad{OnCommand=function(self)\n self:x(GetTimeSinceStart()-started):y(Minute()*60+Second())\nend}",
        |context| {
            context.host_clock_inputs = [
                ("GetTimeSinceStart", 1, 200.25),
                ("GetTimeSinceStart", 3, 200.75),
                ("Minute", 3, 17.0),
                ("Second", 3, 33.0),
            ]
            .into_iter()
            .map(|(clock, line, value)| song::SongLuaHostClockInput {
                clock: clock.into(),
                source: "song:/default.lua".into(),
                line,
                value,
            })
            .collect();
        },
    );
    let mut scene = compiled.live_scene.take().unwrap();
    let output = scene.advance(SongLuaLiveFrame::default()).unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert!(
        scene
            .states()
            .iter()
            .any(|state| state.x == 0.5 && state.y == 1053.0),
        "constructor uptime and each raw clock read must be independent of the frame snapshot"
    );
}

#[test]
fn live_native_broadcast_snapshot_preserves_explicit_nil_argument() {
    let mut compiled = compile(
        "native-broadcast-arity",
        r#"
return Def.Actor { OnCommand = function()
    MESSAGEMAN:Broadcast('NoArgument')
    MESSAGEMAN:Broadcast('ExplicitNil', nil)
    MESSAGEMAN:Broadcast('TableArgument', {value=7})
end }
"#,
    );
    let output = compiled.live_scene.as_mut().unwrap().take_startup_output();
    let events = output.source_broadcasts;
    assert_eq!(
        events
            .iter()
            .map(|event| event.name.as_str())
            .collect::<Vec<_>>(),
        ["NoArgument", "ExplicitNil", "TableArgument"]
    );
    assert_eq!(events[0].params.as_ref().unwrap(), &None);
    assert_eq!(
        events[1].params.as_ref().unwrap(),
        &Some(serde_json::json!({"type":"nil"}))
    );
    assert_eq!(
        events[2].params.as_ref().unwrap(),
        &Some(serde_json::json!({"value":7}))
    );
}

#[test]
fn live_requests() {
    let mut compiled = compile(
        "transition",
        r#"
return Def.ActorFrame {
    OffCommand = function(self) self:aux(17) end,
    OnCommand = function(self)
        self:SetUpdateFunction(function(self, delta)
            if delta > 0 then
                SCREENMAN:GetTopScreen():StartTransitioningScreen('SM_DoNextScreen')
                assert(self:getaux() == 17, 'OffCommand must run immediately')
                error('live diagnostic')
            end
        end)
    end,
}
"#,
    );
    let mut scene = compiled.live_scene.take().expect("scene");
    let frame = SongLuaLiveFrame {
        screen_seconds: 0.1,
        delta_seconds: 0.1,
        ..SongLuaLiveFrame::default()
    };
    let output = scene
        .advance(frame.clone())
        .expect("native partial callback");
    assert_eq!(
        output.request,
        Some(SongLuaLiveRequest::Transition("SM_DoNextScreen".into()))
    );
    assert_eq!(output.errors.len(), 1);
    assert!(
        output.errors[0].contains("live diagnostic"),
        "{:?}",
        output.errors
    );
    assert!(
        scene
            .advance(frame.clone())
            .expect("next frame")
            .errors
            .is_empty(),
        "native error is reported once"
    );
}

#[test]
fn live_native_nonfinite_actor_scalar_write_and_immediate_read() {
    let mut compiled = compile_with_context(
        "native-nonfinite-actor-scalars",
        r#"
local later
return Def.ActorFrame {
    Def.ActorFrame {
        Name='Writer',
        OnCommand=function(self)
            later=self:GetParent():GetChild('Later')
            self:SetUpdateFunction(function()
                -- The original outCirc expression at t=128-122.5, d=2.5
                -- has a negative radicand. Native Actor::SetX preserves it.
                local t=(128-122.5)/2.5-1
                local value=9*math.sqrt(1-math.pow(t,2))+1
                assert(value~=value)
                later:x(value)
                local x=later:GetX()
                assert(x~=x, 'native actor scalar setters must retain NaN, including a later actor in the same update')
                later:zoom(1/x)
                local zoom=later:GetZoom()
                assert(zoom~=zoom, 'native zoom must retain a numeric NaN')
                later:x(math.huge)
                assert(later:GetX()==math.huge, 'native actor float property retains infinity')
                later:x(-math.huge)
                assert(later:GetX()==-math.huge)
            end)
        end,
    },
    Def.Actor {Name='Later',InitCommand=function(self) self:x(7) end},
}
"#,
        |context| {
            context.defer_initial_live_update = true;
        },
    );
    let mut scene = compiled.live_scene.take().expect("native scalar scene");
    let output = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.1,
            beat: 0.2,
            screen_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .expect("native nonfinite scalar update");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
}

fn scene_exec(
    scene: &SongLuaLiveScene,
    source: impl AsRef<[u8]>,
) -> Result<(), deadlib_lua::Error> {
    let code = deadlib_lua::compile_named(source.as_ref(), b"=existing live unit fixture")?;
    scene.runtime().vm_mut().execute(&code)?;
    Ok(())
}

#[test]
fn live_untyped_foreground_uses_native_dummy_actor() {
    let mut compiled = compile_with_context(
        "untyped-foreground",
        "CONSTRUCTED = true; return {}",
        |context| {
            context.layer_start_beats = vec![0.0];
            context.foreground_names = vec!["fallback".into()];
        },
    );
    let mut scene = compiled.live_scene.take().expect("native dummy foreground");
    let output = scene
        .advance(SongLuaLiveFrame {
            beat: 0.1,
            music_seconds: 0.1,
            screen_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    scene_exec(
        &scene,
        r#"
assert(CONSTRUCTED, 'construction side effects survive the fallback')
local foreground = SCREENMAN:GetTopScreen():GetChild('SongForeground'):GetChild('fallback')
assert(foreground and foreground:GetName() == 'fallback')
assert(foreground:GetParent():GetName() == 'SongForeground')
foreground:visible(true)
assert(foreground:GetVisible())
"#,
    )
    .expect("foreground owns a real dummy Actor");
}

#[test]
fn live_untyped_declaration_discards_fields_commands_and_children() {
    let mut compiled = compile_with_context(
        "untyped-declaration",
        r#"
local untyped = {
    Name = 'Ignored',
    InitCommand = function() error('untyped Init must not run') end,
    OnCommand = function() error('untyped On must not run') end,
    Def.Actor { InitCommand = function() error('untyped children must not load') end },
}
return Def.ActorFrame {
    InitCommand = function(self) ROOT = self end,
    untyped,
}
"#,
        |_| {},
    );
    let mut scene = compiled.live_scene.take().expect("native dummy child");
    let output = scene
        .advance(SongLuaLiveFrame {
            beat: 0.1,
            music_seconds: 0.1,
            screen_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    scene_exec(
        &scene,
        r#"
assert(ROOT:GetChild('Ignored') == nil, 'invalid declarations do not load Name')
local child = ROOT:GetChild('')
assert(child and child:GetName() == '')
child:visible(false)
assert(not child:GetVisible())
"#,
    )
    .expect("invalid declarations load only the native dummy Actor");
}

#[test]
fn live_arrow_alpha_uses_visibility_and_native_far_edge_fade() {
    let mut compiled = compile_with_context(
        "native-arrow-alpha",
        "return Def.Actor {InitCommand = function(self) ALPHA_ACTOR = self end}",
        |context| context.defer_initial_live_update = true,
    );
    let mut scene = compiled.live_scene.take().unwrap();
    scene
        .advance(SongLuaLiveFrame {
            music_seconds: 2.0,
            screen_seconds: 2.0,
            beat: 4.0,
            delta_seconds: 0.01,
            ..Default::default()
        })
        .unwrap();
    scene_exec(
        &scene,
        r#"
local ps = GAMESTATE:GetPlayerState(PLAYER_1)
local o = ps:GetCurrentPlayerOptions()
local alpha = ArrowEffects.GetAlpha
assert(alpha(ps, 1, 720) == 1)
assert(alpha(ps, 1, 721) == -math.huge, 'native far-edge fade retains negative infinity')
assert(math.abs(alpha(ps, 1, 600, nil, nil, 1000, 0.5) - 0.8) < 0.000001)
assert(alpha(ps, 1, 1000, nil, nil, 1000, 0.5) == 0)
assert(alpha(ps, 1, 1250, nil, nil, 1000, 0.5) == -0.5)
assert(alpha(ps, 1, 100, 0.5) == 0 and alpha(ps, 1, 100, 0.499) == 1)
o:Reverse(1)
assert(alpha(ps, 1, 721) == -math.huge, 'alpha uses position without reverse')
o:Stealth(0.6)
assert(alpha(ps, 1, 100) == 0)
assert(alpha(ps, 1, -1) == 1, 'past receptors remain visible by default')
o:StealthPastReceptors(true)
assert(alpha(ps, 1, -1) == 0)
assert(alpha(ps, 1, 100, 0) == 1, 'fail fade overrides appearance visibility')
o:Stealth(0)
o:Stealth1(0.6)
assert(alpha(ps, 1, 100) == 0 and alpha(ps, 2, 100) == 1)
o:Stealth1(0)
o:Hidden(1)
assert(alpha(ps, 1, 100) == 0 and alpha(ps, 1, 161) == 1)
o:Hidden(0)
o:Sudden(1)
assert(alpha(ps, 1, 201) == 0 and alpha(ps, 1, 159) == 1)
o:Sudden(0)
o:Mini(1)
o:Hidden(1)
assert(alpha(ps, 1, 250) == 0 and alpha(ps, 1, 321) == 1,
    'mini scales the appearance center line')
local actor = ALPHA_ACTOR
actor:diffusealpha(alpha(ps, 1, 721))
assert(actor:GetDiffuseAlpha() == -math.huge, 'the raw alpha reaches the real actor')
"#,
    )
    .expect("native alpha visibility and far-edge fade");
}

#[test]
fn live_numeric_song_selectors_retain_chart_across_reload() {
    let mut compiled = compile_with_context(
        "numeric-song-selector-reload",
        r#"
NUMERIC_SCENES = (NUMERIC_SCENES or 0) + 1
local scene = NUMERIC_SCENES
local names = {'Beginner','Easy','Medium','Hard','Challenge','Edit'}
return Def.Actor { OnCommand = function(self)
    local song = GAMESTATE:GetCurrentSong()
    for code = 0, 5 do
        local step = song:GetOneSteps(0, code)
        assert(step ~= nil, 'numeric GetOneSteps must select a real chart')
        assert(step:GetDifficulty() == 'Difficulty_' .. names[code + 1])
    end
    assert(#song:GetStepsByStepsType(0) == 6 and song:HasStepsType(0))
    assert(song:HasStepsTypeAndDifficulty(0, 2) and song:HasEdits(0))
    assert(song:GetOneSteps(6, 2):GetStepsType() == 'StepsType_Pump_Single')
    if scene == 1 then
        NUMERIC_CHOSEN = song:GetOneSteps(0.0, 2.0)
        GAMESTATE:SetCurrentSteps(0, NUMERIC_CHOSEN)
        assert(GAMESTATE:GetCurrentSteps(0) == NUMERIC_CHOSEN)
        SCREENMAN:SetNewScreen('ScreenGameplay')
    else
        assert(GAMESTATE:GetCurrentSteps(0):GetDifficulty() == 'Difficulty_Medium')
        assert(GAMESTATE:GetCurrentSteps(1):GetDifficulty() == 'Difficulty_Medium')
    end
    self:sleep(9999)
end }
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
            context.players[0].enabled = true;
            context.players[1].enabled = false;
            context.song_steps = [
                SongLuaDifficulty::Beginner,
                SongLuaDifficulty::Easy,
                SongLuaDifficulty::Medium,
                SongLuaDifficulty::Hard,
                SongLuaDifficulty::Challenge,
                SongLuaDifficulty::Edit,
                SongLuaDifficulty::Medium,
            ]
            .into_iter()
            .enumerate()
            .map(|(index, difficulty)| song::SongLuaStep {
                steps_type: if index == 6 {
                    "StepsType_Pump_Single"
                } else {
                    "StepsType_Dance_Single"
                }
                .into(),
                difficulty,
                description: format!("{difficulty:?}"),
                chart_name: String::new(),
                credit: String::new(),
                meter: 9,
                music_file: String::new(),
                possible_grade_points: 0,
            })
            .collect();
            context.player_step_indices = [Some(4), Some(4)];
        },
    );
    let mut scene = compiled
        .live_scene
        .take()
        .expect("initial numeric selector scene");
    let output = scene.advance(SongLuaLiveFrame::default()).unwrap();
    assert!(
        output.errors.is_empty(),
        "numeric selector callback: {:?}",
        output.errors
    );
    assert_eq!(
        output.request,
        Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into()))
    );
    assert_eq!(
        scene.selected_context().unwrap().player_step_indices,
        [Some(2), Some(4)]
    );
    let mut layers = song::reload_song_lua(
        scene,
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None::<song::SongLuaOverlayModelLayer<()>>,
    )
    .unwrap();
    let mut replacement = layers[0].live_scene.take().unwrap();
    assert_eq!(
        replacement.selected_context().unwrap().player_step_indices,
        [Some(2), Some(2)]
    );
    let output = replacement.advance(SongLuaLiveFrame::default()).unwrap();
    assert!(
        output.errors.is_empty(),
        "replacement callback: {:?}",
        output.errors
    );
    assert_eq!(output.request, None);
}

#[test]
fn live_loadactor_uses_helper_source_during_actor_callbacks() {
    let dir =
        std::env::temp_dir().join(format!("deadsync-live-helper-paths-{}", std::process::id()));
    fs::create_dir_all(dir.join("parts")).unwrap();
    fs::write(
        dir.join("value.lua"),
        "error('actor directory must not shadow the calling helper')",
    )
    .unwrap();
    fs::write(
        dir.join("parts/value.lua"),
        "local a, b, c = ...; assert(b == nil); return a + c",
    )
    .unwrap();
    fs::write(
        dir.join("parts/helper.lua"),
        r#"
return function()
    return LoadActor('value', 23, nil, 5)
end
"#,
    )
    .unwrap();
    let entry = dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local helper = LoadActor('parts/helper')
return Def.ActorFrame {
    OnCommand = function(self)
        self:aux(helper())
        self:SetUpdateFunction(function(self) self:aux(helper() + 1) end)
    end,
}
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&dir, "callback relative actor");
    context.live = true;
    let mut compiled: Compiled = song::compile_song_lua(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("load helper closure");
    let mut scene = compiled.live_scene.take().expect("retained Lua scene");
    let startup = scene.take_startup_output();
    assert!(startup.errors.is_empty(), "{:?}", startup.errors);
    let output = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.01,
            beat: 0.01,
            screen_seconds: 0.01,
            delta_seconds: 0.01,
            ..Default::default()
        })
        .unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert!(
        scene.states().iter().any(|state| state.aux == 29.0),
        "On and update callbacks load relative to the invoked helper, preserving varargs"
    );
}

#[test]
fn live_chart_attacks_use_strict_boundaries_and_keep_song_writes_between_them() {
    let mut compiled = compile_with_context(
        "attack-boundaries",
        r#"
return Def.ActorFrame {
    OnCommand = function(self)
        self:sleep(9999)
        STATE = GAMESTATE:GetPlayerState(PLAYER_1)
        STATE:SetPlayerOptions('ModsLevel_Stage', '25% Dark')
        STATE:SetPlayerOptions('ModsLevel_Song', '75% Drunk')
        OPTIONS = STATE:GetPlayerOptions('ModsLevel_Song')
    end,
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.players[0].enabled = true;
            context.players[1].enabled = false;
        },
    );
    let mut scene = compiled.live_scene.take().unwrap();
    let windows: Arc<[deadsync_gameplay::ChartAttackWindow]> = Arc::from([
        deadsync_gameplay::ChartAttackWindow {
            start_second: 2.0,
            len_seconds: 1.0,
            mods: "Overhead, Onlyonecouples".into(),
        },
        deadsync_gameplay::ChartAttackWindow {
            start_second: 2.5,
            len_seconds: 1.5,
            mods: "25% Drunk".into(),
        },
    ]);
    let schedule = [windows, Arc::from([])];
    let mut previous = 0.0;
    for (seconds, drunk) in [
        (0.0, 0.75),
        (2.0, 0.75),
        (2.25, 0.0),
        (2.5, 0.5),
        (2.75, 0.25),
        (3.0, 0.25),
        (4.0, 0.0),
    ] {
        // Native attack activation reads the preceding ScreenGameplay
        // position. Prime that input while keeping all boundary assertions.
        scene
            .advance(SongLuaLiveFrame {
                music_seconds: seconds,
                screen_seconds: seconds,
                beat: seconds * 2.0,
                ..Default::default()
            })
            .unwrap();
        let output = scene
            .advance(SongLuaLiveFrame {
                music_seconds: seconds,
                screen_seconds: seconds,
                beat: seconds * 2.0,
                delta_seconds: seconds - previous,
                chart_attacks: Some(schedule.clone()),
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        previous = seconds;
        scene_exec(
            &scene,
            format!(
                r#"
assert(STATE:GetPlayerOptions('ModsLevel_Song') == OPTIONS)
assert(OPTIONS:Drunk() == {drunk}, 'attack targets at {seconds}')
"#
            ),
        )
        .unwrap();
        if seconds == 2.25 {
            scene_exec(&scene, "OPTIONS:Drunk(0.5)").unwrap();
        }
    }
    scene_exec(&scene, "assert(OPTIONS:Dark() == 0.25)").unwrap();
}

#[test]
fn live_relative_and_canonical_nested_media_keep_hidden_sprite_slots() {
    let dir = std::path::PathBuf::from(format!(
        "../../target/song-lua-relative-media-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(dir.join("nested")).unwrap();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255; 4]))
        .save(dir.join("nested/ball.png"))
        .unwrap();
    std::fs::write(
        dir.join("entry.lua"),
        "return LoadActor('nested/actor.lua')",
    )
    .unwrap();
    std::fs::write(dir.join("nested/actor.lua"), "return LoadActor('ball.png') .. {Name='HiddenBall',InitCommand=function(self) self:visible(false) end}").unwrap();
    // Windows catalog scanning canonicalizes roots to \\?\ paths. Exercise
    // both that shipping spelling and a process-relative song root.
    let canonical = std::fs::canonicalize(&dir).unwrap();
    for root in [&dir, &canonical] {
        let mut context = song::SongLuaCompileContext::new(root, "relative media");
        context.live = true;
        let compiled: Compiled = song::compile_song_lua(
            &root.join("entry.lua"),
            &context,
            SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None,
            |_, _| None,
        )
        .unwrap();
        let sprite = compiled
            .overlays
            .iter()
            .find(|actor| actor.name.as_deref() == Some("HiddenBall"))
            .expect("nested hidden sprite retains a visual slot");
        assert!(
            matches!(&sprite.kind, SongLuaOverlayKind::Sprite { texture_path, .. }
                if texture_path.is_absolute() && texture_path.is_file())
        );
    }
}

#[test]
fn live_amv_load_texture_preserves_mesh_and_restores_native_default() {
    let compiled = compile_with_context(
        "amv-load-texture",
        r#"
return Def.ActorMultiVertex {
    InitCommand = function(self)
        MESH = self
        self:SetDrawState{Mode='DrawMode_Triangles'}:SetVertices{
            {{0,0,0},{1,1,1,1},{0,0}},
            {{20,0,0},{1,1,1,1},{1,0}},
            {{0,10,0},{1,1,1,1},{0,1}},
        }
        assert(self:LoadTexture(GAMESTATE:GetCurrentSong():GetSongDir() .. 'panel.png') == self)
        local texture = self:GetTexture()
        assert(texture:GetSourceWidth() == 16 and texture:GetSourceHeight() == 8)
        assert(self:GetNumVertices() == 3 and self:GetDestDrawMode() == 'DrawMode_Triangles')
    end,
}
"#,
        |context| {
            image::RgbaImage::new(16, 8)
                .save(context.song_dir.join("panel.png"))
                .unwrap();
        },
    );
    let SongLuaOverlayKind::ActorMultiVertex {
        vertices,
        texture_path: Some(path),
        ..
    } = &compiled.overlays[0].kind
    else {
        panic!("textured authored mesh")
    };
    assert_eq!(path.file_name().unwrap(), "panel.png");
    assert_eq!(vertices.len(), 3);
    assert_eq!(vertices[1].pos, [20.0, 0.0]);
    assert_eq!(vertices[1].uv, [1.0, 0.0]);
    let scene = compiled.live_scene.expect("live mesh scene");
    scene_exec(
        &scene,
        r#"
local retained = MESH:GetTexture()
assert(MESH:LoadTexture(nil) == MESH)
local default = MESH:GetTexture()
assert(default:GetPath() == '' and default:GetSourceWidth() == 1 and default:GetNumFrames() == 1)
assert(retained:GetSourceWidth() == 16, 'replacement preserves retained texture handles')
assert(MESH:GetNumVertices() == 3 and MESH:GetDestDrawMode() == 'DrawMode_Triangles')
MESH:SetTexture(retained)
assert(MESH:GetTexture() == retained)
MESH:LoadTexture()
assert(MESH:GetTexture():GetSourceWidth() == 1, 'missing argument also loads the default')
"#,
    )
    .expect("native LoadTexture reset and mesh preservation");
}

#[test]
fn live_song_meter_owns_native_song_info_children_and_lifecycle() {
    for (name, width, height) in [("wide", 854.0, 480.0), ("standard", 640.0, 480.0)] {
        let mut compiled = compile_with_context(
            &format!("song-meter-{name}"),
            "return Def.ActorFrame {}",
            |context| {
                context.main_title = "Native Song Info".into();
                context.screen_width = width;
                context.screen_height = height;
                context.defer_initial_live_update = true;
            },
        );
        let scene = compiled
            .live_scene
            .as_mut()
            .expect("retained SongMeter scene");
        scene_exec(&scene,r#"
local underlay=SCREENMAN:GetTopScreen():GetChild('Underlay')
local meter=underlay:GetChild('SongMeter')
assert(meter and meter:GetNumChildren()==4, 'native SongInfoBar must own four direct children')
assert(meter:GetParent()==underlay)
local anonymous=meter:GetChild('')
assert(#anonymous==3)
for i=1,2 do
    assert(anonymous[i]:GetParent()==meter and anonymous[i]:GetTexture(), 'border Quad owns native default texture')
    assert(anonymous[i]:GetTexture():GetSourceWidth()==1 and anonymous[i]:GetTexture():GetSourceHeight()==1)
end
assert(tostring(anonymous[3]):find('SongMeterDisplay',1,true))
assert(anonymous[3]:GetParent()==meter)
local title=meter:GetChild('SongTitle')
assert(title and title:GetParent()==meter and title:GetText()=='Native Song Info')
title:settext('stale')
MESSAGEMAN:Broadcast('CurrentSongChanged')
assert(title:GetText()=='Native Song Info', 'normal song change updates the real native title actor')
"#).unwrap();
    }
}

#[test]
fn live_steps_density_uses_real_note_rows_and_chart_timing() {
    use deadsync_rules::timing::{
        FakeSegment, StopSegment, TimeSignatureSegment, TimingData, TimingSegments,
    };
    let mut compiled = compile_with_context(
        "steps-native-density",
        "return Def.ActorFrame {}",
        |context| {
            let timings = [
                TimingData::from_segments(
                    0.0,
                    0.0,
                    &TimingSegments {
                        bpms: vec![(0.0, 120.0), (4.0, 240.0)],
                        stops: vec![StopSegment {
                            beat: 6.0,
                            duration: 0.5,
                        }],
                        fakes: vec![FakeSegment {
                            beat: 1.0,
                            length: 1.0,
                        }],
                        ..Default::default()
                    },
                    &[],
                ),
                TimingData::from_segments(
                    0.0,
                    0.0,
                    &TimingSegments {
                        bpms: vec![(0.0, 120.0)],
                        time_signatures: vec![TimeSignatureSegment {
                            beat: 0.0,
                            numerator: 3,
                            denominator: 4,
                        }],
                        ..Default::default()
                    },
                    &[],
                ),
            ];
            context.player_step_indices = [Some(0), Some(1)];
            context.song_steps = (0..2)
                .map(|index| song::SongLuaStep {
                    steps_type: "StepsType_Dance_Single".into(),
                    difficulty: SongLuaDifficulty::Hard,
                    description: format!("density{index}"),
                    chart_name: String::new(),
                    credit: String::new(),
                    meter: 12,
                    music_file: String::new(),
                    possible_grade_points: 100,
                })
                .collect();
            context.song_step_timing = timings.to_vec();
            context.player_timing = [Some(timings[0].clone()), Some(timings[1].clone())];
            context.song_step_notes = vec![
                std::sync::Arc::from(
                    &b"1100\n1000\n2000\n3000\nM000\n4000\n3000\nL000\nF000\n1000\n"[..],
                ),
                std::sync::Arc::from(&b"1000\n0100\n1000\n0100\n1000\n0100\n"[..]),
            ];
            context.song_step_row_beats = vec![
                std::sync::Arc::from(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0][..]),
                std::sync::Arc::from(&[0.0, 2.0, 3.0, 5.0, 6.0, 8.0][..]),
            ];
        },
    );
    let scene = compiled.live_scene.take().unwrap();
    scene_exec(
        &scene,
        r#"
local charts = GAMESTATE:GetCurrentSong():GetAllSteps()
local a, b = charts[1], charts[2]
assert(type(a.GetNotesPerMeasure) == 'function' and type(a.GetNpsPerMeasure) == 'function'
    and type(a.GetPeakNps) == 'function', 'Steps must expose real cached measure density')
local count, nps = a:GetNotesPerMeasure(), a:GetNpsPerMeasure()
assert(#count == 3 and count[1] == 2 and count[2] == 1 and count[3] == 1)
assert(nps[1] == 1 and math.abs(nps[2] - 2/3) < 0.000001 and nps[3] == 1)
assert(a:GetPeakNps() == 1 and a:GetPeakNps(PLAYER_2) == 1)
count[1], nps[1] = 999, 999
assert(a:GetNotesPerMeasure()[1] == 2 and a:GetNpsPerMeasure()[1] == 1)
local other = b:GetNotesPerMeasure()
assert(#other == 3 and other[1] == 2 and other[2] == 2 and other[3] == 2)
assert(b:GetPeakNps() == 1)
ParseChartInfo(a, 'P1')
ParseChartInfo(b, 'P2')
assert(SL.P1.Streams.PeakNPS == 1 and SL.P1.Streams.NotesPerMeasure[1] == 2)
assert(SL.P2.Streams.PeakNPS == 1 and SL.P2.Streams.NotesPerMeasure[2] == 2)
SL.P1.Streams.NotesPerMeasure[1] = 999
assert(a:GetNotesPerMeasure()[1] == 2)
GAMESTATE:GetSongOptionsObject('ModsLevel_Song'):MusicRate(2)
assert(a:GetNpsPerMeasure()[1] == 1, 'cached density is independent of playback rate')
local ok = pcall(function() a:GetPeakNps('not a player') end)
assert(not ok)
"#,
    )
    .unwrap();
}

#[test]
fn live_foreground_clock() {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-live-foreground-clock-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("test directory");
    let early = dir.join("early.lua");
    let late = dir.join("late.lua");
    fs::write(
        dir.join("clock.lua"),
        r#"
local label = ...
return Def.ActorFrame {
    Name = label,
    InitCommand = function() MESSAGEMAN:Broadcast('Construct:' .. label) end,
    OnCommand = function(self) self:sleep(999); MESSAGEMAN:Broadcast('On:' .. label) end,
    Def.Actor {
        OnCommand = function(self) self:sleep(0.25):queuecommand('Later') end,
        LaterCommand = function() MESSAGEMAN:Broadcast('Later:' .. label) end,
    },
}
"#,
    )
    .expect("clock script");
    fs::write(&early, "return LoadActor('clock.lua', 'early')").expect("early entry");
    fs::write(&late, "return LoadActor('clock.lua', 'late')").expect("late entry");
    let mut context = SongLuaCompileContext::new(&dir, "foreground clock");
    context.live = true;
    context.song_display_bpms = [120.0; 2];
    context.layer_start_beats = vec![2.0, 4.0];
    let mut compiled: Vec<Compiled> = song::compile_song_lua_layers(
        &[&early, &late],
        0,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("scheduled preparation");
    let mut scene = compiled[0]
        .live_scene
        .take()
        .expect("owned scheduled scene");
    let startup = scene.take_startup_output();
    let names = startup
        .broadcasts
        .iter()
        .map(|(_, name, _)| name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"Construct:early") && names.contains(&"Construct:late"));
    assert!(
        !names
            .iter()
            .any(|name| name.starts_with("On:") || name.starts_with("Later:"))
    );
    for (index, (music_seconds, expected)) in [
        (0.75, None),
        (1.0, Some("On:early")),
        (1.0, None),
        (1.125, None),
        (1.25, None),
        (1.375, Some("Later:early")),
        (2.0, Some("On:late")),
    ]
    .into_iter()
    .enumerate()
    {
        let output = scene
            .advance(SongLuaLiveFrame {
                music_seconds,
                beat: music_seconds * 2.0,
                screen_seconds: (index + 1) as f64 * 0.5,
                delta_seconds: 0.5,
                ..Default::default()
            })
            .expect("scheduled frame");
        assert!(output.errors.is_empty());
        let names = output
            .broadcasts
            .iter()
            .map(|(_, name, _)| name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            expected.into_iter().collect::<Vec<_>>(),
            "music second {music_seconds}"
        );
    }
}

#[test]
fn live_background_starts_at_change_beat() {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-live-background-start-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("test directory");
    let entry = dir.join("background.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame {
    OnCommand = function(self)
        MESSAGEMAN:Broadcast('On:' .. tostring(GAMESTATE:GetCurMusicSeconds()))
        self:SetUpdateFunction(function(self, delta)
            MESSAGEMAN:Broadcast('Delta:' .. tostring(delta))
        end)
    end,
    GainFocusCommand = function() MESSAGEMAN:Broadcast('GainFocus') end,
    Def.Actor {
        OnCommand = function(self) self:sleep(0.1):queuecommand('Later') end,
        LaterCommand = function() MESSAGEMAN:Broadcast('Later') end,
    },
}
"#,
    )
    .expect("background script");
    let mut context = SongLuaCompileContext::new(&dir, "background start");
    context.live = true;
    context.song_display_bpms = [120.0; 2];
    context.background_layer_count = 1;
    context.layer_start_beats = vec![2.0];
    let mut compiled: Compiled = song::compile_song_lua(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("background preparation");
    let mut scene = compiled.live_scene.take().expect("owned scene");
    assert!(
        scene
            .take_startup_output()
            .broadcasts
            .iter()
            .all(|(_, name, _)| name == "ScreenChanged")
    );
    let dormant = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.75,
            beat: 1.5,
            screen_seconds: 4.5,
            delta_seconds: 0.5,
            ..Default::default()
        })
        .expect("dormant frame");
    assert!(dormant.broadcasts.is_empty());
    for (music_seconds, expected) in [
        (1.125, vec!["On:1.125", "GainFocus", "Later", "Delta:0.125"]),
        (1.25, vec!["Delta:0.125"]),
    ] {
        let output = scene
            .advance(SongLuaLiveFrame {
                music_seconds,
                beat: music_seconds * 2.0,
                screen_seconds: 5.0,
                delta_seconds: 0.5,
                ..Default::default()
            })
            .expect("background frame");
        assert!(output.errors.is_empty());
        assert_eq!(
            output
                .broadcasts
                .iter()
                .map(|(_, name, _)| name.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn live_foreground_late_start() {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-live-foreground-late-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("test directory");
    let entry = dir.join("late.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame {
    OnCommand = function(self)
        self:sleep(999):SetUpdateFunction(function(self, delta)
            MESSAGEMAN:Broadcast('Delta:' .. tostring(delta))
        end)
    end,
    Def.Actor {
        OnCommand = function(self) self:sleep(0.1):queuecommand('Later') end,
        LaterCommand = function() MESSAGEMAN:Broadcast('Later') end,
    },
}
"#,
    )
    .expect("late script");
    let mut context = SongLuaCompileContext::new(&dir, "foreground late");
    context.live = true;
    context.song_display_bpms = [120.0; 2];
    context.layer_start_beats = vec![2.0];
    let mut compiled: Compiled = song::compile_song_lua(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("late preparation");
    let mut scene = compiled.live_scene.take().expect("owned scene");
    let output = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 1.125,
            beat: 2.25,
            screen_seconds: 5.0,
            delta_seconds: 0.5,
            ..Default::default()
        })
        .expect("late frame");
    assert!(output.errors.is_empty());
    assert_eq!(
        output
            .broadcasts
            .iter()
            .map(|(_, name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["Later", "Delta:0.125"]
    );
}

#[test]
fn live_foreground_finishes() {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-live-foreground-finish-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("test directory");
    let entry = dir.join("finish.lua");
    fs::write(
        &entry,
        r#"
local count = 0
return Def.ActorFrame {
    Name = 'Finish',
    InitCommand = function(self) self:SetUpdateFunction(function(self)
        count = count + 1; MESSAGEMAN:Broadcast('ForegroundTick' .. count)
    end) end,
    OnCommand = function(self) self:sleep(0.25) end,
}
"#,
    )
    .expect("finish script");
    let mut context = SongLuaCompileContext::new(&dir, "foreground finish");
    context.live = true;
    context.song_display_bpms = [120.0; 2];
    context.layer_start_beats = vec![2.0];
    let mut compiled: Compiled = song::compile_song_lua(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("finish preparation");
    let global = compiled.live_actor_indices()[compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Finish"))
        .expect("finish actor")];
    let mut scene = compiled.live_scene.take().expect("owned scene");
    assert!(!scene.states()[global].visible);
    assert!(
        scene
            .take_startup_output()
            .broadcasts
            .iter()
            .all(|(_, name, _)| !name.starts_with("ForegroundTick")),
        "dormant callbacks do not run in preparation"
    );
    for (index, (music_seconds, tick, visible)) in [
        (0.75, None, false),
        (1.0, Some("ForegroundTick1"), true),
        (1.5, Some("ForegroundTick2"), false),
        (1.75, None, false),
        (1.0, None, false),
    ]
    .into_iter()
    .enumerate()
    {
        let output = scene
            .advance(SongLuaLiveFrame {
                music_seconds,
                beat: music_seconds * 2.0,
                screen_seconds: (index + 1) as f64 * 0.5,
                delta_seconds: 0.5,
                ..Default::default()
            })
            .expect("finish frame");
        assert!(output.errors.is_empty());
        assert_eq!(
            output
                .broadcasts
                .iter()
                .map(|(_, name, _)| name.as_str())
                .collect::<Vec<_>>(),
            tick.into_iter().collect::<Vec<_>>()
        );
        assert_eq!(scene.states()[global].visible, visible);
    }
}

#[test]
fn live_end_output() {
    let mut compiled = compile(
        "end-output",
        r#"
            SCENES=(SCENES or 0)+1
            local count=SCENES
            return Def.ActorFrame {
                OnCommand=function(self) MESSAGEMAN:Broadcast('Entry', {count=count}) end,
                EndCommand=function(self)
                    MESSAGEMAN:Broadcast('RootEnd', {count=count, top=SCREENMAN:GetTopScreen() ~= nil})
                end,
                Def.Actor {
                    EndCommand=function(self)
                        MESSAGEMAN:Broadcast('ChildEnd', {count=count})
                        self:queuecommand('Retired')
                    end,
                    RetiredCommand=function(self) MESSAGEMAN:Broadcast('Retired') end,
                },
            }
        "#,
    );
    let mut scene = compiled.live_scene.take().expect("first scene");
    scene.take_startup_output();
    scene
        .advance(SongLuaLiveFrame {
            music_seconds: 2.0,
            beat: 4.0,
            screen_seconds: 10.0,
            delta_seconds: 0.02,
            ..Default::default()
        })
        .expect("old frame");
    let mut compiled = song::reload_song_lua(
        scene,
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None::<song::SongLuaOverlayModelLayer<()>>,
    )
    .expect("same-VM reload");
    let mut scene = compiled[0].live_scene.take().expect("replacement");
    let output = scene.take_startup_output();
    assert_eq!(
        output
            .source_broadcasts
            .iter()
            .map(|event| event.name.as_str())
            .collect::<Vec<_>>(),
        ["RootEnd", "ChildEnd", "Entry"]
    );
    assert_eq!(output.source_broadcasts[0].music_seconds, 2.0);
    assert_eq!(output.source_broadcasts[1].music_seconds, 2.0);
    assert_eq!(output.source_broadcasts[2].music_seconds, 0.0);
    assert_eq!(
        output.source_broadcasts[0]
            .params
            .as_ref()
            .expect("plain end params"),
        &Some(serde_json::json!({"count":1,"top":false}))
    );
    let output = scene.end().expect("final end");
    assert_eq!(output.source_broadcasts.len(), 2);
    assert!(
        scene
            .end()
            .expect("end only once")
            .source_broadcasts
            .is_empty()
    );
    assert!(
        scene.advance(SongLuaLiveFrame::default()).is_err(),
        "ended actors must not update"
    );
}

#[test]
fn live_layers() {
    let dir = std::env::temp_dir().join(format!("deadsync-live-layers-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("layer directory");
    let background = dir.join("background.lua");
    let foreground = dir.join("foreground.lua");
    fs::write(
        &background,
        r#"
LIVE_ORDER = 0
return Def.ActorFrame { Name = 'Background', OnCommand = function(self)
    self:SetUpdateFunction(function(self, delta)
        if delta > 0 then LIVE_ORDER = LIVE_ORDER + 1; self:x(LIVE_ORDER) end
    end)
end }
"#,
    )
    .expect("background script");
    fs::write(
        &foreground,
        r#"
return Def.ActorFrame { Name = 'Foreground', OnCommand = function(self)
    self:SetUpdateFunction(function(self, delta)
        if delta > 0 then self:x(2 * LIVE_ORDER) end
    end)
end }
"#,
    )
    .expect("foreground script");
    let mut context = SongLuaCompileContext::new(&dir, "shared layers");
    context.live = true;
    context.background_layer_count = 1;
    let mut layers = song::compile_song_lua_layers(
        &[&background, &foreground],
        1,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None::<song::SongLuaOverlayModelLayer<()>>,
        |_, _| None,
    )
    .expect("one shared VM");
    assert!(layers[0].live_scene.is_none());
    let mut scene = layers[1].live_scene.take().expect("primary owns the VM");
    let frame = SongLuaLiveFrame {
        screen_seconds: 0.1,
        delta_seconds: 0.1,
        ..SongLuaLiveFrame::default()
    };
    assert!(
        scene
            .advance(frame.clone())
            .expect("shared update")
            .errors
            .is_empty()
    );
    for (layer, name, x) in [(0, "Background", 1.0), (1, "Foreground", 2.0)] {
        let local = layers[layer]
            .overlays
            .iter()
            .position(|actor| actor.name.as_deref() == Some(name))
            .expect("layer actor");
        assert_eq!(
            scene.states()[layers[layer].live_actor_indices()[local]].x,
            x
        );
    }
}

#[test]
fn live_native_mini_zoom_updates_before_song_callbacks_and_preserves_oitg_z() {
    let mut compiled = compile_with_context(
        "native-mini-zoom",
        r#"
return Def.ActorFrame {InitCommand=function(self)
  PLAYER=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
  FIELD=PLAYER:GetChild('NoteField')
  assert(PLAYER:get_oitg_zoom_mode()==false)
  assert(PLAYER:set_oitg_zoom_mode(true)==PLAYER)
  FIELD:zoomz(3)
  local ps=GAMESTATE:GetPlayerState(PLAYER_1)
  ps:GetCurrentPlayerOptions():Mini(0)
  ps:GetPlayerOptions('ModsLevel_Song'):Mini(1,2)
  self:SetUpdateFunction(function()
    SEEN={FIELD:GetZoomX(),FIELD:GetZoomY(),FIELD:GetZoomZ()}
  end)
end}
"#,
        |context| context.defer_initial_live_update = true,
    );
    let mut scene = compiled.live_scene.take().unwrap();
    let frame = |seconds| SongLuaLiveFrame {
        screen_seconds: seconds,
        delta_seconds: 0.125,
        ..Default::default()
    };
    assert!(scene.advance(frame(0.125)).unwrap().errors.is_empty());
    scene_exec(
        &scene,
        "assert(SEEN[1]==0.875 and SEEN[2]==0.875 and SEEN[3]==3)",
    )
    .unwrap();
    scene_exec(&scene, "PLAYER:set_oitg_zoom_mode(false)").unwrap();
    assert!(scene.advance(frame(0.25)).unwrap().errors.is_empty());
    scene_exec(
        &scene,
        "assert(SEEN[1]==0.75 and SEEN[2]==0.75 and SEEN[3]==0.75)",
    )
    .unwrap();
}

#[test]
#[cfg(feature = "test-support")]
fn live_native_option_clocks_are_independent_and_consumed_once() {
    let mut compiled = compile_with_context(
        "independent-option-clocks",
        "return Def.ActorFrame {}",
        |context| {
            context.defer_initial_live_update = true;
            context.players[1].enabled = true;
        },
    );
    let mut scene = compiled.live_scene.take().unwrap();
    scene_exec(
        &scene,
        r#"
for _,pn in ipairs({PLAYER_1,PLAYER_2}) do
 local ps=GAMESTATE:GetPlayerState(pn)
 ps:GetCurrentPlayerOptions():Drunk(0)
 ps:GetPlayerOptions('ModsLevel_Song'):Drunk(1,1)
end
GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate(1)
GAMESTATE:GetSongOptionsObject('ModsLevel_Song'):MusicRate(2,1)
"#,
    )
    .unwrap();
    scene.set_option_clock_deltas([0.03125, 0.0625, 0.125]);
    let frame = |seconds| SongLuaLiveFrame {
        screen_seconds: seconds,
        delta_seconds: 0.25,
        ..Default::default()
    };
    assert!(scene.advance(frame(0.25)).unwrap().errors.is_empty());
    scene_exec(
        &scene,
        r#"
assert(GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate()==1.03125)
assert(GAMESTATE:GetPlayerState(PLAYER_1):GetCurrentPlayerOptions():Drunk()==0.0625)
assert(GAMESTATE:GetPlayerState(PLAYER_2):GetCurrentPlayerOptions():Drunk()==0.125)
"#,
    )
    .unwrap();
    assert!(scene.advance(frame(0.5)).unwrap().errors.is_empty());
    scene_exec(
        &scene,
        r#"
assert(GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):MusicRate()==1.28125)
assert(GAMESTATE:GetPlayerState(PLAYER_1):GetCurrentPlayerOptions():Drunk()==0.3125)
assert(GAMESTATE:GetPlayerState(PLAYER_2):GetCurrentPlayerOptions():Drunk()==0.375)
"#,
    )
    .unwrap();
}

#[test]
fn live_false_condition_skips_subtree_and_compacts_subscriber_paths() {
    let dir = std::env::temp_dir().join(format!("deadsync-live-condition-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("condition directory");
    let entry = dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local function excluded() error('excluded actor must never run') end
local function received(self) self:aux(self:getaux() + 1) end
return Def.ActorFrame {
    InitCommand = function(self)
        assert(self:GetNumChildren() == 2)
        local children = self:GetChildren()
        assert(children.Excluded == nil)
        assert(children.Group:GetNumChildren() == 1)
    end,
    OnCommand = function(self)
        MESSAGEMAN:Broadcast('Probe')
        assert(self:GetChild('First'):getaux() == 1)
        assert(self:GetChild('Group'):GetChild('Last'):getaux() == 1)
    end,
    Def.Quad { Name = 'Excluded', Condition = false, InitCommand = excluded,
        OnCommand = excluded, ProbeMessageCommand = excluded },
    Def.Actor { Name = 'First', Condition = 0, ProbeMessageCommand = received },
    Def.ActorFrame { Condition = false, InitCommand = excluded,
        Def.Quad { ProbeMessageCommand = excluded, OnCommand = excluded } },
    Def.ActorFrame { Name = 'Group',
        Def.Quad { Condition = false, InitCommand = excluded, ProbeMessageCommand = excluded },
        Def.Actor { Name = 'Last', Condition = true, ProbeMessageCommand = received } },
}
"#,
    )
    .expect("conditional actor tree");
    let mut context = SongLuaCompileContext::new(&dir, "conditional children");
    context.live = true;
    #[cfg(feature = "test-support")]
    {
        context.message_actor_order = [("1", 10), ("1/1", 20), ("1/2", 30), ("1/2/1", 40)]
            .into_iter()
            .map(|(path, rank)| (path.into(), rank))
            .collect();
    }
    let mut compiled: Compiled = song::compile_song_lua(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("skip excluded initialization and broadcast subscriptions");
    let mut scene = compiled.live_scene.take().expect("retained active scene");
    let output = scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .expect("active conditional tree");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert!(
        !compiled
            .overlays
            .iter()
            .any(|actor| actor.name.as_deref() == Some("Excluded"))
    );
}

#[test]
fn live_steps_display_owns_native_children_resources_and_step_lifecycle() {
    let mut compiled = compile_with_context(
        "native-gameplay-steps-display",
        "return Def.Actor {}",
        |context| {
            context.defer_initial_live_update = true;
            context.players[0].enabled = true;
            context.players[1].enabled = true;
            context.song_steps = vec![
                song::SongLuaStep {
                    steps_type: "StepsType_Dance_Single".into(),
                    difficulty: SongLuaDifficulty::Challenge,
                    description: "P1 chart".into(),
                    chart_name: String::new(),
                    credit: String::new(),
                    meter: 9,
                    music_file: String::new(),
                    possible_grade_points: 0,
                },
                song::SongLuaStep {
                    steps_type: "StepsType_Dance_Single".into(),
                    difficulty: SongLuaDifficulty::Edit,
                    description: "Fixture edit".into(),
                    chart_name: String::new(),
                    credit: String::new(),
                    meter: 13,
                    music_file: String::new(),
                    possible_grade_points: 0,
                },
            ];
            context.player_step_indices = [Some(0), Some(1)];
        },
    );
    let scene = compiled
        .live_scene
        .take()
        .expect("owned native StepsDisplays");
    scene_exec(&scene,r#"
local screen=SCREENMAN:GetTopScreen()
for index,pn in ipairs{PLAYER_1,PLAYER_2} do
    local display=screen:GetChild('StepsDisplayP'..index)
    assert(display:GetNumChildren()==4, 'native StepsDisplay must own its four metric-loaded children')
    assert(display:GetParent()==screen)
    for _,name in ipairs{'Frame','Meter','Description','Autogen'} do
        assert(display:GetChild(name):GetParent()==display, 'native named child ownership')
    end
    assert(display:GetChild('Frame'):GetTexture() and display:GetChild('Autogen'):GetTexture(),
        'inherited graphics redirections load the native blank resources')
    assert(display:GetChild('Meter'):GetText()==(index==1 and '9' or '13'))
    assert(not display:GetChild('Meter'):GetVisible(), 'gameplay MeterOnCommand hides the loaded meter')
    assert(display:GetChild('Description'):GetVisible()==(index==2))
    assert(display:GetChild('Description'):GetText()==(index==1 and 'Expert' or 'Fixture edit'))
    assert(display:SetFromSteps(nil)==display and not display:GetVisible(), 'native Unset keeps the resource tree')
    assert(display:GetNumChildren()==4)
    assert(display:SetFromGameState(pn)==display and display:GetVisible())
end
local p1=screen:GetChild('StepsDisplayP1')
p1:SetFromSteps(GAMESTATE:GetCurrentSteps(PLAYER_2))
assert(p1:GetChild('Meter'):GetText()=='13' and p1:GetChild('Description'):GetText()=='Fixture edit')
assert(p1:GetChild('Description'):GetVisible(), 'native edit description follows selected Steps')
p1:SetFromGameState(PLAYER_1)
assert(p1:GetChild('Meter'):GetText()=='9' and not p1:GetChild('Description'):GetVisible())
"#).expect("native StepsDisplay resources, metric commands and data lifecycle");
}

fn finish_note_field_intro_fixture(scene: &SongLuaLiveScene) {
    scene_exec(
        scene,
        r#"
local underlay = SCREENMAN:GetTopScreen():GetChild('Underlay')
for _, pn in ipairs{'P1','P2'} do
    local container = underlay:GetChild('NoteFieldContainer'..pn)
    if container then
        local children = container:GetChild('')
        assert(#children >= 2, 'real ColumnFlash and DisplayMods fixture children')
        children[#children]:finishtweening()
    end
end
"#,
    )
    .expect("finish native NoteField intro in isolated queue fixture");
}

fn scene_number(
    scene: &SongLuaLiveScene,
    source: impl AsRef<[u8]>,
) -> Result<f64, deadlib_lua::Error> {
    let code = deadlib_lua::compile_named(source.as_ref(), b"=existing live numeric fixture")?;
    let mut runtime = scene.runtime();
    match runtime.vm_mut().execute(&code)?.first().copied() {
        Some(deadlib_lua::Value::Number(value)) => Ok(value),
        _ => runtime
            .vm_mut()
            .with_native_stack(|stack| Err(stack.error(b"numeric fixture result expected"))),
    }
}

#[test]
fn live_option_approach_uses_native_frame_delta_at_large_screen_time() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../assets/itgmania/native-option-frame-delta-reference-v1.json"
    ))
    .unwrap();
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 25);
    for case in cases {
        let mut compiled = compile_with_context(
            "native-option-frame-delta",
            "return Def.ActorFrame {}",
            |context| context.defer_initial_live_update = true,
        );
        let mut scene = compiled.live_scene.take().unwrap();
        let screen = case["screen"].as_f64().unwrap() as f32;
        let delta = case["delta"].as_f64().unwrap() as f32;
        assert!(
            scene
                .advance(SongLuaLiveFrame {
                    screen_seconds: f64::from(screen),
                    delta_seconds: 0.0,
                    ..Default::default()
                })
                .unwrap()
                .errors
                .is_empty()
        );
        scene_exec(
            &scene,
            format!(
                r#"
PS=GAMESTATE:GetPlayerState(PLAYER_1)
PS:GetCurrentPlayerOptions():Beat({})
PS:GetPlayerOptions('ModsLevel_Song'):Beat({}, {})
"#,
                case["initial"], case["target"], case["speed"]
            ),
        )
        .unwrap();
        assert!(
            scene
                .advance(SongLuaLiveFrame {
                    screen_seconds: f64::from(screen + delta),
                    delta_seconds: f64::from(delta),
                    music_seconds: -1.0,
                    ..Default::default()
                })
                .unwrap()
                .errors
                .is_empty()
        );
        let actual = scene_number(&scene, "return PS:GetCurrentPlayerOptions():Beat()")
            .map(|value| value as f32)
            .unwrap();
        let expected = case["expected"].as_f64().unwrap() as f32;
        assert!(
            (actual - expected).abs() <= 0.0000001,
            "native option approach must use frame delta rather than rounded screen time: {} actual={actual:?} expected={expected:?}",
            case["id"]
        );
    }
}

fn scene_global_number(
    scene: &SongLuaLiveScene,
    name: impl AsRef<[u8]>,
) -> Result<f64, deadlib_lua::Error> {
    let mut runtime = scene.runtime();
    match runtime.vm_mut().get_global(name.as_ref())? {
        deadlib_lua::Value::Number(value) => Ok(value),
        _ => runtime
            .vm_mut()
            .with_native_stack(|stack| Err(stack.error(b"numeric fixture global expected"))),
    }
}
fn scene_global_bool(
    scene: &SongLuaLiveScene,
    name: impl AsRef<[u8]>,
) -> Result<bool, deadlib_lua::Error> {
    let value = scene.runtime().vm_mut().get_global(name.as_ref())?;
    Ok(!matches!(
        value,
        deadlib_lua::Value::Nil | deadlib_lua::Value::Boolean(false)
    ))
}
fn fixture_string(
    vm: &mut deadlib_lua::Vm,
    value: deadlib_lua::Value,
) -> Result<String, deadlib_lua::Error> {
    let Some(id) = vm.coerce_string(value)? else {
        return vm.with_native_stack(|stack| Err(stack.error(b"string fixture result expected")));
    };
    String::from_utf8(vm.string_bytes(id).unwrap().to_vec()).map_err(|_| {
        vm.with_native_stack::<()>(|stack| Err(stack.error(b"UTF-8 fixture string expected")))
            .unwrap_err()
    })
}
fn scene_global_string(
    scene: &SongLuaLiveScene,
    name: impl AsRef<[u8]>,
) -> Result<String, deadlib_lua::Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let value = vm.get_global(name.as_ref())?;
    fixture_string(vm, value)
}
fn scene_string(
    scene: &SongLuaLiveScene,
    source: impl AsRef<[u8]>,
) -> Result<String, deadlib_lua::Error> {
    let code = deadlib_lua::compile_named(source.as_ref(), b"=existing live string fixture")?;
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let value = vm
        .execute(&code)?
        .first()
        .copied()
        .unwrap_or(deadlib_lua::Value::Nil);
    fixture_string(vm, value)
}

#[test]
fn live_gameplay_reload_copies_human_steps_to_disabled_player() {
    let mut compiled = compile_with_context(
        "reload-cpu-steps",
        r#"
SCENES = (SCENES or 0) + 1
local scene = SCENES
local p1 = GAMESTATE:GetCurrentSteps(PLAYER_1)
local p2 = GAMESTATE:GetCurrentSteps(PLAYER_2)
assert(GAMESTATE:IsPlayerEnabled(PLAYER_1) and not GAMESTATE:IsPlayerEnabled(PLAYER_2))
if scene == 1 then
    OLD_CPU_STEPS = p2
else
    assert(p1 == CHOSEN_STEPS and p2 == CHOSEN_STEPS,
        'native gameplay Init copies human Steps to the nonhuman player')
    assert(p2:GetDifficulty() == 'Difficulty_Edit')
end
return Def.Actor {
    OnCommand = function(self)
        if scene == 1 then
            CHOSEN_STEPS = GAMESTATE:GetCurrentSong():GetAllSteps()[2]
            GAMESTATE:SetCurrentSteps(PLAYER_1, CHOSEN_STEPS)
            assert(GAMESTATE:GetCurrentSteps(PLAYER_2) == OLD_CPU_STEPS,
                'SetCurrentSteps only forces enabled other players')
            SCREENMAN:SetNewScreen('ScreenGameplay')
        end
    end,
    EndCommand = function(self)
        if scene == 1 then
            END_SAW_OLD_CPU = GAMESTATE:GetCurrentSteps(PLAYER_2) == OLD_CPU_STEPS
        end
    end,
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
            context.players[0].enabled = true;
            context.players[1].enabled = false;
            context.song_steps = [SongLuaDifficulty::Challenge, SongLuaDifficulty::Edit]
                .into_iter()
                .map(|difficulty| song::SongLuaStep {
                    steps_type: "StepsType_Dance_Single".into(),
                    difficulty,
                    description: format!("{difficulty:?}"),
                    chart_name: String::new(),
                    credit: String::new(),
                    meter: 9,
                    music_file: String::new(),
                    possible_grade_points: 0,
                })
                .collect();
            context.player_step_indices = [Some(0), Some(0)];
        },
    );
    let mut scene = compiled
        .live_scene
        .take()
        .expect("first owned gameplay scene");
    let output = scene
        .advance(SongLuaLiveFrame::default())
        .expect("first On selects Edit");
    assert_eq!(
        output.request,
        Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into()))
    );
    let mut layers = song::reload_song_lua(
        scene,
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None::<song::SongLuaOverlayModelLayer<()>>,
    )
    .expect("native gameplay Init copies human Steps to the nonhuman player");
    let mut scene = layers[0]
        .live_scene
        .take()
        .expect("replacement gameplay scene");
    assert_eq!(
        scene_global_number(&scene, "SCENES")
            .map(|value| value as u32)
            .unwrap(),
        2
    );
    assert!(
        scene_global_bool(&scene, "END_SAW_OLD_CPU").unwrap(),
        "outgoing End runs before the new gameplay initialization"
    );
    assert_eq!(
        scene.selected_context().unwrap().player_step_indices,
        [Some(1), Some(1)]
    );
    let output = scene
        .advance(SongLuaLiveFrame::default())
        .expect("replacement On");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert_eq!(
        output.request, None,
        "Edit construction does not replay the introduction"
    );
}

#[test]
fn interactive_runtime_uses_authored_controls_motion_timers_sounds_and_reload() {
    // Two independently authored configurations: no known chart filenames,
    // globals, system layout, constants or asset names enter the runtime.
    for (name, button, speed, delay, duration, sound) in [
        ("pilot-a", "Left", 14.0, 0.25, 2.0, "custom/chime.wav"),
        ("pilot-b", "Up", 38.0, 0.5, 3.0, "different/tone.wav"),
    ] {
        let source = format!(
            r#"
-- Changing this comment or any parameter must not select another engine path.
VISITS = (VISITS or 0) + 1
local tone, velocity, elapsed, finished = nil, 0, 0, false
return Def.ActorFrame {{
    Def.Sound {{ File='{sound}', InitCommand=function(self) tone=self end }},
    Def.ActorFrame {{
        Name='Pilot',
        Def.Quad {{ InitCommand=function(self) self:zoomto(20,20) end }},
        InitCommand=function(self) self:x(17) end,
        OnCommand=function(self)
            self:sleep({delay}):queuecommand('Arm')
        end,
        ArmCommand=function(self)
            SCREENMAN:GetTopScreen():AddInputCallback(function(event)
                if event.button == '{button}' then
                    if event.type == 'InputEventType_FirstPress' then
                        velocity={speed}; tone:play()
                    elseif event.type == 'InputEventType_Release' then
                        velocity=0; tone:pause(true):pause(false):stop()
                    end
                end
            end)
            self:SetUpdateFunction(function(self, dt)
                self:x(self:GetX() + velocity*dt)
                elapsed=elapsed+dt
                if elapsed >= {duration} and not finished then
                    finished=true
                    SCREENMAN:SetNewScreen('ScreenGameplay')
                end
            end)
        end,
    }},
}}
"#
        );
        let mut compiled = compile_with_context(name, &source, |context| {
            write_interactive_sound(&context.song_dir, sound)
        });
        let index = compiled
            .overlays
            .iter()
            .position(|a| a.name.as_deref() == Some("Pilot"))
            .unwrap();
        let index = compiled.live_actor_indices()[index];
        let mut scene = compiled.live_scene.take().unwrap();
        let mut input = song::SongLuaInputEvent {
            player: Some(0),
            controller: Some(0),
            button,
            game_button: button,
            kind: song::SongLuaInputType::FirstPress,
            device: "InputDevice_Key",
            device_button: "DeviceButton_space",
            level: 1.0,
            z: 0,
            down: true,
            ago: 0.0,
            is_joystick: false,
            is_mouse: false,
        };
        assert!(
            scene.input(&input).unwrap().1.actor_sounds.is_empty(),
            "controls are not registered yet"
        );
        let frame = |seconds, delta| SongLuaLiveFrame {
            screen_seconds: seconds,
            music_seconds: seconds,
            beat: seconds * 2.0,
            delta_seconds: delta,
            ..Default::default()
        };
        for step in 1..=8 {
            let output = scene.advance(frame(f64::from(step) / 8.0, 0.125)).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            assert!(output.request.is_none());
        }
        assert_eq!(scene.states()[index].x, 17.0);
        let output = scene.input(&input).unwrap().1;
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let [SongLuaActorSound::Play { owner, path }] = output.actor_sounds.as_slice() else {
            panic!("authored sound was not played: {:?}", output.actor_sounds);
        };
        let owner = *owner;
        assert!(path.ends_with(sound));
        scene.advance(frame(1.5, 0.5)).unwrap();
        assert_eq!(scene.states()[index].x, 17.0 + speed as f32 * 0.5);
        input.kind = song::SongLuaInputType::Release;
        input.down = false;
        let output = scene.input(&input).unwrap().1;
        assert_eq!(
            output.actor_sounds,
            [
                SongLuaActorSound::Pause {
                    owner,
                    paused: true
                },
                SongLuaActorSound::Pause {
                    owner,
                    paused: false
                },
                SongLuaActorSound::Stop { owner },
            ]
        );
        let mut requested_at = None;
        for step in 13..=32 {
            let seconds = f64::from(step) / 8.0;
            let output = scene.advance(frame(seconds, 0.125)).unwrap();
            assert!(output.errors.is_empty());
            if let Some(request) = output.request {
                assert!(
                    matches!(request, SongLuaLiveRequest::NewScreen(ref screen) if screen == "ScreenGameplay")
                );
                requested_at = Some(seconds);
                break;
            }
        }
        let at = requested_at.expect("Lua timer requested a screen reload");
        assert!((at - (delay + duration)).abs() <= 0.25, "timer at {at}");
        let mut layers =
            song::reload_song_lua::<(), ()>(scene, |_, _, _| Ok(Arc::<[()]>::from([])), |_| None)
                .unwrap();
        let scene = layers
            .iter_mut()
            .find_map(|layer| layer.live_scene.take())
            .unwrap();
        assert_eq!(
            scene_global_number(&scene, "VISITS")
                .map(|value| value as i32)
                .unwrap(),
            2,
            "screen replacement preserves arbitrary Lua globals"
        );
    }
}

#[test]
fn live_player_life_setter_updates_shared_meter_and_notifies_synchronously() {
    let mut compiled = compile(
        "player-life",
        r#"
local top = SCREENMAN:GetTopScreen()
local player = top:GetChild('PlayerP1')
local meter = top:GetLifeMeter(PLAYER_1)
local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local state = GAMESTATE:GetPlayerState(PLAYER_1)
local done, changed = false, 0
HEALTH_CHANGES = 0
return Def.ActorFrame {
    OnCommand = function(self)
        self:sleep(60)
        self:SetUpdateFunction(function(_, delta)
            if delta <= 0 or done then return end
            done = true
            assert(math.abs(meter:GetLife() - 0.7) < 0.000001)
            assert(player:SetLife(0.2) == player)
            assert(math.abs(meter:GetLife() - 0.2) < 0.000001)
            assert(math.abs(stats:GetCurrentLife() - 0.2) < 0.000001)
            assert(changed == 1)
            assert(player:SetLife(2) == player and meter:GetLife() == 1 and meter:IsHot())
            assert(player:SetLife(-1) == player and meter:GetLife() == 0 and meter:IsFailing())
            assert(changed == 3)
            assert(state:GetHealthState() == 'HealthState_Alive')
            stats:FailPlayer()
            assert(stats:GetFailed())
        end)
    end,
    LifeChangedMessageCommand = function(_, params)
        assert(params.Player == PLAYER_1 and params.LifeMeter == meter)
        assert(params.LifeMeter:GetLife() >= 0 and params.LifeMeter:GetLife() <= 1)
        assert(state:GetHealthState() == 'HealthState_Alive')
        changed = changed + 1
        local ok, err = pcall(function() player:SetLife(0.8) end)
        assert(not ok and tostring(err):find('LifeChangedMessageCommand'))
    end,
    HealthStateChangedMessageCommand = function(_, params)
        assert(params.PlayerNumber == PLAYER_1)
        assert(params.OldHealthState == 'HealthState_Alive' and params.HealthState == 'HealthState_Dead')
        assert(state:GetHealthState() == 'HealthState_Dead')
        HEALTH_CHANGES = HEALTH_CHANGES + 1
    end,
}
"#,
    );
    let mut scene = compiled.live_scene.take().expect("owned life scene");
    let mut frame = SongLuaLiveFrame {
        screen_seconds: 0.1,
        music_seconds: 0.1,
        delta_seconds: 0.1,
        ..Default::default()
    };
    frame.players[0].life = 0.7;
    frame.players[0].fail_enabled = true;
    let output = scene.advance(frame.clone()).expect("life frame");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert_eq!(
        scene_global_number(&scene, "HEALTH_CHANGES")
            .map(|value| value as u32)
            .expect("health change"),
        1
    );
    assert_eq!(
        output.player_writes,
        vec![
            SongLuaLivePlayerWrite::SetLife {
                player: 0,
                life: 0.2
            },
            SongLuaLivePlayerWrite::SetLife {
                player: 0,
                life: 1.0
            },
            SongLuaLivePlayerWrite::SetLife {
                player: 0,
                life: 0.0
            },
            SongLuaLivePlayerWrite::Fail { player: 0 },
        ]
    );
}

#[test]
fn live_theme_setup_broadcasts_respect_song_construction_boundary() {
    use deadsync_rules::timing::{TimingData, TimingSegments};
    for reload in [false, true] {
        let source = format!(
            r#"
ROUND=(ROUND or 0)+1
return Def.ActorFrame {{
    InitCommand=function(self)
        assert(GAMESTATE:Env().P1PeakNPS==1.5, 'real theme histogram must initialize before song Init')
    end,
    OnCommand=function(self)
        MESSAGEMAN:Broadcast('PeakNPSUpdated')
        self:sleep(0.25):queuecommand('AuthoredRuntime')
        if {reload} and ROUND==1 then SCREENMAN:SetNewScreen('ScreenGameplay') end
    end,
    PeakNPSUpdatedMessageCommand=function(self)
        MESSAGEMAN:Broadcast('AuthoredPeakReaction')
    end,
    AuthoredRuntimeCommand=function(self)
        MESSAGEMAN:Broadcast('CurrentSongChanged')
        MESSAGEMAN:Broadcast('PeakNPSUpdated')
    end,
}}
"#
        );
        let mut compiled = compile_with_context(
            &format!("theme-broadcast-boundary-{reload}"),
            &source,
            |context| {
                context.defer_initial_live_update = true;
                context.screen_width = 854.0;
                context.players[0].enabled = true;
                context.players[1].enabled = false;
                context.players[0].step_statistics_enabled = true;
                context.player_step_indices = [Some(0), None];
                context.song_steps = vec![song::SongLuaStep {
                    steps_type: "StepsType_Dance_Single".into(),
                    difficulty: song::SongLuaDifficulty::Challenge,
                    description: String::new(),
                    chart_name: String::new(),
                    credit: String::new(),
                    meter: 12,
                    music_file: String::new(),
                    possible_grade_points: 100,
                }];
                let timing = TimingData::from_segments(
                    0.0,
                    0.0,
                    &TimingSegments {
                        bpms: vec![(0.0, 120.0)],
                        ..Default::default()
                    },
                    &[],
                );
                context.song_step_timing = vec![timing.clone()];
                context.player_timing = [Some(timing), None];
                context.song_step_notes = vec![std::sync::Arc::from(
                    &b"1000\n0100\n2000\n3000\nM000\n4000\n3000\n1000\n"[..],
                )];
                context.song_step_row_beats = vec![std::sync::Arc::from(
                    &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0][..],
                )];
            },
        );
        assert_eq!(compiled.startup_reloads, u32::from(reload));
        let mut scene = compiled.live_scene.take().expect("retained boundary scene");
        let startup = scene.take_startup_output();
        assert!(startup.errors.is_empty(), "{:?}", startup.errors);
        // compile.rs resets source capture when reconstructing a startup scene;
        // take_startup_output exposes the retained scene's source output. The
        // legacy stream separately keeps broadcasts from prior startup scenes.
        assert_eq!(
            scene_global_number(&scene, "ROUND")
                .map(|value| value as u32)
                .unwrap(),
            if reload { 2 } else { 1 }
        );
        assert_eq!(
            startup
                .broadcasts
                .iter()
                .filter(|(_, name, _)| name == "PeakNPSUpdated")
                .count(),
            if reload { 2 } else { 1 }
        );
        let expected = ["PeakNPSUpdated", "AuthoredPeakReaction"];
        assert_eq!(
            startup
                .source_broadcasts
                .iter()
                .map(|event| event.name.as_str())
                .collect::<Vec<_>>(),
            expected,
            "pre-song theme broadcasts must stay outside authored song output"
        );
        scene.advance(SongLuaLiveFrame::default()).unwrap();
        let output = scene
            .advance(SongLuaLiveFrame {
                screen_seconds: 0.5,
                delta_seconds: 0.5,
                music_seconds: 0.5,
                beat: 1.0,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(
            output
                .source_broadcasts
                .iter()
                .map(|event| event.name.as_str())
                .collect::<Vec<_>>(),
            [
                "CurrentSongChanged",
                "PeakNPSUpdated",
                "AuthoredPeakReaction",
                "PeakNPSUpdated",
                "AuthoredPeakReaction"
            ],
            "theme broadcasts nested under authored runtime commands must remain observable"
        );
    }
}

#[test]
fn live_note_field_loads_inherited_board_sprite_outside_column_vector() {
    let compiled = compile(
        "note-field-board",
        r#"
return Def.ActorFrame { OnCommand = function()
    BOARDS_CHECKED = 0
    local previous
    for index = 1, 2 do
        local field = SCREENMAN:GetTopScreen():GetChild('PlayerP' .. index):GetChild('NoteField')
        assert(field:GetNumChildren() == 1, 'NoteField must load its native Board child')
        local board = assert(field:GetChild('Board'))
        assert(board:GetName() == 'Board' and board:GetParent() == field)
        assert(field:GetChildren().Board == board and board ~= previous)
        local texture = assert(board:GetTexture(), 'Board must load the inherited blank bitmap')
        assert(texture:GetPath() == 'Themes/_fallback/Graphics/_blank.png')
        assert(texture:GetSourceWidth() == 4 and texture:GetSourceHeight() == 2)
        assert(texture:GetImageWidth() == 8 and texture:GetImageHeight() == 8)
        assert(texture:GetTextureWidth() == 8 and texture:GetTextureHeight() == 8)
        assert(texture:GetNumFrames() == 1 and board:GetWidth() == 4 and board:GetHeight() == 2)
        local columns = field:GetColumnActors()
        assert(#columns == 4)
        assert(field:GetNumChildren() == 1 and field:GetChild('Board') == board)
        assert(field:GetChild('Column1') == nil, 'column vector is separate from Board child inventory')
        previous = board
        BOARDS_CHECKED = BOARDS_CHECKED + 1
    end
end }
"#,
    );
    let scene = compiled.live_scene.expect("native NoteField board");
    assert_eq!(
        scene_global_number(&scene, "BOARDS_CHECKED")
            .map(|value| value as i64)
            .unwrap(),
        2,
        "regression must inspect both live NoteFields"
    );
}

#[test]
fn live_color_constants_preserve_native_palette_bound() {
    let mut compiled = compile(
        "native-color-palette",
        r#"
assert(Color.Pink == nil, 'native Color table does not define Pink')
assert(Color('Pink') == nil and Color.NotAColor == nil)
assert(Color('White') == Color.White)
local palette = {
    Color.White, Color.Yellow, Color.Blue, Color.Red, Color.White,
    Color.Purple, Color.Green, Color.Orange, Color.Pink,
}
assert(#palette == 8, 'trailing native nil leaves an eight-entry palette')
math.randomseed(1)
local expected = {4, 8, 6, 8, 1, 2, 3, 8}
return Def.ActorFrame {
    InitCommand = function(self)
        for _, index in ipairs(expected) do
            local selected = math.random(#palette)
            assert(selected == index, 'native palette preserves seeded random choices')
            self:diffuse(palette[selected])
        end
        PALETTE_CHECKED = true
    end,
}
"#,
    );
    let scene = compiled.live_scene.take().expect("production color scene");
    assert!(scene_global_bool(&scene, "PALETTE_CHECKED").unwrap());
    assert_eq!(
        scene.states()[0].diffuse,
        [247.0 / 255.0, 148.0 / 255.0, 29.0 / 255.0, 1.0]
    );
}

#[test]
fn live_native_tween_argument_and_hurry_contract() {
    let mut compiled = compile_with_context(
        "native-tween-arguments",
        r#"
return Def.Actor { OnCommand=function(a)
    for _,kind in ipairs({0,1,2,3,'TweenType_Linear','accelerate','DECELERATE','Spring'}) do
        a:stoptweening()
        assert(a:tween('2',kind,0)==a)
        assert(a:GetTweenTimeLeft()==2)
        a:hurrytweening(0.25)
        assert(a:GetTweenTimeLeft()==0.5)
    end
    for _,curve in ipairs({{0.2,-0.3,1.4,1.3},
        {-0.2,0.1,0.1,-0.4,0.85,1.4,1.2,0.9},{false,'0',nil,1}}) do
        a:stoptweening();assert(a:tween(2,'TweenType_Bezier',curve)==a)
        assert(a:GetTweenTimeLeft()==2)
    end
    a:stoptweening()
    assert(not pcall(function() a:tween(1,1.5) end))
    assert(not pcall(function() a:tween(1,'bad') end))
    assert(not pcall(function() a:tween(1,'TweenType_Bezier',false) end))
    assert(a:GetTweenTimeLeft()==0)
    assert(a:tween(-1,'bad')==a and a:GetTweenTimeLeft()==0)
    assert(a:tween(1,'TweenType_Bezier',{0,1})==a and a:GetTweenTimeLeft()==0)
    a:hibernate(10):tween(2,0,0):hurrytweening(0.25)
    assert(a:GetTweenTimeLeft()==10.5, 'hurry must not scale hibernation')
    CONTRACT_PASSED = true
end }
"#,
        |context| context.defer_initial_live_update = true,
    );
    let mut scene = compiled.live_scene.take().unwrap();
    let output = scene.advance(SongLuaLiveFrame::default()).unwrap();
    assert!(
        output.errors.is_empty(),
        "native tween arguments: {:?}",
        output.errors
    );
    assert!(scene_global_bool(&scene, "CONTRACT_PASSED").unwrap());
}

#[test]
fn live_source_function_params() {
    let mut compiled = compile(
        "broadcast-function-params",
        r#"local function transform(value)
    CALLS=(CALLS or 0)+1
    return value+2
end
ORIGINAL_TRANSFORM=transform
return Def.ActorFrame {
    OnCommand=function(self)
        CALLS=0
        MESSAGEMAN:Broadcast('FunctionSnapshot', {transform, nested={callback=transform}, native=math.abs})
    end,
    FunctionSnapshotMessageCommand=function(self, params)
        assert(params[1]==ORIGINAL_TRANSFORM and params.nested.callback==ORIGINAL_TRANSFORM)
        assert(params.native==math.abs)
        RESULT=params[1](3)+params.nested.callback(4)+params.native(-7)
    end,
}"#,
    );
    let scene = compiled.live_scene.as_mut().expect("owned scene");
    let output = scene.take_startup_output();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert_eq!(
        scene_global_number(&scene, "CALLS")
            .map(|value| value as u32)
            .expect("listener function calls"),
        2
    );
    assert_eq!(
        scene_global_number(&scene, "RESULT")
            .map(|value| value as u32)
            .expect("listener result"),
        18
    );
    let params = output.source_broadcasts[0]
        .params
        .as_ref()
        .expect("function metadata must preserve callable broadcast parameters")
        .as_ref()
        .expect("params");
    let function = &params["1"];
    assert_eq!(function["type"], "function");
    assert!(
        function["source"]
            .as_str()
            .expect("Lua source")
            .ends_with("/default.lua")
    );
    assert_eq!(function["line"], 1);
    assert_eq!(function["last_line"], 4);
    assert_eq!(&params["nested"]["callback"], function);
    assert_eq!(
        params["native"],
        serde_json::json!({
            "type":"function", "source":"=[C]", "line":-1, "last_line":-1
        })
    );
}

#[test]
fn live_source_params() {
    let mut compiled = compile(
        "broadcast-params",
        r#"
            return Def.ActorFrame {
                OnCommand=function(self)
                    LOOKUPS=0
                    local params = setmetatable({ count=2, nested={true, 'original'}, [3]='mixed' }, {
                        __index=function() LOOKUPS=LOOKUPS+1 end,
                    })
                    MESSAGEMAN:Broadcast('Snapshot', params)
                    local cycle = {}; cycle.self = cycle
                    MESSAGEMAN:Broadcast('Unsupported', cycle)
                    MESSAGEMAN:Broadcast('Empty', {})
                    MESSAGEMAN:Broadcast('NoParams')
                end,
                SnapshotMessageCommand=function(self, params)
                    params.count=99; params.nested[2]='mutated'
                end,
                UnsupportedMessageCommand=function(self) DELIVERED=true end,
            }
        "#,
    );
    let scene = compiled.live_scene.as_mut().expect("owned scene");
    let output = scene.take_startup_output();
    assert!(
        output.errors.is_empty(),
        "trace capture must not interrupt Lua"
    );
    assert_eq!(
        scene_global_number(&scene, "LOOKUPS")
            .map(|value| value as u32)
            .expect("raw trace reads"),
        0
    );
    assert_eq!(
        output.source_broadcasts[0]
            .params
            .as_ref()
            .expect("plain values"),
        &Some(serde_json::json!({"count":2,"nested":[true,"original"],"3":"mixed"}))
    );
    assert!(
        output.source_broadcasts[1]
            .params
            .as_ref()
            .expect_err("cycle stays pending")
            .contains("cycle")
    );
    assert!(scene_global_bool(&scene, "DELIVERED").expect("listener ran"));
    assert_eq!(
        output.source_broadcasts[2]
            .params
            .as_ref()
            .expect("empty table"),
        &Some(serde_json::json!([]))
    );
    assert_eq!(
        output.source_broadcasts[3]
            .params
            .as_ref()
            .expect("no params"),
        &None
    );
}

#[test]
fn live_loadfile_preserves_secondary_ready_function_and_nil_results() {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-live-loader-results-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("song directory");
    fs::write(
        dir.join("module.lua"),
        r#"
local value = ...
return function() UPDATES = (UPDATES or 0) + value end,
    function() READY = (READY or 0) + 1 end, nil, 17, nil
"#,
    )
    .expect("multiple return module");
    let entry = dir.join("default.lua");
    fs::write(
        &entry,
        r#"
local chunk = assert(loadfile('module.lua'))
local values = {n=select('#',chunk(3)),chunk(3)}
RESULT_COUNT = values.n
assert(values.n == 5, 'scoped loadfile must retain all return values')
assert(type(values[2]) == 'function' and values[3] == nil and values[4] == 17 and values[5] == nil)
local update, ready = values[1], values[2]
return Def.ActorFrame{OnCommand=function(self)
    ready()
    self:SetUpdateFunction(function(self, delta) if delta > 0 then update() end end)
end}
"#,
    )
    .expect("ready callback source");
    let mut compiled = compile_with_context(
        "loader-results",
        &fs::read_to_string(&entry).unwrap(),
        |context| {
            context.song_dir = dir.clone();
        },
    );
    let scene = compiled.live_scene.as_mut().expect("retained live scene");
    assert_eq!(
        scene_global_number(&scene, "RESULT_COUNT")
            .map(|value| value as u32)
            .unwrap(),
        5,
        "scoped loadfile must retain all return values"
    );
    let output = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.1,
            beat: 0.1,
            screen_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert_eq!(
        scene_global_number(&scene, "READY")
            .map(|value| value as u32)
            .expect("secondary ready function ran"),
        1
    );
    assert_eq!(
        scene_global_number(&scene, "UPDATES")
            .map(|value| value as u32)
            .expect("first function retained its argument"),
        3
    );
}

#[test]
fn live_sparse_declarations_keep_native_child_callback_order() {
    let mut compiled = compile(
        "sparse-declaration-order",
        r#"
local function native_def(class, t)
    t.Class = class
    t._Source = '@sparse-declaration-order.lua'
    t._Line = 1
    t._Dir = './'
    return setmetatable(t, {})
end
local function root(ctor)
    local a = ctor('ActorFrame', {OnCommand=function() end,
        ctor('Actor', {Name='yolo', InitCommand=function(self) self:sleep(420) end}),
        ctor('Sprite', {Name='leader', OnCommand=function(self) table.insert(ON_ORDER, self:GetName()) end})})
    for offset, name in ipairs({'border','badge','left','right','black','sound'}) do
        a[#a + offset] = ctor(offset == 6 and 'Actor' or 'Sprite', {
            Name=name, OnCommand=function(self) table.insert(ON_ORDER, self:GetName()) end})
    end
    return a
end
local native = root(native_def)
local expected, prefix = {}, 0
for _, child in ipairs(native) do
    if type(child) == 'table' then
        if type(child.OnCommand) == 'function' then expected[#expected+1] = child.Name end
        prefix = prefix + 1
    end
end
for key, child in pairs(native) do
    if type(child) == 'table' and type(child.OnCommand) == 'function' and type(key) == 'number' and not (key >= 1 and key <= prefix and key % 1 == 0) then
        expected[#expected+1] = child.Name
    end
end
NATIVE_ORDER = table.concat(expected, ',')
ON_ORDER = {}
return root(function(class, t) return Def[class](t) end)
"#,
    );
    let scene = compiled.live_scene.as_mut().expect("retained scene");
    let output = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.1,
            beat: 0.1,
            screen_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    let actual = scene_string(&scene, "return table.concat(ON_ORDER, ',')").unwrap();
    let expected = scene_global_string(&scene, "NATIVE_ORDER").unwrap();
    assert_eq!(
        actual, expected,
        "native sparse declarations retain child callback order"
    );
}

#[test]
fn live_null_foreground_keeps_later_entries_and_owns_each_instance() {
    let dir = std::env::temp_dir().join(format!("deadsync-live-null-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("foreground directory");
    let blank = dir.join("blank.lua");
    let active = dir.join("active.lua");
    fs::write(
        &blank,
        "assert(NullActor.Class == 'Actor'); return NullActor",
    )
    .expect("empty declaration");
    fs::write(
        &active,
        r#"
ACTIVATIONS = 0
return Def.Actor { OnCommand = function(self)
    ACTIVATIONS = ACTIVATIONS + 1
    assert(GAMESTATE:IsPlayerEnabled(0))
    local children = self:GetParent():GetChildren()
    assert(children.first ~= children.last)
    children.first:aux(17)
    assert(children.last:getaux() == 0)
    assert(NullActor:getaux() == 0)
end }
"#,
    )
    .expect("later foreground");
    let mut context = SongLuaCompileContext::new(&dir, "null foreground");
    context.live = true;
    context.layer_start_beats = vec![0.0; 3];
    context.foreground_names = vec!["first".into(), "active".into(), "last".into()];
    let mut layers: Vec<Compiled> = song::compile_song_lua_layers(
        &[&blank, &active, &blank],
        0,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("all foreground entries");
    let mut scene = layers[0].live_scene.take().expect("primary retains scene");
    let output = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.02,
            beat: 0.01,
            screen_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .expect("foreground activation");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert_eq!(
        scene_global_number(&scene, "ACTIVATIONS")
            .map(|value| value as u32)
            .expect("later On"),
        1
    );
}

use deadlib_lua::{Error, Root, Value, Vm, compile_named};
// These handles replace the original owned table handles for each fixture.
// The VM owns their pins until the fixture's scene/runtime is dropped.
fn scene_global_table(scene: &SongLuaLiveScene, name: impl AsRef<[u8]>) -> Result<Root, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let value = vm.get_global(name.as_ref())?;
    if !matches!(value, Value::Table(_)) {
        return vm.with_native_stack(|stack| Err(stack.error(b"table fixture global expected")));
    }
    vm.pin(value)
}
#[derive(Clone, Copy)]
enum TableRead {
    Field,
    Raw,
    Method,
}
fn scene_table_read<R>(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: impl AsRef<[u8]>,
    read: TableRead,
    inspect: impl FnOnce(&mut Vm, Value) -> Result<R, Error>,
) -> Result<R, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let object = vm.rooted(owner)?;
    let key = vm.try_intern(name.as_ref())?;
    let value = match read {
        TableRead::Raw => {
            let Value::Table(table) = object else {
                unreachable!("rooted fixture table")
            };
            vm.raw_table_get(table, key)?
        }
        TableRead::Field | TableRead::Method => {
            let source = if matches!(read, TableRead::Method) {
                b"local object,key=...;return object[key](object)".as_slice()
            } else {
                b"local object,key=...;return object[key]".as_slice()
            };
            let code = compile_named(source, b"=existing live table fixture")?;
            vm.execute_with_args(&code, &[object, key])?
                .first()
                .copied()
                .unwrap_or(Value::Nil)
        }
    };
    inspect(vm, value)
}
fn fixture_number(vm: &mut Vm, value: Value) -> Result<f64, Error> {
    match value {
        Value::Number(number) => Ok(number),
        _ => vm.with_native_stack(|stack| Err(stack.error(b"numeric fixture value expected"))),
    }
}
fn scene_method_number(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: impl AsRef<[u8]>,
) -> Result<f64, Error> {
    scene_table_read(scene, owner, name, TableRead::Method, fixture_number)
}
fn scene_method_string(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: impl AsRef<[u8]>,
) -> Result<String, Error> {
    scene_table_read(scene, owner, name, TableRead::Method, fixture_string)
}

fn scene_method_is_table(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: impl AsRef<[u8]>,
) -> Result<bool, Error> {
    scene_table_read(scene, owner, name, TableRead::Method, |_, value| {
        Ok(matches!(value, Value::Table(_)))
    })
}

fn scene_field_number(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: impl AsRef<[u8]>,
    raw: bool,
) -> Result<f64, Error> {
    scene_table_read(
        scene,
        owner,
        name,
        if raw {
            TableRead::Raw
        } else {
            TableRead::Field
        },
        fixture_number,
    )
}
fn scene_field_string(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: impl AsRef<[u8]>,
    raw: bool,
) -> Result<String, Error> {
    scene_table_read(
        scene,
        owner,
        name,
        if raw {
            TableRead::Raw
        } else {
            TableRead::Field
        },
        fixture_string,
    )
}
fn scene_field_bool(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: impl AsRef<[u8]>,
    raw: bool,
) -> Result<bool, Error> {
    scene_table_read(
        scene,
        owner,
        name,
        if raw {
            TableRead::Raw
        } else {
            TableRead::Field
        },
        |_, value| Ok(!matches!(value, Value::Nil | Value::Boolean(false))),
    )
}
fn scene_index_number(scene: &SongLuaLiveScene, owner: Root, index: usize) -> Result<f64, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let Value::Table(table) = vm.rooted(owner)? else {
        unreachable!("rooted fixture table")
    };
    let value = vm.raw_table_get(table, Value::Number(index as f64))?;
    fixture_number(vm, value)
}
fn scene_table_len(scene: &SongLuaLiveScene, owner: Root) -> Result<usize, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let Value::Table(table) = vm.rooted(owner)? else {
        unreachable!("rooted fixture table")
    };
    vm.raw_table_len(table)
}
fn scene_table_identity(scene: &SongLuaLiveScene, owner: Root) -> Result<usize, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let value = vm.rooted(owner)?;
    Ok(vm
        .object_pointer(value)
        .expect("rooted fixture table identity"))
}

#[test]
fn live_gameplay_children_retain_native_owned_inventory() {
    let mut compiled = compile_with_context(
        "native-screen-children",
        r#"
return Def.Actor {
    OnCommand=function(self)
        SCREEN = SCREENMAN:GetTopScreen()
        CHILDREN = SCREEN:GetChildren()
        CHILD_KEYS = {}
        for name, child in pairs(CHILDREN) do CHILD_KEYS[#CHILD_KEYS+1] = name end
    end,
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
            context.players[0].enabled = true;
            context.players[1].enabled = false;
        },
    );
    let mut scene = compiled.live_scene.take().expect("owned gameplay screen");
    let output = scene
        .advance(SongLuaLiveFrame::default())
        .expect("first foreground On");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    let keys = scene_global_table(&scene, "CHILD_KEYS").unwrap();
    assert_eq!(
        scene_table_len(&scene, keys).unwrap(),
        22,
        "native gameplay child collection includes engine and adopted decoration owners"
    );
    scene_exec(
        &scene,
        r#"
local expected = {'', 'ActiveAttackListP1', 'ActiveAttackListP2', 'Cancel', 'Debug',
    'Footer', 'Header', 'In', 'LifeP1', 'LyricDisplay', 'OniGameOverP1', 'Out',
    'Overlay', 'PlayerOptionsP1', 'PlayerP1', 'ScoreP1', 'SongBackground',
    'SongForeground', 'SongOptions', 'StepsDescriptionP1', 'StepsDisplayP1', 'Underlay'}
local nextChildren = SCREEN:GetChildren()
for _, name in ipairs(expected) do
    assert(CHILDREN[name] ~= nil, 'missing native owned child: ' .. name)
    if name ~= '' then
        assert(nextChildren[name] == CHILDREN[name] and SCREEN:GetChild(name) == CHILDREN[name],
            'named child lookup preserves owned actor identity: ' .. name)
        assert(CHILDREN[name]:GetParent() == SCREEN)
    end
end
assert(#CHILDREN[''] == 5, 'native duplicate empty names retain all five screen actors')
for i, child in ipairs(CHILDREN['']) do
    assert(nextChildren[''][i] == child and child:GetParent() == SCREEN)
end
assert(SCREEN:GetNumChildren() == 26, 'native screen counts owned actors, not table keys')
local relevant = {}
for _, name in ipairs(CHILD_KEYS) do
    if name == 'In' or name == 'Underlay' then relevant[#relevant+1] = name end
end
table.sort(relevant)
assert(relevant[1] == 'In' and relevant[2] == 'Underlay',
    'native child lookup contains both In and Underlay')
"#,
    )
    .expect("native inventory, grouping, identity and traversal");
}

#[test]
fn live_infinite_sleep_keeps_foreground_sibling_reload_queue_active() {
    let mut compiled = compile_with_context(
        "infinite-foreground-reload",
        r#"
local root
return Def.ActorFrame {
    InitCommand = function(self) root = self; FOREGROUND = self end,
    Def.Actor {
        InitCommand = function(self) PERSISTENT = self end,
        OnCommand = function(self) self:sleep(math.huge) end,
    },
    Def.Actor {
        OnCommand = function(self) self:queuecommand('WaitToAct') end,
        WaitToActCommand = function(self)
            if GAMESTATE:GetSongBeat() > 19 then
                SCREENMAN:SetNewScreen('ScreenGameplay')
            else
                self:sleep(0.01):queuecommand('WaitToAct')
            end
        end,
    },
}
"#,
        |context| {
            context.song_display_bpms = [120.0; 2];
            context.song_timing_bpms = vec![(0.0, 120.0)];
            context.layer_start_beats = vec![0.0];
            context.foreground_names = vec!["foreground".into()];
            context.defer_initial_live_update = true;
        },
    );
    let mut scene = compiled
        .live_scene
        .take()
        .expect("owned infinite foreground");
    assert!(scene.take_startup_output().errors.is_empty());
    let output = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.1,
            beat: 0.2,
            screen_seconds: 6.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    let persistent = scene_global_table(&scene, "PERSISTENT").unwrap();
    let foreground = scene_global_table(&scene, "FOREGROUND").unwrap();
    assert_eq!(
        scene_method_number(&scene, persistent, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        f32::INFINITY,
        "native infinite sleep retains foreground lifetime"
    );
    assert_eq!(
        scene_method_number(&scene, foreground, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        f32::INFINITY
    );
    assert!(output.request.is_none());
    let mut requested = false;
    for index in 2..=100 {
        let music_seconds = index as f64 * 0.1;
        let output = scene
            .advance(SongLuaLiveFrame {
                music_seconds,
                beat: music_seconds * 2.0,
                screen_seconds: 6.0 + music_seconds,
                delta_seconds: 0.1,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(
            scene_method_number(&scene, persistent, "GetTweenTimeLeft")
                .map(|value| value as f32)
                .unwrap(),
            f32::INFINITY
        );
        if let Some(request) = output.request {
            assert!(music_seconds * 2.0 > 19.0);
            assert!(
                matches!(request, SongLuaLiveRequest::NewScreen(ref name) if name == "ScreenGameplay")
            );
            requested = true;
            break;
        }
    }
    assert!(
        requested,
        "finite sibling queue reaches the authored reload beat beside an infinite sleep"
    );
}

#[test]
fn live_background_sprite_keeps_declared_texture_for_upper_left() {
    let mut compiled = compile_with_context(
        "background-upper-left-texture",
        r#"
return Def.Sprite {
    Texture=GAMESTATE:GetCurrentSong():GetSongDir()..'/image.png',
    InitCommand=function(self) BG_SPRITE = self end,
    OnCommand=function(self) self:aux(31) end,
}
"#,
        |context| {
            image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 255, 255, 255]))
                .save(context.song_dir.join("image.png"))
                .unwrap();
            context.background_layer_count = 1;
            context.layer_start_beats = vec![10.0];
            context.background_upper_left_colors = vec![Some([1.0; 4])];
        },
    );
    let mut scene = compiled.live_scene.take().unwrap();
    let sprite = scene_global_table(&scene, "BG_SPRITE").unwrap();
    assert!(scene_method_is_table(&scene, sprite, "GetTexture").unwrap());
    assert!(
        scene_method_is_table(&scene, sprite, "GetParent").unwrap(),
        "native UpperLeft owns its returned root inside a real ActorFrame"
    );
    let output = scene
        .advance(SongLuaLiveFrame {
            beat: 10.0,
            music_seconds: 10.0,
            screen_seconds: 10.0,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert!(scene_method_is_table(&scene, sprite, "GetTexture").unwrap());
}

#[test]
fn live_background_start_time_uses_native_note_row() {
    let mut compiled = compile_with_context(
        "background-rounded-start",
        r#"
return Def.Actor {
    InitCommand = function(self) BACKGROUND = self end,
    OnCommand = function(self) self:sleep(0.004):queuecommand('Done') end,
    DoneCommand = function(self) self:aux(self:getaux()+1) end,
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.background_layer_count = 1;
            context.layer_start_beats = vec![0.01];
            context.song_timing = Some(deadsync_rules::timing::TimingData::from_segments(
                0.0,
                0.0,
                &deadsync_rules::timing::TimingSegments {
                    bpms: vec![(0.0, 120.0)],
                    ..Default::default()
                },
                &[],
            ));
        },
    );
    let mut scene = compiled.live_scene.take().expect("retained background");
    // The authored marker selects the activation; elapsed time rounds
    // that marker to note row zero, consuming all 6ms on the first update.
    let output = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.006,
            beat: 0.012,
            screen_seconds: 0.006,
            delta_seconds: 0.001,
            ..Default::default()
        })
        .expect("background activation frame");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    let actor = scene_global_table(&scene, "BACKGROUND").expect("owned background");
    assert_eq!(
        scene_method_number(&scene, actor, "getaux")
            .map(|value| value as f32)
            .unwrap(),
        1.0,
        "background consumes delta from the native rounded start row"
    );
}

#[test]
fn live_native_float_foreground_activation_clock() {
    use deadsync_rules::timing::{TimingData, TimingSegments};
    for start_beat in [17.25f32, 20.5, 21.25, 44.75] {
        for music_rate in [1.0f32, 2.0] {
            let bpms = vec![
                (0.000000f32, 71.697998f32),
                (1.000000f32, 71.695999f32),
                (2.000000f32, 74.014000f32),
                (3.000000f32, 74.012001f32),
                (4.000000f32, 72.457001f32),
                (5.000000f32, 72.455002f32),
                (6.000000f32, 72.969002f32),
                (8.000000f32, 72.704002f32),
                (10.000000f32, 72.622002f32),
                (12.000000f32, 71.123001f32),
                (14.000000f32, 73.051003f32),
                (18.000000f32, 73.054001f32),
                (19.000000f32, 73.051003f32),
                (20.000000f32, 71.791000f32),
                (21.000000f32, 71.789001f32),
                (22.000000f32, 72.101997f32),
                (24.000000f32, 72.457001f32),
                (26.000000f32, 73.051003f32),
                (28.000000f32, 68.597000f32),
                (30.000000f32, 76.014999f32),
                (31.000000f32, 76.013000f32),
                (32.000000f32, 70.313004f32),
                (34.000000f32, 73.402000f32),
                (36.000000f32, 70.874001f32),
                (37.000000f32, 70.872002f32),
                (38.000000f32, 71.204002f32),
                (39.000000f32, 71.202003f32),
                (40.000000f32, 69.674004f32),
                (41.000000f32, 69.671997f32),
                (42.000000f32, 74.014000f32),
                (43.000000f32, 74.012001f32),
                (44.000000f32, 72.718002f32),
            ];
            let song_offset = -0.532426f32;
            let group_offset = -0.009f32;
            let global_offset = -0.008f32;
            let mut elapsed = -song_offset - group_offset;
            for (index, &(beat, bpm)) in bpms.iter().enumerate() {
                let end = bpms
                    .get(index + 1)
                    .map_or(start_beat, |point| point.0.min(start_beat));
                if end > beat {
                    elapsed += (end - beat) / (bpm / 60.0);
                }
            }
            // Captured native build a9d70b63 uses TimingData.cpp e93614b8:
            // GetElapsedTimeFromBeatNoOffset initializes last_time with the
            // separate song/group offsets before traversing timing events.
            let native_start = elapsed - music_rate * global_offset;
            let mut timing = TimingData::from_segments(
                -song_offset,
                global_offset,
                &TimingSegments {
                    bpms: bpms.clone(),
                    ..Default::default()
                },
                &[],
            );
            timing.shift_group_offset_seconds(group_offset);
            assert_eq!(
                timing
                    .get_song_time_for_beat(start_beat, music_rate)
                    .to_bits(),
                native_start.to_bits(),
                "captured native offsets initialize float event clock"
            );
            let mut compiled = compile_with_context(
                &format!("foreground-float-clock-{start_beat}-{music_rate}"),
                r#"
return Def.ActorFrame {
    Def.Actor {OnCommand=function(self) self:sleep(999) end},
    Def.Actor {
        InitCommand=function(self) FOREGROUND=self end,
        OnCommand=function(self) self:aux(1):sleep(1/128):queuecommand('Done') end,
        DoneCommand=function(self) self:aux(2) end,
    },
}
"#,
                |context| {
                    context.defer_initial_live_update = true;
                    context.layer_start_beats = vec![start_beat];
                    context.song_timing = Some(timing);
                    context.song_timing_bpms = bpms;
                    context.song_music_rate = music_rate;
                },
            );
            let mut scene = compiled
                .live_scene
                .take()
                .expect("foreground float event-clock scene");
            let first = native_start + music_rate / 128.0;
            let next = f32::from_bits(first.to_bits() + 1);
            for (music, delta) in [(first, 0.01f32), (next, next - first)] {
                let output = scene
                    .advance(SongLuaLiveFrame {
                        music_seconds: f64::from(music),
                        screen_seconds: f64::from(music),
                        delta_seconds: f64::from(delta),
                        beat: f64::from(start_beat + 0.01),
                        ..Default::default()
                    })
                    .unwrap();
                assert!(output.errors.is_empty(), "{:?}", output.errors);
            }
            let actor = scene_global_table(&scene, "FOREGROUND").unwrap();
            assert_eq!(
                scene_method_number(&scene, actor, "getaux")
                    .map(|value| value as f32)
                    .unwrap(),
                2.0,
                "foreground command must start from native float timing"
            );
        }
    }
}

#[test]
fn live_foreground_start_time_uses_native_note_row() {
    let mut compiled = compile_with_context(
        "foreground-rounded-start",
        r#"
return Def.Actor {
    InitCommand = function(self) FOREGROUND = self end,
    OnCommand = function(self) self:sleep(0.004):queuecommand('Done') end,
    DoneCommand = function(self) self:aux(self:getaux()+1) end,
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.01];
            context.song_timing = Some(deadsync_rules::timing::TimingData::from_segments(
                0.0,
                0.0,
                &deadsync_rules::timing::TimingSegments {
                    bpms: vec![(0.0, 120.0)],
                    ..Default::default()
                },
                &[],
            ));
        },
    );
    let mut scene = compiled.live_scene.take().expect("retained foreground");
    // Foreground checks the authored 0.01 beat, then GetElapsedTimeFromBeat
    // rounds its marker to row zero. Its first update consumes all 6ms.
    let output = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.006,
            beat: 0.012,
            screen_seconds: 0.006,
            delta_seconds: 0.001,
            ..Default::default()
        })
        .expect("foreground activation frame");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    let actor = scene_global_table(&scene, "FOREGROUND").expect("owned foreground");
    assert_eq!(
        scene_method_number(&scene, actor, "getaux")
            .map(|value| value as f32)
            .unwrap(),
        1.0,
        "foreground consumes delta from the native rounded start row"
    );
}

#[test]
fn live_screen_debug_owns_its_native_tween_clock() {
    let mut compiled = compile(
        "screen-debug",
        r#"
local top = SCREENMAN:GetTopScreen()
local text = top:GetChild('Debug')
assert(text == top:GetChildren().Debug)
assert(text:GetParent() == top)
assert(tostring(text):find('^BitmapText'))
assert(text:GetText() == '')
assert(text:GetX() == SCREEN_CENTER_X)
assert(text:GetY() == SCREEN_BOTTOM - 116)
assert(text:GetZoom() == 0.75)
LEFT = {}
return Def.ActorFrame { OnCommand = function(self)
    text:linear(1):y(SCREEN_CENTER_Y-60)
    self:SetUpdateFunction(function(_, delta)
        if delta > 0 then LEFT[#LEFT+1] = text:GetTweenTimeLeft() end
    end)
end }
"#,
    );
    let mut scene = compiled.live_scene.take().expect("owned scene");
    for seconds in [0.25, 0.5, 0.75, 1.0] {
        let output = scene
            .advance(SongLuaLiveFrame {
                screen_seconds: seconds,
                delta_seconds: 0.25,
                ..Default::default()
            })
            .expect("screen frame");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
    }
    let left = scene_global_table(&scene, "LEFT").expect("authored tween queries");
    for (index, expected) in [(1, 0.75), (2, 0.5), (3, 0.25), (4, 0.0)] {
        assert_eq!(
            scene_index_number(&scene, left, index)
                .map(|value| value as f32)
                .expect("tween left"),
            expected
        );
    }
}

#[test]
fn live_screen_layer_consumes_earlier_tween_before_later_message() {
    let mut compiled = compile(
        "screen-layer-tweens",
        r#"
local underlay = SCREENMAN:GetTopScreen():GetChild('Underlay')
LEFT = {}
return Def.ActorFrame { OnCommand = function(self)
    underlay:smooth(1.6):diffusealpha(0)
    local updates = 0
    self:SetUpdateFunction(function(self, delta)
        if delta <= 0 then return end
        updates = updates + 1
        if updates == 2 then MESSAGEMAN:Broadcast('StatsShow') end
        LEFT[#LEFT + 1] = underlay:GetTweenTimeLeft()
    end)
end, StatsShowMessageCommand = function(self)
    underlay:smooth(1.6):diffusealpha(1)
end }
"#,
    );
    let mut scene = compiled.live_scene.take().expect("owned scene");
    finish_note_field_intro_fixture(&scene);
    for seconds in [0.8, 1.8, 2.8] {
        let previous = if seconds == 0.8 { 0.0 } else { seconds - 1.0 };
        let output = scene
            .advance(SongLuaLiveFrame {
                screen_seconds: seconds,
                delta_seconds: seconds - previous,
                ..Default::default()
            })
            .expect("screen frame");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
    }
    let left = scene_global_table(&scene, "LEFT").expect("authored tween queries");
    for (index, expected) in [(1, 0.8), (2, 1.6), (3, 0.6)] {
        let actual = scene_index_number(&scene, left, index)
            .map(|value| value as f32)
            .expect("tween left");
        assert!(
            (actual - expected).abs() < 1e-6,
            "frame {index}: {actual} != {expected}"
        );
    }
}

#[test]
fn live_reload_resets_constructor_song_position() {
    let mut compiled = compile_with_context(
        "reload-song-position",
        r#"
CLOCK_SCENE = (CLOCK_SCENE or 0) + 1
local scene = CLOCK_SCENE
local bps = GAMESTATE:GetSongBPS()
assert(bps == 10, 'constructors observe SongPosition::Reset, including on reload')
assert(GAMESTATE:GetSongPosition():GetCurBPS() == 10)
assert(GAMESTATE:GetPlayerState(PLAYER_1):GetSongPosition():GetCurBPS() == 10)
return Def.ActorFrame {
    InitCommand=function(self) CLOCK_ACTOR = self end,
    OnCommand=function(self)
        assert(math.abs(GAMESTATE:GetSongBPS() - 155/60) < 0.00001,
            'the first update replaces reset BPS with song timing')
        self:smooth(8/bps):rotationz(90)
        if scene == 1 then SCREENMAN:SetNewScreen('ScreenGameplay') end
    end,
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
            context.song_timing_bpms = vec![(0.0, 155.0)];
        },
    );
    let mut scene = compiled.live_scene.take().expect("first scene");
    let frame = SongLuaLiveFrame::default();
    let output = scene.advance(frame.clone()).expect("first song update");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert_eq!(
        output.request,
        Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into()))
    );
    let mut layers = song::reload_song_lua(
        scene,
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None::<song::SongLuaOverlayModelLayer<()>>,
    )
    .expect("replacement constructors");
    let mut scene = layers[0].live_scene.take().expect("replacement scene");
    let output = scene.advance(frame).expect("replacement first update");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    let actor: deadlib_lua::Root =
        scene_global_table(&scene, "CLOCK_ACTOR").expect("replacement tween owner");
    let remaining = scene_method_number(&scene, actor, "GetTweenTimeLeft")
        .map(|value| value as f32)
        .unwrap();
    assert!(
        (remaining - 0.8).abs() < 1e-6,
        "replacement tween uses its cached construction BPS: {remaining}"
    );
}

#[test]
fn live_native_sound_pitch_preserves_identity_and_parameters() {
    let mut compiled = compile(
        "native-sound-pitch",
        r#"
local primary = Def.Sound { InitCommand = function(self)
    PRIMARY = self:get()
    assert(PRIMARY:pitch('1.25'):volume(0.5) == PRIMARY)
    assert(not pcall(PRIMARY.pitch, PRIMARY))
end }
local other = Def.Sound { InitCommand = function(self) OTHER = self:get() end }
return Def.ActorFrame { primary, other,
    OnCommand = function(self) self:SetUpdateFunction(function(self, delta)
        if delta > 0 then PRIMARY:pitch(-0.5); self:aux(23) end
    end) end,
}
"#,
    );
    let mut scene = compiled.live_scene.take().expect("sound scene");
    let primary = scene_global_table(&scene, "PRIMARY").unwrap();
    let other = scene_global_table(&scene, "OTHER").unwrap();
    assert_eq!(
        scene_field_number(&scene, primary, "__songlua_pitch", true)
            .map(|value| value as f32)
            .unwrap(),
        1.25
    );
    assert_eq!(
        scene_field_number(&scene, primary, "__songlua_volume", true)
            .map(|value| value as f32)
            .unwrap(),
        0.5
    );
    assert_eq!(
        scene_field_number(&scene, other, "__songlua_pitch", true)
            .map(|value| value as f32)
            .unwrap(),
        1.0
    );
    let output = scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert_eq!(
        scene_field_number(&scene, primary, "__songlua_pitch", true)
            .map(|value| value as f32)
            .unwrap(),
        -0.5
    );
    assert_eq!(
        scene_field_number(&scene, other, "__songlua_pitch", true)
            .map(|value| value as f32)
            .unwrap(),
        1.0
    );
    assert!(scene.states().iter().any(|state| state.aux == 23.0));
}

#[test]
fn live_sound_volume_keeps_the_sound_identity_and_owner() {
    let mut compiled = compile(
        "sound-volume",
        r#"
local primary = Def.Sound { InitCommand = function(self)
    PRIMARY_SOUND = self:get()
    assert(PRIMARY_SOUND == self:get())
    assert(PRIMARY_SOUND:volume('1.5') == PRIMARY_SOUND)
    assert(not pcall(PRIMARY_SOUND.volume, PRIMARY_SOUND, {}))
end }
local other = Def.Sound { InitCommand = function(self) OTHER_SOUND = self:get() end }
return Def.ActorFrame { primary, other,
    OnCommand = function(self)
        self:SetUpdateFunction(function(self, delta)
            if delta > 0 then
                assert(PRIMARY_SOUND:volume(0.25) == PRIMARY_SOUND)
                self:aux(19)
            end
        end)
    end,
}
"#,
    );
    let mut scene = compiled.live_scene.take().expect("retained sound scene");
    let primary: deadlib_lua::Root =
        scene_global_table(&scene, "PRIMARY_SOUND").expect("primary sound");
    let other: deadlib_lua::Root = scene_global_table(&scene, "OTHER_SOUND").expect("other sound");
    assert_ne!(
        scene_table_identity(&scene, primary).unwrap(),
        scene_table_identity(&scene, other).unwrap()
    );
    assert_eq!(
        scene_field_number(&scene, primary, "__songlua_volume", true)
            .map(|value| value as f32)
            .expect("unclamped native parameter"),
        1.5
    );
    assert_eq!(
        scene_field_number(&scene, other, "__songlua_volume", true)
            .map(|value| value as f32)
            .expect("default sound parameter"),
        1.0
    );
    let output = scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .expect("volume callback");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert_eq!(
        scene_field_number(&scene, primary, "__songlua_volume", true)
            .map(|value| value as f32)
            .expect("updated sound parameter"),
        0.25
    );
    assert_eq!(
        scene_field_number(&scene, other, "__songlua_volume", true)
            .map(|value| value as f32)
            .expect("independent other sound"),
        1.0
    );
    assert!(
        scene.states().iter().any(|state| state.aux == 19.0),
        "callback continues after volume"
    );
}

fn scene_index_field_read<R>(
    scene: &SongLuaLiveScene,
    owner: Root,
    index: usize,
    raw: bool,
    inspect: impl FnOnce(&mut Vm, Value) -> Result<R, Error>,
) -> Result<R, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let object = vm.rooted(owner)?;
    let key = Value::Number(index as f64);
    let value = if raw {
        let Value::Table(table) = object else {
            unreachable!("rooted fixture table")
        };
        vm.raw_table_get(table, key)?
    } else {
        let code = compile_named(
            b"local object,key=...;return object[key]",
            b"=existing live indexed fixture",
        )?;
        vm.execute_with_args(&code, &[object, key])?
            .first()
            .copied()
            .unwrap_or(Value::Nil)
    };
    inspect(vm, value)
}
fn scene_index_field_number(
    scene: &SongLuaLiveScene,
    owner: Root,
    index: usize,
    raw: bool,
) -> Result<f64, Error> {
    scene_index_field_read(scene, owner, index, raw, fixture_number)
}
fn scene_index_field_bool(
    scene: &SongLuaLiveScene,
    owner: Root,
    index: usize,
    raw: bool,
) -> Result<bool, Error> {
    scene_index_field_read(scene, owner, index, raw, |_, value| {
        Ok(!matches!(value, Value::Nil | Value::Boolean(false)))
    })
}

#[test]
fn live_song_clock_and_timing_queries_follow_current_music_rate() {
    let mut compiled = compile_with_context(
        "current-rate-clock",
        r#"
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function()
        local timing = GAMESTATE:GetCurrentSong():GetTimingData()
        CLOCK = {GAMESTATE:GetSongBeat(),
            GAMESTATE:GetSongPosition():GetSongBeat(),
            timing:GetBeatFromElapsedTime(1), timing:GetElapsedTimeFromBeat(2)}
    end)
end }
"#,
        |context| {
            context.song_timing = Some(deadsync_rules::timing::TimingData::from_segments(
                0.0,
                0.04,
                &deadsync_rules::timing::TimingSegments {
                    bpms: vec![(0.0, 120.0)],
                    ..Default::default()
                },
                &[],
            ));
        },
    );
    let mut scene = compiled.live_scene.take().unwrap();
    for rate in [1.0_f32, 1.25, 0.75, 1.0] {
        scene_exec(
            &scene,
            format!("GAMESTATE:ApplyGameCommand('mod,{rate}xmusic')"),
        )
        .unwrap();
        let output = scene
            .advance(SongLuaLiveFrame {
                music_seconds: 1.0,
                screen_seconds: 1.0,
                delta_seconds: 0.01,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let clock: deadlib_lua::Root = scene_global_table(&scene, "CLOCK").unwrap();
        // TimingData.cpp adds rate*globalOffset before the 120 BPM
        // traversal; its inverse subtracts it after the traversal.
        let beat = (1.0_f32 + rate * 0.04) * 2.0;
        for index in 1..=3 {
            assert_eq!(
                scene_index_field_number(&scene, clock, index, false)
                    .map(|value| value as f32)
                    .unwrap()
                    .to_bits(),
                beat.to_bits()
            );
        }
        assert_eq!(
            scene_index_field_number(&scene, clock, 4, false)
                .map(|value| value as f32)
                .unwrap()
                .to_bits(),
            (1.0_f32 - rate * 0.04).to_bits()
        );
    }
    scene_exec(
        &scene,
        r#"
assert(GAMESTATE:ApplyGameCommand('stagemod,0.75xmusic;mod,1.25xmusic') == GAMESTATE)
for _, level in ipairs({'Preferred','Stage','Song','Current'}) do
    assert(GAMESTATE:GetSongOptionsObject('ModsLevel_'..level):MusicRate() == 1.25)
end
GAMESTATE:ApplyGameCommand('mod,1.25xmusic;stagemod,0.75xmusic')
assert(GAMESTATE:GetSongOptionsObject('ModsLevel_Preferred'):MusicRate() == 1.25)
for _, level in ipairs({'Stage','Song','Current'}) do
    assert(GAMESTATE:GetSongOptionsObject('ModsLevel_'..level):MusicRate() == 0.75)
end
GAMESTATE:ApplyGameCommand('mod,50% drunk', 1)
for _, level in ipairs({'Preferred','Stage','Song','Current'}) do
    assert(GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_'..level):Drunk() == 0.5)
    assert(GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions('ModsLevel_'..level):Drunk() == 0)
end
"#,
    )
    .unwrap();
}

#[test]
fn live_uptime_advances_while_music_is_paused() {
    let mut compiled = compile(
        "uptime-paused",
        r#"
local last = nil
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function()
        local now = GetTimeSinceStart()
        if now == 0 then return end
        if last == nil then
            last = now
            SCREENMAN:GetTopScreen():PauseGame(true)
        elseif now - last > 0.25 then
            SCREENMAN:GetTopScreen():PauseGame(false)
        end
        CLOCK = {now, GAMESTATE:GetCurMusicSeconds(), SCREENMAN:GetTopScreen():IsPaused()}
    end)
end }
"#,
    );
    let mut scene = compiled.live_scene.take().expect("scene");
    for (uptime, paused) in [(40.1250001, true), (40.25, true), (40.5, false)] {
        let output = scene
            .advance(SongLuaLiveFrame {
                music_seconds: 12.0,
                screen_seconds: 1.0,
                delta_seconds: 0.125,
                uptime_seconds: Some(uptime),
                ..Default::default()
            })
            .expect("paused clock frame");
        assert!(output.errors.is_empty());
        let clock = scene_global_table(&scene, "CLOCK").expect("actual clock queries");
        assert_eq!(
            scene_index_field_number(&scene, clock, 1, false)
                .map(|value| value as f64)
                .expect("double process time"),
            uptime
        );
        assert_eq!(
            scene_index_field_number(&scene, clock, 2, false)
                .map(|value| value as f64)
                .expect("frozen music time"),
            12.0
        );
        assert_eq!(
            scene_index_field_bool(&scene, clock, 3, false).expect("pause state"),
            paused
        );
    }
    let output = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.0,
            screen_seconds: 1.125,
            delta_seconds: 0.125,
            ..Default::default()
        })
        .expect("fallback elapsed clock");
    assert!(output.errors.is_empty());
    let clock = scene_global_table(&scene, "CLOCK").expect("fallback clock queries");
    assert_eq!(
        scene_index_field_number(&scene, clock, 1, false)
            .map(|value| value as f64)
            .expect("process clock retains its epoch"),
        40.625
    );
}

#[test]
fn live_stats() {
    let mut compiled = compile(
        "stats",
        r#"
local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local stage = STATSMAN:GetCurStageStats()
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function()
        OBSERVED = {stats:GetActualDancePoints(), stats:GetPossibleDancePoints(),
            stats:GetCurrentPossibleDancePoints(), stats:GetCurrentLife(), stats:GetFailed(),
            stats:GetCurrentCombo(), stats:GetCurrentMissCombo(), stats:GetTapNoteScores('TapNoteScore_Miss'),
            stats:GetTapNoteScores('TapNoteScore_W1'), stats:GetTapNoteScores('TapNoteScore_HitMine'),
            stage:GetGameplaySeconds()}
    end)
end }
"#,
    );
    let mut scene = compiled.live_scene.take().expect("scene");
    let mut player = deadsync_gameplay::init_player_runtime();
    player.earned_grade_points = -24;
    player.current_possible_points = 20;
    player.life = 0.0;
    player.fail_time = Some(2.0);
    player.combo = 3;
    player.miss_combo = 2;
    player.judgment_counts = [3, 0, 0, 0, 0, 6];
    player.scoring_counts = [0, 0, 0, 0, 0, 2];
    player.mines_hit = 1;
    let output = scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 3.5,
            delta_seconds: 0.5,
            players: [
                SongLuaLivePlayer::from_player(&player, 25),
                SongLuaLivePlayer::default(),
            ],
            ..SongLuaLiveFrame::default()
        })
        .expect("actual stage snapshot");
    assert!(output.errors.is_empty());
    let observed = scene_global_table(&scene, "OBSERVED").expect("callback values");
    assert_eq!(
        scene_index_field_number(&scene, observed, 1, false)
            .map(|value| value as i32)
            .expect("actual"),
        -24
    );
    assert_eq!(
        scene_index_field_number(&scene, observed, 2, false)
            .map(|value| value as i32)
            .expect("maximum"),
        25
    );
    assert_eq!(
        scene_index_field_number(&scene, observed, 3, false)
            .map(|value| value as i32)
            .expect("progress"),
        20
    );
    assert_eq!(
        scene_index_field_number(&scene, observed, 4, false)
            .map(|value| value as f32)
            .expect("life"),
        0.0
    );
    assert!(scene_index_field_bool(&scene, observed, 5, false).expect("failure"));
    for (index, expected) in [(6, 3), (7, 2), (8, 6), (9, 3), (10, 1)] {
        assert_eq!(
            scene_index_field_number(&scene, observed, index, false)
                .map(|value| value as u32)
                .expect("native count"),
            expected
        );
    }
    assert_eq!(
        scene_index_field_number(&scene, observed, 11, false)
            .map(|value| value as f64)
            .expect("screen seconds"),
        3.5
    );
}

// Collection adapters use only the tested VM and production host readers.
fn scene_optional_global_table(
    scene: &SongLuaLiveScene,
    name: impl AsRef<[u8]>,
) -> Result<Option<Root>, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let value = vm.get_global(name.as_ref())?;
    if value == Value::Nil {
        return Ok(None);
    }
    assert!(matches!(value, Value::Table(_)), "optional fixture table");
    vm.pin(value).map(Some)
}
fn scene_field_table(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: impl AsRef<[u8]>,
    raw: bool,
) -> Result<Root, Error> {
    scene_table_read(
        scene,
        owner,
        name,
        if raw {
            TableRead::Raw
        } else {
            TableRead::Field
        },
        |vm, value| {
            assert!(matches!(value, Value::Table(_)), "fixture table");
            vm.pin(value)
        },
    )
}
fn scene_index_table(scene: &SongLuaLiveScene, owner: Root, index: usize) -> Result<Root, Error> {
    scene_index_field_read(scene, owner, index, true, |vm, value| {
        assert!(matches!(value, Value::Table(_)), "indexed fixture table");
        vm.pin(value)
    })
}
fn scene_index_string(
    scene: &SongLuaLiveScene,
    owner: Root,
    index: usize,
) -> Result<String, Error> {
    scene_index_field_read(scene, owner, index, true, fixture_string)
}
fn scene_sequence_tables(scene: &SongLuaLiveScene, owner: Root) -> Result<Vec<Root>, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let Value::Table(table) = vm.rooted(owner)? else {
        unreachable!()
    };
    let mut values = Vec::new();
    for index in 1.. {
        let value = vm.raw_table_get(table, Value::Number(index as f64))?;
        if value == Value::Nil {
            break;
        }
        assert!(matches!(value, Value::Table(_)), "fixture sequence table");
        values.push(vm.pin(value)?)
    }
    Ok(values)
}
fn fixture_strings(vm: &mut Vm, value: Value) -> Result<Vec<String>, Error> {
    let Value::Table(table) = value else {
        panic!("fixture string sequence")
    };
    let pin = vm.pin(value)?;
    let result = (|| {
        let mut values = Vec::new();
        for index in 1.. {
            let value = vm.raw_table_get(table, Value::Number(index as f64))?;
            if value == Value::Nil {
                break;
            }
            values.push(fixture_string(vm, value)?)
        }
        Ok(values)
    })();
    vm.unpin(pin)?;
    result
}
fn scene_global_strings(
    scene: &SongLuaLiveScene,
    name: impl AsRef<[u8]>,
) -> Result<Vec<String>, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let value = vm.get_global(name.as_ref())?;
    fixture_strings(vm, value)
}
fn scene_expression_strings(scene: &SongLuaLiveScene, source: &str) -> Result<Vec<String>, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let code = compile_named(source.as_bytes(), b"=existing live collection fixture")?;
    let value = vm.execute(&code)?.first().copied().unwrap_or(Value::Nil);
    fixture_strings(vm, value)
}
fn scene_scale(scene: &SongLuaLiveScene, args: [f32; 5]) -> Result<f32, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let function = vm.get_global(b"scale")?;
    let args = args.map(|x| Value::Number(f64::from(x)));
    let value = vm
        .call(function, &args)?
        .first()
        .copied()
        .unwrap_or(Value::Nil);
    fixture_number(vm, value).map(|value| value as f32)
}
fn scene_set_number(
    scene: &SongLuaLiveScene,
    name: impl AsRef<[u8]>,
    value: f64,
) -> Result<(), Error> {
    scene
        .runtime()
        .vm_mut()
        .set_global(name.as_ref(), Value::Number(value))
}
fn scene_set_bools(
    scene: &SongLuaLiveScene,
    name: impl AsRef<[u8]>,
    values: &[bool],
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let table = vm.try_new_table()?;
    let pin = vm.pin(Value::Table(table))?;
    let result = (|| {
        for (index, &value) in values.iter().enumerate() {
            vm.raw_table_set(
                table,
                Value::Number((index + 1) as f64),
                Value::Boolean(value),
            )?
        }
        vm.set_global(name.as_ref(), Value::Table(table))
    })();
    vm.unpin(pin)?;
    result
}
fn scene_state(scene: &SongLuaLiveScene, actor: Root) -> Result<SongLuaOverlayState, Error> {
    let mut runtime = scene.runtime();
    let value = runtime.vm_mut().rooted(actor)?;
    song::test_support::read_live_actor_state(&mut runtime, value)
}
fn scene_has_function(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: impl AsRef<[u8]>,
) -> Result<bool, Error> {
    scene_table_read(scene, owner, name, TableRead::Raw, |_, value| {
        Ok(matches!(value, Value::Function(_) | Value::Native(_)))
    })
}

#[test]
fn live_native_float_scale_operation_order() {
    let compiled = compile("native-scale-operation-order", "return Def.Actor {}");
    let scene = compiled.live_scene.expect("native scale host");
    let width = 480.0f32 * (16.0f32 / 9.0f32);
    let actual = scene_scale(&scene, [width, 640.0f32, 854.0f32, -8.0f32, 20.0f32]).unwrap();
    // RageUtil.h SCALE multiplies before dividing. LuaManager.cpp casts
    // all five inputs to float before invoking that macro.
    let native = (width - 640.0f32) * (20.0f32 - (-8.0f32)) / (854.0f32 - 640.0f32) + (-8.0f32);
    assert_eq!(
        actual.to_bits(),
        native.to_bits(),
        "native SCALE must multiply before dividing without fused addition"
    );
}

#[test]
fn live_outro_dispatches_off_after_actor_update_once() {
    for failed in [false, true] {
        let mut compiled = compile_with_context(
            &format!("outro-off-{failed}"),
            r#"
OFFS, SEEN = 0, {}
return Def.ActorFrame {
    OnCommand = function(self)
        self:sleep(10):SetUpdateFunction(function(_, delta)
            if delta > 0 then SEEN[#SEEN + 1] = OFFS end
        end)
    end,
    OffCommand = function(self) OFFS = OFFS + 1; self:stoptweening() end,
    EndCommand = function(self) self:aux(23) end,
}
"#,
            |context| context.players[1].enabled = false,
        );
        let mut scene = compiled.live_scene.take().expect("scene");
        let mut frame = SongLuaLiveFrame {
            delta_seconds: 0.125,
            ..Default::default()
        };
        frame.players[0].failed = failed;
        for index in 0..5 {
            frame.screen_seconds = (index + 1) as f64 * 0.125;
            frame.gameplay_outro = index >= 1;
            scene.advance(frame.clone()).expect("outro frame");
            let expected = i64::from(index >= if failed { 3 } else { 2 });
            assert_eq!(
                scene_global_number(&scene, "OFFS")
                    .map(|value| value as i64)
                    .expect("Off count"),
                expected,
                "NotesEnded dispatches after actors; BeginFailed waits another update"
            );
        }
        let seen: deadlib_lua::Root =
            scene_global_table(&scene, "SEEN").expect("callback observations");
        assert_eq!(
            scene_index_number(&scene, seen, if failed { 4 } else { 3 })
                .map(|value| value as i64)
                .expect("same-frame observation"),
            0
        );
        assert_eq!(
            scene_index_number(&scene, seen, 5)
                .map(|value| value as i64)
                .expect("following-frame observation"),
            1
        );
        scene.end().expect("End is separate");
        assert_eq!(
            scene_global_number(&scene, "OFFS")
                .map(|value| value as i64)
                .expect("no repeated Off"),
            1
        );
    }
}

#[test]
fn live_loading_screen_is_unavailable_until_begin() {
    for loading in [false, true] {
        let mut compiled = compile_with_context(
            if loading {
                "loading-screen"
            } else {
                "active-screen"
            },
            r#"
CONSTRUCTION_TOP = SCREENMAN:GetTopScreen()
return Def.ActorFrame {
    InitCommand = function(self)
        INIT_TOP = SCREENMAN:GetTopScreen()
        INIT_PARENT_SCREEN = self:GetParent():GetParent()
    end,
    BeginCommand = function(self) BEGIN_TOP = SCREENMAN:GetTopScreen() end,
    OnCommand = function(self)
        ON_TOP = SCREENMAN:GetTopScreen()
        self:aux(17)
    end,
    EndCommand = function(self) END_TOP = SCREENMAN:GetTopScreen() end,
}
"#,
            |context| {
                context.screen_loading = loading;
                context.defer_initial_live_update = true;
                context.layer_start_beats = vec![0.0];
            },
        );
        let mut scene = compiled.live_scene.take().expect("screen lifecycle scene");

        let screen = scene_global_table(&scene, "__songlua_top_screen").unwrap();
        for name in ["CONSTRUCTION_TOP", "INIT_TOP"] {
            let top = scene_optional_global_table(&scene, name).unwrap();
            if loading {
                assert!(
                    top.is_none(),
                    "{name} must not expose a screen before it is pushed"
                );
            } else {
                assert_eq!(
                    scene_table_identity(&scene, top.unwrap()).unwrap(),
                    scene_table_identity(&scene, screen).unwrap()
                );
            }
        }
        assert_eq!(
            scene_table_identity(
                &scene,
                scene_global_table(&scene, "INIT_PARENT_SCREEN").unwrap()
            )
            .unwrap(),
            scene_table_identity(&scene, screen).unwrap(),
            "the loading screen owns its actors before it becomes the top screen"
        );
        assert_eq!(
            scene_table_identity(&scene, scene_global_table(&scene, "BEGIN_TOP").unwrap()).unwrap(),
            scene_table_identity(&scene, screen).unwrap()
        );
        let output = scene
            .advance(SongLuaLiveFrame {
                music_seconds: 0.1,
                beat: 0.1,
                screen_seconds: 0.1,
                delta_seconds: 0.1,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(
            scene_table_identity(&scene, scene_global_table(&scene, "ON_TOP").unwrap()).unwrap(),
            scene_table_identity(&scene, screen).unwrap()
        );
        assert_eq!(scene.states()[0].aux, 17.0);
        scene.end().unwrap();
        assert!(
            scene_optional_global_table(&scene, "END_TOP")
                .unwrap()
                .is_none(),
            "ScreenManager pops the screen before End"
        );
    }
}

#[test]
fn live_player_combo_uses_current_theme_font_resources() {
    for populated in [false, true] {
        let source = r#"
COMBOS = {}
return Def.Actor { OnCommand = function()
    for index = 1, 2 do
        local player = SCREENMAN:GetTopScreen():GetChild('PlayerP' .. index)
        local combo = player:GetChild('Combo')
        assert(combo:GetParent() == player and combo:GetName() == 'Combo')
        if $POPULATED then
            assert(tostring(combo):find('ActorFrame', 1, true))
            local number = combo:GetChild('Number')
            assert(tostring(number):find('BitmapText', 1, true))
            assert(number:GetParent() == combo and combo:GetNumChildren() == 3)
            local hundred = combo:GetChild('OneHundredMilestone')
            local thousand = combo:GetChild('OneThousandMilestone')
            assert(hundred:GetNumChildren() == 4 and thousand:GetNumChildren() == 3)
            assert(hundred:GetParent() == combo and thousand:GetParent() == combo)
            assert(hundred:GetVisible() == false and thousand:GetVisible() == false)
            local nested = thousand:GetChildren()[''][1]
            assert(nested:GetNumChildren() == 4 and nested:GetParent() == thousand)
            for _, sprite in ipairs(hundred:GetChildren()['']) do
                assert(tostring(sprite):find('Sprite', 1, true) and sprite:GetRotationZ() == 0)
                assert(sprite:GetDiffuseAlpha() == 0)
            end
            hundred:playcommand('HundredMilestone')
            assert(hundred:GetVisible() == true and hundred:GetTweenTimeLeft() > 0)
            hundred:playcommand('Hide')
            assert(hundred:GetVisible() == false)
            assert(combo:GetChildren().Number == number)
        else
            assert(not tostring(combo):find('ActorFrame', 1, true),
                'Combo without current-theme fonts must be a leaf Actor')
            assert(combo:GetVisible() == false and combo.GetNumChildren == nil)
        end
        assert(SL['P' .. index].ActiveModifiers.HideCombo == false)
        COMBOS[index] = combo
    end
end }
"#
        .replace("$POPULATED", if populated { "true" } else { "false" });
        let compiled = compile_with_context(
            &format!("combo-current-theme-resources-{populated}"),
            &source,
            |context| {
                let fonts = context
                    .song_dir
                    .join(song::SONG_LUA_THEME_PATH_PREFIX)
                    .join("Fonts/_Combo Fonts");
                if populated {
                    for name in [
                        "Asap",
                        "Wendy",
                        "Wendy (Cursed)",
                        "MissingIni",
                        "MissingPng",
                        "WrongName",
                    ] {
                        let folder = fonts.join(name);
                        fs::create_dir_all(&folder).unwrap();
                        if name != "MissingIni" {
                            fs::write(folder.join(format!("{name}.ini")), "[main]\n").unwrap();
                        }
                        if name != "MissingPng" {
                            let stem = if name == "WrongName" { "Other" } else { name };
                            fs::write(folder.join(format!("{stem} 4x4.png")), b"").unwrap();
                        }
                    }
                }
            },
        );
        let scene = compiled.live_scene.expect("owned Combo resource selection");
        let combos = scene_global_table(&scene, "COMBOS").unwrap();
        assert_eq!(
            scene_table_len(&scene, combos).unwrap(),
            2,
            "inspect both configured players after On"
        );
        assert_ne!(
            scene_table_identity(&scene, scene_index_table(&scene, combos, 1).unwrap()).unwrap(),
            scene_table_identity(&scene, scene_index_table(&scene, combos, 2).unwrap()).unwrap()
        );
        let fonts = scene_expression_strings(&scene, "return GetComboFonts()").unwrap();
        assert_eq!(
            fonts,
            if populated {
                vec!["Wendy", "Asap", "Wendy (Cursed)"]
            } else {
                vec![]
            }
        );
    }
}

#[test]
fn live_native_tween_curves_match_native_probes() {
    let mut compiled = compile_with_context(
        "native-tween-curves",
        r#"
ACTORS = {}
local function actor(name, kind, curve)
    return Def.Actor { Name=name,
        InitCommand=function(self) ACTORS[name]=self;self:x(0) end,
        OnCommand=function(self)
            if name == 'drop' then
                assert(self:drop(1) == self)
                self:x(100)
            else self:tween(1,kind,curve):x(100) end
        end }
end
return Def.ActorFrame { InitCommand=function(self) TWEEN_ROOT=self end,
    actor('linear',0,0), actor('accelerate','TweenType_Accelerate'),
    actor('decelerate','DECELERATE'), actor('spring',3),
    actor('bezier4',4,{0.2,-0.3,1.4,1.3}),
    actor('bezier8','TweenType_Bezier',{-0.2,0.1,0.1,-0.4,0.85,1.4,1.2,0.9}),
    actor('drop') }
"#,
        |context| context.defer_initial_live_update = true,
    );
    let mut scene = compiled.live_scene.take().unwrap();
    let output = scene.advance(SongLuaLiveFrame::default()).unwrap();
    assert!(
        output.errors.is_empty(),
        "native tween callbacks: {:?}",
        output.errors
    );
    scene_exec(
        &scene,
        r#"
TWEEN_ROOT:hurrytweening(2)
for _,a in pairs(ACTORS) do assert(a:GetTweenTimeLeft()==2) end
TWEEN_ROOT:hurrytweening(0.5)
for _,a in pairs(ACTORS) do assert(a:GetTweenTimeLeft()==1) end
"#,
    )
    .unwrap();
    // ITGmania actor-conformance curve_probe values, generated by the
    // native ITween factory rather than the owned curve evaluator.
    let probes = [
        ("linear", [0.0, 0.25, 0.5, 0.75, 1.0]),
        ("accelerate", [0.0, 0.0625, 0.25, 0.5625, 1.0]),
        ("decelerate", [0.0, 0.4375, 0.75, 0.9375, 1.0]),
        ("spring", [0.0, 1.2186763, 1.2828426, 0.71572936, 1.0000001]),
        ("bezier4", [0.2, 0.17500003, 0.6000001, 1.0999999, 1.3]),
        (
            "bezier8",
            [0.026412196, 0.22954142, 0.5226672, 0.80551004, 0.98091364],
        ),
        ("drop", [0.0, 0.5078125, 0.6875, 0.7734375, 1.0]),
    ];
    for step in 1..=3 {
        let seconds = f64::from(step) * 0.25;
        let output = scene
            .advance(SongLuaLiveFrame {
                beat: seconds,
                music_seconds: seconds,
                screen_seconds: seconds,
                delta_seconds: 0.25,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let actors = scene_global_table(&scene, "ACTORS").unwrap();
        for (name, expected) in probes {
            let actor = scene_field_table(&scene, actors, name, false).unwrap();
            let actual = scene_method_number(&scene, actor, "GetX")
                .map(|value| value as f32)
                .unwrap();
            assert!(
                (actual - expected[step as usize] * 100.0).abs() < 0.0001,
                "native curve {name} at {seconds}: {actual}"
            );
        }
    }
    for (curve, expected) in [
        ("bezier(0.2,-0.3,1.4,1.3)", probes[4].1),
        ("bezier(-0.2,0.1,0.1,-0.4,0.85,1.4,1.2,0.9)", probes[5].1),
        ("bezier(0,0,0.33333334,1,0.6666667,0.5,1,1)", probes[6].1),
    ] {
        for (index, expected) in expected.into_iter().enumerate() {
            let factor = deadsync_gameplay::song_lua_ease_factor(
                Some(curve),
                index as f32 * 0.25,
                None,
                None,
            );
            assert!(
                (factor - expected).abs() < 0.000001,
                "native probe {curve}: {factor}"
            );
        }
    }
    scene_exec(
        &scene,
        r#"
local a=ACTORS.linear
a:hurrytweening(0.5)
assert(a:GetTweenTimeLeft()==0.125)
assert(a:GetX()==75, 'hurry retains the current interpolation fraction')
"#,
    )
    .unwrap();
    scene
        .advance(SongLuaLiveFrame {
            beat: 0.8125,
            music_seconds: 0.8125,
            screen_seconds: 0.8125,
            delta_seconds: 0.0625,
            ..Default::default()
        })
        .unwrap();
    scene_exec(
        &scene,
        r#"
local a=ACTORS.linear
assert(a:GetX()==87.5)
a:hurrytweening(0)
assert(a:GetTweenTimeLeft()==0 and a:GetX()==87.5)
assert(not pcall(function() a:hurrytweening(-1) end))
"#,
    )
    .unwrap();
    scene
        .advance(SongLuaLiveFrame {
            beat: 0.8225,
            music_seconds: 0.8225,
            screen_seconds: 0.8225,
            delta_seconds: 0.01,
            ..Default::default()
        })
        .unwrap();
    scene_exec(&scene, "assert(ACTORS.linear:GetX()==100)").unwrap();
}

#[test]
fn live_screen_direct_and_queued_commands_reach_song_children() {
    let mut compiled = compile(
        "screen-command-children",
        r#"
CALLS={}; PHASE='immediate'; MARKER={value=17}
local function probe(tag)
    return function(self, params)
        if PHASE=='immediate' then assert(params==MARKER) else assert(params==nil) end
        CALLS[#CALLS+1]=tag..':'..PHASE
    end
end
return Def.ActorFrame {
    ScreenCommandProbeCommand=probe('root'),
    OnCommand=function(self)
        self:sleep(1000)
        local screen=SCREENMAN:GetTopScreen()
        screen:playcommand('ScreenCommandProbe', MARKER)
        PHASE='queued'
        screen:sleep(0.25):queuecommand('ScreenCommandProbe')
    end,
    Def.ActorFrame { ScreenCommandProbeCommand=probe('nested'),
        Def.Actor { ScreenCommandProbeCommand=probe('leaf') },
    },
}
"#,
    );
    let scene = compiled.live_scene.as_mut().expect("owned scene");
    let startup = scene.take_startup_output();
    assert!(startup.errors.is_empty(), "{:?}", startup.errors);
    let calls = || scene_global_strings(scene, "CALLS").unwrap();
    assert_eq!(
        calls(),
        ["root:immediate", "nested:immediate", "leaf:immediate"],
        "native Screen commands must reach nested song actors"
    );
    let early = scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.125,
            delta_seconds: 0.125,
            ..Default::default()
        })
        .expect("before queued command");
    assert!(early.errors.is_empty(), "{:?}", early.errors);
    assert_eq!(
        scene_table_len(scene, scene_global_table(scene, "CALLS").unwrap()).unwrap(),
        3
    );
    let due = scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.375,
            delta_seconds: 0.25,
            ..Default::default()
        })
        .expect("queued screen command");
    assert!(due.errors.is_empty(), "{:?}", due.errors);
    let calls = scene_global_strings(scene, "CALLS").unwrap();
    assert_eq!(
        calls,
        [
            "root:immediate",
            "nested:immediate",
            "leaf:immediate",
            "root:queued",
            "nested:queued",
            "leaf:queued"
        ],
        "the screen queue advances independently of its sleeping descendants"
    );
    scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.5,
            delta_seconds: 0.125,
            ..Default::default()
        })
        .expect("later update");
    assert_eq!(
        scene_table_len(scene, scene_global_table(scene, "CALLS").unwrap()).unwrap(),
        6,
        "each descendant receives each direct command once"
    );
}

#[test]
fn live_simply_love_sm_broadcasts() {
    let mut compiled = compile(
        "simply-love-sm-broadcasts",
        r#"
return Def.Actor {
    OnCommand=function(self)
        RECEIVED={}; TRACED={}
        Trace=function(msg)
            assert(#RECEIVED==#TRACED+1, 'SM broadcasts before tracing')
            TRACED[#TRACED+1]=msg
        end
        SM('hello')
        SM(17, 0, false)
        SM(false, 2.5, true)
        SM(nil)
        local cycle={}; cycle[1]=cycle; SM(cycle)
        SM({'nested'})
    end,
    SystemMessageMessageCommand=function(self, params)
        RECEIVED[#RECEIVED+1]=params
        params.Message='listener-mutated'
    end,
}
"#,
    );
    let scene = compiled.live_scene.as_mut().expect("owned scene");
    let output = scene.take_startup_output();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert_eq!(
        output.source_broadcasts.len(),
        6,
        "SM must dispatch Simply Love SystemMessage broadcasts"
    );
    let expected = [
        serde_json::json!({"Message":"hello"}),
        serde_json::json!({"Message":"17", "Duration":0, "Stack":false}),
        serde_json::json!({"Message":"false", "Duration":2.5, "Stack":true}),
        serde_json::json!({"Message":"nil"}),
        serde_json::json!({"Message":"Value = {\n|    [1] = {} -- Value (self reference)\n}"}),
        serde_json::json!({"Message":"Value = {\n|    [1] = \"nested\"\n}"}),
    ];
    let received: deadlib_lua::Root =
        scene_global_table(&scene, "RECEIVED").expect("listeners ran");
    let traced: deadlib_lua::Root = scene_global_table(&scene, "TRACED").expect("trace calls");
    assert_eq!(scene_table_len(&scene, received).unwrap(), 6);
    assert_eq!(scene_table_len(&scene, traced).unwrap(), 6);
    for (index, (event, expected)) in output.source_broadcasts.iter().zip(expected).enumerate() {
        assert_eq!(event.name, "SystemMessage");
        assert_eq!(
            event.params.as_ref().expect("trace params"),
            &Some(expected.clone())
        );
        assert_eq!(
            scene_index_string(scene, traced, index + 1).expect("original Trace value"),
            expected["Message"].as_str().unwrap()
        );
        assert_eq!(
            scene_field_string(
                scene,
                scene_index_table(scene, received, index + 1).unwrap(),
                "Message",
                true
            )
            .unwrap(),
            "listener-mutated"
        );
    }
    scene_exec(
        &scene,
        r#"
            assert(TableToString({}, 'Root', '>')=='>Root = {}')
            local original=TableToString
            TableToString=function(value) assert(value.flag); return 'custom formatter' end
            SM({flag=true}, 4, false)
            assert(TRACED[7]=='custom formatter')
            TableToString=original
        "#,
    )
    .expect("SM uses the current public formatter");
}

#[test]
fn live_song_clock() {
    let mut compiled = compile_with_context(
        "song-clock",
        r#"
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function()
        CLOCK = {GAMESTATE:GetSongBeat(), GAMESTATE:GetSongPosition():GetSongBeat(),
            GAMESTATE:GetCurMusicSeconds(), GAMESTATE:GetSongBPS(), GAMESTATE:GetSongPosition():GetCurBPS()}
    end)
end }
"#,
        |context| {
            // Native SongPosition adds the machine offset and group sync offset.
            // A 120 BPM chart has different beats from this 60 BPM song clock.
            context.song_timing = Some(deadsync_rules::timing::TimingData::from_segments(
                0.009,
                -0.008,
                &deadsync_rules::timing::TimingSegments {
                    bpms: vec![(0.0, 60.0), (2.0, 120.0)],
                    ..Default::default()
                },
                &[],
            ));
        },
    );
    let mut scene = compiled.live_scene.take().expect("scene");
    for (seconds, player_beat, expected, bps) in [(1.017, 2.0, 1.0, 1.0), (3.017, 6.0, 4.0, 2.0)] {
        let output = scene
            .advance(SongLuaLiveFrame {
                beat: player_beat,
                music_seconds: seconds,
                screen_seconds: seconds,
                delta_seconds: 0.1,
                ..Default::default()
            })
            .expect("live frame");
        assert!(output.errors.is_empty());
        let values = scene_global_table(&scene, "CLOCK").expect("actual Lua queries");
        for index in [1, 2] {
            let actual = scene_index_field_number(&scene, values, index, false)
                .map(|value| value as f64)
                .expect("song beat");
            assert!(
                (actual - expected).abs() < 1e-5,
                "query {index}: seconds={seconds} song={actual} player={player_beat} expected={expected}"
            );
        }
        assert!(
            (scene_index_field_number(&scene, values, 3, false)
                .map(|value| value as f64)
                .expect("music seconds")
                - seconds)
                .abs()
                < 1e-5
        );
        for index in [4, 5] {
            assert_eq!(
                scene_index_field_number(&scene, values, index, false)
                    .map(|value| value as f64)
                    .expect("current BPS"),
                bps
            );
        }
    }
}

#[test]
fn live_step_statistics_judgment_labels_follow_initial_timing_windows() {
    for (name, windows, count) in [
        ("all", [true; 5], 12),
        ("no-way-off", [true, true, true, true, false], 11),
        ("casual", [true, true, true, false, false], 10),
        ("no-top-two", [false, false, true, true, true], 10),
    ] {
        let mut compiled = compile_with_context(
            &format!("timing-window-labels-{name}"),
            "return Def.ActorFrame {}",
            |context| {
                context.screen_width = 854.0;
                context.defer_initial_live_update = true;
                context.players[0].timing_windows = windows;
                context.players[1].timing_windows = [true; 5];
            },
        );
        let scene = compiled
            .live_scene
            .take()
            .expect("real StepStatistics scene");
        scene_set_number(&scene, "EXPECTED_COUNT", count as f64).unwrap();
        scene_set_bools(&scene, "EXPECTED_WINDOWS", &windows).unwrap();
        scene_exec(&scene,r#"
local underlay=SCREENMAN:GetTopScreen():GetChild('Underlay')
for player=1,2 do
    local expected=player==1 and EXPECTED_WINDOWS or {true,true,true,true,true}
    local actual=SL['P'..player].ActiveModifiers.TimingWindows
    for i=1,5 do
        assert(actual[i]==expected[i], 'SL must use each player initial timing-window inputs')
    end
    local judgments=underlay:GetChild('StepStatsPaneP'..player):GetChild('BannerAndData'):GetChild('TapNoteJudgments')
    assert(judgments:GetNumChildren()==(player==1 and EXPECTED_COUNT or 12),
        'real native judgment label actors must follow enabled timing windows')
    local children=judgments:GetChild('')
    local labels={}
    local values=0
    for _,child in ipairs(children) do
        local text=child:GetText()
        if text=='0000' then values=values+1 else labels[text]=true end
    end
    assert(values==6, 'all six native judgment counters remain present')
    for i=1,5 do
        local label=THEME:GetString('TapNoteScore','W'..i):upper()
        assert((labels[label] or false)==expected[i], 'enabled judgment label must exist; disabled label must be absent')
    end
    assert(labels[THEME:GetString('TapNoteScore','Miss'):upper()], 'Miss label always exists')
end
"#).unwrap();
    }
}

#[test]
fn live_empty_bitmap_text_zoomtoheight_sets_native_zoom() {
    let mut compiled = compile(
        "empty-text-zoomtoheight",
        r#"
return Def.BitmapText {
    Font = 'Common normal',
    InitCommand = function(self)
        TEXT = self
        assert(self:GetText() == '' and self:GetHeight() == 1)
        self:zoomtoheight(0.004 * SCREEN_HEIGHT)
        local expected = 1.9199999570846558
        assert(self:GetZoomY() == expected, 'native ZoomToHeight changes zoom before SetText')
        self:zoomx(self:GetZoomY())
        assert(self:GetZoomX() == expected and self:GetZoomedHeight() == expected)
        self:zoomtoheight(3)
        assert(self:GetHeight() == 1 and self:GetZoomY() == 3,
            'repeated sizing divides by the original unzoomed height')
        self:settext('1:59')
        assert(self:GetZoomY() == 3, 'later text assignment preserves the authored zoom')
    end,
}
"#,
    );
    let mut scene = compiled.live_scene.take().expect("live timer text");
    let output = scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.1,
            music_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    let text = scene_global_table(&scene, "TEXT").unwrap();
    let state = scene_state(&scene, text).unwrap();
    assert_eq!(state.zoom_x, 1.92);
    assert_eq!(state.zoom_y, 3.0);
    assert_eq!(
        state.size, None,
        "empty text sizing is a zoom, not a future font fit"
    );
}

#[test]
fn live_underlay_background_filter_uses_initial_profile_inputs() {
    for (style, p1, p2, enabled, expected_width, expected_count) in [
        ("single", 75, 0, false, 256, 1),
        ("single", 50, 95, true, 256, 2),
        ("double", 37, 0, false, 512, 1),
        ("routine", 75, 95, true, 512, 1),
        ("single", 0, 0, false, 256, 0),
    ] {
        let source = r#"
FILTERS = {}
return Def.Actor { OnCommand = function()
    local underlay = SCREENMAN:GetTopScreen():GetChild('Underlay')
    local unnamed = underlay:GetChildren()['']
    if unnamed and (tostring(unnamed):find('Sprite', 1, true) or tostring(unnamed):find('ActorFrame', 1, true)) then unnamed = {unnamed} end
    for _, child in ipairs(unnamed or {}) do
        if tostring(child):find('Sprite', 1, true) then
            assert(child:GetParent() == underlay and child:GetName() == '')
            assert(child:GetZoomedWidth() == $WIDTH and child:GetZoomedHeight() == SCREEN_HEIGHT)
            assert(child:GetTexture() ~= nil)
            FILTERS[#FILTERS+1] = child
        end
    end
    assert(#FILTERS == $COUNT, 'Underlay must load the profile-selected BackgroundFilter Quad')
end }
"#.replace("$WIDTH", &expected_width.to_string()).replace("$COUNT", &expected_count.to_string());
        let compiled = compile_with_context(
            &format!("underlay-filter-{style}-{p1}-{p2}"),
            &source,
            |context| {
                context.style_name = style.into();
                context.players[0].background_filter_percent = p1;
                context.players[1].background_filter_percent = p2;
                context.players[1].enabled = enabled;
                context.players[0].screen_x = 130.0;
                context.players[1].screen_x = 510.0;
            },
        );
        let scene = compiled
            .live_scene
            .expect("selected native background filters");
        let filters = scene_global_table(&scene, "FILTERS").unwrap();
        assert_eq!(scene_table_len(&scene, filters).unwrap(), expected_count);
        for (index, filter) in scene_sequence_tables(&scene, filters)
            .unwrap()
            .into_iter()
            .enumerate()
        {
            let state = scene_state(&scene, filter).unwrap();
            assert_eq!(state.x, [130.0, 510.0][index]);
            assert_eq!(
                state.y,
                scene.selected_context().unwrap().screen_height * 0.5
            );
            let percent = [p1, p2][index];
            assert!((state.diffuse[3] - f32::from(percent) / 100.0).abs() < 1e-6);
            assert_eq!(&state.diffuse[..3], &[0.0; 3]);
            assert!(scene_has_function(&scene, filter, "OffCommand").unwrap());
            assert!(scene_has_function(&scene, filter, "ComboFlashCommand").unwrap());
        }
    }
}

fn scene_expression_table(
    scene: &SongLuaLiveScene,
    source: impl AsRef<str>,
) -> Result<Root, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let code = compile_named(
        source.as_ref().as_bytes(),
        b"=existing live expression fixture",
    )?;
    let value = vm.execute(&code)?.first().copied().unwrap_or(Value::Nil);
    assert!(matches!(value, Value::Table(_)), "table expression fixture");
    vm.pin(value)
}
fn scene_set_bool(
    scene: &SongLuaLiveScene,
    name: impl AsRef<[u8]>,
    value: bool,
) -> Result<(), Error> {
    scene
        .runtime()
        .vm_mut()
        .set_global(name.as_ref(), Value::Boolean(value))
}
fn scene_set_string(
    scene: &SongLuaLiveScene,
    name: impl AsRef<[u8]>,
    value: impl AsRef<str>,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let value = vm.try_intern(value.as_ref().as_bytes())?;
    vm.set_global(name.as_ref(), value)
}
fn scene_write_value(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: impl AsRef<[u8]>,
    value: Value,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let object = vm.rooted(owner)?;
    let key = vm.try_intern(name.as_ref())?;
    let code = compile_named(
        b"local object,key,value=...;return object[key](object,value)",
        b"=existing live option write",
    )?;
    vm.execute_with_args(&code, &[object, key, value])?;
    Ok(())
}
fn scene_write_number(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: impl AsRef<[u8]>,
    value: f64,
) -> Result<(), Error> {
    scene_write_value(scene, owner, name, Value::Number(value))
}
fn scene_write_bool(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: impl AsRef<[u8]>,
    value: bool,
) -> Result<(), Error> {
    scene_write_value(scene, owner, name, Value::Boolean(value))
}
fn scene_write_string(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: impl AsRef<[u8]>,
    value: &str,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let object = vm.rooted(owner)?;
    let key = vm.try_intern(name.as_ref())?;
    let value = vm.try_intern(value.as_bytes())?;
    let code = compile_named(
        b"local object,key,value=...;return object[key](object,value)",
        b"=existing live option write",
    )?;
    vm.execute_with_args(&code, &[object, key, value])?;
    Ok(())
}

#[test]
fn live_foreground_observes_transition_before_current_frame_update() {
    let mut compiled = compile_with_context(
        "foreground-transition-order",
        r#"
IN = SCREENMAN:GetTopScreen():GetChild('In')
LEFT = {}
return Def.ActorFrame {
    OnCommand = function(self)
        self:aux(IN:GetTweenTimeLeft()):sleep(10)
        self:SetUpdateFunction(function()
            LEFT[#LEFT + 1] = IN:GetTweenTimeLeft()
        end)
    end,
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
        },
    );
    let mut scene = compiled.live_scene.take().expect("scene");
    scene
        .advance(SongLuaLiveFrame {
            music_seconds: -0.5,
            beat: -1.0,
            screen_seconds: 0.125,
            delta_seconds: 0.125,
            ..Default::default()
        })
        .unwrap();
    scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.0625,
            beat: 0.125,
            screen_seconds: 0.1875,
            delta_seconds: 0.0625,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        scene.states()[0].aux,
        1.875,
        "foreground On precedes the transition's current-frame Update"
    );
    let left = scene_global_table(&scene, "LEFT").unwrap();
    assert_eq!(
        scene_index_field_number(&scene, left, 1, true)
            .map(|value| value as f32)
            .unwrap(),
        1.875,
        "foreground Update also precedes the transition"
    );
    let transition = scene_global_table(&scene, "IN").unwrap();
    assert_eq!(
        scene_method_number(&scene, transition, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        1.8125,
        "the same frame advances the transition after the foreground"
    );
}

#[test]
fn live_deferred_initial_update_uses_first_input_clock_once() {
    let dir =
        std::env::temp_dir().join(format!("deadsync-live-first-clock-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("default.lua");
    fs::write(
        &entry,
        r#"
UPDATES = {}
return Def.ActorFrame { InitCommand = function(self)
    self:SetUpdateFunction(function(_, delta)
        UPDATES[#UPDATES+1] = { delta, GAMESTATE:GetCurMusicSeconds() }
    end)
end }
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&dir, "first input clock");
    context.live = true;
    context.defer_initial_live_update = true;
    let mut compiled: Compiled = song::compile_song_lua(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .unwrap();
    let mut scene = compiled.live_scene.take().unwrap();
    let updates = scene_global_table(&scene, "UPDATES").unwrap();
    assert_eq!(
        scene_table_len(&scene, updates).unwrap(),
        0,
        "preparation must leave Update(0) to the input clock"
    );
    for (music, delta) in [(-4.5, 0.0), (-4.49, 0.01)] {
        let output = scene
            .advance(SongLuaLiveFrame {
                music_seconds: music,
                screen_seconds: delta,
                delta_seconds: delta,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
    }
    assert_eq!(scene_table_len(&scene, updates).unwrap(), 2);
    let first = scene_index_table(&scene, updates, 1).unwrap();
    assert_eq!(scene_index_number(&scene, first, 1).unwrap(), 0.0);
    assert_eq!(scene_index_number(&scene, first, 2).unwrap(), -4.5);
    assert_eq!(
        scene_index_number(&scene, scene_index_table(&scene, updates, 2).unwrap(), 2)
            .map(|value| value as f32)
            .unwrap(),
        -4.49_f32
    );
}

#[test]
fn live_table_find_returns_native_keys_and_completes_show_arrow() {
    let mut compiled = compile_with_context(
        "table-find-show-arrow",
        r#"
SHOWS, AFTER_SHOW, SELECTED = 0, false, {}
local fired = false
return Def.ActorFrame {
    OnCommand=function(self)
        assert(table.find({4,8,12}, 8) == 2)
        assert(table.find({[6]='six', tail='end'}, 'six') == 6)
        assert(table.find({[6]='six', tail='end'}, 'end') == 'tail')
        assert(table.find({off=false}, false) == 'off')
        assert(table.find({[false]='needle'}, 'needle') == false)
        assert(table.find({4,8}, '8') == nil)
        assert(table.find({4,8}, nil) == nil and table.find({}, 8) == nil)
        local marker = self:GetChild('Arrow2')
        assert(table.find({self:GetChild('Arrow1'), marker}, marker) == 2)
        local same = {__eq=function(a,b) return a.id == b.id end}
        local a, b = setmetatable({id=7}, same), setmetatable({id=7}, same)
        assert(table.find({[3]=a}, b) == 3)
        self:SetUpdateFunction(function(actor, delta)
            if fired or delta <= 0 then return end
            MESSAGEMAN:Broadcast('ShowArrow', {{2,4},0.5})
            AFTER_SHOW = true
            fired = true
        end)
    end,
    ShowArrowMessageCommand=function(self, args)
        for column=1,4 do
            if table.find(args[1], column) then
                self:GetChild('Arrow'..column):aux(10+column)
                SELECTED[#SELECTED+1] = column
            end
        end
        SHOWS = SHOWS + 1
    end,
    Def.Actor {Name='Arrow1'}, Def.Actor {Name='Arrow2'},
    Def.Actor {Name='Arrow3'}, Def.Actor {Name='Arrow4'},
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
        },
    );
    let mut scene = compiled.live_scene.take().expect("owned ShowArrow scene");
    for seconds in [0.1, 0.2, 0.3] {
        let output = scene
            .advance(SongLuaLiveFrame {
                screen_seconds: seconds,
                music_seconds: seconds,
                beat: seconds,
                delta_seconds: 0.1,
                ..Default::default()
            })
            .expect("ShowArrow frame");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
    }
    assert!(
        scene_global_bool(&scene, "AFTER_SHOW").unwrap(),
        "the update continues after the real message callback"
    );
    assert_eq!(
        scene_global_number(&scene, "SHOWS")
            .map(|value| value as u32)
            .unwrap(),
        1,
        "successful lookup prevents repeated broadcasts"
    );
    let selected = scene_global_table(&scene, "SELECTED").unwrap();
    assert_eq!(scene_table_len(&scene, selected).unwrap(), 2);
    assert_eq!(
        scene_index_field_number(&scene, selected, 1, true)
            .map(|value| value as u32)
            .unwrap(),
        2
    );
    assert_eq!(
        scene_index_field_number(&scene, selected, 2, true)
            .map(|value| value as u32)
            .unwrap(),
        4
    );
}

#[test]
fn live_note_field_container_owns_original_children_and_mod_lifecycle() {
    for player in 0..2 {
        let mut compiled = compile_with_context(
            &format!("native-note-field-container-{player}"),
            "return Def.ActorFrame {}",
            |context| {
                context.defer_initial_live_update = true;
                context.screen_width = 854.0;
                for side in 0..2 {
                    context.players[side].enabled = side == player;
                    context.players[side].column_cues = true;
                }
                context.song_steps = vec![song::SongLuaStep {
                    steps_type: "StepsType_Dance_Single".into(),
                    difficulty: SongLuaDifficulty::Challenge,
                    description: String::new(),
                    chart_name: String::new(),
                    credit: String::new(),
                    meter: 9,
                    music_file: String::new(),
                    possible_grade_points: 0,
                }];
                context.player_step_indices = if player == 0 {
                    [Some(0), None]
                } else {
                    [None, Some(0)]
                };
                context.song_step_notes = vec![Arc::from(&b"1000\n0M00\n00F0\n000L\n"[..])];
                context.song_step_row_beats = vec![Arc::from([0.0, 4.0, 8.0, 12.0])];
            },
        );
        let mut scene = compiled.live_scene.take().expect("native container scene");
        scene_set_string(&scene, "PN", format!("PlayerNumber_P{}", player + 1)).unwrap();
        scene_set_number(&scene, "SIDE", (player + 1) as f64).unwrap();
        scene_exec(&scene,r#"
local underlay=SCREENMAN:GetTopScreen():GetChild('Underlay')
local container=underlay:GetChild('NoteFieldContainerP'..SIDE)
assert(container:GetNumChildren()==3, 'native NoteField container must own its three enabled children')
assert(container:GetParent()==underlay)
local children=container:GetChild('')
assert(#children==3)
local flash,cues,mods=children[1],children[2],children[3]
assert(flash:GetParent()==container and flash:GetNumChildren()==4)
assert(cues:GetParent()==container and cues:GetNumChildren()==4)
assert(mods:GetParent()==container and mods:GetNumChildren()==1)
assert(math.abs(mods:GetTweenTimeLeft()-5.5)<0.00001, 'real DisplayMods On queue is retained')
for _,quad in ipairs(flash:GetChild('')) do
    assert(quad:GetParent()==flash and quad:GetTexture())
end
assert(cues:GetChild('Column1'):GetChild('ColumnFlash'):GetParent():GetParent()==cues)
MODS=mods
MODTEXT=mods:GetChild('')
MODTEXT:settext('stale')
MESSAGEMAN:Broadcast('PlayerOptionsChanged',{Player=PN})
assert(MODTEXT:GetText()~='stale', 'native options message refreshes the actual display text')
"#).unwrap();
        let output = scene
            .advance(SongLuaLiveFrame {
                screen_seconds: 0.001,
                delta_seconds: 0.001,
                music_seconds: 0.001,
                beat: 0.002,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        scene_exec(&scene,"assert(math.abs(MODS:GetTweenTimeLeft()-5.499)<0.00001, 'native options message restarts On through the ordinary queue')").unwrap();
    }
}

#[test]
fn live_danger_owns_native_quad_and_health_lifecycle() {
    for (name, enabled, double, hidden, hide_danger) in [
        ("p1", [true, false], false, false, false),
        ("p2", [false, true], false, false, false),
        ("double", [true, false], true, false, false),
        ("hidden-life", [true, false], false, true, false),
        ("hide-danger", [true, false], false, false, true),
    ] {
        let mut compiled = compile_with_context(
            &format!("native-danger-{name}"),
            "return Def.ActorFrame {}",
            |context| {
                context.screen_width = 854.0;
                context.defer_initial_live_update = true;
                if double {
                    context.style_name = "double".into();
                }
                for player in 0..2 {
                    context.players[player].enabled = enabled[player];
                    context.players[player].hide_lifebar = hidden;
                    context.players[player].hide_danger = hide_danger;
                }
            },
        );
        let mut scene = compiled.live_scene.take().expect("retained Danger scene");
        scene_set_bool(&scene, "DOUBLE", double).unwrap();
        scene_set_bool(&scene, "HIDDEN", hidden).unwrap();
        scene_set_bool(&scene, "HIDE_DANGER", hide_danger).unwrap();
        scene_exec(
            &scene,
            r#"
local underlay=SCREENMAN:GetTopScreen():GetChild('Underlay')
for p=1,2 do
    local actor=underlay:GetChild('DangerP'..p)
    if GAMESTATE:IsPlayerEnabled('PlayerNumber_P'..p) then
        assert(actor and actor:GetTexture(), 'Danger must own the native Quad default texture')
        assert(actor:GetTexture():GetSourceWidth()==1 and actor:GetTexture():GetSourceHeight()==1)
        assert(actor:GetParent()==underlay and actor:GetVisible()==not HIDDEN)
        assert(actor:GetZoomedWidth()==(DOUBLE and 854 or 427) and actor:GetZoomedHeight()==480)
        assert(actor:GetX()==(DOUBLE and 427 or (p==1 and 213.5 or 640.5)) and actor:GetY()==240)
        assert(actor:GetDiffuseAlpha()==0)
        assert(SL['P'..p].ActiveModifiers.HideDanger==HIDE_DANGER)
        DANGER=actor; PN='PlayerNumber_P'..p
    else assert(actor==nil, 'inactive player has no Danger actor') end
end
"#,
        )
        .unwrap();
        scene.advance(SongLuaLiveFrame::default()).unwrap();
        scene
            .advance(SongLuaLiveFrame {
                screen_seconds: 0.3,
                delta_seconds: 0.3,
                music_seconds: 0.3,
                beat: 0.6,
                ..Default::default()
            })
            .unwrap();
        scene_exec(
            &scene,
            r#"
MESSAGEMAN:Broadcast('HealthStateChanged',{PlayerNumber=PN,HealthState='HealthState_Danger'})
assert(math.abs(DANGER:GetTweenTimeLeft()-(HIDE_DANGER and 0 or 0.3))<0.00001)
"#,
        )
        .unwrap();
        scene
            .advance(SongLuaLiveFrame {
                screen_seconds: 0.6,
                delta_seconds: 0.3,
                music_seconds: 0.6,
                beat: 1.2,
                ..Default::default()
            })
            .unwrap();
        scene_exec(
            &scene,
            r#"
assert(math.abs(DANGER:GetDiffuseAlpha()-(HIDE_DANGER and 0 or 0.7))<0.00001)
MESSAGEMAN:Broadcast('HealthStateChanged',{PlayerNumber=PN,HealthState='HealthState_Dead'})
assert(math.abs(DANGER:GetTweenTimeLeft()-0.6)<0.00001, 'fail flash survives HideDanger')
"#,
        )
        .unwrap();
        scene
            .advance(SongLuaLiveFrame {
                screen_seconds: 0.9,
                delta_seconds: 0.3,
                music_seconds: 0.9,
                beat: 1.8,
                ..Default::default()
            })
            .unwrap();
        scene_exec(&scene,"assert(DANGER:GetDiffuseAlpha()==0 and math.abs(DANGER:GetTweenTimeLeft()-0.3)<0.00001, 'native diffuse getter reports queued destination while the fail flash still has a pending tween')").unwrap();
        scene
            .advance(SongLuaLiveFrame {
                screen_seconds: 1.2,
                delta_seconds: 0.3,
                music_seconds: 1.2,
                beat: 2.4,
                ..Default::default()
            })
            .unwrap();
        scene_exec(
            &scene,
            "assert(DANGER:GetDiffuseAlpha()==0 and DANGER:GetTweenTimeLeft()==0)",
        )
        .unwrap();
    }
    for (name, setup) in [
        ("casual", "SL.Global.GameMode='Casual'"),
        (
            "fail-off",
            "GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Preferred'):FailSetting('FailType_Off')",
        ),
    ] {
        let source = format!(
            r#"
ROUND=(ROUND or 0)+1
return Def.ActorFrame {{ OnCommand=function(self)
    if ROUND==1 then {setup}; SCREENMAN:SetNewScreen('ScreenGameplay') end
end }}
"#
        );
        let compiled = compile_with_context(&format!("native-danger-{name}"), &source, |context| {
            context.players[1].enabled = false;
            context.defer_initial_live_update = true;
        });
        assert_eq!(compiled.startup_reloads, 1);
        let scene = compiled.live_scene.expect("retained eligibility reload");
        scene_exec(&scene,"assert(SCREENMAN:GetTopScreen():GetChild('Underlay'):GetChild('DangerP1')==nil, 'native Danger eligibility must survive screen reconstruction')").unwrap();
    }
}

#[test]
fn live_player_song_positions_use_selected_chart_timing() {
    use deadsync_rules::timing::{
        DelaySegment, StopSegment, TimingData, TimingSegments, WarpSegment,
    };
    let mut compiled = compile_with_context(
        "player-song-positions",
        r#"
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function()
        POSITIONS = {}
        for _, player in ipairs({PLAYER_1, PLAYER_2}) do
            local position = GAMESTATE:GetPlayerState(player):GetSongPosition()
            POSITIONS[#POSITIONS+1] = {position:GetSongBeat(), position:GetCurBPS(),
                position:GetMusicSeconds(), position:GetFreeze(), position:GetDelay()}
        end
        GLOBAL_BEAT = GAMESTATE:GetSongPosition():GetSongBeat()
    end)
end }
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.players[1].enabled = true;
            context.song_timing = Some(TimingData::from_segments(
                0.0,
                0.0,
                &TimingSegments {
                    bpms: vec![(0.0, 60.0)],
                    ..Default::default()
                },
                &[],
            ));
            context.player_timing = [
                Some(TimingData::from_segments(
                    0.0,
                    0.0,
                    &TimingSegments {
                        bpms: vec![(0.0, 120.0)],
                        warps: vec![WarpSegment {
                            beat: 4.0,
                            length: 20.0,
                        }],
                        ..Default::default()
                    },
                    &[],
                )),
                Some(TimingData::from_segments(
                    0.0,
                    0.0,
                    &TimingSegments {
                        bpms: vec![(0.0, 60.0)],
                        stops: vec![StopSegment {
                            beat: 1.0,
                            duration: 1.0,
                        }],
                        delays: vec![DelaySegment {
                            beat: 3.0,
                            duration: 0.5,
                        }],
                        ..Default::default()
                    },
                    &[],
                )),
            ];
        },
    );
    let mut scene = compiled.live_scene.take().expect("live scene");
    let timing_context = scene.selected_context().unwrap();
    for seconds in [1.5_f32, 2.5, 4.25] {
        let output = scene
            .advance(SongLuaLiveFrame {
                music_seconds: seconds.into(),
                screen_seconds: seconds.into(),
                delta_seconds: 0.1,
                ..Default::default()
            })
            .expect("live frame");
        assert!(output.errors.is_empty());
        let positions = scene_global_table(&scene, "POSITIONS").unwrap();
        for player in 0..LUA_PLAYERS {
            let expected = timing_context.player_timing[player]
                .as_ref()
                .unwrap()
                .get_song_position(seconds);
            let actual = scene_index_table(&scene, positions, player + 1).unwrap();
            assert_eq!(
                scene_index_field_number(&scene, actual, 1, false)
                    .map(|value| value as f32)
                    .unwrap(),
                expected.beat,
                "player {player}"
            );
            assert_eq!(
                scene_index_field_number(&scene, actual, 2, false)
                    .map(|value| value as f32)
                    .unwrap(),
                expected.bpm / 60.0
            );
            assert_eq!(
                scene_index_field_number(&scene, actual, 3, false)
                    .map(|value| value as f32)
                    .unwrap(),
                seconds
            );
            assert_eq!(
                scene_index_field_bool(&scene, actual, 4, false).unwrap(),
                expected.is_in_freeze
            );
            assert_eq!(
                scene_index_field_bool(&scene, actual, 5, false).unwrap(),
                expected.is_in_delay
            );
        }
        assert_eq!(
            scene_global_number(&scene, "GLOBAL_BEAT")
                .map(|value| value as f32)
                .unwrap(),
            seconds
        );
    }
}

#[cfg(feature = "test-support")]
#[test]
fn live_native_arrow_clocks_keep_query_and_cache_update_reads_independent() {
    let mut compiled = compile_with_context(
        "raw-arrow-clock",
        "return Def.ActorFrame{OnCommand=function(self)\n self:SetUpdateFunction(function(self)\n local ps=GAMESTATE:GetPlayerState(PLAYER_1)\n local po=ps:GetPlayerOptions('ModsLevel_Current');po:Drunk(1);po:Tipsy(1)\n self:x(ArrowEffects.GetXPos(ps,1,0)):y(ArrowEffects.GetXPos(ps,1,0))\n end)\nend}",
        |context| {
            context.defer_initial_live_update = true;
            context.host_clock_inputs = [50.0, 51.0]
                .into_iter()
                .map(|value| song::SongLuaHostClockInput {
                    clock: "ArrowEffects.GetXPos".into(),
                    source: "song:/default.lua".into(),
                    line: 5,
                    value,
                })
                .collect();
        },
    );
    let mut scene = compiled.live_scene.take().unwrap();
    scene.set_arrow_clock_inputs([Some(40.0), None], [None, None]);
    let output = scene
        .advance(SongLuaLiveFrame {
            uptime_seconds: Some(1000.0),
            ..Default::default()
        })
        .unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    let expected = |time: f32| -96.0 + time.cos() * 32.0;
    assert!(
        scene
            .states()
            .iter()
            .any(|state| (state.x - expected(50.0)).abs() < 1e-5
                && (state.y - expected(51.0)).abs() < 1e-5)
    );
    let y: f32 = scene_number(
        &scene,
        "return ArrowEffects.GetYPos(GAMESTATE:GetPlayerState(PLAYER_1),1,0,0)",
    )
    .map(|value| value as f32)
    .unwrap();
    assert!(
        (y - (40.0_f32 * 1.2).cos() * 25.6).abs() < 1e-5,
        "cached Tipsy uses its own raw update clock: {y}"
    );
}

#[test]
fn live_arrow_x_matches_captured_native_source_and_cached_update_phase() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../assets/itgmania/arrow-x-native-reference-v1.json"
    ))
    .unwrap();
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 130);
    for case in cases {
        let style = case["style"].as_str().unwrap();
        let mut compiled = compile_with_context(
            "native-arrow-x-source",
            "return Def.ActorFrame {}",
            |context| {
                context.defer_initial_live_update = true;
                context.style_name = style.into();
                context.screen_height = 480.0;
                for player in &mut context.players {
                    player.enabled = true;
                }
            },
        );
        let mut scene = compiled.live_scene.take().unwrap();
        let player = case["player"].as_u64().unwrap();
        let options = scene_expression_table(
            &scene,
            format!(
                "return GAMESTATE:GetPlayerState(PLAYER_{}):GetCurrentPlayerOptions()",
                player + 1
            ),
        )
        .unwrap();
        let song_options = scene_expression_table(
            &scene,
            format!(
                "return GAMESTATE:GetPlayerState(PLAYER_{}):GetPlayerOptions('ModsLevel_Song')",
                player + 1
            ),
        )
        .unwrap();
        // Native Current approaches the Song target before actor updates.
        // Keep both levels at the same authored values for these math cases.
        for options in [options, song_options] {
            for (name, value) in case["effects"].as_object().unwrap() {
                scene_write_number(&scene, options, name.as_str(), value.as_f64().unwrap())
                    .unwrap();
            }
            scene_write_bool(
                &scene,
                options,
                "Cosecant",
                case["cosecant"].as_bool().unwrap(),
            )
            .unwrap();
            let timer = [
                "ModTimerType_Game",
                "ModTimerType_Beat",
                "ModTimerType_Song",
                "ModTimerType_Default",
            ][case["timer"].as_u64().unwrap() as usize];
            scene_write_string(&scene, options, "ModTimerSetting", timer).unwrap();
            for (method, name) in [
                ("ModTimerOffset", "timer_offset"),
                ("ModTimerMult", "timer_mult"),
            ] {
                scene_write_number(&scene, options, method, case[name].as_f64().unwrap()).unwrap();
            }
        }
        let output = scene
            .advance(SongLuaLiveFrame {
                beat: case["beat"].as_f64().unwrap(),
                music_seconds: case["seconds"].as_f64().unwrap(),
                screen_seconds: 0.1,
                delta_seconds: 0.1,
                uptime_seconds: case["uptime"].as_f64(),
                ..Default::default()
            })
            .unwrap();
        assert!(
            output.errors.is_empty(),
            "{}: {:?}",
            case["id"],
            output.errors
        );
        let actual = scene_number(
            &scene,
            format!(
                "return ArrowEffects.GetXPos(GAMESTATE:GetPlayerState(PLAYER_{}), {}, {})",
                player + 1,
                case["column"],
                case["y"]
            ),
        )
        .map(|value| value as f32)
        .unwrap();
        let expected = &case["expected_x"];
        let valid = match expected.as_str() {
            Some("nan") => actual.is_nan(),
            Some("infinity") => actual == f32::INFINITY,
            Some("-infinity") => actual == f32::NEG_INFINITY,
            _ => (actual - expected.as_f64().unwrap() as f32).abs() <= 0.0001,
        };
        assert!(
            valid,
            "native GetXPos must apply source-defined X effects: {} actual={actual:?}, expected={expected}",
            case["id"]
        );
    }

    let mut compiled = compile_with_context(
        "native-arrow-x-update-phase",
        r#"
return Def.ActorFrame {InitCommand=function(self) XROOT=self end}
"#,
        |context| context.defer_initial_live_update = true,
    );
    let mut scene = compiled.live_scene.take().unwrap();
    scene_exec(
        &scene,
        r#"
PS=GAMESTATE:GetPlayerState(PLAYER_1)
PS:GetCurrentPlayerOptions():Beat(0.05)
PS:GetPlayerOptions('ModsLevel_Song'):Beat(0.05)
assert(ArrowEffects.GetXPos(PS,1,0)==-96, 'native Beat cache begins at zero')
"#,
    )
    .unwrap();
    let frame = |beat| SongLuaLiveFrame {
        beat,
        music_seconds: beat / 2.0,
        screen_seconds: beat,
        delta_seconds: 0.01,
        ..Default::default()
    };
    assert!(scene.advance(frame(0.05)).unwrap().errors.is_empty());
    scene_exec(
        &scene,
        r#"
PREVIOUS=ArrowEffects.GetXPos(PS,1,0)
assert(math.abs(PREVIOUS+96)>0.5)
XROOT:SetUpdateFunction(function() DURING=ArrowEffects.GetXPos(PS,1,0) end)
"#,
    )
    .unwrap();
    assert!(scene.advance(frame(0.35)).unwrap().errors.is_empty());
    scene_exec(&scene,r#"
assert(DURING==PREVIOUS, 'native Beat uses the preceding completed screen update')
assert(ArrowEffects.GetXPos(PS,1,0)==-96, 'native screen update refreshes Beat after actor callbacks')
PS:GetCurrentPlayerOptions():BeatOffset(0.5)
ArrowEffects.Update()
assert(ArrowEffects.GetXPos(PS,1,0)~=-96, 'explicit native Update refreshes effect data')
"#).unwrap();
}

#[test]
fn live_arrow_zoom_matches_native_source_and_column_tiny_lifecycle() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../assets/itgmania/arrow-zoom-native-reference-v1.json"
    ))
    .unwrap();
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 53);
    for case in cases {
        let column = case["column"].as_u64().unwrap();
        let mut compiled = compile_with_context(
            "native-arrow-zoom-source",
            "return Def.ActorFrame {}",
            |context| {
                context.defer_initial_live_update = true;
                context.style_name = if column > 4 { "double" } else { "single" }.into();
            },
        );
        let scene = compiled.live_scene.take().unwrap();
        let options = scene_expression_table(
            &scene,
            "return GAMESTATE:GetPlayerState(PLAYER_1):GetCurrentPlayerOptions()",
        )
        .unwrap();
        for (name, value) in case["effects"].as_object().unwrap() {
            scene_write_number(&scene, options, name.as_str(), value.as_f64().unwrap()).unwrap();
        }
        let actual = scene_number(
            &scene,
            format!(
                "return ArrowEffects.GetZoom(GAMESTATE:GetPlayerState(PLAYER_1), {}, {})",
                case["y"], column
            ),
        )
        .map(|value| value as f32)
        .unwrap();
        let expected = &case["expected"];
        let valid = match expected.as_str() {
            Some("nan") => actual.is_nan(),
            Some("infinity") => actual == f32::INFINITY,
            Some("-infinity") => actual == f32::NEG_INFINITY,
            _ => (actual - expected.as_f64().unwrap() as f32).abs() <= 0.0001,
        };
        assert!(
            valid,
            "native GetZoom must apply per-column Tiny and variable zoom: {} actual={actual:?}, expected={expected}",
            case["id"]
        );
    }
    let mut compiled = compile_with_context(
        "native-arrow-zoom-current-lifecycle",
        "return Def.ActorFrame {}",
        |context| context.defer_initial_live_update = true,
    );
    let mut scene = compiled.live_scene.take().unwrap();
    scene_exec(
        &scene,
        r#"
PS=GAMESTATE:GetPlayerState(PLAYER_1)
SONG=PS:GetPlayerOptions('ModsLevel_Song')
SONG:Tiny4(-0.08752030434412367,10000)
assert(ArrowEffects.GetZoom(PS,150,4)==1, 'Current remains separate until the next update')
"#,
    )
    .unwrap();
    let frame = |time| SongLuaLiveFrame {
        music_seconds: time,
        beat: time * 2.0,
        screen_seconds: time,
        delta_seconds: 0.1,
        ..Default::default()
    };
    assert!(scene.advance(frame(0.1)).unwrap().errors.is_empty());
    scene_exec(
        &scene,
        r#"
assert(math.abs(ArrowEffects.GetZoom(PS,150,4)-1.0625423192977905)<0.000001)
assert(ArrowEffects.GetZoom(PS,150,1)==1, 'per-column Tiny does not affect another lane')
SONG:Tiny4(-0.40300090498656843,10000)
assert(math.abs(ArrowEffects.GetZoom(PS,150,4)-1.0625423192977905)<0.000001,
    'native Current still exposes the prior Song target during the setter frame')
"#,
    )
    .unwrap();
    assert!(scene.advance(frame(0.2)).unwrap().errors.is_empty());
    scene_exec(&scene, "assert(ArrowEffects.GetZoom(PS,150,4)>1.3)").unwrap();
}

#[test]
fn live_arrow_y_matches_native_source_wave_and_cached_tipsy_phase() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../assets/itgmania/arrow-y-native-reference-v1.json"
    ))
    .unwrap();
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 63);
    for case in cases {
        let y_pos = case["kind"] == "ypos";
        let mut compiled = compile_with_context(
            "native-arrow-y-source",
            "return Def.ActorFrame {}",
            |context| {
                context.defer_initial_live_update = true;
                context.screen_height = 480.0;
                if y_pos {
                    context.style_name = case["style"].as_str().unwrap().into();
                }
                for player in &mut context.players {
                    player.enabled = true;
                }
            },
        );
        let mut scene = compiled.live_scene.take().unwrap();
        let player = case["player"].as_u64().unwrap_or(0) + 1;
        scene_exec(
            &scene,
            format!("PS=GAMESTATE:GetPlayerState(PLAYER_{player})"),
        )
        .unwrap();
        let current =
            scene_expression_table(&scene, "return PS:GetCurrentPlayerOptions()").unwrap();
        let song =
            scene_expression_table(&scene, "return PS:GetPlayerOptions('ModsLevel_Song')").unwrap();
        let expression;
        if y_pos {
            for options in [current, song] {
                for (name, value) in case["effects"].as_object().unwrap() {
                    scene_write_number(&scene, options, name.as_str(), value.as_f64().unwrap())
                        .unwrap();
                }
                for (method, name) in [
                    ("Mini", "mini"),
                    ("Reverse", "reverse"),
                    ("Centered", "centered"),
                ] {
                    scene_write_number(&scene, options, method, case[name].as_f64().unwrap())
                        .unwrap();
                }
                scene_write_bool(
                    &scene,
                    options,
                    "Cosecant",
                    case["cosecant"].as_bool().unwrap(),
                )
                .unwrap();
                let timer = [
                    "ModTimerType_Game",
                    "ModTimerType_Beat",
                    "ModTimerType_Song",
                    "ModTimerType_Default",
                ][case["timer"].as_u64().unwrap() as usize];
                scene_write_string(&scene, options, "ModTimerSetting", timer).unwrap();
            }
            expression = format!(
                "return ArrowEffects.GetYPos(PS, {}, {}, 270)",
                case["column"], case["y"]
            );
            // The native binding selects Current options before cached Update.
            scene_number(&scene, &expression)
                .map(|value| value as f32)
                .unwrap();
            if case.get("cache_timer").is_some() {
                scene_exec(
                    &scene,
                    r#"
local other=GAMESTATE:GetPlayerState(PLAYER_1)
other:GetCurrentPlayerOptions():ModTimerSetting('ModTimerType_Song')
other:GetPlayerOptions('ModsLevel_Song'):ModTimerSetting('ModTimerType_Song')
ArrowEffects.GetYPos(other,1,0)
"#,
                )
                .unwrap();
            }
            let output = scene
                .advance(SongLuaLiveFrame {
                    beat: case["beat"].as_f64().unwrap(),
                    music_seconds: case["seconds"].as_f64().unwrap(),
                    screen_seconds: 0.1,
                    delta_seconds: 0.1,
                    uptime_seconds: case["uptime"].as_f64(),
                    ..Default::default()
                })
                .unwrap();
            assert!(
                output.errors.is_empty(),
                "{}: {:?}",
                case["id"],
                output.errors
            );
        } else {
            for options in [current, song] {
                for (method, name) in [
                    ("Wave", "wave"),
                    ("WavePeriod", "period"),
                    ("XMod", "speed"),
                ] {
                    scene_write_number(&scene, options, method, case[name].as_f64().unwrap())
                        .unwrap();
                }
            }
            expression = format!("return ArrowEffects.GetYOffset(PS,1,{})", case["note"]);
        }
        let actual = scene_number(&scene, &expression)
            .map(|value| value as f32)
            .unwrap();
        let expected = &case["expected"];
        let valid = match expected.as_str() {
            Some("nan") => actual.is_nan(),
            Some("infinity") => actual == f32::INFINITY,
            Some("-infinity") => actual == f32::NEG_INFINITY,
            _ => (actual - expected.as_f64().unwrap() as f32).abs() <= 0.0001,
        };
        assert!(
            valid,
            "native arrow Y must apply Wave and cached Y effects: {} actual={actual:?}, expected={expected}",
            case["id"]
        );
    }
    let mut compiled = compile_with_context(
        "native-tipsy-update-phase",
        r#"
return Def.ActorFrame {InitCommand=function(self) YROOT=self end}
"#,
        |context| context.defer_initial_live_update = true,
    );
    let mut scene = compiled.live_scene.take().unwrap();
    scene_exec(
        &scene,
        r#"
PS=GAMESTATE:GetPlayerState(PLAYER_1)
PS:GetCurrentPlayerOptions():Tipsy(0.5)
PS:GetPlayerOptions('ModsLevel_Song'):Tipsy(0.5)
BASE=ArrowEffects.GetYPos(PS,1,0)
YROOT:SetUpdateFunction(function() DURING=ArrowEffects.GetYPos(PS,1,0) end)
"#,
    )
    .unwrap();
    let frame = |time, uptime| SongLuaLiveFrame {
        music_seconds: time,
        beat: time * 2.0,
        screen_seconds: time,
        delta_seconds: 0.1,
        uptime_seconds: Some(uptime),
        ..Default::default()
    };
    assert!(scene.advance(frame(0.1, 2.0)).unwrap().errors.is_empty());
    scene_exec(
        &scene,
        r#"
assert(DURING==BASE,'native first callback observes the zero Tipsy cache')
PREVIOUS=ArrowEffects.GetYPos(PS,1,0)
assert(math.abs(PREVIOUS-BASE)>1)
"#,
    )
    .unwrap();
    assert!(scene.advance(frame(0.2, 2.125)).unwrap().errors.is_empty());
    scene_exec(
        &scene,
        r#"
assert(DURING==PREVIOUS,'native callbacks observe the preceding completed Tipsy update')
assert(ArrowEffects.GetYPos(PS,1,0)~=PREVIOUS,'cache refresh happens after callbacks')
PS:GetPlayerOptions('ModsLevel_Song'):Tipsy(0,10000)
"#,
    )
    .unwrap();
    assert!(scene.advance(frame(0.3, 2.25)).unwrap().errors.is_empty());
    scene_exec(
        &scene,
        "assert(DURING==BASE and ArrowEffects.GetYPos(PS,1,0)==BASE)",
    )
    .unwrap();
}

fn scene_direct_children(scene: &SongLuaLiveScene, owner: Root) -> Result<Vec<Root>, Error> {
    let mut runtime = scene.runtime();
    let value = runtime.vm_mut().rooted(owner)?;
    let children = song::test_support::actor_direct_children(&mut runtime, value)?;
    children
        .into_iter()
        .map(|value| runtime.vm_mut().pin(value))
        .collect()
}
fn scene_sprite_states(
    scene: &SongLuaLiveScene,
    owner: Root,
) -> Result<Vec<SongLuaSpriteState>, Error> {
    let mut runtime = scene.runtime();
    let value = runtime.vm_mut().rooted(owner)?;
    song::test_support::read_sprite_states(&mut runtime, value)
}
fn scene_init_state(scene: &SongLuaLiveScene, owner: Root) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let value = runtime.vm_mut().rooted(owner)?;
    song::test_support::init_actor_native_state(&mut runtime, value)
}
fn scene_method_table(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: impl AsRef<[u8]>,
) -> Result<Root, Error> {
    scene_table_read(scene, owner, name, TableRead::Method, |vm, value| {
        assert!(matches!(value, Value::Table(_)), "table method result");
        vm.pin(value)
    })
}
fn scene_child(scene: &SongLuaLiveScene, owner: Root, name: &str) -> Result<Root, Error> {
    let children = scene_named_children(scene, owner)?;
    scene_field_table(scene, children, name, true)
}
fn scene_optional_child(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: &str,
) -> Result<Option<Root>, Error> {
    let children = scene_named_children(scene, owner)?;
    scene_table_read(scene, children, name, TableRead::Raw, |vm, value| {
        if value == Value::Nil {
            Ok(None)
        } else {
            assert!(matches!(value, Value::Table(_)));
            vm.pin(value).map(Some)
        }
    })
}
fn scene_set_root(scene: &SongLuaLiveScene, name: &str, root: Root) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let value = vm.rooted(root)?;
    vm.set_global(name.as_bytes(), value)
}
fn scene_root_sequence(scene: &SongLuaLiveScene, roots: &[Root]) -> Result<Root, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let table = vm.try_new_table()?;
    let pin = vm.pin(Value::Table(table))?;
    for (index, &root) in roots.iter().enumerate() {
        let value = vm.rooted(root)?;
        vm.raw_table_set(table, Value::Number((index + 1) as f64), value)?
    }
    Ok(pin)
}
fn scene_font(
    scene: &SongLuaLiveScene,
    owner: Root,
) -> Result<Option<(&'static str, std::path::PathBuf)>, Error> {
    let mut runtime = scene.runtime();
    let actor = runtime.vm_mut().rooted(owner)?;
    Ok(runtime.actor_bitmap_text(actor)?.font)
}
fn scene_type_is(scene: &SongLuaLiveScene, owner: Root, expected: &str) -> Result<bool, Error> {
    scene_table_read(
        scene,
        owner,
        "__songlua_actor_type",
        TableRead::Field,
        |vm, value| {
            if value == Value::Nil {
                Ok(false)
            } else {
                fixture_string(vm, value).map(|text| text.eq_ignore_ascii_case(expected))
            }
        },
    )
}
fn scene_texture_path(
    scene: &SongLuaLiveScene,
    owner: Root,
) -> Result<Option<std::path::PathBuf>, Error> {
    let texture = scene_table_read(
        scene,
        owner,
        "GetTexture",
        TableRead::Method,
        |vm, value| {
            if value == Value::Nil {
                Ok(None)
            } else {
                vm.pin(value).map(Some)
            }
        },
    )?;
    texture
        .map(|texture| scene_method_string(scene, texture, "GetPath").map(Into::into))
        .transpose()
}
fn scene_start_transition(scene: &SongLuaLiveScene, owner: Root) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let actor = runtime.vm_mut().rooted(owner)?;
    song::test_support::start_actor_transition(&mut runtime, actor)
}
fn scene_field_function(scene: &SongLuaLiveScene, owner: Root, name: &str) -> Result<Root, Error> {
    scene_table_read(scene, owner, name, TableRead::Raw, |vm, value| {
        assert!(
            matches!(value, Value::Function(_) | Value::Native(_)),
            "fixture function"
        );
        vm.pin(value)
    })
}
fn scene_function_source(
    scene: &SongLuaLiveScene,
    function: Root,
) -> Result<Option<String>, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let value = vm.rooted(function)?;
    vm.with_native_stack(|stack| {
        stack.push(value)?;
        let record = deadlib_lua::NativeDebugFrame::from_index(stack.thread_id(), 0);
        let info = stack.get_info(record, b">S")?;
        Ok(info
            .source
            .and_then(|source| stack.debug_bytes(source))
            .map(|bytes| {
                String::from_utf8_lossy(bytes.strip_suffix(&[0]).unwrap_or(bytes)).into_owned()
            }))
    })
}
fn scene_call_method_function(
    scene: &SongLuaLiveScene,
    function: Root,
    owner: Root,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let function = vm.rooted(function)?;
    let owner = vm.rooted(owner)?;
    vm.call(function, &[owner])?;
    Ok(())
}
fn scene_difficulty_color(scene: &SongLuaLiveScene, name: &str) -> Result<Root, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let function = vm.get_global(b"DifficultyColor")?;
    let name = vm.try_intern(name.as_bytes())?;
    let value = vm
        .call(function, &[name])?
        .first()
        .copied()
        .unwrap_or(Value::Nil);
    vm.pin(value)
}
fn scene_two_numbers_bool(
    scene: &SongLuaLiveScene,
    source: &str,
) -> Result<(f32, f32, bool), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let code = compile_named(source.as_bytes(), b"=existing live tuple fixture")?;
    let values = vm.execute(&code)?.to_vec();
    assert!(values.len() >= 3);
    Ok((
        fixture_number(vm, values[0])? as f32,
        fixture_number(vm, values[1])? as f32,
        !matches!(values[2], Value::Nil | Value::Boolean(false)),
    ))
}
fn scene_entry_root(scene: &SongLuaLiveScene) -> Result<Root, Error> {
    let (roots, _) = song::test_support::live_scene_actor_handles(scene)?;
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let Value::Table(roots) = roots else {
        unreachable!()
    };
    let actor = vm.raw_table_get(roots, Value::Number(1.))?;
    assert!(matches!(actor, Value::Table(_)), "entry actor");
    vm.pin(actor)
}

#[test]
fn live_hsva_initializes_native_vertex_colors() {
    let mut compiled = compile(
        "native-hsva-vertices",
        r#"
return Def.ActorMultiVertex {
    Name = 'SpaceshipTrailP1',
    InitCommand = function(self)
        assert(type(HSVA) == 'function', 'native HSVA is available for authored vertex initialization')
        local inputs = {
            {0, 1, 1, 1}, {60, 1, 1, 0.75}, {120, 1, 1, 0.5},
            {180, 1, 1, 0.25}, {240, 1, 1, 0}, {300, 1, 1, 1},
            {360, 1, 1, 0.5}, {-30, 1, 1, 0.25}, {480, 1, 1, 1},
            {15, 0.5, 0.25, 0.125}, {270, 0, 0.375, -0.25}, {30, 2, 0.5, 1.25},
        }
        local vertices = {}
        HSVA_COLORS = {}
        for index, input in ipairs(inputs) do
            local rgba = HSVA(unpack(input))
            HSVA_COLORS[index] = rgba
            vertices[index] = {{index, index % 2, 0}, rgba}
        end
        assert(HSVToColor{Hue=0, Sat=0, Value=0.5}[4] == 0)
        assert(HSVToColor{Hue=0, Sat=0, Value=0.5, Alpha=false}[4] == 0)
        assert(HSVA(0, 1, 1)[4] == 0)
        self:SetDrawState{Mode='DrawMode_Quads'}:SetVertices(vertices)
    end,
}
"#,
    );
    let expected: [[f64; 4]; 12] = [
        [1.0, 0.0, 0.0, 1.0],
        [1.0, 1.0, 0.0, 0.75],
        [0.0, 1.0, 0.0, 0.5],
        [0.0, 1.0, 1.0, 0.25],
        [0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 1.0, 1.0],
        [1.0, 0.0, 1.0, 0.5],
        [1.0, 0.0, 0.5, 0.25],
        [1.0, 0.0, 1.0, 1.0],
        [0.25, 0.15625, 0.125, 0.125],
        [0.375, 0.375, 0.375, -0.25],
        [0.5, 0.0, -0.5, 1.25],
    ];
    let mut scene = compiled
        .live_scene
        .take()
        .expect("owned vertex color scene");
    assert!(scene.take_startup_output().errors.is_empty());
    let colors = scene_global_table(&scene, "HSVA_COLORS").unwrap();
    for (index, expected) in expected.iter().enumerate() {
        let rgba = scene_index_table(&scene, colors, index + 1).unwrap();
        for (channel, expected) in expected.iter().enumerate() {
            assert_eq!(
                scene_index_number(&scene, rgba, channel + 1).unwrap(),
                *expected,
                "native Lua color {index}, channel {channel}"
            );
        }
    }
    let SongLuaOverlayKind::ActorMultiVertex { vertices, .. } = &compiled.overlays[0].kind else {
        panic!("the authored trail retains its vertex mesh");
    };
    assert_eq!(vertices.len(), 18);
    let mut seen = [false; 12];
    for vertex in vertices.iter() {
        let index = vertex.pos[0] as usize;
        assert!(
            (1..=expected.len()).contains(&index),
            "authored vertex position retained"
        );
        assert_eq!(vertex.color, expected[index - 1].map(|v| v as f32));
        seen[index - 1] = true;
    }
    assert!(
        seen.into_iter().all(|seen| seen),
        "all authored colors reach the rendered mesh"
    );
}

#[test]
fn live_sprite_linear_frames_builds_native_authored_states() {
    let mut compiled = compile_with_context(
        "sprite-linear-frames",
        r#"
assert(type(Sprite.LinearFrames) == 'function', 'native Sprite.LinearFrames helper is available')
local seconds = 184/60*2
local frames = Sprite.LinearFrames(144, seconds)
assert(#frames == 144)
for i, entry in ipairs(frames) do
    assert(entry.Frame == i-1 and entry.Delay == (1/144)*seconds)
end
assert(#Sprite.LinearFrames(0, 1) == 0)
local other = Sprite.LinearFrames(3, 1.5)
assert(#other == 3 and other[1].Frame == 0 and other[3].Frame == 2)
assert(other[1].Delay == 0.5 and other[3].Delay == 0.5)
other[1].Frame = 99
assert(Sprite.LinearFrames(3, 1.5)[1].Frame == 0)
return Def.Sprite {
    Frames = frames,
    InitCommand = function(self) FRAME_SPRITE = self end,
}
"#,
        |context| context.defer_initial_live_update = true,
    );
    let scene = compiled.live_scene.take().expect("live sprite declaration");
    let actor = scene_global_table(&scene, "FRAME_SPRITE").expect("initialized authored sprite");
    let states = scene_sprite_states(&scene, actor).expect("native Frames table is consumed");
    assert_eq!(states.len(), 144);
    for (index, state) in states.iter().enumerate() {
        assert_eq!(state.frame, index as u32);
        assert_eq!(
            state.delay,
            ((1.0_f64 / 144.0) * (184.0 / 60.0 * 2.0)) as f32
        );
    }
}

#[test]
fn live_lyric_display_loads_text_layers_font_and_changed_commands() {
    let compiled = compile(
        "lyric-display-construction",
        r#"
return Def.ActorFrame {
    OnCommand = function()
        local screen = SCREENMAN:GetTopScreen()
        local lyrics = screen:GetChild('LyricDisplay')
        assert(lyrics:GetNumChildren() == 2, 'LyricDisplay must load both native text layers')
        LYRIC_DISPLAY = lyrics
        local back, front = lyrics:GetChild('LyricBack'), lyrics:GetChild('LyricFront')
        assert(back:GetParent() == lyrics and front:GetParent() == lyrics)
        assert(lyrics:GetParent() == screen)
        assert(back:GetName() == 'LyricBack' and front:GetName() == 'LyricFront')
        assert(back:GetText() == '' and front:GetText() == '')
        assert(lyrics:GetX() == SCREEN_CENTER_X and lyrics:GetY() == SCREEN_CENTER_Y + 160)
        for _, position in ipairs{{'SetReverse', -140}, {'SetOneReverse', -205}, {'SetNoReverse', 160}} do
            lyrics:playcommand(position[1])
            assert(lyrics:GetX() == SCREEN_CENTER_X and lyrics:GetY() == SCREEN_CENTER_Y + position[2])
        end
        local original_var = Var
        Var = function(name)
            if name == 'LyricText' then return 'fixture lyric' end
            if name == 'LyricDuration' then return 4 end
            if name == 'LyricColor' then return {0.8, 0.6, 0.4, 1} end
            return original_var(name)
        end
        for index, text in ipairs{back, front} do
            text:playcommand('Changed')
            assert(text:GetText() == 'fixture lyric')
            assert(math.abs(text:GetTweenTimeLeft() - 4.4) < 0.00001)
            local factor = index == 1 and 0.5 or 0.9
            local color = text:GetDiffuse()
            for channel, value in ipairs{0.8, 0.6, 0.4} do
                assert(math.abs(color[channel] - value * factor) < 0.00001)
            end
            text:playcommand('Changed')
            assert(math.abs(text:GetTweenTimeLeft() - 4.4) < 0.00001,
                'a changed lyric must replace the previous lyric tween')
        end
        Var = original_var
    end,
}
"#,
    );
    let scene = compiled.live_scene.expect("lyric scene");
    let lyrics = scene_global_table(&scene, "LYRIC_DISPLAY").unwrap();
    let children = scene_direct_children(&scene, lyrics).unwrap();
    assert_eq!(
        children
            .iter()
            .map(|child| scene_method_string(&scene, *child, "GetName").unwrap())
            .collect::<Vec<_>>(),
        ["LyricBack", "LyricFront"]
    );
    for child in children {
        let (font, path) = scene_font(&scene, child)
            .unwrap()
            .expect("inherited LyricDisplay text font");
        assert_eq!(font, "miso");
        assert!(path.ends_with("assets/fonts/miso/_miso light.ini"));
    }
}

#[test]
fn live_underlay_header_uses_native_quad_texture_and_init() {
    for (width, height, enabled) in [
        (640.0, 480.0, [true, false]),
        (854.0, 480.0, [true, true]),
        (1280.0, 720.0, [false, true]),
    ] {
        let mut compiled = compile_with_context(
            &format!("underlay-header-{width}"),
            "return Def.ActorFrame {}",
            |context| {
                context.screen_width = width;
                context.screen_height = height;
                context.players[0].enabled = enabled[0];
                context.players[1].enabled = enabled[1];
            },
        );
        let mut scene = compiled.live_scene.take().unwrap();
        let screen = scene_global_table(&scene, "__songlua_top_screen").unwrap();
        let underlay = scene_child(&scene, screen, "Underlay").unwrap();
        let header = scene_child(&scene, underlay, "Header").unwrap();
        assert!(
            scene_type_is(&scene, header, "Quad").unwrap(),
            "Underlay Header must be the native Quad, including its default texture"
        );
        scene_set_root(&scene, "HEADER", header).unwrap();
        scene_set_root(&scene, "UNDERLAY", underlay).unwrap();
        scene_exec(
            &scene,
            r#"
assert(HEADER:GetName() == 'Header' and HEADER:GetParent() == UNDERLAY)
assert(tostring(HEADER):find('Sprite', 1, true))
local texture = HEADER:GetTexture()
assert(texture ~= nil and texture:GetPath() == '')
assert(texture:GetSourceWidth() == 1 and texture:GetSourceHeight() == 1)
assert(texture:GetTextureWidth() == 1 and texture:GetTextureHeight() == 1)
assert(texture:GetImageWidth() == 1 and texture:GetImageHeight() == 1)
assert(HEADER:GetWidth() == 1 and HEADER:GetHeight() == 1)
assert(HEADER:GetZoomedWidth() == SCREEN_WIDTH and HEADER:GetZoomedHeight() == 80)
assert(HEADER:GetX() == SCREEN_WIDTH/2 and HEADER:GetY() == 0)
assert(HEADER:GetVAlign() == 0)
local color = HEADER:GetDiffuse()
assert(color[1] == 0 and color[2] == 0 and color[3] == 0 and math.abs(color[4] - 0.85) < 0.000001)
"#,
        )
        .unwrap();
        let state = scene_state(&scene, header).unwrap();
        assert_eq!((state.x, state.y), (width * 0.5, 0.0));
        assert_eq!((state.zoom_x, state.zoom_y), (width, 80.0));
        assert_eq!(state.diffuse, [0.0, 0.0, 0.0, 0.85]);
        let init = scene_field_function(&scene, header, "InitCommand").unwrap();
        assert_eq!(
            scene_function_source(&scene, init).unwrap().as_deref(),
            Some("@theme:/BGAnimations/ScreenGameplay underlay/Shared/Header.lua")
        );
        scene_exec(
            &scene,
            "HEADER:xy(1,2):diffuse(1,1,1,1):zoomto(4,5):valign(1)",
        )
        .unwrap();
        scene_call_method_function(&scene, init, header).unwrap();
        let output = scene
            .advance(SongLuaLiveFrame {
                delta_seconds: 0.01,
                screen_seconds: 0.01,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let restored = scene_state(&scene, header).unwrap();
        assert_eq!(
            (restored.x, restored.y, restored.zoom_x, restored.zoom_y),
            (width * 0.5, 0.0, width, 80.0)
        );
        assert_eq!(restored.diffuse, state.diffuse);
    }
}

#[test]
fn live_standard_life_children_follow_native_subactor_table_construction() {
    let compiled = compile("standard-life-child-table", "return Def.ActorFrame {}");
    let scene = compiled.live_scene.expect("real standard life owners");
    let screen = scene_global_table(&scene, "__songlua_top_screen").unwrap();
    let underlay = scene_child(&scene, screen, "Underlay").unwrap();
    let meters = scene_direct_children(&scene, underlay)
        .unwrap()
        .into_iter()
        .filter(|actor| {
            scene_optional_child(&scene, *actor, "MeterSwoosh")
                .unwrap()
                .is_some()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        meters.len(),
        2,
        "inspect both enabled native Standard owners"
    );
    for meter in meters {
        let parts = scene_direct_children(&scene, meter).unwrap();
        assert_eq!(parts.len(), 4);
        let ordered = scene_root_sequence(&scene, &parts).unwrap();
        scene_set_root(&scene, "STANDARD_OWNER", meter).unwrap();
        scene_set_root(&scene, "STANDARD_SUBACTORS", ordered).unwrap();
        scene_exec(
            &scene,
            r#"
local owner, parts = STANDARD_OWNER, STANDARD_SUBACTORS
assert(parts[1]:GetName() == '' and parts[2]:GetName() == '')
assert(parts[3]:GetName() == 'MeterFill' and parts[4]:GetName() == 'MeterSwoosh')
-- ActorFrame.cpp::PushChildrenTable starts empty and inserts each m_SubActor.
-- Standard.lua supplies two empty names, then MeterFill, then MeterSwoosh.
local expected = {}
rawset(expected, '', parts[1])
rawset(expected, '', {parts[1], parts[2]})
rawset(expected, 'MeterFill', parts[3])
rawset(expected, 'MeterSwoosh', parts[4])
local actual, wanted, observed = owner:GetChildren(), {}, {}
for name in pairs(expected) do wanted[#wanted + 1] = name end
for name in pairs(actual) do observed[#observed + 1] = name end
assert(table.concat(observed, '|') == table.concat(wanted, '|'),
    'Standard GetChildren must follow native subactor table construction')
assert(actual.MeterFill == parts[3] and actual.MeterSwoosh == parts[4])
assert(actual[''][1] == parts[1] and actual[''][2] == parts[2])
for _, part in ipairs(parts) do assert(part:GetParent() == owner) end
actual.MeterFill = nil
local fresh = owner:GetChildren()
assert(fresh ~= actual and fresh.MeterFill == parts[3])
assert(owner:GetChild('MeterFill') == parts[3] and owner:GetChild('MeterSwoosh') == parts[4])
"#,
        )
        .expect("native child insertion history, identities, groups and snapshot ownership");
    }
}

#[test]
fn live_underlay_difficulty_meter_owns_native_quad_and_text() {
    for (width, enabled_p1, enabled_p2) in [
        (640.0, true, false),
        (854.0, true, true),
        (854.0, false, true),
    ] {
        let source = r#"
DIFFICULTY_METERS = {}
return Def.Actor { OnCommand = function()
    local underlay = SCREENMAN:GetTopScreen():GetChild('Underlay')
    local unnamed = underlay:GetChildren()['']
    if unnamed and tostring(unnamed):find('ActorFrame', 1, true) then unnamed = {unnamed} end
    for _, owner in ipairs(unnamed or {}) do
        if owner.GetNumChildren and owner:GetNumChildren() == 2 then
            local parts = owner:GetChildren()['']
            if parts and #parts == 2 and tostring(parts[1]):find('Sprite', 1, true)
                and tostring(parts[2]):find('BitmapText', 1, true) then
                assert(owner:GetParent() == underlay and owner:GetName() == '')
                assert(parts[1]:GetParent() == owner and parts[2]:GetParent() == owner)
                assert(parts[1]:GetName() == '' and parts[2]:GetName() == '')
                assert(parts[1]:GetZoomedWidth() == 30 and parts[1]:GetZoomedHeight() == 30)
                assert(parts[1]:GetTexture():GetPath() == '')
                assert(math.abs(parts[2]:GetZoom() - 0.4) < 0.000001)
                DIFFICULTY_METERS[#DIFFICULTY_METERS + 1] = owner
            end
        end
    end
    assert(#DIFFICULTY_METERS == #GAMESTATE:GetHumanPlayers(),
        'Underlay must load each human player native DifficultyMeter frame')
end }
"#;
        let mut compiled = compile_with_context(
            &format!("underlay-difficulty-{width}-{enabled_p1}-{enabled_p2}"),
            source,
            |context| {
                context.screen_width = width;
                context.players[0].enabled = enabled_p1;
                context.players[1].enabled = enabled_p2;
                context.players[0].difficulty = SongLuaDifficulty::Hard;
                context.players[1].difficulty = SongLuaDifficulty::Easy;
                context.player_step_indices = [Some(0), Some(1)];
                context.song_steps = vec![
                    song::SongLuaStep {
                        steps_type: "StepsType_Dance_Single".into(),
                        difficulty: SongLuaDifficulty::Hard,
                        description: String::new(),
                        chart_name: String::new(),
                        credit: String::new(),
                        meter: 12,
                        music_file: String::new(),
                        possible_grade_points: 100,
                    },
                    song::SongLuaStep {
                        steps_type: "StepsType_Dance_Single".into(),
                        difficulty: SongLuaDifficulty::Easy,
                        description: String::new(),
                        chart_name: String::new(),
                        credit: String::new(),
                        meter: 4,
                        music_file: String::new(),
                        possible_grade_points: 100,
                    },
                ];
            },
        );
        let mut scene = compiled
            .live_scene
            .take()
            .expect("actual DifficultyMeter actors");
        let owners = scene_sequence_tables(
            &scene,
            scene_global_table(&scene, "DIFFICULTY_METERS").unwrap(),
        )
        .unwrap();
        let players = [enabled_p1, enabled_p2]
            .iter()
            .enumerate()
            .filter_map(|(player, enabled)| enabled.then_some(player))
            .collect::<Vec<_>>();
        for (owner, player) in owners.iter().zip(players) {
            let state = scene_state(&scene, *owner).unwrap();
            let sign = if player == 0 { -1.0 } else { 1.0 };
            let offset = if width == 640.0 { 292.5 } else { 342.5 };
            assert_eq!(state.x, width * 0.5 + sign * offset);
            assert_eq!(state.y, 56.0);
            let parts = scene_direct_children(&scene, *owner).unwrap();
            let text = &parts[1];
            assert_eq!(
                scene_field_string(&scene, *text, "Font", true).unwrap(),
                "Common Bold"
            );
            assert_eq!(
                scene_method_string(&scene, *text, "GetText").unwrap(),
                if player == 0 { "12" } else { "4" }
            );
            assert_eq!(
                scene_state(&scene, *text).unwrap().diffuse,
                [0.0, 0.0, 0.0, 1.0]
            );
            let color = scene_difficulty_color(
                &scene,
                if player == 0 {
                    "Difficulty_Hard"
                } else {
                    "Difficulty_Easy"
                },
            )
            .unwrap();
            assert_eq!(
                scene_state(&scene, parts[0]).unwrap().diffuse,
                std::array::from_fn(|index| scene_index_number(&scene, color, index + 1)
                    .map(|value| value as f32)
                    .unwrap())
            );
            for part in &parts {
                assert!(scene_has_function(&scene, *part, "BeginCommand").unwrap());
                assert!(
                    scene_has_function(&scene, *part, "CurrentSongChangedMessageCommand").unwrap()
                );
            }
        }
        scene_exec(&scene,r#"
local steps = GAMESTATE:GetCurrentSong():GetAllSteps()
for _, player in ipairs(GAMESTATE:GetHumanPlayers()) do GAMESTATE:SetCurrentSteps(player, steps[2]) end
MESSAGEMAN:Broadcast('CurrentSongChanged')
"#).unwrap();
        let output = scene
            .advance(SongLuaLiveFrame {
                delta_seconds: 0.01,
                screen_seconds: 0.01,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        for owner in owners {
            let parts = scene_direct_children(&scene, owner).unwrap();
            assert_eq!(
                scene_method_string(&scene, parts[1], "GetText").unwrap(),
                "4"
            );
        }
    }
}

#[test]
fn live_in_transition_loads_native_style_bitmaps_and_poses() {
    let compiled = compile(
        "in-transition-bitmaps",
        r#"
IN_SPRITES = {}
return Def.Actor { OnCommand = function()
    local transition = SCREENMAN:GetTopScreen():GetChild('In')
    local loaded = transition:GetChildren()['']
    local visuals = loaded:GetChildren()[''][1]
    local children = visuals:GetChildren()['']
    assert(#children == 4)
    for index = 2, 4 do
        local sprite = children[index]
        local texture = assert(sprite:GetTexture(), 'In visual Sprite must load its native bitmap')
        assert(sprite:GetParent() == visuals and sprite:GetName() == '')
        assert(sprite:GetWidth() == 640 and sprite:GetHeight() == 480)
        assert(texture:GetNumFrames() == 1)
        assert(texture:GetPath():find('/Technique/GameplayIn ', 1, true))
        assert(sprite:GetX() == SCREEN_CENTER_X and sprite:GetY() == SCREEN_CENTER_Y)
        local destination = ({1.1, 1.3, 0.9})[index - 1]
        assert(math.abs(sprite:GetZoomX() - destination) < 0.000001)
        assert(math.abs(sprite:GetZoomY() - destination) < 0.000001)
        IN_SPRITES[#IN_SPRITES + 1] = sprite
    end
    assert(ThemePrefs.Get('VisualStyle') == 'Technique')
end }
"#,
    );
    let scene = compiled
        .live_scene
        .expect("native In resource construction");
    let sprites = scene_global_table(&scene, "IN_SPRITES").unwrap();
    assert_eq!(
        scene_table_len(&scene, sprites).unwrap(),
        3,
        "inspect all three native In sprites"
    );
    let sprites = scene_sequence_tables(&scene, sprites).unwrap();
    let paths = sprites
        .iter()
        .map(|sprite| scene_texture_path(&scene, *sprite).unwrap().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        paths[0], paths[1],
        "the two splodes share the same native resource"
    );
    assert_ne!(paths[0], paths[2], "minisplode retains its own bitmap");
    for path in &paths {
        let texture = deadlib_assets::generated_texture(&path.to_string_lossy()).unwrap();
        assert_eq!(texture.source_size, [1280, 960]);
        assert!(
            texture.image.pixels().any(|pixel| pixel[3] != 0),
            "native bitmap contains visible pixels"
        );
    }
    for (index, sprite) in sprites.iter().enumerate() {
        assert_eq!(
            scene_method_number(&scene, *sprite, "GetTweenTimeLeft")
                .map(|value| value as f32)
                .unwrap(),
            if index == 2 { 1.2 } else { 1.0 }
        );
        let pose = scene_state(&scene, *sprite).unwrap();
        assert_eq!(
            pose.zoom * pose.zoom_x,
            0.0,
            "native rendered X scale starts at zero"
        );
        assert_eq!(
            pose.zoom * pose.zoom_y,
            0.0,
            "native rendered Y scale starts at zero"
        );
        assert_eq!(
            scene_state(&scene, *sprite).unwrap().rot_z_deg,
            if index == 1 { -10.0 } else { 10.0 }
        );
    }
}

#[test]
fn live_out_transition_loads_quad_directly_with_native_commands() {
    let mut compiled = compile(
        "out-transition-direct-quad",
        r#"
return Def.Actor { OnCommand = function()
    OUT = SCREENMAN:GetTopScreen():GetChild('Out')
    assert(OUT:GetNumChildren() == 1)
    OUT_QUAD = OUT:GetChildren()['']
    assert(tostring(OUT_QUAD):find('Sprite', 1, true),
        'Out must load its native Quad directly without an extra ActorFrame')
    assert(OUT_QUAD:GetParent() == OUT and OUT_QUAD:GetName() == '')
    assert(OUT_QUAD.GetNumChildren == nil)
    assert(OUT_QUAD:GetZoomedWidth() == SCREEN_WIDTH)
    assert(OUT_QUAD:GetZoomedHeight() == SCREEN_HEIGHT)
    assert(type(OUT_QUAD.OffCommand) == 'function')
end }
"#,
    );
    let mut scene = compiled.live_scene.take().expect("native Out loaded child");
    let out = scene_global_table(&scene, "OUT").unwrap();
    let quad = scene_global_table(&scene, "OUT_QUAD").unwrap();
    let children = scene_direct_children(&scene, out).unwrap();
    assert_eq!(children.len(), 1, "inspect the actual transition owner");
    assert_eq!(
        scene_table_identity(&scene, children[0]).unwrap(),
        scene_table_identity(&scene, quad).unwrap()
    );
    assert_eq!(
        scene_table_identity(
            &scene,
            scene_field_table(&scene, out, "__songlua_transition_child", true).unwrap()
        )
        .unwrap(),
        scene_table_identity(&scene, quad).unwrap()
    );
    assert_eq!(scene_state(&scene, quad).unwrap().diffuse, [0.0; 4]);
    assert_eq!(
        scene_method_number(&scene, quad, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        1.5
    );
    scene_exec(&scene, "OUT_QUAD:playcommand('Off')")
        .expect("native Off handler with fresh human profiles");
    scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.25,
            delta_seconds: 0.25,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        scene_method_number(&scene, quad, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        1.5,
        "waiting transition freezes its loaded Quad"
    );
    assert_eq!(scene_state(&scene, quad).unwrap().diffuse[3], 0.0);
    scene_start_transition(&scene, out).unwrap();
    for (seconds, delta, left, alpha) in [
        (0.5, 0.25, 1.25, 0.0),
        (0.75, 0.25, 1.0, 0.0),
        (1.25, 0.5, 0.5, 0.5),
        (1.75, 0.5, 0.0, 1.0),
    ] {
        let output = scene
            .advance(SongLuaLiveFrame {
                screen_seconds: seconds,
                delta_seconds: delta,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert!(
            (scene_method_number(&scene, out, "GetTweenTimeLeft")
                .map(|value| value as f32)
                .unwrap()
                - left)
                .abs()
                < 1e-6
        );
        assert!((scene_state(&scene, quad).unwrap().diffuse[3] - alpha).abs() < 1e-6);
    }
    scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 1.76,
            delta_seconds: 0.01,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        scene_field_number(&scene, out, "__songlua_transition_state", true)
            .map(|value| value as u8)
            .unwrap(),
        2
    );
}

#[test]
fn live_transition_getter_uses_loaded_child_and_native_lifecycle() {
    let mut compiled = compile(
        "transition-virtual-getter",
        r#"
OUT = SCREENMAN:GetTopScreen():GetChild('Out')
return Def.Actor { OnCommand=function(self)
    OUT:smooth(7):diffusealpha(0)
    assert(OUT:GetTweenTimeLeft() == 0, 'waiting transition ignores its own queue')
end }
"#,
    );
    let mut scene = compiled.live_scene.take().expect("transition scene");
    let out = scene_global_table(&scene, "OUT").unwrap();
    let child = scene_field_table(&scene, out, "__songlua_transition_child", true).unwrap();
    assert_eq!(
        scene_method_number(&scene, child, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        1.5
    );
    scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.25,
            delta_seconds: 0.25,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        scene_method_number(&scene, child, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        1.5,
        "waiting transition freezes its loaded child"
    );
    scene_start_transition(&scene, out).unwrap();
    assert_eq!(
        scene_method_number(&scene, out, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        1.5
    );
    for (seconds, delta, expected) in [(0.5, 0.25, 1.25), (1.75, 1.25, 0.0), (2.0, 0.25, 0.0)] {
        let output = scene
            .advance(SongLuaLiveFrame {
                screen_seconds: seconds,
                delta_seconds: delta,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert_eq!(
            scene_method_number(&scene, out, "GetTweenTimeLeft")
                .map(|value| value as f32)
                .unwrap(),
            expected
        );
    }
    assert_eq!(
        scene_field_number(&scene, out, "__songlua_transition_state", true)
            .map(|value| value as u8)
            .unwrap(),
        2,
        "completion is checked before the next child update"
    );
    scene_start_transition(&scene, out).unwrap();
    assert_eq!(
        scene_field_number(&scene, out, "__songlua_transition_state", true)
            .map(|value| value as u8)
            .unwrap(),
        2,
        "a finished transition cannot restart before Reset"
    );
}

#[test]
fn live_outro_rebuilds_song_options_after_queued_screen_message() {
    let mut compiled = compile(
        "outro-options",
        r#"
local ps = GAMESTATE:GetPlayerState(PLAYER_1)
PREFERRED = ps:GetPlayerOptions('ModsLevel_Preferred')
STAGE = ps:GetPlayerOptions('ModsLevel_Stage')
SONG = ps:GetPlayerOptions('ModsLevel_Song')
CURRENT = ps:GetPlayerOptions('ModsLevel_Current')
SPEEDS = {}
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function(self, delta)
        if delta <= 0 then return end
        local value, speed = SONG:Reverse(0, 1000)
        SPEEDS[#SPEEDS + 1] = {value, speed}
    end)
end }
"#,
    );
    let mut scene = compiled.live_scene.take().expect("owned scene");
    scene_exec(
        &scene,
        r#"
            PREFERRED:Reverse(0.2, 0.25)
            STAGE:Reverse(0.4, 0.5)
            SONG:Reverse(1, 1000)
            CURRENT:Reverse(0)
        "#,
    )
    .expect("distinct option levels");
    let mut frame = SongLuaLiveFrame {
        screen_seconds: 0.125,
        delta_seconds: 0.125,
        ..Default::default()
    };
    for index in 0..4 {
        frame.gameplay_outro = index > 0;
        scene.advance(frame.clone()).expect("outro frame");
        if index == 2 {
            let current = scene_number(&scene, "return CURRENT:Reverse()")
                .map(|value| value as f32)
                .expect("current retained after Song assignment");
            assert_eq!(current, 0.0);
            assert_eq!(
                scene_number(&scene, "return SONG:Reverse()")
                    .map(|value| value as f32)
                    .expect("stage copied after actor callbacks"),
                0.4
            );
        }
        frame.screen_seconds += 0.125;
    }
    let speeds = scene_global_table(&scene, "SPEEDS").expect("actual setter returns");
    for (index, value, speed) in [
        (1, 1.0, 1000.0),
        (2, 0.0, 1000.0),
        (3, 0.0, 1000.0),
        (4, 0.4, 0.5),
    ] {
        let result = scene_index_table(&scene, speeds, index).expect("callback result");
        assert_eq!(
            scene_index_number(&scene, result, 1)
                .map(|value| value as f32)
                .expect("previous value"),
            value
        );
        assert_eq!(
            scene_index_number(&scene, result, 2)
                .map(|value| value as f32)
                .expect("previous approach speed"),
            speed
        );
    }
    assert_eq!(
        scene_number(&scene, "return CURRENT:Reverse()")
            .map(|value| value as f32)
            .expect("Current approaches the rebuilt Song target"),
        0.0625
    );
    scene.advance(frame.clone()).expect("continued outro");
    assert_eq!(
        scene_index_number(
            &scene,
            scene_index_table(&scene, speeds, 5).expect("continued setter"),
            2
        )
        .map(|value| value as f32)
        .expect("reset occurs once"),
        1000.0
    );
    assert_eq!(scene_two_numbers_bool(&scene,"return PREFERRED:Reverse(), STAGE:Reverse(), SONG == GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')").expect("retained references"), (0.2, 0.4, true));
}

#[test]
fn live_player_attack_display_preserves_native_inventory_and_hold_indices() {
    for (style, columns) in [("single", 4), ("double", 8)] {
        let source = r#"
INVENTORY_PLAYERS = {}
return Def.ActorFrame { OnCommand = function()
local screen = SCREENMAN:GetTopScreen()
for index = 1, $PLAYERS do
    local player = screen:GetChild('PlayerP' .. index)
    assert(player:GetNumChildren() == $COLUMNS + 4,
        'visible Player must include its native AttackDisplay')
    local children = player:GetChildren()['']
    local named = player:GetChild('')
    assert(#children == $COLUMNS + 1 and #named == #children)
    local attack = children[1]
    assert(attack:GetName() == '' and attack:GetParent() == player)
    assert(tostring(attack):find('ActorFrame', 1, true) and attack:GetNumChildren() == 0)
    assert(attack:GetX() == -40 and attack:GetY() == 0 and attack:GetTweenTimeLeft() == 0)
    for index = 1, #children do
        assert(named[index] == children[index], 'lookups share the same unnamed actors')
        if index > 1 then
            local hold = children[index]
            assert(tostring(hold):find('HoldJudgment', 1, true))
            assert(hold:GetParent() == player and hold:GetNumChildren() == 1)
            assert(hold:GetChild('HoldJudgment'):GetParent() == hold)
        end
    end
    table.insert(INVENTORY_PLAYERS, player)
end
end }
"#
        .replace("$COLUMNS", &columns.to_string())
        .replace("$PLAYERS", if style == "single" { "2" } else { "1" });
        let compiled = compile_with_context(
            &format!("player-attack-display-{style}"),
            &source,
            |context| {
                context.style_name = style.into();
                context.players[1].enabled = style == "single";
            },
        );
        let scene = compiled.live_scene.expect("native Player inventory");
        let players = scene_global_table(&scene, "INVENTORY_PLAYERS").unwrap();
        assert_eq!(
            scene_table_len(&scene, players).unwrap(),
            if style == "single" { 2 } else { 1 },
            "regression must inspect every configured visible player"
        );
        for player in scene_sequence_tables(&scene, players)
            .unwrap()
            .into_iter()
            .map(Ok::<_, Error>)
        {
            let player = player.unwrap();
            let children = scene_direct_children(&scene, player).unwrap();
            let mut expected = vec!["", "Combo", "Judgment"];
            expected.extend(std::iter::repeat_n("", columns));
            expected.push("NoteField");
            assert_eq!(
                children
                    .iter()
                    .map(|child| scene_method_string(&scene, *child, "GetName").unwrap())
                    .collect::<Vec<_>>(),
                expected,
                "native constructor and Init child order"
            );
            let holds =
                scene_field_table(&scene, player, "__songlua_hold_judgments", true).unwrap();
            assert_eq!(
                scene_table_len(&scene, holds).unwrap(),
                columns,
                "feedback retains exactly one frame per column"
            );
            for column in 0..columns {
                assert_eq!(
                    scene_table_identity(
                        &scene,
                        scene_index_table(&scene, holds, column + 1).unwrap()
                    )
                    .unwrap(),
                    scene_table_identity(&scene, children[column + 3]).unwrap(),
                    "attack display must not shift hold indices"
                );
            }
        }
    }
}

#[test]
fn live_cancel_transition_loads_quad_and_native_fade() {
    let mut compiled = compile(
        "cancel-transition-child",
        r#"
return Def.Actor { OnCommand = function(self) self:sleep(9999) end }
"#,
    );
    let mut scene = compiled.live_scene.take().expect("cancel transition scene");
    let screen: deadlib_lua::Root = scene_global_table(&scene, "__songlua_top_screen").unwrap();
    let cancel = scene_child(&scene, screen, "Cancel").unwrap();
    assert_eq!(
        scene_method_number(&scene, cancel, "GetNumChildren")
            .map(|value| value as usize)
            .unwrap(),
        1,
        "Cancel must retain its native loaded transition child"
    );
    let children = scene_method_table(&scene, cancel, "GetChildren").unwrap();
    let quad = scene_field_table(&scene, children, "", true).expect("unnamed cancel Quad");
    assert_eq!(
        scene_table_identity(
            &scene,
            scene_method_table(&scene, quad, "GetParent").unwrap()
        )
        .unwrap(),
        scene_table_identity(&scene, cancel).unwrap()
    );
    assert_eq!(
        scene_method_number(&scene, quad, "GetZoomedWidth")
            .map(|value| value as f32)
            .unwrap(),
        scene.selected_context().unwrap().screen_width
    );
    assert_eq!(
        scene_method_number(&scene, quad, "GetZoomedHeight")
            .map(|value| value as f32)
            .unwrap(),
        scene.selected_context().unwrap().screen_height
    );
    assert_eq!(scene_state(&scene, quad).unwrap().diffuse[3], 0.0);
    scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.25,
            delta_seconds: 0.25,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        scene_method_number(&scene, cancel, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        0.0
    );
    scene_start_transition(&scene, cancel).unwrap();
    assert!(
        (scene_method_number(&scene, cancel, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap()
            - 0.5)
            .abs()
            < 1e-6
    );
    for (seconds, delta, alpha) in [(0.35, 0.1, 0.0), (0.55, 0.2, 0.5), (0.75, 0.2, 1.0)] {
        let output = scene
            .advance(SongLuaLiveFrame {
                screen_seconds: seconds,
                delta_seconds: delta,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        assert!((scene_state(&scene, quad).unwrap().diffuse[3] - alpha).abs() < 1e-5);
    }
    scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.76,
            delta_seconds: 0.01,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        scene_field_number(&scene, cancel, "__songlua_transition_state", true)
            .map(|value| value as u8)
            .unwrap(),
        2
    );
}

#[test]
fn live_native_scalar_methods_follow_receivers_and_reset() {
    let mut compiled = compile_with_context(
        "native-scalar-methods",
        r#"
return Def.ActorFrame {
    Def.Quad { InitCommand=function(self) A=self end },
    Def.Quad { InitCommand=function(self) B=self end },
}
"#,
        |context| context.defer_initial_live_update = true,
    );
    let mut scene = compiled.live_scene.take().unwrap();
    scene.advance(SongLuaLiveFrame::default()).unwrap();
    scene_exec(
        &scene,
        r#"
SET_X, ADD_X = Actor.x, Actor.addx
SET_X(A, 4); SET_X(B, 20)
A:linear(1):x(12):linear(1):x(20)
ADD_X(A, 4); ADD_X(B, 2)
assert(A:GetX()==4 and A:GetDestX()==24)
assert(B:GetX()==22 and B:GetDestX()==22)
-- Public methods remain replaceable, and saved methods use their receiver.
local native = Actor.x
Actor.x = function(self, value) return native(self, value + 1) end
B:x(30)
Actor.x = native
assert(B:GetX()==31 and A:GetDestX()==24)
"#,
    )
    .unwrap();
    let output = scene
        .advance(SongLuaLiveFrame {
            delta_seconds: 0.5,
            screen_seconds: 0.5,
            music_seconds: 0.5,
            ..Default::default()
        })
        .unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    scene_exec(
        &scene,
        r#"
assert(A:GetX()==8 and A:GetDestX()==24 and B:GetX()==31)
A:stoptweening()
ADD_X(A, 5)
assert(A:GetX()==13 and A:GetDestX()==13)
"#,
    )
    .unwrap();
    let actor: deadlib_lua::Root = scene_global_table(&scene, "A").unwrap();
    scene_init_state(&scene, actor).unwrap();
    scene_exec(
        &scene,
        r#"
assert(A:GetX()==0 and A:GetDestX()==0)
SET_X(A, -3)
assert(A:GetX()==-3)
A:linear(1)
SET_X(A, 16)
ADD_X(A, 4)
assert(A:GetX()==-3 and A:GetDestX()==20 and B:GetX()==31)
A:finishtweening()
assert(A:GetX()==20)
"#,
    )
    .unwrap();
}

#[test]
fn explicit_actor_children() {
    let mut compiled = compile(
        "explicit-children",
        r#"
return Def.ActorFrame{
 Name='Root',
 Def.Quad{InitCommand=function() error('excluded direct child') end},
 children={
  [1]=Def.Quad{Name='Head',InitCommand=function(self) self:x(1) end},
  [3]=Def.ActorFrame{Name='Tail',InitCommand=function(self) self:x(3) end},
  named=Def.Quad{Name='Named',InitCommand=function(self) self:x(7) end},
 },
 OnCommand=function(self)
  assert(self:GetNumChildren()==3)
  assert(self:GetChild('Head'):GetX()==1)
  assert(self:GetChild('Tail'):GetX()==3)
  assert(self:GetChild('Named'):GetX()==7)
  self:aux(1)
 end,
}
"#,
    );
    let mut scene = compiled.live_scene.take().expect("owned scene");
    let output = scene
        .advance(SongLuaLiveFrame::default())
        .expect("child activation");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    let actor: Root = scene_entry_root(&scene).expect("root");
    assert_eq!(
        scene_method_number(&scene, actor, "getaux")
            .map(|value| value as f32)
            .expect("On result"),
        1.0
    );
}

fn scene_new_table(scene: &SongLuaLiveScene) -> Result<Root, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let table = vm.try_new_table()?;
    vm.pin(Value::Table(table))
}
fn scene_set_field_number(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: &str,
    value: f64,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let Value::Table(table) = vm.rooted(owner)? else {
        unreachable!()
    };
    let key = vm.try_intern(name.as_bytes())?;
    vm.raw_table_set(table, key, Value::Number(value))
}
fn scene_set_field_root(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: &str,
    root: Root,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let Value::Table(table) = vm.rooted(owner)? else {
        unreachable!()
    };
    let value = vm.rooted(root)?;
    let key = vm.try_intern(name.as_bytes())?;
    vm.raw_table_set(table, key, value)
}
fn scene_set_index_root(
    scene: &SongLuaLiveScene,
    owner: Root,
    index: usize,
    root: Root,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let Value::Table(table) = vm.rooted(owner)? else {
        unreachable!()
    };
    let value = vm.rooted(root)?;
    vm.raw_table_set(table, Value::Number(index as f64), value)
}

#[test]
fn live_foreground_directory_uses_requested_default_chunk_name() {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-live-directory-source-{}",
        std::process::id()
    ));
    let folder = dir.join("directory");
    fs::create_dir_all(&folder).expect("foreground directory");
    let entry = folder.join("Default.lua");
    fs::write(
        &entry,
        r#"
CONSTRUCTION_SOURCE = debug.getinfo(1, 'S').source
local function on(self)
    self:aux(17)
end
CALLBACK_SOURCE = debug.getinfo(on, 'S').source
return Def.Actor { OnCommand = on }
"#,
    )
    .expect("case-sensitive physical source");
    for (name, expected) in [
        (
            "directory",
            song::file_path_string(&folder.join("default.lua")),
        ),
        (
            "directory/",
            format!("{}//default.lua", song::file_path_string(&folder)),
        ),
        (
            "./",
            format!("{}/.//default.lua", song::file_path_string(&dir)),
        ),
        ("directory/Default.lua", song::file_path_string(&entry)),
        (
            "./directory/Default.lua",
            format!("{}/./directory/Default.lua", song::file_path_string(&dir)),
        ),
        (
            "directory//Default.lua",
            format!("{}/directory//Default.lua", song::file_path_string(&dir)),
        ),
    ] {
        let physical_entry = if name == "./" {
            let root_entry = dir.join("Default.lua");
            fs::copy(&entry, &root_entry).expect("root directory entry");
            root_entry
        } else {
            entry.clone()
        };
        let mut context = SongLuaCompileContext::new(&dir, "foreground source");
        context.live = true;
        context.foreground_names = vec![name.into()];
        context.layer_start_beats = vec![0.0];
        let mut compiled: Compiled = song::compile_song_lua(
            &physical_entry,
            &context,
            SongLuaNoteskinResolver::default(),
            |_, _, _| Ok(Arc::<[()]>::from([])),
            |_| None,
            |_, _| None,
        )
        .expect("read physical file with native source name");
        let mut scene = compiled.live_scene.take().expect("retained Lua scene");
        let output = scene
            .advance(SongLuaLiveFrame {
                music_seconds: 0.02,
                beat: 0.01,
                screen_seconds: 0.1,
                delta_seconds: 0.1,
                ..Default::default()
            })
            .expect("foreground callback");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        for key in ["CONSTRUCTION_SOURCE", "CALLBACK_SOURCE"] {
            let source = scene_global_string(&scene, key).expect("Lua source name");
            assert_eq!(source.trim_start_matches('@').replace('\\', "/"), expected);
        }
        assert!(scene.states().iter().any(|state| state.aux == 17.0));
    }
}

#[test]
fn live_loadactor_collapses_paths_before_loading_and_naming_chunks() {
    let dir =
        std::env::temp_dir().join(format!("deadsync-live-actor-paths-{}", std::process::id()));
    fs::create_dir_all(dir.join("parts")).expect("actor directory");
    fs::write(
        dir.join("parts/child.lua"),
        r#"
CHILD_SOURCE = debug.getinfo(1, 'S').source
return Def.ActorFrame { LoadActor('.././sibling.lua') }
"#,
    )
    .expect("nested actor script");
    fs::write(
        dir.join("sibling.lua"),
        r#"
SIBLING_SOURCE = debug.getinfo(1, 'S').source
return Def.Actor { OnCommand = function(self) self:aux(19) end }
"#,
    )
    .expect("sibling script");
    let entry = dir.join("default.lua");
    fs::write(
        &entry,
        "return Def.ActorFrame { LoadActor('./parts/missing/../child.lua') }",
    )
    .expect("actor path aliases");
    let mut context = SongLuaCompileContext::new(&dir, "actor path collapse");
    context.live = true;
    let mut compiled: Compiled = song::compile_song_lua(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("native collapse happens before checking missing directory");
    let scene = compiled.live_scene.take().expect("retained Lua scene");
    for (key, path) in [
        ("CHILD_SOURCE", dir.join("parts/child.lua")),
        ("SIBLING_SOURCE", dir.join("sibling.lua")),
    ] {
        let source = scene_global_string(&scene, key).expect("actual Lua chunk source");
        assert_eq!(
            source.trim_start_matches('@').replace('\\', "/"),
            song::file_path_string(&path)
        );
    }
    assert!(
        scene.states().iter().any(|state| state.aux == 19.0),
        "loaded sibling On command"
    );
}

#[test]
fn live_column_cues_derive_owned_tweens_from_chart_notes() {
    let compiled =
        compile_with_context("column-cue-owned-tweens", "return Def.Actor{}", |context| {
            context.players[0].column_cues = true;
            context.players[1].enabled = false;
            context.song_steps = vec![song::SongLuaStep {
                steps_type: "StepsType_Dance_Single".into(),
                difficulty: SongLuaDifficulty::Challenge,
                description: String::new(),
                chart_name: String::new(),
                credit: String::new(),
                meter: 9,
                music_file: String::new(),
                possible_grade_points: 0,
            }];
            context.player_step_indices = [Some(0), None];
            // Real payloads minimize row divisions. Four stored rows can
            // span twelve beats; their indices are not native 48ths.
            context.song_step_notes = vec![Arc::from(&b"1000\n0M00\n00F0\n000L\n"[..])];
            context.song_step_row_beats = vec![Arc::from([0.0, 4.0, 8.0, 12.0])];
        });
    let mut scene = compiled.live_scene.expect("cue scene");
    finish_note_field_intro_fixture(&scene);
    scene_exec(&scene,r#"
local steps = GAMESTATE:GetCurrentSteps(PLAYER_1)
local cues = steps:GetColumnCues()
assert(#cues == 4 and cues[1].startTime == 0 and cues[2].duration == 4)
assert(cues[2].columns[1].colNum == 2 and cues[2].columns[1].noteType == 4)
assert(#cues[3].columns == 0 and cues[4].columns[1].noteType == 5)
assert(#steps:GetColumnCues(4) == 2, 'duration threshold is strict; previous zero is special')
ParseColumnCues(steps, 'P1')
assert(SL.P1.Streams.ColumnCues[2].columns[1].isMine)
assert(steps:GetColumnCues()[2].columns[1].noteType == 4, 'fresh mutable query results')
UNDERLAY = SCREENMAN:GetTopScreen():GetChild('Underlay')
CONTAINER = UNDERLAY:GetChild('NoteFieldContainerP1')
CUES = CONTAINER:GetChild('')[2] -- native duplicate-name group: ColumnFlash, ColumnCues, DisplayMods
FLASH = CUES:GetChild('Column1'):GetChild('ColumnFlash')
assert(FLASH:GetParent():GetParent() == CUES and CUES:GetParent() == CONTAINER)
"#).expect("native cue getter, parser and ownership");
    for (screen, music, delta, expected) in [(0.1, -1.0, 0.1, 1.0), (0.3, -0.8, 0.2, 0.8)] {
        let output = scene
            .advance(SongLuaLiveFrame {
                screen_seconds: screen,
                music_seconds: music,
                delta_seconds: delta,
                ..Default::default()
            })
            .expect("cue frame");
        assert!(output.errors.is_empty(), "{:?}", output.errors);

        let flash = scene_global_table(&scene, "FLASH").unwrap();
        let underlay = scene_global_table(&scene, "UNDERLAY").unwrap();
        let remaining = scene_method_number(&scene, flash, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap();
        assert!(
            (remaining - expected).abs() < 1e-6,
            "{remaining} != {expected}"
        );
        assert_eq!(
            scene_method_number(&scene, underlay, "GetTweenTimeLeft")
                .map(|value| value as f32)
                .unwrap(),
            remaining
        );
    }
}

#[test]
fn live_reload() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
    let native: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/itgmania-song-lua-micro/startup-reload-native.json"
    ))
    .expect("native control");
    let song = deadsync_simfile::song::parse_song_meta_file(
        &fixtures.join("startup-reload.ssc"),
        &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        0.0,
        |_| 0.0,
    )
    .expect("native simfile");
    let mut context = SongLuaCompileContext::new(&fixtures, "Native reload");
    context.live = true;
    context.song_steps = song.charts.iter().map(song::SongLuaStep::from).collect();
    context.player_step_indices[0] = context
        .song_steps
        .iter()
        .position(|steps| steps.difficulty == SongLuaDifficulty::Challenge);
    context.players[0].difficulty = SongLuaDifficulty::Challenge;
    let mut compiled = song::compile_song_lua(
        &fixtures.join("startup-reload.lua"),
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None::<song::SongLuaOverlayModelLayer<()>>,
        |_, _| None,
    )
    .expect("retained native control");
    assert_eq!(compiled.startup_reloads, 1);
    let scene = compiled.live_scene.take().expect("single VM");
    assert_eq!(
        scene_global_number(&scene, "HARNESS_NATIVE_RELOAD_COUNT")
            .map(|value| value as u32)
            .expect("same VM count"),
        2
    );
    assert_eq!(scene_number(&scene,"return STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1):GetPossibleDancePoints()").map(|value|value as i32).expect("current maximum"), native["snapshots"][1]["possible"].as_i64().expect("native maximum") as i32);
    assert_eq!(
        scene.selected_context().unwrap().song_steps
            [scene.selected_context().unwrap().player_step_indices[0].expect("selected chart")]
        .difficulty,
        SongLuaDifficulty::Easy
    );
}

#[test]
fn live_reload_updates() {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-live-reload-updates-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("reload directory");
    let entry = dir.join("default.lua");
    fs::write(
        &entry,
        r#"
            RELOAD_PASS = (RELOAD_PASS or 0) + 1
            local pass = RELOAD_PASS
            UPDATES = UPDATES or {0, 0}
            return Def.ActorFrame {
                InitCommand=function(self)
                    -- Installing a new callback invalidates the update plan.
                    -- Leave scene 2 without one so the test cannot hide a
                    -- stale plan by replacing it through SetUpdateFunction.
                    if pass == 1 then
                        self:SetUpdateFunction(function() UPDATES[pass] = UPDATES[pass] + 1 end)
                    end
                end,
                OnCommand=function(self)
                    self:sleep(2)
                    if pass == 1 then SCREENMAN:SetNewScreen('ScreenGameplay') end
                end,
                Def.ActorFrame {
                    OnCommand=function(self) self:sleep(0.05):queuecommand('Tick') end,
                    TickCommand=function(self)
                        TICK_PASS = pass
                        SCREENMAN:GetTopScreen():StartTransitioningScreen('SM_DoNextScreen')
                    end,
                },
            }
        "#,
    )
    .expect("reload source");
    let mut context = SongLuaCompileContext::new(&dir, "reload updates");
    context.live = true;
    context.layer_start_beats = vec![0.0];
    let mut compiled = song::compile_song_lua(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None::<song::SongLuaOverlayModelLayer<()>>,
        |_, _| None,
    )
    .expect("first screen");
    let mut scene = compiled.live_scene.take().expect("first VM scene");
    let mut frame = SongLuaLiveFrame::default();
    frame.players[0].last_judgment = Some(deadsync_gameplay::JudgmentRenderInfo {
        started_at_screen_s: 0.0,
        judgment: deadsync_rules::judgment::Judgment {
            grade: deadsync_rules::judgment::JudgeGrade::Fantastic,
            window: Some(deadsync_rules::judgment::TimingWindow::W0),
            time_error_ms: 0.0,
            time_error_music_ns: 0,
            miss_because_held: false,
        },
    });
    let output = scene.advance(frame.clone()).expect("first foreground On");
    assert_eq!(
        output.request,
        Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into()))
    );
    let mut layers = song::reload_song_lua(
        scene,
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None::<song::SongLuaOverlayModelLayer<()>>,
    )
    .expect("replacement screen");
    let mut scene = layers[0].live_scene.take().expect("replacement scene");
    let player: deadlib_lua::Root =
        scene_global_table(&scene, "__songlua_top_screen_player_1").expect("replacement player");
    assert_eq!(
        scene_method_number(&scene, player, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .expect("new player queue"),
        0.0,
        "a retired screen's judgment must not animate the replacement during startup"
    );
    let output = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.1,
            beat: 0.1,
            screen_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .expect("replacement update");
    assert_eq!(
        output.request,
        Some(SongLuaLiveRequest::Transition("SM_DoNextScreen".into()))
    );
    let updates = scene_global_table(&scene, "UPDATES").expect("scene updates");
    assert_eq!(
        scene_index_field_number(&scene, updates, 1, false)
            .map(|value| value as u32)
            .expect("retired updates"),
        1
    );
    assert_eq!(
        scene_index_field_number(&scene, updates, 2, false)
            .map(|value| value as u32)
            .expect("replacement has no callback"),
        0
    );
    assert_eq!(
        scene_global_number(&scene, "TICK_PASS")
            .map(|value| value as u32)
            .expect("new child queue"),
        2
    );
}

#[test]
fn live_screen_positions_survive_registration_and_seed_authored_tweens() {
    let dir =
        std::env::temp_dir().join(format!("deadsync-live-screen-pose-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("screen position directory");
    let entry = dir.join("default.lua");
    fs::write(
        &entry,
        r#"
PLAYER = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
FIELD = PLAYER:GetChild('NoteField')
START_X, START_Y, START_FIELD_Y = PLAYER:GetX(), PLAYER:GetY(), FIELD:GetY()
return Def.ActorFrame { OnCommand = function(self)
    self:SetUpdateFunction(function(self, delta)
        if delta > 0 and FIRST_X == nil then
            FIRST_X, FIRST_Y, FIRST_FIELD_Y = PLAYER:GetX(), PLAYER:GetY(), FIELD:GetY()
        end
    end)
end }
"#,
    )
    .expect("independent position control");
    let mut context = SongLuaCompileContext::new(&dir, "screen positions");
    context.live = true;
    context.players[0].screen_x = 231.25;
    context.players[0].screen_y = 199.0;
    context.players[1].screen_x = 620.0;
    let mut compiled: Compiled = song::compile_song_lua(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("configured screen positions");
    let mut scene = compiled.live_scene.take().expect("retained screen scene");

    let field_y = f32::midpoint(song::THEME_RECEPTOR_Y_STD, song::THEME_RECEPTOR_Y_REV);
    for (name, value) in [
        ("START_X", 231.25),
        ("START_Y", 199.0),
        ("START_FIELD_Y", field_y),
    ] {
        assert_eq!(
            scene_global_number(&scene, name)
                .map(|value| value as f32)
                .expect("startup getter"),
            value,
            "{name}"
        );
    }
    let output = scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .expect("first live position frame");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    for (name, value) in [
        ("FIRST_X", 231.25),
        ("FIRST_Y", 199.0),
        ("FIRST_FIELD_Y", field_y),
    ] {
        assert_eq!(
            scene_global_number(&scene, name)
                .map(|value| value as f32)
                .expect("positive frame getter"),
            value,
            "{name}"
        );
    }
    scene_exec(&scene, "PLAYER:linear(0.4):x(431.25)").expect("authored player tween");
    let output = scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.2,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .expect("player tween advances");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    scene_exec(
        &scene,
        "assert(math.abs(PLAYER:GetX() - 281.25) < 0.0001); assert(PLAYER:GetDestX() == 431.25)",
    )
    .expect("tween starts from the initialized pose");
}

#[test]
fn foreground_name_changes_after_init() {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-live-foreground-name-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("test directory");
    let entry = dir.join("default.lua");
    fs::write(
        &entry,
        r#"
return Def.ActorFrame{
 Name='Declared',
 InitCommand=function(self)
  assert(self:GetName()=='Declared')
  self:name('from-init')
  assert(self:GetName()=='from-init')
 end,
 OnCommand=function(self)
  assert(self:GetName()=='scripts')
  self:aux(1)
 end,
}
"#,
    )
    .expect("test script");
    let mut context = SongLuaCompileContext::new(&dir, "foreground-name");
    context.live = true;
    context.layer_start_beats = vec![0.0];
    context.foreground_names = vec!["scripts".into()];
    context.music_length_seconds = 60.0;
    let mut compiled: Compiled = song::compile_song_lua(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("foreground preparation");
    let mut scene = compiled.live_scene.take().expect("owned scene");
    let actor: Root = scene_entry_root(&scene).expect("foreground root");
    assert_eq!(
        scene_field_string(&scene, actor, "Name", false).expect("loaded name"),
        "scripts"
    );
    scene
        .advance(SongLuaLiveFrame::default())
        .expect("foreground activation");
    assert_eq!(
        scene_method_number(&scene, actor, "getaux")
            .map(|value| value as f32)
            .expect("On result"),
        1.0
    );
}

#[test]
fn live_arrow_rotation_z_uses_native_player_beat_offsets_and_hold_policy() {
    let mut compiled = compile_with_context(
        "native-arrow-rotation-z",
        "return Def.ActorFrame {}",
        |context| {
            context.defer_initial_live_update = true;
            for player in &mut context.players {
                player.enabled = true;
            }
        },
    );
    let scene = compiled.live_scene.take().unwrap();
    let runtime = scene_global_table(&scene, song::SONG_LUA_RUNTIME_KEY).unwrap();
    scene_set_field_number(
        &scene,
        runtime,
        song::SONG_LUA_RUNTIME_BEAT_KEY,
        f64::from(99.0_f32),
    )
    .unwrap();
    let positions = scene_new_table(&scene).unwrap();
    for (player, beat) in [std::f32::consts::PI, -std::f32::consts::PI]
        .into_iter()
        .enumerate()
    {
        let position = scene_new_table(&scene).unwrap();
        scene_set_field_number(
            &scene,
            position,
            song::SONG_LUA_RUNTIME_BEAT_KEY,
            f64::from(beat),
        )
        .unwrap();
        scene_set_index_root(&scene, positions, player + 1, position).unwrap();
    }
    scene_set_field_root(&scene, runtime, "__songlua_player_positions", positions).unwrap();
    scene_exec(
        &scene,
        r#"
local function near(actual, expected)
    assert(math.abs(actual-expected)<0.0001, tostring(actual)..' vs '..tostring(expected))
end
local ps=GAMESTATE:GetPlayerState(PLAYER_1)
local options=ps:GetCurrentPlayerOptions()
options:Confusion(1)
options:ConfusionOffset(math.pi/2)
options:ConfusionOffset2(math.pi/4)
options:Dizzy(1)
-- At player beat pi: confusion -180, global offset +90, column offset +45.
-- The tap's additional half-pi beat distance contributes +90 dizzy degrees.
near(ArrowEffects.GetRotationZ(ps,math.pi*1.5,false,2),45)
near(ArrowEffects.GetRotationZ(ps,math.pi*1.5,true,2),-45)
options:DizzyHolds(true)
near(ArrowEffects.GetRotationZ(ps,math.pi*1.5,true,2),45)
local other=GAMESTATE:GetPlayerState(PLAYER_2)
other:GetCurrentPlayerOptions():Confusion(1)
near(ArrowEffects.GetRotationZ(other,0,false,1),180)
"#,
    )
    .expect("native confusion, column offset, dizzy hold policy and per-player visible beat");
}

#[test]
fn live_large_scene_preserves_actor_identity_and_update_order() {
    let mut compiled = compile(
        "canonical-large-scene",
        r#"
local root = { Class = 'ActorFrame' }
for i = 1, 2800 do
    root[i] = { Class = 'Quad', Name = 'Child' .. i }
end
return root
"#,
    );
    let mut scene = compiled
        .live_scene
        .take()
        .expect("construct large live scene");
    let root = scene_entry_root(&scene).expect("large retained root");
    scene_set_root(&scene, "LARGE_ROOT", root).unwrap();
    let initial = scene
        .advance(SongLuaLiveFrame::default())
        .expect("initialize large live scene before installing runtime tweens");
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    scene_exec(
        &scene,
        r#"
local count = 2800
local root = LARGE_ROOT
local retained = {}
for i = 1, count do
    local child = root:GetChild('Child' .. i)
    retained[i] = child
    child:x(i):aux(i):linear(1):x(i + 1)
end
root:SetUpdateFunction(function(self, delta)
    if delta <= 0 then return end
    for i = 1, count do
        local child = self:GetChild('Child' .. i)
        assert(child == retained[i], 'live preparation must preserve actor identity')
        assert(child:getaux() == i, 'authored child identity and order must be preserved')
        assert(math.abs(child:GetX() - (i + 0.25)) < 0.001,
            'parent Update must observe every child advanced exactly once: child=' .. i ..
            ' x=' .. child:GetX() .. ' delta=' .. delta .. ' left=' .. child:GetTweenTimeLeft())
    end
end)
"#,
    )
    .expect("install large-scene update assertions");
    let output = scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.25,
            music_seconds: 0.25,
            delta_seconds: 0.25,
            ..Default::default()
        })
        .expect("advance large live scene");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
}

fn scene_named_children(scene: &SongLuaLiveScene, owner: Root) -> Result<Root, Error> {
    let mut runtime = scene.runtime();
    let actor = runtime.vm_mut().rooted(owner)?;
    song::test_support::actor_named_children(&mut runtime, actor)
}

fn scene_actor_at(scene: &SongLuaLiveScene, index: usize) -> Result<Root, Error> {
    let (_, actors) = song::test_support::live_scene_actor_handles(scene)?;
    let value = actors[index];
    scene.runtime().vm_mut().pin(value)
}
fn scene_sync_visual<N, M>(
    scene: &SongLuaLiveScene,
    owner: Root,
    kind: &mut SongLuaOverlayKind<N, M, deadlib_present::actors::TextAttribute>,
) -> Result<(), String> {
    let mut runtime = scene.runtime();
    let actor = runtime
        .vm_mut()
        .rooted(owner)
        .map_err(|error| error.to_string())?;
    song::test_support::publish_live_visual_content(&mut runtime, actor, kind)
}
fn scene_write_root(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: &str,
    root: Root,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let owner = vm.rooted(owner)?;
    let value = vm.rooted(root)?;
    let key = vm.try_intern(name.as_bytes())?;
    let code = compile_named(
        b"local owner,key,value=...;return owner[key](owner,value)",
        b"=existing live actor reference write",
    )?;
    vm.execute_with_args(&code, &[owner, key, value])?;
    Ok(())
}

#[test]
fn live_text_publication_preserves_input_update_draw_and_lookup_order() {
    let mut compiled = compile_with_context(
        "live-text-publication",
        r#"
return Def.ActorFrame {
    OnCommand=function(self)
        SCREENMAN:GetTopScreen():AddInputCallback(function()
            LABEL:settext('input'):AddAttribute(1, {Length=2, Diffuse={1,0,0,1}})
        end)
        self:SetUpdateFunction(function()
            LABEL:settext('update'):ClearAttributes():AddAttribute(2, {Length=3, Glow={0,1,0,1}})
        end)
        self:SetDrawFunction(function() LABEL:settext('draw'):ClearAttributes(); LABEL:Draw() end)
    end,
    Def.BitmapText {
        Name='Label', Font='Common normal', Text='initial',
        InitCommand=function(self) LABEL=self end,
    },
}
"#,
        |context| context.defer_initial_live_update = true,
    );
    let local = compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Label"))
        .unwrap();
    let global = compiled.live_actor_indices()[local];
    let mut kind = compiled.overlays[local].kind.clone();
    let payload = |kind: &SongLuaOverlayKind<_, _, _>| match kind {
        SongLuaOverlayKind::BitmapText {
            text, attributes, ..
        } => (text.clone(), attributes.clone()),
        _ => panic!("text visual slot"),
    };
    let original = payload(&kind);
    let mut scene = compiled.live_scene.take().unwrap();
    let owned = scene_actor_at(&scene, global).unwrap();
    scene_sync_visual(&scene, owned, &mut kind).unwrap();
    assert!(Arc::ptr_eq(&original.0, &payload(&kind).0));
    assert!(Arc::ptr_eq(&original.1, &payload(&kind).1));
    let input = song::SongLuaInputEvent {
        player: Some(0),
        controller: Some(0),
        button: "Right",
        game_button: "MenuRight",
        kind: song::SongLuaInputType::FirstPress,
        device: "InputDevice_Key",
        device_button: "DeviceButton_right",
        level: 1.0,
        z: 0,
        down: true,
        ago: 0.0,
        is_joystick: false,
        is_mouse: false,
    };
    assert!(scene.input(&input).unwrap().1.errors.is_empty());
    scene_sync_visual(&scene, owned, &mut kind).unwrap();
    let after_input = payload(&kind);
    assert_eq!(after_input.0.as_ref(), "input");
    assert_eq!((after_input.1[0].start, after_input.1[0].length), (1, 2));
    assert!(
        scene
            .update(SongLuaLiveFrame::default())
            .unwrap()
            .errors
            .is_empty()
    );
    scene_sync_visual(&scene, owned, &mut kind).unwrap();
    let after_update = payload(&kind);
    assert_eq!(after_update.0.as_ref(), "update");
    assert_eq!(after_update.1[0].glow, Some([0.0, 1.0, 0.0, 1.0]));
    assert!(scene.draw().unwrap().errors.is_empty());
    scene_sync_visual(&scene, owned, &mut kind).unwrap();
    assert_eq!(payload(&kind).0.as_ref(), "draw");
    assert!(payload(&kind).1.is_empty());
    assert_eq!(original.0.as_ref(), "initial");
    assert_eq!(after_input.0.as_ref(), "input");
    assert_eq!(after_input.1[0].color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(after_update.1.len(), 1);

    // Text publishes before attributes, and stroke lookup follows both.
    // A failed attribute read retains the previous attributes and stroke.
    let failing: Root = scene_expression_table(
        &scene,
        r#"
visits = ''
return setmetatable({}, {__index=function(_, key)
    visits=visits .. key .. ';'
    if key=='Text' then return 'new text' end
    if key=='__songlua_text_attributes' then return false end
    error('stroke must not be read')
end})
"#,
    )
    .unwrap();
    assert!(scene_sync_visual(&scene, failing, &mut kind).is_err());
    assert_eq!(
        scene_global_string(&scene, "visits").unwrap(),
        "Text;__songlua_text_attributes;"
    );
    assert_eq!(payload(&kind).0.as_ref(), "new text");
    assert!(payload(&kind).1.is_empty());
}

#[test]
fn live_sprite_publication_preserves_input_update_draw_and_texture_loads() {
    let mut compiled = compile_with_context(
        "live-sprite-publication",
        r#"
return Def.ActorFrame {
    OnCommand=function(self)
        SCREENMAN:GetTopScreen():AddInputCallback(function() SPRITE.Frames[1].Frame=2 end)
        self:SetUpdateFunction(function() SPRITE.Frames[1].Delay=0.25 end)
        self:SetDrawFunction(function() SPRITE.Frames[1].Frame=3; SPRITE:Draw() end)
    end,
    Def.Sprite {
        Name='Animated', Texture='first.png', Frames={{Frame=0,Delay=0.5}},
        InitCommand=function(self) SPRITE=self end,
    },
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            for file in ["first.png", "second.png"] {
                image::RgbaImage::new(16, 16)
                    .save(context.song_dir.join(file))
                    .unwrap();
            }
        },
    );
    let local = compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Animated"))
        .unwrap();
    let global = compiled.live_actor_indices()[local];
    let mut kind = compiled.overlays[local].kind.clone();
    let frames = |kind: &SongLuaOverlayKind<_, _, _>| match kind {
        SongLuaOverlayKind::Sprite { states, .. } => states.clone(),
        _ => panic!("sprite visual slot"),
    };
    let original = frames(&kind);
    let mut scene = compiled.live_scene.take().unwrap();
    let owned = scene_actor_at(&scene, global).unwrap();
    scene_sync_visual(&scene, owned, &mut kind).unwrap();
    assert!(Arc::ptr_eq(&original, &frames(&kind)));
    let input = song::SongLuaInputEvent {
        player: Some(0),
        controller: Some(0),
        button: "Right",
        game_button: "MenuRight",
        kind: song::SongLuaInputType::FirstPress,
        device: "InputDevice_Key",
        device_button: "DeviceButton_right",
        level: 1.0,
        z: 0,
        down: true,
        ago: 0.0,
        is_joystick: false,
        is_mouse: false,
    };
    assert!(scene.input(&input).unwrap().1.errors.is_empty());
    scene_sync_visual(&scene, owned, &mut kind).unwrap();
    assert_eq!(frames(&kind)[0].frame, 2);
    assert!(
        scene
            .update(SongLuaLiveFrame::default())
            .unwrap()
            .errors
            .is_empty()
    );
    scene_sync_visual(&scene, owned, &mut kind).unwrap();
    assert_eq!(frames(&kind)[0].delay, 0.25);
    assert!(scene.draw().unwrap().errors.is_empty());
    scene_sync_visual(&scene, owned, &mut kind).unwrap();
    assert_eq!(frames(&kind)[0].frame, 3);
    assert_eq!(
        original[0],
        song::SongLuaSpriteState {
            frame: 0,
            delay: 0.5
        }
    );
    let drawn = frames(&kind);
    let next = scene_texture_path(&scene, owned)
        .unwrap()
        .unwrap()
        .with_file_name("second.png");
    scene_set_string(&scene, "NEXT_TEXTURE", next.to_string_lossy().as_ref()).unwrap();
    scene_exec(&scene, "SPRITE:SetTexture(NEXT_TEXTURE)").unwrap();
    scene_sync_visual(&scene, owned, &mut kind).unwrap();
    assert!(
        Arc::ptr_eq(&drawn, &frames(&kind)),
        "SetTexture keeps custom frames"
    );
    scene_exec(&scene, "SPRITE:Load(NEXT_TEXTURE)").unwrap();
    scene_sync_visual(&scene, owned, &mut kind).unwrap();
    assert!(
        frames(&kind).is_empty(),
        "Load replaces custom frames with texture defaults"
    );
    assert_eq!(
        drawn[0].frame, 3,
        "previous draw snapshots remain immutable"
    );
}

#[test]
fn live_content_reaches_visual_slots_after_input_and_updates() {
    let mut compiled = compile(
        "live-input-visual-content",
        r#"
local mesh, text, held, x = nil, nil, false, 0
return Def.ActorFrame {
    OnCommand = function(self)
        SCREENMAN:GetTopScreen():AddInputCallback(function(event)
            if event.button == 'Right' then held = event.type ~= 'InputEventType_Release' end
        end)
        self:SetUpdateFunction(function(_, delta)
            if delta <= 0 then return end
            if held then x = x + 1 end
            mesh:SetVertices{
                {{x,0,-10},{1,1,1,1},{0,0}},
                {{x+10,0,-20},{1,1,1,1},{1,0}},
                {{x,10,-30},{1,1,1,1},{0,1}},
                {{0/0,0/0,0/0}}, {{0/0,0/0,0/0}}, {{0/0,0/0,0/0}},
            }
            text:settext(tostring(x))
        end)
    end,
    Def.ActorMultiVertex {
        Name='MovingMesh',
        InitCommand=function(self) mesh=self; self:SetDrawState{Mode='DrawMode_Triangles'}:SetNumVertices(6) end,
    },
    Def.BitmapText { Name='TimerText', Font='Common normal', InitCommand=function(self) text=self end },
}
"#,
    );
    let mesh_local = compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("MovingMesh"))
        .expect("initially empty native mesh retains a visual slot");
    let text_local = compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("TimerText"))
        .unwrap();
    let mesh_global = compiled.live_actor_indices()[mesh_local];
    let text_global = compiled.live_actor_indices()[text_local];
    let mut mesh = compiled.overlays[mesh_local].kind.clone();
    let mut text = compiled.overlays[text_local].kind.clone();
    let mut scene = compiled.live_scene.take().unwrap();
    let mut input = song::SongLuaInputEvent {
        player: Some(0),
        controller: Some(0),
        button: "Right",
        game_button: "MenuRight",
        kind: song::SongLuaInputType::FirstPress,
        device: "InputDevice_Key",
        device_button: "DeviceButton_right",
        level: 1.0,
        z: 0,
        down: true,
        ago: 0.0,
        is_joystick: false,
        is_mouse: false,
    };
    assert!(scene.input(&input).unwrap().1.errors.is_empty());
    for step in 1..=2 {
        let seconds = f64::from(step) / 60.0;
        assert!(
            scene
                .advance(SongLuaLiveFrame {
                    screen_seconds: seconds,
                    music_seconds: seconds,
                    delta_seconds: 1.0 / 60.0,
                    ..Default::default()
                })
                .unwrap()
                .errors
                .is_empty()
        );
    }
    scene_sync_visual(
        &scene,
        scene_actor_at(&scene, mesh_global).unwrap(),
        &mut mesh,
    )
    .unwrap();
    scene_sync_visual(
        &scene,
        scene_actor_at(&scene, text_global).unwrap(),
        &mut text,
    )
    .unwrap();
    let SongLuaOverlayKind::ActorMultiVertex {
        vertices,
        texture_path,
        ..
    } = &mesh
    else {
        panic!("mesh")
    };
    assert_eq!(
        vertices.len(),
        6,
        "non-finite vertices preserve native triangle boundaries"
    );
    assert_eq!(vertices[0].pos, [2.0, 0.0]);
    assert_eq!(
        vertices.iter().take(3).map(|v| v.z).collect::<Vec<_>>(),
        [-10.0, -20.0, -30.0]
    );
    assert!(vertices[3].pos[0].is_nan());
    assert!(
        texture_path.is_some(),
        "native default texture is opaque white"
    );
    assert!(matches!(&text,SongLuaOverlayKind::BitmapText{text,..} if text.as_ref()=="2"));
    input.kind = song::SongLuaInputType::Release;
    input.down = false;
    scene.input(&input).unwrap();
    scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.05,
            music_seconds: 0.05,
            delta_seconds: 1.0 / 60.0,
            ..Default::default()
        })
        .unwrap();
    scene_sync_visual(
        &scene,
        scene_actor_at(&scene, mesh_global).unwrap(),
        &mut mesh,
    )
    .unwrap();
    let SongLuaOverlayKind::ActorMultiVertex { vertices, .. } = &mesh else {
        panic!("mesh")
    };
    assert_eq!(
        vertices[0].pos,
        [2.0, 0.0],
        "release reaches the authored controller"
    );
}

#[test]
fn live_foreground_proxy_targets_reach_prepared_visuals() {
    let mut compiled = compile_with_context(
        "late-proxy-target",
        r#"
return Def.ActorFrame {
    OnCommand=function(self) self:sleep(1000) end,
    Def.ActorFrameTexture {
        InitCommand=function(self)
            self:SetTextureName('ScreenTex'):SetWidth(640):SetHeight(480):Create()
        end,
        Def.ActorProxy {
            Name='FieldProxy',
            OnCommand=function(self)
                self:SetTarget(SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'))
            end,
        },
    },
    Def.Quad { Name='LocalTarget', InitCommand=function(self) self:visible(false) end },
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
            context.players[0].step_statistics_enabled = false;
            context.players[1].enabled = false;
        },
    );
    let proxy = compiled
        .overlays
        .iter()
        .position(|a| a.name.as_deref() == Some("FieldProxy"))
        .unwrap();
    let local = compiled
        .overlays
        .iter()
        .position(|a| a.name.as_deref() == Some("LocalTarget"))
        .unwrap();
    let bindings = compiled
        .live_actor_indices()
        .iter()
        .copied()
        .enumerate()
        .map(|(local, global)| (global, SongLuaLiveTarget::Root(local)))
        .collect();
    let mut scene = compiled.live_scene.take().unwrap();
    scene.bind_targets(bindings).unwrap();
    let mut visuals = deadsync_gameplay::SongLuaRuntimeVisuals {
        overlays: compiled.overlays,
        screen_overlay_index: compiled.screen_overlay_index,
        overlay_eases: Vec::new(),
        overlay_ease_ranges: Vec::new(),
        overlay_events: Vec::new(),
        background_visual_layers: Vec::new(),
        foreground_visual_layers: Vec::new(),
        player_actors: compiled.player_actors,
        player_events: Default::default(),
        player_judgment_events: Default::default(),
        player_combo_events: Default::default(),
        song_foreground: compiled.song_foreground,
        song_foreground_events: Vec::new(),
        hidden_players: [false; 2],
        screen_layers: compiled.screen_layers,
        screen_layer_events: Default::default(),
        note_hides: Default::default(),
        column_offsets: Default::default(),
        column_splines: Default::default(),
        screen_width: 640.0,
        screen_height: 480.0,
    };
    scene.write_visuals(&mut visuals).unwrap();
    assert!(
        matches!(
            visuals.overlays[proxy].kind,
            SongLuaOverlayKind::ActorProxy {
                target: song::SongLuaProxyTarget::Actor {
                    overlay_index: usize::MAX
                }
            }
        ),
        "foreground On has not run during preparation"
    );
    let output = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.1,
            beat: 0.2,
            screen_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    scene.write_visuals(&mut visuals).unwrap();
    assert!(
        matches!(
            visuals.overlays[proxy].kind,
            SongLuaOverlayKind::ActorProxy {
                target: song::SongLuaProxyTarget::NoteField { player_index: 0 }
            }
        ),
        "late foreground On must bind the actual prepared render slot"
    );
    let owned = scene_actor_at(
        &scene,
        scene
            .visual_draw_bindings()
            .iter()
            .find(|(_, target)| matches!(target,SongLuaLiveTarget::Root(index)if *index==proxy))
            .unwrap()
            .0,
    )
    .unwrap();
    let target_actor = scene_actor_at(
        &scene,
        scene
            .visual_draw_bindings()
            .iter()
            .find(|(_, target)| matches!(target,SongLuaLiveTarget::Root(index)if *index==local))
            .unwrap()
            .0,
    )
    .unwrap();
    scene_write_root(&scene, owned, "SetTarget", target_actor).unwrap();
    scene.write_visuals(&mut visuals).unwrap();
    assert!(
        matches!(visuals.overlays[proxy].kind, SongLuaOverlayKind::ActorProxy {
            target: song::SongLuaProxyTarget::Actor { overlay_index }
        } if overlay_index == local),
        "runtime retargeting resolves actor identity to the layer's visual index"
    );
    let underlay = scene_expression_table(
        &scene,
        "return SCREENMAN:GetTopScreen():GetChild('Underlay'):visible(false)",
    )
    .unwrap();
    scene_write_root(&scene, owned, "SetTarget", underlay).unwrap();
    scene.write_visuals(&mut visuals).unwrap();
    assert!(
        matches!(
            visuals.overlays[proxy].kind,
            SongLuaOverlayKind::ActorProxy {
                target: song::SongLuaProxyTarget::Underlay { hidden: true }
            }
        ),
        "retargeting also carries the source's current visibility"
    );
}

fn fixture_runtime_bps(runtime: &mut song::SongRuntime) -> Result<f64, Error> {
    let vm = runtime.vm_mut();
    let Value::Table(table) = vm.get_global(song::SONG_LUA_RUNTIME_KEY.as_bytes())? else {
        panic!("runtime fixture table")
    };
    let key = vm.try_intern(song::SONG_LUA_RUNTIME_BPS_KEY.as_bytes())?;
    let value = vm.raw_table_get(table, key)?;
    fixture_number(vm, value)
}
fn scene_trace_messages(
    scene: &SongLuaLiveScene,
    messages: Arc<std::sync::Mutex<Vec<String>>>,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let callback = vm.new_host_function_with_data(
        |call| {
            let messages = call
                .function_data::<Arc<std::sync::Mutex<Vec<String>>>>()
                .unwrap()
                .clone();
            let value = call.arguments().first().copied().unwrap_or(Value::Nil);
            let id = call
                .coerce_string(value)?
                .ok_or_else(|| call.error(b"trace fixture string expected"))?;
            let text = String::from_utf8(call.string_bytes(id).unwrap().to_vec())
                .map_err(|error| call.error(error.to_string().as_bytes()))?;
            messages.lock().unwrap().push(text);
            Ok(())
        },
        &[],
        messages,
    )?;
    vm.set_global(b"Trace", callback)
}
fn scene_update_only(scene: &SongLuaLiveScene) -> Result<bool, Error> {
    let (root, _) = song::test_support::live_scene_actor_handles(scene)?;
    scene.runtime().song_multitap_update_only(root)
}
fn scene_empty_update(scene: &SongLuaLiveScene, owner: Root) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let owner = vm.rooted(owner)?;
    let callback = vm.new_host_function(|_| Ok(()), &[])?;
    let pin = vm.pin(callback)?;
    let code = compile_named(
        b"local owner,callback=...;return owner:SetUpdateFunction(callback)",
        b"=existing empty callback fixture",
    )?;
    let result = vm.execute_with_args(&code, &[owner, callback]).map(|_| ());
    vm.unpin(pin)?;
    result
}

#[test]
fn live_get_all_steps_returns_native_array_without_internal_getter_calls() {
    let compiled =
        compile_with_context("all-steps-native-array", "return Def.Actor{}", |context| {
            context.song_steps = [SongLuaDifficulty::Challenge, SongLuaDifficulty::Edit]
                .into_iter()
                .enumerate()
                .map(|(index, difficulty)| song::SongLuaStep {
                    steps_type: "StepsType_Dance_Single".into(),
                    difficulty,
                    description: format!("chart {index}"),
                    chart_name: String::new(),
                    credit: String::new(),
                    meter: 9,
                    music_file: String::new(),
                    possible_grade_points: 0,
                })
                .collect();
            context.player_step_indices = [Some(0), Some(0)];
        });
    let scene = compiled.live_scene.expect("owned song array");
    let game = scene_global_table(&scene, "GAMESTATE").unwrap();
    let song = scene_method_table(&scene, game, "GetCurrentSong").unwrap();
    let original = scene_field_table(&scene, song, "__songlua_steps", true).unwrap();
    scene_set_root(&scene, "SONG", song).unwrap();
    scene_set_root(&scene, "ORIGINAL", original).unwrap();
    scene_exec(
        &scene,
        r#"
GETTER_CALLS = 0
for _, step in ipairs(ORIGINAL) do
    for _, name in ipairs({'GetStepsType', 'GetDifficulty'}) do
        local getter = step[name]
        step[name] = function(self)
            GETTER_CALLS = GETTER_CALLS + 1
            return getter(self)
        end
    end
end
FIRST = SONG:GetAllSteps()
SECOND = SONG:GetAllSteps()
assert(SONG:GetOneSteps(0, 4) == ORIGINAL[1])
assert(SONG:GetOneSteps('StepsType_Dance_Single', 'Difficulty_Edit') == ORIGINAL[2])
assert(SONG:GetStepsByStepsType(0)[2] == ORIGINAL[2])
assert(SONG:HasStepsType(0))
assert(SONG:HasStepsTypeAndDifficulty(0, 5))
assert(SONG:HasEdits(0))
assert(not SONG:HasStepsType(1))
assert(SONG:GetOneSteps(0, 2) == nil)
"#,
    )
    .unwrap();

    assert_eq!(
        scene_global_number(&scene, "GETTER_CALLS")
            .map(|value| value as u32)
            .unwrap(),
        0,
        "native SongUtil selectors read metadata without Lua getter callbacks"
    );
    let first = scene_global_table(&scene, "FIRST").unwrap();
    let second = scene_global_table(&scene, "SECOND").unwrap();
    let original = scene_global_table(&scene, "ORIGINAL").unwrap();
    assert_eq!(scene_table_len(&scene, first).unwrap(), 2);
    assert_eq!(scene_table_len(&scene, second).unwrap(), 2);
    assert_ne!(
        scene_table_identity(&scene, first).unwrap(),
        scene_table_identity(&scene, second).unwrap(),
        "each getter returns a fresh array"
    );
    assert_ne!(
        scene_table_identity(&scene, first).unwrap(),
        scene_table_identity(&scene, original).unwrap()
    );
    for index in 1..=2 {
        assert_eq!(
            scene_table_identity(&scene, scene_index_table(&scene, first, index).unwrap()).unwrap(),
            scene_table_identity(&scene, scene_index_table(&scene, original, index).unwrap())
                .unwrap()
        );
        assert_eq!(
            scene_table_identity(&scene, scene_index_table(&scene, second, index).unwrap())
                .unwrap(),
            scene_table_identity(&scene, scene_index_table(&scene, original, index).unwrap())
                .unwrap()
        );
    }
    assert_eq!(
        scene_method_string(
            &scene,
            scene_index_table(&scene, first, 2).unwrap(),
            "GetDifficulty"
        )
        .unwrap(),
        "Difficulty_Edit"
    );
    assert_eq!(
        scene_global_number(&scene, "GETTER_CALLS")
            .map(|value| value as u32)
            .unwrap(),
        1,
        "authored getter calls remain observable"
    );
}

#[test]
fn live_initial_song_bps_uses_native_reset_before_timing_update() {
    for timing_clock in [false, true] {
        let mut compiled = compile_with_context(
            &format!("reset-bps-{timing_clock}"),
            r#"
INIT_CLOCKS = {}
local function init(self)
    INIT_CLOCKS[#INIT_CLOCKS + 1] = {GAMESTATE:GetSongBPS(),
        GAMESTATE:GetSongPosition():GetCurBPS(), GAMESTATE:GetSongBeat(),
        GAMESTATE:GetCurMusicSeconds()}
    self:effectperiod(4.0 / GAMESTATE:GetSongBPS())
end
return Def.ActorFrame {
    Def.Sprite {InitCommand = init}, Def.Sprite {InitCommand = init},
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            CLOCK = {GAMESTATE:GetSongBPS(), GAMESTATE:GetSongPosition():GetCurBPS()}
        end)
    end,
}
"#,
            |context| {
                context.defer_initial_live_update = true;
                context.song_display_bpms = [135.0; 2];
                context.song_timing_bpms = vec![(0.0, 135.0), (4.0, 180.0)];
                if timing_clock {
                    context.song_timing = Some(deadsync_rules::timing::TimingData::from_segments(
                        0.0,
                        0.0,
                        &deadsync_rules::timing::TimingSegments {
                            bpms: context.song_timing_bpms.clone(),
                            ..Default::default()
                        },
                        &[],
                    ));
                }
            },
        );
        let mut scene = compiled
            .live_scene
            .take()
            .expect("owned initial song clock");
        let startup = scene.take_startup_output();
        assert!(startup.errors.is_empty(), "{:?}", startup.errors);
        let clocks = scene_global_table(&scene, "INIT_CLOCKS").unwrap();
        assert_eq!(scene_table_len(&scene, clocks).unwrap(), 2);
        for clock in scene_sequence_tables(&scene, clocks)
            .unwrap()
            .into_iter()
            .map(Ok::<_, Error>)
        {
            let clock = clock.unwrap();
            for index in [1, 2] {
                assert_eq!(
                    scene_index_field_number(&scene, clock, index, false).unwrap(),
                    10.0,
                    "live Init sees native SongPosition Reset BPS before first timing update"
                );
            }
            for index in [3, 4] {
                assert_eq!(
                    scene_index_field_number(&scene, clock, index, false).unwrap(),
                    0.0
                );
            }
        }
        for (beat, seconds, bps) in [(2.25, 1.0, 2.25), (4.75, 2.0277778, 3.0)] {
            let output = scene
                .advance(SongLuaLiveFrame {
                    beat,
                    music_seconds: seconds,
                    screen_seconds: seconds,
                    delta_seconds: 0.01,
                    ..Default::default()
                })
                .unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            let clock = scene_global_table(&scene, "CLOCK").unwrap();
            for index in [1, 2] {
                assert_eq!(
                    scene_index_field_number(&scene, clock, index, false).unwrap(),
                    bps
                );
            }
        }
    }

    let mut preview = SongLuaCompileContext::new(".", "preview BPS");
    preview.song_display_bpms = [135.0; 2];
    let mut runtime = song::SongRuntime::new(&preview).unwrap();
    assert_eq!(
        fixture_runtime_bps(&mut runtime).unwrap(),
        2.25,
        "nonlive preview retains display BPS"
    );
}

#[test]
fn live_rec_print_table_handles_owned_actor_values_and_depth() {
    let mut compiled = compile_with_context(
        "recursive-debug-table",
        r#"
return Def.ActorFrame {
    OnCommand = function(self)
        rec_print_table(nil, 'outer>')
        local cycle = {}; cycle.child = cycle
        rec_print_table(cycle, nil, 0)
        rec_print_table({child={leaf=4}}, '>', 1)
        rec_print_table({child={leaf=4}}, '>', 2)
        rec_print_table({enabled=false})
        local fish = self:GetChild('Fish')
        rec_print_table(fish, 'fish>')
        rec_print_table({item=fish}, 'nested>', 1)
        self:queuecommand('AfterDebug')
    end,
    AfterDebugCommand = function(self) self:aux(19) end,
    Def.Actor { Name='Fish', InitCommand=function(self)
        self.loop = self
        self.parent = self:GetParent()
    end },
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.layer_start_beats = vec![0.0];
        },
    );
    let mut scene = compiled.live_scene.take().expect("owned debug scene");
    // A traversal regression must fail before allocating unbounded debug output.
    scene
        .runtime()
        .vm_mut()
        .set_instruction_limit(Some(5_000_000));

    let messages = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let traced = messages.clone();
    scene_trace_messages(&scene, traced).expect("observe fallback debug output");
    for seconds in [0.1, 0.2] {
        let output = scene
            .advance(SongLuaLiveFrame {
                music_seconds: seconds,
                beat: seconds,
                screen_seconds: seconds,
                delta_seconds: 0.1,
                ..Default::default()
            })
            .expect("debug callback frame");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
    }
    assert!(
        scene.states().iter().any(|state| state.aux == 19.0),
        "the command after the debug helper runs"
    );
    let messages = messages.lock().unwrap();
    assert_eq!(
        &messages[..10],
        [
            "outer>rec_print_table passed a nil",
            ">child: table",
            ">end",
            ">child: table",
            ">  (string)leaf: (number)4",
            ">  end",
            ">end",
            "(string)enabled: (boolean)false",
            "end",
            "fish>rec_print_table passed a userdata",
        ]
    );
    assert!(
        messages[10].starts_with("nested>(string)item: (userdata)Actor"),
        "{:?}",
        messages
    );
    assert_eq!(messages[11], "nested>end");
    assert_eq!(messages.len(), 12);
}

#[test]
fn live_density_graph_owns_quantized_recurring_sleep() {
    for (name, bpms, rate, expected_period) in [
        (
            "constant",
            vec![(0.0, 140.0)],
            1.0,
            (30.0_f64 / 140.0 * 2.0) as f32,
        ),
        ("changes", vec![(0.0, 140.0), (8.0, 160.0)], 1.0, 0.25),
        ("slow", vec![(0.0, 30.0)], 1.0, 0.25),
        (
            "rate",
            vec![(0.0, 140.0)],
            2.0,
            (30.0_f64 / 140.0 * 2.0) as f32,
        ),
    ] {
        let mut compiled = compile_with_context(
            &format!("density-{name}"),
            "return Def.ActorFrame { OnCommand = function(self) self:sleep(10) end }",
            |context| {
                context.defer_initial_live_update = true;
                for player in &mut context.players {
                    player.hide_lifebar = true;
                }
                context.screen_width = 854.0;
                context.players[0].screen_x = 213.5;
                context.players[1].enabled = false;
                context.song_timing_bpms = bpms;
                context.song_music_rate = rate;
                context.player_timing[0] = Some(deadsync_rules::timing::TimingData::from_segments(
                    0.0,
                    0.0,
                    &deadsync_rules::timing::TimingSegments {
                        bpms: vec![(0.0, 300.0)],
                        ..Default::default()
                    },
                    &[],
                ));
            },
        );
        let mut scene = compiled.live_scene.take().expect("scene");
        finish_note_field_intro_fixture(&scene);
        let underlay: Root = scene_child(
            &scene,
            scene_global_table(&scene, "__songlua_top_screen").expect("screen"),
            "Underlay",
        )
        .expect("underlay");
        let pane: Root = scene_child(&scene, underlay, "StepStatsPaneP1").expect("enabled pane");
        let mut left = 0.0_f32;
        let mut screen = 0.0;
        for delta in [0.0_f32, 0.01, 0.2, 0.3, 0.02] {
            let mut remaining = delta;
            while remaining > 0.0 {
                if left == 0.0 {
                    left = expected_period;
                }
                if remaining < left {
                    left -= remaining;
                    break;
                }
                remaining -= left;
                left = 0.0;
            }
            screen += f64::from(delta);
            scene
                .advance(SongLuaLiveFrame {
                    screen_seconds: screen,
                    music_seconds: -1.0 + screen,
                    beat: -2.0 + screen * 2.0,
                    delta_seconds: f64::from(delta),
                    ..Default::default()
                })
                .expect("density frame");
            assert_eq!(
                scene_method_number(&scene, pane, "GetTweenTimeLeft")
                    .map(|value| value as f32)
                    .expect("pane queue"),
                left,
                "{name}: native float queue after delta {delta}"
            );
            assert_eq!(
                scene_method_number(&scene, underlay, "GetTweenTimeLeft")
                    .map(|value| value as f32)
                    .expect("underlay queue"),
                left,
                "ancestor reports its owned descendant"
            );
        }
    }
    for (name, width, style) in [
        ("narrow-centered", 640.0, "single"),
        ("versus", 854.0, "versus"),
    ] {
        let mut compiled = compile_with_context(
            &format!("density-excluded-{name}"),
            "return Def.ActorFrame {}",
            |context| {
                context.defer_initial_live_update = true;
                for player in &mut context.players {
                    player.hide_lifebar = true;
                }
                context.screen_width = width;
                context.style_name = style.into();
            },
        );
        let mut scene = compiled.live_scene.take().expect("excluded layout");
        finish_note_field_intro_fixture(&scene);
        scene
            .advance(SongLuaLiveFrame {
                delta_seconds: 0.01,
                ..Default::default()
            })
            .expect("layout frame");
        let screen: deadlib_lua::Root =
            scene_global_table(&scene, "__songlua_top_screen").expect("screen");
        let underlay: Root = scene_child(&scene, screen, "Underlay").expect("underlay");
        assert_eq!(
            scene_method_number(&scene, underlay, "GetTweenTimeLeft")
                .map(|value| value as f32)
                .expect("excluded graph"),
            0.0,
            "{name}: excluded StepStatistics layouts create no recurring queue"
        );
    }
}

#[test]
fn live_empty_callback_slots_preserve_single_update_detection() {
    let mut compiled = compile_with_context(
        "callback-slots",
        r#"
return Def.ActorFrame {
    Def.ActorFrame { Name = 'Update',
        InitCommand = function(self) self:SetUpdateFunction(function() end) end,
        Def.Quad {},
    },
}
"#,
        |context| {
            for player in &mut context.players {
                player.hide_lifebar = true;
            }
        },
    );
    let mut scene = compiled.live_scene.take().expect("scene");
    // The screen's native BPM callback is independent of the subtree under test.
    let screen = scene_global_table(&scene, "__songlua_top_screen").unwrap();
    let underlay = scene_child(&scene, screen, "Underlay").unwrap();
    for owner in scene_direct_children(&scene, underlay).unwrap() {
        if scene_optional_child(&scene, owner, "RatemodDisplay")
            .unwrap()
            .is_some()
        {
            let inner = scene_direct_children(&scene, owner).unwrap()[1].clone();
            scene_write_value(&scene, inner, "SetUpdateFunction", Value::Nil).unwrap();
        }
    }
    scene.advance(SongLuaLiveFrame::default()).unwrap();
    assert!(scene_update_only(&scene).expect("only the current update callback counts"));
    let root: Root = scene_entry_root(&scene).expect("authored root");
    scene_empty_update(&scene, root).expect("install second callback");
    assert!(!scene_update_only(&scene).expect("additional callbacks still prevent substitution"));
}

fn scene_query_value(
    scene: &SongLuaLiveScene,
    root: Root,
    depth: usize,
) -> Result<serde_json::Value, Error> {
    let mut runtime = scene.runtime();
    let value = runtime.vm_mut().rooted(root)?;
    song::test_support::live_query_value(&mut runtime, value, depth)
}
fn scene_seed_queries(scene: &SongLuaLiveScene, objects: &[(Root, serde_json::Value)]) {
    let mut runtime = scene.runtime();
    let values = objects
        .iter()
        .map(|(root, label)| (runtime.vm_mut().rooted(*root).unwrap(), label.clone()))
        .collect::<Vec<_>>();
    song::test_support::seed_live_query_objects(&runtime, &values)
}
fn scene_hook_query(
    scene: &SongLuaLiveScene,
    root: Root,
    class: &str,
    name: &str,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let owner = runtime.vm_mut().rooted(root)?;
    song::test_support::hook_live_query(&mut runtime, owner, class, name)
}
fn scene_query_hooks(scene: &SongLuaLiveScene) -> Result<Vec<(Root, String)>, Error> {
    let mut runtime = scene.runtime();
    let hooks = song::test_support::live_query_hooks(&runtime);
    hooks
        .into_iter()
        .map(|(owner, name)| runtime.vm_mut().pin(owner).map(|root| (root, name)))
        .collect()
}
fn scene_field_value(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: &str,
    raw: bool,
) -> Result<Value, Error> {
    scene_table_read(
        scene,
        owner,
        name,
        if raw {
            TableRead::Raw
        } else {
            TableRead::Field
        },
        |_, value| Ok(value),
    )
}
fn scene_set_field_value(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: &str,
    value: Value,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let Value::Table(table) = vm.rooted(owner)? else {
        unreachable!()
    };
    let key = vm.try_intern(name.as_bytes())?;
    vm.raw_table_set(table, key, value)
}
fn scene_method_table_arg(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: &str,
    value: Value,
) -> Result<Root, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let owner = vm.rooted(owner)?;
    let key = vm.try_intern(name.as_bytes())?;
    let code = compile_named(
        b"local owner,key,arg=...;return owner[key](owner,arg)",
        b"=existing query method fixture",
    )?;
    let result = vm
        .execute_with_args(&code, &[owner, key, value])?
        .first()
        .copied()
        .unwrap_or(Value::Nil);
    assert!(matches!(result, Value::Table(_)));
    vm.pin(result)
}
fn scene_method_table_number(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: &str,
    value: f64,
) -> Result<Root, Error> {
    scene_method_table_arg(scene, owner, name, Value::Number(value))
}
fn scene_method_table_string(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: &str,
    value: &str,
) -> Result<Root, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let owner = vm.rooted(owner)?;
    let key = vm.try_intern(name.as_bytes())?;
    let value = vm.try_intern(value.as_bytes())?;
    let code = compile_named(
        b"local owner,key,arg=...;return owner[key](owner,arg)",
        b"=existing query method fixture",
    )?;
    let result = vm
        .execute_with_args(&code, &[owner, key, value])?
        .first()
        .copied()
        .unwrap_or(Value::Nil);
    assert!(matches!(result, Value::Table(_)));
    vm.pin(result)
}
fn scene_count_query(
    scene: &SongLuaLiveScene,
    owner: Root,
    native: Root,
    calls: Arc<std::sync::atomic::AtomicUsize>,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let Value::Table(owner) = vm.rooted(owner)? else {
        unreachable!()
    };
    let native = vm.rooted(native)?;
    let callback = vm.new_host_function_with_data(
        |call| {
            let calls = call
                .function_data::<Arc<std::sync::atomic::AtomicUsize>>()
                .unwrap()
                .clone();
            calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let native = call.captures()[0];
            let args = call.arguments().to_vec();
            call.call_and_return(native, &args)
        },
        &[native],
        calls,
    )?;
    let pin = vm.pin(callback)?;
    let result = (|| {
        let key = vm.try_intern(b"GetSongBeat")?;
        vm.raw_table_set(owner, key, callback)
    })();
    vm.unpin(pin)?;
    result
}
thread_local! {static FIXTURE_QUERY_LOG_ENABLED:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
struct FixtureQueryLogger;
impl log::Log for FixtureQueryLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target() == "deadsync_song_lua::trace"
            && FIXTURE_QUERY_LOG_ENABLED.with(std::cell::Cell::get)
    }
    fn log(&self, _: &log::Record<'_>) {}
    fn flush(&self) {}
}
struct FixtureQueryLogging(bool);
impl Drop for FixtureQueryLogging {
    fn drop(&mut self) {
        FIXTURE_QUERY_LOG_ENABLED.with(|flag| flag.set(self.0));
    }
}
fn fixture_query_logging() -> FixtureQueryLogging {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    static LOGGER: FixtureQueryLogger = FixtureQueryLogger;
    INSTALL.call_once(|| {
        log::set_logger(&LOGGER).expect("query fixture logger");
        log::set_max_level(log::LevelFilter::Trace);
    });
    FixtureQueryLogging(FIXTURE_QUERY_LOG_ENABLED.with(|flag| flag.replace(true)))
}

#[test]
fn live_game_state_class_methods_dispatch_on_owned_instance() {
    let mut compiled = compile_with_context(
        "game-state-class-extension",
        r#"
assert(type(GameState) == 'table', 'GameState class methods must be available')
assert(GameState ~= GAMESTATE, 'class methods and owned instance are distinct')
function GameState:ApplyModifiers(level)
    assert(self == GAMESTATE)
    self:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song'):Mini(level)
    return self
end
return Def.Actor {}
"#,
        |context| context.defer_initial_live_update = true,
    );
    let mut scene = compiled.live_scene.take().expect("owned gameplay scene");
    scene
        .advance(SongLuaLiveFrame {
            music_seconds: 2.0,
            screen_seconds: 2.0,
            beat: 4.0,
            delta_seconds: 0.01,
            ..Default::default()
        })
        .unwrap();
    scene_seed_queries(&scene, &[]);
    let game = scene_global_table(&scene, "GAMESTATE").unwrap();
    let class = scene_global_table(&scene, "GameState").unwrap();
    let native = scene_field_function(&scene, class, "GetSongBeat").unwrap();
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    scene_count_query(&scene, class, native, calls.clone()).unwrap();
    scene_hook_query(&scene, game, "GameState", "GetSongBeat").unwrap();
    scene_hook_query(&scene, game, "GameState", "GetSongBeat").unwrap();
    scene_exec(
        &scene,
        r#"
local nativeBeat = GameState.GetSongBeat
function GameState:GetSongBeat()
    assert(self == GAMESTATE)
    return nativeBeat(self) + 0.125
end
assert(GAMESTATE:GetSongBeat() == 4.125)
assert(GAMESTATE:ApplyModifiers(0.25) == GAMESTATE)
assert(GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song'):Mini() == 0.25)
assert(GameState.GetSongBeat(GAMESTATE) == 4.125)
"#,
    )
    .expect("class extension and override through the instance");
    let trace = scene_query_hooks(&scene).unwrap();
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::Relaxed),
        2,
        "hooking must not manufacture extra native getter calls"
    );
    assert_eq!(trace.len(), 1, "one hook per native class method");
    assert_eq!(
        scene_table_identity(&scene, trace[0].0).unwrap(),
        scene_table_identity(&scene, class).unwrap(),
        "native hooks preserve class override lookup through the instance"
    );
    assert!(
        matches!(
            scene_field_value(&scene, game, "GetSongBeat", true).unwrap(),
            Value::Nil
        ),
        "a recorder wrapper must not shadow the authored class override"
    );
}

#[cfg(feature = "test-support")]
#[test]
fn live_numeric_option_getter_retains_native_argument_path() {
    let mut compiled = compile("numeric-option-path", "return Def.Actor{}");
    let scene = compiled.live_scene.take().expect("owned option state");
    let game = scene_global_table(&scene, "GAMESTATE").unwrap();
    let state =
        scene_method_table_string(&scene, game, "GetPlayerState", "PlayerNumber_P1").unwrap();
    let original =
        scene_method_table_string(&scene, state, "GetPlayerOptions", "ModsLevel_Current").unwrap();
    let _query_logging = fixture_query_logging();
    scene_seed_queries(
        &scene,
        &[
            (
                state.clone(),
                serde_json::json!({"type":"PlayerState", "path":"player-state:PlayerNumber_P1"}),
            ),
            (
                original.clone(),
                serde_json::json!({"type":"PlayerOptions", "path":"player-state:PlayerNumber_P1/options:ModsLevel_Current"}),
            ),
        ],
    );
    scene_hook_query(&scene, state, "PlayerState", "GetPlayerOptions").unwrap();
    let numeric = scene_method_table_number(&scene, state, "GetPlayerOptions", 3.0).unwrap();
    assert_eq!(
        scene_table_identity(&scene, numeric).unwrap(),
        scene_table_identity(&scene, original).unwrap(),
        "numeric level addresses the existing option object"
    );
    assert_eq!(
        scene_query_value(&scene, numeric.clone(), 0).unwrap(),
        serde_json::json!({"type":"PlayerOptions", "path":"player-state:PlayerNumber_P1/options:3"}),
        "numeric ModsLevel query retains the native raw argument path"
    );
    scene_write_number(&scene, numeric, "Reverse", 0.5).unwrap();
    let symbolic =
        scene_method_table_string(&scene, state, "GetPlayerOptions", "ModsLevel_Current").unwrap();
    assert_eq!(
        scene_table_identity(&scene, symbolic).unwrap(),
        scene_table_identity(&scene, numeric).unwrap(),
        "getter aliases preserve object identity"
    );
    assert_eq!(
        scene_method_number(&scene, symbolic, "Reverse")
            .map(|value| value as f32)
            .unwrap(),
        0.5
    );
    assert_eq!(
        scene_query_value(&scene, symbolic, 0).unwrap(),
        serde_json::json!({"type":"PlayerOptions", "path":"player-state:PlayerNumber_P1/options:ModsLevel_Current"}),
        "a later symbolic getter refreshes the same native object path"
    );
}

#[test]
fn live_actor_name_ignores_lua_attribute_writes() {
    let mut compiled = compile(
        "native-actor-name",
        r#"
return Def.ActorFrame{Name='Root',
 Def.ActorFrame{InitCommand=function(self) self.Name='Fish_001' end},
 Def.Actor{Name='Declared',InitCommand=function(self) self.Name='Attribute' end},
 OnCommand=function(self)
  local unnamed=self:GetChildren()['']
  assert(unnamed and unnamed:GetName()=='', 'runtime actor name ignores Lua Name attribute')
  assert(unnamed.Name=='Fish_001')
  assert(self:GetChild('Fish_001')==nil)
  local declared=self:GetChild('Declared')
  assert(declared and declared:GetName()=='Declared')
  assert(declared.Name=='Attribute' and self:GetChild('Attribute')==nil)
  unnamed:name('RuntimeFish')
  assert(unnamed:GetName()=='RuntimeFish' and unnamed.Name=='Fish_001')
  assert(self:GetChild('RuntimeFish')==unnamed)
  unnamed.Name='AnotherAttribute'
  assert(unnamed:GetName()=='RuntimeFish')
  self:aux(1)
 end,
}
"#,
    );
    let mut scene = compiled.live_scene.take().expect("owned scene");
    let output = scene
        .advance(SongLuaLiveFrame::default())
        .expect("owned update");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    let root: Root = scene_entry_root(&scene).expect("root");
    assert_eq!(
        scene_method_number(&scene, root, "getaux")
            .map(|value| value as f32)
            .unwrap(),
        1.0
    );
    let fish: Root = scene_index_table(&scene, root, 1).expect("same retained fish");
    let observed = scene_query_value(&scene, fish, 0).expect("query identity");
    assert_eq!(observed["name"], "RuntimeFish");
    assert_eq!(
        observed["path"],
        "ScreenGameplay/SongForeground/Root/RuntimeFish"
    );
}

#[test]
fn actor_query_loading_names() {
    let compiled = compile("query-loading-names", "return Def.Actor{}");
    let scene = compiled.live_scene.expect("query fixture VM");
    let actor: Root = scene_expression_table(
        &scene,
        r#"
local screen={__songlua_actor_type='TopScreen',Name='ScreenGameplay'}
return setmetatable({__songlua_actor_type='ActorFrame',Name='Declared',
 __songlua_load_name='',__songlua_parent=screen},
 {__index=function() error('query observation must not run authored lookup') end})
"#,
    )
    .expect("actor with authored lookup");
    let mut value = scene_query_value(&scene, actor, 0).expect("loading actor");
    assert_eq!(value["name"], "");
    assert_eq!(value["path"], "ScreenGameplay");
    assert!(value["actor"].is_string());
    scene_set_field_value(&scene, actor, "__songlua_load_name", Value::Nil)
        .expect("finish loading");
    value = scene_query_value(&scene, actor, 0).expect("loaded actor");
    assert_eq!(value["name"], "Declared");
    assert_eq!(value["path"], "ScreenGameplay/Declared");
    scene_set_field_root(&scene, actor, "__songlua_parent", actor).expect("invalid ownership");
    assert!(scene_query_value(&scene, actor, 0).is_err());
}

fn scene_initial_context(scene: &SongLuaLiveScene) -> &SongLuaCompileContext {
    song::test_support::live_scene_initial_context(scene)
}
fn scene_take_output(scene: &mut SongLuaLiveScene) -> Result<SongLuaLiveOutput, Error> {
    song::test_support::take_live_scene_output(scene)
}
fn scene_three_numbers(scene: &SongLuaLiveScene, source: &str) -> Result<(f32, f32, f32), Error> {
    let mut runtime = scene.runtime();
    let code = compile_named(source.as_bytes(), b"=original reload tuple fixture")?;
    let values = runtime.vm_mut().execute(&code)?;
    let [Value::Number(a), Value::Number(b), Value::Number(c), ..] = *values else {
        panic!("reload fixture numeric tuple: {values:?}")
    };
    Ok((a as f32, b as f32, c as f32))
}
fn scene_optional_global_number(
    scene: &SongLuaLiveScene,
    name: &str,
) -> Result<Option<f64>, Error> {
    let mut runtime = scene.runtime();
    match runtime.vm_mut().get_global(name.as_bytes())? {
        Value::Nil => Ok(None),
        Value::Number(value) => Ok(Some(value)),
        other => panic!("reload fixture optional number: {other:?}"),
    }
}
fn scene_counter_command(
    scene: &SongLuaLiveScene,
    actor: Root,
    name: &str,
    counter: &str,
    set_one: bool,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let actor = vm.rooted(actor)?;
    let globals = vm.get_global(b"_G")?;
    let counter = vm.try_intern(counter.as_bytes())?;
    let callback = vm.new_host_function_with_data(
        |call| {
            assert!(matches!(call.arguments().first(), Some(Value::Table(_))));
            let [Value::Table(globals), key] = *call.captures() else {
                unreachable!()
            };
            let set_one = *call.function_data::<bool>().unwrap();
            let value = if set_one {
                1.0
            } else {
                let Value::Number(value) = call.raw_table_get(globals, key)? else {
                    return Err(call.error(b"counter fixture requires a number"));
                };
                value + 1.0
            };
            call.raw_table_set(globals, key, Value::Number(value))
        },
        &[globals, counter],
        set_one,
    )?;
    let pin = vm.pin(callback)?;
    let result = (|| {
        let key = vm.try_intern(name.as_bytes())?;
        let code = compile_named(
            b"local actor,key,callback=...;actor[key]=callback",
            b"=original reload command fixture",
        )?;
        vm.execute_with_args(&code, &[actor, key, callback])
            .map(|_| ())
    })();
    vm.unpin(pin)?;
    result
}

#[test]
fn live_screen_request_calls_preserve_duplicates_and_final_target() {
    let compiled = compile_with_context("screen-request-calls", "return Def.Actor{}", |_| {});
    let mut scene = compiled.live_scene.expect("live request observations");
    scene.runtime().set_values(2.0, 1.25).unwrap();
    scene_exec(
        &scene,
        r#"
SCREENMAN:SetNewScreen('ScreenSelectMusic')
SCREENMAN:SetNewScreen('ScreenGameplay')
SCREENMAN:SetNewScreen('ScreenGameplay')
"#,
    )
    .unwrap();
    let mut output = scene_take_output(&mut scene).unwrap();
    assert_eq!(
        output.request,
        Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into()))
    );
    assert_eq!(
        output.source_requests.len(),
        3,
        "every authored screen request remains observable"
    );
    for (event, name) in
        output
            .source_requests
            .iter()
            .zip(["ScreenSelectMusic", "ScreenGameplay", "ScreenGameplay"])
    {
        assert_eq!(event.music_seconds, 1.25);
        assert_eq!(event.request, SongLuaLiveRequest::NewScreen(name.into()));
    }
    assert!(
        scene_take_output(&mut scene)
            .unwrap()
            .source_requests
            .is_empty(),
        "observations drain once"
    );
    scene_exec(
        &scene,
        "SCREENMAN:GetTopScreen():StartTransitioningScreen('SM_DoNextScreen')",
    )
    .unwrap();
    let transition = scene_take_output(&mut scene).unwrap();
    assert_eq!(transition.source_requests.len(), 1);
    assert_eq!(
        transition.source_requests[0].request,
        SongLuaLiveRequest::Transition("SM_DoNextScreen".into())
    );
    output.append(transition);
    assert_eq!(
        output.source_requests.len(),
        4,
        "output merging preserves all source calls"
    );
    assert_eq!(
        output.request,
        Some(SongLuaLiveRequest::Transition("SM_DoNextScreen".into()))
    );
}

#[test]
fn live_scheduled_reload() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
    let song = deadsync_simfile::song::parse_song_meta_file(
        &fixtures.join("startup-reload.ssc"),
        &deadsync_simfile::song::ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        0.0,
        |_| 0.0,
    )
    .expect("native reload simfile");
    let mut context = SongLuaCompileContext::new(&fixtures, "Scheduled native reload");
    context.live = true;
    context.layer_start_beats = vec![0.0];
    context.song_steps = song.charts.iter().map(song::SongLuaStep::from).collect();
    context.player_step_indices[0] = context
        .song_steps
        .iter()
        .position(|steps| steps.difficulty == SongLuaDifficulty::Challenge);
    context.players[0].difficulty = SongLuaDifficulty::Challenge;
    let mut compiled = song::compile_song_lua(
        &fixtures.join("startup-reload.lua"),
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None::<song::SongLuaOverlayModelLayer<()>>,
        |_, _| None,
    )
    .expect("scheduled scene");
    assert_eq!(compiled.startup_reloads, 0);
    let mut scene = compiled.live_scene.take().expect("first scene");
    let selected = scene.selected_context().expect("raw GAMESTATE selection");
    assert_ne!(
        context.player_step_indices[0],
        selected.player_step_indices[0]
    );
    assert_eq!(
        scene_initial_context(&scene).player_step_indices,
        context.player_step_indices
    );
    scene_exec(
        &scene,
        r#"
            math.randomseed(73)
            math.random()
            EXPECTED_RANDOM = math.random()
            math.randomseed(73)
            math.random()
            OLD_MESSAGES, OLD_QUEUES, ENDED = 0, 0, 0
            local ps = GAMESTATE:GetPlayerState(PLAYER_1)
            RETAINED_PREFERRED = ps:GetPlayerOptions('ModsLevel_Preferred')
            RETAINED_CURRENT = ps:GetPlayerOptions('ModsLevel_Current')
            RETAINED_PREFERRED:Dark(0.25)
            RETAINED_CURRENT:Dark(0.5)
            ps:GetPlayerOptions('ModsLevel_Song'):Dark(1, 0.1)
        "#,
    )
    .expect("random and lifetime controls");
    let old_actor = scene_actor_at(&scene, 0).unwrap();
    scene_counter_command(
        &scene,
        old_actor,
        "RetiredMessageCommand",
        "OLD_MESSAGES",
        false,
    )
    .expect("install old listener");
    scene_counter_command(&scene, old_actor, "EndCommand", "ENDED", false).expect("install End");
    scene_counter_command(&scene, old_actor, "StaleCommand", "OLD_QUEUES", true)
        .expect("install stale command");
    scene_method_table_number(&scene, old_actor, "sleep", 10.0).expect("old sleep");
    scene_method_table_string(&scene, old_actor, "queuecommand", "Stale").expect("old queue");
    let output = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.017,
            beat: 0.0,
            screen_seconds: 2.0,
            delta_seconds: 0.001,
            ..Default::default()
        })
        .expect("activate foreground");
    assert_eq!(
        output.request,
        Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into()))
    );
    let mut layers = song::reload_song_lua(
        scene,
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None::<song::SongLuaOverlayModelLayer<()>>,
    )
    .expect("runtime reload");
    let mut scene = layers[0].live_scene.take().expect("replacement scene");
    assert_eq!(
        scene_initial_context(&scene).player_step_indices,
        selected.player_step_indices
    );
    assert_eq!(
        scene_global_number(&scene, "HARNESS_NATIVE_RELOAD_COUNT")
            .map(|value| value as u32)
            .expect("same VM"),
        2
    );
    assert_eq!(
        scene_global_number(&scene, "ENDED")
            .map(|value| value as u32)
            .expect("old End delivered"),
        1
    );
    let dark = scene_three_numbers(&scene,"return RETAINED_PREFERRED:Dark(), RETAINED_CURRENT:Dark(), GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song'):Dark()").expect("restarted regular stage options");
    assert_eq!(dark, (0.25, 0.25, 0.25));
    assert_eq!(
        scene_number(&scene, "return math.random()")
            .map(|value| value as f64)
            .expect("next random"),
        scene_global_number(&scene, "EXPECTED_RANDOM")
            .map(|value| value as f64)
            .expect("retained generator")
    );
    assert_eq!(scene_number(&scene,"return STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1):GetPossibleDancePoints()").map(|value|value as i32).expect("replacement maximum"), 10);
    scene_exec(&scene, "MESSAGEMAN:Broadcast('Retired')").expect("new registry broadcast");
    scene
        .advance(SongLuaLiveFrame {
            music_seconds: 10.0,
            beat: 20.0,
            screen_seconds: 10.0,
            delta_seconds: 10.0,
            ..Default::default()
        })
        .expect("new scene update");
    assert_eq!(
        scene_global_number(&scene, "OLD_MESSAGES")
            .map(|value| value as u32)
            .expect("retired listener"),
        0
    );
    assert_eq!(
        scene_global_number(&scene, "OLD_QUEUES")
            .map(|value| value as u32)
            .expect("retired queue"),
        0
    );
}

#[test]
fn live_failed_actor_load_keeps_vm_and_rebuilds_on_reload() {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-live-load-fallback-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("song directory");
    let background = dir.join("background.lua");
    let foreground = dir.join("foreground.lua");
    let invalid = dir.join("invalid.lua");
    fs::write(
        &background,
        r#"
BACKGROUND_LOADS = (BACKGROUND_LOADS or 0) + 1
if COUNTER == nil then error('native failed actor load uses a dummy') end
return Def.ActorFrame{InitCommand=function(self) BACKGROUND_INITS = COUNTER end}
"#,
    )
    .expect("background source");
    fs::write(
        &foreground,
        r#"
COUNTER = COUNTER or 0
return Def.ActorFrame{OnCommand=function(self)
    if not RELOADED then RELOADED = true; SCREENMAN:SetNewScreen('ScreenGameplay') end
end}
"#,
    )
    .expect("foreground source");
    fs::write(
        &invalid,
        "INVALID_LOADS = (INVALID_LOADS or 0) + 1; return 42",
    )
    .expect("invalid return source");
    let mut context = SongLuaCompileContext::new(&dir, "native actor load fallback");
    context.live = true;
    context.background_layer_count = 1;
    context.layer_start_beats = vec![4.0, 0.0, 0.0];
    context.foreground_names = vec!["foreground".into(), "invalid".into()];
    let mut layers: Vec<Compiled> = song::compile_song_lua_layers(
        &[&background, &foreground, &invalid],
        1,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("native failed actor load uses a dummy and preserves the shared VM");
    let mut scene = layers[1]
        .live_scene
        .take()
        .expect("primary retains shared scene");
    assert_eq!(
        scene_global_number(&scene, "BACKGROUND_LOADS")
            .map(|value| value as u32)
            .unwrap(),
        1
    );
    assert_eq!(
        scene_global_number(&scene, "INVALID_LOADS")
            .map(|value| value as u32)
            .unwrap(),
        1
    );
    assert!(
        scene_optional_global_number(&scene, "BACKGROUND_INITS")
            .map(|value| value.map(|value| value as u32))
            .unwrap()
            .is_none()
    );
    scene_exec(
        &scene,
        r#"
local children = SCREENMAN:GetTopScreen():GetChild('SongForeground'):GetChildren()
assert(children.foreground ~= children.invalid)
assert(children.invalid.__songlua_actor_type == 'Actor')
"#,
    )
    .expect("native dummy occupies its foreground slot");
    let output = scene
        .advance(SongLuaLiveFrame {
            music_seconds: 0.1,
            beat: 0.1,
            screen_seconds: 0.1,
            delta_seconds: 0.1,
            ..Default::default()
        })
        .expect("activate foreground");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert_eq!(
        output.request,
        Some(SongLuaLiveRequest::NewScreen("ScreenGameplay".into()))
    );
    let mut layers: Vec<Compiled> =
        song::reload_song_lua(scene, |_, _, _| Ok(Arc::<[()]>::from([])), |_| None)
            .expect("reload same process VM");
    let mut scene = layers[1]
        .live_scene
        .take()
        .expect("replacement retains scene");
    assert_eq!(
        scene_global_number(&scene, "BACKGROUND_LOADS")
            .map(|value| value as u32)
            .unwrap(),
        2
    );
    assert_eq!(
        scene_global_number(&scene, "BACKGROUND_INITS")
            .map(|value| value as u32)
            .unwrap(),
        0
    );
    assert_eq!(
        scene_global_number(&scene, "INVALID_LOADS")
            .map(|value| value as u32)
            .unwrap(),
        2
    );
    assert!(
        scene
            .advance(SongLuaLiveFrame {
                music_seconds: 1.0,
                beat: 4.0,
                screen_seconds: 1.0,
                delta_seconds: 0.1,
                ..Default::default()
            })
            .unwrap()
            .errors
            .is_empty()
    );
}

fn scene_set_field_string(
    scene: &SongLuaLiveScene,
    owner: Root,
    key: &str,
    text: &str,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let Value::Table(owner) = vm.rooted(owner)? else {
        unreachable!()
    };
    let key = vm.try_intern(key.as_bytes())?;
    let pin = vm.pin(key)?;
    let result = (|| {
        let value = vm.try_intern(text.as_bytes())?;
        vm.raw_table_set(owner, key, value)
    })();
    vm.unpin(pin)?;
    result
}
fn scene_native_broadcast(
    scene: &SongLuaLiveScene,
    name: &str,
    params: Option<Root>,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let params = params
        .map(|root| runtime.vm_mut().rooted(root))
        .transpose()?;
    runtime.broadcast_native_message(name, params)
}
fn scene_optional_function(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: &str,
) -> Result<Option<Root>, Error> {
    scene_table_read(
        scene,
        owner,
        name,
        TableRead::Raw,
        |vm, value| match value {
            Value::Nil => Ok(None),
            Value::Function(_) | Value::Native(_) => vm.pin(value).map(Some),
            other => panic!("fixture optional function: {other:?}"),
        },
    )
}
fn scene_method_identity(
    scene: &SongLuaLiveScene,
    owner: Root,
    name: &str,
) -> Result<usize, Error> {
    scene_table_read(scene, owner, name, TableRead::Method, |vm, value| {
        Ok(vm.object_pointer(value).expect("fixture method object"))
    })
}
fn scene_replay_pose(
    scene: &SongLuaLiveScene,
    owner: Root,
    state: &mut SongLuaOverlayState,
) -> bool {
    let mut runtime = scene.runtime();
    let actor = runtime.vm_mut().rooted(owner).unwrap();
    song::test_support::read_live_tween_pose(&runtime, actor, state)
}

#[test]
fn live_screen_begin_reaches_message_commands_only_in_screen_children() {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-live-begin-message-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("begin message directory");
    let background = dir.join("background.lua");
    let foreground = dir.join("foreground.lua");
    fs::write(
        &background,
        r#"
BG_BEGIN = 0
return Def.ActorFrame { Def.Actor {
    BeginMessageCommand = function() BG_BEGIN = BG_BEGIN + 1 end,
} }
"#,
    )
    .unwrap();
    fs::write(
        &foreground,
        r#"
FG_BEGIN, TARGETS = 0, 0
local proxy = Def.ActorProxy {
    InitCommand = function(self) PROXY = self end,
    BeginMessageCommand = function(self)
        FG_BEGIN = FG_BEGIN + 1
        if TARGET then self:SetTarget(TARGET); TARGETS = TARGETS + 1 end
    end,
}
return Def.ActorFrame { proxy, Def.ActorProxy {
    BeginMessageCommand = function(self) FG_BEGIN = FG_BEGIN + 1 end,
} }
"#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(&dir, "native begin aliases");
    context.live = true;
    context.background_layer_count = 1;
    context.layer_start_beats = vec![0.0, 0.0];
    context.foreground_names = vec!["foreground".into()];
    let mut layers: Vec<Compiled> = song::compile_song_lua_layers(
        &[&background, &foreground],
        1,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("owned background and foreground");
    let mut scene = layers[1].live_scene.take().expect("primary scene");
    let startup = scene.take_startup_output();
    assert!(startup.errors.is_empty(), "{:?}", startup.errors);

    assert_eq!(
        scene_global_number(&scene, "FG_BEGIN")
            .map(|value| value as u32)
            .unwrap(),
        2,
        "screen Begin invokes native foreground BeginMessageCommand aliases"
    );
    assert_eq!(
        scene_global_number(&scene, "BG_BEGIN")
            .map(|value| value as u32)
            .unwrap(),
        0,
        "cached background owners are outside screen Begin propagation"
    );
    assert_eq!(
        scene_global_number(&scene, "TARGETS")
            .map(|value| value as u32)
            .unwrap(),
        0
    );
    assert!(
        scene_bool(
            &scene,
            "return type(PROXY:GetCommand('Begin')) == 'function'"
        )
        .unwrap()
    );
    scene_exec(&scene, "PROXY:playcommand('Begin')").unwrap();
    assert_eq!(
        scene_global_number(&scene, "FG_BEGIN")
            .map(|value| value as u32)
            .unwrap(),
        3
    );
    assert_eq!(
        scene_global_number(&scene, "BG_BEGIN")
            .map(|value| value as u32)
            .unwrap(),
        0
    );
    assert!(
        startup
            .broadcasts
            .iter()
            .all(|(_, name, _)| name != "Begin"),
        "direct screen command creates no synthetic Begin broadcast"
    );
}

#[test]
fn live_overlay_fails_judged_autoplay_during_off_after_exit_decision() {
    for (name, autoplay, judgment) in [
        ("used", true, true),
        ("unused", true, false),
        ("human", false, true),
    ] {
        let mut compiled = compile_with_context(
            &format!("overlay-failure-{name}"),
            r#"
OFFS, FAILED_AT_OFF = 0, false
return Def.ActorFrame { OffCommand = function(self)
    OFFS = OFFS + 1
    FAILED_AT_OFF = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1):GetFailed()
end }
"#,
            |context| {
                context.defer_initial_live_update = true;
                context.players[1].enabled = false;
            },
        );
        let mut scene = compiled.live_scene.take().expect("owned scene");
        let mut frame = SongLuaLiveFrame {
            delta_seconds: 0.1,
            ..Default::default()
        };
        frame.players[0].autoplay = autoplay;
        scene.advance(frame.clone()).expect("controller frame");
        assert_eq!(
            scene_bool(&scene, "return IsAutoplay(PLAYER_1)").unwrap(),
            autoplay
        );
        if judgment {
            let params = scene_new_table(&scene).unwrap();
            scene_set_field_string(&scene, params, "Player", "PlayerNumber_P1").unwrap();
            scene_set_field_string(&scene, params, "TapNoteScore", "TapNoteScore_W1").unwrap();
            scene_native_broadcast(&scene, "Judgment", Some(params)).unwrap();
        }
        // The overlay remembers actual Judgment use, even after autoplay
        // is disabled. Merely selecting autoplay does not fail a player.
        frame.players[0].autoplay = false;
        frame.gameplay_outro = true;
        let boundary = scene.advance(frame.clone()).expect("NotesEnded posted");
        assert!(boundary.player_writes.is_empty());
        assert_eq!(
            scene_global_number(&scene, "OFFS")
                .map(|value| value as i64)
                .unwrap(),
            0
        );
        let off = scene
            .advance(frame.clone())
            .expect("LeaveGameplay after actor update");
        assert_eq!(
            scene_global_number(&scene, "OFFS")
                .map(|value| value as i64)
                .unwrap(),
            1,
            "autoplay fails inside successful Off, without posting BeginFailed"
        );
        assert_eq!(
            scene_global_bool(&scene, "FAILED_AT_OFF").unwrap(),
            autoplay && judgment,
            "overlay Off precedes foreground Off"
        );
        assert_eq!(
            off.player_writes,
            if autoplay && judgment {
                vec![SongLuaLivePlayerWrite::Fail { player: 0 }]
            } else {
                vec![]
            }
        );
        frame.players[0].failed = autoplay && judgment;
        assert!(
            scene
                .advance(frame.clone())
                .unwrap()
                .player_writes
                .is_empty(),
            "Off runs once"
        );
        assert_eq!(
            scene_global_number(&scene, "OFFS")
                .map(|value| value as i64)
                .unwrap(),
            1
        );
    }
}

#[test]
fn live_step_statistics_owns_native_resources_and_updates_real_data() {
    use deadsync_rules::timing::{TimingData, TimingSegments};
    for (name, width, height, enabled, centered, selected) in [
        ("p1", 854.0, 480.0, [true, false], false, true),
        ("p2-centered", 854.0, 480.0, [false, true], true, true),
        ("ultrawide", 2560.0, 720.0, [true, true], false, true),
        ("narrow", 640.0, 480.0, [true, false], true, true),
        ("disabled", 854.0, 480.0, [true, false], false, false),
    ] {
        let mut compiled = compile_with_context(
            &format!("native-step-statistics-{name}"),
            "return Def.ActorFrame {}",
            |context| {
                context.defer_initial_live_update = true;
                context.screen_width = width;
                context.screen_height = height;
                for player in 0..2 {
                    context.players[player].enabled = enabled[player];
                    context.players[player].screen_x = if centered { width / 2.0 } else { 320.0 };
                    context.players[player].step_statistics_enabled = selected;
                    context.players[player].timing_windows = [true, true, true, false, false];
                    context.players[player].background_filter_percent = 75;
                }
                context.player_step_indices = [Some(0), Some(0)];
                context.song_steps = vec![song::SongLuaStep {
                    steps_type: "StepsType_Dance_Single".into(),
                    difficulty: SongLuaDifficulty::Hard,
                    description: String::new(),
                    chart_name: String::new(),
                    credit: String::new(),
                    meter: 12,
                    music_file: String::new(),
                    possible_grade_points: 100,
                }];
                let timing = TimingData::from_segments(
                    0.0,
                    0.0,
                    &TimingSegments {
                        bpms: vec![(0.0, 120.0)],
                        ..Default::default()
                    },
                    &[],
                );
                context.song_step_timing = vec![timing.clone()];
                context.player_timing = [Some(timing.clone()), Some(timing)];
                context.song_step_notes = vec![std::sync::Arc::from(
                    &b"1000\n0100\n2000\n3000\nM000\n4000\n3000\n1000\n"[..],
                )];
                context.song_step_row_beats = vec![std::sync::Arc::from(
                    &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0][..],
                )];
            },
        );
        let mut scene = compiled.live_scene.take().unwrap();
        scene_set_bool(&scene, "EXPECT_PANE", selected && width > 640.0).unwrap();
        scene_exec(&scene,r#"
PANES={}
local underlay=SCREENMAN:GetTopScreen():GetChild('Underlay')
for p=1,2 do
    local pane=underlay:GetChild('StepStatsPaneP'..p)
    if GAMESTATE:IsPlayerEnabled('PlayerNumber_P'..p) and EXPECT_PANE then
        assert(pane and pane:GetNumChildren()==3, 'StepStatistics must own its real three native children')
        assert(pane:GetParent()==underlay)
        local data=pane:GetChild('BannerAndData')
        assert(data and data:GetNumChildren()==4)
        assert(data:GetChild('TapNoteJudgments'):GetNumChildren()==10)
        assert(data:GetChild('HoldsMinesRolls'):GetChild('Holds_Values'):GetText()=='000/001')
        assert(data:GetChild('HoldsMinesRolls'):GetChild('Mines_Values'):GetText()=='000/001')
        local loose=data:GetChild('')
        local banner, timer=loose[1],loose[2]
        assert(tostring(banner):find('Banner',1,true) and timer:GetNumChildren()==4)
        local time=timer:GetChild('')
        assert(time[1]:GetText()=='' and time[3]:GetText()=='1:00', 'native pre-update timer: '..time[1]:GetText()..' / '..time[3]:GetText())
        local parts=pane:GetChild('')
        local dark, graph=parts[1],parts[2]
        assert(tostring(dark):find('Sprite',1,true) and dark:GetTexture())
        assert(graph:GetNumChildren()==3)
        local graph_parts=graph:GetChild('')
        local plot=graph_parts[3]
        assert(plot:GetNumChildren()==2 and plot:GetChild('LifeLine_AMV'))
        assert(GAMESTATE:Env()['P'..p..'PeakNPS']==1.5)
        PANES[#PANES+1]={pane=pane, timer=time[1], count=data:GetChild('TapNoteJudgments'):GetChild('')[1],
            life=plot:GetChild('LifeLine_AMV'), histogram=plot:GetChild(''), player='PlayerNumber_P'..p}
    else assert(pane==nil) end
end
"#).unwrap();
        if !selected || width <= 640.0 {
            continue;
        }
        let output = scene
            .advance(SongLuaLiveFrame {
                screen_seconds: 0.01,
                delta_seconds: 0.01,
                music_seconds: 2.0,
                beat: 4.0,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        scene_exec(
            &scene,
            r#"
for _,v in ipairs(PANES) do
    assert(v.timer:GetText()=='0:58')
    assert(v.histogram:GetNumVertices()>0 and v.life:GetNumVertices()>0)
end
"#,
        )
        .unwrap();
        let panes = scene_global_table(&scene, "PANES").unwrap();
        for pane in scene_sequence_tables(&scene, panes).unwrap() {
            let params = scene_new_table(&scene).unwrap();
            scene_set_field_string(
                &scene,
                params,
                "Player",
                &scene_field_string(&scene, pane, "player", true).unwrap(),
            )
            .unwrap();
            scene_set_field_string(&scene, params, "TapNoteScore", "TapNoteScore_W1").unwrap();
            scene_native_broadcast(&scene, "Judgment", Some(params)).unwrap();
        }
        scene_exec(&scene,r#"
for _,v in ipairs(PANES) do
    assert(v.count:GetText()=='0001')
    MESSAGEMAN:Broadcast('HealthStateChanged',{PlayerNumber=v.player,HealthState='HealthState_Dead'})
end
"#).unwrap();
        let output = scene
            .advance(SongLuaLiveFrame {
                screen_seconds: 0.3,
                delta_seconds: 0.29,
                music_seconds: 3.0,
                beat: 6.0,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        scene_exec(
            &scene,
            "for _,v in ipairs(PANES) do assert(v.timer:GetText()=='0:58') end",
        )
        .unwrap();
    }
}

#[test]
fn live_shared_bpm_owns_native_hierarchy_and_updates_real_timing_and_rate() {
    use deadsync_rules::timing::{TimingData, TimingSegments};
    for (enabled, split, upper) in [
        ([true, false], false, false),
        ([true, true], false, false),
        ([true, true], true, false),
        ([false, true], true, true),
        ([true, false], false, true),
    ] {
        let mut compiled = compile_with_context(
            &format!("shared-bpm-{enabled:?}-{split}-{upper}"),
            "return Def.ActorFrame {}",
            |context| {
                context.defer_initial_live_update = true;
                context.song_display_bpms = [666.0, 999.0];
                context.player_step_indices = if split {
                    [Some(0), Some(1)]
                } else {
                    [Some(0), Some(0)]
                };
                for player in 0..LUA_PLAYERS {
                    context.players[player].enabled = enabled[player];
                    context.players[player].nps_graph_at_top = upper;
                    context.players[player].screen_x = 320.0;
                    context.players[player].difficulty = SongLuaDifficulty::Hard;
                }
                let timings = [
                    TimingData::from_segments(
                        0.0,
                        0.0,
                        &TimingSegments {
                            bpms: vec![(0.0, 120.0), (2.0, 150.0)],
                            ..Default::default()
                        },
                        &[],
                    ),
                    TimingData::from_segments(
                        0.0,
                        0.0,
                        &TimingSegments {
                            bpms: vec![(0.0, 240.0), (2.0, 300.0)],
                            ..Default::default()
                        },
                        &[],
                    ),
                ];
                context.player_timing = [
                    Some(timings[0].clone()),
                    Some(timings[usize::from(split)].clone()),
                ];
                context.song_step_timing = timings.to_vec();
                context.song_steps = (0..2)
                    .map(|index| song::SongLuaStep {
                        steps_type: "StepsType_Dance_Single".into(),
                        difficulty: SongLuaDifficulty::Hard,
                        description: format!("chart{index}"),
                        chart_name: String::new(),
                        credit: String::new(),
                        meter: 12,
                        music_file: String::new(),
                        possible_grade_points: 100,
                    })
                    .collect();
            },
        );
        let mut scene = compiled.live_scene.take().unwrap();
        let screen = scene_global_table(&scene, "__songlua_top_screen").unwrap();
        let underlay = scene_child(&scene, screen, "Underlay").unwrap();
        let outer = scene_direct_children(&scene, underlay)
            .unwrap()
            .into_iter()
            .find(|owner| {
                scene_optional_child(&scene, *owner, "RatemodDisplay")
                    .unwrap()
                    .is_some()
            });
        let outer = outer.expect("Underlay must own the native shared BPM frame hierarchy");
        let outer_parts = scene_direct_children(&scene, outer).unwrap();
        assert_eq!(outer_parts.len(), 2);
        let rate = outer_parts[0].clone();
        let inner = outer_parts[1].clone();
        assert_eq!(
            scene_method_string(&scene, rate, "GetName").unwrap(),
            "RatemodDisplay"
        );
        assert_eq!(scene_method_string(&scene, inner, "GetName").unwrap(), "");
        let texts = scene_direct_children(&scene, inner).unwrap();
        let dual = enabled == [true, true] && split;
        assert_eq!(texts.len(), if dual { 2 } else { 1 });
        assert_eq!(
            scene_method_string(&scene, texts[0], "GetName").unwrap(),
            if dual { "DisplayP1" } else { "BPMDisplay" }
        );
        let pose = scene_state(&scene, outer).unwrap();
        let master = usize::from(!enabled[0]);
        assert_eq!(
            pose.x,
            320.0
                + if upper {
                    if master == 0 { 192.0 } else { -192.0 }
                } else {
                    0.0
                }
        );
        assert_eq!(pose.y, 52.0);
        assert!((pose.zoom - 1.33).abs() < 1e-6);
        assert_eq!(scene_state(&scene, rate).unwrap().y, 12.0);
        assert!((scene_state(&scene, rate).unwrap().zoom - 0.5).abs() < 1e-6);
        for text in &texts {
            assert_eq!(
                scene_field_string(&scene, *text, "Font", true).unwrap(),
                "Common Normal"
            );
            assert_eq!(scene_method_string(&scene, *text, "GetText").unwrap(), "");
            assert_eq!(
                scene_method_identity(&scene, *text, "GetParent").unwrap(),
                scene_table_identity(&scene, inner).unwrap()
            );
        }
        scene_set_root(&scene, "BPM_UNDERLAY", underlay).unwrap();
        scene_set_root(&scene, "BPM_OUTER", outer.clone()).unwrap();
        scene_set_root(&scene, "BPM_INNER", inner).unwrap();
        scene_exec(&scene,"assert(BPM_UNDERLAY:GetChild('BPMDisplay')==nil); assert(BPM_OUTER:GetChild('BPMDisplay')==nil); assert(BPM_OUTER:GetParent()==BPM_UNDERLAY); assert(BPM_INNER:GetParent()==BPM_OUTER)").unwrap();
        for (seconds, music_rate) in [(0.25, 1.0), (1.5, 1.5)] {
            let game = scene_global_table(&scene, "GAMESTATE").unwrap();
            let options =
                scene_method_table_string(&scene, game, "GetSongOptionsObject", "ModsLevel_Song")
                    .unwrap();
            scene_write_number(&scene, options, "MusicRate", music_rate).unwrap();
            let output = scene
                .advance(SongLuaLiveFrame {
                    screen_seconds: seconds,
                    music_seconds: seconds,
                    delta_seconds: 0.25,
                    ..Default::default()
                })
                .unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            for (index, text) in texts.iter().enumerate() {
                let player = if dual { index } else { master };
                let fast = player == 1 && split;
                let bpm = if seconds == 0.25 {
                    if fast { 240.0 } else { 120.0 }
                } else if fast {
                    300.0
                } else {
                    150.0
                };
                let expected = format!("{:.0}", bpm * music_rate);
                assert_eq!(
                    scene_method_string(&scene, *text, "GetText").unwrap(),
                    expected
                );
            }
            assert_eq!(
                scene_method_string(&scene, rate, "GetText").unwrap(),
                if music_rate == 1.0 { "" } else { "1.50x rate" }
            );
        }
    }
}

#[test]
fn live_underlay_standard_life_meter_uses_real_children_and_life_events() {
    use deadsync_rules::timing::{DelaySegment, StopSegment, TimingData, TimingSegments};
    for (width, enabled, hidden, expected) in [
        (640.0, [true, false], [false, false], 1),
        (854.0, [true, true], [false, false], 2),
        (854.0, [false, true], [false, false], 1),
        (640.0, [true, false], [true, false], 0),
    ] {
        let source = r#"
LIFE_METERS = {}
return Def.Actor { OnCommand = function()
    local underlay = SCREENMAN:GetTopScreen():GetChild('Underlay')
    local owners = underlay:GetChildren()['']
    if owners and tostring(owners):find('ActorFrame', 1, true) then owners = {owners} end
    for _, owner in ipairs(owners or {}) do
        if owner.GetChild and owner:GetChild('MeterFill') and owner:GetChild('MeterSwoosh') then
            assert(owner:GetParent() == underlay and owner:GetName() == '')
            assert(owner:GetNumChildren() == 4)
            assert(owner:GetVisible() and owner:GetY() == 20)
            LIFE_METERS[#LIFE_METERS + 1] = owner
        end
    end
    assert(#LIFE_METERS == $COUNT, 'Underlay must load the selected native Standard LifeMeter')
end }
"#
        .replace("$COUNT", &expected.to_string());
        let mut compiled = compile_with_context(
            &format!("standard-life-{width}-{enabled:?}-{hidden:?}"),
            &source,
            |context| {
                context.screen_width = width;
                for player in 0..LUA_PLAYERS {
                    context.players[player].enabled = enabled[player];
                    context.players[player].hide_lifebar = hidden[player];
                }
                context.player_timing = [
                    Some(TimingData::from_segments(
                        0.0,
                        0.0,
                        &TimingSegments {
                            bpms: vec![(0.0, 120.0)],
                            stops: vec![StopSegment {
                                beat: 1.0,
                                duration: 0.5,
                            }],
                            ..Default::default()
                        },
                        &[],
                    )),
                    Some(TimingData::from_segments(
                        0.0,
                        0.0,
                        &TimingSegments {
                            bpms: vec![(0.0, 240.0)],
                            delays: vec![DelaySegment {
                                beat: 1.0,
                                duration: 0.5,
                            }],
                            ..Default::default()
                        },
                        &[],
                    )),
                ];
            },
        );
        let mut scene = compiled
            .live_scene
            .take()
            .expect("real Standard life meters");
        let meters =
            scene_sequence_tables(&scene, scene_global_table(&scene, "LIFE_METERS").unwrap())
                .unwrap();
        let visible = (0..LUA_PLAYERS)
            .filter(|p| enabled[*p] && !hidden[*p])
            .collect::<Vec<_>>();
        assert_eq!(meters.len(), expected);
        for (meter, player) in meters.iter().zip(&visible) {
            let parts = scene_direct_children(&scene, *meter).unwrap();
            assert_eq!(parts.len(), 4);
            let x = width * 0.5
                + if *player == 0 { -1.0 } else { 1.0 }
                    * if width == 640.0 { 238.0 } else { 288.0 };
            for (index, width) in [(0, 140.0), (1, 136.0)] {
                assert_eq!(
                    scene_method_number(&scene, parts[index], "GetX")
                        .map(|value| value as f32)
                        .unwrap(),
                    x
                );
                assert_eq!(
                    scene_method_number(&scene, parts[index], "GetZoomedWidth")
                        .map(|value| value as f32)
                        .unwrap(),
                    width
                );
            }
            assert_eq!(
                scene_state(&scene, parts[1]).unwrap().diffuse,
                [0.0, 0.0, 0.0, 1.0]
            );
            assert_eq!(
                scene_state(&scene, parts[2]).unwrap().zoom_x,
                0.0,
                "native initial LifeMeter::Load message starts from zero fill width"
            );
            assert!(
                (scene_method_number(&scene, *meter, "GetTweenTimeLeft")
                    .map(|value| value as f32)
                    .unwrap()
                    - 0.1)
                    .abs()
                    < 1e-6
            );
            for part in &parts[2..] {
                assert_eq!(
                    scene_method_number(&scene, *part, "GetX")
                        .map(|value| value as f32)
                        .unwrap(),
                    x - 68.0
                );
                assert!(
                    (scene_method_number(&scene, *part, "GetZoomedWidth")
                        .map(|value| value as f32)
                        .unwrap()
                        - 68.0)
                        .abs()
                        < 1e-5
                );
                assert!(
                    scene_optional_function(&scene, *part, "LifeChangedMessageCommand")
                        .unwrap()
                        .is_some()
                );
                assert!(
                    scene_optional_function(&scene, *part, "HealthStateChangedMessageCommand")
                        .unwrap()
                        .is_some()
                );
            }
            let texture = scene_method_table(&scene, parts[3], "GetTexture").unwrap();
            assert_eq!(
                scene_method_number(&scene, texture, "GetSourceWidth")
                    .map(|value| value as u32)
                    .unwrap(),
                128
            );
            assert_eq!(
                scene_method_number(&scene, texture, "GetSourceHeight")
                    .map(|value| value as u32)
                    .unwrap(),
                16
            );
            let path = scene_texture_path(&scene, parts[3]).unwrap().unwrap();
            let bitmap = deadlib_assets::generated_texture(&path.to_string_lossy()).unwrap();
            assert!(bitmap.image.pixels().any(|pixel| pixel[3] != 0));
            assert!((scene_state(&scene, parts[3]).unwrap().diffuse[3] - 0.2).abs() < 1e-6);
        }
        for (seconds, delta) in [(0.1, 0.1), (0.6, 0.5), (1.4, 0.8)] {
            let output = scene
                .advance(SongLuaLiveFrame {
                    screen_seconds: seconds,
                    music_seconds: seconds,
                    delta_seconds: delta,
                    ..Default::default()
                })
                .unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            for (meter, player) in meters.iter().zip(&visible) {
                let parts = scene_direct_children(&scene, *meter).unwrap();
                assert!((scene_state(&scene, parts[2]).unwrap().zoom_x - 68.0).abs() < 1e-5);
                let pose = scene_state(&scene, parts[3]).unwrap();
                assert!((pose.zoom * pose.zoom_x * 128.0 - 68.0).abs() < 1e-5);
                assert_eq!(
                    pose.texcoord_velocity.unwrap()[0],
                    if seconds == 0.6 {
                        0.0
                    } else {
                        -(*player as f32 + 1.0)
                    }
                );
            }
        }
        if enabled[0] && !hidden[0] {
            scene_exec(
                &scene,
                "SCREENMAN:GetTopScreen():GetChild('PlayerP1'):SetLife(1)",
            )
            .unwrap();
            let fill = scene_direct_children(&scene, meters[0]).unwrap()[2].clone();
            assert!(
                (scene_method_number(&scene, fill, "GetZoomedWidth")
                    .map(|value| value as f32)
                    .unwrap()
                    - 136.0)
                    .abs()
                    < 1e-5
            );
            let mut frame = SongLuaLiveFrame {
                screen_seconds: 1.5,
                music_seconds: 1.5,
                delta_seconds: 0.1,
                ..Default::default()
            };
            frame.players[0].life = 1.0;
            let output = scene.advance(frame).unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
            let parts = scene_direct_children(&scene, meters[0]).unwrap();
            assert_eq!(scene_state(&scene, parts[2]).unwrap().diffuse, [1.0; 4]);
            assert_eq!(scene_state(&scene, parts[3]).unwrap().diffuse[3], 1.0);
            if meters.len() == 2 {
                let other = scene_direct_children(&scene, meters[1]).unwrap();
                assert!((scene_state(&scene, other[2]).unwrap().zoom_x - 68.0).abs() < 1e-5);
                assert!((scene_state(&scene, other[3]).unwrap().diffuse[3] - 0.2).abs() < 1e-6);
            }
        }
    }
}

#[test]
fn live_player_judgment_tree() {
    let mut compiled = compile(
        "player-judgment-tree",
        r#"
local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
local frame = player:GetChild('Judgment')
local sprite = frame:GetChild('JudgmentWithOffsets')
assert(frame == player:GetChild('Judgment'))
assert(sprite == frame:GetChildren().JudgmentWithOffsets)
return Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function(self) self:aux(player:GetTweenTimeLeft()) end)
    end,
}
"#,
    );
    let mut scene = compiled.live_scene.take().expect("owned scene");
    let player: Root = scene_global_table(&scene, "__songlua_top_screen_player_1").expect("P1");
    let frame: Root =
        scene_method_table_string(&scene, player, "GetChild", "Judgment").expect("judgment frame");
    let sprite: Root = scene_method_table_string(&scene, frame, "GetChild", "JudgmentWithOffsets")
        .expect("judgment sprite");
    let mut runtime = deadsync_gameplay::init_player_runtime();
    let judgment = deadsync_rules::judgment::Judgment {
        grade: deadsync_rules::judgment::JudgeGrade::Fantastic,
        window: Some(deadsync_rules::judgment::TimingWindow::W0),
        time_error_ms: 0.0,
        time_error_music_ns: 0,
        miss_because_held: false,
    };
    deadsync_gameplay::set_player_last_judgment(&mut runtime, judgment, 0.125);
    let mut input = SongLuaLiveFrame {
        screen_seconds: 0.125,
        music_seconds: 0.125,
        beat: 0.25,
        delta_seconds: 0.125,
        ..Default::default()
    };
    input.players[0] = SongLuaLivePlayer::from_player(&runtime, 100);
    assert_eq!(
        input.players[0].tap_scores, [0; 11],
        "autoplay feedback does not require scoring"
    );
    scene.advance(input.clone()).expect("first judgment");
    let left = |actor: &Root| {
        scene_method_number(&scene, *actor, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .expect("tween time")
    };
    assert!((left(&sprite) - 0.9).abs() < 1e-6);
    assert!((left(&player) - 0.9).abs() < 1e-6);
    assert!(
        (scene.states()[0].aux - 0.9).abs() < 1e-6,
        "authored callback sees the new queue"
    );
    let mut pose = SongLuaOverlayState::default();
    assert!(scene_replay_pose(&scene, sprite, &mut pose));
    assert!(
        (pose.zoom_x - 0.8).abs() < 1e-6,
        "new queue has not consumed this frame's delta"
    );
    let other: Root = scene_global_table(&scene, "__songlua_top_screen_player_2").expect("P2");
    assert_eq!(left(&other), 0.0, "a P1 judgment does not animate P2");

    input.screen_seconds += 0.0625;
    input.delta_seconds = 0.0625;
    scene
        .advance(input.clone())
        .expect("existing judgment advances");
    assert!(
        (scene_method_number(&scene, player, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .expect("P1 time")
            - 0.8375)
            .abs()
            < 1e-6,
        "the named child advances once and the unchanged snapshot does not restart it"
    );
    deadsync_gameplay::set_player_last_judgment(
        &mut runtime,
        deadsync_rules::judgment::Judgment {
            grade: deadsync_rules::judgment::JudgeGrade::Miss,
            window: None,
            ..judgment
        },
        0.25,
    );
    input.players[0] = SongLuaLivePlayer::from_player(&runtime, 100);
    input.screen_seconds = 0.25;
    scene.advance(input.clone()).expect("replacement judgment");
    assert!(
        (scene_method_number(&scene, sprite, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .expect("sprite time")
            - 0.9)
            .abs()
            < 1e-6
    );
    assert_eq!(
        scene_method_number(&scene, sprite, "GetState")
            .map(|value| value as usize)
            .expect("miss frame"),
        5
    );

    scene_write_number(&scene, player, "hibernate", 0.125).expect("pause player");
    input.screen_seconds += 0.0625;
    scene.advance(input.clone()).expect("hibernating player");
    assert!(
        (scene_method_number(&scene, sprite, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .expect("paused child")
            - 0.9)
            .abs()
            < 1e-6
    );
    assert!(
        (scene_method_number(&scene, player, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .expect("paused parent")
            - 0.9625)
            .abs()
            < 1e-6
    );
    input.screen_seconds += 0.125;
    input.delta_seconds = 0.125;
    scene.advance(input.clone()).expect("wake remainder");
    assert!(
        (scene_method_number(&scene, sprite, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .expect("wake child")
            - 0.8375)
            .abs()
            < 1e-6
    );
    scene_write_number(&scene, player, "SetUpdateRate", 2.0).expect("double rate");
    input.screen_seconds += 0.0625;
    input.delta_seconds = 0.0625;
    scene.advance(input.clone()).expect("rated child update");
    assert!(
        (scene_method_number(&scene, sprite, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .expect("rated child")
            - 0.7125)
            .abs()
            < 1e-6
    );
}

#[test]
fn live_player_hold_judgment_tree() {
    let mut compiled = compile(
        "player-hold-judgment-tree",
        r#"
local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
local unnamed = player:GetChildren()['']
local named = player:GetChild('')
assert(#unnamed == 5 and #named == 5 and named[1] == unnamed[1])
assert(player:GetNumChildren() == 8)
assert(unnamed[1]:GetNumChildren() == 0 and unnamed[1]:GetParent() == player)
local holds = {unnamed[2], unnamed[3], unnamed[4], unnamed[5]}
assert(#holds == 4 and named[2] == holds[1])
assert(tostring(holds[1]):find('HoldJudgment', 1, true))
for _, frame in ipairs(holds) do
    assert(frame:GetChild('HoldJudgment'):GetParent() == frame)
end
return Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function(self) self:aux(player:GetTweenTimeLeft()) end)
    end,
}
"#,
    );
    let mut scene = compiled.live_scene.take().expect("owned scene");
    let player: Root = scene_global_table(&scene, "__songlua_top_screen_player_1").expect("P1");
    let other: Root = scene_global_table(&scene, "__songlua_top_screen_player_2").expect("P2");
    let frames: Root = scene_field_table(&scene, player, "__songlua_hold_judgments", true)
        .expect("column judgments");
    let frame: Root = scene_index_table(&scene, frames, 2).expect("second column");
    let sprite: Root = scene_child(&scene, frame, "HoldJudgment").expect("native sprite");
    let mut input = SongLuaLiveFrame {
        screen_seconds: 0.125,
        delta_seconds: 0.125,
        ..Default::default()
    };
    input.players[0].hold_judgments[1] = Some(deadsync_gameplay::HoldJudgmentRenderInfo {
        result: deadsync_rules::note::HoldResult::Held,
        started_at_screen_s: 0.125,
    });
    scene
        .advance(input.clone())
        .expect("held feedback without tap feedback or scoring");
    assert!(
        (scene_method_number(&scene, sprite, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .expect("hold queue")
            - 0.8)
            .abs()
            < 1e-6
    );
    assert!(
        (scene.states()[0].aux - 0.8).abs() < 1e-6,
        "authored callbacks see hold descendants"
    );
    assert_eq!(
        scene_method_number(&scene, other, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .expect("P2 queue"),
        0.0
    );
    let mut pose = SongLuaOverlayState::default();
    assert!(scene_replay_pose(&scene, sprite, &mut pose));
    assert!(
        (pose.zoom_x - 0.4).abs() < 1e-6,
        "new queue does not consume the current frame"
    );
    assert_eq!(pose.diffuse[3], 1.0);

    input.screen_seconds += 0.0625;
    input.delta_seconds = 0.0625;
    scene
        .advance(input.clone())
        .expect("unchanged hold feedback");
    assert!(
        (scene.states()[0].aux - 0.7375).abs() < 1e-6,
        "the snapshot does not restart the queue"
    );
    input.players[0].hold_judgments[1] = Some(deadsync_gameplay::HoldJudgmentRenderInfo {
        result: deadsync_rules::note::HoldResult::LetGo,
        started_at_screen_s: 0.25,
    });
    scene
        .advance(input.clone())
        .expect("let-go replaces the old queue");
    assert!((scene.states()[0].aux - 0.8).abs() < 1e-6);
    assert_eq!(
        scene_method_number(&scene, sprite, "GetState")
            .map(|value| value as usize)
            .expect("let-go frame"),
        1
    );
    input.players[0].hold_judgments[1] = Some(deadsync_gameplay::HoldJudgmentRenderInfo {
        result: deadsync_rules::note::HoldResult::Missed,
        started_at_screen_s: 0.3125,
    });
    scene
        .advance(input.clone())
        .expect("native empty MissedHold command");
    assert!((scene.states()[0].aux - 0.7375).abs() < 1e-6);
    input.players[0].hold_judgments.fill(None);
    input.delta_seconds = 1.0;
    scene
        .advance(input.clone())
        .expect("feedback expires independently of the snapshot");
    assert_eq!(
        scene_method_number(&scene, player, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .expect("finished player"),
        0.0
    );
}

#[test]
fn live_named_notefield_advances_once() {
    let mut compiled = compile(
        "named-notefield-once",
        r#"
local field = SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField')
FIELD = field
return Def.ActorFrame {
    OnCommand = function(self)
        field:sleep(0.5)
        self:SetUpdateFunction(function(self) self:aux(field:GetTweenTimeLeft()) end)
    end,
}
"#,
    );
    let mut scene = compiled.live_scene.take().expect("owned scene");
    scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.125,
            delta_seconds: 0.125,
            ..Default::default()
        })
        .expect("one native child update");
    assert!(
        (scene.states()[0].aux - 0.375).abs() < 1e-6,
        "NoteField does not receive an extra special update"
    );
    scene_exec(
        &scene,
        r#"
local before = FIELD:GetNumChildren()
COL = FIELD:GetColumnActors()[2]
assert(COL == FIELD:get_column_actors()[2])
assert(FIELD:GetNumChildren() == before, 'column vector is separate from ActorFrame children')
COL:x(0):linear(1):x(100):queuecommand('ColumnDone')
COL:addcommand('ColumnDone', function(self) COLUMN_DONE = (COLUMN_DONE or 0) + 1 end)
FIELD:SetUpdateFunction(function() COLUMN_BEFORE = COL:GetTweenTimeLeft() end)
"#,
    )
    .expect("owned column commands");
    let column: Root = scene_global_table(&scene, "COL").unwrap();
    let left = |actor: &Root| {
        scene_method_number(&scene, *actor, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap()
    };
    assert_eq!(left(&column), 1.0);
    let mut input = SongLuaLiveFrame {
        screen_seconds: 0.25,
        delta_seconds: 0.125,
        ..Default::default()
    };
    scene
        .advance(input.clone())
        .expect("column update after the field callback");
    assert_eq!(
        scene_method_number(&scene, column, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        0.875,
        "NoteField's owned column vector advances once per screen update"
    );
    assert_eq!(
        scene_global_number(&scene, "COLUMN_BEFORE")
            .map(|value| value as f32)
            .unwrap(),
        1.0
    );
    let x = scene_method_number(&scene, column, "GetX")
        .map(|value| value as f32)
        .unwrap();
    assert_eq!(x, 12.5, "the real column tween updates its authored pose");
    input.delta_seconds = 0.0;
    scene
        .advance(input.clone())
        .expect("zero delta preserves the column queue");
    assert_eq!(
        scene_method_number(&scene, column, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        0.875
    );
    scene_exec(
        &scene,
        "FIELD:SetUpdateRate(3); WRAP = COL:AddWrapperState():sleep(1)",
    )
    .expect("field rate and column wrapper");
    let wrapper: Root = scene_global_table(&scene, "WRAP").unwrap();
    input.delta_seconds = 0.125;
    input.screen_seconds += 0.125;
    scene
        .advance(input.clone())
        .expect("columns use the field's original delta");
    assert_eq!(
        scene_method_number(&scene, column, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        0.75
    );
    assert_eq!(
        scene_method_number(&scene, wrapper, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        0.875
    );
    scene_exec(&scene, "FIELD:hibernate(0.25)").unwrap();
    input.screen_seconds += 0.125;
    scene
        .advance(input.clone())
        .expect("NoteField still updates columns while its ActorFrame sleeps");
    assert_eq!(
        scene_method_number(&scene, column, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        0.625
    );
    let player: Root = scene_global_table(&scene, "__songlua_top_screen_player_1").unwrap();
    scene_write_number(&scene, player, "hibernate", 0.25).unwrap();
    for _ in 0..2 {
        input.screen_seconds += 0.125;
        scene
            .advance(input.clone())
            .expect("hibernating ancestor skips the field; wake has zero overshoot");
        assert_eq!(
            scene_method_number(&scene, column, "GetTweenTimeLeft")
                .map(|value| value as f32)
                .unwrap(),
            0.625
        );
    }
    input.screen_seconds += 0.125;
    scene
        .advance(input.clone())
        .expect("field wake and column vector receive the original delta");
    assert_eq!(
        scene_method_number(&scene, column, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        0.5
    );
    scene_write_number(&scene, player, "SetUpdateRate", 2.0).unwrap();
    input.screen_seconds += 0.125;
    scene
        .advance(input.clone())
        .expect("column vector inherits its ancestor's rate");
    assert_eq!(
        scene_method_number(&scene, column, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        0.25
    );
    scene_write_number(&scene, column, "hibernate", 0.125).unwrap();
    input.screen_seconds += 0.125;
    scene
        .advance(input.clone())
        .expect("column hibernation passes only its wake overshoot to wrappers");
    assert_eq!(
        scene_method_number(&scene, column, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        0.125
    );
    assert_eq!(
        scene_method_number(&scene, wrapper, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        0.25
    );
    for _ in 0..2 {
        input.screen_seconds += 0.125;
        scene
            .advance(input.clone())
            .expect("column tween and queued command complete");
    }
    assert_eq!(
        scene_method_number(&scene, column, "GetTweenTimeLeft")
            .map(|value| value as f32)
            .unwrap(),
        0.0
    );
    assert_eq!(
        scene_method_number(&scene, column, "GetX")
            .map(|value| value as f32)
            .unwrap(),
        100.0
    );
    assert_eq!(
        scene_global_number(&scene, "COLUMN_DONE")
            .map(|value| value as u32)
            .unwrap(),
        1
    );
}

#[test]
fn live_scalar_writes_and_idle_frames_preserve_owned_getter_state() {
    let mut compiled = compile_with_context(
        "scalar-publication",
        r#"
return Def.Actor { InitCommand=function(self)
    ROOT=self; self:x(10):y(20):zoom(1):diffuse(0.2,0.3,0.4,1)
end }
"#,
        |context| {
            context.players[0].step_statistics_enabled = false;
            context.players[1].enabled = false;
        },
    );
    let mut scene = compiled.live_scene.take().unwrap();
    assert!(!scene.has_custom_draw().unwrap());
    let actor: Root = scene_global_table(&scene, "ROOT").unwrap();
    let colors = scene_owned_color(&scene, actor).unwrap();
    scene_exec(
        &scene,
        r#"
ROOT:skewx(0.25):x(11):zoom(1.25)
assert(ROOT:GetX()==11 and ROOT:GetZoomX()==1.25)
ROOT:linear(1):x(99):y(30):skewx(0)
assert(ROOT:GetX()==11 and ROOT:GetY()==20)
assert(ROOT:GetDestX()==99 and ROOT:GetDestY()==30)
"#,
    )
    .unwrap();
    assert_eq!(
        scene_owned_color(&scene, actor).unwrap(),
        colors,
        "scalar writes must preserve the unchanged owned color snapshot"
    );
    let frame = SongLuaLiveFrame {
        music_seconds: 0.5,
        screen_seconds: 0.5,
        beat: 0.5,
        delta_seconds: 0.5,
        ..Default::default()
    };
    assert!(scene.advance(frame.clone()).unwrap().errors.is_empty());
    scene_exec(&scene,"assert(ROOT:GetX()==55 and ROOT:GetY()==25); ROOT:y(50); assert(ROOT:GetY()==25 and ROOT:GetDestY()==50)").unwrap();
    assert!(scene.advance(frame.clone()).unwrap().errors.is_empty());
    scene_exec(
        &scene,
        "assert(ROOT:GetX()==99 and ROOT:GetY()==50); ROOT:x(101)",
    )
    .unwrap();
    let colors = scene_owned_color(&scene, actor).unwrap();
    assert!(scene.advance(frame).unwrap().errors.is_empty());
    assert_eq!(
        scene_owned_color(&scene, actor).unwrap(),
        colors,
        "an idle tween queue must not allocate and republish an unchanged pose"
    );
    scene_exec(
        &scene,
        "assert(ROOT:GetX()==101 and ROOT:GetY()==50 and ROOT:GetZoomX()==1.25)",
    )
    .unwrap();
}

fn scene_registry(scene: &SongLuaLiveScene) -> Result<Vec<Root>, Error> {
    let mut runtime = scene.runtime();
    let values = song::test_support::live_actor_registry(&runtime);
    values
        .into_iter()
        .map(|value| runtime.vm_mut().pin(value))
        .collect()
}
fn scene_draw_storage(scene: &SongLuaLiveScene) -> (usize, usize, usize) {
    song::test_support::live_draw_storage(&scene.runtime())
}
fn scene_bool(scene: &SongLuaLiveScene, source: impl AsRef<str>) -> Result<bool, Error> {
    let mut runtime = scene.runtime();
    let code = compile_named(
        source.as_ref().as_bytes(),
        b"=original boolean expression fixture",
    )?;
    let values = runtime.vm_mut().execute(&code)?;
    let Some(Value::Boolean(value)) = values.first() else {
        panic!("fixture Boolean expected: {values:?}")
    };
    Ok(*value)
}
fn scene_advance_actor(scene: &SongLuaLiveScene, root: Root, delta: f32) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let actor = runtime.vm_mut().rooted(root)?;
    song::test_support::advance_live_actor(&mut runtime, actor, delta)
}
fn scene_initial_state(scene: &SongLuaLiveScene, root: Root) -> Result<SongLuaOverlayState, Error> {
    let mut runtime = scene.runtime();
    let actor = runtime.vm_mut().rooted(root)?;
    song::test_support::read_actor_initial_state(&mut runtime, actor)
}
fn scene_command_depth(scene: &SongLuaLiveScene) -> usize {
    song::test_support::live_command_depth(&scene.runtime())
}
fn scene_set_command_depth(scene: &SongLuaLiveScene, depth: usize) {
    song::test_support::set_live_command_depth(&scene.runtime(), depth)
}
fn scene_install_command_depth(scene: &SongLuaLiveScene) -> Result<(), Error> {
    song::test_support::install_live_command_depth_probe(&mut scene.runtime())
}
fn scene_expression_function(
    scene: &SongLuaLiveScene,
    source: impl AsRef<str>,
) -> Result<Root, Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let code = compile_named(
        source.as_ref().as_bytes(),
        b"=original tap callback fixture",
    )?;
    let function = vm.execute(&code)?.first().copied().unwrap_or(Value::Nil);
    assert!(matches!(function, Value::Function(_) | Value::Native(_)));
    vm.pin(function)
}
fn scene_tap_callback(
    scene: &SongLuaLiveScene,
    field: Root,
    callback: Root,
    tap: deadsync_gameplay::GameplayNoteFieldTap,
    columns: usize,
) -> Result<deadsync_gameplay::GameplayNoteFieldTap, Error> {
    let mut runtime = scene.runtime();
    let field = runtime.vm_mut().rooted(field)?;
    let callback = runtime.vm_mut().rooted(callback)?;
    song::test_support::invoke_live_tap_callback(&mut runtime, field, callback, tap, columns)
}

#[cfg(feature = "test-support")]
#[test]
fn live_message_order_gaps_preserve_callback_dispatch() {
    let mut compiled = compile_with_context(
        "sole-native-judgment-subscriber",
        "return Def.Actor{}",
        |context| {
            context.defer_initial_live_update = true;
            context.message_actor_order = vec![("1".into(), 10)];
        },
    );
    let mut scene = compiled.live_scene.take().expect("retained theme observer");
    // Keep this registry-order corner case a genuinely lone subscriber.
    // The newly loaded native ColumnFlash actors are separate unit fixtures.
    scene_exec(
        &scene,
        r#"
local underlay=SCREENMAN:GetTopScreen():GetChild('Underlay')
for _,pn in ipairs{'P1','P2'} do
    local container=underlay:GetChild('NoteFieldContainer'..pn)
    if container then rawset(container:GetChild('')[1], 'JudgmentMessageCommand', nil) end
end
"#,
    )
    .expect("isolate original lone Judgment subscriber fixture");
    for actor in scene_registry(&scene).unwrap().into_iter() {
        let actor = actor;
        if let Some(callback) =
            scene_optional_function(&scene, actor, "JudgmentMessageCommand").unwrap()
            && !scene_function_source(&scene, callback)
                .unwrap()
                .is_some_and(|source| source.ends_with("FailOnHoldStart.lua"))
        {
            scene_set_field_value(&scene, actor, "JudgmentMessageCommand", Value::Nil).unwrap();
        }
    }
    let mut frame = SongLuaLiveFrame {
        music_seconds: 60.0,
        screen_seconds: 60.0,
        beat: 120.0,
        delta_seconds: 0.01,
        ..Default::default()
    };
    frame.players[0].life = 0.5;
    frame.players[0].autoplay = true;
    frame.players[0].fail_enabled = true;
    let output = scene.advance(frame.clone()).expect("owned gameplay frame");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    let subscribers = scene_registry(&scene)
        .unwrap()
        .into_iter()
        .filter(|actor| {
            scene_optional_function(&scene, *actor, "JudgmentMessageCommand")
                .unwrap()
                .is_some()
        })
        .count();
    assert_eq!(
        subscribers, 1,
        "only the owned FailOnHoldStart listener subscribes"
    );
    let params = scene_new_table(&scene).unwrap();
    scene_set_field_string(&scene, params, "Player", "PlayerNumber_P1").unwrap();
    let result = scene_native_broadcast(&scene, "Judgment", Some(params));
    assert!(
        result.is_ok(),
        "a lone native Judgment subscriber is dispatched without a sort rank: {result:?}"
    );
    scene_exec(
        &scene,
        "SCREENMAN:GetTopScreen():GetChild('Overlay'):playcommand('Off')",
    )
    .expect("owned overlay Off");
    assert_eq!(
        scene_take_output(&mut scene).unwrap().player_writes,
        vec![SongLuaLivePlayerWrite::Fail { player: 0 }],
        "the real Judgment latch fails autoplay during overlay Off"
    );

    let mut compiled = compile_with_context(
        "multiple-captured-subscribers",
        r#"
CALLS = 0
return Def.ActorFrame {
    ProbeMessageCommand = function() CALLS = CALLS + 1 end,
    Def.Actor {ProbeMessageCommand = function() CALLS = CALLS + 1 end},
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.message_actor_order = vec![("1".into(), 10)];
        },
    );
    let mut scene = compiled.live_scene.take().unwrap();
    let result = scene_native_broadcast(&scene, "Probe", None);
    assert!(
        result.is_ok(),
        "missing captured ordering must not abort callback dispatch: {result:?}"
    );
    assert_eq!(
        scene_global_number(&scene, "CALLS")
            .map(|value| value as u32)
            .unwrap(),
        2,
        "both subscribed callbacks run when an ordering identity is absent"
    );
    let output = scene_take_output(&mut scene).unwrap();
    assert!(output.errors.is_empty());
    assert_eq!(
        output.coverage_gaps,
        vec!["missing captured subscriber identity for 1/1"]
    );
    scene_native_broadcast(&scene, "Probe", None).unwrap();
    assert_eq!(
        scene_global_number(&scene, "CALLS")
            .map(|value| value as u32)
            .unwrap(),
        4
    );
    assert!(
        scene_take_output(&mut scene)
            .unwrap()
            .coverage_gaps
            .is_empty(),
        "a recurring ordering gap is reported once per scene"
    );

    let mut compiled = compile_with_context(
        "partially-captured-order",
        r#"
SEEN = {}
return Def.ActorFrame {
    InitCommand = function(self) ROOT = self end,
    ProbeMessageCommand = function() table.insert(SEEN, 'root') end,
    Def.Actor {ProbeMessageCommand = function() table.insert(SEEN, 'first') end},
    Def.Actor {ProbeMessageCommand = function() table.insert(SEEN, 'second') end},
}
"#,
        |context| {
            context.defer_initial_live_update = true;
            context.message_actor_order = vec![("1/1".into(), 20), ("1/2".into(), 10)];
        },
    );
    let mut scene = compiled.live_scene.take().unwrap();
    let root =
        scene_table_identity(&scene, scene_global_table(&scene, "ROOT").unwrap()).unwrap() as usize;
    let mut pointers: Vec<_> = scene_registry(&scene)
        .unwrap()
        .into_iter()
        .filter(|actor| {
            scene_optional_function(&scene, *actor, "ProbeMessageCommand")
                .unwrap()
                .is_some()
        })
        .map(|actor| scene_table_identity(&scene, actor).unwrap())
        .collect();
    pointers.sort_unstable();
    let root_slot = pointers
        .iter()
        .position(|pointer| *pointer == root)
        .unwrap();
    scene_native_broadcast(&scene, "Probe", None).unwrap();
    let seen: Vec<_> = scene_global_strings(&scene, "SEEN").unwrap();
    assert_eq!(seen.len(), 3);
    assert_eq!(
        seen[root_slot], "root",
        "the unknown subscriber keeps its VM identity slot"
    );
    assert_eq!(
        seen.into_iter()
            .filter(|name| name != "root")
            .collect::<Vec<_>>(),
        vec!["second", "first"],
        "captured listeners retain their native relative order"
    );
    assert_eq!(
        scene_take_output(&mut scene).unwrap().coverage_gaps,
        vec!["missing captured subscriber identity for 1"]
    );

    let mut compiled = compile_with_context(
        "mixed-theme-judgment-subscribers",
        "CALLS = 0; return Def.Actor {JudgmentMessageCommand = function() CALLS = CALLS + 1 end}",
        |context| {
            context.defer_initial_live_update = true;
            context.message_actor_order = vec![("1".into(), 10)];
        },
    );
    let mut scene = compiled.live_scene.take().unwrap();
    scene.advance(frame).unwrap();
    let params = scene_new_table(&scene).unwrap();
    scene_set_field_string(&scene, params, "Player", "PlayerNumber_P1").unwrap();
    scene_set_field_string(&scene, params, "TapNoteScore", "TapNoteScore_W1").unwrap();
    scene_native_broadcast(&scene, "Judgment", Some(params)).unwrap();
    scene_exec(
        &scene,
        "SCREENMAN:GetTopScreen():GetChild('Overlay'):playcommand('Off')",
    )
    .unwrap();
    let output = scene_take_output(&mut scene).unwrap();
    assert!(output.errors.is_empty());
    assert_eq!(
        scene_global_number(&scene, "CALLS")
            .map(|value| value as u32)
            .unwrap(),
        1
    );
    assert_eq!(
        output.player_writes,
        vec![SongLuaLivePlayerWrite::Fail { player: 0 }],
        "the owned Judgment latch still fails autoplay with an authored subscriber"
    );
    assert!(
        output
            .coverage_gaps
            .iter()
            .any(|gap| gap.contains("ScreenGameplay/")),
        "missing native theme ordering remains explicit coverage"
    );
}

#[test]
fn live_draw_follows_owned_update_and_replaces_automatic_children() {
    let mut compiled = compile_with_context(
        "live-draw",
        r#"
DRAWS, NESTED, AUTOMATIC, QUEUED = 0, 0, 0, 0
return Def.ActorFrame {
    Name='DrawRoot',
    OnCommand=function(self)
        ROOT = self
        local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
        local state = GAMESTATE:GetPlayerState('PlayerNumber_P1')
        self:SetUpdateFunction(function(actor) actor:aux(actor:getaux()+1) end)
        self:SetDrawFunction(function(actor)
            assert(actor:getaux() > DRAWS, 'draw must follow the owned actor update')
            assert(state:GetHealthState() == 'HealthState_Dead', 'draw must see the updated health')
            DRAWS = DRAWS + 1
            actor:GetChild('Nested'):Draw()
            actor:queuecommand('AfterDraw')
        end)
        ORIGINAL_DRAW = self:GetDrawFunction()
        player:SetLife(0)
    end,
    AfterDrawCommand=function() QUEUED = QUEUED + 1 end,
    Def.ActorFrame {
        Name='Nested',
        InitCommand=function(self)
            self:SetDrawFunction(function(actor)
                NESTED = NESTED + 1
                actor:GetChild('Leaf'):x(NESTED):Draw()
            end)
        end,
        Def.Quad { Name='Leaf' },
    },
    Def.ActorFrame {
        Name='Automatic',
        InitCommand=function(self)
            self:SetDrawFunction(function() AUTOMATIC = AUTOMATIC + 1 end)
        end,
    },
}
"#,
        |context| context.defer_initial_live_update = true,
    );
    let mut scene = compiled.live_scene.take().expect("owned draw scene");
    let output = scene
        .advance(SongLuaLiveFrame {
            screen_seconds: 0.1,
            music_seconds: 0.1,
            beat: 0.1,
            delta_seconds: 0.1,
            players: [SongLuaLivePlayer {
                fail_enabled: true,
                ..Default::default()
            }; LUA_PLAYERS],
            ..Default::default()
        })
        .expect("first update and draw");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    assert_eq!(
        scene_global_number(&scene, "DRAWS")
            .map(|value| value as u32)
            .unwrap(),
        1,
        "the live frame must execute its installed draw callback"
    );
    assert_eq!(
        scene_global_number(&scene, "NESTED")
            .map(|value| value as u32)
            .unwrap(),
        1,
        "explicit Draw must execute the nested callback"
    );
    assert_eq!(
        scene_global_number(&scene, "AUTOMATIC")
            .map(|value| value as u32)
            .unwrap(),
        0,
        "custom draw replaces automatic child traversal"
    );
    assert_eq!(
        scene_global_number(&scene, "QUEUED")
            .map(|value| value as u32)
            .unwrap(),
        0,
        "draw must leave queued commands for the next actor update"
    );

    let frame = SongLuaLiveFrame {
        screen_seconds: 0.2,
        music_seconds: 0.2,
        beat: 0.2,
        delta_seconds: 0.1,
        players: [SongLuaLivePlayer {
            fail_enabled: true,
            ..Default::default()
        }; LUA_PLAYERS],
        ..Default::default()
    };
    scene.update(frame.clone()).expect("update without draw");
    assert_eq!(
        scene_global_number(&scene, "DRAWS")
            .map(|value| value as u32)
            .unwrap(),
        1
    );
    assert_eq!(
        scene_global_number(&scene, "QUEUED")
            .map(|value| value as u32)
            .unwrap(),
        1
    );
    scene.draw().expect("separate draw");
    assert_eq!(
        scene_global_number(&scene, "DRAWS")
            .map(|value| value as u32)
            .unwrap(),
        2
    );

    scene_exec(&scene, "ROOT:visible(false)").unwrap();
    scene.advance(frame.clone()).unwrap();
    assert_eq!(
        scene_global_number(&scene, "DRAWS")
            .map(|value| value as u32)
            .unwrap(),
        2
    );
    scene_exec(&scene, "ROOT:visible(true):hibernate(0.25)").unwrap();
    scene.advance(frame.clone()).unwrap();
    assert_eq!(
        scene_global_number(&scene, "DRAWS")
            .map(|value| value as u32)
            .unwrap(),
        2
    );
    scene_exec(&scene, "ROOT:hibernate(0):diffusealpha(0)").unwrap();
    scene.advance(frame.clone()).unwrap();
    assert_eq!(
        scene_global_number(&scene, "DRAWS")
            .map(|value| value as u32)
            .unwrap(),
        2,
        "transparent actors do not reach native DrawPrimitives"
    );
    scene_exec(&scene, "ROOT:glow(1,1,1,1)").unwrap();
    scene.advance(frame.clone()).unwrap();
    assert_eq!(
        scene_global_number(&scene, "DRAWS")
            .map(|value| value as u32)
            .unwrap(),
        3,
        "glow makes a transparent actor partially opaque"
    );

    scene_exec(&scene,"ROOT:diffusealpha(1):glow(0,0,0,0):AddWrapperState(); ROOT:GetWrapperState(1):visible(false)").unwrap();
    scene.advance(frame.clone()).unwrap();
    assert_eq!(
        scene_global_number(&scene, "DRAWS")
            .map(|value| value as u32)
            .unwrap(),
        3,
        "an invisible wrapper aborts the nested draw"
    );
    scene_exec(
        &scene,
        "ROOT:GetWrapperState(1):visible(true); ROOT:SetDrawFunction(nil)",
    )
    .unwrap();
    scene.advance(frame.clone()).unwrap();
    assert_eq!(
        scene_global_number(&scene, "DRAWS")
            .map(|value| value as u32)
            .unwrap(),
        3
    );
    assert_eq!(
        scene_global_number(&scene, "AUTOMATIC")
            .map(|value| value as u32)
            .unwrap(),
        1,
        "removing a callback restores ordinary child traversal"
    );
    scene_exec(
        &scene,
        "ROOT:SetDrawFunction(function(actor) actor:x(42) end)",
    )
    .unwrap();
    scene.advance(frame.clone()).unwrap();
    assert_eq!(
        scene
            .states()
            .iter()
            .find(|state| state.x == 42.0)
            .unwrap()
            .x,
        42.0,
        "draw mutations are reflected in the current live visual poses"
    );
    scene_exec(&scene, "ROOT:SetDrawFunction(ORIGINAL_DRAW)").unwrap();
    for index in 0..2000 {
        let output = scene
            .advance(SongLuaLiveFrame {
                screen_seconds: 1.0 + index as f64 * 0.1,
                music_seconds: 1.0 + index as f64 * 0.1,
                beat: 1.0 + index as f64 * 0.1,
                ..frame.clone()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let (frames, calls, bytes) = scene_draw_storage(&scene);
        assert_eq!(frames, 1, "draw storage must retain only the current frame");
        assert_eq!(calls, 1, "one explicitly drawn leaf per live frame");
        assert!(
            bytes < 8192,
            "live draw storage must not accumulate song history"
        );
    }
    let draws = scene_global_number(&scene, "DRAWS")
        .map(|value| value as u32)
        .unwrap();
    scene.end().unwrap();
    scene.draw().unwrap();
    assert_eq!(
        scene_global_number(&scene, "DRAWS")
            .map(|value| value as u32)
            .unwrap(),
        draws,
        "a popped screen does not run another draw callback"
    );
}

#[test]
fn live_render_defaults_do_not_dispatch_public_class_lookups() {
    let compiled = compile("raw-render-defaults", "return Def.Actor{}");
    let scene = compiled.live_scene.unwrap();
    let actor=scene_expression_table(&scene,"return setmetatable({__songlua_actor_type='Actor',__songlua_state_x=42}, {__index=function() error('render defaults invoked public class lookup') end})").unwrap();
    let state = scene_initial_state(&scene, actor).unwrap();
    assert_eq!(state.x, 42.0);
    assert!(state.visible);
    assert_eq!(state.diffuse, [1.0; 4]);
}

#[test]
fn actor_parent_loading() {
    let mut compiled = compile(
        "parent-loading",
        r#"
return Def.ActorFrame{Name='DeclaredRoot',
 Def.ActorFrame{InitCommand=function(self)
  local parent=self:GetParent()
  assert(parent:GetName()=='')
  parent:name('from-child')
  assert(parent:GetName()=='from-child')
 end},
 InitCommand=function(self) assert(self:GetName()=='DeclaredRoot') end,
 OnCommand=function(self)
  local parent=self:GetParent()
  self:SetUpdateFunction(function(self)
   assert(self:GetParent()==parent)
   assert(parent:GetName()=='SongForeground')
   assert(parent:GetParent()==SCREENMAN:GetTopScreen())
   assert(parent:GetParent():GetParent()==nil)
   self:aux(1)
  end)
 end,
}
"#,
    );
    let mut scene = compiled.live_scene.take().expect("owned scene");
    for seconds in [0.0, 0.125] {
        let output = scene
            .advance(SongLuaLiveFrame {
                music_seconds: seconds,
                screen_seconds: seconds,
                delta_seconds: 0.125,
                ..Default::default()
            })
            .expect("owned update");
        assert!(output.errors.is_empty(), "{:?}", output.errors);
    }
    scene_advance_actor(&scene, scene_entry_root(&scene).unwrap(), 0.0).expect("direct update");
    let output = scene_take_output(&mut scene).expect("direct output");
    assert!(output.errors.is_empty(), "{:?}", output.errors);
    let actor: Root = scene_entry_root(&scene).expect("root");
    assert_eq!(
        scene_method_number(&scene, actor, "getaux")
            .map(|value| value as f32)
            .expect("updated aux"),
        1.0
    );
}

#[test]
fn live_note_field_tap_returns_follow_native_optional_conversions() {
    let compiled = compile("tap-return-conversions", "return Def.Actor{}");
    let scene = compiled.live_scene.unwrap();
    let field = scene_new_table(&scene).unwrap();
    scene_install_command_depth(&scene).unwrap();
    let original = deadsync_gameplay::GameplayNoteFieldTap {
        player: 1,
        column: 3,
        score: "W3",
        bright: false,
    };
    for (body, column, score, bright) in [
        ("return nil, nil, nil", 3, "W3", false),
        ("return '2.8', 'TapNoteScore_W1', true", 1, "W1", true),
        ("return 0.5, 'w2', 1", 0, "W2", false),
        ("return false, 9, true", 3, "W1", true),
    ] {
        let callback: Root = scene_expression_function(
            &scene,
            format!(
                r#"
return function(column, score, bright)
    assert(CommandDepth() == 0, 'direct NoteField callback has no ActorFrame command scope')
    assert(column == 4 and score == 'TapNoteScore_W3' and bright == false)
    {body}
end
"#
            ),
        )
        .unwrap();
        let actual = scene_tap_callback(&scene, field, callback, original, 4).unwrap();
        assert_eq!(
            actual,
            deadsync_gameplay::GameplayNoteFieldTap {
                column,
                score,
                bright,
                ..original
            }
        );
    }
    let invalid: Root =
        scene_expression_function(&scene, "return function() return 9, 'typo', true end").unwrap();
    let actual = scene_tap_callback(&scene, field, invalid, original, 4).unwrap();
    assert_eq!(
        actual,
        deadsync_gameplay::GameplayNoteFieldTap {
            bright: true,
            ..original
        }
    );
    assert!(
        scene_field_bool(
            &scene,
            field,
            "__songlua_did_tap_note_column_error_reported",
            true
        )
        .unwrap()
    );
    assert!(
        scene_field_bool(
            &scene,
            field,
            "__songlua_did_tap_note_score_error_reported",
            true
        )
        .unwrap()
    );
    let fails: Root = scene_expression_function(
        &scene,
        "return function() error('authored tap failure') end",
    )
    .unwrap();
    assert!(scene_tap_callback(&scene, field, fails, original, 4).is_err());
    assert_eq!(scene_command_depth(&scene), 0);
    scene_set_command_depth(&scene, 2);
    let nested: Root = scene_expression_function(
        &scene,
        "return function() assert(CommandDepth() == 2); return nil end",
    )
    .unwrap();
    assert_eq!(
        scene_tap_callback(&scene, field, nested, original, 4).unwrap(),
        original
    );
    assert_eq!(
        scene_command_depth(&scene),
        2,
        "the direct call preserves an already active authored scope"
    );
}

#[path = "../../../tests/support/perf.rs"]
mod allocations;
fn scene_copy_pose(scene: &SongLuaLiveScene, source: Root, into: Root) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let source = runtime.vm_mut().rooted(source)?;
    let into = runtime.vm_mut().rooted(into)?;
    song::test_support::copy_live_actor_pose(&mut runtime, source, into)
}
fn scene_snapshot(scene: &SongLuaLiveScene, source: Root, mutable: bool) -> Result<Root, Error> {
    let mut runtime = scene.runtime();
    let source = runtime.vm_mut().rooted(source)?;
    song::test_support::export_live_actor_state(&mut runtime, source, mutable)
}
fn scene_restore_snapshot(scene: &SongLuaLiveScene, into: Root, saved: Root) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let into = runtime.vm_mut().rooted(into)?;
    let saved = runtime.vm_mut().rooted(saved)?;
    song::test_support::restore_live_actor_state(&mut runtime, into, saved)
}
fn scene_destination_vertices(
    scene: &SongLuaLiveScene,
    actor: Root,
) -> Result<[[f32; 4]; 4], Error> {
    let mut runtime = scene.runtime();
    let value = runtime.vm_mut().rooted(actor)?;
    Ok(
        song::test_support::live_destination_vertices(&runtime, value)
            .expect("owned native corner colors"),
    )
}
fn scene_value(scene: &SongLuaLiveScene, root: Root) -> Value {
    scene.runtime().vm_mut().rooted(root).unwrap()
}
fn scene_owned_color(scene: &SongLuaLiveScene, root: Root) -> Option<(usize, [f32; 4], [f32; 4])> {
    let mut runtime = scene.runtime();
    let value = runtime.vm_mut().rooted(root).unwrap();
    song::test_support::live_color_storage(&runtime, value)
}
fn scene_set_index_number(
    scene: &SongLuaLiveScene,
    root: Root,
    index: usize,
    value: f64,
) -> Result<(), Error> {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let Value::Table(table) = vm.rooted(root)? else {
        unreachable!()
    };
    vm.raw_table_set(table, Value::Number(index as f64), Value::Number(value))
}

/// Cache ordinary class methods before timing the mutation path. Calls use
/// real VM argument registers and production setters, including the first
/// uniform-to-corner color conversion inside the measured closure.
struct FixtureMutations<'a> {
    vm: &'a mut deadlib_lua::Vm,
    actor: Value,
    methods: std::collections::HashMap<&'static str, Value>,
}
fn scene_with_mutations<R>(
    scene: &SongLuaLiveScene,
    actor: Root,
    run: impl FnOnce(&mut FixtureMutations<'_>) -> R,
) -> R {
    let mut runtime = scene.runtime();
    let vm = runtime.vm_mut();
    let actor = vm.rooted(actor).unwrap();
    let code = compile_named(b"return Actor", b"=native mutation class fixture").unwrap();
    let class = vm.execute(&code).unwrap()[0];
    let mut methods = std::collections::HashMap::new();
    for name in [
        "x",
        "y",
        "z",
        "aux",
        "zoom",
        "zoomx",
        "zoomy",
        "zoomz",
        "skewx",
        "skewy",
        "cropleft",
        "cropright",
        "croptop",
        "cropbottom",
        "fadeleft",
        "faderight",
        "fadetop",
        "fadebottom",
        "basezoomx",
        "halign",
        "GetHAlign",
        "SetTextureFiltering",
        "ztest",
        "diffuse",
        "glow",
        "diffuseupperleft",
        "diffuseupperright",
        "diffuselowerleft",
        "diffuselowerright",
        "diffusetopedge",
    ] {
        let Value::Table(class) = class else {
            unreachable!()
        };
        let key = vm.try_intern(name.as_bytes()).unwrap();
        // x/y class entries forward through another call. The original allocation
        // fixture measures the native setter itself; use its installed binding.
        let method_owner = if matches!(name, "x" | "y") {
            let Value::Table(owner) = actor else {
                unreachable!()
            };
            let key = vm.try_intern(b"__songlua_bound_methods").unwrap();
            let Value::Table(bound) = vm.raw_table_get(owner, key).unwrap() else {
                panic!("native scalar bindings")
            };
            bound
        } else {
            class
        };
        let function = vm.raw_table_get(method_owner, key).unwrap();
        assert!(
            matches!(function, Value::Native(_) | Value::Function(_)),
            "mutation fixture class method {name}"
        );
        methods.insert(name, function);
    }
    run(&mut FixtureMutations { vm, actor, methods })
}
impl FixtureMutations<'_> {
    fn scalar(&mut self, key: &str, value: f32) -> Result<(), Error> {
        let name = match key {
            "zoom_x" => "zoomx",
            "zoom_y" => "zoomy",
            "zoom_z" => "zoomz",
            "skew_x" => "skewx",
            "skew_y" => "skewy",
            "basezoom_x" => "basezoomx",
            other => other,
        };
        self.vm.call(
            self.methods[name],
            &[self.actor, Value::Number(f64::from(value))],
        )?;
        Ok(())
    }
    fn flag(&mut self, key: &str, value: bool) -> Result<(), Error> {
        let name = match key {
            "texture_filtering" => "SetTextureFiltering",
            "depth_test" => "ztest",
            _ => panic!("fixture flag {key}"),
        };
        self.vm
            .call(self.methods[name], &[self.actor, Value::Boolean(value)])?;
        Ok(())
    }
    fn color(&mut self, name: &str, value: [f32; 4]) -> Result<(), Error> {
        self.vm.call(
            self.methods[name],
            &[
                self.actor,
                Value::Number(f64::from(value[0])),
                Value::Number(f64::from(value[1])),
                Value::Number(f64::from(value[2])),
                Value::Number(f64::from(value[3])),
            ],
        )?;
        Ok(())
    }
    fn vertices(&mut self, colors: [[f32; 4]; 4]) -> Result<(), Error> {
        for (name, color) in [
            "diffuseupperleft",
            "diffuseupperright",
            "diffuselowerleft",
            "diffuselowerright",
        ]
        .into_iter()
        .zip(colors)
        {
            self.color(name, color)?
        }
        Ok(())
    }
    fn vertex_diffuse(&mut self, args: &[Value], mask: u8) -> Result<(), Error> {
        assert_eq!(args.first(), Some(&self.actor));
        let name = match mask {
            3 => "diffusetopedge",
            _ => panic!("fixture corner mask {mask}"),
        };
        self.vm.call(self.methods[name], args)?;
        Ok(())
    }
    fn halign(&mut self) -> Result<f32, Error> {
        let values = self.vm.call(self.methods["GetHAlign"], &[self.actor])?;
        let [Value::Number(value)] = *values else {
            panic!("native alignment")
        };
        Ok(value as f32)
    }
}

#[test]
fn live_native_colors_export_independent_destination_snapshots() {
    let mut compiled = compile_with_context(
        "native-color-snapshots",
        r#"
return Def.Actor {
    InitCommand=function(self) ROOT=self; self.authored={retained=true} end,
}
"#,
        |context| context.defer_initial_live_update = true,
    );
    let mut scene = compiled.live_scene.take().unwrap();
    scene.advance(SongLuaLiveFrame::default()).unwrap();
    scene_exec(
        &scene,
        r#"
ROOT:diffuse(0.25,0.5,0.75,1):glow(0,0,0,0)
ROOT:linear(1):diffuse(0.5,0.75,1,0.5):diffuseupperleft(1,0,0,1):glow(1,0.5,0.25,0.75)
assert(ROOT:GetDiffuse()[1]==0.5 and ROOT:GetGlow()[4]==0.75)
"#,
    )
    .unwrap();
    let actor: Root = scene_global_table(&scene, "ROOT").unwrap();
    let before = scene_state(&scene, actor).unwrap();
    assert_eq!(before.diffuse, [0.25, 0.5, 0.75, 1.0]);
    assert_eq!(before.glow, [0.0; 4]);
    let copied = scene_new_table(&scene).unwrap();
    scene_copy_pose(&scene, actor, copied).unwrap();
    let snapshot = scene_snapshot(&scene, actor, true).unwrap();
    let semantic = scene_snapshot(&scene, actor, false).unwrap();
    let restored = scene_new_table(&scene).unwrap();
    scene_restore_snapshot(&scene, restored, snapshot).unwrap();
    let restored_semantic = scene_new_table(&scene).unwrap();
    scene_restore_snapshot(
        &scene,
        restored_semantic,
        scene_snapshot(&scene, semantic, false).unwrap(),
    )
    .unwrap();
    for snapshot in [&copied, &restored, &restored_semantic] {
        let state = scene_initial_state(&scene, *snapshot).unwrap();
        assert_eq!(state.diffuse, [0.5, 0.75, 1.0, 0.5]);
        assert_eq!(state.glow, [1.0, 0.5, 0.25, 0.75]);
        assert_eq!(state.vertex_colors.unwrap()[0], [1.0, 0.0, 0.0, 1.0]);
    }
    // A snapshot owns its tables; neither mutation direction changes the other.
    scene_set_index_number(
        &scene,
        scene_field_table(&scene, copied, "__songlua_diffuse", true).unwrap(),
        1,
        99.0,
    )
    .unwrap();
    scene_exec(
        &scene,
        r#"
assert(ROOT:GetDiffuse()[1]==0.5)
ROOT:finishtweening():diffuse(0.75,0.25,0.5,1):glow(0,0,0,0)
assert(ROOT:GetDiffuse()[1]==0.75 and ROOT:GetGlow()[4]==0)
assert(ROOT.authored.retained)
"#,
    )
    .unwrap();
    assert_eq!(
        scene_initial_state(&scene, restored).unwrap().diffuse,
        [0.5, 0.75, 1.0, 0.5]
    );
    assert_eq!(
        before.diffuse,
        [0.25, 0.5, 0.75, 1.0],
        "earlier draw state is immutable"
    );
    // Immediate native color writes do not allocate replacement Lua tables.
    scene_with_mutations(&scene, actor, |mutation| {
        allocations::assert_no_churn(|| {
            for i in 0..100 {
                mutation.color("diffuse", [i as f32; 4]).unwrap();
                mutation.color("glow", [i as f32; 4]).unwrap();
            }
        })
    });
}

#[test]
fn live_native_vertex_colors_mutate_destinations_without_allocation() {
    let mut compiled = compile_with_context(
        "native-vertex-color-writes",
        r#"
return Def.Quad { InitCommand=function(self) Q=self end }
"#,
        |context| context.defer_initial_live_update = true,
    );
    let mut scene = compiled.live_scene.take().unwrap();
    scene.advance(SongLuaLiveFrame::default()).unwrap();
    let actor: Root = scene_global_table(&scene, "Q").unwrap();
    let base = [0.25, 0.5, 0.75, 1.0];
    let color = [1.0, 0.75, 0.5, 0.25];
    // ITGmania Actor.h stores diffuse corners as UL, UR, LL, LR.
    // Sprite.cpp maps those slots to render vertices separately.
    for (method, mask) in [
        ("diffuseupperleft", 1),
        ("diffuseupperright", 2),
        ("diffuselowerleft", 4),
        ("diffuselowerright", 8),
        ("diffusetopedge", 3),
        ("diffuserightedge", 10),
        ("diffusebottomedge", 12),
        ("diffuseleftedge", 5),
    ] {
        for queued in [false, true] {
            scene_init_state(&scene, actor).unwrap();
            scene_exec(&scene, "Q:stoptweening():diffuse(0.25,0.5,0.75,1)").unwrap();
            if queued {
                scene_exec(&scene, "Q:linear(1)").unwrap();
            }
            let before = scene_state(&scene, actor).unwrap();
            scene_exec(&scene, format!("Q:{method}(1,0.75,0.5,0.25)")).unwrap();
            let expected = std::array::from_fn(|i| {
                if mask & (1 << i) != 0 {
                    color
                } else {
                    [1.0; 4]
                }
            });
            assert_eq!(scene_destination_vertices(&scene, actor).unwrap(), expected);
            let current = scene_state(&scene, actor).unwrap();
            if queued {
                assert_eq!(current, before, "a destination write cannot change current");
                scene_exec(&scene, "Q:finishtweening()").unwrap();
            } else {
                assert_eq!(current.vertex_colors, Some(expected));
            }
            assert_eq!(
                scene_state(&scene, actor).unwrap().vertex_colors,
                Some(expected)
            );
        }
    }
    // Read color tables at the call boundary; later edits cannot change native state.
    scene_exec(
        &scene,
        r#"
local color = {0,1,0,0.5}
Q:diffusetopedge(color)
color[2] = 99
Q:diffusetopedge()
"#,
    )
    .unwrap();
    assert_eq!(
        scene_destination_vertices(&scene, actor).unwrap()[0],
        [0.0, 1.0, 0.0, 0.5]
    );

    // Measure the production mutation path, without the optional observer's owned samples.
    // Its observation semantics are checked separately in lua_util.
    song::test_support::end_live_update_capture(&scene);
    let args = [
        scene_value(&scene, actor),
        Value::Number(1.0),
        Value::Number(0.75),
        Value::Number(0.5),
        Value::Number(0.25),
    ];
    for queued in [false, true] {
        scene_init_state(&scene, actor).unwrap();
        scene_exec(&scene, "Q:stoptweening():diffuse(0.25,0.5,0.75,1)").unwrap();
        if queued {
            scene_exec(&scene, "Q:linear(1)").unwrap();
        }
        let before = scene_state(&scene, actor).unwrap();
        scene_with_mutations(&scene, actor, |mutation| {
            allocations::assert_no_churn(|| {
                // Includes the first conversion from a uniform diffuse to four corner colors.
                for _ in 0..100 {
                    mutation.vertex_diffuse(&args, 3).unwrap();
                    mutation.vertices([base; 4]).unwrap();
                }
            })
        });
        assert_eq!(
            scene_destination_vertices(&scene, actor).unwrap(),
            [base; 4]
        );
        if queued {
            assert_eq!(scene_state(&scene, actor).unwrap(), before);
        }
    }
}

#[test]
fn live_native_scalars_export_current_and_destination_snapshots() {
    let mut compiled = compile_with_context(
        "native-scalar-snapshots",
        r#"
return Def.Quad { InitCommand=function(self) ROOT=self; self.authored={retained=true} end }
"#,
        |context| context.defer_initial_live_update = true,
    );
    let mut scene = compiled.live_scene.take().unwrap();
    scene.advance(SongLuaLiveFrame::default()).unwrap();
    scene_exec(
        &scene,
        r#"
ROOT:SetSize(8,12):basezoomx(2):basezoomy(3):xy(4,8):z(12):aux(16):zoom(1)
ROOT:linear(1):xy(20,24):z(28):aux(32):zoom(2):zoomx(3):zoomy(4):zoomz(5)
    :skewx(0.25):skewy(-0.5):cropleft(0.125):cropright(0.25):croptop(0.375):cropbottom(0.5)
    :fadeleft(0.5):faderight(0.375):fadetop(0.25):fadebottom(0.125)
assert(ROOT:GetX()==4 and ROOT:GetDestX()==20 and ROOT:getaux()==16)
assert(ROOT:GetZoom()==3 and ROOT:GetZoomX()==3 and ROOT:GetZoomY()==4 and ROOT:GetZoomZ()==5)
assert(ROOT:GetZoomedWidth()==48 and ROOT:GetZoomedHeight()==144)
"#,
    )
    .unwrap();
    scene
        .advance(SongLuaLiveFrame {
            delta_seconds: 0.25,
            screen_seconds: 0.25,
            music_seconds: 0.25,
            ..Default::default()
        })
        .unwrap();
    let actor: Root = scene_global_table(&scene, "ROOT").unwrap();
    let current = scene_state(&scene, actor).unwrap();
    assert_eq!(
        (current.x, current.y, current.z, current.aux),
        (8.0, 12.0, 16.0, 20.0)
    );
    let snapshot = scene_snapshot(&scene, actor, true).unwrap();
    let semantic = scene_snapshot(&scene, actor, false).unwrap();
    let copied = scene_new_table(&scene).unwrap();
    scene_copy_pose(&scene, actor, copied).unwrap();
    let restored = scene_new_table(&scene).unwrap();
    scene_restore_snapshot(&scene, restored, snapshot).unwrap();
    let restored_semantic = scene_new_table(&scene).unwrap();
    scene_restore_snapshot(
        &scene,
        restored_semantic,
        scene_snapshot(&scene, semantic, false).unwrap(),
    )
    .unwrap();
    for snapshot in [&copied, &restored, &restored_semantic] {
        let dest = scene_initial_state(&scene, *snapshot).unwrap();
        assert_eq!((dest.x, dest.y, dest.z, dest.aux), (20.0, 24.0, 28.0, 32.0));
        assert_eq!((dest.zoom_x, dest.zoom_y, dest.zoom_z), (3.0, 4.0, 5.0));
        assert_eq!((dest.skew_x, dest.skew_y), (0.25, -0.5));
        assert_eq!(
            [dest.cropleft, dest.cropright, dest.croptop, dest.cropbottom],
            [0.125, 0.25, 0.375, 0.5]
        );
        assert_eq!(
            [dest.fadeleft, dest.faderight, dest.fadetop, dest.fadebottom],
            [0.5, 0.375, 0.25, 0.125]
        );
        for (key, expected) in [
            ("__songlua_current_x", 8.0),
            ("__songlua_current_y", 12.0),
            ("__songlua_current_z", 16.0),
            ("__songlua_aux", 20.0),
        ] {
            assert_eq!(
                scene_field_number(&scene, *snapshot, key, true)
                    .map(|value| value as f32)
                    .unwrap(),
                expected
            );
        }
    }
    scene_exec(
        &scene,
        r#"
ROOT:stoptweening():addx(3):addy(4):addz(5)
assert(ROOT:GetX()==11 and ROOT:GetDestX()==11 and ROOT:GetY()==16 and ROOT:GetZ()==21)
ROOT:linear(1):zoom(6):x(40):finishtweening()
assert(ROOT:GetZoom()==6 and ROOT:GetZoomX()==6 and ROOT:GetZoomY()==6 and ROOT:GetZoomZ()==6)
assert(ROOT:GetX()==40 and ROOT.authored.retained)
"#,
    )
    .unwrap();
    assert_eq!(scene_initial_state(&scene, restored).unwrap().x, 20.0);
    assert_eq!(current.x, 8.0, "previous draw pose remains independent");
    scene_with_mutations(&scene, actor, |mutation| {
        allocations::assert_no_churn(|| {
            for i in 0..100 {
                for key in [
                    "x",
                    "y",
                    "z",
                    "aux",
                    "zoom",
                    "zoom_x",
                    "zoom_y",
                    "zoom_z",
                    "skew_x",
                    "skew_y",
                    "cropleft",
                    "cropright",
                    "croptop",
                    "cropbottom",
                    "fadeleft",
                    "faderight",
                    "fadetop",
                    "fadebottom",
                ] {
                    mutation.scalar(key, i as f32).unwrap();
                }
            }
        })
    });
}

#[expect(
    dead_code,
    reason = "this fixture uses a subset of the shared snapshot API"
)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
fn scene_actor_roots(scene: &SongLuaLiveScene) -> Result<Vec<Root>, Error> {
    let (_, actors) = song::test_support::live_scene_actor_handles(scene)?;
    let mut runtime = scene.runtime();
    actors
        .into_iter()
        .map(|value| runtime.vm_mut().pin(value))
        .collect()
}
fn scene_has_motion(scene: &SongLuaLiveScene, root: Root) -> bool {
    let mut runtime = scene.runtime();
    let value = runtime.vm_mut().rooted(root).unwrap();
    song::test_support::live_actor_has_motion(&runtime, value)
}

#[test]
fn live_native_base_transforms_preserve_getters_draws_and_reset() {
    let source = r#"
local tick = 0
return Def.ActorFrame {
    InitCommand=function(self) ROOT=self; self.authored={retained=true} end,
    OnCommand=function(self)
        SCREENMAN:GetTopScreen():AddInputCallback(function()
            Q:basezoom(2):basezoomx(1):halign(0):valign(1)
            assert(Q:GetBaseZoomX()==1 and Q:GetBaseZoomY()==2 and Q:GetBaseZoomZ()==2)
        end)
        self:SetUpdateFunction(function()
            tick=tick+1
            if tick==1 then Q:linear(2):zoom(2):x(100) end
            if tick==3 then Q:stoptweening() end
            if tick==4 then Q:linear(1):zoom(3):finishtweening() end
            local v=tick/8
            Q:basezoom(v):basezoomx(1):basezoomy(v+1):basezoomz(-v):align(v,1-v)
            assert(Q:GetBaseZoomX()==1 and Q:GetBaseZoomY()==v+1 and Q:GetBaseZoomZ()==-v)
            assert(Q:GetHAlign()==v and Q:GetVAlign()==1-v)
            assert(Q:GetZoomedWidth()==8*Q:GetZoomX())
            assert(Q:GetZoomedHeight()==12*((v+1)*Q:GetZoomY()))
        end)
        self:SetDrawFunction(function()
            Q:basezoomx(2):halign(0):Draw()
            Q:basezoomx(3):halign(1):Draw()
        end)
    end,
    Def.Quad { InitCommand=function(self) Q=self; self:SetSize(8,12) end },
}
"#;
    let build = || {
        compile_with_context("native-base-transform", source, |context| {
            context.defer_initial_live_update = true
        })
        .live_scene
        .take()
        .unwrap()
    };
    let mut live = build();

    let registry = |scene: &SongLuaLiveScene| scene_registry(scene).unwrap();

    let q: Root = scene_global_table(&live, "Q").unwrap();
    for index in 0..6 {
        if index == 2 {
            let input = song::SongLuaInputEvent {
                player: Some(0),
                controller: Some(0),
                button: "Right",
                game_button: "MenuRight",
                kind: song::SongLuaInputType::FirstPress,
                device: "InputDevice_Key",
                device_button: "DeviceButton_right",
                level: 1.0,
                z: 0,
                down: true,
                ago: 0.0,
                is_joystick: false,
                is_mouse: false,
            };
            assert!(live.input(&input).unwrap().1.errors.is_empty());

            reference_test::assert_debug(
                "song/unit/live.rs::live_native_base_transforms_preserve_getters_draws_and_reset::assert_0",
                &(live.states()),
            );
        }
        let frame = SongLuaLiveFrame {
            screen_seconds: index as f64 / 16.0,
            music_seconds: index as f64 / 16.0,
            beat: index as f64 / 8.0,
            delta_seconds: 0.0625,
            ..Default::default()
        };
        let actual = live.advance(frame.clone()).unwrap();

        assert!(actual.errors.is_empty(), "{:?}", actual.errors);

        reference_test::assert_debug(
            "song/unit/live.rs::live_native_base_transforms_preserve_getters_draws_and_reset::assert_1",
            &(live.states()),
        );
        reference_test::assert_debug(
            "song/unit/live.rs::live_native_base_transforms_preserve_getters_draws_and_reset::assert_2",
            &(actual.draw_frames),
        );
        for actor in registry(&live) {
            for field in song::test_support::base_transform_state_keys() {
                assert!(
                    (scene_field_value(&live, actor, field, true).unwrap() == Value::Nil),
                    "native base transforms must not maintain Lua mirrors"
                );
            }
        }
    }
    // Scaling to a rectangle consumes alignment through the same getter as Lua.
    for scene in [&live] {
        scene_exec(
            &scene,
            "Q:stoptweening():align(0.25,0.75):scaletofit(10,20,110,120)",
        )
        .unwrap();
    }

    reference_test::assert_debug(
        "song/unit/live.rs::live_native_base_transforms_preserve_getters_draws_and_reset::assert_3",
        &(scene_state(&live, q).unwrap()),
    );
    let snapshot = scene_snapshot(&live, q, true).unwrap();
    let exported = scene_new_table(&live).unwrap();
    scene_restore_snapshot(&live, exported, snapshot).unwrap();
    let saved = scene_initial_state(&live, exported).unwrap();
    assert_eq!(saved.halign, 0.25);
    assert_eq!(saved.valign, 0.75);
    for scene in [&live] {
        let root = scene_global_table(scene, "ROOT").unwrap();
        scene_init_state(scene, root).unwrap();
        scene_exec(&scene, "assert(ROOT.authored.retained)").unwrap();
    }
    let reset = scene_state(&live, q).unwrap();
    reference_test::assert_debug(
        "song/unit/live.rs::live_native_base_transforms_preserve_getters_draws_and_reset::assert_6",
        &(reset),
    );
    assert_eq!(
        (
            reset.basezoom,
            reset.basezoom_x,
            reset.basezoom_y,
            reset.basezoom_z
        ),
        (1.0, 1.0, 1.0, 1.0)
    );
    assert_eq!((reset.halign, reset.valign), (0.5, 0.5));
    assert_eq!(scene_initial_state(&live, exported).unwrap(), saved);
    scene_with_mutations(&live, q, |mutation| {
        allocations::assert_no_churn(|| {
            for i in 0..100 {
                mutation.scalar("basezoom_x", i as f32).unwrap();
                mutation.scalar("halign", i as f32 / 100.0).unwrap();
                mutation.halign().unwrap();
            }
        })
    });
}

#[test]
fn live_native_render_flags_preserve_recursive_writes_draws_and_reset() {
    let source = r#"
local tick = 0
return Def.ActorFrame {
    InitCommand=function(self) ROOT=self; self.authored={retained=true} end,
    OnCommand=function(self)
        SCREENMAN:GetTopScreen():AddInputCallback(function()
            ROOT:zbuffer(false)
            Q:ztestmode('WriteOnFail'):zwrite(true):texturewrapping(true)
        end)
        self:SetUpdateFunction(function()
            tick = tick + 1
            ROOT:SetDrawByZPosition(tick % 2 == 0):zbuffer(tick % 3 == 0)
            Q:ztestmode('WriteOnFail'):zwrite(false)
                :SetTextureFiltering(tick % 2 == 0):texturewrapping(tick % 2 ~= 0)
                :MaskSource():MaskDest()
            if tick == 1 then Q:linear(2):x(100) end
            if tick == 3 then Q:stoptweening() end
            if tick == 4 then Q:linear(1):x(200):finishtweening() end
        end)
        self:SetDrawFunction(function()
            Q:ztest(false):SetTextureFiltering(false):Draw()
            Q:ztestmode('WriteOnFail'):SetTextureFiltering(true):Draw()
        end)
    end,
    Def.Quad { InitCommand=function(self) Q=self; self:zoomto(16,16) end },
    Def.ActorFrame {
        InitCommand=function(self) CHILD=self end,
        Def.Quad { InitCommand=function(self) GRANDCHILD=self end },
    },
}
"#;
    let build = || {
        compile_with_context("native-render-flags", source, |context| {
            context.defer_initial_live_update = true
        })
        .live_scene
        .take()
        .unwrap()
    };
    let mut live = build();

    let registry = |scene: &SongLuaLiveScene| scene_registry(scene).unwrap();

    let q: Root = scene_global_table(&live, "Q").unwrap();
    for frame in 0..6 {
        if frame == 2 {
            let input = song::SongLuaInputEvent {
                player: Some(0),
                controller: Some(0),
                button: "Right",
                game_button: "MenuRight",
                kind: song::SongLuaInputType::FirstPress,
                device: "InputDevice_Key",
                device_button: "DeviceButton_right",
                level: 1.0,
                z: 0,
                down: true,
                ago: 0.0,
                is_joystick: false,
                is_mouse: false,
            };
            assert!(live.input(&input).unwrap().1.errors.is_empty());

            reference_test::assert_debug(
                "song/unit/live.rs::live_native_render_flags_preserve_recursive_writes_draws_and_reset::assert_0",
                &(live.states()),
            );
            let state = scene_state(&live, q).unwrap();
            assert!(state.depth_test && state.depth_test_on_fail && state.depth_write);
        }
        let frame = SongLuaLiveFrame {
            screen_seconds: frame as f64 / 16.0,
            music_seconds: frame as f64 / 16.0,
            beat: frame as f64 / 8.0,
            delta_seconds: 0.0625,
            ..Default::default()
        };
        let output = live.advance(frame.clone()).unwrap();

        assert!(output.errors.is_empty(), "{:?}", output.errors);

        reference_test::assert_debug(
            "song/unit/live.rs::live_native_render_flags_preserve_recursive_writes_draws_and_reset::assert_1",
            &(live.states()),
        );
        reference_test::assert_debug(
            "song/unit/live.rs::live_native_render_flags_preserve_recursive_writes_draws_and_reset::assert_2",
            &(output.draw_frames),
        );
        let state = scene_state(&live, q).unwrap();
        assert!(state.depth_test && state.depth_test_on_fail && !state.depth_write);
        assert!(state.mask_source && state.mask_dest && state.texture_filtering);
        for actor in registry(&live) {
            for flag in song::test_support::render_flag_state_keys() {
                assert!(
                    (scene_field_value(&live, actor, flag, true).unwrap() == Value::Nil),
                    "native render flags must not maintain Lua mirrors"
                );
            }
        }
    }
    let copied = scene_new_table(&live).unwrap();
    scene_copy_pose(&live, q, copied).unwrap();
    let saved = scene_initial_state(&live, copied).unwrap();
    let snapshot = scene_snapshot(&live, q, true).unwrap();
    let restored = scene_new_table(&live).unwrap();
    scene_restore_snapshot(&live, restored, snapshot).unwrap();
    assert_eq!(
        scene_initial_state(&live, restored)
            .unwrap()
            .depth_test_on_fail,
        saved.depth_test_on_fail
    );
    // Reset both representations and compare recursive child state. Draw
    // policy and masks survive; depth and texture settings return to defaults.
    for scene in [&live] {
        let root = scene_global_table(scene, "ROOT").unwrap();
        scene_init_state(scene, root).unwrap();
        scene_exec(&scene, "assert(ROOT.authored.retained)").unwrap();
    }
    for name in ["ROOT", "Q", "CHILD", "GRANDCHILD"] {
        let actor: Root = scene_global_table(&live, name).unwrap();

        reference_test::assert_debug(
            "song/unit/live.rs::live_native_render_flags_preserve_recursive_writes_draws_and_reset::assert_4",
            &(scene_state(&live, actor).unwrap()),
        );
    }
    let reset = scene_state(&live, q).unwrap();
    assert!(!reset.depth_test && !reset.depth_write && !reset.depth_test_on_fail);
    assert!(
        reset.texture_filtering && !reset.texture_wrapping && reset.mask_source && reset.mask_dest
    );
    assert_eq!(
        scene_initial_state(&live, copied).unwrap(),
        saved,
        "earlier state exports stay independent of InitState"
    );
    scene_with_mutations(&live, q, |mutation| {
        allocations::assert_no_churn(|| {
            for i in 0..100 {
                mutation.flag("texture_filtering", i % 2 == 0).unwrap();
                mutation.flag("depth_test", i % 2 == 0).unwrap();
            }
        })
    });
}

#[test]
fn live_effect_clocks_preserve_effects_getters_and_update_order() {
    let source = r#"
local actors, tick = {}, 0
local root = Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateRate(0.5)
        self:SetUpdateFunction(function()
            tick = tick + 1
            for i, actor in ipairs(actors) do
                if tick == 2 then actor:bob():effectperiod(0.5) end
                if tick == 4 then actor:pulse() end
                if tick == 6 then actor:hibernate(0.125) end
                if tick == 8 then actor:diffuseblink():effectclock('beat') end
                if tick == 10 then actor:diffuseshift():effectclock('music') end
                if tick == 12 then actor:bounce():effectclock('timer') end
                if tick == 14 then actor:spin():effectmagnitude(20,30,40):linear(0.25):rotationz(45) end
                if tick == 16 then actor:stopeffect():finishtweening() end
                if tick == 18 then actor:bob():effectperiod(0.75) end
                if tick == 20 then actor:bob():effectperiod(0.5) end
                actor:x(actor:GetSecsIntoEffect()):y(actor:GetEffectDelta())
            end
        end)
        self:SetDrawFunction(function()
            local values = {}
            for i, actor in ipairs(actors) do
                values[#values+1] = string.format('%.9g:%.9g',
                    actor:GetSecsIntoEffect(), actor:GetEffectDelta())
                actor:Draw()
            end
            OBSERVED = table.concat(values, ',')
        end)
    end,
}
for i = 1, 3 do
    root[#root+1] = Def.Quad {
        InitCommand = function(self) actors[i] = self end,
        OnCommand = function(self)
            self:zoomto(16,16):effectclock(({'timer','music','beat'})[i])
        end,
    }
end
return root
"#;
    let build = || {
        compile_with_context("live-effect-clocks", source, |context| {
            context.defer_initial_live_update = true
        })
        .live_scene
        .take()
        .unwrap()
    };
    let mut native = build();

    let registry = |scene: &SongLuaLiveScene| scene_registry(scene).unwrap();

    for frame in 0..24 {
        let seconds = f64::from(frame) / 8.0;
        let input = SongLuaLiveFrame {
            screen_seconds: seconds,
            music_seconds: if frame == 9 { seconds - 0.5 } else { seconds },
            beat: seconds * 3.0,
            delta_seconds: match frame {
                0 | 5 => 0.0,
                7 => 2.5,
                _ => 0.125,
            },
            ..Default::default()
        };
        let after = native.advance(input.clone()).unwrap();

        assert!(after.errors.is_empty(), "{:?}", after.errors);

        reference_test::assert_debug(
            "song/unit/live.rs::live_effect_clocks_preserve_effects_getters_and_update_order::assert_0",
            &(after.draw_frames),
        );
        reference_test::assert_debug(
            "song/unit/live.rs::live_effect_clocks_preserve_effects_getters_and_update_order::assert_1",
            &(native.states()),
        );
        reference_test::assert_debug(
            "song/unit/live.rs::live_effect_clocks_preserve_effects_getters_and_update_order::assert_2",
            &(scene_global_string(&native, "OBSERVED").unwrap()),
        );
    }
    let actors = registry(&native);
    let _key = "__songlua_state_motion_clock";
    assert!(actors.iter().any(|actor| scene_has_motion(&native, *actor)));
    for actor in actors {
        scene_init_state(&native, actor).unwrap();
        assert!(!scene_has_motion(&native, actor));
    }
}

#[test]
fn live_native_queues_preserve_behavior_without_capture_history() {
    let source = r#"
local tick = 0
return Def.ActorFrame {
    OnCommand = function(self)
        SCREENMAN:GetTopScreen():AddInputCallback(function()
            Q:SetWidth(99):shadowlength(7):effectmagnitude(4,5,6)
        end)
        self:SetUpdateFunction(function()
            tick = tick + 1
            Q:visible(tick % 2 == 0):shadowlengthx(tick):shadowlengthy(-tick)
                :zwrite(tick % 2 == 0):SetTextureFiltering(tick % 2 == 0)
                :diffusealpha(0.5):glow(0.1,0.2,0.3,0.4)
                :SetWidth(12+tick):SetHeight(18+tick):shadowcolor(0.1,0.2,0.3,0.4)
                :baserotationx(tick):baserotationy(tick*2):baserotationz(-tick)
                :effectmagnitude(tick,2,3):effectcolor1(0.2,0.4,0.6,0.8)
                :effectcolor2(0.8,0.6,0.4,0.2):effecttiming(0.1,0.2,0.3,0.4,0.5)
                :effect_hold_at_full(0.2):effectperiod(2)
            assert(Q:GetWidth()==12+tick and Q:GetHeight()==18+tick)
            S:customtexturerect(0,0,1,1):setstate(0)
            if tick == 3 then Q:hibernate(0.125):hurrytweening(0.5) end
            if tick == 7 then Q:stoptweening():addx(3):sleep(0):queuecommand('Again') end
            if tick == 11 then Q:finishtweening() end
            if tick == 13 then Q:xy(7,9):addy(2):shadowlength(3) end
            Q:diffusecolor(0.2,0.4,0.6,0.8):diffuseupperleft(0.9,0.7,0.5,0.3)
                :diffuseupperright(0.1,0.2,0.3,0.4):diffuselowerleft(0.2,0.3,0.4,0.5)
                :diffuselowerright(0.3,0.4,0.5,0.6):diffusetopedge(0.4,0.5,0.6,0.7)
                :diffuserightedge(0.5,0.6,0.7,0.8):diffusebottomedge(0.6,0.7,0.8,0.9)
                :diffuseleftedge(0.7,0.8,0.9,1)
            Q:zoom(1+tick/32):zoomy(2+tick/32):zoomz(3+tick/32)
                :y(tick):z(-tick):addz(2):skewx(tick/32):skewy(-tick/32)
                :cropleft(0.125):cropright(0.25):croptop(0.375):cropbottom(0.5)
                :fadeleft(0.5):faderight(0.375):fadetop(0.25):fadebottom(0.125)
            local color, glow = Q:GetDiffuse(), Q:GetGlow()
            OBSERVED = table.concat({Q:GetX(),Q:GetDestX(),Q:GetY(),Q:GetDestY(),
                Q:GetZ(),Q:GetDestZ(),Q:getaux(),Q:GetTweenTimeLeft(),color[1],color[4],glow[2],
                Q:GetZoom(),Q:GetZoomX(),Q:GetZoomY(),Q:GetZoomZ(),
                Q:GetZoomedWidth(),Q:GetZoomedHeight()}, ',')
            color[1], glow[2] = 99, 99
            assert(Q:GetDiffuse()[1] ~= 99 and Q:GetGlow()[2] ~= 99)
        end)
        self:SetDrawFunction(function()
            Q:basezoomx(tick / 10 + 1):SetHeight(40+tick):shadowlengthy(tick+2):glow(1,0,0,0.1)
            Q:Draw()
            Q:SetHeight(80+tick):shadowlengthy(tick+4):glow(0,1,0,0.2):zoomx(2):addx(1)
            Q:Draw()
            D:diffusetopedge(1,0,0,1):diffusebottomedge(0,0,1,0.5):Draw()
            D:diffuseleftedge(0,1,0,0.25):Draw()
        end)
    end,
    Def.Quad {
        InitCommand = function(self) Q=self; self:zoomto(12,18) end,
        OnCommand = function(self) self:linear(0.125):x(8):aux(2):queuecommand('Again') end,
        AgainCommand = function(self)
            self:addx(4):aux(self:getaux()+1):sleep(0.125):queuecommand('Again')
        end,
    },
    Def.Sprite { InitCommand = function(self) S=self end },
    Def.Quad { InitCommand = function(self) D=self; self:zoomto(8,8) end },
}
"#;
    let build = || {
        compile_with_context("live-capture-history", source, |context| {
            context.defer_initial_live_update = true
        })
        .live_scene
        .take()
        .unwrap()
    };
    let mut live = build();

    let registry = |scene: &SongLuaLiveScene| scene_registry(scene).unwrap();
    // Exercise the previous capture-backed setters with identical Lua,
    // queues and frames. Its old frame boundary discarded audit blocks.

    for frame in 0..20 {
        if frame == 5 {
            let input = song::SongLuaInputEvent {
                player: Some(0),
                controller: Some(0),
                button: "Right",
                game_button: "MenuRight",
                kind: song::SongLuaInputType::FirstPress,
                device: "InputDevice_Key",
                device_button: "DeviceButton_right",
                level: 1.0,
                z: 0,
                down: true,
                ago: 0.0,
                is_joystick: false,
                is_mouse: false,
            };
            assert!(live.input(&input).unwrap().1.errors.is_empty());

            reference_test::assert_debug(
                "song/unit/live.rs::live_native_queues_preserve_behavior_without_capture_history::assert_0",
                &(live.states()),
            );
            scene_exec(&live, "assert(Q:GetWidth()==99)").unwrap();
        }
        let seconds = f64::from(frame) / 16.0;
        let input = SongLuaLiveFrame {
            screen_seconds: seconds,
            music_seconds: seconds,
            beat: seconds * 2.0,
            delta_seconds: if frame == 0 { 0.0 } else { 0.0625 },
            ..Default::default()
        };
        let output = live.advance(input.clone()).unwrap();

        assert!(output.errors.is_empty(), "{:?}", output.errors);

        reference_test::assert_debug(
            "song/unit/live.rs::live_native_queues_preserve_behavior_without_capture_history::assert_1",
            &(output.draw_frames),
        );
        reference_test::assert_debug(
            "song/unit/live.rs::live_native_queues_preserve_behavior_without_capture_history::assert_2",
            &(live.states()),
        );
        reference_test::assert_debug(
            "song/unit/live.rs::live_native_queues_preserve_behavior_without_capture_history::assert_3",
            &(live
                .tracked_states()
                .map(|(_, state)| state)
                .collect::<Vec<_>>()),
        );
        reference_test::assert_debug(
            "song/unit/live.rs::live_native_queues_preserve_behavior_without_capture_history::assert_4",
            &(scene_global_string(&live, "OBSERVED").unwrap()),
        );
        for actor in registry(&live) {
            for key in [
                "__songlua_capture_blocks",
                "__songlua_capture_block",
                "__songlua_capture_immediate_block",
            ] {
                assert!(
                    (scene_field_value(&live, actor, key, true).unwrap() == Value::Nil),
                    "live actor retained {key}"
                );
            }
        }
    }
    let reset = |scene: &SongLuaLiveScene| {
        let actor = scene_global_table(scene, "Q").unwrap();
        scene_init_state(scene, actor).unwrap();
        scene_state(scene, actor).unwrap()
    };
    reference_test::assert_debug(
        "song/unit/live.rs::live_native_queues_preserve_behavior_without_capture_history::assert_5",
        &(reset(&live)),
    );
}

#[test]
fn live_native_pose_getters_preserve_queue_and_sibling_order() {
    let mut compiled = compile_with_context(
        "native-pose-getters",
        r#"
return Def.ActorFrame {
    Def.ActorFrame {
        InitCommand = function(self)
            FIRST = self
            self.authored = { retained = true }
            self:SetUpdateFunction(function() BEFORE_SECOND = SECOND:GetX() end)
        end,
        LaterCommand = function(self)
            LATER = {self:GetX(), self:getaux(), self:GetDestX()}
            self:linear(0.25):x(24):aux(12)
        end,
    },
    Def.ActorFrame {
        InitCommand = function(self)
            SECOND = self
            self:SetUpdateFunction(function() AFTER_FIRST = FIRST:GetX() end)
        end,
    },
    Def.Quad {
        OnCommand = function(self)
            self:stretchto(10,20,110,220):linear(2):addx(128):addy(64)
        end,
    },
}
"#,
        |context| context.defer_initial_live_update = true,
    );
    let mut scene = compiled.live_scene.take().unwrap();
    scene.advance(SongLuaLiveFrame::default()).unwrap();
    scene_exec(
        &scene,
        r#"
FIRST:stoptweening():x(0):aux(0):zoom(1)
FIRST:linear(0.5):x(8):aux(4):zoom(2):queuecommand('Later'):linear(0.5):x(16):aux(8):zoom(3)
FIRST:y(10):z(20):rotationx(30):rotationy(40):rotationz(50):skewx(0.1):skewy(0.2)
FIRST:cropleft(0.1):cropright(0.2):croptop(0.3):cropbottom(0.4)
FIRST:fadeleft(0.2):faderight(0.3):fadetop(0.4):fadebottom(0.5)
FIRST:diffuse(0.1,0.2,0.3,0.4):diffuseupperleft(0.5,0.6,0.7,0.8):glow(0.2,0.3,0.4,0.5)
SECOND:stoptweening():x(100):linear(1):x(108)
"#,
    )
    .unwrap();
    let mut time = 0.0;
    for (delta, assertion) in [
        (
            0.25,
            "assert(FIRST:GetX()==4 and FIRST:getaux()==2 and FIRST:GetDestX()==16); assert(FIRST:GetZoom()==3); assert(BEFORE_SECOND==100 and AFTER_FIRST==4)",
        ),
        (
            0.25,
            "assert(FIRST:GetX()==8 and FIRST:getaux()==4 and LATER==nil)",
        ),
        (0.0, "assert(FIRST:GetX()==8 and LATER==nil)"),
        (
            0.125,
            "assert(LATER[1]==8 and LATER[2]==4 and LATER[3]==16); assert(FIRST:GetX()==10 and FIRST:getaux()==5 and FIRST:GetDestX()==24)",
        ),
    ] {
        time += delta;
        let output = scene
            .advance(SongLuaLiveFrame {
                screen_seconds: time,
                music_seconds: time,
                beat: time * 2.0,
                delta_seconds: delta,
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        scene_exec(&scene, assertion).unwrap();
        for actor in scene_actor_roots(&scene).unwrap() {
            // Export a table-backed destination for the old decoder. Live
            // setters no longer maintain these private mirror fields.

            reference_test::assert_debug(
                "song/unit/live.rs::live_native_pose_getters_preserve_queue_and_sibling_order::assert_0",
                &(scene_state(&scene, actor).unwrap()),
            );
        }
    }
    scene_exec(
        &scene,
        r#"
FIRST:stoptweening():addx(3):aux(42)
assert(FIRST:GetX()==13 and FIRST:GetDestX()==13 and FIRST:getaux()==42)
FIRST:linear(1):x(29):aux(50):finishtweening()
assert(FIRST:GetX()==29 and FIRST:GetDestX()==29 and FIRST:getaux()==50)
assert(FIRST.authored.retained)
"#,
    )
    .unwrap();
}
