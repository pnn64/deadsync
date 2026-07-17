use super::App;
use deadsync_config::prelude as config;
use deadsync_online::score_compat as scores;
use deadsync_profile as profile_data;
use deadsync_theme::views::AudioPlaybackView;
use deadsync_theme_simply_love::screens::{SimplyLoveScreen as CurrentScreen, select_music};
use deadsync_theme_simply_love::views::{
    SelectMusicDownloadView, SelectMusicLeaderboardSideView, SelectMusicLeaderboardView,
    SelectMusicPadProfileView, SelectMusicProfileView, SelectMusicRuntimeView,
    SelectMusicSessionView,
};

fn pad_in_play(session: SelectMusicSessionView, pad: usize) -> bool {
    match session.play_style {
        profile_data::PlayStyle::Double | profile_data::PlayStyle::Versus => true,
        profile_data::PlayStyle::Single => session.joined.get(pad).copied().unwrap_or(false),
    }
}

fn pad_profile_rows_match(
    configs: &[profile_data::pad_config::PadConfigProfile],
    serial: &str,
    rows: &[SelectMusicPadProfileView],
) -> bool {
    configs.len() == rows.len()
        && configs.iter().zip(rows).all(|(config, row)| {
            config.name == row.name
                && profile_data::pad_config::is_default_for(config, serial) == row.is_default
        })
}

impl App {
    fn select_music_pad_profiles(
        &mut self,
        session: SelectMusicSessionView,
        profiles: &SelectMusicProfileView,
    ) -> Option<[Vec<SelectMusicPadProfileView>; 2]> {
        let state = &self.state.screens.select_music_state;
        if !select_music::pad_profile_menu_visible(state) {
            return None;
        }

        let active: [bool; 2] = std::array::from_fn(|pad| {
            state.smx_pads[pad].connected
                && pad_in_play(session, pad)
                && profiles.pad_profile_id(pad).is_some()
        });
        for pad in 0..2 {
            if !active[pad] {
                continue;
            }
            let smx = &state.smx_pads[pad];
            let profile_id = profiles
                .pad_profile_id(pad)
                .expect("active pad profile should have an id");
            if self
                .pad_config_sync
                .profiles_stale(pad, Some(profile_id), smx.pad_type.as_deref())
            {
                let configs = deadsync_profile::compat::load_pad_configs(profile_id)
                    .into_iter()
                    .filter(|config| {
                        profile_data::pad_config::config_matches(
                            config,
                            &smx.backend_id,
                            smx.pad_type.as_deref(),
                        )
                    })
                    .collect();
                self.pad_config_sync.store_profiles(
                    pad,
                    Some(profile_id.to_owned()),
                    smx.pad_type.clone(),
                    configs,
                );
            }
        }

        let changed = (0..2).any(|pad| {
            let configs = if active[pad] {
                self.pad_config_sync.profiles_for(pad)
            } else {
                &[]
            };
            !pad_profile_rows_match(
                configs,
                &state.smx_pads[pad].serial,
                select_music::pad_profile_rows(state, pad),
            )
        });
        changed.then(|| {
            std::array::from_fn(|pad| {
                if !active[pad] {
                    return Vec::new();
                }
                let serial = &state.smx_pads[pad].serial;
                self.pad_config_sync
                    .profiles_for(pad)
                    .iter()
                    .map(|config| SelectMusicPadProfileView {
                        name: config.name.clone(),
                        is_default: profile_data::pad_config::is_default_for(config, serial),
                    })
                    .collect()
            })
        })
    }

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
        let sync_graph_orientation = config.null_or_die_graph_orientation;
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
            config,
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
        let pane_filter = super::scorebox_pane_filter(config);
        let srpg10 = matches!(config.srpg_variant, config::SrpgVariant::Srpg10)
            && config.visual_style.is_srpg();
        let scoreboxes = [
            Self::scorebox_side_view(p1_profile, p1_hash, p1_leaderboards, pane_filter, srpg10),
            Self::scorebox_side_view(p2_profile, p2_hash, p2_leaderboards, pane_filter, srpg10),
        ];
        let session = crate::select_music::session_view();
        let profiles = crate::select_music::profile_view();
        let favorites = (select_music::local_profile_ids(&self.state.screens.select_music_state)
            != &profiles.local_profile_ids)
            .then(deadsync_profile::runtime_favorite_snapshot);
        let pad_profiles = self.select_music_pad_profiles(session, &profiles);
        select_music::sync_runtime_view(
            &mut self.state.screens.select_music_state,
            SelectMusicRuntimeView {
                session,
                profiles,
                favorites,
                pad_profiles,
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
                sync_graph_orientation,
                sync_confidence_percent,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(
        name: &str,
        default_serial: Option<&str>,
    ) -> profile_data::pad_config::PadConfigProfile {
        profile_data::pad_config::PadConfigProfile {
            name: name.to_owned(),
            backend: "smx".to_owned(),
            pad_type: Some("fsr".to_owned()),
            serial: None,
            default_for_serials: default_serial.into_iter().map(str::to_owned).collect(),
            global_default: false,
            settings: Vec::new(),
        }
    }

    #[test]
    fn cached_pad_rows_compare_name_and_per_pad_default() {
        let configs = [config("Soft", Some("pad-a")), config("Firm", None)];
        let rows = [
            SelectMusicPadProfileView {
                name: "Soft".to_owned(),
                is_default: true,
            },
            SelectMusicPadProfileView {
                name: "Firm".to_owned(),
                is_default: false,
            },
        ];

        assert!(pad_profile_rows_match(&configs, "pad-a", &rows));
        assert!(!pad_profile_rows_match(&configs, "pad-b", &rows));
    }

    #[test]
    fn pad_profile_activity_matches_play_style() {
        let single = SelectMusicSessionView {
            joined: [true, false],
            ..Default::default()
        };
        assert!(pad_in_play(single, 0));
        assert!(!pad_in_play(single, 1));

        let double = SelectMusicSessionView {
            play_style: profile_data::PlayStyle::Double,
            joined: [true, false],
            ..Default::default()
        };
        assert!(pad_in_play(double, 0));
        assert!(pad_in_play(double, 1));
    }
}
