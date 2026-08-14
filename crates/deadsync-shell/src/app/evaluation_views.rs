use super::{
    App, EVALUATION_LEADERBOARD_ROWS, EvaluationPolicyView, EvaluationRuntimeView,
    EvaluationSubmissionView, MAX_PLAYERS, ScoreboxSideView, SimplyLoveGrooveStatsService,
    evaluation_context_view, scorebox_pane_filter,
};
use deadsync_config::prelude as config;
use deadsync_online::score_compat as scores;
use deadsync_profile as profile_data;
use deadsync_theme_simply_love::screens::{SimplyLoveScreen as CurrentScreen, evaluation};
use std::{sync::Arc, time::Instant};

type PlayerQueries<'a> = [Option<(profile_data::PlayerSide, &'a str)>; MAX_PLAYERS];

/// Config-generation policy for Evaluation's retained runtime views.
#[derive(Clone, Copy, Debug)]
pub(super) struct EvaluationFramePolicy {
    context: EvaluationPolicyView,
    pane_filter: deadsync_score::SelectMusicScoreboxFilter,
    enable_boogiestats: bool,
    auto_populate_gs_scores: bool,
    groovestats_service: SimplyLoveGrooveStatsService,
}

impl EvaluationFramePolicy {
    pub(super) fn from_config(config: &config::Config) -> Self {
        Self {
            context: EvaluationPolicyView {
                enable_groovestats: config.enable_groovestats,
                enable_arrowcloud: config.enable_arrowcloud,
                autosubmit_course_scores_individually: config.autosubmit_course_scores_individually,
                submit_arrowcloud_fails: config.submit_arrowcloud_fails,
                smooth_histogram: config.smooth_histogram,
                shade_scatterplot_judgments: config.shade_scatterplot_judgments,
                only_dedicated_menu_buttons: config.only_dedicated_menu_buttons,
                three_key_navigation: config.three_key_navigation,
                machine_easter_eggs: config.machine_easter_eggs,
                machine_nice_sound: config.machine_nice_sound,
                show_gameplay_timer: config.show_select_music_gameplay_timer,
                translated_titles: config.translated_titles,
                transparent_panels: matches!(
                    config.machine_evaluation_style.resolve(config.visual_style),
                    config::MachineEvaluationStyle::Transparent
                ),
                srpg10_visuals: config.visual_style.is_srpg()
                    && matches!(config.srpg_variant, config::SrpgVariant::Srpg10),
                machine_font: config.machine_font,
                zmod_rating_box_text: config.zmod_rating_box_text,
                breakdown_style: config.select_music_breakdown_style,
            },
            pane_filter: scorebox_pane_filter(config),
            enable_boogiestats: config.enable_boogiestats,
            auto_populate_gs_scores: config.auto_populate_gs_scores,
            groovestats_service: if config.enable_groovestats && config.enable_boogiestats {
                SimplyLoveGrooveStatsService::BoogieStats
            } else {
                SimplyLoveGrooveStatsService::GrooveStats
            },
        }
    }

    pub(super) const fn profile_policy(self) -> (bool, bool, bool) {
        (
            self.context.enable_groovestats,
            self.context.enable_arrowcloud,
            self.auto_populate_gs_scores,
        )
    }

    pub(super) const fn context(self) -> EvaluationPolicyView {
        self.context
    }
}

#[derive(Debug)]
struct EvaluationQueryKey {
    chart_hashes: [Option<Box<str>>; MAX_PLAYERS],
    sides: [Option<profile_data::PlayerSide>; MAX_PLAYERS],
}

impl EvaluationQueryKey {
    fn new(queries: &[Option<(profile_data::PlayerSide, &str)>; MAX_PLAYERS]) -> Self {
        Self {
            chart_hashes: queries.map(|query| query.map(|(_, hash)| hash.into())),
            sides: queries.map(|query| query.map(|(side, _)| side)),
        }
    }

    fn matches(&self, queries: &[Option<(profile_data::PlayerSide, &str)>; MAX_PLAYERS]) -> bool {
        self.chart_hashes.iter().zip(self.sides).zip(queries).all(
            |((stored_hash, stored_side), query)| match (stored_hash.as_deref(), query) {
                (None, None) => stored_side.is_none(),
                (Some(stored_hash), Some((side, hash))) => {
                    stored_side == Some(*side) && stored_hash == *hash
                }
                _ => false,
            },
        )
    }
}

#[derive(Debug)]
pub(super) struct EvaluationFavoriteKey {
    query: EvaluationQueryKey,
    generation: u64,
}

#[derive(Debug)]
pub(super) struct EvaluationSubmissionKey {
    query: EvaluationQueryKey,
    generation: scores::EvaluationSubmissionGeneration,
}

impl EvaluationSubmissionKey {
    fn new(
        queries: &PlayerQueries<'_>,
        generation: scores::EvaluationSubmissionGeneration,
    ) -> Self {
        Self {
            query: EvaluationQueryKey::new(queries),
            generation,
        }
    }

    fn matches(&self, queries: &PlayerQueries<'_>) -> bool {
        self.query.matches(queries) && self.generation == scores::evaluation_submission_generation()
    }
}

impl EvaluationFavoriteKey {
    fn new(queries: &[Option<(profile_data::PlayerSide, &str)>; MAX_PLAYERS]) -> Self {
        Self {
            query: EvaluationQueryKey::new(queries),
            generation: profile_data::runtime_favorites_generation(),
        }
    }

    fn matches(&self, queries: &[Option<(profile_data::PlayerSide, &str)>; MAX_PLAYERS]) -> bool {
        self.query.matches(queries)
            && self.generation == profile_data::runtime_favorites_generation()
    }
}

#[derive(Debug)]
pub(super) struct EvaluationScoreboxKey {
    query: EvaluationQueryKey,
    leaderboard_requests: [bool; MAX_PLAYERS],
    allow_online_panes: bool,
    profile_snapshot: Arc<profile_data::MusicProfileSnapshot>,
    pane_filter: deadsync_score::SelectMusicScoreboxFilter,
    score_generation: u64,
    leaderboard_generation: u64,
}

impl EvaluationScoreboxKey {
    fn new(
        queries: &[Option<(profile_data::PlayerSide, &str)>; MAX_PLAYERS],
        leaderboard_requests: [bool; MAX_PLAYERS],
        allow_online_panes: bool,
        profile_snapshot: &Arc<profile_data::MusicProfileSnapshot>,
        pane_filter: deadsync_score::SelectMusicScoreboxFilter,
    ) -> Self {
        Self {
            query: EvaluationQueryKey::new(queries),
            leaderboard_requests,
            allow_online_panes,
            profile_snapshot: Arc::clone(profile_snapshot),
            pane_filter,
            score_generation: deadsync_score::runtime_music_wheel_score_generation(),
            leaderboard_generation: deadsync_score::runtime_player_leaderboard_generation(),
        }
    }

    fn matches(
        &self,
        queries: &[Option<(profile_data::PlayerSide, &str)>; MAX_PLAYERS],
        leaderboard_requests: [bool; MAX_PLAYERS],
        allow_online_panes: bool,
        profile_snapshot: &Arc<profile_data::MusicProfileSnapshot>,
        pane_filter: deadsync_score::SelectMusicScoreboxFilter,
    ) -> bool {
        self.query.matches(queries)
            && self.leaderboard_requests == leaderboard_requests
            && self.allow_online_panes == allow_online_panes
            && Arc::ptr_eq(&self.profile_snapshot, profile_snapshot)
            && self.pane_filter == pane_filter
            && self.score_generation == deadsync_score::runtime_music_wheel_score_generation()
            && self.leaderboard_generation
                == deadsync_score::runtime_player_leaderboard_generation()
    }
}

fn retry_deadline(
    queries: &PlayerQueries<'_>,
    leaderboard_requests: [bool; MAX_PLAYERS],
    allow_online_panes: bool,
    profiles: &profile_data::ScoreboxRuntimeView,
) -> Option<Instant> {
    if !allow_online_panes {
        return None;
    }
    queries
        .iter()
        .zip(leaderboard_requests)
        .filter_map(|(query, requested)| {
            let (side, chart_hash) = requested.then_some(query.as_ref()?)?;
            deadsync_score::runtime_player_leaderboard_retry_deadline(
                chart_hash,
                &profiles.sides[profile_data::player_side_index(*side)].leaderboard,
            )
        })
        .min()
}

fn scorebox_views(
    queries: &PlayerQueries<'_>,
    leaderboard_requests: [bool; MAX_PLAYERS],
    allow_online_panes: bool,
    profiles: &profile_data::ScoreboxRuntimeView,
    pane_filter: deadsync_score::SelectMusicScoreboxFilter,
) -> ([ScoreboxSideView; MAX_PLAYERS], Option<Instant>) {
    let leaderboards: [Option<deadsync_score::CachedPlayerLeaderboardData>; MAX_PLAYERS] =
        std::array::from_fn(|player_idx| {
            if !allow_online_panes || !leaderboard_requests[player_idx] {
                return None;
            }
            let (side, chart_hash) = queries[player_idx]?;
            scores::get_or_fetch_player_leaderboards_for_profile(
                chart_hash,
                &profiles.sides[profile_data::player_side_index(side)].leaderboard,
                EVALUATION_LEADERBOARD_ROWS,
            )
        });
    let views = std::array::from_fn(|player_idx| {
        let Some((side, chart_hash)) = queries[player_idx] else {
            return ScoreboxSideView::default();
        };
        App::scorebox_side_view(
            &profiles.sides[profile_data::player_side_index(side)],
            Some(chart_hash.to_owned()),
            leaderboards[player_idx].clone(),
            pane_filter,
        )
    });
    (
        views,
        retry_deadline(queries, leaderboard_requests, allow_online_panes, profiles),
    )
}

fn submission_views(
    snapshots: [scores::EvaluationSubmissionSnapshot; MAX_PLAYERS],
) -> [EvaluationSubmissionView; MAX_PLAYERS] {
    snapshots.map(|snapshot| EvaluationSubmissionView {
        groovestats_status: snapshot.groovestats_status,
        arrowcloud_status: snapshot.arrowcloud_status,
        event_progress: snapshot.event_progress,
        record_banner: snapshot.record_banner,
        groovestats_next_retry_secs: snapshot.groovestats_next_retry_secs,
        arrowcloud_next_retry_secs: snapshot.arrowcloud_next_retry_secs,
        groovestats_next_retry_is_auto: snapshot.groovestats_next_retry_is_auto,
        arrowcloud_next_retry_is_auto: snapshot.arrowcloud_next_retry_is_auto,
    })
}

fn auto_retry_at(
    policy: EvaluationFramePolicy,
    refresh: &scores::EvaluationSubmissionRefresh<MAX_PLAYERS>,
) -> Option<Instant> {
    [
        policy
            .context
            .enable_groovestats
            .then_some(refresh.groovestats_next_auto_retry_at)
            .flatten(),
        policy
            .context
            .enable_arrowcloud
            .then_some(refresh.arrowcloud_next_auto_retry_at)
            .flatten(),
    ]
    .into_iter()
    .flatten()
    .min()
}

fn refresh_submissions(
    policy: EvaluationFramePolicy,
    queries: &PlayerQueries<'_>,
    now: Instant,
) -> scores::EvaluationSubmissionRefresh<MAX_PLAYERS> {
    let mut refresh = scores::evaluation_submission_refresh(queries);
    if auto_retry_at(policy, &refresh).is_some_and(|retry_at| now >= retry_at) {
        scores::tick_evaluation_auto_retries(
            policy.context.enable_groovestats,
            policy.enable_boogiestats,
            policy.context.enable_arrowcloud,
        );
        refresh = scores::evaluation_submission_refresh(queries);
    }
    refresh
}

impl App {
    pub(super) fn mark_evaluation_runtime_dirty(&mut self) {
        self.evaluation_context_rebuild = true;
        self.evaluation_lobby.force_refresh();
        self.evaluation_favorites_rebuild = true;
        self.evaluation_scoreboxes_rebuild = true;
        self.evaluation_submissions_rebuild = true;
    }

    /// Evaluation's dirty-view coordinator stays ordered in one state-machine
    /// function so borrowed chart hashes cannot outlive or diverge from the
    /// screen page whose retained fields they refresh.
    pub(super) fn sync_evaluation_runtime_view(
        &mut self,
        policy: EvaluationFramePolicy,
        now: Instant,
    ) {
        if self.state.screens.current_screen != CurrentScreen::Evaluation {
            return;
        }

        let profile_snapshot_changed =
            self.refresh_selection_profile_snapshot(policy.profile_policy());
        let lobby = self.evaluation_lobby.refresh_if_dirty(now);
        let profile_snapshot = self
            .selection_profile_snapshot
            .as_ref()
            .expect("Evaluation profile snapshot should be warmed");
        let profile_view = &profile_snapshot.scorebox;

        let context_dirty = self.evaluation_context_rebuild || profile_snapshot_changed;
        let context =
            context_dirty.then(|| evaluation_context_view(policy.context, profile_snapshot));
        self.evaluation_context_rebuild = false;

        let state = &self.state.screens.evaluation_state;
        let player_queries = std::array::from_fn(|player_idx| {
            state
                .score_info
                .get(player_idx)
                .and_then(Option::as_ref)
                .map(|score_info| (score_info.side, score_info.chart.short_hash.as_str()))
        });
        let leaderboard_requests = evaluation::leaderboard_requests(state);
        let allow_online_panes = state.allow_online_panes;

        let submission_auto_retry_due = self
            .evaluation_submission_auto_retry_at
            .is_some_and(|retry_at| now >= retry_at);
        if submission_auto_retry_due {
            scores::tick_evaluation_auto_retries(
                policy.context.enable_groovestats,
                policy.enable_boogiestats,
                policy.context.enable_arrowcloud,
            );
        }
        let submission_refresh_due = self
            .evaluation_submission_refresh_at
            .is_some_and(|refresh_at| now >= refresh_at);
        let submissions_dirty = self.evaluation_submissions_rebuild
            || submission_auto_retry_due
            || submission_refresh_due
            || self
                .evaluation_submission_key
                .as_ref()
                .is_none_or(|key| !key.matches(&player_queries));
        let submissions = submissions_dirty.then(|| {
            let refresh = refresh_submissions(policy, &player_queries, now);
            self.evaluation_submission_key = Some(EvaluationSubmissionKey::new(
                &player_queries,
                refresh.generation,
            ));
            self.evaluation_submission_refresh_at = refresh.next_refresh_at;
            self.evaluation_submission_auto_retry_at = auto_retry_at(policy, &refresh);
            submission_views(refresh.snapshots)
        });
        self.evaluation_submissions_rebuild = false;

        let favorites_dirty = self.evaluation_favorites_rebuild
            || self
                .evaluation_favorite_key
                .as_ref()
                .is_none_or(|key| !key.matches(&player_queries));
        let favorites = favorites_dirty.then(|| {
            let view = profile_data::runtime_evaluation_favorite_membership(&player_queries);
            self.evaluation_favorite_key = Some(EvaluationFavoriteKey::new(&player_queries));
            view
        });
        self.evaluation_favorites_rebuild = false;

        let scorebox_retry_due = self
            .evaluation_scorebox_retry_at
            .is_some_and(|retry_at| now >= retry_at);
        let scoreboxes_dirty = self.evaluation_scoreboxes_rebuild
            || scorebox_retry_due
            || self.evaluation_scorebox_key.as_ref().is_none_or(|key| {
                !key.matches(
                    &player_queries,
                    leaderboard_requests,
                    allow_online_panes,
                    profile_snapshot,
                    policy.pane_filter,
                )
            });
        let scoreboxes = scoreboxes_dirty.then(|| {
            let (views, retry_at) = scorebox_views(
                &player_queries,
                leaderboard_requests,
                allow_online_panes,
                profile_view,
                policy.pane_filter,
            );
            self.evaluation_scorebox_retry_at = retry_at;
            self.evaluation_scorebox_key = Some(EvaluationScoreboxKey::new(
                &player_queries,
                leaderboard_requests,
                allow_online_panes,
                profile_snapshot,
                policy.pane_filter,
            ));
            views
        });
        self.evaluation_scoreboxes_rebuild = false;

        if context.is_none()
            && lobby.is_none()
            && submissions.is_none()
            && scoreboxes.is_none()
            && favorites.is_none()
        {
            return;
        }

        evaluation::sync_runtime_view(
            &mut self.state.screens.evaluation_state,
            EvaluationRuntimeView {
                context,
                lobby,
                groovestats_service: context_dirty.then_some(policy.groovestats_service),
                submissions,
                scoreboxes,
                favorites,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_compiles_evaluation_only_inputs() {
        let config = config::Config {
            enable_groovestats: true,
            enable_boogiestats: true,
            enable_arrowcloud: false,
            auto_populate_gs_scores: true,
            smooth_histogram: true,
            select_music_scorebox_cycle_itg: false,
            ..Default::default()
        };
        let policy = EvaluationFramePolicy::from_config(&config);

        assert_eq!(policy.profile_policy(), (true, false, true));
        assert!(policy.context.smooth_histogram);
        assert!(!policy.pane_filter.itg);
        assert_eq!(
            policy.groovestats_service,
            SimplyLoveGrooveStatsService::BoogieStats
        );
    }

    #[test]
    fn query_key_matches_borrowed_hashes_and_sides() {
        let original = [
            Some((profile_data::PlayerSide::P1, "chart-a")),
            Some((profile_data::PlayerSide::P2, "chart-b")),
        ];
        let key = EvaluationQueryKey::new(&original);

        assert!(key.matches(&original));
        assert!(!key.matches(&[
            Some((profile_data::PlayerSide::P2, "chart-a")),
            Some((profile_data::PlayerSide::P2, "chart-b")),
        ]));
        assert!(!key.matches(&[
            Some((profile_data::PlayerSide::P1, "chart-a")),
            Some((profile_data::PlayerSide::P2, "chart-c")),
        ]));
    }

    #[test]
    fn auto_retry_deadline_respects_enabled_services() {
        let now = Instant::now();
        let refresh = scores::EvaluationSubmissionRefresh {
            generation: Default::default(),
            snapshots: std::array::from_fn(|_| Default::default()),
            next_refresh_at: None,
            groovestats_next_auto_retry_at: Some(now),
            arrowcloud_next_auto_retry_at: Some(now + std::time::Duration::from_secs(1)),
        };
        let policy = EvaluationFramePolicy::from_config(&config::Config {
            enable_groovestats: false,
            enable_arrowcloud: true,
            ..Default::default()
        });

        assert_eq!(
            auto_retry_at(policy, &refresh),
            refresh.arrowcloud_next_auto_retry_at
        );
    }
}
