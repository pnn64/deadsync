//! A message can start a polling loop that alternates two commands:
//! `Update` queues `Update2`, which sleeps one tick and queues `Update`.
//! Baked preparation must keep that loop running for as long as Runtime does.
use deadsync_song_lua::*;

type Compiled = CompiledSongLua<
    SongLuaOverlayActor<SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>>,
>;

fn compile(path: &std::path::Path, context: &SongLuaCompileContext) -> Compiled {
    compile_song_lua::<(), (), _>(
        path,
        context,
        Default::default(),
        |_, _, _| Ok(Default::default()),
        |_| None,
        |_, _| None,
    )
    .unwrap()
}

// One actor's loop writes a global each tick. A full-screen quad copies that
// global into its alpha from its own loop, which a broadcast wakes and which
// stops requeuing once the value has rested at zero. FIRST_WAKE is the beat of
// the first broadcast.
const SCRIPT: &str = r#"
local opacity = 0
local wakes = { { beat = FIRST_WAKE, length = 4 }, { beat = 9, length = 1 } }
local current
local rested = -1

local frame = Def.ActorFrame{
   Def.Actor{ OnCommand=function(self) self:sleep(9999) end }
}

frame[#frame+1] = Def.Quad{
   Name='Driver',
   InitCommand=function(self) self:visible(false) end,
   OnCommand=function(self) self:queuecommand('Update') end,
   UpdateCommand=function(self)
      local beat = GAMESTATE:GetSongBeat()
      for _, wake in ipairs(wakes) do
         if not wake.fired and beat >= wake.beat then
            wake.fired = true
            current = wake
            MESSAGEMAN:Broadcast('FlashWakeUp')
         end
      end
      if current then
         local amount = (beat - current.beat) / current.length
         opacity = 1 - math.min(math.max(amount, 0), 1)
      end
      self:queuecommand('Update2')
   end,
   Update2Command=function(self)
      self:sleep(1 / 60)
      self:queuecommand('Update')
   end,
}

frame[#frame+1] = Def.Quad{
   Name='Flash',
   InitCommand=function(self)
      self:stretchto(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT)
      self:diffusealpha(0)
   end,
   UpdateCommand=function(self)
      self:diffusealpha(opacity)
      if opacity > 0 then
         rested = -1
      elseif rested == -1 then
         rested = GAMESTATE:GetSongBeat()
      end
      if rested == -1 or GAMESTATE:GetSongBeat() - rested < 2 then
         self:queuecommand('Update2')
      end
   end,
   Update2Command=function(self)
      self:sleep(1 / 60)
      self:queuecommand('Update')
   end,
   FlashWakeUpMessageCommand=function(self)
      rested = -1
      self:queuecommand('Update')
   end,
}

return frame
"#;

// Whether the loop survived used to depend on how much of the frame was left
// when the broadcast arrived, so wake it at several phases of the tick.
#[test]
fn baked_message_started_two_command_loop_matches_live() {
    for first_wake in ["0.1", "0.5", "1.3"] {
        loop_matches_live(first_wake);
    }
}

fn loop_matches_live(first_wake: &str) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("default.lua");
    std::fs::write(&path, SCRIPT.replace("FIRST_WAKE", first_wake)).unwrap();
    let mut context = SongLuaCompileContext::new(dir.path(), "Message poll loop");
    context.music_length_seconds = 12.;
    context.song_display_bpms = [60.; 2];

    let baked = compile(&path, &context);
    assert_eq!(baked.info, SongLuaCompileInfo::default());
    assert!(baked.live_scene.is_none(), "must test actual baked output");
    let target = baked
        .overlays
        .iter()
        .position(|a| a.name.as_deref() == Some("Flash"))
        .unwrap();
    assert_eq!(baked.overlays[target].initial_state.diffuse[3], 0.);
    let track = baked
        .overlay_updates
        .iter()
        .find(|track| {
            track.overlay_index == target && track.target == SongLuaOverlayUpdateTarget::Diffuse
        })
        .expect("the woken loop must write the quad's alpha");
    let baked_alpha = |seconds: f32| {
        track
            .samples
            .iter()
            .rev()
            .find(|s| s.time <= seconds)
            .map_or(0., |s| match s.value {
                SongLuaOverlayUpdateValue::Vec4(color) => color[3],
                _ => panic!("diffuse sample"),
            })
    };

    context.live = true;
    let mut live = compile(&path, &context);
    let local = live
        .overlays
        .iter()
        .position(|a| a.name.as_deref() == Some("Flash"))
        .unwrap();
    let index = live.live_actor_indices()[local];
    let mut scene = live.live_scene.take().unwrap();
    let mut faded = false;
    let mut second_flash = false;
    for frame in 0_usize..=690 {
        let seconds = frame as f64 / 60.;
        let prior = frame.saturating_sub(1) as f64 / 60.;
        let output = scene
            .update(SongLuaLiveFrame {
                beat: seconds,
                music_seconds: seconds,
                screen_seconds: seconds,
                delta_seconds: f64::from(seconds as f32 - prior as f32),
                ..Default::default()
            })
            .unwrap();
        assert!(output.errors.is_empty(), "{:?}", output.errors);
        let live_alpha = scene.states()[index].diffuse[3];
        let baked_alpha = baked_alpha(seconds as f32);
        // The two clocks may publish one tick apart; a stalled loop is off by
        // most of the range.
        assert!(
            (baked_alpha - live_alpha).abs() < 0.03,
            "wake {first_wake} at {seconds:.3}: baked={baked_alpha}, live={live_alpha}"
        );
        faded |= (6.0..8.0).contains(&seconds) && baked_alpha == 0.;
        second_flash |= (9.0..9.5).contains(&seconds) && baked_alpha > 0.5;
    }
    scene.end().unwrap();
    assert!(
        faded,
        "wake {first_wake}: the first flash must fade out completely"
    );
    assert!(
        second_flash,
        "wake {first_wake}: a later broadcast must restart the stopped loop"
    );
}
