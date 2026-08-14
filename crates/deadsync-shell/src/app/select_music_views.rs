use super::{App, MusicWheelDisplayPolicy};
use deadsync_config::prelude as config;
use deadsync_online::score_compat as scores;
use deadsync_profile as profile_data;
use deadsync_theme::views::AudioPlaybackView;
use deadsync_theme_simply_love::screens::{SimplyLoveScreen as CurrentScreen, select_music};
use deadsync_theme_simply_love::views::{
    SelectMusicDownloadView, SelectMusicLeaderboardRequest, SelectMusicLeaderboardSideView,
    SelectMusicLeaderboardView, SelectMusicPadProfileView, SelectMusicPolicyView,
    SelectMusicProfileView, SelectMusicRuntimeView, SelectMusicScoreboxRequest,
    SelectMusicSessionView, SelectMusicSettingsView, SimplyLoveLobbyRuntimeView,
};
use std::{sync::Arc, time::Instant};

/// Config-generation policy for Select Music's frame-time view preparation.
/// The persisted `Config` remains owned by the app; this compact value carries
/// only resolved inputs needed by the active screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SelectMusicFramePolicy {
    view: SelectMusicPolicyView,
    wheel: MusicWheelDisplayPolicy,
    pane_filter: deadsync_score::SelectMusicScoreboxFilter,
    sync_graph_mode: config::SyncGraphMode,
    sync_graph_orientation: config::GraphOrientation,
    sync_graph_origin: config::GraphOrigin,
    arrow_bounce_offset: f32,
    sync_confidence_percent: u8,
    enable_groovestats: bool,
    enable_arrowcloud: bool,
    auto_populate_gs_scores: bool,
    auto_download_unlocks: bool,
}

#[derive(Debug)]
pub(super) struct MusicWheelRuntimeKey {
    source: select_music::MusicWheelRuntimeToken,
    profile_snapshot: Arc<profile_data::MusicProfileSnapshot>,
    display: MusicWheelDisplayPolicy,
    favorites_generation: u64,
    score_generation: u64,
}

impl MusicWheelRuntimeKey {
    fn new(
        source: select_music::MusicWheelRuntimeToken,
        profile_snapshot: &Arc<profile_data::MusicProfileSnapshot>,
        display: MusicWheelDisplayPolicy,
    ) -> Self {
        Self {
            source,
            profile_snapshot: Arc::clone(profile_snapshot),
            display,
            favorites_generation: profile_data::runtime_favorites_generation(),
            score_generation: deadsync_score::runtime_music_wheel_score_generation(),
        }
    }

    fn matches(
        &self,
        source: select_music::MusicWheelRuntimeToken,
        profile_snapshot: &Arc<profile_data::MusicProfileSnapshot>,
        display: MusicWheelDisplayPolicy,
    ) -> bool {
        self.source == source
            && Arc::ptr_eq(&self.profile_snapshot, profile_snapshot)
            && self.display == display
            && self.favorites_generation == profile_data::runtime_favorites_generation()
            && self.score_generation == deadsync_score::runtime_music_wheel_score_generation()
    }
}

#[derive(Debug)]
struct ChartPairKey([Option<Box<str>>; 2]);

impl ChartPairKey {
    fn new(hashes: [Option<&str>; 2]) -> Self {
        Self(hashes.map(|hash| hash.map(Into::into)))
    }

    fn matches(&self, hashes: [Option<&str>; 2]) -> bool {
        self.0
            .iter()
            .zip(hashes)
            .all(|(stored, hash)| stored.as_deref() == hash)
    }
}

#[derive(Debug)]
pub(super) struct ScoreboxRuntimeKey {
    chart_hashes: ChartPairKey,
    leaderboards_allowed: bool,
    max_entries: usize,
    profile_snapshot: Arc<profile_data::MusicProfileSnapshot>,
    pane_filter: deadsync_score::SelectMusicScoreboxFilter,
    enabled: bool,
    score_generation: u64,
    leaderboard_generation: u64,
}

impl ScoreboxRuntimeKey {
    fn new(
        request: SelectMusicScoreboxRequest<'_>,
        profile_snapshot: &Arc<profile_data::MusicProfileSnapshot>,
        pane_filter: deadsync_score::SelectMusicScoreboxFilter,
        enabled: bool,
    ) -> Self {
        Self {
            chart_hashes: ChartPairKey::new(request.chart_hashes),
            leaderboards_allowed: request.leaderboards_allowed,
            max_entries: request.max_entries,
            profile_snapshot: Arc::clone(profile_snapshot),
            pane_filter,
            enabled,
            score_generation: deadsync_score::runtime_music_wheel_score_generation(),
            leaderboard_generation: deadsync_score::runtime_player_leaderboard_generation(),
        }
    }

    fn matches(
        &self,
        request: SelectMusicScoreboxRequest<'_>,
        profile_snapshot: &Arc<profile_data::MusicProfileSnapshot>,
        pane_filter: deadsync_score::SelectMusicScoreboxFilter,
        enabled: bool,
    ) -> bool {
        self.chart_hashes.matches(request.chart_hashes)
            && self.leaderboards_allowed == request.leaderboards_allowed
            && self.max_entries == request.max_entries
            && Arc::ptr_eq(&self.profile_snapshot, profile_snapshot)
            && self.pane_filter == pane_filter
            && self.enabled == enabled
            && self.score_generation == deadsync_score::runtime_music_wheel_score_generation()
            && self.leaderboard_generation
                == deadsync_score::runtime_player_leaderboard_generation()
    }
}

#[derive(Debug)]
struct LeaderboardRequestKey {
    chart_hashes: ChartPairKey,
    max_entries: usize,
}

impl LeaderboardRequestKey {
    fn new(request: SelectMusicLeaderboardRequest<'_>) -> Self {
        Self {
            chart_hashes: ChartPairKey::new(request.chart_hashes),
            max_entries: request.max_entries,
        }
    }

    fn matches(&self, request: SelectMusicLeaderboardRequest<'_>) -> bool {
        self.chart_hashes.matches(request.chart_hashes) && self.max_entries == request.max_entries
    }
}

#[derive(Debug)]
pub(super) struct LeaderboardRuntimeKey {
    request: Option<LeaderboardRequestKey>,
    profile_snapshot: Arc<profile_data::MusicProfileSnapshot>,
    score_generation: u64,
    leaderboard_generation: u64,
}

impl LeaderboardRuntimeKey {
    fn new(
        request: Option<SelectMusicLeaderboardRequest<'_>>,
        profile_snapshot: &Arc<profile_data::MusicProfileSnapshot>,
    ) -> Self {
        Self {
            request: request.map(LeaderboardRequestKey::new),
            profile_snapshot: Arc::clone(profile_snapshot),
            score_generation: deadsync_score::runtime_music_wheel_score_generation(),
            leaderboard_generation: deadsync_score::runtime_player_leaderboard_generation(),
        }
    }

    fn matches(
        &self,
        request: Option<SelectMusicLeaderboardRequest<'_>>,
        profile_snapshot: &Arc<profile_data::MusicProfileSnapshot>,
    ) -> bool {
        let request_matches = match (&self.request, request) {
            (None, None) => true,
            (Some(stored), Some(request)) => stored.matches(request),
            _ => false,
        };
        request_matches
            && Arc::ptr_eq(&self.profile_snapshot, profile_snapshot)
            && self.score_generation == deadsync_score::runtime_music_wheel_score_generation()
            && self.leaderboard_generation
                == deadsync_score::runtime_player_leaderboard_generation()
    }
}

impl SelectMusicFramePolicy {
    pub(super) fn from_config(config: &config::Config) -> Self {
        Self {
            view: crate::select_music::policy_view(config),
            wheel: MusicWheelDisplayPolicy::from_config(config),
            pane_filter: super::scorebox_pane_filter(config),
            sync_graph_mode: config.null_or_die_sync_graph,
            sync_graph_orientation: config.null_or_die_graph_orientation,
            sync_graph_origin: config.null_or_die_graph_origin,
            arrow_bounce_offset: -10.0 * config.global_offset_seconds,
            sync_confidence_percent: config.null_or_die_confidence_percent,
            enable_groovestats: config.enable_groovestats,
            enable_arrowcloud: config.enable_arrowcloud,
            auto_populate_gs_scores: config.auto_populate_gs_scores,
            auto_download_unlocks: config.auto_download_unlocks,
        }
    }

    fn settings_view(self) -> SelectMusicSettingsView {
        SelectMusicSettingsView {
            arrow_bounce_offset: self.arrow_bounce_offset,
            policy: self.view,
            sync_graph_mode: self.sync_graph_mode,
            sync_graph_orientation: self.sync_graph_orientation,
            sync_graph_origin: self.sync_graph_origin,
            sync_confidence_percent: self.sync_confidence_percent,
        }
    }
}

fn pad_in_play(session: SelectMusicSessionView, pad: usize) -> bool {
    match session.play_style {
        profile_data::PlayStyle::Double
        | profile_data::PlayStyle::Versus
        | profile_data::PlayStyle::PumpDouble
        | profile_data::PlayStyle::PumpVersus => true,
        profile_data::PlayStyle::Single | profile_data::PlayStyle::PumpSingle => {
            session.joined.get(pad).copied().unwrap_or(false)
        }
    }
}

fn leaderboard_retry_deadline(
    hashes: [Option<&str>; 2],
    profiles: &profile_data::ScoreboxRuntimeView,
) -> Option<Instant> {
    hashes
        .into_iter()
        .enumerate()
        .filter_map(|(side_idx, hash)| {
            deadsync_score::runtime_player_leaderboard_retry_deadline(
                hash?,
                &profiles.sides[side_idx].leaderboard,
            )
        })
        .min()
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
        profiles: &profile_data::MusicProfileSnapshot,
    ) -> Option<[Vec<SelectMusicPadProfileView>; 2]> {
        let state = &self.state.screens.select_music_state;
        if !select_music::pad_profile_menu_visible(state) {
            return None;
        }

        let active: [bool; 2] = std::array::from_fn(|pad| {
            state.smx_pads[pad].connected
                && pad_in_play(session, pad)
                && profiles.pad_profile_ids[pad].is_some()
        });
        for pad in 0..2 {
            if !active[pad] {
                continue;
            }
            let smx = &state.smx_pads[pad];
            let profile_id = profiles.pad_profile_ids[pad]
                .as_deref()
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

    pub(super) fn sync_select_music_runtime_view(&mut self, policy: SelectMusicFramePolicy) {
        if self.state.screens.current_screen != CurrentScreen::SelectMusic {
            return;
        }
        let now = Instant::now();
        let lobby_generation = deadsync_online::lobbies::runtime_view_generation();
        let lobby_refresh_due = self
            .select_music_lobby_refresh_at
            .is_some_and(|refresh_at| now >= refresh_at);
        let lobby_dirty = self.select_music_lobby_rebuild
            || self.select_music_lobby_generation != lobby_generation
            || lobby_refresh_due;
        let lobby = lobby_dirty.then(|| {
            let refresh = deadsync_online::lobbies::runtime_refresh_view_state_default();
            self.select_music_lobby_generation = refresh.generation;
            self.select_music_lobby_refresh_at = refresh.next_refresh_at;
            SimplyLoveLobbyRuntimeView {
                snapshot: refresh.snapshot,
                reconnect_status_text: refresh.reconnect_status_text,
                disconnect_hold_seconds: deadsync_online::lobbies::LOBBY_DISCONNECT_HOLD_SECONDS,
            }
        });
        self.select_music_lobby_rebuild = false;
        let downloads_visible =
            select_music::downloads_overlay_visible(&self.state.screens.select_music_state);
        let downloads = if downloads_visible {
            let last_generation = if self.select_music_downloads_visible {
                self.select_music_download_generation
            } else {
                0
            };
            self.select_music_downloads_visible = true;
            deadsync_online::runtime::unlock_download_snapshots_if_changed(last_generation).map(
                |(generation, snapshots)| {
                    self.select_music_download_generation = generation;
                    snapshots
                        .into_iter()
                        .map(|snapshot| SelectMusicDownloadView {
                            name: snapshot.name,
                            current_bytes: snapshot.current_bytes,
                            total_bytes: snapshot.total_bytes,
                            complete: snapshot.complete,
                            error_message: snapshot.error_message,
                        })
                        .collect()
                },
            )
        } else {
            self.select_music_downloads_visible = false;
            None
        };
        let shop_visible =
            select_music::srpg_shop_overlay_visible(&self.state.screens.select_music_state);
        let srpg_shop = if shop_visible {
            let last_generation = if self.select_music_shop_visible {
                self.select_music_shop_generation
            } else {
                0
            };
            self.select_music_shop_visible = true;
            deadsync_online::srpg_shop::runtime_snapshot_if_changed(last_generation).map(
                |(generation, snapshot)| {
                    self.select_music_shop_generation = generation;
                    snapshot
                },
            )
        } else {
            self.select_music_shop_visible = false;
            None
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
        let scorebox_enabled = policy.view.presentation.show_scorebox
            && policy.view.presentation.scorebox_cycle_enabled;
        let profile_generation = profile_data::runtime_profile_generation();
        let profile_policy = (
            policy.enable_groovestats,
            policy.enable_arrowcloud,
            policy.auto_populate_gs_scores,
        );
        let profile_source_dirty = self.select_music_profile_snapshot.is_none()
            || self.select_music_profile_generation != profile_generation
            || self.select_music_profile_policy != Some(profile_policy);
        let profile_snapshot_changed = if profile_source_dirty {
            let snapshot = profile_data::runtime_music_profile_snapshot(
                policy.enable_groovestats,
                policy.enable_arrowcloud,
                policy.auto_populate_gs_scores,
            );
            let changed = self
                .select_music_profile_snapshot
                .as_ref()
                .is_none_or(|previous| !Arc::ptr_eq(previous, &snapshot));
            self.select_music_profile_generation = profile_generation;
            self.select_music_profile_policy = Some(profile_policy);
            self.select_music_profile_snapshot = Some(snapshot);
            changed
        } else {
            false
        };
        let profile_views_dirty = self.select_music_profile_rebuild || profile_snapshot_changed;
        self.select_music_profile_rebuild = false;
        let profile_snapshot = Arc::clone(
            self.select_music_profile_snapshot
                .as_ref()
                .expect("Select Music profile snapshot should be warmed"),
        );
        let profile_view = &profile_snapshot.scorebox;
        let music_wheel_source =
            select_music::music_wheel_runtime_token(&self.state.screens.select_music_state);
        let music_wheel_dirty = self.select_music_wheel_rebuild
            || self.select_music_wheel_key.as_ref().is_none_or(|key| {
                !key.matches(music_wheel_source, &profile_snapshot, policy.wheel)
            });
        let music_wheel = music_wheel_dirty.then(|| {
            let request =
                select_music::music_wheel_runtime_request(&self.state.screens.select_music_state);
            self.select_music_wheel_key = Some(MusicWheelRuntimeKey::new(
                music_wheel_source,
                &profile_snapshot,
                policy.wheel,
            ));
            self.select_music_wheel_rebuild = false;
            Self::prepare_music_wheel_runtime(request, profile_view, policy.wheel)
        });
        let scorebox_retry_due = self
            .select_music_scorebox_retry_at
            .is_some_and(|retry_at| now >= retry_at);
        let scoreboxes_dirty = self.select_music_score_views_rebuild
            || scorebox_retry_due
            || self.select_music_scorebox_key.as_ref().is_none_or(|key| {
                !key.matches(
                    scorebox_request,
                    &profile_snapshot,
                    policy.pane_filter,
                    scorebox_enabled,
                )
            });
        let scoreboxes = scoreboxes_dirty.then(|| {
            self.select_music_scorebox_key = Some(ScoreboxRuntimeKey::new(
                scorebox_request,
                &profile_snapshot,
                policy.pane_filter,
                scorebox_enabled,
            ));
            let mut hashes = [None, None];
            if profile_view.play_style.is_versus() {
                hashes = scorebox_request.chart_hashes;
            } else {
                let side = if profile_data::runtime_player_is_p2(
                    profile_view.play_style,
                    profile_view.player_side,
                ) {
                    profile_data::PlayerSide::P2
                } else {
                    profile_data::PlayerSide::P1
                };
                hashes[profile_data::player_side_index(side)] = scorebox_request.chart_hashes[0];
            }
            let leaderboards: [Option<deadsync_score::CachedPlayerLeaderboardData>; 2] =
                std::array::from_fn(|side_idx| {
                    if !(scorebox_request.leaderboards_allowed && scorebox_enabled) {
                        return None;
                    }
                    hashes[side_idx].and_then(|hash| {
                        scores::get_or_fetch_player_leaderboards_for_profile(
                            hash,
                            &profile_view.sides[side_idx].leaderboard,
                            scorebox_request.max_entries,
                        )
                    })
                });
            self.select_music_scorebox_retry_at =
                if scorebox_request.leaderboards_allowed && scorebox_enabled {
                    leaderboard_retry_deadline(hashes, profile_view)
                } else {
                    None
                };
            std::array::from_fn(|side_idx| {
                Self::scorebox_side_view(
                    &profile_view.sides[side_idx],
                    hashes[side_idx].map(str::to_owned),
                    leaderboards[side_idx].clone(),
                    policy.pane_filter,
                )
            })
        });
        let leaderboard_retry_due = self
            .select_music_leaderboard_retry_at
            .is_some_and(|retry_at| now >= retry_at);
        let leaderboard_dirty = self.select_music_score_views_rebuild
            || leaderboard_retry_due
            || self
                .select_music_leaderboard_key
                .as_ref()
                .is_none_or(|key| !key.matches(leaderboard_request, &profile_snapshot));
        let leaderboard = leaderboard_dirty.then(|| {
            self.select_music_leaderboard_key = Some(LeaderboardRuntimeKey::new(
                leaderboard_request,
                &profile_snapshot,
            ));
            let view =
                leaderboard_request.map_or_else(SelectMusicLeaderboardView::default, |request| {
                    SelectMusicLeaderboardView {
                        sides: std::array::from_fn(|side_idx| {
                            let chart_hash = request.chart_hashes[side_idx];
                            let player = &profile_view.sides[side_idx];
                            let machine_entries = if player.joined {
                                chart_hash
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
                                chart_hash.and_then(|hash| {
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
                                chart_hash: chart_hash.map(str::to_owned),
                                machine_entries,
                                leaderboards,
                            }
                        }),
                    }
                });
            self.select_music_leaderboard_retry_at = leaderboard_request
                .and_then(|request| leaderboard_retry_deadline(request.chart_hashes, profile_view));
            view
        });
        self.select_music_score_views_rebuild = false;
        let session_view = SelectMusicSessionView {
            play_style: profile_view.play_style,
            player_side: profile_view.player_side,
            joined: profile_view.sides.each_ref().map(|side| side.joined),
            guest: profile_view.sides.each_ref().map(|side| side.guest),
            music_rate: profile_snapshot.music_rate,
        };
        let session = profile_views_dirty.then_some(session_view);
        let profiles = profile_views_dirty.then(|| SelectMusicProfileView {
            display_names: profile_view
                .sides
                .each_ref()
                .map(|side| Arc::clone(&side.display_name)),
            avatar_texture_keys: profile_snapshot
                .avatar_texture_keys
                .each_ref()
                .map(|key| key.as_ref().map(Arc::clone)),
            local_profile_ids: profile_snapshot
                .local_profile_ids
                .each_ref()
                .map(|id| id.as_ref().map(Arc::clone)),
            pad_profile_ids: profile_snapshot
                .pad_profile_ids
                .each_ref()
                .map(|id| id.as_ref().map(Arc::clone)),
        });
        let favorites = (profile_views_dirty
            && select_music::local_profile_ids(&self.state.screens.select_music_state)
                != &profile_snapshot.local_profile_ids)
            .then(deadsync_profile::runtime_favorite_snapshot);
        let pad_profiles = self.select_music_pad_profiles(session_view, &profile_snapshot);
        let settings = self
            .select_music_settings_rebuild
            .then(|| policy.settings_view());
        self.select_music_settings_rebuild = false;
        let unlock_status_generation =
            deadsync_online::runtime::unlock_download_status_generation();
        let unlock_dirty = self.select_music_unlock_rebuild
            || self.select_music_unlock_status_generation != unlock_status_generation;
        let unlock_downloads_available = unlock_dirty.then(|| {
            self.select_music_unlock_status_generation = unlock_status_generation;
            deadsync_online::runtime::unlock_downloads_available(policy.auto_download_unlocks)
        });
        self.select_music_unlock_rebuild = false;
        let ready_song_reload_dirs =
            deadsync_online::runtime::take_ready_song_reload_request_if_changed(
                self.select_music_ready_reload_generation,
            )
            .map(|(generation, dirs)| {
                self.select_music_ready_reload_generation = generation;
                dirs
            });
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
                settings,
                music_wheel,
                scoreboxes,
                leaderboard,
                unlock_downloads_available,
                ready_song_reload_dirs,
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

    #[test]
    fn frame_policy_compiles_select_music_config() {
        let config = config::Config {
            translated_titles: false,
            machine_bar_color: config::MachineBarColor::Transparent,
            global_offset_seconds: 0.025,
            null_or_die_confidence_percent: 91,
            enable_groovestats: false,
            enable_arrowcloud: false,
            auto_populate_gs_scores: true,
            auto_download_unlocks: true,
            select_music_scorebox_cycle_itg: false,
            select_music_scorebox_cycle_ex: true,
            select_music_scorebox_cycle_hard_ex: false,
            select_music_scorebox_cycle_tournaments: true,
            ..Default::default()
        };

        let policy = SelectMusicFramePolicy::from_config(&config);
        assert_eq!(policy.view, crate::select_music::policy_view(&config));
        assert!(!policy.wheel.translated_titles);
        assert!(policy.wheel.song_bg_dimmed);
        assert!(policy.wheel.section_bg_dimmed);
        assert!((policy.arrow_bounce_offset + 0.25).abs() <= f32::EPSILON);
        assert_eq!(policy.sync_confidence_percent, 91);
        assert!(!policy.enable_groovestats);
        assert!(!policy.enable_arrowcloud);
        assert!(policy.auto_populate_gs_scores);
        assert!(policy.auto_download_unlocks);
        assert!(!policy.pane_filter.itg);
        assert!(policy.pane_filter.ex);
        assert!(!policy.pane_filter.hard_ex);
        assert!(policy.pane_filter.tournaments);
    }
}
