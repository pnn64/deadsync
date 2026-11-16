use deadlib_lua::compile_named;
use deadsync_song_lua::{
    SongLuaCompileContext, SongLuaDifficulty, SongLuaLiveFrame, SongLuaLivePlayer, SongLuaStep,
    SongRuntime, test_support,
};
use std::path::Path;

fn script(runtime: &mut SongRuntime, source: &str) {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/score.lua").unwrap())
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"));
}

#[test]
fn score_redraws_follow_fed_statistics_without_cancelling_authored_tweens() {
    for enabled in [[true, false], [false, true], [true, true]] {
        for routine in [false, true] {
            if routine && enabled != [true, true] {
                continue;
            }
            let mut context = SongLuaCompileContext::new(Path::new("."), "score redraw");
            context.live = true;
            context.style_name = if routine { "routine" } else { "single" }.into();
            context.screen_width = 854.;
            context.screen_height = 480.;
            context.song_steps.push(SongLuaStep {
                steps_type: if routine {
                    "StepsType_Dance_Routine"
                } else {
                    "StepsType_Dance_Single"
                }
                .into(),
                difficulty: SongLuaDifficulty::Hard,
                description: "Score".into(),
                chart_name: String::new(),
                credit: String::new(),
                meter: 1,
                music_file: "silence.wav".into(),
                possible_grade_points: 10,
            });
            context.player_step_indices = [Some(0); 2];
            for (player, enabled) in context.players.iter_mut().zip(enabled) {
                player.enabled = enabled;
            }
            let mut runtime = SongRuntime::new(&context).unwrap();
            let scene = test_support::gameplay_screen(&mut runtime, &context).unwrap();
            runtime.begin_actor_preparation().unwrap();
            assert!(test_support::gameplay_underlay(&mut runtime, scene).unwrap());
            script(
                &mut runtime,
                r#"
                underlay=SCREENMAN:GetTopScreen():GetChild('Underlay')
                assert(FormatPercentScore(0)==' 0.00%')
                scores={}
                for _,p in ipairs(GAMESTATE:GetHumanPlayers()) do
                    local a=underlay:GetChild(ToEnumShortString(p)..'Score')
                    assert(a:GetParent()==underlay and a:GetText()=='0.00')
                    scores[#scores+1]=a
                    a:playcommand('Judgment'):stoptweening():x(0):linear(2):x(100)
                    for i=1,75 do a:playcommand('Judgment') end
                end
                collectgarbage('collect')
            "#,
            );
            let mut frame = SongLuaLiveFrame::default();
            frame.players[0].possible_points = 10;
            frame.players[0].actual_points = 5;
            frame.players[1].possible_points = 20;
            frame.players[1].actual_points = 5;
            if routine {
                frame.routine = Some(SongLuaLivePlayer {
                    possible_points: 25,
                    actual_points: 8,
                    ..Default::default()
                });
            }
            runtime.feed_stage_statistics(&frame).unwrap();
            runtime.advance_actor_scene(scene, 0.2).unwrap();
            for (index, enabled) in enabled.into_iter().enumerate() {
                if !enabled {
                    continue;
                }
                let expected = if routine {
                    "32.00"
                } else if index == 0 {
                    "50.00"
                } else {
                    "25.00"
                };
                script(
                    &mut runtime,
                    &format!(
                        r#"
                    local a=underlay:GetChild('P{}Score')
                    assert(a:GetText()=='{}', a:GetText())
                    assert(a:GetTweenTimeLeft()>0 and a:GetX()>0 and a:GetX()<100)
                "#,
                        index + 1,
                        expected
                    ),
                );
            }
            script(
                &mut runtime,
                r#"
                for _,a in ipairs(scores) do
                    a:playcommand('Judgment'):stoptweening()
                    for i=1,75 do a:playcommand('Judgment') end
                end
            "#,
            );
            frame.players[0].actual_points = 10;
            frame.players[1].actual_points = 20;
            if let Some(team) = &mut frame.routine {
                team.actual_points = 25;
            }
            runtime.feed_stage_statistics(&frame).unwrap();
            runtime.advance_actor_scene(scene, 0.2).unwrap();
            script(
                &mut runtime,
                "for _,a in ipairs(scores) do assert(a:GetText()=='100.00',a:GetText()) end",
            );
            assert!(runtime.take_actor_diagnostics().is_empty());
        }
    }
}
