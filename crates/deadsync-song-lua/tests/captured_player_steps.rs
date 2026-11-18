use deadsync_song_lua::*;

type Compiled = CompiledSongLua<
    SongLuaOverlayActor<SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>>,
>;

fn compile(source: &str) -> Compiled {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("default.lua");
    std::fs::write(&path, source).unwrap();
    let mut context = SongLuaCompileContext::new(directory.path(), "Captured player steps");
    context.music_length_seconds = 1.25;
    context.song_display_bpms = [60.; 2];
    compile_song_lua::<(), (), _>(
        &path,
        &context,
        Default::default(),
        |_, _, _| Ok(Default::default()),
        |_| None,
        |_, _| None,
    )
    .unwrap()
}

#[test]
fn recurring_player_setters_hold_until_the_next_callback() {
    // Dyscontrolled Galaxy uses this pattern for its post-pad alternating
    // rotation and, later, alternating depth/zoom. Actor::SetRotation and
    // SetZoom modify the current pose immediately when no tween is queued.
    let compiled = compile(
        r#"
        local player, sign, angle = nil, 1, 0
        return Def.ActorFrame {
            OnCommand=function(self)
                player=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
                self:queuecommand('Tick')
            end,
            TickCommand=function(self)
                sign=-sign
                angle=angle+17
                player:rotationz(angle+90*sign):rotationy(35*sign)
                    :z(120*sign):zoom(1+.2*sign)
                self:sleep(.02):queuecommand('Tick')
            end,
        }
        "#,
    );
    assert!(
        compiled.live_scene.is_none(),
        "exercise ordinary baked output"
    );
    for target in [
        SongLuaEaseTarget::PlayerRotationZ,
        SongLuaEaseTarget::PlayerRotationY,
        SongLuaEaseTarget::PlayerZ,
        SongLuaEaseTarget::PlayerZoomX,
        SongLuaEaseTarget::PlayerZoomY,
        SongLuaEaseTarget::PlayerZoomZ,
    ] {
        let windows = compiled
            .eases
            .iter()
            .filter(|window| {
                window.player == Some(1) && window.target == target && window.start > 0.1
            })
            .collect::<Vec<_>>();
        assert!(windows.len() > 20, "missing recurring {target:?} samples");
        assert!(
            windows.windows(2).any(|pair| pair[0].from != pair[1].from),
            "the authored changes must remain visible for {target:?}"
        );
        for window in windows {
            assert_eq!(
                window.from, window.to,
                "an immediate {target:?} setter at {} must not manufacture a turn or move before its callback",
                window.start
            );
        }
    }
}

#[test]
fn native_player_tweens_keep_continuous_motion_between_samples() {
    let compiled = compile(
        r#"
        local started = false
        return Def.ActorFrame {
            OnCommand=function(self)
                self:SetUpdateFunction(function()
                    if not started then
                        started=true
                        SCREENMAN:GetTopScreen():GetChild('PlayerP1')
                            :linear(1):rotationz(360):z(120):zoom(.4)
                    end
                end)
            end,
        }
        "#,
    );
    for target in [
        SongLuaEaseTarget::PlayerRotationZ,
        SongLuaEaseTarget::PlayerZ,
        SongLuaEaseTarget::PlayerZoomX,
    ] {
        assert!(
            compiled.eases.iter().any(|window| {
                window.player == Some(1)
                    && window.target == target
                    && window.start > 0.1
                    && window.start < 0.8
                    && window.from != window.to
            }),
            "native tween interpolation was lost for {target:?}"
        );
    }
}

#[test]
fn player_setters_retargeting_a_pending_tween_keep_continuous_motion() {
    let compiled = compile(
        r#"
        local player
        return Def.ActorFrame {
            OnCommand=function(self)
                player=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
                player:linear(1):rotationz(360)
                self:SetUpdateFunction(function()
                    player:rotationz(180+GAMESTATE:GetSongBeat()*90)
                end)
            end,
        }
        "#,
    );
    assert!(
        compiled.eases.iter().any(|window| {
            window.player == Some(1)
                && window.target == SongLuaEaseTarget::PlayerRotationZ
                && window.start > 0.1
                && window.start < 0.8
                && window.from != window.to
        }),
        "a setter must keep modifying the queued tween destination"
    );
}
