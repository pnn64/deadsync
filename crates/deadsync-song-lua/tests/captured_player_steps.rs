use deadsync_rules::timing::{TimingData, TimingSegments};
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
    let compiled = compile_song_lua::<(), (), _>(
        &path,
        &context,
        Default::default(),
        |_, _, _| Ok(Default::default()),
        |_| None,
        |_, _| None,
    )
    .unwrap();
    assert_eq!(
        compiled.info.unsupported_perframes, 0,
        "callback failures: {:?}",
        compiled.info.unsupported_perframe_captures
    );
    assert!(compiled.info.unsupported_perframe_captures.is_empty());
    compiled
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

#[test]
fn tweened_actor_control_separates_both_players() {
    // Dyscontrolled Galaxy retains this actor only in a local closure. Its
    // default pose does not produce a rendered overlay, and the tween begins
    // long after startup. Native Actor::GetX still reads the advancing pose.
    assert_tweened_actor_control("", "", ".25");
}

#[test]
fn retained_and_startup_actor_controls_keep_current_getters() {
    assert_tweened_actor_control("Name='Control',", "", ".25");
    assert_tweened_actor_control("", "self:x(1)", ".25");
    assert_tweened_actor_control("", "", "-.1");
    assert_tweened_actor_control("", "self:x(0)", ".25");
}

fn assert_tweened_actor_control(name: &str, init: &str, start: &str) {
    let source = r#"
        local amount, started
        return Def.ActorFrame {
            Def.Actor { CONTROL_NAME InitCommand=function(self) amount=self CONTROL_INIT end },
            Def.Actor {
                OnCommand=function(self) self:queuecommand('Tick') end,
                TickCommand=function(self)
                    for pn=1,2 do
                        SCREENMAN:GetTopScreen():GetChild('PlayerP'..pn)
                            :x(320+(pn*2-3)*amount:GetX())
                    end
                    if not started and GAMESTATE:GetSongBeat() > CONTROL_START then
                        started=true
                        amount:linear(.5):x(180)
                    end
                    self:sleep(.02):queuecommand('Tick')
                end,
            },
        }
        "#
    .replace("CONTROL_NAME", name)
    .replace("CONTROL_INIT", init)
    .replace("CONTROL_START", start);
    let compiled = compile(&source);
    assert!(
        compiled.live_scene.is_none(),
        "exercise ordinary baked output"
    );
    for (player, expected) in [(1, 140.), (2, 500.)] {
        let windows = compiled
            .eases
            .iter()
            .filter(|window| {
                window.player == Some(player)
                    && window.target == SongLuaEaseTarget::PlayerX
                    && window.start > 0.8
                    && window.start < 1.2
            })
            .collect::<Vec<_>>();
        assert!(!windows.is_empty(), "missing P{player} position samples");
        for window in windows {
            assert!(
                (window.from - expected).abs() < 0.001,
                "P{player} at {}: expected {expected}, got {}",
                window.start,
                window.from
            );
        }
    }
}

#[test]
fn unrendered_actor_control_keeps_native_stop_finish_and_immediate_getters() {
    let compiled = compile(
        r#"
        local amount, stage = nil, 0
        return Def.ActorFrame {
            Def.Actor { InitCommand=function(self) amount=self end },
            Def.ActorFrame {
                OnCommand=function(self)
                    self:SetUpdateFunction(function()
                        local beat=GAMESTATE:GetSongBeat()
                        if stage==0 and beat>.25 then
                            amount:linear(1):x(180)
                            assert(amount:GetX()==0, 'a queued destination is not the current position')
                            assert(amount:GetDestX()==180)
                            stage=1
                        elseif stage==1 and beat>.5 then
                            local current=amount:GetX()
                            assert(current>30 and current<60, 'the control tween must advance')
                            amount:stoptweening()
                            assert(amount:GetX()==current)
                            assert(amount:GetDestX()==current)
                            amount:x(90)
                            assert(amount:GetX()==90, 'an immediate setter updates the current position')
                            amount:linear(1):x(180):finishtweening()
                            assert(amount:GetX()==180)
                            assert(amount:GetDestX()==180)
                            stage=2
                        elseif stage==2 then
                            assert(amount:GetX()==180, 'the completed pose must persist')
                            SCREENMAN:GetTopScreen():GetChild('PlayerP1'):x(amount:GetX())
                        end
                    end)
                end,
            },
        }
        "#,
    );
    assert!(
        compiled.eases.iter().any(|window| {
            window.player == Some(1)
                && window.target == SongLuaEaseTarget::PlayerX
                && window.start > 0.8
                && window.from == 180.
                && window.to == 180.
        }),
        "the final callback stage must publish the finished control pose"
    );
}

#[test]
fn completed_sampled_player_tween_keeps_later_xy_setters_as_steps() {
    // Start the Player tween after preparation so it uses sampled fallback
    // ownership. Its completed queue must not turn later setters into motion.
    let compiled = compile(
        r#"
        local player, started, sign = nil, false, 1
        return Def.ActorFrame {
            OnCommand=function(self)
                player=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
                self:queuecommand('Tick')
            end,
            TickCommand=function(self)
                local beat=GAMESTATE:GetSongBeat()
                if not started and beat>.1 then
                    started=true
                    player:linear(.2):z(120)
                end
                if beat>.5 then
                    sign=-sign
                    player:x(320+180*sign):y(240+60*sign)
                end
                self:sleep(.02):queuecommand('Tick')
            end,
        }
        "#,
    );
    assert!(
        compiled.live_scene.is_none(),
        "exercise ordinary baked output"
    );
    for (target, expected) in [
        (SongLuaEaseTarget::PlayerX, [140., 500.]),
        (SongLuaEaseTarget::PlayerY, [180., 300.]),
    ] {
        let windows = compiled
            .eases
            .iter()
            .filter(|window| {
                window.player == Some(1)
                    && window.target == target
                    && window.start > 0.6
                    && window.start < 1.2
            })
            .collect::<Vec<_>>();
        assert!(windows.len() > 20, "missing late {target:?} samples");
        for value in expected {
            assert!(
                windows.iter().any(|window| window.from == value),
                "late {target:?} setters must retain both authored positions {expected:?}"
            );
        }
        for window in windows {
            assert!(expected.contains(&window.from));
            assert_eq!(
                window.from, window.to,
                "a completed tween must not interpolate the later {target:?} setter at {}",
                window.start
            );
        }
    }
}

#[test]
fn sampled_player_tween_accepts_continuous_retarget_of_another_property() {
    // Actor::DestTweenState applies to the entire pose. An X setter changes
    // the destination even when only Z was authored on the pending tween.
    let compiled = compile(
        r#"
        local player, started = nil, false
        return Def.ActorFrame {
            OnCommand=function(self)
                player=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
                self:SetUpdateFunction(function()
                    local beat=GAMESTATE:GetSongBeat()
                    if not started and beat>.1 then
                        started=true
                        player:linear(.8):z(120)
                    end
                    if beat>.3 and beat<.8 then
                        player:x(500+beat*120)
                    end
                end)
            end,
        }
        "#,
    );
    assert!(
        compiled.live_scene.is_none(),
        "exercise ordinary baked output"
    );
    for target in [SongLuaEaseTarget::PlayerX, SongLuaEaseTarget::PlayerZ] {
        let windows = compiled
            .eases
            .iter()
            .filter(|window| {
                window.player == Some(1)
                    && window.target == target
                    && window.start > 0.4
                    && window.start < 0.7
            })
            .collect::<Vec<_>>();
        assert!(
            windows.len() > 10,
            "missing active {target:?} tween samples"
        );
        for window in windows {
            assert!(
                window.from < window.to,
                "the pending whole-pose tween must interpolate {target:?} at {}: {} -> {}",
                window.start,
                window.from,
                window.to
            );
            if target == SongLuaEaseTarget::PlayerX {
                assert!(
                    window.from < 500. + window.start * 120.,
                    "retargeting X must preserve the current interpolated position"
                );
            }
        }
    }
}

fn gameplay_runtime(compiled: &Compiled) -> deadsync_gameplay::GameplayAttackRuntimeState {
    let timing = TimingData::from_segments(
        0.,
        0.,
        &TimingSegments {
            bpms: vec![(0., 60.)],
            ..Default::default()
        },
        &[],
    );
    // Use production window conversion and tail extension: a missing reset
    // otherwise lets an old Player value survive beyond its captured interval.
    let windows = std::array::from_fn(|player| {
        let (windows, unsupported) =
            gameplay::build_song_lua_ease_windows_for_player(compiled, &timing, player, 0., &[]);
        assert_eq!(unsupported, 0);
        windows
    });
    deadsync_gameplay::GameplayAttackRuntimeState::new([Vec::new(), Vec::new()], windows)
}

fn player_at(
    runtime: &mut deadsync_gameplay::GameplayAttackRuntimeState,
    second: f32,
) -> deadsync_gameplay::SongLuaPlayerTransform {
    runtime
        .refresh_player(
            0,
            second,
            1. / 60.,
            deadsync_gameplay::AppearanceEffects::default(),
            deadsync_gameplay::AttackBaseEffects::default,
            deadsync_gameplay::SongLuaPlayerTransform::default(),
        )
        .unwrap_or_default()
}

#[test]
fn immediate_player_reset_publishes_and_sustains_the_baseline() {
    let compiled = compile(
        r#"
        local player, stage = nil, 0
        return Def.ActorFrame {
            OnCommand=function(self)
                player=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
                self:queuecommand('Tick')
            end,
            TickCommand=function(self)
                local beat=GAMESTATE:GetSongBeat()
                if stage==0 and beat>.1 then
                    player:y(1900):zoomz(1.25)
                    stage=1
                elseif stage==1 and beat>.5 then
                    player:y(240):zoomz(1)
                    stage=2
                end
                self:sleep(.02):queuecommand('Tick')
            end,
        }
        "#,
    );
    let reset = compiled
        .eases
        .iter()
        .find(|window| {
            window.player == Some(1)
                && window.target == SongLuaEaseTarget::PlayerY
                && window.start > 0.5
                && window.from == 240.
                && window.to == 240.
        })
        .expect("the callback returning Y to its baseline must have a window");
    assert!(reset.start < 0.55, "the reset must start at its callback");
    for (target, baseline) in [
        (SongLuaEaseTarget::PlayerY, 240.),
        (SongLuaEaseTarget::PlayerZoomZ, 1.),
    ] {
        let resets = compiled
            .eases
            .iter()
            .filter(|window| {
                window.player == Some(1)
                    && window.target == target
                    && window.start > 0.5
                    && window.from == baseline
                    && window.to == baseline
            })
            .collect::<Vec<_>>();
        assert_eq!(resets.len(), 1, "only the first baseline sample is needed");
        assert_eq!(resets[0].start, reset.start);
    }
    let mut runtime = gameplay_runtime(&compiled);
    let before = player_at(&mut runtime, reset.start - 0.001);
    assert_eq!(before.y, Some(1900.));
    assert_eq!(before.zoom_z, 1.25);
    for second in [reset.start + 0.001, 0.9, 2.] {
        let actual = player_at(&mut runtime, second);
        assert_eq!(actual.y, Some(240.), "Y at {second}");
        assert_eq!(actual.zoom_z, 1., "Z scale at {second}");
    }
}

#[test]
fn helper_driven_player_zoom_pulse_returns_exactly_to_baseline() {
    let compiled = compile(
        r#"
        local amount, started
        return Def.ActorFrame {
            Def.Actor { InitCommand=function(self) amount=self self:z(1) end },
            Def.Actor {
                OnCommand=function(self) self:queuecommand('Tick') end,
                TickCommand=function(self)
                    SCREENMAN:GetTopScreen():GetChild('PlayerP1'):zoomz(amount:GetZ())
                    if not started and GAMESTATE:GetSongBeat()>.1 then
                        started=true
                        amount:linear(.15):z(5):linear(.25):z(1)
                    end
                    self:sleep(.02):queuecommand('Tick')
                end,
            },
        }
        "#,
    );
    let windows = compiled
        .eases
        .iter()
        .filter(|window| {
            window.player == Some(1) && window.target == SongLuaEaseTarget::PlayerZoomZ
        })
        .collect::<Vec<_>>();
    assert!(windows.iter().any(|window| window.from > 1.1));
    let final_window = windows
        .last()
        .expect("the pulse must publish Player Z scale");
    assert_eq!(
        final_window.from, 1.,
        "retain the exact reset, not its preceding sample"
    );
    assert_eq!(final_window.to, 1.);
    assert!(final_window.start > 0.4 && final_window.start < 0.65);
    for window in windows {
        assert_eq!(window.from, window.to, "Player setters remain steps");
    }
    let mut runtime = gameplay_runtime(&compiled);
    assert!(player_at(&mut runtime, 0.3).zoom_z > 1.1);
    for second in [0.75, 1.1, 2.] {
        assert_eq!(
            player_at(&mut runtime, second).zoom_z,
            1.,
            "Z scale at {second}"
        );
    }
}

#[test]
fn player_reset_on_the_final_sample_keeps_a_persistent_baseline() {
    let compiled = compile(
        r#"
        return Def.ActorFrame {
            OnCommand=function(self)
                local player=SCREENMAN:GetTopScreen():GetChild('PlayerP1')
                self:SetUpdateFunction(function()
                    local beat=GAMESTATE:GetSongBeat()
                    if beat>=1.25 then
                        player:y(240):zoomz(1)
                    elseif beat>.1 then
                        player:y(1900):zoomz(1.25)
                    end
                end)
            end,
        }
        "#,
    );
    for (target, baseline) in [
        (SongLuaEaseTarget::PlayerY, 240.),
        (SongLuaEaseTarget::PlayerZoomZ, 1.),
    ] {
        let last = compiled
            .eases
            .iter()
            .rfind(|window| window.player == Some(1) && window.target == target)
            .expect("the final reset must retain a nonempty interval");
        assert_eq!(last.start, 1.25);
        assert!(last.limit > 0.);
        assert_eq!(last.from, baseline);
        assert_eq!(last.to, baseline);
    }
    let mut runtime = gameplay_runtime(&compiled);
    let before = player_at(&mut runtime, 1.249);
    assert_eq!(before.y, Some(1900.));
    assert_eq!(before.zoom_z, 1.25);
    for second in [1.251, 2.] {
        let actual = player_at(&mut runtime, second);
        assert_eq!(actual.y, Some(240.));
        assert_eq!(actual.zoom_z, 1.);
    }
}

#[test]
fn untouched_player_baselines_do_not_publish_transform_windows() {
    let compiled = compile(
        r#"
        return Def.ActorFrame {
            OnCommand=function(self) self:SetUpdateFunction(function() end) end,
        }
        "#,
    );
    assert!(compiled.eases.iter().all(|window| matches!(
        window.target,
        SongLuaEaseTarget::Mod(_) | SongLuaEaseTarget::Function
    )));
}
