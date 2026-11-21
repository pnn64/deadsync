/// The scoring system used to decide whether a player's target remains attainable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayTargetScoreKind {
    #[default]
    Itg,
    Ex,
    HardEx,
}

/// Player-selected behavior when the remaining notes cannot reach the target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayTargetScoreMissPolicy {
    #[default]
    Nothing,
    DimMiniIndicator,
    Fail,
    RestartSong,
}

/// Score facts shared by the stage policy and its presentation.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameplayTargetScoreProgress {
    pub judged_any: bool,
    pub current_score_percent: f64,
    pub current_possible_ratio: f64,
}

impl GameplayTargetScoreProgress {
    #[inline]
    #[must_use]
    pub fn from_itg(player: &PlayerRuntime, possible_grade_points: i32) -> Self {
        let possible = possible_grade_points.max(1);
        let actual = player.earned_grade_points;
        let current_score_percent = if actual <= 0 {
            0.0
        } else {
            ((f64::from(actual) / f64::from(possible)) * 10000.0).floor() / 100.0
        };
        let current_possible_ratio = (f64::from(player.current_possible_points.max(0))
            / f64::from(possible))
        .clamp(0.0, 1.0);
        Self {
            judged_any: player_has_judged_score(player),
            current_score_percent,
            current_possible_ratio,
        }
    }

    /// Reuse an already sampled EX score when the HUD also needs its predictive
    /// values or judgment counts. No second runtime score snapshot is needed.
    #[inline]
    #[must_use]
    pub fn from_ex(player: &PlayerRuntime, score: &judgment::ExScoreData, hard_ex: bool) -> Self {
        let current_possible_ratio = judgment::ex_current_possible_ratio(score);
        let current_score_percent = if hard_ex {
            judgment::hard_ex_score_percent(score)
        } else {
            judgment::ex_score_percent(score)
        };
        Self {
            judged_any: player_has_judged_score(player),
            current_score_percent,
            current_possible_ratio,
        }
    }

    /// The score already earned plus every remaining point must be strictly
    /// below the target. Merely reaching the target remains attainable.
    #[inline(always)]
    #[must_use]
    pub fn missed(self, target_score_percent: f64) -> bool {
        if !self.judged_any {
            return false;
        }
        let attainable = (1.0 - self.current_possible_ratio.clamp(0.0, 1.0))
            .mul_add(100.0, self.current_score_percent.clamp(0.0, 100.0));
        attainable < target_score_percent.clamp(0.0, 100.0)
    }
}

/// Whether scoring or failure has progressed far enough to expose score feedback.
#[inline]
#[must_use]
pub fn player_has_judged_score(player: &PlayerRuntime) -> bool {
    player.scoring_counts.iter().any(|&count| count > 0)
        || player.holds_let_go_for_score > 0
        || player.rolls_let_go_for_score > 0
        || player.mines_hit_for_score > 0
        || player.is_failing
        || player.life <= 0.0
}

impl<Profile, OverlayActor, CapturedActor, StateDelta>
    GameplayRuntimeState<Profile, OverlayActor, CapturedActor, StateDelta>
where
    Profile: GameplayProfileData,
{
    #[must_use]
    pub fn target_score_progress(
        &self,
        player_idx: usize,
        kind: GameplayTargetScoreKind,
    ) -> Option<GameplayTargetScoreProgress> {
        let player = self.player(player_idx)?;
        Some(match kind {
            GameplayTargetScoreKind::Itg => GameplayTargetScoreProgress::from_itg(
                player,
                self.display_totals_for_player(player_idx)
                    .possible_grade_points,
            ),
            GameplayTargetScoreKind::Ex | GameplayTargetScoreKind::HardEx => {
                let score = self.display_scored_ex_score_data(
                    player_idx,
                    self.player_blue_window_ms(player_idx),
                );
                GameplayTargetScoreProgress::from_ex(
                    player,
                    &score,
                    kind == GameplayTargetScoreKind::HardEx,
                )
            }
        })
    }

    /// Finalize the frame after song-Lua player writes and before frame-owned
    /// input and Draw. The host maps the resulting action to its chosen screens.
    /// Existing completion/cancellation always takes priority over target policy.
    pub fn resolve_frame_action(&mut self, action: GameplayAction) -> GameplayAction {
        if action != GameplayAction::None {
            return action;
        }
        for player_idx in 0..self.setup.num_players.min(MAX_PLAYERS) {
            if self.control.missed_target_handled[player_idx] {
                continue;
            }
            let profile = &self.profiles_runtime.profiles[player_idx];
            let policy = profile.target_score_miss_policy();
            if matches!(
                policy,
                GameplayTargetScoreMissPolicy::Nothing
                    | GameplayTargetScoreMissPolicy::DimMiniIndicator
            ) {
                continue;
            }
            let target = self.mini_indicator_target_score_percent(player_idx);
            if !self
                .target_score_progress(player_idx, profile.target_score_kind())
                .is_some_and(|progress| progress.missed(target))
            {
                continue;
            }
            self.control.missed_target_handled[player_idx] = true;
            return match policy {
                GameplayTargetScoreMissPolicy::Fail => {
                    self.force_fail_player(player_idx);
                    GameplayAction::NavigateNoFade(GameplayExit::Complete)
                }
                GameplayTargetScoreMissPolicy::RestartSong => GameplayAction::Restart,
                GameplayTargetScoreMissPolicy::Nothing
                | GameplayTargetScoreMissPolicy::DimMiniIndicator => unreachable!(),
            };
        }
        GameplayAction::None
    }

    /// Preserve autoplay score invalidation at the actual Off/evaluation boundary
    /// without draining life or changing the player's failure clock.
    pub fn mark_autoplay_stage_failures(&mut self) {
        let count = self.setup.num_players.min(MAX_PLAYERS);
        for player in &mut self.players_runtime.players[..count] {
            player.stage_failed |= player.autoplay_judged;
        }
    }

    /// Complete the immediate handoff for a stage without an owned Lua outro.
    /// During a Lua outro, notes keep judging until the scene's own Off boundary.
    pub fn finalize_completed_stage(&mut self) {
        if self.progress.stage.song_completed_naturally {
            self.finalize_completed_mines();
        }
        self.mark_autoplay_stage_failures();
    }
}

#[cfg(test)]
mod target_score_progress_tests {
    use super::GameplayTargetScoreProgress;

    #[test]
    fn target_reachability_keeps_strict_threshold_and_judgment_guard() {
        let progress = GameplayTargetScoreProgress {
            judged_any: false,
            current_score_percent: 50.0,
            current_possible_ratio: 0.6,
        };
        assert!(!progress.missed(92.0));
        let progress = GameplayTargetScoreProgress {
            judged_any: true,
            ..progress
        };
        assert!(progress.missed(92.0));
        assert!(!progress.missed(90.0));
        assert!(!progress.missed(89.999));
        assert!(!progress.missed(-1.0));
        let perfect = GameplayTargetScoreProgress {
            judged_any: true,
            current_score_percent: 110.0,
            current_possible_ratio: 2.0,
        };
        assert!(!perfect.missed(110.0));
    }
}
