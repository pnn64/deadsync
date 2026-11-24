/// ScoreKeeperNormal's integer counter, separate from percentage dance points.
/// The default rules enable internal scoring, W1, no combo bonus, and unit rounding.
#[derive(Clone, Copy, Debug, Default)]
pub struct StageScore {
    pub score: u32,
    pub current_max_score: u32,
    pub configured: bool,
    total: u64,
    judged: u64,
    maximum: i64,
    bonus: i64,
}

impl StageScore {
    pub fn for_song(total: usize, music_seconds: f32) -> Self {
        // Song::IsLong/IsMarathon use the actual audio length and inclusive
        // native default thresholds; marathon excludes the long-song branch.
        let stages = if music_seconds >= 300.0 {
            3
        } else if music_seconds >= 150.0 {
            2
        } else {
            1
        };
        let maximum = 100_000_000 * stages;
        Self {
            configured: true,
            total: total as u64,
            maximum,
            bonus: maximum,
            ..Self::default()
        }
    }

    pub fn reset(&mut self) {
        self.score = 0;
        self.current_max_score = 0;
        self.judged = 0;
        self.bonus = self.maximum;
    }

    pub fn tap(&mut self, grade: JudgeGrade, failed: bool) {
        if !self.configured || self.total == 0 {
            return;
        }
        let weight = match grade {
            JudgeGrade::Fantastic => 10,
            JudgeGrade::Excellent => 9,
            JudgeGrade::Great => 5,
            _ => 0,
        };
        self.judged += 1;
        let denominator = self.total * (self.total + 1) / 2;
        let points =
            |weight: u64| (weight * self.judged * (self.maximum / 10) as u64 / denominator) as u32;
        if failed {
            self.score = self.score.wrapping_add(weight as u32);
            if weight > 0 {
                self.score = (self.score.wrapping_add(2) / 5).wrapping_mul(5);
            }
        } else {
            self.score = self.score.wrapping_add(points(weight));
        }
        let possible = points(10);
        self.bonus -= i64::from(possible);
        self.current_max_score = self.current_max_score.wrapping_add(possible);
        if self.judged == self.total
            && matches!(grade, JudgeGrade::Fantastic | JudgeGrade::Excellent)
        {
            if !failed {
                self.score = self.score.wrapping_add(self.bonus as u32);
            }
            self.current_max_score = self.current_max_score.wrapping_add(self.bonus as u32);
        }
    }

    pub fn hold(&mut self, result: HoldResult, failed: bool) {
        match result {
            HoldResult::Held => self.tap(JudgeGrade::Fantastic, failed),
            HoldResult::LetGo => self.tap(JudgeGrade::Decent, failed),
            HoldResult::Missed => {}
        }
    }
}
