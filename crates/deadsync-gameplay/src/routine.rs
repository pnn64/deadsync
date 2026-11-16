/// Routine's secondary ScoreKeeperShared owns an independent PlayerStageStats.
/// Reuse the ordinary scoring state and transitions, without participant input,
/// life, feedback or combo-history updates. Events arrive in Player update order.
#[derive(Clone, Debug)]
pub struct RoutineStageStats {
    pub stats: PlayerRuntime,
    checkpoints_hit_for_score: u32,
    checkpoints_missed_for_score: u32,
}

impl Default for RoutineStageStats {
    fn default() -> Self {
        let mut stats = init_player_runtime_with_caps(PlayerBufferCaps::EMPTY);
        stats.life = 0.0;
        stats.fail_type = GameplayFailType::Off;
        Self {
            stats,
            checkpoints_hit_for_score: 0,
            checkpoints_missed_for_score: 0,
        }
    }
}

impl RoutineStageStats {
    pub fn tap_row(
        &mut self,
        judgment: &Judgment,
        note_count: u32,
        combo_per_row: bool,
        combo_multiplier: u32,
        miss_combo_multiplier: u32,
    ) {
        let mut row = row_finalization_player_state(&self.stats);
        let update = apply_row_finalization_player_state(
            &mut row,
            judgment,
            note_count,
            0,
            self.stats.stage_failed,
            combo_per_row,
            combo_multiplier,
            miss_combo_multiplier,
        );
        set_row_finalization_player_state(&mut self.stats, row);
        if update.update_grade_totals {
            self.refresh_points();
        }
    }

    pub fn mine_hit(&mut self) {
        let mut mine = mine_hit_player_state(&self.stats);
        let update = apply_mine_hit_player_state(&mut mine, false, self.stats.stage_failed);
        write_mine_hit_player_state(&mut self.stats, mine);
        if update.counted_for_score {
            self.refresh_points();
        }
    }

    pub fn hold(&mut self, note_type: NoteType, result: HoldResult) {
        let update = hold_result_stats_update(note_type, result, false, self.stats.stage_failed);
        let mut stats = hold_result_stats_state(&self.stats);
        apply_hold_result_stats_update(&mut stats, update);
        set_hold_result_stats_state(&mut self.stats, stats);
        if update.update_grade_totals {
            self.refresh_points();
        }
        // ScoreKeeperNormal::HandleHoldScore does not change combo history.
    }

    pub fn checkpoint(&mut self, held: u32, missed: u32, combo: u32, miss_combo: u32) {
        if missed == 0 {
            self.stats.checkpoints_hit = self.stats.checkpoints_hit.saturating_add(combo);
            if !self.stats.stage_failed {
                self.checkpoints_hit_for_score = self.checkpoints_hit_for_score.saturating_add(1);
            }
        } else {
            self.stats.checkpoints_missed =
                self.stats.checkpoints_missed.saturating_add(miss_combo);
            if !self.stats.stage_failed {
                self.checkpoints_missed_for_score =
                    self.checkpoints_missed_for_score.saturating_add(1);
            }
        }
        self.stats.current_possible_points = self
            .stats
            .current_possible_points
            .saturating_add(judgment::CHECKPOINT_SCORE_HIT);
        // Pump is combo-per-row. A partially hit checkpoint also clears the
        // preceding miss streak before the misses in this row are counted.
        if held > 0 {
            self.stats.miss_combo = 0;
        }
        if missed == 0 {
            self.stats.combo = self
                .stats
                .combo
                .saturating_add(held.min(1).saturating_mul(combo));
        } else {
            self.stats.combo = 0;
            self.stats.miss_combo = self.stats.miss_combo.saturating_add(miss_combo);
        }
        self.refresh_points();
    }

    pub fn score_stage(&self) -> ItgScoreStage {
        ItgScoreStage {
            checkpoints_hit: self.checkpoints_hit_for_score,
            checkpoints_missed: self.checkpoints_missed_for_score,
            ..player_score_stage(&self.stats)
        }
    }

    fn refresh_points(&mut self) {
        let score = self.score_stage();
        self.stats.earned_grade_points = judgment::itg_grade_points_with_checkpoints(
            &score.scoring_counts,
            score.holds_held_for_score,
            score.rolls_held_for_score,
            score.mines_hit_for_score,
            score.checkpoints_hit,
            score.checkpoints_missed,
        );
    }
}
