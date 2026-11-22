#![cfg(feature = "test-support")]

use deadlib_lua::{Value, compile_named};
use deadsync_gameplay::{GameplayTurnOption as Turn, turn_column_mapping};
use deadsync_song_lua::{self as song, *};
use std::{fs, path::Path, sync::Arc};

type Kind = SongLuaOverlayKind<(), (), deadlib_present::actors::TextAttribute>;
type Compiled = CompiledSongLua<SongLuaOverlayActor<Kind>>;

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"=multitap turn regression").unwrap())
        .unwrap_or_else(|error| panic!("{source}\n{error:?}"))
        .to_vec()
}

fn set_turns(context: &mut SongLuaCompileContext, turns: [Turn; 2]) {
    for (player, turn) in context.players.iter_mut().zip(turns) {
        player.enabled = true;
        player.difficulty = SongLuaDifficulty::Challenge;
        player.turn_option = turn;
        player.column_take_from = turn_column_mapping(turn, 4, 123);
        player.step_statistics_enabled = false;
        player.hide_lifebar = true;
    }
}

#[test]
fn saved_turns_seed_every_mods_level_and_reset_without_leaking_between_players() {
    let turns = [
        Turn::None,
        Turn::Mirror,
        Turn::Left,
        Turn::Right,
        Turn::LRMirror,
        Turn::UDMirror,
        Turn::Shuffle,
        Turn::Blender,
        Turn::Random,
    ];
    for (index, &turn) in turns.iter().enumerate() {
        let other = turns[(index + 4) % turns.len()];
        let mut context = SongLuaCompileContext::new(Path::new("."), "saved turn options");
        set_turns(&mut context, [turn, other]);
        let mut runtime = SongRuntime::new(&context).unwrap();
        let actual = execute(
            &mut runtime,
            r#"
            TURN_NAMES={'Mirror','Left','Right','LRMirror','UDMirror','Shuffle','SuperShuffle','HyperShuffle'}
            local out={}
            for player=0,1 do
              for _,level in ipairs({'Preferred','Stage','Song','Current'}) do
                local options=GAMESTATE:GetPlayerState(player):GetPlayerOptions('ModsLevel_'..level)
                for _,name in ipairs(TURN_NAMES) do out[#out+1]=options[name](options) end
              end
            end
            return unpack(out)
            "#,
        );
        let flags = |turn| {
            turns[1..]
                .iter()
                .map(move |&candidate| Value::Boolean(candidate == turn))
        };
        let expected: Vec<_> = [turn, other]
            .into_iter()
            .flat_map(|turn| (0..4).flat_map(move |_| flags(turn)))
            .collect();
        assert_eq!(actual, expected, "P1 {turn:?}, P2 {other:?}");

        let actual = execute(
            &mut runtime,
            r#"
            local state=GAMESTATE:GetPlayerState(0)
            state:SetPlayerOptions('ModsLevel_Stage','')
            local out={}
            for _,level in ipairs({'Preferred','Stage','Song','Current'}) do
              local options=state:GetPlayerOptions('ModsLevel_'..level)
              for _,name in ipairs(TURN_NAMES) do out[#out+1]=options[name](options) end
            end
            local other=GAMESTATE:GetPlayerState(1):GetPlayerOptions('ModsLevel_Song')
            for _,name in ipairs(TURN_NAMES) do out[#out+1]=other[name](other) end
            return unpack(out)
            "#,
        );
        let expected: Vec<_> = flags(turn)
            .chain(std::iter::repeat_n(Value::Boolean(false), 24))
            .chain(flags(other))
            .collect();
        assert_eq!(actual, expected, "fresh Stage reset for {turn:?}");
    }
}

fn actual_factory(turns: [Turn; 2], tilt: f32, live: bool) -> (tempfile::TempDir, Compiled) {
    let dir = tempfile::tempdir().unwrap();
    let factory_dir = dir.path().join("multitap");
    fs::create_dir_all(&factory_dir).unwrap();
    fs::write(
        factory_dir.join("Default.lua"),
        include_str!("../../../tests/fixtures/song-lua/multitap-factory.lua"),
    )
    .unwrap();
    fs::write(
        factory_dir.join("multitap_data.lua"),
        // Empty descriptors size the untouched columns' authored zoom splines.
        "multitaps={_version={1,2},Challenge={{lane=2,taps={4,8},peak=6},{lane=1,taps={}},{lane=3,taps={}},{lane=4,taps={}}}}",
    )
    .unwrap();
    let entry = dir.path().join("default.lua");
    fs::write(
        &entry,
        r#"
        NOTESKIN.LoadActorForNoteSkin=function() return Def.Sprite{} end
        local factory=LoadActor('multitap/Default.lua')
        factory.InitCommand=function(self)
          COLUMNS={}
          for pn=1,2 do
            local field=SCREENMAN:GetTopScreen():GetChild('PlayerP'..pn):GetChild('NoteField')
            COLUMNS[pn]=field:GetColumnActors()
            for lane,column in ipairs(COLUMNS[pn]) do column:aux(pn*10+lane) end
          end
        end
        return factory
        "#,
    )
    .unwrap();
    let mut context = SongLuaCompileContext::new(dir.path(), "turned multitap factory");
    context.live = live;
    context.defer_initial_live_update = false;
    context.music_length_seconds = if live { 4.1 } else { 0.1 };
    set_turns(&mut context, turns);
    for player in &mut context.players {
        player.perspective.tilt = tilt;
        player.perspective.skew = 0.0;
    }
    let compiled = song::compile_song_lua(
        &entry,
        &context,
        SongLuaNoteskinResolver::default(),
        |_, _, _| Ok(Arc::<[()]>::from([])),
        |_| None,
        |_, _| None,
    )
    .expect("authored multitap factory");
    assert_eq!(compiled.info.unsupported_perframes, 0);
    (dir, compiled)
}

fn live_actor(compiled: &Compiled, scene: &SongLuaLiveScene, name: &str) -> SongLuaOverlayState {
    let index = compiled
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("missing {name}"));
    scene.states()[compiled.live_indices[index]]
}

fn assert_hidden_destinations(
    hides: &[SongLuaNoteHideWindow],
    turns: [Turn; 2],
    lanes: [usize; 2],
) {
    for (player, &lane) in lanes.iter().enumerate() {
        for column in 0..4 {
            let hidden = hides.iter().any(|window| {
                window.player == player
                    && window.column == column
                    && window.start_beat <= 4.0
                    && window.end_beat >= 4.0
            });
            assert_eq!(
                hidden,
                column + 1 == lane,
                "P{} {:?}: beat-4 native note in column {} must agree with replacement lane {}: {:?}",
                player + 1,
                turns[player],
                column + 1,
                lane,
                hides
            );
        }
    }
}

#[test]
fn compiled_actual_factory_hides_moved_native_notes_without_hiding_the_source_lane() {
    let turns = [Turn::Mirror, Turn::Left];
    let (_dir, compiled) = actual_factory(turns, 0.0, false);
    assert!(compiled.live_scene.is_none());
    // Finite preparation exports the complete chart-timed zoom windows used by
    // the gameplay bridge, even though this short bake stops before the taps.
    assert_hidden_destinations(&compiled.note_hides, turns, [3, 4]);
}

#[test]
fn authored_factory_moves_down_multitaps_and_explosions_for_each_player_and_perspective() {
    // The destination lanes are native chart results, not an inversion derived
    // by the test from the implementation's permutation. Left and Right catch
    // the direction error that a self-inverse Mirror test cannot expose.
    for (turns, lanes, tilt) in [
        ([Turn::None, Turn::None], [2, 2], 0.0),
        ([Turn::Mirror, Turn::Left], [3, 4], 0.0),
        ([Turn::Left, Turn::Right], [4, 1], 0.5),
        ([Turn::Right, Turn::Mirror], [1, 3], -0.5),
    ] {
        let (_dir, mut compiled) = actual_factory(turns, tilt, true);
        let mut scene = compiled.live_scene.take().unwrap();
        let startup = scene.take_startup_output();
        assert!(startup.errors.is_empty(), "{:?}", startup.errors);
        for seconds in [0.0, 0.125] {
            let output = scene
                .advance(SongLuaLiveFrame {
                    beat: 4.0,
                    music_seconds: 4.0,
                    screen_seconds: seconds,
                    delta_seconds: 0.125,
                    ..Default::default()
                })
                .unwrap();
            assert!(output.errors.is_empty(), "{:?}", output.errors);
        }
        // Live preparation retains a VM at activation and leaves the finite
        // output's note_hides empty. Inspect the factory's installed handlers
        // after the real updates that also positioned its replacement arrows.
        let hides = scene.runtime().read_song_note_column_zoom_hides().unwrap();
        assert_hidden_destinations(&hides, turns, lanes);
        for (player, lane) in lanes.into_iter().enumerate() {
            let pn = player + 1;
            let frame = live_actor(&compiled, &scene, &format!("MultitapP{pn}_1"));
            let arrow = live_actor(&compiled, &scene, &format!("MultitapArrowP{pn}_1"));
            assert!(frame.visible, "P{pn} {:?}, tilt {tilt}", turns[player]);
            assert_eq!(frame.x, [-96.0, -32.0, 32.0, 96.0][lane - 1]);
            assert_eq!(
                arrow.base_rotation[2] + arrow.rot_z_deg,
                [90.0, 0.0, 180.0, 270.0][lane - 1],
                "P{pn} {:?}: replacement arrow faces its destination",
                turns[player]
            );
            for column in 1..=4 {
                let explosion = live_actor(
                    &compiled,
                    &scene,
                    &format!("MultitapExplosionP{pn}_{column}"),
                );
                assert_eq!(
                    explosion.visible,
                    column == lane,
                    "P{pn} explosion {column}"
                );
            }
        }
        execute(
            &mut scene.runtime(),
            r#"
            for pn=1,2 do
              local field=SCREENMAN:GetTopScreen():GetChild('PlayerP'..pn):GetChild('NoteField')
              for lane,column in ipairs(field:GetColumnActors()) do
                assert(column==COLUMNS[pn][lane])
                assert(column:getaux()==pn*10+lane)
              end
            end
            "#,
        );
    }
}

fn analytic_source(base_rotations: [f32; 2]) -> String {
    let mut source = String::from(
        "multitaps={Challenge={{lane=2,taps={4,8},peak=6}}};local root=Def.ActorFrame{};",
    );
    for pn in 1..=2 {
        let base_rotation = base_rotations[pn - 1];
        source.push_str(&format!(
            "root[#root+1]=Def.ActorFrame{{Name='MultitapFrameP{pn}'}};\
             root[#root+1]=Def.ActorFrame{{Name='MultitapP{pn}_1',InitCommand=function(self)self:visible(false)end,\
             Def.ActorFrame{{Name='MultitapArrowP{pn}_1',InitCommand=function(self)self:baserotationz({base_rotation})end}},\
             Def.BitmapText{{Name='MultitapTextP{pn}_1',Font='Common Normal',Text='2'}}}};"
        ));
        for lane in 1..=4 {
            source.push_str(&format!(
                "root[#root+1]=Def.ActorFrame{{Name='MultitapExplosionP{pn}_{lane}',InitCommand=function(self)self:visible(false)end}};"
            ));
        }
    }
    source.push_str("root[#root+1]=Def.ActorFrame{Name='Update',InitCommand=function(self)self:SetUpdateFunction(function()end)end};return root");
    source
}

#[test]
fn analytic_multitaps_use_the_inverse_turn_map_for_motion_rotation_and_explosions() {
    for (turns, lanes, base_rotations, mutation) in [
        ([Turn::None, Turn::Mirror], Some([2, 3]), [0.0, 0.0], ""),
        ([Turn::Left, Turn::Right], Some([4, 1]), [0.0, 0.0], ""),
        // A real startup update may already have oriented each false arrow.
        // Analytic preparation must retain that direction without adding it twice.
        ([Turn::Mirror, Turn::Left], Some([3, 4]), [180.0, 270.0], ""),
        // This particular legacy factory does not implement these turns. Keep
        // its authored callback instead of inventing different analytic behavior.
        ([Turn::LRMirror, Turn::None], None, [0.0, 0.0], ""),
        ([Turn::None, Turn::Shuffle], None, [0.0, 0.0], ""),
        // The authored callback reads current Song flags, while native spline
        // ownership keeps the initial Stage map. A later flag edit must therefore
        // use Lua replay instead of baking arrows from the saved permutation.
        (
            [Turn::Mirror, Turn::None],
            None,
            [0.0, 0.0],
            "GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Song'):Mirror(false)",
        ),
        (
            [Turn::None, Turn::None],
            None,
            [0.0, 0.0],
            "GAMESTATE:GetPlayerState(1):GetPlayerOptions('ModsLevel_Song'):Left(true)",
        ),
        (
            [Turn::Mirror, Turn::Right],
            None,
            [0.0, 0.0],
            "GAMESTATE:GetPlayerState(0):SetPlayerOptions('ModsLevel_Stage','')",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let entry = dir.path().join("default.lua");
        fs::write(&entry, analytic_source(base_rotations)).unwrap();
        let mut context = SongLuaCompileContext::new(dir.path(), "analytic multitap turns");
        context.music_length_seconds = 0.1;
        context.background_layer_count = 1;
        set_turns(&mut context, turns);
        let mut runtime = SongRuntime::new(&context).unwrap();
        runtime.begin_song_compilation().unwrap();
        let (screen, selected) = runtime.construct_gameplay_screen(&context).unwrap();
        runtime.load_gameplay_theme(screen, &selected).unwrap();
        let root = runtime
            .initialize_song_layers(screen, &SongLayerPlan::new(&[&entry], 0, &context).unwrap())
            .unwrap()
            .unwrap();
        let activation = runtime
            .activate_song_layers(screen, &context)
            .unwrap()
            .unwrap();
        runtime
            .discover_song_startup_commands(screen, &activation)
            .unwrap();
        let mut rows: SongOverlayActors<Kind> = runtime
            .read_song_overlay_actors::<(), ()>(
                root,
                &context,
                |_, _, _| Ok(Arc::from([])),
                |_| None,
                |_| {},
            )
            .unwrap();
        // Isolate the recognized factory callback from independent theme jobs,
        // as the existing analytic preparation tests do.
        execute(
            &mut runtime,
            "__songlua_top_screen=nil;__songlua_top_screen_player_1=nil;__songlua_top_screen_player_2=nil",
        );
        if !mutation.is_empty() {
            execute(&mut runtime, mutation);
        }
        assert_eq!(
            runtime.song_multitap_update_only(root).unwrap(),
            mutation.is_empty(),
            "analytic eligibility after {mutation:?} with initial {turns:?}"
        );
        let eases = runtime
            .compile_song_multitap_update_overlays(
                root,
                &context,
                &mut rows,
                SongLuaNoteskinResolver::default(),
                |_| {
                    Some((
                        SongLuaOverlayKind::Model {
                            layers: Arc::from([]),
                        },
                        SongLuaOverlayState::default(),
                    ))
                },
            )
            .unwrap();
        let Some(lanes) = lanes else {
            assert!(
                eases.is_none(),
                "{turns:?} requires the authored Lua callback"
            );
            runtime.release_song_overlay_actors(&mut rows).unwrap();
            runtime.release_actor_scene(screen).unwrap();
            continue;
        };
        let eases = eases.expect("neutral known factory uses analytic curves");
        let index = |name: &str| {
            rows.actors
                .iter()
                .position(|row| row.actor.name.as_deref() == Some(name))
                .unwrap_or_else(|| panic!("missing {name}"))
        };
        for (player, lane) in lanes.into_iter().enumerate() {
            let pn = player + 1;
            let frame = index(&format!("MultitapP{pn}_1"));
            let arrow = index(&format!("MultitapArrowP{pn}_1"));
            let expected_x = [-96.0, -32.0, 32.0, 96.0][lane - 1];
            assert!(
                eases.iter().any(|ease| {
                    ease.overlay_index == frame && ease.start < 4.0 && ease.to.x == Some(expected_x)
                }),
                "P{pn} {:?} must move the replacement frame to {expected_x}",
                turns[player]
            );
            let rotation = eases
                .iter()
                .filter(|ease| ease.overlay_index == arrow && ease.start < 4.0)
                .find_map(|ease| ease.to.rot_z_deg)
                .unwrap_or(rows.actors[arrow].actor.initial_state.rot_z_deg);
            let base_rotation = eases
                .iter()
                .filter(|ease| ease.overlay_index == arrow && ease.start < 4.0)
                .find_map(|ease| ease.to.base_rotation)
                .unwrap_or(rows.actors[arrow].actor.initial_state.base_rotation);
            // Native baserotationz remains separate from ArrowEffects rotation;
            // their matrices are not interchangeable with nonzero X/Y rotation.
            assert_eq!(base_rotation[2], [90.0, 0.0, 180.0, 270.0][lane - 1]);
            assert_eq!(rotation, 0.0);
            assert_eq!(
                base_rotation[2] + rotation,
                [90.0, 0.0, 180.0, 270.0][lane - 1],
                "P{pn} {:?} with captured base rotation {}",
                turns[player],
                base_rotations[player]
            );
            for column in 1..=4 {
                let explosion = index(&format!("MultitapExplosionP{pn}_{column}"));
                let becomes_visible = eases
                    .iter()
                    .any(|ease| ease.overlay_index == explosion && ease.to.visible == Some(true));
                assert_eq!(becomes_visible, column == lane, "P{pn} explosion {column}");
            }
        }
        runtime.release_song_overlay_actors(&mut rows).unwrap();
        runtime.release_actor_scene(screen).unwrap();
    }
}
