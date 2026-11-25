use crate::Command;
pub use deadsync_assets::gameplay_stage::PreparedGameplaySongLua;
pub use deadsync_assets::song_lua::apply_song_lua_reload;
use deadsync_chart::SongData;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::stage_charts::GameplayChartEntryPlan;
use deadsync_profile::{PlayStyle, PlayerSide, player_side_index};

mod song_lua;

pub use song_lua::{SONG_LUA_MEDIA_PROBES, prepare_song_lua, song_lua_context};

pub(crate) fn gameplay_last_played_commands(
    song: &SongData,
    plan: &GameplayChartEntryPlan,
    preferred_steps: [usize; MAX_PLAYERS],
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> Vec<Command> {
    let mut commands = Vec::with_capacity(MAX_PLAYERS);
    if play_style.player_count() == MAX_PLAYERS {
        for (idx, side) in [(0, PlayerSide::P1), (1, PlayerSide::P2)] {
            commands.push(Command::UpdateLastPlayed {
                side,
                play_style,
                music_path: song.music_path.clone(),
                chart_hash: Some(plan.charts[idx].short_hash.clone()),
                difficulty_index: preferred_steps[idx],
            });
        }
    } else {
        let index = player_side_index(player_side);
        commands.push(Command::UpdateLastPlayed {
            side: player_side,
            play_style,
            music_path: song.music_path.clone(),
            chart_hash: Some(plan.charts[index].short_hash.clone()),
            difficulty_index: preferred_steps[index],
        });
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_gameplay::GameplayInputPlayStyle as Style;
    use deadsync_gameplay::stage_charts::lua_reload_chart_indices;
    use deadsync_profile_gameplay::stage_charts::gameplay_chart_entry_plan;
    use deadsync_chart::{ArrowStats, ChartData, StaminaCounts, TechCounts};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn chart(difficulty: &str, hash: &str) -> ChartData {
        ChartData {
            radar_numerators: [0.0; 5],
            chart_type: "dance-single".to_string(),
            difficulty: difficulty.to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 9,
            step_artist: String::new(),
            music_file: String::new(),
            music_path: None,
            short_hash: hash.to_string(),
            stats: ArrowStats::default(),
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            matrix_profile: Box::default(),
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
        }
    }

    fn song() -> Arc<SongData> {
        Arc::new(SongData {
            simfile_path: PathBuf::from("song.ssc"),
            title: "Song".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            translit_artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: Some(PathBuf::from("song.ogg")),
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
            normalized_bpms: "120.000".to_string(),
            song_timing: None,
            music_length_seconds: 60.0,
            first_second: 0.0,
            total_length_seconds: 60,
            precise_last_second_seconds: 60.0,
            precise_note_first_second_seconds: None,
            precise_note_last_second_seconds: Some(60.0),
            last_second_hint: 0.0,
            charts: vec![chart("Hard", "hard"), chart("Challenge", "challenge")],
        })
    }

    #[test]
    fn song_lua_mode_selects_execution_and_reports_bake_progress() {
        use deadsync_config::options::SongLuaMode;

        crate::tests::init_paths();
        let dir = tempfile::tempdir().unwrap();
        let simfile = dir.path().join("mode.ssc");
        let source = dir.path().join("main.lua");
        std::fs::write(
            &simfile,
            "#TITLE:Lua mode;\n#BPMS:0=120;\n\
            #FGCHANGES:0=main.lua=1=0=0=0=StretchNoLoop====;\n\
            #NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Easy;\n\
            #METER:1;\n#NOTES:\n1000\n0000\n0010\n0000\n;\n",
        )
        .unwrap();
        let frames = 2_000_u32;
        let rate = 8_000_u32;
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&(36 + frames).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(b"\x01\0\x08\0data");
        wav.extend_from_slice(&frames.to_le_bytes());
        wav.resize(44 + frames as usize, 128);
        std::fs::write(dir.path().join("probe.wav"), wav).unwrap();
        std::fs::write(
            &source,
            r#"return Def.ActorFrame {
                Def.Quad { OnCommand = function(self) self:linear(1):x(100) end },
                Def.Sound {
                    File = 'probe.wav',
                    InitCommand = function(self) assert(self:get():get_length() == 0.25) end,
                },
            }"#,
        )
        .unwrap();
        let song = deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0).unwrap();
        let chart = Arc::new(song.charts[0].clone());
        let charts = [chart.clone(), chart];
        let loaded = crate::song_loading::load_gameplay_charts(&song, &[0], 0.0).unwrap();
        let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
        let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
        let session = deadsync_gameplay::GameplaySession::default();
        let config = deadsync_gameplay::GameplayConfig {
            global_offset_seconds: 0.0,
            ..Default::default()
        };
        let prepare = |song: &SongData, mode, progress| {
            prepare_song_lua(
                SONG_LUA_MEDIA_PROBES,
                song,
                &charts,
                [&loaded[0].timing; MAX_PLAYERS],
                &profiles,
                &scroll,
                1.0,
                deadsync_gameplay::GameplayViewport::design(),
                (1280, 720),
                16.0 / 9.0,
                0.0,
                &session,
                &config,
                "Software",
                1,
                mode,
                progress,
            )
        };
        for (mode, live) in [(SongLuaMode::Baked, false), (SongLuaMode::Runtime, true)] {
            let progress = deadsync_song_lua::SongLuaBakeProgress::default();
            let mut prepared = prepare(&song, mode, Some(progress.clone()));
            assert_eq!(progress.fraction(), if live { 0.0 } else { 1.0 });
            assert!(prepared.compile_succeeded(), "{mode:?} compilation");
            assert!(
                prepared
                    .compiled_layers()
                    .any(|layer| !layer.overlays.is_empty()),
                "{mode:?} actors"
            );
            assert_eq!(
                prepared.take_live_scene().unwrap().is_some(),
                live,
                "{mode:?}"
            );
        }

        std::fs::write(&source, "SCREENMAN:GetTopScreen():AddInputCallback(function(event) return false end)\nreturn Def.Actor {}").unwrap();
        let mut interactive = prepare(&song, SongLuaMode::Baked, None);
        assert!(interactive.compile_succeeded());
        assert!(
            interactive.take_live_scene().unwrap().is_some(),
            "input callbacks need runtime in baked mode"
        );

        let mut ordinary = song;
        ordinary.foreground_lua_changes.clear();
        ordinary.has_lua = false;
        for mode in [SongLuaMode::Baked, SongLuaMode::Runtime] {
            let mut prepared = prepare(&ordinary, mode, None);
            assert!(prepared.compiled_layers().next().is_none());
            assert!(
                prepared.take_live_scene().unwrap().is_none(),
                "ordinary song in {mode:?} mode"
            );
        }
    }

    #[test]
    fn pump_preparation_applies_its_retained_game_qualified_style() {
        use deadsync_gameplay::GameplayInputPlayStyle as Style;
        crate::tests::init_paths();
        let dir = tempfile::tempdir().unwrap();
        let simfile = dir.path().join("pump.ssc");
        std::fs::write(dir.path().join("main.lua"), "return Def.ActorFrame {}").unwrap();
        for (style, kind, notes) in [
            (Style::PumpSingle, "single", "10000\n00000\n00100\n00000"),
            (Style::PumpVersus, "single", "10000\n00000\n00100\n00000"),
            (Style::PumpDouble, "double", "1000000000\n0000000000\n0000000010\n0000000000"),
            (Style::PumpCouple, "couple", "1000000000\n0000000000\n0000000010\n0000000000"),
            (Style::PumpRoutine, "routine", "1000000000\n0000000000\n0000000000\n0000000000\n&\n0000000000\n0000000000\n0000000010\n0000000000"),
        ] {
            std::fs::write(&simfile, format!(
                "#TITLE:Pump preparation;#BPMS:0=120;#FGCHANGES:0=main.lua=1=0=0=1;\n\
                 #NOTEDATA:;#STEPSTYPE:pump-{kind};#DIFFICULTY:Challenge;#METER:1;#NOTES:\n{notes}\n;"
            )).unwrap();
            let song = deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0).unwrap();
            let mut session = deadsync_gameplay::GameplaySession {
                play_style: style,
                joined_sides: [true, style.player_count() == 2],
                ..Default::default()
            };
            let mut charts = std::array::from_fn(|_| Arc::new(song.charts[0].clone()));
            let payload = Arc::new(crate::song_loading::load_gameplay_charts(&song, &[0], 0.0).unwrap().remove(0));
            let mut payloads = [payload.clone(), payload];
            let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
            let scroll = profiles.each_ref().map(|profile| profile.scroll_speed);
            let config = deadsync_gameplay::GameplayConfig { global_offset_seconds: 0.0, ..Default::default() };
            let prepared = prepare_song_lua(SONG_LUA_MEDIA_PROBES, &song, &charts,
                std::array::from_fn(|p| &payloads[p].timing), &profiles, &scroll, 1.0,
                deadsync_gameplay::GameplayViewport::design(), (1280, 720), 16.0 / 9.0,
                1.0, &session, &config, "Software", 1,
                deadsync_config::options::SongLuaMode::Runtime, None);
            assert!(prepared.compile_succeeded(), "{style:?} compilation");
            assert!(prepared.loaded_style().unwrap().starts_with("pump-"));
            assert!(apply_song_lua_reload(&song, &mut charts, &mut payloads, &prepared,
                &mut session, 0.0).unwrap(), "{style:?} selection was applied");
            assert_eq!(session.play_style, style);
            assert_eq!(session.joined_sides, [true, style.player_count() == 2]);
            assert!(charts.iter().all(|chart| chart.chart_type == format!("pump-{kind}")));
        }
    }

    #[test]
    fn lua_reload_uses_the_same_chart_payload_for_a_solo_stage() {
        assert_eq!(
            lua_reload_chart_indices(&song(), [Some(1), None], Style::Single),
            Ok([1, 1])
        );
    }

    #[test]
    fn reload_keeps_versus_selections_and_rejects_missing_steps() {
        assert_eq!(
            lua_reload_chart_indices(&song(), [Some(1), Some(0)], Style::Versus),
            Ok([1, 0])
        );
        assert!(lua_reload_chart_indices(&song(), [Some(1), None], Style::Versus).is_err());
        assert!(lua_reload_chart_indices(&song(), [Some(1), Some(7)], Style::Versus).is_err());
        let mut incompatible = song().as_ref().clone();
        incompatible.charts[1].chart_type = "dance-double".into();
        assert!(
            lua_reload_chart_indices(&incompatible, [Some(1), Some(0)], Style::Versus).is_err()
        );
    }

    #[test]
    fn chart_reload_commits_only_after_all_replacements_load() {
        use deadsync_gameplay::GameplaySession;
        use deadsync_profile_gameplay::stage_charts::{StageChartReload, apply_stage_chart_reload};

        crate::tests::init_paths();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollback.sm");
        std::fs::write(
            &path,
            "#TITLE:Reload rollback;#BPMS:0=120;\
             #NOTES:dance-single::Easy:1:0,0,0,0,0:\n1000\n0000\n0010\n0000\n;",
        )
        .unwrap();
        let song = deadsync_simfile::app_runtime::parse_song_for_test(&path, 0.0).unwrap();
        let chart = Arc::new(song.charts[0].clone());
        let payload = Arc::new(
            crate::song_loading::load_gameplay_charts(&song, &[0], 0.0)
                .unwrap()
                .remove(0),
        );
        let mut unavailable_song = song.clone();
        unavailable_song.simfile_path = dir.path().join("missing.sm");
        let mut charts = [Arc::clone(&chart), Arc::clone(&chart)];
        let mut payloads = [Arc::clone(&payload), Arc::clone(&payload)];
        let initial_session = GameplaySession::default();
        let mut session = initial_session.clone();
        let versus = |p2_step| StageChartReload {
            steps: [Some(0), p2_step],
            joined_sides: Some([true, true]),
            play_style: Some(Style::Versus),
        };

        for (source, p2_step) in [(&song, Some(1)), (&unavailable_song, Some(0))] {
            assert!(apply_stage_chart_reload(
                source, &mut charts, &mut payloads, versus(p2_step), &mut session, 0.0,
            )
            .is_err());
            assert_eq!(session, initial_session);
            assert!(charts.iter().all(|item| Arc::ptr_eq(item, &chart)));
            assert!(payloads.iter().all(|item| Arc::ptr_eq(item, &payload)));
        }

        apply_stage_chart_reload(
            &song, &mut charts, &mut payloads, versus(Some(0)), &mut session, 0.0,
        )
        .unwrap();
        assert_eq!(session.play_style, Style::Versus);
        assert_eq!(session.joined_sides, [true, true]);
        assert!(charts.iter().all(|item| !Arc::ptr_eq(item, &chart)));
        assert!(payloads.iter().all(|item| !Arc::ptr_eq(item, &payload)));
    }

    #[test]
    fn versus_keeps_independent_chart_slots_and_profile_updates() {
        let plan =
            gameplay_chart_entry_plan(&song(), [3, 4], [3, 4], PlayStyle::Versus, PlayerSide::P1);

        assert_eq!(plan.resolved_steps_index, [3, 4]);
        assert_eq!(plan.chart_indices, [0, 1]);
        assert_eq!(plan.charts[0].short_hash, "hard");
        assert_eq!(plan.charts[1].short_hash, "challenge");
        let commands = gameplay_last_played_commands(
            song().as_ref(),
            &plan,
            [3, 4],
            PlayStyle::Versus,
            PlayerSide::P1,
        );
        assert_eq!(commands.len(), 2);
        assert!(matches!(
            &commands[1],
            Command::UpdateLastPlayed {
                side: PlayerSide::P2,
                chart_hash: Some(hash),
                difficulty_index: 4,
                ..
            } if hash == "challenge"
        ));
    }

    #[test]
    fn single_p2_falls_back_and_duplicates_the_active_chart() {
        let plan =
            gameplay_chart_entry_plan(&song(), [3, 0], [3, 4], PlayStyle::Single, PlayerSide::P2);

        assert_eq!(plan.resolved_steps_index, [3, 4]);
        assert_eq!(plan.chart_indices, [1, 1]);
        assert_eq!(plan.charts[0].short_hash, "challenge");
        assert_eq!(plan.charts[1].short_hash, "challenge");
        assert!(Arc::ptr_eq(&plan.charts[0], &plan.charts[1]));
        let commands = gameplay_last_played_commands(
            song().as_ref(),
            &plan,
            [3, 4],
            PlayStyle::Single,
            PlayerSide::P2,
        );
        assert!(matches!(
            &commands[0],
            Command::UpdateLastPlayed {
                side: PlayerSide::P2,
                chart_hash: Some(hash),
                difficulty_index: 4,
                ..
            } if hash == "challenge"
        ));
    }

    #[test]
    fn partner_styles_use_the_first_selection_for_both_players() {
        for style in [
            PlayStyle::Couple,
            PlayStyle::Routine,
            PlayStyle::PumpCouple,
            PlayStyle::PumpRoutine,
        ] {
            let mut song = song();
            for chart in &mut Arc::make_mut(&mut song).charts {
                chart.chart_type = style.chart_type().into();
            }
            let plan = gameplay_chart_entry_plan(&song, [3, 4], [3, 4], style, PlayerSide::P2);
            assert_eq!(plan.chart_indices, [0, 0]);
            assert_eq!(plan.resolved_steps_index, [3, 3]);
            assert_eq!(plan.charts[0].short_hash, "hard");
            assert!(Arc::ptr_eq(&plan.charts[0], &plan.charts[1]));
        }
    }
}
