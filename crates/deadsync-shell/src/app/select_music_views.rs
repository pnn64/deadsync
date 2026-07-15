use super::App;
use deadsync_config::prelude as config;
use deadsync_online::score_compat as scores;
use deadsync_profile as profile_data;
use deadsync_theme::views::AudioPlaybackView;
use deadsync_theme_simply_love::screens::{SimplyLoveScreen as CurrentScreen, select_music};
use deadsync_theme_simply_love::views::{
    SelectMusicDownloadView, SelectMusicLeaderboardSideView, SelectMusicLeaderboardView,
    SelectMusicRuntimeView,
};

impl App {
    pub(super) fn sync_select_music_runtime_view(&mut self, config: &config::Config) {
        if self.state.screens.current_screen != CurrentScreen::SelectMusic {
            return;
        }
        let lobby = Self::refresh_lobby_runtime_view();
        let downloads =
            if select_music::downloads_overlay_visible(&self.state.screens.select_music_state) {
                deadsync_online::runtime::unlock_download_snapshots()
                    .into_iter()
                    .map(|snapshot| SelectMusicDownloadView {
                        name: snapshot.name,
                        current_bytes: snapshot.current_bytes,
                        total_bytes: snapshot.total_bytes,
                        complete: snapshot.complete,
                        error_message: snapshot.error_message,
                    })
                    .collect()
            } else {
                Vec::new()
            };
        let srpg_shop =
            if select_music::srpg_shop_overlay_visible(&self.state.screens.select_music_state) {
                deadsync_online::srpg_shop::runtime_snapshot()
            } else {
                Default::default()
            };
        let music_position_seconds = if deadsync_audio_stream::is_initialized() {
            f64::from(deadsync_audio_stream::get_music_stream_clock_snapshot().music_seconds)
        } else {
            0.0
        };
        let scorebox_request =
            select_music::scorebox_runtime_request(&self.state.screens.select_music_state);
        let leaderboard_request =
            select_music::leaderboard_runtime_request(&self.state.screens.select_music_state);
        let arrow_bounce_offset = -10.0 * config.global_offset_seconds;
        let policy = crate::select_music::policy_view(config);
        let sync_graph_mode = config.null_or_die_sync_graph;
        let sync_confidence_percent = config.null_or_die_confidence_percent;
        let scorebox_enabled = config.show_select_music_scorebox
            && (config.select_music_scorebox_cycle_itg
                || config.select_music_scorebox_cycle_ex
                || config.select_music_scorebox_cycle_hard_ex
                || config.select_music_scorebox_cycle_tournaments);
        let profile_view = profile_data::runtime_scorebox_view(
            config.enable_groovestats,
            config.enable_arrowcloud,
            config.auto_populate_gs_scores,
        );
        let music_wheel = Self::prepare_music_wheel_runtime(
            select_music::music_wheel_runtime_request(&self.state.screens.select_music_state),
            &profile_view,
        );
        let mut scorebox_hashes: [Option<String>; 2] = Default::default();
        if profile_view.play_style == profile_data::PlayStyle::Versus {
            scorebox_hashes = scorebox_request.chart_hashes;
        } else {
            let side = if profile_data::runtime_player_is_p2(
                profile_view.play_style,
                profile_view.player_side,
            ) {
                profile_data::PlayerSide::P2
            } else {
                profile_data::PlayerSide::P1
            };
            scorebox_hashes[profile_data::player_side_index(side)] =
                scorebox_request.chart_hashes[0].clone();
        }
        let scorebox_leaderboards: [Option<deadsync_score::CachedPlayerLeaderboardData>; 2] =
            std::array::from_fn(|side_idx| {
                if !(scorebox_request.leaderboards_allowed && scorebox_enabled) {
                    return None;
                }
                scorebox_hashes[side_idx].as_deref().and_then(|hash| {
                    scores::get_or_fetch_player_leaderboards_for_profile(
                        hash,
                        &profile_view.sides[side_idx].leaderboard,
                        scorebox_request.max_entries,
                    )
                })
            });
        let leaderboard =
            leaderboard_request.map_or_else(SelectMusicLeaderboardView::default, |request| {
                SelectMusicLeaderboardView {
                    sides: std::array::from_fn(|side_idx| {
                        let chart_hash = request.chart_hashes[side_idx].clone();
                        let player = &profile_view.sides[side_idx];
                        let machine_entries = if player.joined {
                            chart_hash
                                .as_deref()
                                .map(|hash| {
                                    scores::get_machine_leaderboard_local_with_names(
                                        hash,
                                        request.max_entries,
                                    )
                                })
                                .unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                        let leaderboards = if player.leaderboard.gs_active {
                            chart_hash.as_deref().and_then(|hash| {
                                scores::get_or_fetch_player_leaderboards_for_profile(
                                    hash,
                                    &player.leaderboard,
                                    request.max_entries,
                                )
                            })
                        } else {
                            None
                        };
                        SelectMusicLeaderboardSideView {
                            chart_hash,
                            machine_entries,
                            leaderboards,
                        }
                    }),
                }
            });
        let [p1_profile, p2_profile] = profile_view.sides;
        let [p1_hash, p2_hash] = scorebox_hashes;
        let [p1_leaderboards, p2_leaderboards] = scorebox_leaderboards;
        let scoreboxes = [
            Self::scorebox_side_view(p1_profile, p1_hash, p1_leaderboards),
            Self::scorebox_side_view(p2_profile, p2_hash, p2_leaderboards),
        ];
        select_music::sync_runtime_view(
            &mut self.state.screens.select_music_state,
            SelectMusicRuntimeView {
                audio_playback: AudioPlaybackView {
                    music_position_seconds,
                },
                lobby,
                downloads,
                srpg_shop,
                arrow_bounce_offset,
                policy,
                music_wheel,
                scoreboxes,
                leaderboard,
                unlock_downloads_available: deadsync_online::runtime::unlock_downloads_available(),
                ready_song_reload_dirs: deadsync_online::runtime::take_ready_song_reload_request(),
                sync_graph_mode,
                sync_confidence_percent,
            },
        );
    }
}
